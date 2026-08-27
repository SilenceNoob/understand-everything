use makepad_widgets::*;

use std::collections::HashMap;
use std::time::Instant;

use crate::ai::{self};
use crate::app::{rag_bm25_context, set_card_title_indicator, show_toast, RAG_RETRIEVE_TIMEOUT};
use crate::create_card_popup::CreateCardPopupWidgetRefExt;
use crate::gen::*;
use crate::mindmap::MindMapWidgetRefExt;
use crate::popup_panel::PopupPanelWidgetRefExt;
use crate::rag;
use crate::App;

/// A deferred generation request waiting for hybrid RAG retrieval to finish.
pub(crate) struct GenWait {
    path: String,
    /// The sections to generate, in order (7 items for "所有").
    sections: Vec<GenSection>,
    title: String,
    rx: std::sync::mpsc::Receiver<rag::service::RetrieveResult>,
    fallback: String,
    started: Instant,
}

/// One card's generation queue: sections are generated one request at a time
/// (a thinking model can't eat the whole output budget with reasoning), so the
/// task keeps the remaining queue plus the id of the request in flight.
pub(crate) struct GenTask {
    /// Request id of the section currently in flight (fresh per section).
    id: LiveId,
    path: String,
    title: String,
    context: String,
    /// Sections still to generate after the in-flight one.
    sections: Vec<GenSection>,
    /// Total section count, for the progress indicator.
    total: usize,
}

/// One 生成子卡片 phase-1 request (type/title/input/output judgement).
pub(crate) struct SubcardTask {
    id: LiveId,
    parent: String,
    selected: String,
}

/// One create-card request (archetype chosen by the user, topic typed).
pub(crate) struct CreateTask {
    id: LiveId,
    ctype: crate::gen::NewCardType,
    /// Whether the AI should auto-attach the card to a related card (false =
    /// always standalone, no wiring).
    auto_attach: bool,
}

/// One 重新估计学习序号 request (root card rel path).
pub(crate) struct ReorderTask {
    id: LiveId,
    root: String,
}

/// Card-generation state + logic, extracted from App. Unlike the single-slot
/// controller this replaced, generation is keyed per card: any number of cards
/// can generate at once (each card still queues its own sections serially),
/// and 生成子卡片 judgement requests run in parallel the same way.
#[derive(Default)]
pub(crate) struct GenController {
    pub(crate) ui: WidgetRef,
    /// Deferred generation requests waiting on hybrid RAG retrieval.
    pub(crate) gen_waits: Vec<GenWait>,
    /// Card-generation queues, one per card, each firing one request at a time.
    pub(crate) gen_tasks: Vec<GenTask>,
    /// Subcard judgement requests in flight (deduped per parent + selection).
    pub(crate) subcards: Vec<SubcardTask>,
    /// Create-card requests in flight (parallel, like subcards).
    pub(crate) creates: Vec<CreateTask>,
    /// 重新估计学习序号 requests in flight.
    pub(crate) reorders: Vec<ReorderTask>,
    /// SSE accumulators for the in-flight streaming requests, keyed by
    /// request id (streaming keeps bytes flowing during long generations;
    /// the reply is finalized into an HttpResponse by the app's stream
    /// handlers).
    pub(crate) streams: HashMap<LiveId, ai::StructStream>,
}
impl GenController {
    /// Whether a generation queue (running or awaiting retrieval) targets path.
    pub(crate) fn is_generating(&self, path: &str) -> bool {
        self.gen_waits.iter().any(|w| w.path == path)
            || self.gen_tasks.iter().any(|t| t.path == path)
    }

    /// Whether id belongs to a card-generation request in flight.
    pub(crate) fn is_gen_request(&self, id: LiveId) -> bool {
        self.gen_tasks.iter().any(|t| t.id == id)
    }

    /// Whether id belongs to a subcard-judgement request in flight.
    pub(crate) fn is_subcard_request(&self, id: LiveId) -> bool {
        self.subcards.iter().any(|t| t.id == id)
    }

    /// Whether id belongs to a create-card request in flight.
    pub(crate) fn is_create_request(&self, id: LiveId) -> bool {
        self.creates.iter().any(|t| t.id == id)
    }

    /// Whether id belongs to a reorder request in flight.
    pub(crate) fn is_reorder_request(&self, id: LiveId) -> bool {
        self.reorders.iter().any(|t| t.id == id)
    }

    pub(crate) fn start_generation(
        &mut self,
        cx: &mut Cx,
        path: &str,
        section: GenSection,
        rag: Option<&rag::service::RagService>,
        ai_config: &crate::ai::AIConfig,
    ) {
        // One queue per card: while it runs (or waits on retrieval) further
        // requests for the same card are dropped, other cards run in parallel.
        if self.is_generating(path) {
            return;
        }
        let title = std::path::Path::new(path)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if title.is_empty() {
            return;
        }
        // "所有" becomes a queue of per-section requests; each section is
        // generated in its own request so a thinking model can't eat the
        // whole output budget with reasoning.
        let sections: Vec<GenSection> = if section == GenSection::All {
            GenSection::all().to_vec()
        } else {
            vec![section]
        };
        let fallback = rag_bm25_context(rag, &title);
        let upgradeable = rag.is_some_and(|r| r.models().is_some_and(|m| m.embedding_ready()));
        if upgradeable {
            let rx = rag.unwrap().retrieve(&title);
            self.gen_waits.push(GenWait {
                path: path.to_string(),
                sections,
                title: title.clone(),
                rx,
                fallback,
                started: Instant::now(),
            });
            set_card_title_indicator(&self.ui, cx, path, Some("生成中…"));
        } else {
            self.send_generation(cx, path, sections, &title, &fallback, ai_config);
        }
    }

    pub(crate) fn send_generation(
        &mut self,
        cx: &mut Cx,
        path: &str,
        mut sections: Vec<GenSection>,
        title: &str,
        context: &str,
        ai_config: &crate::ai::AIConfig,
    ) {
        let total = sections.len();
        let Some(first) = sections.drain(..1).next() else {
            return;
        };
        let task = GenTask {
            id: LiveId::unique(),
            path: path.to_string(),
            title: title.to_string(),
            context: context.to_string(),
            sections,
            total,
        };
        self.gen_tasks.push(task);
        let idx = self.gen_tasks.len() - 1;
        self.send_gen_section(cx, idx, first, ai_config);
    }

    /// Fire the HTTP request for one generation section, with progress.
    fn send_gen_section(
        &mut self,
        cx: &mut Cx,
        idx: usize,
        section: GenSection,
        ai_config: &crate::ai::AIConfig,
    ) {
        let Some(task) = self.gen_tasks.get_mut(idx) else {
            return;
        };
        task.id = LiveId::unique();
        let id = task.id;
        let done = task.total.saturating_sub(task.sections.len() + 1);
        let indicator = format!("生成中… ({}/{})", done + 1, task.total);
        let path = task.path.clone();
        set_card_title_indicator(&self.ui, cx, &path, Some(&indicator));
        let body = std::fs::read_to_string(crate::util::data_dir().join(&path))
            .unwrap_or_default();
        let ctype = crate::gen::card_type(&body);
        let (system, user) = generation_messages(section, &task.title, &task.context, ctype);
        self.streams.insert(id, ai::StructStream::default());
        ai::chat_completions_structured_stream(
            cx,
            id,
            ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            ai::StructuredRequest {
                max_tokens: 358400,
                json_mode: false,
                thinking: None,
            },
        );
    }

    /// Drop the card-generation queue at idx and surface msg as a toast.
    pub(crate) fn abort_task(
        &mut self,
        cx: &mut Cx,
        idx: usize,
        msg: String,
        toast_until: &mut Option<Instant>,
    ) {
        let Some(task) = self.gen_tasks.get(idx) else {
            show_toast(&self.ui, toast_until, cx, &msg);
            return;
        };
        let path = task.path.clone();
        self.gen_tasks.remove(idx);
        set_card_title_indicator(&self.ui, cx, &path, None);
        show_toast(&self.ui, toast_until, cx, &msg);
    }

    /// A card-generation request failed at the transport level: drop its
    /// queue and toast.
    pub(crate) fn gen_request_failed(
        &mut self,
        cx: &mut Cx,
        request_id: LiveId,
        msg: String,
        toast_until: &mut Option<Instant>,
    ) {
        if let Some(idx) = self.gen_tasks.iter().position(|t| t.id == request_id) {
            self.abort_task(cx, idx, msg, toast_until);
        }
    }

    /// A subcard-judgement request failed at the transport level.
    pub(crate) fn subcard_request_failed(
        &mut self,
        cx: &mut Cx,
        request_id: LiveId,
        msg: String,
        toast_until: &mut Option<Instant>,
    ) {
        if let Some(idx) = self.subcards.iter().position(|t| t.id == request_id) {
            self.subcards.remove(idx);
            show_toast(&self.ui, toast_until, cx, &msg);
        }
    }

    pub(crate) fn handle_gen_response(
        &mut self,
        cx: &mut Cx,
        request_id: LiveId,
        response: &HttpResponse,
        rag: Option<&rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        let Some(idx) = self.gen_tasks.iter().position(|t| t.id == request_id) else {
            return;
        };
        let status = response.status_code;
        if status != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            self.abort_task(cx, idx, format!("生成失败 ({})：{}", status, detail), toast_until);
            return;
        }
        let content = ai::response_content(response).unwrap_or_default();
        let sections = parse_generation_output(&content);
        if sections.is_empty() {
            let debug = ai::response_debug_preview(response);
            self.abort_task(cx, idx, format!("生成返回为空或格式不正确（{debug}）"), toast_until);
            return;
        }
        let path = self.gen_tasks[idx].path.clone();
        let full_path = crate::util::data_dir().join(&path);
        let body = std::fs::read_to_string(&full_path).unwrap_or_default();
        let new_body = upsert_sections(&body, &sections);
        if let Err(e) = std::fs::write(&full_path, &new_body) {
            self.abort_task(cx, idx, format!("保存卡片失败：{}", e), toast_until);
            return;
        }
        // Update the card body if the card is still in the current map.
        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
        mind_map.update_card_body(cx, &full_path, new_body);
        // Continue the queue, or finish when it is exhausted.
        if !self.gen_tasks[idx].sections.is_empty() {
            let next = self.gen_tasks[idx].sections.remove(0);
            self.send_gen_section(cx, idx, next, ai_config);
        } else {
            self.gen_tasks.remove(idx);
            set_card_title_indicator(&self.ui, cx, &path, None);
            if let Some(rag) = rag {
                rag.set_map(&self.ui.mind_map(cx, ids!(mindmap)).current_map_file().unwrap_or_default());
            }
        }
    }

    /// 划选生成子卡片, phase 1: the model judges type/title/input/output
    /// (a small JSON that cannot be truncated). The body is filled in phase 2
    /// by the existing per-section generation pipeline.
    pub(crate) fn start_subcard_gen(
        &mut self,
        cx: &mut Cx,
        parent: &str,
        selected: &str,
        rag: Option<&rag::service::RagService>,
        ai_config: &crate::ai::AIConfig,
    ) {
        // One judge request per (parent, selection): identical re-clicks are
        // dropped, other cards/selections run in parallel.
        if self
            .subcards
            .iter()
            .any(|t| t.parent == parent && t.selected == selected)
        {
            return;
        }
        let base = crate::util::data_dir();
        let parent_body = std::fs::read_to_string(base.join(parent)).unwrap_or_default();
        let parent_title = std::path::Path::new(parent)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ctx = rag_bm25_context(rag, selected);
        let (system, user) =
            crate::gen::subcard_judge_messages(&parent_title, &parent_body, selected, &ctx);
        let id = LiveId::unique();
        self.subcards.push(SubcardTask {
            id,
            parent: parent.to_string(),
            selected: selected.to_string(),
        });
        self.streams.insert(id, ai::StructStream::default());
        ai::chat_completions_structured_stream(
            cx,
            id,
            ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            ai::StructuredRequest {
                max_tokens: 358400,
                json_mode: false,
                thinking: None,
            },
        );
    }

    pub(crate) fn handle_subcard_response(
        &mut self,
        cx: &mut Cx,
        request_id: LiveId,
        response: &HttpResponse,
        rag: Option<&rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        let Some(idx) = self.subcards.iter().position(|t| t.id == request_id) else {
            return;
        };
        if response.status_code != 200 {
            self.subcards.remove(idx);
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            show_toast(&self.ui, toast_until, cx, &format!("生成子卡片失败 ({}): {}", response.status_code, detail));
            return;
        }
        let content = ai::response_content(response).unwrap_or_default();
        let judge = match crate::gen::parse_subcard_judge(&content) {
            Ok(v) => v,
            Err(e) => {
                self.subcards.remove(idx);
                let preview = ai::response_debug_preview(response);
                show_toast(&self.ui, toast_until, cx, &format!("生成子卡片失败：{e}（{preview}）"));
                return;
            }
        };
        let parent_rel = self.subcards[idx].parent.clone();
        self.subcards.remove(idx);
        let base = crate::util::data_dir();
        let parent_path = base.join(&parent_rel);
        let dir = parent_path
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "cards".to_string());
        let safe = crate::file_panel::normalize_name(&judge.title, Some(".md"))
            .unwrap_or_else(|| "未命名.md".to_string());
        let safe = safe.strip_suffix(".md").unwrap_or(&safe).to_string();
        // Seed body: the 知识类型/输入输出/输入输出空间 blocks are assembled
        // here so every subcard carries them; the rest is generated later.
        let seed = match judge.ctype {
            crate::gen::CardType::Knowledge => {
                let mut s = format!(
                    "#c 知识类型 联结模型\n\n#c 输入输出\n输入：{}\n输出：{}\n",
                    judge.input.trim(),
                    judge.output.trim()
                );
                if !judge.input_space.trim().is_empty() {
                    s.push_str(&format!(
                        "\n#c 输入空间\n{}\n",
                        judge.input_space.trim()
                    ));
                }
                if !judge.output_space.trim().is_empty() {
                    s.push_str(&format!(
                        "\n#c 输出空间\n{}\n",
                        judge.output_space.trim()
                    ));
                }
                s
            }
            crate::gen::CardType::Concept => {
                let mut s = "#c 知识类型 概念\n".to_string();
                if !judge.input.trim().is_empty() || !judge.output.trim().is_empty() {
                    s.push_str(&format!(
                        "\n#c 输入输出\n输入：{}\n输出：{}\n",
                        judge.input.trim(),
                        judge.output.trim()
                    ));
                }
                s
            }
        };
        let mut rel = None;
        for n in 0.. {
            let fname = if n == 0 {
                format!("{safe}.md")
            } else {
                format!("{safe}-{n}.md")
            };
            let p = base.join(&dir).join(&fname);
            if !p.exists() {
                std::fs::create_dir_all(p.parent().unwrap_or(&base)).ok();
                if std::fs::write(&p, seed.clone()).is_ok() {
                    rel = Some(format!("{dir}/{fname}"));
                }
                break;
            }
        }
        match rel {
            Some(rel) => {
                let mind_map = self.ui.mind_map(cx, ids!(mindmap));
                mind_map.add_child_card(cx, &parent_rel, &rel);
                let kind = match judge.ctype {
                    crate::gen::CardType::Concept => "概念",
                    crate::gen::CardType::Knowledge => "知识",
                };
                show_toast(
                    &self.ui,
                    toast_until,
                    cx,
                    &format!("已生成{kind}子卡片「{}」，已挂到父卡片下，开始逐板块生成学习材料…", judge.title),
                );
                // Phase 2: the per-section pipeline fills the card body
                // (each section in its own request — immune to truncation).
                // The new card has no queue yet, so it starts immediately even
                // while other cards are still generating.
                self.start_generation(cx, &rel, crate::gen::GenSection::All, rag, ai_config);
            }
            None => show_toast(&self.ui, toast_until, cx, "生成子卡片失败：无法创建卡片文件。"),
        }
    }

    /// 创建卡片 (create-card dialog): fire one judge request that names the
    /// card, summarizes its input/output, and (when `auto_attach`) picks the
    /// most related existing card as parent. `ctype` is user-chosen and
    /// forced. The popup stays open with a busy status until the response
    /// lands.
    pub(crate) fn start_card_creation(
        &mut self,
        cx: &mut Cx,
        ctype: crate::gen::NewCardType,
        topic: &str,
        auto_attach: bool,
        rag: Option<&rag::service::RagService>,
        ai_config: &crate::ai::AIConfig,
    ) {
        // Parent selection is only relevant when auto-attach is on; without
        // the card list the model has nothing to attach to (parent = null).
        let map_context = if auto_attach {
            let mind_map = self.ui.mind_map(cx, ids!(mindmap));
            let infos = mind_map.card_infos();
            infos
                .iter()
                .map(|(t, c)| {
                    let kind = match c {
                        crate::gen::CardType::Concept => "判别模型",
                        crate::gen::CardType::Knowledge => "联结模型",
                    };
                    format!("「{t}」（{kind}）")
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            String::new()
        };
        let ctx = rag_bm25_context(rag, topic);
        let (system, user) = crate::gen::create_card_messages(topic, ctype, &map_context, &ctx);
        let id = LiveId::unique();
        self.creates.push(CreateTask {
            id,
            ctype,
            auto_attach,
        });
        self.streams.insert(id, ai::StructStream::default());
        ai::chat_completions_structured_stream(
            cx,
            id,
            ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            ai::StructuredRequest {
                max_tokens: 358400,
                json_mode: false,
                thinking: None,
            },
        );
    }

    /// A create-card request failed at the transport level: unlock the popup
    /// and toast.
    pub(crate) fn create_request_failed(
        &mut self,
        cx: &mut Cx,
        request_id: LiveId,
        msg: String,
        toast_until: &mut Option<Instant>,
    ) {
        if let Some(idx) = self.creates.iter().position(|t| t.id == request_id) {
            self.creates.remove(idx);
            crate::create_card_popup::create_content(&self.ui)
                .as_create_card_popup()
                .set_status(cx, &msg, false);
            show_toast(&self.ui, toast_until, cx, &msg);
        }
    }

    /// A create-card judge response: write the seed body file (基础内容, same
    /// shape as route cards), attach it under the suggested parent (or as an
    /// independent root card when unrelated), close the popup, and toast.
    pub(crate) fn handle_create_response(
        &mut self,
        cx: &mut Cx,
        request_id: LiveId,
        response: &HttpResponse,
        rag: Option<&rag::service::RagService>,
        toast_until: &mut Option<Instant>,
    ) {
        let Some(idx) = self.creates.iter().position(|t| t.id == request_id) else {
            return;
        };
        let ctype = self.creates[idx].ctype;
        let auto_attach = self.creates[idx].auto_attach;
        self.creates.remove(idx);
        let content_widget = crate::create_card_popup::create_content(&self.ui);
        let popup = content_widget.as_create_card_popup();
        let fail = |ui: &WidgetRef, popup: &crate::create_card_popup::CreateCardPopupRef, cx: &mut Cx, msg: &str, toast_until: &mut Option<Instant>| {
            popup.set_status(cx, msg, false);
            show_toast(ui, toast_until, cx, msg);
        };
        if response.status_code != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            fail(&self.ui, &popup, cx, &format!("生成失败 ({})：{}", response.status_code, detail), toast_until);
            return;
        }
        let content = ai::response_content(response).unwrap_or_default();
        let plan = match crate::gen::parse_create_card(&content) {
            Ok(p) => p,
            Err(e) => {
                let preview = ai::response_debug_preview(response);
                fail(&self.ui, &popup, cx, &format!("生成失败：{e}（{preview}）"), toast_until);
                return;
            }
        };
        // Seed body: the 知识类型/输入输出(+空间) blocks, like route cards —
        // 基础内容 only; per-section generation is the user's follow-up.
        let seed = match ctype {
            crate::gen::NewCardType::Knowledge => {
                let mut s = format!(
                    "#c 知识类型 联结模型\n\n#c 输入输出\n输入：{}\n输出：{}\n",
                    plan.input.trim(),
                    plan.output.trim()
                );
                if !plan.input_space.trim().is_empty() {
                    s.push_str(&format!("\n#c 输入空间\n{}\n", plan.input_space.trim()));
                }
                if !plan.output_space.trim().is_empty() {
                    s.push_str(&format!("\n#c 输出空间\n{}\n", plan.output_space.trim()));
                }
                s
            }
            crate::gen::NewCardType::Concept => {
                let mut s = "#c 知识类型 概念\n".to_string();
                if !plan.input.trim().is_empty() || !plan.output.trim().is_empty() {
                    s.push_str(&format!(
                        "\n#c 输入输出\n输入：{}\n输出：{}\n",
                        plan.input.trim(),
                        plan.output.trim()
                    ));
                }
                s
            }
        };
        // File under cards/<map stem>/ (grouped like route cards), unique-ified.
        let base = crate::util::data_dir();
        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
        let map_file = mind_map.current_map_file().unwrap_or_default();
        let stem = map_file
            .strip_prefix("maps/")
            .unwrap_or(&map_file)
            .strip_suffix(".json")
            .unwrap_or(&map_file);
        let safe = crate::file_panel::normalize_name(&plan.title, Some(".md"))
            .unwrap_or_else(|| "未命名.md".to_string());
        let safe = safe.strip_suffix(".md").unwrap_or(&safe).to_string();
        let mut rel = None;
        for n in 0.. {
            let fname = if n == 0 {
                format!("{safe}.md")
            } else {
                format!("{safe}-{n}.md")
            };
            let p = base.join("cards").join(stem).join(&fname);
            if !p.exists() {
                std::fs::create_dir_all(p.parent().unwrap_or(&base)).ok();
                if std::fs::write(&p, seed).is_ok() {
                    rel = Some(format!("cards/{stem}/{fname}"));
                }
                break;
            }
        }
        let Some(rel) = rel else {
            fail(&self.ui, &popup, cx, "生成失败：无法创建卡片文件。", toast_until);
            return;
        };
        // Attach: auto-attach on → a parent-title hit attaches the card as a
        // child, otherwise it lands as an independent root card at the
        // right-click spot (parent-less 联结模型 roots then offer
        // 生成学习路线 in their context menu). Auto-attach off → always
        // standalone, never wired.
        let parent_rel = if auto_attach {
            plan.parent
                .as_deref()
                .and_then(|t| mind_map.rel_path_by_title(t))
                .filter(|p| !p.is_empty())
        } else {
            None
        };
        let kind = ctype.short();
        match parent_rel {
            Some(parent_rel) => {
                mind_map.add_child_card(cx, &parent_rel, &rel);
                show_toast(
                    &self.ui,
                    toast_until,
                    cx,
                    &format!("已生成{kind}卡「{}」，已挂到「{}」下。", plan.title, plan.parent.as_deref().unwrap_or("")),
                );
            }
            None => {
                mind_map.add_card_at(cx, &rel);
                let msg = if auto_attach {
                    format!("已生成{kind}卡「{}」，与现有内容无关，已作为独立根卡片。", plan.title)
                } else {
                    format!("已生成{kind}卡「{}」，未自动连线，已作为独立根卡片。", plan.title)
                };
                show_toast(&self.ui, toast_until, cx, &msg);
            }
        }
        crate::create_card_popup::create_popup(&self.ui).as_popup_panel().hide(cx);
        if let Some(rag) = rag {
            rag.set_map(&mind_map.current_map_file().unwrap_or_default());
        }
    }

    /// 重新估计学习序号: ask the model for a title→order mapping of the
    /// subtree under the root card at `root_rel`. The root card's title flips
    /// to 估计序号中… while the request is in flight.
    pub(crate) fn start_reorder(
        &mut self,
        cx: &mut Cx,
        root_rel: &str,
        rag: Option<&rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
        let entries = mind_map.subtree_entries(root_rel);
        if entries.is_empty() {
            show_toast(&self.ui, toast_until, cx, "该根卡片下没有子卡片，无需估计序号。");
            return;
        }
        let root_title = std::path::Path::new(root_rel)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ctx = rag_bm25_context(rag, &root_title);
        let (system, user) = crate::gen::reorder_messages(&root_title, &entries, &ctx);
        set_card_title_indicator(&self.ui, cx, root_rel, Some("估计序号中…"));
        let id = LiveId::unique();
        self.reorders.push(ReorderTask {
            id,
            root: root_rel.to_string(),
        });
        self.streams.insert(id, ai::StructStream::default());
        ai::chat_completions_structured_stream(
            cx,
            id,
            ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            ai::StructuredRequest {
                max_tokens: 358400,
                json_mode: false,
                thinking: None,
            },
        );
    }

    /// A reorder request failed at the transport level: restore the root
    /// title and toast.
    pub(crate) fn reorder_request_failed(
        &mut self,
        cx: &mut Cx,
        request_id: LiveId,
        msg: String,
        toast_until: &mut Option<Instant>,
    ) {
        if let Some(idx) = self.reorders.iter().position(|t| t.id == request_id) {
            let root = self.reorders[idx].root.clone();
            self.reorders.remove(idx);
            set_card_title_indicator(&self.ui, cx, &root, None);
            show_toast(&self.ui, toast_until, cx, &msg);
        }
    }

    /// A reorder response: apply the title→order mapping to the root's
    /// subtree, restore the root title, and toast.
    pub(crate) fn handle_reorder_response(
        &mut self,
        cx: &mut Cx,
        request_id: LiveId,
        response: &HttpResponse,
        rag: Option<&rag::service::RagService>,
        toast_until: &mut Option<Instant>,
    ) {
        let Some(idx) = self.reorders.iter().position(|t| t.id == request_id) else {
            return;
        };
        let root = self.reorders[idx].root.clone();
        self.reorders.remove(idx);
        if response.status_code != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            set_card_title_indicator(&self.ui, cx, &root, None);
            show_toast(
                &self.ui,
                toast_until,
                cx,
                &format!("序号估计失败 ({})：{}", response.status_code, detail),
            );
            return;
        }
        let content = ai::response_content(response).unwrap_or_default();
        match crate::gen::parse_reorder(&content) {
            Ok(orders) => {
                let n = orders.len();
                self.ui.mind_map(cx, ids!(mindmap)).apply_orders(cx, &root, &orders);
                set_card_title_indicator(&self.ui, cx, &root, None);
                show_toast(&self.ui, toast_until, cx, &format!("已重新估计学习序号：{n} 张卡片。"));
                if let Some(rag) = rag {
                    rag.set_map(
                        &self
                            .ui
                            .mind_map(cx, ids!(mindmap))
                            .current_map_file()
                            .unwrap_or_default(),
                    );
                }
            }
            Err(e) => {
                let preview = ai::response_debug_preview(response);
                set_card_title_indicator(&self.ui, cx, &root, None);
                show_toast(&self.ui, toast_until, cx, &format!("序号估计失败：{e}（{preview}）"));
            }
        }
    }
}
impl GenController {
    /// Poll deferred card-generation retrievals and promote the ready ones to
    /// real requests (each wait is polled independently, so several cards can
    /// be promoted in the same tick).
    pub(crate) fn poll_gen_wait(
        &mut self,
        cx: &mut Cx,
        ai_config: &crate::ai::AIConfig,
    ) {
        let now = Instant::now();
        let mut ready: Vec<(String, Vec<GenSection>, String, String)> = Vec::new();
        self.gen_waits.retain(|wait| {
            let hits = match wait.rx.try_recv() {
                Ok(r) if r.query == wait.title => Some(r.hits),
                Ok(_) => None,
                Err(std::sync::mpsc::TryRecvError::Empty) => {
                    if now.duration_since(wait.started) > RAG_RETRIEVE_TIMEOUT {
                        Some(Vec::new())
                    } else {
                        None
                    }
                }
                Err(_) => Some(Vec::new()),
            };
            match hits {
                None => true,
                Some(hits) => {
                    let ctx = if hits.is_empty() {
                        wait.fallback.clone()
                    } else {
                        rag::service::format_context(&hits)
                    };
                    ready.push((wait.path.clone(), wait.sections.clone(), wait.title.clone(), ctx));
                    false
                }
            }
        });
        for (path, sections, title, ctx) in ready {
            self.send_generation(cx, &path, sections, &title, &ctx, ai_config);
        }
    }
}

impl App {
    /// Forwarding shims (state lives in GenController).
    pub(crate) fn start_generation(&mut self, cx: &mut Cx, path: &str, section: GenSection) {
        if self.gen.is_generating(path) {
            show_toast(&self.ui, &mut self.toast_until, cx, "该卡片正在生成中，请稍候");
            return;
        }
        self.gen.start_generation(cx, path, section, self.rag.as_ref(), &self.ai_config);
    }

    pub(crate) fn handle_gen_response(&mut self, cx: &mut Cx, request_id: LiveId, response: &HttpResponse) {
        self.gen
            .handle_gen_response(cx, request_id, response, self.rag.as_ref(), &mut self.toast_until, &self.ai_config);
    }

    pub(crate) fn start_subcard_gen(&mut self, cx: &mut Cx, parent: &str, selected: &str) {
        self.gen.start_subcard_gen(cx, parent, selected, self.rag.as_ref(), &self.ai_config);
    }

    pub(crate) fn handle_subcard_response(&mut self, cx: &mut Cx, request_id: LiveId, response: &HttpResponse) {
        self.gen
            .handle_subcard_response(cx, request_id, response, self.rag.as_ref(), &mut self.toast_until, &self.ai_config);
    }

    pub(crate) fn handle_create_response(&mut self, cx: &mut Cx, request_id: LiveId, response: &HttpResponse) {
        self.gen
            .handle_create_response(cx, request_id, response, self.rag.as_ref(), &mut self.toast_until);
    }

    pub(crate) fn handle_reorder_response(&mut self, cx: &mut Cx, request_id: LiveId, response: &HttpResponse) {
        self.gen
            .handle_reorder_response(cx, request_id, response, self.rag.as_ref(), &mut self.toast_until);
    }

    pub(crate) fn handle_gen_rag_tick(&mut self, cx: &mut Cx) {
        self.gen.poll_gen_wait(cx, &self.ai_config);
    }
}

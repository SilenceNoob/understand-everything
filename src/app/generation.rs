use makepad_widgets::*;

use std::time::Instant;

use crate::ai::{self};
use crate::app::{rag_bm25_context, set_card_title_indicator, show_toast, RAG_RETRIEVE_TIMEOUT};
use crate::gen::*;
use crate::mindmap::MindMapWidgetRefExt;
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
        ai::chat_completions(
            cx,
            id,
            ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
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
        ai::chat_completions(
            cx,
            id,
            ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
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

    pub(crate) fn handle_gen_rag_tick(&mut self, cx: &mut Cx) {
        self.gen.poll_gen_wait(cx, &self.ai_config);
    }
}

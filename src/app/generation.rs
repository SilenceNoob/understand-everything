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

/// Card-generation state + logic, extracted from App.
#[derive(Default)]
pub(crate) struct GenController {
    pub(crate) ui: WidgetRef,
    pub(crate) gen_wait: Option<GenWait>,
    pub(crate) gen_id: LiveId,
    pub(crate) gen_path: String,
    pub(crate) gen_sections: Vec<GenSection>,
    pub(crate) gen_total: usize,
    pub(crate) gen_context: String,
    pub(crate) gen_title: String,
    pub(crate) subcard_id: LiveId,
    pub(crate) subcard_parent: String,
}
impl GenController {
    pub(crate) fn start_generation(
        &mut self,
        cx: &mut Cx,
        path: &str,
        section: GenSection,
        rag: Option<&rag::service::RagService>,
        ai_config: &crate::ai::AIConfig,
    ) {
        if self.gen_wait.is_some() || self.gen_id != LiveId::empty() {
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
            self.gen_wait = Some(GenWait {
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
        self.gen_path = path.to_string();
        self.gen_title = title.to_string();
        self.gen_context = context.to_string();
        self.gen_total = sections.len();
        let Some(first) = sections.drain(..1).next() else {
            return;
        };
        self.gen_sections = sections;
        self.send_gen_section(cx, first, ai_config);
    }

    /// Fire the HTTP request for one generation section, with progress.
    pub(crate) fn send_gen_section(
        &mut self,
        cx: &mut Cx,
        section: GenSection,
        ai_config: &crate::ai::AIConfig,
    ) {
        self.gen_id = LiveId::unique();
        let done = self.gen_total.saturating_sub(self.gen_sections.len() + 1);
        let indicator = format!("生成中… ({}/{})", done + 1, self.gen_total);
        set_card_title_indicator(&self.ui, cx, &self.gen_path, Some(&indicator));
        let body = std::fs::read_to_string(crate::util::data_dir().join(&self.gen_path))
            .unwrap_or_default();
        let ctype = crate::gen::card_type(&body);
        let (system, user) = generation_messages(section, &self.gen_title, &self.gen_context, ctype);
        ai::chat_completions(
            cx,
            self.gen_id,
            &ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
        );
    }

    /// Abort the current generation queue and surface `msg` as a toast.
    pub(crate) fn abort_generation(
        &mut self,
        cx: &mut Cx,
        msg: String,
        toast_until: &mut Option<Instant>,
    ) {
        self.gen_sections.clear();
        set_card_title_indicator(&self.ui, cx, &self.gen_path, None);
        show_toast(&self.ui, toast_until, cx, &msg);
    }

    pub(crate) fn handle_gen_response(
        &mut self,
        cx: &mut Cx,
        response: &HttpResponse,
        rag: Option<&rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        self.gen_id = LiveId::empty();
        let status = response.status_code;
        let full_path = crate::util::data_dir().join(&self.gen_path);
        if status != 200 {
            let detail = response
                .get_string_body()
                .and_then(|b| ai::body_error_message(&b))
                .unwrap_or_default();
            self.abort_generation(cx, format!("生成失败 ({})：{}", status, detail), toast_until);
            return;
        }
        let content = ai::response_content(response).unwrap_or_default();
        let sections = parse_generation_output(&content);
        if sections.is_empty() {
            let debug = ai::response_debug_preview(response);
            self.abort_generation(cx, format!("生成返回为空或格式不正确（{debug}）"), toast_until);
            return;
        }
        let body = std::fs::read_to_string(&full_path).unwrap_or_default();
        let new_body = upsert_sections(&body, &sections);
        if let Err(e) = std::fs::write(&full_path, &new_body) {
            self.abort_generation(cx, format!("保存卡片失败：{}", e), toast_until);
            return;
        }
        // Update the card body if the card is still in the current map.
        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
        mind_map.update_card_body(cx, &full_path, new_body);
        // Continue the queue, or finish when it is exhausted.
        if !self.gen_sections.is_empty() {
            let next = self.gen_sections.remove(0);
            self.send_gen_section(cx, next, ai_config);
        } else {
            set_card_title_indicator(&self.ui, cx, &self.gen_path, None);
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
        if self.subcard_id != LiveId::empty() {
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
        self.subcard_parent = parent.to_string();
        self.subcard_id = LiveId::unique();
        ai::chat_completions(
            cx,
            self.subcard_id,
            &ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
        );
    }

    pub(crate) fn handle_subcard_response(
        &mut self,
        cx: &mut Cx,
        response: &HttpResponse,
        rag: Option<&rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        self.subcard_id = LiveId::empty();
        if response.status_code != 200 {
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
                let preview = ai::response_debug_preview(response);
                show_toast(&self.ui, toast_until, cx, &format!("生成子卡片失败：{e}（{preview}）"));
                return;
            }
        };
        let base = crate::util::data_dir();
        let parent_rel = self.subcard_parent.clone();
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
                self.start_generation(cx, &rel, crate::gen::GenSection::All, rag, ai_config);
            }
            None => show_toast(&self.ui, toast_until, cx, "生成子卡片失败：无法创建卡片文件。"),
        }
    }
}
impl GenController {
    /// Poll deferred card-generation retrieval and promote it to a real request.
    pub(crate) fn poll_gen_wait(
        &mut self,
        cx: &mut Cx,
        ai_config: &crate::ai::AIConfig,
    ) {
        let Some(wait) = &mut self.gen_wait else { return };
        let now = Instant::now();
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
        let Some(hits) = hits else { return };
        let ctx = if hits.is_empty() {
            wait.fallback.clone()
        } else {
            rag::service::format_context(&hits)
        };
        let path = wait.path.clone();
        let sections = std::mem::take(&mut wait.sections);
        let title = wait.title.clone();
        self.gen_wait = None;
        self.send_generation(cx, &path, sections, &title, &ctx, ai_config);
    }
}

impl App {
    /// Forwarding shims (state lives in GenController).
    pub(crate) fn start_generation(&mut self, cx: &mut Cx, path: &str, section: GenSection) {
        self.gen.start_generation(cx, path, section, self.rag.as_ref(), &self.ai_config);
    }

    pub(crate) fn abort_generation(&mut self, cx: &mut Cx, msg: String) {
        self.gen.abort_generation(cx, msg, &mut self.toast_until);
    }

    pub(crate) fn handle_gen_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        self.gen
            .handle_gen_response(cx, response, self.rag.as_ref(), &mut self.toast_until, &self.ai_config);
    }

    pub(crate) fn start_subcard_gen(&mut self, cx: &mut Cx, parent: &str, selected: &str) {
        self.gen.start_subcard_gen(cx, parent, selected, self.rag.as_ref(), &self.ai_config);
    }

    pub(crate) fn handle_subcard_response(&mut self, cx: &mut Cx, response: &HttpResponse) {
        self.gen
            .handle_subcard_response(cx, response, self.rag.as_ref(), &mut self.toast_until, &self.ai_config);
    }

    pub(crate) fn handle_gen_rag_tick(&mut self, cx: &mut Cx) {
        self.gen.poll_gen_wait(cx, &self.ai_config);
    }
}

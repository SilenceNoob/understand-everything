use makepad_widgets::*;

use std::sync::atomic::Ordering;
use std::time::Instant;

use crate::ai::{self};
use crate::app::{popup_child, popup_widget, rag_bm25_context, set_card_title_indicator, show_toast, RAG_RETRIEVE_TIMEOUT};
use crate::mindmap::{self, MindMapWidgetRefExt};
use crate::popup_panel::PopupPanelWidgetRefExt;
use crate::rag;
use crate::App;
use crate::app::diag::{DiagController, StartupPhase};

/// A learning-route plan request deferred until the hybrid RAG retrieval
/// answers (or times out to the BM25 fallback).
pub(crate) struct RouteWait {
    pub(crate) goal: String,
    /// Diagnostic transcript passed through to the route planner.
    pub(crate) diag: String,
    pub(crate) rx: std::sync::mpsc::Receiver<rag::service::RetrieveResult>,
    pub(crate) fallback: String,
    pub(crate) started: Instant,
}

/// Route-plan retrieval fired when the diagnostic interview starts (the
/// query is just the goal, known minutes before the interview ends); adopted
/// by `start_route_plan` when the goal matches, dropped otherwise.
pub(crate) struct RoutePrefetch {
    pub(crate) goal: String,
    pub(crate) rx: std::sync::mpsc::Receiver<rag::service::RetrieveResult>,
    pub(crate) started: Instant,
}

/// Route-planning state + logic, extracted from App.
#[derive(Default)]
pub(crate) struct RouteController {
    pub(crate) ui: WidgetRef,
    pub(crate) route_buf: String,
    pub(crate) route_parser: crate::ai::SseParser,
    pub(crate) route_toast_len: usize,
    pub(crate) route_retried: bool,
    pub(crate) route_context: String,
    pub(crate) route_wait: Option<RouteWait>,
    pub(crate) route_prefetch: Option<RoutePrefetch>,
    pub(crate) route_id: LiveId,
    pub(crate) route_goal: String,
    pub(crate) route_root: String,
    pub(crate) route_diag: String,
}

/// Seed body for a route card: the archetype marker (drives 生成/测试 prompt
/// selection), the card's own input/output, and why it's in the route.
fn route_card_seed_body(rc: &crate::gen::RouteCard) -> String {
    let ctype = if rc.card_type == "knowledge" {
        "联结模型"
    } else {
        "概念"
    };
    let mut body = format!("#c 知识类型 {ctype}\n");
    if !rc.input.is_empty() || !rc.output.is_empty() {
        body.push_str(&format!("\n#c 输入输出\n输入：{}\n输出：{}\n", rc.input, rc.output));
    }
    if !rc.reason.is_empty() {
        body.push_str(&format!("\n#c 为何学\n{}\n", rc.reason));
    }
    body
}
impl RouteController {
    /// Kick off learning-route planning for `goal` under the root card at
    /// `root` (rel path; empty = the startup flow, which falls back to the
    /// map's only card or creates the primary root on an empty map). Runs the
    /// diagnostic interview first (see `begin_diag`). Re-planning is refused
    /// when the target root already has children.
    pub(crate) fn start_route_plan(
        &mut self,
        cx: &mut Cx,
        goal: &str,
        root: &str,
        diagnostics: &str,
        rag: Option<&rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        if self.route_id != LiveId::empty() || self.route_wait.is_some() {
            return;
        }
        if ai_config.api_key.trim().is_empty() {
            show_toast(&self.ui, toast_until, cx, "请先在 Setting 中配置 API Key 再生成学习路线");
            return;
        }
        let base = crate::util::data_dir();
        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
        let Some(map_file) = mind_map.current_map_file() else {
            return;
        };
        // Root card: the menu card when planning from a root card (multi-route
        // maps pass its rel path through the diagnostic), else the map's only
        // card, else a fresh primary root on an empty map.
        let root_rel = if !root.trim().is_empty() {
            root.trim().to_string()
        } else {
            let existing = mind_map.card_rel_paths();
            if existing.len() == 1 {
                existing[0].clone()
            } else if existing.is_empty() {
                let Some(rel) = self.create_route_card_file(&map_file, goal) else {
                    return;
                };
                // The goal itself is the target knowledge (联结模型): it gets
                // the knowledge-card prompts for 生成/测试.
                let body = format!("#c 知识类型 联结模型\n\n#d 学习目标\n{goal}\n");
                if std::fs::write(base.join(&rel), body).is_err() {
                    return;
                }
                if std::fs::write(base.join(&map_file), mindmap::route_map_json(goal, &rel, &[])).is_err() {
                    return;
                }
                mind_map.reload_map(cx);
                rel
            } else {
                show_toast(
                    &self.ui,
                    toast_until,
                    cx,
                    "当前地图卡片较多，请右键目标根卡片生成学习路线。",
                );
                return;
            }
        };
        // Re-planning is refused when the target root already has a route.
        if mind_map.card_child_count(&root_rel).unwrap_or(0) > 0 {
            show_toast(&self.ui, toast_until, cx, "该学习目标已有学习路线，暂不支持重新规划。");
            return;
        }
        self.route_goal = goal.to_string();
        self.route_root = root_rel.clone();
        self.route_diag = diagnostics.to_string();
        self.route_retried = false;
        // Hide the 生成学习路线 menu row while the plan is in flight.
        self.ui.mind_map(cx, ids!(mindmap)).set_route_planning(cx, true);
        // Visible progress: the root card title flips to 规划中….
        set_card_title_indicator(&self.ui, cx, &root_rel, Some("规划中…"));
        let fallback = rag_bm25_context(rag, goal);
        let upgradeable = rag.is_some_and(|r| {
            r.models().is_some_and(|m| m.embedding_ready()) && r.has_chunks_for(&map_file)
        });
        // A prefetch fired at diagnostic start (goal matches) skips the
        // retrieval wait entirely; a stale/goalless prefetch is dropped.
        let prefetch = match self.route_prefetch.take() {
            Some(p) if p.goal == goal => Some(p),
            _ => None,
        };
        if let Some(p) = prefetch {
            self.route_wait = Some(RouteWait {
                goal: goal.to_string(),
                diag: diagnostics.to_string(),
                rx: p.rx,
                fallback,
                started: p.started,
            });
            show_toast(&self.ui, toast_until, cx, "正在检索参考资料…");
        } else if upgradeable {
            let rx = rag.unwrap().retrieve(goal);
            self.route_wait = Some(RouteWait {
                goal: goal.to_string(),
                diag: diagnostics.to_string(),
                rx,
                fallback,
                started: Instant::now(),
            });
            show_toast(&self.ui, toast_until, cx, "正在检索参考资料…");
        } else {
            self.send_route_request(cx, goal, diagnostics, &fallback, toast_until, ai_config);
        }
    }

    /// Create a route card file `cards/<map stem>/<prefix>-<title>.md`,
    /// unique-ified with a numeric suffix when the name is taken.
    pub(crate) fn create_route_card_file(&self, map_file: &str, title: &str) -> Option<String> {
        let stem = map_file
            .strip_prefix("maps/")
            .unwrap_or(map_file)
            .strip_suffix(".json")
            .unwrap_or(map_file);
        let safe = crate::file_panel::normalize_name(title, Some(".md"))
            .unwrap_or_else(|| "未命名.md".to_string());
        let safe = safe.strip_suffix(".md").unwrap_or(&safe).to_string();
        let base = crate::util::data_dir();
        for n in 0.. {
            let fname = if n == 0 {
                format!("{safe}.md")
            } else {
                format!("{safe}-{n}.md")
            };
            let p = base.join("cards").join(stem).join(&fname);
            if !p.exists() {
                std::fs::create_dir_all(p.parent()?).ok()?;
                std::fs::write(&p, "").ok()?;
                return Some(format!("cards/{stem}/{fname}"));
            }
        }
        None
    }

    /// Fire the route-plan request (streaming; parsed when the stream ends).
    pub(crate) fn send_route_request(
        &mut self,
        cx: &mut Cx,
        goal: &str,
        diagnostics: &str,
        context: &str,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        self.route_id = LiveId::unique();
        show_toast(&self.ui, toast_until, cx, "正在生成学习路线…");
        self.route_buf.clear();
        self.route_parser = ai::SseParser::new();
        self.route_context = context.to_string();
        let (system, user) = crate::gen::route_plan_messages(goal, context, diagnostics);
        ai::chat_stream_request_max(
            cx,
            self.route_id,
            ai_config,
            &[("system".to_string(), system), ("user".to_string(), user)],
            358400,
        );
    }

    /// Abort route planning and surface `msg` as a toast.
    pub(crate) fn abort_route(&mut self, cx: &mut Cx, msg: String, toast_until: &mut Option<Instant>) {
        self.route_wait = None;
        self.route_id = LiveId::empty();
        self.ui.mind_map(cx, ids!(mindmap)).set_route_planning(cx, false);
        if !self.route_root.is_empty() {
            set_card_title_indicator(&self.ui, cx, &self.route_root, None);
        }
        show_toast(&self.ui, toast_until, cx, &msg);
    }

    /// Materialize a parsed route plan: write the card files, rebuild the map
    /// tree under the root goal card, and reload the canvas.
    pub(crate) fn apply_route_plan(
        &mut self,
        cx: &mut Cx,
        content: String,
        rag: Option<&rag::service::RagService>,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        let mut plan = match crate::gen::parse_route_plan(&content) {
            Ok(p) => p,
            Err(e) => {
                if !self.route_retried {
                    // Same goal/context/diag, fresh draw: intermittent
                    // malformed-JSON output usually parses on the retry.
                    self.route_retried = true;
                    let goal = self.route_goal.clone();
                    let diag = self.route_diag.clone();
                    let ctx = self.route_context.clone();
                    self.send_route_request(cx, &goal, &diag, &ctx, toast_until, ai_config);
                    return;
                }
                let preview: String = content.chars().take(200).collect();
                self.abort_route(cx, format!("路线解析失败：{e}\n原始输出预览：{preview}"), toast_until);
                return;
            }
        };
        // The planner sometimes lists the goal itself as a card; the root
        // already exists, so drop those and re-attach their children to the
        // root (else a "-1" duplicate root gets created).
        crate::gen::drop_goal_duplicates(&mut plan, &self.route_goal);
        let base = crate::util::data_dir();
        let mind_map = self.ui.mind_map(cx, ids!(mindmap));
        let Some(map_file) = mind_map.current_map_file() else {
            self.abort_route(cx, "路线生成失败：当前地图不存在".to_string(), toast_until);
            return;
        };
        if self.route_root.is_empty() {
            self.abort_route(cx, "路线生成失败：缺少根卡片".to_string(), toast_until);
            return;
        }
        // Card files, numbered by learning order (leaves first). Reuse an
        // existing library card when its title matches — never overwrite a
        // non-empty body (other maps may reference the file).
        let existing_cards = crate::file_panel::all_card_files(&base);
        // The root card file is part of the library scan above; pre-marking it
        // as used keeps a planned card whose title equals the goal from
        // reusing the root's file (which would spawn an identical duplicate
        // node connected to the root).
        let mut used: std::collections::HashSet<String> =
            [self.route_root.clone()].into_iter().collect();
        let mut cards: Vec<(String, String, String, Option<String>, Option<u32>)> = Vec::new();
        for (n, &ci) in crate::gen::learning_order(&plan.cards).iter().enumerate() {
            let rc = &plan.cards[ci];
            let rel = match crate::gen::match_card_path(&existing_cards, &rc.title)
                .filter(|p| !used.contains(p))
            {
                Some(p) => {
                    used.insert(p.clone());
                    let full = base.join(&p);
                    let body = std::fs::read_to_string(&full).unwrap_or_default();
                    if body.trim().is_empty() {
                        std::fs::write(&full, route_card_seed_body(rc)).ok();
                    }
                    p
                }
                None => {
                    let Some(rel) = self.create_route_card_file(&map_file, &rc.title) else {
                        self.abort_route(cx, format!("创建卡片失败：{}", rc.title), toast_until);
                        return;
                    };
                    if std::fs::write(base.join(&rel), route_card_seed_body(rc)).is_err() {
                        self.abort_route(cx, format!("写入卡片失败：{}", rc.title), toast_until);
                        return;
                    }
                    rel
                }
            };
            // Learning-order number (leaves first); the root goal card stays
            // unnumbered.
            cards.push((
                rc.id.clone(),
                rc.title.clone(),
                rel,
                rc.parent.clone(),
                Some(n as u32 + 1),
            ));
        }
        // Goal analysis lands on the root card.
        let root_path = base.join(&self.route_root);
        let mut body = std::fs::read_to_string(&root_path).unwrap_or_default();
        if !plan.goal_input.is_empty() {
            body = crate::gen::upsert_section(&body, "#c 输入空间", &plan.goal_input);
        }
        if !plan.goal_output.is_empty() {
            body = crate::gen::upsert_section(&body, "#c 输出空间", &plan.goal_output);
        }
        // 用户情况 = the planner's assessment of the user's knowledge state;
        // the raw interview transcript (questions + answers) is prompt input
        // only and must not land on the card.
        if !plan.user_assessment.is_empty() {
            body = crate::gen::upsert_section(&body, "#c 用户情况", &plan.user_assessment);
        }
        std::fs::write(&root_path, body).ok();
        // Merge in-memory: the new nodes attach under the route root without
        // rewriting the map file, so other roots/groups/pan-zoom survive.
        mind_map.attach_route(cx, &self.route_root, &cards);
        set_card_title_indicator(&self.ui, cx, &self.route_root, None);
        if let Some(rag) = rag {
            rag.set_map(&map_file);
        }
        let summary = format!(
            "学习路线已生成：{} 张卡片（概念卡 {} 张，知识卡 {} 张）。",
            plan.cards.len(),
            plan.cards.iter().filter(|c| c.card_type == "concept").count(),
            plan.cards.iter().filter(|c| c.card_type == "knowledge").count(),
        );
        self.ui.mind_map(cx, ids!(mindmap)).set_route_planning(cx, false);
        show_toast(&self.ui, toast_until, cx, &summary);
    }

    /// Poll deferred route-plan retrieval and promote it to a real request.
    pub(crate) fn handle_route_rag_tick(
        &mut self,
        cx: &mut Cx,
        toast_until: &mut Option<Instant>,
        ai_config: &crate::ai::AIConfig,
    ) {
        let Some(wait) = &mut self.route_wait else { return };
        let now = Instant::now();
        let hits = match wait.rx.try_recv() {
            Ok(r) if r.query == wait.goal => Some(r.hits),
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
        let goal = wait.goal.clone();
        let diag = wait.diag.clone();
        self.route_wait = None;
        self.send_route_request(cx, &goal, &diag, &ctx, toast_until, ai_config);
    }

    /// Throttled progress toast while the route stream is in flight: refresh
    /// the text only when the accumulated buffer grew, so the 5s auto-close
    /// timer keeps resetting and the toast stays visible until the plan ends
    /// (finish/abort replace it via their own show_toast calls).
    pub(crate) fn handle_route_progress_toast(
        &mut self,
        cx: &mut Cx,
        toast_until: &mut Option<Instant>,
    ) {
        if self.route_id == LiveId::empty() {
            self.route_toast_len = 0;
            return;
        }
        let len = self.route_buf.len();
        if len == self.route_toast_len {
            return;
        }
        self.route_toast_len = len;
        show_toast(&self.ui, toast_until, cx, &format!("正在生成学习路线…（已生成 {} 字）", len));
    }

    pub(crate) fn close_startup(&mut self, cx: &mut Cx, diag: &mut DiagController) {
        // The input can't emit KeyFocusLost once the popup is gone.
        crate::float_panel::CHAT_INPUT_ACTIVE.store(false, Ordering::Relaxed);
        diag.reset_diag();
        popup_widget(&self.ui, live_id!(startup_popup)).as_popup_panel().hide(cx);
    }

    /// Open the startup welcome page, clearing any leftover input text and
    /// resetting the diagnostic session back to the goal-input phase.
    pub(crate) fn show_startup(&mut self, cx: &mut Cx, diag: &mut DiagController) {
        diag.reset_diag();
        diag.set_startup_phase(cx, StartupPhase::Goal);
        popup_child(
            &self.ui,
            live_id!(startup_popup),
            &[
                live_id!(content),
                live_id!(panel),
                live_id!(goal_view),
                live_id!(input_row),
                live_id!(start_input),
            ],
        )
        .as_text_input()
        .set_text(cx, "");
        popup_widget(&self.ui, live_id!(startup_popup)).as_popup_panel().show(cx);
    }
}

impl App {
    /// Forwarding shims (state lives in RouteController).
    pub(crate) fn abort_route(&mut self, cx: &mut Cx, msg: String) {
        self.route.abort_route(cx, msg, &mut self.toast_until);
    }

    pub(crate) fn apply_route_plan(&mut self, cx: &mut Cx, content: String) {
        self.route
            .apply_route_plan(cx, content, self.rag.as_ref(), &mut self.toast_until, &self.ai_config);
    }

    pub(crate) fn handle_route_rag_tick(&mut self, cx: &mut Cx) {
        self.route.handle_route_rag_tick(cx, &mut self.toast_until, &self.ai_config);
    }

    pub(crate) fn handle_route_progress_toast(&mut self, cx: &mut Cx) {
        self.route.handle_route_progress_toast(cx, &mut self.toast_until);
    }

    pub(crate) fn close_startup(&mut self, cx: &mut Cx) {
        self.route.close_startup(cx, &mut self.diag);
    }

    pub(crate) fn show_startup(&mut self, cx: &mut Cx) {
        self.route.show_startup(cx, &mut self.diag);
    }
}

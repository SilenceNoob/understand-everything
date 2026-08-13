use makepad_widgets::*;

use crate::mindmap::MindMapWidgetRefExt;

use std::time::{Duration, Instant};

pub mod chat;
pub mod diag;
pub mod files;
pub mod generation;
pub mod http;
pub mod mindmap_actions;
pub mod quiz;
pub mod route;
pub mod ui;

/// Child of `parent` by name, via live children (graph-independent).
pub(crate) fn child_by_name(parent: &WidgetRef, id: LiveId) -> WidgetRef {
    let mut found = WidgetRef::empty();
    parent.try_children(&mut |name, child| {
        if name == id {
            found = child;
        }
    });
    found
}

/// Upper bound for the retrieval pre-roll; on timeout the BM25 fallback
/// context fires the request. Measured hybrid ≈ 11s (5 × ~2s rerank + embed),
/// plus first-call reranker lazy load (~4s) and CPU contention from a
/// concurrent index build, so the budget carries headroom.
pub(crate) const RAG_RETRIEVE_TIMEOUT: Duration = Duration::from_secs(20);
/// How often the app re-syncs the index from disk (catches card edits and
/// refs changes; fingerprint-diffed so unchanged snapshots are free).
pub(crate) const RAG_RESYNC_SECS: u64 = 5;
/// Token slack for the async hybrid context: 5 excerpts × 300 chars ≈ 1050
/// CJK-weighted tokens, not yet known at the context gauge.
pub(crate) const RAG_CONTEXT_SLACK: usize = 1100;

/// How long a feature toast stays visible before auto-closing.
pub(crate) const TOAST_DURATION: Duration = Duration::from_secs(5);

/// BM25 context for `query` from the shared RAG service, when present.
pub(crate) fn rag_bm25_context(
    rag: Option<&crate::rag::service::RagService>,
    query: &str,
) -> String {
    let Some(rag) = rag else {
        return String::new();
    };
    let hits = rag.bm25_search(query, 5);
    crate::rag::service::format_context(&hits)
}

/// The popup widget (setting/about), walked through live children from the
/// root — the widget-tree graph does not index widgets inside custom-widget
/// content, while live navigation always reflects the real tree.
pub(crate) fn popup_widget(ui: &WidgetRef, id: LiveId) -> WidgetRef {
    let main_window = child_by_name(ui, live_id!(main_window));
    let body = child_by_name(&main_window, live_id!(body));
    child_by_name(&body, id)
}

/// Descendant of a popup by live-child path (content → panel → …).
pub(crate) fn popup_child(ui: &WidgetRef, popup_id: LiveId, path: &[LiveId]) -> WidgetRef {
    let mut cur = popup_widget(ui, popup_id);
    for &seg in path {
        cur = child_by_name(&cur, seg);
        if cur.is_empty() {
            break;
        }
    }
    cur
}

/// The corner toast widget (feature-task notifications), via the body's
/// live children like the popups.
pub(crate) fn toast_widget(ui: &WidgetRef) -> WidgetRef {
    let main_window = child_by_name(ui, live_id!(main_window));
    let body = child_by_name(&main_window, live_id!(body));
    child_by_name(&body, live_id!(toast))
}

/// Show a 5-second corner toast; replaces any toast still showing.
pub(crate) fn show_toast(
    ui: &WidgetRef,
    toast_until: &mut Option<Instant>,
    cx: &mut Cx,
    msg: &str,
) {
    let toast = toast_widget(ui);
    let content = child_by_name(&toast, live_id!(content));
    let label = child_by_name(&content, live_id!(label));
    if !label.is_empty() {
        label.set_text(cx, msg);
    }
    toast.as_popup_notification().open(cx);
    *toast_until = Some(Instant::now() + TOAST_DURATION);
}

/// Card title indicator via the mindmap widget (shared by route/gen flows).
pub(crate) fn set_card_title_indicator(
    ui: &WidgetRef,
    cx: &mut Cx,
    path: &str,
    indicator: Option<&str>,
) {
    let full_path = crate::util::data_dir().join(path);
    let mind_map = ui.mind_map(cx, ids!(mindmap));
    mind_map.set_card_title_indicator(cx, &full_path, indicator);
}

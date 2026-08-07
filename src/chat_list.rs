use makepad_widgets::*;

use std::time::Instant;

use crate::markdown_media::MarkdownMediaWidgetRefExt;

/// Action data for the per-bubble copy button; distinct from the thinking
/// fold button's plain `usize` so the two clicks never collide.
#[derive(Debug, Clone, PartialEq)]
struct CopyMsg(usize);

script_mod! {
    use mod.prelude.widgets_internal.*
    use mod.widgets.*

    mod.widgets.ChatList = #(ChatList::register_widget(vm))
}

/// One chat message: the assistant's thinking chain (streamed as
/// `reasoning_content`) is kept alongside the final answer and rendered
/// in a foldable block.
#[derive(Clone, Default)]
pub struct ChatMsg {
    pub role: String,
    pub content: String,
    pub thinking: String,
    pub thinking_open: bool,
}

/// Virtualized chat message list: a PortalList whose rows come from the
/// `msgs` slice set via `set_msgs`. Handles unbounded history lengths
/// without instantiating a widget per message.
#[derive(Script, Widget)]
pub struct ChatList {
    #[source]
    source: ScriptObjectRef,
    #[deref]
    view: View,

    #[rust]
    msgs: Vec<ChatMsg>,
    /// Scroll to the newest message on the next draw pass.
    #[rust]
    scroll_end: bool,
    /// Row index and time of the last copy click, for the "已复制" flash.
    #[rust]
    copied_flash: Option<(usize, Instant)>,
}

impl ScriptHook for ChatList {}

impl Widget for ChatList {
    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        let mut list_ref = None;
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.borrow_mut::<PortalList>() {
                let total = self.msgs.len();
                list.set_item_range(cx, 0, total);
                while let Some(idx) = list.next_visible_item(cx) {
                    if idx >= total {
                        continue;
                    }
                    let msg = &self.msgs[idx];
                    let template = if msg.role == "user" {
                        live_id!(UserLine)
                    } else {
                        live_id!(AssistantLine)
                    };
                    let item = list.item(cx, idx, template);
                    if item.is_empty() {
                        continue;
                    }
                    // Copy button (both line templates): swap the gray copy
                    // icon for a green check for two seconds after the click.
                    let flashing = matches!(self.copied_flash, Some((i, t)) if i == idx && t.elapsed().as_secs_f32() < 2.0);
                    item.button(cx, ids!(copy_btn)).set_visible(cx, !flashing);
                    item.button(cx, ids!(copy_on_btn)).set_visible(cx, flashing);
                    item.button(cx, ids!(copy_btn)).set_action_data(CopyMsg(idx));
                    item.button(cx, ids!(copy_on_btn)).set_action_data(CopyMsg(idx));
                    if msg.role == "user" {
                        item.markdown_media(cx, ids!(line_md)).set_text(cx, &msg.content);
                    } else {
                        item.markdown_media(cx, ids!(line_md)).set_text(cx, &msg.content);
                        let has_thinking = !msg.thinking.is_empty();
                        let thinking_row = item.view(cx, ids!(thinking_row));
                        thinking_row.set_visible(cx, has_thinking);
                        if has_thinking {
                            let btn = item.button(cx, ids!(thinking_btn));
                            btn.set_text(
                                cx,
                                if msg.thinking_open { "思考过程 ↓" } else { "思考过程 →" },
                            );
                            btn.set_action_data(idx);
                            // Fold via the container: MarkdownMedia has no
                            // visible field, so set_visible on it is a no-op.
                            item.view(cx, ids!(thinking_body))
                                .set_visible(cx, msg.thinking_open);
                            item.markdown_media(cx, ids!(thinking_md))
                                .set_text(cx, &msg.thinking);
                        }
                    }
                    item.draw_all(cx, &mut Scope::empty());
                }
            }
            list_ref = Some(item);
        }
        // Only after the PortalList's draw cycle has fully ended (begin/end
        // pair consumed): jumping first_id mid-draw makes end() unwrap on an
        // item that wasn't drawn this pass.
        if self.scroll_end {
            self.scroll_end = false;
            if let Some(item) = list_ref {
                item.as_portal_list().scroll_to_end(cx);
            }
        }
        DrawStep::done()
    }

    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        // Fold/unfold: the thinking header button carries its row index as
        // action data.
        if let Event::Actions(actions) = event {
            for action in actions {
                let Some(wa) = action.downcast_ref::<WidgetAction>() else {
                    continue;
                };
                // Only respond to the completed click (FingerUp); Pressed
                // would flip the fold twice per press.
                let clicked = matches!(
                    wa.action.downcast_ref::<ButtonAction>(),
                    Some(ButtonAction::Clicked(_))
                );
                if !clicked {
                    continue;
                }
                if let Some(data) = wa.data.as_ref() {
                    if let Some(CopyMsg(idx)) = data.downcast_ref::<CopyMsg>() {
                        if let Some(msg) = self.msgs.get(*idx) {
                            cx.copy_to_clipboard(&msg.content);
                            self.copied_flash = Some((*idx, Instant::now()));
                            self.redraw(cx);
                        }
                    } else if let Some(idx) = data.downcast_ref::<usize>().copied() {
                        if let Some(msg) = self.msgs.get_mut(idx) {
                            msg.thinking_open = !msg.thinking_open;
                            self.redraw(cx);
                        }
                    }
                }
            }
        }
    }
}

impl ChatList {
    /// Replace the message list (oldest first) and scroll to the bottom.
    /// Existing rows keep their fold state; new rows default to unfolded.
    pub fn set_msgs(&mut self, cx: &mut Cx, msgs: &[ChatMsg]) {
        for (idx, msg) in msgs.iter().enumerate() {
            let open = self
                .msgs
                .get(idx)
                .map(|old| old.thinking_open)
                .unwrap_or(true);
            let mut m = msg.clone();
            m.thinking_open = open;
            if idx < self.msgs.len() {
                self.msgs[idx] = m;
            } else {
                self.msgs.push(m);
            }
        }
        self.msgs.truncate(msgs.len());
        self.scroll_end = true;
        self.redraw(cx);
    }
}

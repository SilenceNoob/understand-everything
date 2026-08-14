use makepad_widgets::*;

use crate::create_card_popup::{CreateCardPopupRef, CreateCardPopupWidgetRefExt};
use crate::popup_panel::PopupPanelWidgetRefExt;
use crate::App;

impl App {
    /// The create-card dialog's content widget.
    pub(crate) fn create_card_popup(&self) -> CreateCardPopupRef {
        self.popup_child(live_id!(create_card_popup), &[live_id!(content)])
            .as_create_card_popup()
    }

    /// Open the create-card dialog with `topic` prefilled (empty = clean
    /// form). Closes every other popup first (one modal at a time).
    pub(crate) fn open_create_card_popup(&mut self, cx: &mut Cx, topic: &str) {
        for id in [
            live_id!(setting_popup),
            live_id!(about_popup),
            live_id!(startup_popup),
            live_id!(quiz_popup),
            live_id!(picker_popup),
            live_id!(confirm_popup),
        ] {
            self.popup_widget(id).as_popup_panel().hide(cx);
        }
        self.popup_widget(live_id!(create_card_popup)).as_popup_panel().show(cx);
        self.create_card_popup().open(cx, topic);
    }

    /// Create-card dialog actions: close, or submit → AI card creation
    /// (GenController; the popup shows a busy status until the reply lands).
    pub(crate) fn handle_create_card_actions(&mut self, cx: &mut Cx, actions: &Actions) {
        let popup = self.create_card_popup();
        if popup.close_clicked(actions) {
            self.popup_widget(live_id!(create_card_popup)).as_popup_panel().hide(cx);
            return;
        }
        if popup.busy() {
            return;
        }
        if let Some((ctype, topic, auto_attach)) = popup.submitted(actions) {
            let topic = topic.trim().to_string();
            if topic.is_empty() {
                popup.set_status(cx, "请输入卡片主题", false);
                return;
            }
            popup.set_status(cx, "正在生成…", true);
            self.gen
                .start_card_creation(cx, ctype, &topic, auto_attach, self.rag.as_ref(), &self.ai_config);
        }
    }
}

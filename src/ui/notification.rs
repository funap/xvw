use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Icon, Placement};
use gpui_kit::prelude::*;
use gpui_kit::{ClipboardItem, SharedString};

use std::time::Duration;

use crate::ui::icon::IconName;

/// Default auto-dismiss timeout for notifications (3 seconds).
pub const NOTIFICATION_TIMEOUT: Duration = Duration::from_secs(3);

/// Trait to extend `Notification` with copy actions.
pub trait NotificationExt {
    /// Adds a copy action button to the notification that copies `copy_text` to the clipboard.
    fn with_copy_action(self, copy_text: impl Into<SharedString>) -> Self;
}

impl NotificationExt for Notification {
    fn with_copy_action(self, copy_text: impl Into<SharedString>) -> Self {
        let copy_text: SharedString = copy_text.into();
        let copied = std::rc::Rc::new(std::cell::Cell::new(false));
        self.action(move |_note, _window, cx| {
            let text = copy_text.clone();
            let copied_cell = copied.clone();
            let is_copied = copied_cell.get();
            let note_weak = cx.entity().downgrade();

            let button = if is_copied {
                Button::new("copy-message")
                    .ghost()
                    .icon(Icon::new(IconName::Check).text_color(cx.theme().success))
                    .label("Copied!")
            } else {
                Button::new("copy-message")
                    .ghost()
                    .icon(IconName::Copy)
                    .tooltip("Copy message")
                    .tooltip_placement(Placement::Top)
            };

            let note_entity = cx.entity();
            button.on_click(move |_, _window, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(text.to_string()));
                copied_cell.set(true);
                let reset_copied = copied_cell.clone();
                let note_for_timer = note_weak.clone();

                note_entity.update(cx, |_, cx| cx.notify());

                cx.spawn(async move |cx| {
                    cx.background_executor().timer(Duration::from_millis(1500)).await;
                    cx.update(|cx| {
                        if let Some(note) = note_for_timer.upgrade() {
                            note.update(cx, |_, cx| {
                                reset_copied.set(false);
                                cx.notify();
                            });
                        }
                    });
                })
                .detach();
            })
        })
    }
}

/// Helper to construct a notification with autohide disabled and a copy action button.
fn persistent_notification_with_copy(create: impl FnOnce(SharedString) -> Notification, message: impl Into<SharedString>) -> Notification {
    let message: SharedString = message.into();
    create(message.clone()).autohide(false).with_copy_action(message)
}

/// Creates an error notification with a copy button that copies the message to the clipboard,
/// and disables autohide so it remains visible until closed.
pub fn error(message: impl Into<SharedString>) -> Notification {
    persistent_notification_with_copy(Notification::error, message)
}

/// Creates an informational notification with a copy button that copies the message to the clipboard,
/// and disables autohide so it remains visible until closed.
#[allow(dead_code)]
pub fn info(message: impl Into<SharedString>) -> Notification {
    persistent_notification_with_copy(Notification::info, message)
}

/// Creates a success notification with a copy button that copies the message to the clipboard,
/// and disables autohide so it remains visible until closed.
#[allow(dead_code)]
pub fn success(message: impl Into<SharedString>) -> Notification {
    persistent_notification_with_copy(Notification::success, message)
}

/// Creates a warning notification with a copy button that copies the message to the clipboard,
/// and disables autohide so it remains visible until closed.
#[allow(dead_code)]
pub fn warning(message: impl Into<SharedString>) -> Notification {
    persistent_notification_with_copy(Notification::warning, message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_notification_creation() {
        assert_eq!(NOTIFICATION_TIMEOUT, Duration::from_secs(3));
        let _note = error("An unexpected error occurred");
    }

    #[test]
    fn test_standard_notifications() {
        let _info = info("Information message");
        let _success = success("Saved successfully");
        let _warning = warning("Warning message");
    }

    #[test]
    fn test_with_copy_action() {
        let note = Notification::error("Test error").with_copy_action("Test copy text");
        let _note = note.autohide(false);

        let note_info = Notification::info("Test info").with_copy_action("Test info copy");
        let _note_info = note_info.autohide(false);
    }
}

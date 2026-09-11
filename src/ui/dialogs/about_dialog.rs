use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Icon, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{App, ClipboardItem, FontWeight, MouseButton, Window, div, px};

use crate::ui::icon::IconName;

pub const APP_NAME: &str = "xvw";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");
pub const APP_REPOSITORY: &str = "https://github.com/funap/xvw";
pub const APP_LICENSE: &str = "MIT";

/// Returns current platform and architecture target (e.g., "macos-aarch64").
pub fn app_target() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Returns formatted full version information string suitable for clipboard copying.
pub fn version_info_text() -> String {
    format!(
        "{name} v{version} ({target})\n{desc}\nLicense: {license}\n{repo}",
        name = APP_NAME,
        version = APP_VERSION,
        target = app_target(),
        desc = APP_DESCRIPTION,
        license = APP_LICENSE,
        repo = APP_REPOSITORY
    )
}

/// Displays the About / Version dialog.
pub fn open_about_dialog(window: &mut Window, cx: &mut App) {
    if window.has_active_dialog(cx) {
        return;
    }

    let target = app_target();

    window.open_dialog(cx, move |dialog, _window, cx| {
        let theme = cx.theme();
        let copy_text = version_info_text();
        let target_str = target.clone();

        dialog
            .w(px(440.0))
            .title(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::Binary).size(px(20.0)).text_color(theme.primary))
                    .child(
                        div()
                            .text_base()
                            .font_weight(FontWeight::BOLD)
                            .text_color(theme.foreground)
                            .child(format!("About {APP_NAME}")),
                    ),
            )
            .content(move |content, _window, cx| {
                let theme = cx.theme();
                content.child(
                    v_flex()
                        .gap_4()
                        .py_2()
                        .child(
                            h_flex()
                                .items_center()
                                .gap_3()
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .size(px(48.0))
                                        .rounded_lg()
                                        .bg(theme.accent.opacity(0.12))
                                        .border_1()
                                        .border_color(theme.accent.opacity(0.3))
                                        .child(Icon::new(IconName::Binary).size(px(28.0)).text_color(theme.primary)),
                                )
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap_1()
                                        .child(
                                            h_flex()
                                                .items_baseline()
                                                .gap_2()
                                                .child(div().text_xl().font_weight(FontWeight::BOLD).text_color(theme.foreground).child(APP_NAME))
                                                .child(
                                                    div()
                                                        .px_1p5()
                                                        .py_0p5()
                                                        .rounded_md()
                                                        .bg(theme.muted.opacity(0.2))
                                                        .border_1()
                                                        .border_color(theme.border)
                                                        .text_xs()
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .text_color(theme.muted_foreground)
                                                        .child(format!("v{APP_VERSION}")),
                                                ),
                                        )
                                        .child(div().text_xs().text_color(theme.muted_foreground).child(APP_DESCRIPTION)),
                                ),
                        )
                        .child(
                            v_flex()
                                .rounded_md()
                                .border_1()
                                .border_color(theme.border)
                                .bg(theme.tab_bar)
                                .p_3()
                                .gap_2()
                                .child(render_info_row("Version", APP_VERSION, theme))
                                .child(render_info_row("Platform", &target_str, theme))
                                .child(render_info_row("License", APP_LICENSE, theme))
                                .child(
                                    h_flex()
                                        .justify_between()
                                        .items_center()
                                        .text_xs()
                                        .child(div().text_color(theme.muted_foreground).child("Repository"))
                                        .child(
                                            h_flex()
                                                .items_center()
                                                .gap_1()
                                                .cursor_pointer()
                                                .text_color(theme.link)
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.open_url(APP_REPOSITORY);
                                                })
                                                .child(div().child("funap/xvw"))
                                                .child(Icon::new(IconName::ExternalLink).size(px(12.0))),
                                        ),
                                ),
                        ),
                )
            })
            .footer(
                DialogFooter::new()
                    .child(
                        Button::new("copy-about-info")
                            .ghost()
                            .icon(IconName::Copy)
                            .label("Copy Info")
                            .tooltip("Copy version details to clipboard")
                            .on_click(move |_, window, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copy_text.clone()));
                                window.push_notification(Notification::info("Version info copied to clipboard"), cx);
                            }),
                    )
                    .child(Button::new("about-ok").primary().label("OK").on_click(|_, window, cx| {
                        window.close_dialog(cx);
                    })),
            )
    });
}

fn render_info_row(label: &'static str, value: &str, theme: &gpui_kit::component::Theme) -> impl IntoElement {
    h_flex()
        .justify_between()
        .items_center()
        .text_xs()
        .child(div().text_color(theme.muted_foreground).child(label))
        .child(div().font_weight(FontWeight::MEDIUM).text_color(theme.foreground).child(value.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_info_text_contains_expected_fields() {
        let text = version_info_text();
        assert!(text.contains(APP_NAME));
        assert!(text.contains(APP_VERSION));
        assert!(text.contains(APP_DESCRIPTION));
        assert!(text.contains(APP_REPOSITORY));
        assert!(text.contains(APP_LICENSE));
        assert!(text.contains(&app_target()));
    }

    #[test]
    fn test_app_target_format() {
        let target = app_target();
        assert!(target.contains('-'));
        assert_eq!(target, format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH));
    }
}

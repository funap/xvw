use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme as _, Icon, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::{App, FontWeight, MouseButton, ObjectFit, Window, div, img, px};

use crate::ui::icon::IconName;

pub const APP_NAME: &str = "xvw";
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const APP_DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");
pub const APP_REPOSITORY: &str = "https://github.com/funap/xvw";
pub const APP_LICENSE: &str = "MIT";
pub const APP_ICON_PATH: &str = "icons/xvw.svg";

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

/// Displays the About / Version modal dialog.
pub fn open_about_modal(window: &mut Window, cx: &mut App) {
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
                    .child(Icon::new(IconName::Info).size(px(20.0)).text_color(theme.primary))
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
                            h_flex().items_center().gap_3().child(render_app_icon(theme)).child(
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
                                                    .bg(theme.secondary)
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
                                                .id("about-repo-link")
                                                .items_center()
                                                .gap_1()
                                                .cursor_pointer()
                                                .text_color(theme.link)
                                                .hover(|s| s.underline())
                                                .tooltip(|_window, cx| {
                                                    cx.new(|_| gpui_kit::component::tooltip::Tooltip::new(format!("Open {APP_REPOSITORY} in browser")))
                                                        .into()
                                                })
                                                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                                    cx.open_url(APP_REPOSITORY);
                                                })
                                                .child("funap/xvw")
                                                .child(Icon::new(IconName::ExternalLink).size(px(12.0))),
                                        ),
                                ),
                        ),
                )
            })
            .footer(
                DialogFooter::new()
                    .child(
                        Clipboard::new("copy-about-info")
                            .value(copy_text.clone())
                            .tooltip("Copy version details to clipboard")
                            .on_copied(move |_, window, cx| {
                                window.push_notification(Notification::info("Version info copied to clipboard"), cx);
                            }),
                    )
                    .child(Button::new("about-ok").primary().label("OK").on_click(|_, window, cx| {
                        window.close_dialog(cx);
                    })),
            )
            .on_ok(|_, _, _| true)
    });
}

fn render_app_icon(theme: &gpui_kit::component::Theme) -> impl IntoElement {
    let fallback_primary = theme.primary;
    let fallback_secondary = theme.secondary;
    let fallback_border = theme.border;

    img(APP_ICON_PATH)
        .size(px(64.0))
        .flex_shrink_0()
        .object_fit(ObjectFit::Contain)
        .with_fallback(move || {
            div()
                .flex()
                .items_center()
                .justify_center()
                .size(px(64.0))
                .rounded_lg()
                .bg(fallback_secondary)
                .border_1()
                .border_color(fallback_border)
                .child(Icon::new(IconName::Binary).size(px(32.0)).text_color(fallback_primary))
                .into_any_element()
        })
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

    #[test]
    fn test_app_icon_asset_loads_and_renders() {
        use crate::assets::Assets;
        use gpui_kit::{AssetSource as _, SvgRenderer};
        use std::sync::Arc;

        assert_eq!(APP_ICON_PATH, "icons/xvw.svg");

        let bytes = Assets.load(APP_ICON_PATH).expect("load app icon asset").expect("app icon asset exists");
        let svg_text = std::str::from_utf8(&bytes).expect("app icon SVG is valid UTF-8");
        assert!(svg_text.contains("<svg"));

        let renderer = SvgRenderer::new(Arc::new(Assets));
        let rendered = renderer.render_single_frame(&bytes, 0.25).expect("render app icon SVG");
        assert_eq!(rendered.frame_count(), 1);
        let size = rendered.size(0);
        assert!(i32::from(size.width) > 0 && size.width == size.height);
        let bgra = rendered.as_bytes(0).expect("rendered frame bytes");
        assert!(
            bgra.as_chunks::<4>().0.iter().any(|px| px[3] > 0),
            "rendered icon should contain visible pixels"
        );
    }
}

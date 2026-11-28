use std::path::Path;

use crate::actions::{CommandPalette, NewEmptyFile, NewFile, OpenFile, OpenFileDialog, OpenFolder, QuickOpen, RemoveFileFromHistory};
use crate::core::recent_history::RecentFileEntry;
use crate::ui::icon::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme, Icon, Sizable as _, StyledExt};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, ParentElement, SharedString, StatefulInteractiveElement as _,
    Styled, div, px,
};

use super::Workspace;

/// Extracts `(file_name, parent_dir_display)` from a file path, shortening `home_dir` prefix to `~`.
pub(crate) fn format_recent_entry_display(path: &Path, home_dir: Option<&Path>) -> (String, String) {
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let parent_display = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => {
            if let Some(home) = home_dir
                && let Ok(stripped) = parent.strip_prefix(home)
            {
                if stripped.as_os_str().is_empty() {
                    "~".to_string()
                } else {
                    format!("~/{}", stripped.to_string_lossy())
                }
            } else {
                parent.to_string_lossy().to_string()
            }
        }
        _ => String::new(),
    };

    (file_name, parent_display)
}

fn render_action_card<F>(
    id: &'static str,
    icon: IconName,
    title: &'static str,
    shortcut: &'static str,
    on_trigger: F,
    cx: &Context<Workspace>,
) -> impl IntoElement
where
    F: Fn(&mut gpui_kit::Window, &mut App) + 'static,
{
    let theme = cx.theme();
    let hover_bg = theme.secondary.opacity(0.7);
    let hover_border = theme.primary.opacity(0.45);
    div()
        .id(id)
        .flex_1()
        .min_w(px(140.0))
        .px_3()
        .py_2p5()
        .rounded_lg()
        .bg(theme.secondary.opacity(0.35))
        .border_1()
        .border_color(theme.border)
        .cursor_pointer()
        .flex()
        .items_center()
        .justify_between()
        .gap_2()
        .hover(move |s| s.bg(hover_bg).border_color(hover_border))
        .on_click(move |_, window, cx| {
            on_trigger(window, cx);
        })
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .min_w_0()
                .child(Icon::new(icon).size(px(15.0)).text_color(theme.primary))
                .child(div().text_sm().font_medium().text_color(theme.foreground).truncate().child(title)),
        )
        .child(
            div()
                .px_1p5()
                .py(px(1.0))
                .rounded_sm()
                .bg(theme.background)
                .border_1()
                .border_color(theme.border)
                .text_xs()
                .text_color(theme.muted_foreground)
                .flex_shrink_0()
                .child(shortcut),
        )
}

/// Renders the Welcome screen shown when no editor tabs are open.
pub(crate) fn render_welcome_screen(recent_files: &[RecentFileEntry], cx: &Context<Workspace>) -> AnyElement {
    let theme = cx.theme();
    let is_macos = cfg!(target_os = "macos");
    let home_dir = dirs::home_dir();

    let (sc_open_file, sc_open_folder, sc_new_file, sc_quick_open, sc_palette) = if is_macos {
        ("⌘O", "⇧⌘O", "⌘N", "⌘P", "⇧⌘P")
    } else {
        ("Ctrl+O", "Ctrl+Shift+O", "Ctrl+N", "Ctrl+P", "Ctrl+Shift+P")
    };

    let quick_actions = div()
        .flex()
        .flex_col()
        .gap_2()
        .w_full()
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .w_full()
                .child(render_action_card(
                    "welcome-open-file",
                    IconName::FolderOpen,
                    "Open File...",
                    sc_open_file,
                    |window, cx| window.dispatch_action(Box::new(OpenFileDialog), cx),
                    cx,
                ))
                .child(render_action_card(
                    "welcome-open-folder",
                    IconName::Folder,
                    "Open Folder...",
                    sc_open_folder,
                    |window, cx| window.dispatch_action(Box::new(OpenFolder), cx),
                    cx,
                )),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .w_full()
                .child(render_action_card(
                    "welcome-new-file",
                    IconName::Plus,
                    "New File...",
                    sc_new_file,
                    |window, cx| window.dispatch_action(Box::new(NewFile), cx),
                    cx,
                ))
                .child(render_action_card(
                    "welcome-command-palette",
                    IconName::Sparkles,
                    "Command Palette",
                    sc_palette,
                    |window, cx| window.dispatch_action(Box::new(CommandPalette), cx),
                    cx,
                )),
        );

    let recent_section = (!recent_files.is_empty()).then(|| {
        let hover_bg = theme.secondary.opacity(0.6);
        let rows = recent_files.iter().enumerate().map(|(ix, entry)| {
            let (file_name, parent_display) = format_recent_entry_display(&entry.path, home_dir.as_deref());
            let path_str = entry.path.to_string_lossy().to_string();
            let open_path = path_str.clone();
            let remove_path = path_str;
            let format = entry.format;
            let format_label = format.map(|f| f.label());
            let row_group: SharedString = format!("welcome-recent-{ix}").into();

            div()
                .id(("welcome-recent-row", ix))
                .group(row_group.clone())
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .px_3()
                .py_1p5()
                .cursor_pointer()
                .when(ix > 0, |el| el.border_t_1().border_color(theme.border.opacity(0.6)))
                .hover(move |s| s.bg(hover_bg))
                .on_click(move |_, window, cx| {
                    window.dispatch_action(Box::new(OpenFile::with_format(open_path.clone(), format)), cx);
                })
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .flex_1()
                        .child(Icon::new(IconName::Binary).size(px(14.0)).text_color(theme.muted_foreground))
                        .child(div().text_sm().font_medium().text_color(theme.foreground).truncate().child(file_name))
                        .when_some(format_label, |el, label| {
                            el.child(
                                div()
                                    .px_1p5()
                                    .py(px(1.0))
                                    .rounded_sm()
                                    .flex_shrink_0()
                                    .bg(theme.secondary)
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(label),
                            )
                        })
                        .when(!parent_display.is_empty(), |el| {
                            el.child(div().text_xs().text_color(theme.muted_foreground).truncate().child(parent_display))
                        }),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .invisible()
                        .group_hover(row_group, |s| s.visible())
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .child(
                            Button::new(("welcome-recent-remove", ix))
                                .ghost()
                                .xsmall()
                                .icon(IconName::Close)
                                .tooltip("Remove from recent files")
                                .on_click(move |_, window, cx| {
                                    window.dispatch_action(Box::new(RemoveFileFromHistory { path: remove_path.clone() }), cx);
                                }),
                        ),
                )
        });

        div()
            .flex()
            .flex_col()
            .gap_1p5()
            .w_full()
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .px_1()
                    .child(div().text_xs().font_semibold().text_color(theme.muted_foreground).child("RECENT FILES"))
                    .child(
                        div()
                            .id("welcome-quick-open-link")
                            .cursor_pointer()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .hover(|s| s.text_color(theme.primary))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(QuickOpen), cx);
                            })
                            .child(format!("Quick Open ({sc_quick_open})")),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w_full()
                    .rounded_lg()
                    .bg(theme.secondary.opacity(0.2))
                    .border_1()
                    .border_color(theme.border)
                    .overflow_hidden()
                    .children(rows),
            )
    });

    div()
        .id("workspace-welcome-screen")
        .size_full()
        .min_w_0()
        .min_h_0()
        .overflow_y_scroll()
        .bg(theme.background)
        .flex()
        .items_center()
        .justify_center()
        .p_6()
        .on_mouse_down(MouseButton::Left, |event: &MouseDownEvent, window, cx| {
            if event.click_count == 2 {
                window.dispatch_action(Box::new(NewEmptyFile), cx);
            }
        })
        .child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_6()
                .w_full()
                .max_w(px(540.0))
                .on_mouse_down(MouseButton::Left, |_, _, cx| {
                    // Prevent double-clicking cards/rows from creating an unintended empty buffer.
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .w(px(48.0))
                                .h(px(48.0))
                                .rounded_xl()
                                .bg(theme.primary.opacity(0.12))
                                .border_1()
                                .border_color(theme.primary.opacity(0.25))
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(Icon::new(IconName::Binary).size(px(26.0)).text_color(theme.primary)),
                        )
                        .child(div().text_2xl().font_bold().text_color(theme.foreground).child("XVW"))
                        .child(div().text_sm().text_color(theme.muted_foreground).child("Binary & Hex Editor")),
                )
                .child(quick_actions)
                .children(recent_section)
                .child(
                    div()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child("Drop files or folders anywhere to open  •  Double-click background for an empty buffer"),
                ),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::format_recent_entry_display;
    use std::path::Path;

    #[test]
    fn test_format_recent_entry_display_shortens_home_prefix() {
        let home = Path::new("/home/alice");
        assert_eq!(
            format_recent_entry_display(Path::new("/home/alice/firmware/boot.bin"), Some(home)),
            ("boot.bin".to_string(), "~/firmware".to_string())
        );
        assert_eq!(
            format_recent_entry_display(Path::new("/home/alice/dump.hex"), Some(home)),
            ("dump.hex".to_string(), "~".to_string())
        );
        assert_eq!(
            format_recent_entry_display(Path::new("/var/tmp/test.bin"), Some(home)),
            ("test.bin".to_string(), "/var/tmp".to_string())
        );
    }
}

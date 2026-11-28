use crate::ui::icon::IconName;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AppContext as _, Context, EventEmitter, InteractiveElement as _, IntoElement, ParentElement, Render, StatefulInteractiveElement as _, Styled, Window, div,
    px,
};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Activity {
    Files = 0,
    Search = 1,
    Structure = 2,
    Inspector = 3,
    Map = 4,
    Checksum = 5,
    Bookmarks = 6,
    Strings = 7,
    Zlib = 8,
}

/// Returns the descriptive tooltip label (including shortcut hints where defined) for an ActivityBar icon.
pub(crate) fn activity_tooltip(activity: Activity, is_macos: bool) -> &'static str {
    match activity {
        Activity::Files => "Files Explorer",
        Activity::Search => {
            if is_macos {
                "Search in File (⇧⌘F)"
            } else {
                "Search in File (Ctrl+Shift+F)"
            }
        }
        Activity::Strings => "Strings",
        Activity::Zlib => "Zlib / Deflate Streams",
        Activity::Structure => "Structure Tree (Kaitai)",
        Activity::Inspector => "Data Inspector",
        Activity::Map => "Visual Map",
        Activity::Checksum => "Checksums & Hashes",
        Activity::Bookmarks => "Bookmarks",
    }
}

pub(crate) fn settings_tooltip(is_macos: bool) -> &'static str {
    if is_macos { "Settings (⌘,)" } else { "Settings (Ctrl+,)" }
}

pub enum ActivityBarEvent {
    Select(Activity),
    OpenSettings,
}

pub struct ActivityBar {
    pub active_activity: Option<Activity>,
}

impl EventEmitter<ActivityBarEvent> for ActivityBar {}

impl ActivityBar {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            active_activity: Some(Activity::Files),
        }
    }

    pub fn set_activity(&mut self, activity: Option<Activity>, cx: &mut Context<Self>) {
        if self.active_activity != activity {
            self.active_activity = activity;
            cx.notify();
        }
    }
}

impl Render for ActivityBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let (bg_color, border_color, foreground_color, muted_color, hover_bg) = (
            theme.sidebar,
            theme.border,
            theme.foreground,
            theme.muted_foreground,
            theme.sidebar_accent.opacity(0.65),
        );
        let is_macos = cfg!(target_os = "macos");
        let settings_tip = settings_tooltip(is_macos);

        div()
            .id("activity-bar")
            .flex()
            .flex_col()
            .w(px(44.0))
            .flex_shrink_0()
            .h_full()
            .bg(bg_color)
            .border_r_1()
            .border_color(border_color)
            .items_center()
            .py_2()
            .justify_between()
            .child(
                div()
                    .id("activity-bar-items")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .overflow_y_scroll()
                    .gap_1()
                    .items_center()
                    .child(self.render_icon(Activity::Files, IconName::Files, activity_tooltip(Activity::Files, is_macos), cx))
                    .child(self.render_icon(Activity::Search, IconName::Search, activity_tooltip(Activity::Search, is_macos), cx))
                    .child(self.render_icon(Activity::Strings, IconName::SearchText, activity_tooltip(Activity::Strings, is_macos), cx))
                    .child(self.render_icon(Activity::Zlib, IconName::PackageSearch, activity_tooltip(Activity::Zlib, is_macos), cx))
                    .child(self.render_icon(Activity::Structure, IconName::Braces, activity_tooltip(Activity::Structure, is_macos), cx))
                    .child(self.render_icon(Activity::Inspector, IconName::Binary, activity_tooltip(Activity::Inspector, is_macos), cx))
                    .child(self.render_icon(Activity::Map, IconName::Map, activity_tooltip(Activity::Map, is_macos), cx))
                    .child(self.render_icon(Activity::Checksum, IconName::Hash, activity_tooltip(Activity::Checksum, is_macos), cx))
                    .child(self.render_icon(Activity::Bookmarks, IconName::Bookmark, activity_tooltip(Activity::Bookmarks, is_macos), cx)),
            )
            .child(
                div()
                    .pt_2()
                    .w_full()
                    .flex()
                    .justify_center()
                    .border_t_1()
                    .border_color(border_color.opacity(0.6))
                    .child(
                        div()
                            .id("activity-settings")
                            .cursor_pointer()
                            .w(px(34.0))
                            .h(px(34.0))
                            .rounded_md()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(muted_color)
                            .hover(move |style| style.text_color(foreground_color).bg(hover_bg))
                            .tooltip(move |_window, cx| cx.new(|_| Tooltip::new(settings_tip)).into())
                            .on_click(cx.listener(move |_, _, _window, cx| {
                                cx.emit(ActivityBarEvent::OpenSettings);
                            }))
                            .child(Icon::new(IconName::Settings).size(px(20.0))),
                    ),
            )
    }
}

impl ActivityBar {
    fn render_icon(&self, activity: Activity, icon: IconName, tooltip: &'static str, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let is_active = self.active_activity == Some(activity);
        let active_bg = theme.sidebar_accent.opacity(0.8);
        let hover_bg = theme.sidebar_accent.opacity(0.65);

        div()
            .id(("activity", activity as u32))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .relative()
            .when(is_active, |this| {
                this.child(
                    div()
                        .absolute()
                        .left_0()
                        .top(px(6.0))
                        .bottom(px(6.0))
                        .w(px(2.5))
                        .rounded_r_full()
                        .bg(theme.sidebar_primary),
                )
            })
            .child(
                div()
                    .id(("activity-btn", activity as u32))
                    .cursor_pointer()
                    .w(px(34.0))
                    .h(px(34.0))
                    .rounded_md()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(if is_active { theme.foreground } else { theme.muted_foreground })
                    .when(is_active, |this| this.bg(active_bg))
                    .hover(move |style| style.text_color(theme.foreground).bg(if is_active { active_bg } else { hover_bg }))
                    .tooltip(move |_window, cx| cx.new(|_| Tooltip::new(tooltip)).into())
                    .on_click(cx.listener(move |_, _, _window, cx| {
                        cx.emit(ActivityBarEvent::Select(activity));
                    }))
                    .child(Icon::new(icon).size(px(20.0))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{Activity, activity_tooltip, settings_tooltip};

    #[test]
    fn test_activity_tooltips_are_non_empty_and_platform_aware() {
        let all = [
            Activity::Files,
            Activity::Search,
            Activity::Strings,
            Activity::Zlib,
            Activity::Structure,
            Activity::Inspector,
            Activity::Map,
            Activity::Checksum,
            Activity::Bookmarks,
        ];
        for activity in all {
            assert!(!activity_tooltip(activity, true).is_empty());
            assert!(!activity_tooltip(activity, false).is_empty());
        }
        assert!(activity_tooltip(Activity::Search, true).contains("⌘"));
        assert!(activity_tooltip(Activity::Search, false).contains("Ctrl"));
        assert!(settings_tooltip(true).contains("⌘"));
        assert!(settings_tooltip(false).contains("Ctrl"));
    }
}

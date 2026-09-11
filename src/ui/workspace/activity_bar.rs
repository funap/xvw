use crate::ui::icon::IconName;
use gpui_kit::component::{ActiveTheme, Icon};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Activity {
    Files = 0,
    Search = 1,
    Structure = 2,
    Inspector = 3,
    Map = 4,
    Checksum = 5,
    Bookmarks = 6,
    Strings = 7,
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
        let (bg_color, border_color, foreground_color, muted_color) = (theme.sidebar, theme.border, theme.foreground, theme.muted_foreground);

        div()
            .id("activity-bar")
            .flex()
            .flex_col()
            .w(px(42.0))
            .flex_shrink_0()
            .h_full()
            .bg(bg_color)
            .when(self.active_activity.is_some(), |this| this.border_r_1().border_color(border_color))
            .items_center()
            .py_4()
            .justify_between()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .items_center()
                    .child(self.render_icon(Activity::Files, IconName::Files, "Files", cx))
                    .child(self.render_icon(Activity::Search, IconName::Search, "Search", cx))
                    .child(self.render_icon(Activity::Strings, IconName::SearchText, "Strings", cx))
                    .child(self.render_icon(Activity::Structure, IconName::Braces, "Structure", cx))
                    .child(self.render_icon(Activity::Inspector, IconName::Binary, "Inspector", cx))
                    .child(self.render_icon(Activity::Map, IconName::Map, "Map", cx))
                    .child(self.render_icon(Activity::Checksum, IconName::Hash, "Checksum", cx))
                    .child(self.render_icon(Activity::Bookmarks, IconName::Bookmark, "Bookmarks", cx)),
            )
            .child(
                div()
                    .id("activity-settings")
                    .cursor_pointer()
                    .p_2()
                    .text_color(muted_color)
                    .hover(|style| style.text_color(foreground_color))
                    .on_click(cx.listener(move |_, _, _window, cx| {
                        cx.emit(ActivityBarEvent::OpenSettings);
                    }))
                    .child(Icon::new(IconName::Settings).size(px(24.0))),
            )
    }
}

impl ActivityBar {
    fn render_icon(&self, activity: Activity, icon: IconName, _tooltip: &str, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let is_active = self.active_activity == Some(activity);

        div()
            .id(("activity", activity as u32))
            .cursor_pointer()
            .p_2()
            .text_color(if is_active { theme.foreground } else { theme.muted_foreground })
            .relative()
            .hover(|style| style.text_color(theme.foreground))
            .on_click(cx.listener(move |_, _, _window, cx| {
                cx.emit(ActivityBarEvent::Select(activity));
            }))
            .child(Icon::new(icon).size(px(24.0)))
            .when(is_active, |this| {
                this.child(div().absolute().left_0().top_2().bottom_2().w_0p5().bg(theme.accent))
            })
    }
}

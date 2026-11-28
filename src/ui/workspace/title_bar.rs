use crate::ui::icon::IconName;
use crate::ui::menus::MenuEditorState;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{ActiveTheme, Icon, Selectable, Sizable, StyledExt, TitleBar, button::Button, h_flex};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Action, Anchor, App, AppContext as _, ClickEvent, Context, DismissEvent, Entity, EventEmitter, Focusable as _, InteractiveElement as _, IntoElement,
    KeyBinding, MouseButton, ParentElement, Render, SharedString, StatefulInteractiveElement as _, Styled, Subscription, WeakEntity, Window, anchored,
    deferred, div, px,
};

const CONTEXT: &str = "AppMenuBar";

#[derive(Clone, PartialEq, Action)]
pub struct MenuCancel;
#[derive(Clone, PartialEq, Action)]
pub struct MenuSelectLeft;
#[derive(Clone, PartialEq, Action)]
pub struct MenuSelectRight;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", MenuCancel, Some(CONTEXT)),
        KeyBinding::new("left", MenuSelectLeft, Some(CONTEXT)),
        KeyBinding::new("right", MenuSelectRight, Some(CONTEXT)),
    ]);
}

/// Snapshot of workspace state rendered inside `AppTitleBar`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TitleBarState {
    pub document_title: Option<String>,
    pub is_dirty: bool,
    pub is_read_only: bool,
    pub is_left_panel_visible: bool,
    pub is_right_panel_visible: bool,
}

/// Formats the native OS window title from the active document title and dirty state.
pub(crate) fn format_os_window_title(document_title: Option<&str>, is_dirty: bool) -> String {
    match document_title {
        Some(title) if !title.is_empty() => {
            if is_dirty {
                format!("{title} * — XVW")
            } else {
                format!("{title} — XVW")
            }
        }
        _ => "XVW".to_string(),
    }
}

pub enum AppTitleBarEvent {
    OpenSettings,
    OpenAbout,
}

pub struct AppTitleBar {
    pub app_menu_bar: Entity<AppMenuBar>,
    state: TitleBarState,
    last_window_title: String,
}

impl EventEmitter<AppTitleBarEvent> for AppTitleBar {}

impl AppTitleBar {
    pub fn new(workspace: WeakEntity<crate::ui::workspace::Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app_menu_bar = AppMenuBar::new(workspace, window, cx);
        Self {
            app_menu_bar,
            state: TitleBarState {
                is_left_panel_visible: true,
                ..Default::default()
            },
            last_window_title: "XVW".to_string(),
        }
    }

    pub fn set_state(&mut self, state: TitleBarState, cx: &mut Context<Self>) {
        if self.state != state {
            self.state = state;
            cx.notify();
        }
    }
}

impl Render for AppTitleBar {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let os_title = format_os_window_title(self.state.document_title.as_deref(), self.state.is_dirty);
        if self.last_window_title != os_title {
            self.last_window_title = os_title.clone();
            window.set_window_title(&os_title);
        }

        let theme = cx.theme();
        let is_macos = cfg!(target_os = "macos");

        let center_title = match &self.state.document_title {
            Some(title) => div()
                .flex()
                .items_center()
                .justify_center()
                .gap_1p5()
                .min_w_0()
                .overflow_hidden()
                .child(div().text_xs().font_medium().text_color(theme.foreground).truncate().child(title.clone()))
                .when(self.state.is_dirty, |el| {
                    el.child(div().w(px(6.0)).h(px(6.0)).rounded_full().flex_shrink_0().bg(theme.primary))
                })
                .when(self.state.is_read_only, |el| {
                    el.child(
                        div()
                            .flex()
                            .items_center()
                            .gap_0p5()
                            .px_1p5()
                            .py(px(1.0))
                            .rounded_sm()
                            .flex_shrink_0()
                            .bg(theme.secondary)
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(Icon::new(IconName::PenOff).size(px(10.0)).text_color(theme.muted_foreground))
                            .child("Read-only"),
                    )
                })
                .into_any_element(),
            None => div().text_xs().font_medium().text_color(theme.muted_foreground).child("XVW").into_any_element(),
        };

        let left_and_center = if is_macos {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .justify_center()
                .px_2()
                .child(center_title)
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap_2()
                .child(self.app_menu_bar.clone())
                .child(div().flex_1().min_w_0().flex().items_center().justify_center().px_2().child(center_title))
                .into_any_element()
        };

        let left_panel_tooltip = if is_macos { "Toggle Left Panel (⌘B)" } else { "Toggle Left Panel (Ctrl+B)" };
        let right_panel_tooltip = if is_macos {
            "Toggle Scratchpad (⌥⌘B)"
        } else {
            "Toggle Scratchpad (Alt+Ctrl+B)"
        };
        let settings_tooltip = if is_macos { "Settings (⌘,)" } else { "Settings (Ctrl+,)" };

        TitleBar::new().child(left_and_center).child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .flex_shrink_0()
                .gap_1()
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    // Stop propagation to avoid dragging the window.
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .child(
                    Button::new("toggle-left-panel")
                        .small()
                        .ghost()
                        .selected(self.state.is_left_panel_visible)
                        .icon(IconName::PanelLeft)
                        .tooltip(left_panel_tooltip)
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(crate::actions::ToggleLeftPanel), cx);
                        })),
                )
                .child(
                    Button::new("toggle-scratchpad-panel")
                        .small()
                        .ghost()
                        .selected(self.state.is_right_panel_visible)
                        .icon(IconName::PanelRight)
                        .tooltip(right_panel_tooltip)
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .on_click(cx.listener(|_, _, window, cx| {
                            window.dispatch_action(Box::new(crate::actions::ToggleRightPanel), cx);
                        })),
                )
                .child(div().w_px().h_3p5().mx_0p5().bg(theme.border))
                .child(
                    Button::new("settings")
                        .small()
                        .ghost()
                        .icon(IconName::Settings)
                        .tooltip(settings_tooltip)
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            // Stop propagation to avoid dragging the window.
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(AppTitleBarEvent::OpenSettings);
                        })),
                )
                .child(
                    Button::new("about")
                        .small()
                        .ghost()
                        .icon(IconName::Info)
                        .tooltip("About XVW")
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            // Stop propagation to avoid dragging the window.
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .on_click(cx.listener(|_, _, _, cx| {
                            cx.emit(AppTitleBarEvent::OpenAbout);
                        })),
                ),
        )
    }
}

pub struct AppMenuBar {
    menus: Vec<Entity<AppMenu>>,
    selected_ix: Option<usize>,
}

impl AppMenuBar {
    pub fn new(workspace: WeakEntity<crate::ui::workspace::Workspace>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| {
            let menu_bar = cx.entity();
            let menus = crate::ui::menus::application_menus()
                .iter()
                .enumerate()
                .map(|(ix, menu_def)| AppMenu::new(ix, menu_def.name.into(), workspace.clone(), menu_bar.clone(), window, cx))
                .collect();
            Self { menus, selected_ix: None }
        })
    }

    fn on_move_left(&mut self, _: &MenuSelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected_ix) = self.selected_ix else {
            return;
        };
        let new_ix = if selected_ix == 0 {
            self.menus.len().saturating_sub(1)
        } else {
            selected_ix.saturating_sub(1)
        };
        self.set_selected_index(Some(new_ix), window, cx);
    }

    fn on_move_right(&mut self, _: &MenuSelectRight, window: &mut Window, cx: &mut Context<Self>) {
        let Some(selected_ix) = self.selected_ix else {
            return;
        };
        let new_ix = if selected_ix + 1 >= self.menus.len() { 0 } else { selected_ix + 1 };
        self.set_selected_index(Some(new_ix), window, cx);
    }

    fn on_cancel(&mut self, _: &MenuCancel, window: &mut Window, cx: &mut Context<Self>) {
        self.set_selected_index(None, window, cx);
    }

    pub fn set_selected_index(&mut self, ix: Option<usize>, _: &mut Window, cx: &mut Context<Self>) {
        if self.selected_ix == ix {
            return;
        }
        self.selected_ix = ix;
        cx.notify();
    }

    #[inline]
    fn has_activated_menu(&self) -> bool {
        self.selected_ix.is_some()
    }
}

impl Render for AppMenuBar {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        h_flex()
            .id("app-menu-bar")
            .key_context(CONTEXT)
            .on_action(cx.listener(Self::on_move_left))
            .on_action(cx.listener(Self::on_move_right))
            .on_action(cx.listener(Self::on_cancel))
            .h_full()
            .flex_shrink_0()
            .gap_x_1()
            .overflow_x_scroll()
            .children(self.menus.clone())
    }
}

pub struct AppMenu {
    ix: usize,
    name: SharedString,
    workspace: WeakEntity<crate::ui::workspace::Workspace>,
    menu_bar: Entity<AppMenuBar>,
    popup_menu: Option<Entity<PopupMenu>>,
    _subscription: Option<Subscription>,
}

impl AppMenu {
    pub fn new(
        ix: usize,
        name: SharedString,
        workspace: WeakEntity<crate::ui::workspace::Workspace>,
        menu_bar: Entity<AppMenuBar>,
        _: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|_| Self {
            ix,
            name,
            workspace,
            menu_bar,
            popup_menu: None,
            _subscription: None,
        })
    }

    fn query_editor_state(&self, cx: &mut Context<Self>) -> MenuEditorState {
        if let Some(workspace) = self.workspace.upgrade() {
            workspace.read(cx).menu_editor_state(cx)
        } else {
            MenuEditorState {
                is_read_only: true,
                ..Default::default()
            }
        }
    }

    fn build_popup_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<PopupMenu> {
        let popup_menu = match self.popup_menu.as_ref() {
            None => {
                let focus_handle = window.focused(cx);
                let state = self.query_editor_state(cx);
                let ix = self.ix;
                let popup = PopupMenu::build(window, cx, move |menu, window, cx| {
                    let menu = menu.when_some(focus_handle, |this, handle| this.action_context(handle));
                    if let Some(menu_def) = crate::ui::menus::application_menus().get(ix) {
                        menu_def.build_popup_menu(menu, &state, window, cx)
                    } else {
                        menu
                    }
                });
                popup.read(cx).focus_handle(cx).focus(window, cx);
                self._subscription = Some(cx.subscribe_in(&popup, window, Self::handle_dismiss));
                self.popup_menu = Some(popup.clone());
                popup
            }
            Some(menu) => menu.clone(),
        };

        let focus_handle = popup_menu.read(cx).focus_handle(cx);
        if !focus_handle.contains_focused(window, cx) {
            focus_handle.focus(window, cx);
        }

        popup_menu
    }

    fn handle_dismiss(&mut self, _: &Entity<PopupMenu>, _: &DismissEvent, window: &mut Window, cx: &mut Context<Self>) {
        self._subscription.take();
        self.popup_menu.take();
        self.menu_bar.update(cx, |state, cx| {
            state.on_cancel(&MenuCancel, window, cx);
        });
    }

    fn handle_trigger_click(&mut self, _: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        let is_selected = self.menu_bar.read(cx).selected_ix == Some(self.ix);
        self.menu_bar.update(cx, |state, cx| {
            let new_ix = if is_selected { None } else { Some(self.ix) };
            state.set_selected_index(new_ix, window, cx);
        });
    }

    fn handle_hover(&mut self, hovered: &bool, window: &mut Window, cx: &mut Context<Self>) {
        if !*hovered {
            return;
        }
        let has_activated_menu = self.menu_bar.read(cx).has_activated_menu();
        if !has_activated_menu {
            return;
        }
        self.menu_bar.update(cx, |state, cx| {
            state.set_selected_index(Some(self.ix), window, cx);
        });
    }
}

impl Render for AppMenu {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let menu_bar = self.menu_bar.read(cx);
        let is_selected = menu_bar.selected_ix == Some(self.ix);
        if !is_selected {
            self._subscription.take();
            self.popup_menu.take();
        }

        div()
            .id(self.ix)
            .relative()
            .child(
                Button::new("menu")
                    .small()
                    .py_0p5()
                    .compact()
                    .ghost()
                    .label(self.name.clone())
                    .selected(is_selected)
                    .on_mouse_down(MouseButton::Left, |_, window, cx| {
                        // Stop propagation to avoid dragging the window.
                        window.prevent_default();
                        cx.stop_propagation();
                    })
                    .on_click(cx.listener(Self::handle_trigger_click)),
            )
            .on_hover(cx.listener(Self::handle_hover))
            .when(is_selected, |this| {
                this.child(deferred(
                    anchored()
                        .anchor(Anchor::TopLeft)
                        .snap_to_window_with_margin(px(8.))
                        .child(div().size_full().occlude().top_1().child(self.build_popup_menu(window, cx))),
                ))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::format_os_window_title;

    #[test]
    fn test_format_os_window_title() {
        assert_eq!(format_os_window_title(None, false), "XVW");
        assert_eq!(format_os_window_title(Some(""), false), "XVW");
        assert_eq!(format_os_window_title(Some("firmware.bin"), false), "firmware.bin — XVW");
        assert_eq!(format_os_window_title(Some("firmware.bin"), true), "firmware.bin * — XVW");
    }
}

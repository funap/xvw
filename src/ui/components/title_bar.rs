use crate::ui::icon::IconName;
use crate::ui::menus::MenuEditorState;
use gpui_kit::component::button::ButtonVariants;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{Selectable, Sizable, TitleBar, button::Button, h_flex};
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

pub enum AppTitleBarEvent {
    OpenSettings,
    OpenAbout,
}

pub struct AppTitleBar {
    pub app_menu_bar: Entity<AppMenuBar>,
}

impl EventEmitter<AppTitleBarEvent> for AppTitleBar {}

impl AppTitleBar {
    pub fn new(workspace: WeakEntity<crate::ui::workspace::Workspace>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let app_menu_bar = AppMenuBar::new(workspace, window, cx);
        Self { app_menu_bar }
    }
}

impl Render for AppTitleBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        TitleBar::new().child(div().flex().items_center().child(self.app_menu_bar.clone())).child(
            div()
                .flex()
                .items_center()
                .justify_end()
                .gap_2()
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    // Stop propagation to avoid dragging the window.
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .child(
                    Button::new("settings")
                        .ghost()
                        .icon(IconName::Settings)
                        .tooltip("Settings")
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
                        .ghost()
                        .icon(IconName::Info)
                        .tooltip("About")
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
            .size_full()
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
            let ws = workspace.read(cx);
            if let Some(editor) = ws.active_editor(cx) {
                let ed = editor.read(cx);
                let is_ro = ed.is_read_only();
                let can_u = !is_ro && ed.can_undo();
                let can_r = !is_ro && ed.can_redo();
                let has_sel = ed.has_selection();
                let (can_o, can_r_tab, has_s) = if let Some(group) = ws.pane_tree.read(cx).active_group(cx) {
                    let g = group.read(cx);
                    let can_o = g.tabs.len() > 1;
                    let active_ix = g.active_index;
                    let can_r_tab = active_ix + 1 < g.tabs.len();
                    let has_s = g.tabs.iter().any(|t| !t.is_dirty(cx));
                    (can_o, can_r_tab, has_s)
                } else {
                    (false, false, false)
                };
                MenuEditorState {
                    has_doc: true,
                    is_read_only: is_ro,
                    can_undo: can_u,
                    can_redo: can_r,
                    has_selection: has_sel,
                    can_close_others: can_o,
                    can_close_right: can_r_tab,
                    has_saved: has_s,
                }
            } else {
                MenuEditorState {
                    is_read_only: true,
                    ..Default::default()
                }
            }
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

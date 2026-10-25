use crate::core::encoding::Encoding;
use crate::core::layout::{BytesPerRow, DEFAULT_BYTES_PER_ROW, MAX_BYTES_PER_ROW, MIN_BYTES_PER_ROW};
use crate::core::radix::{ByteCategory, ByteCategoryColors, ByteColoring, ByteGroupSize, ByteOrder, CategoryColor, DisplayRadix};
use crate::core::structure::{StructureYamlIncludeOffsets, StructureYamlShaThreshold};
use crate::ui::appearance::{Appearance, MAX_FONT_SIZE, MIN_FONT_SIZE};
use crate::ui::icon::IconName;
use crate::ui::views::hex_view::resolve_category_color;
use gpui_kit::component::{
    ActiveTheme, Icon, IndexPath, Sizable as _, Size, StyledExt, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dock::{Panel, PanelEvent},
    input::{self, Input, InputState, NumberInput},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    searchable_list::{SearchableListDelegate, SearchableListItem},
    select::{Select, SelectEvent, SelectState},
    theme::Theme,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    Action, Anchor, AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, KeyDownEvent, ParentElement, Render,
    SharedString, Subscription, WeakEntity, Window, div, px,
};

use std::cell::RefCell;
use std::rc::Rc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemePreviewState {
    origin: Option<SharedString>,
    current_index: Option<usize>,
    confirmed: bool,
    open: bool,
}

impl ThemePreviewState {
    /// Returns whether the theme dropdown popup is currently open.
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Sets whether the theme dropdown popup is open.
    pub fn set_open(&mut self, open: bool) {
        self.open = open;
    }

    /// Records the current theme as origin if not already previewing.
    pub fn start_preview(&mut self, current_theme: SharedString) {
        if self.origin.is_none() {
            self.origin = Some(current_theme);
            self.confirmed = false;
        }
    }

    /// Sets the current preview index (e.g. from mouse hover).
    pub fn set_hover_index(&mut self, index: usize) {
        self.current_index = Some(index);
    }

    /// Steps selection index up or down across `total_items`, matching PopupMenu keyboard behavior.
    /// Returns the newly selected index.
    #[allow(dead_code)]
    pub fn step_index(&mut self, step_down: bool, total_items: usize) -> Option<usize> {
        if total_items == 0 {
            return None;
        }
        let next_idx = match self.current_index {
            None => {
                if step_down {
                    0
                } else {
                    total_items - 1
                }
            }
            Some(idx) => {
                if step_down {
                    if idx + 1 >= total_items { 0 } else { idx + 1 }
                } else {
                    if idx == 0 { total_items - 1 } else { idx - 1 }
                }
            }
        };
        self.current_index = Some(next_idx);
        Some(next_idx)
    }

    /// Marks the selected theme as confirmed and clears origin so dismissal will not revert.
    pub fn confirm(&mut self) {
        self.confirmed = true;
        self.origin = None;
        self.current_index = None;
        self.open = false;
    }

    /// Cancels preview and returns the original theme to restore, if not confirmed.
    pub fn cancel(&mut self) -> Option<SharedString> {
        self.current_index = None;
        self.open = false;
        let is_confirmed = self.confirmed;
        self.confirmed = false;
        if !is_confirmed {
            self.origin.take()
        } else {
            self.origin = None;
            None
        }
    }

    /// Returns the original saved theme name if actively previewing, or None.
    pub fn origin(&self) -> Option<&SharedString> {
        self.origin.as_ref()
    }

    /// Returns the currently previewed index, if any.
    pub fn current_index(&self) -> Option<usize> {
        self.current_index
    }
}

/// Calculates the next index when stepping up or down across a list of items of length `total`,
/// starting from `current_index`. Stepping down advances with wraparound; stepping up retreats with wraparound.
pub fn cycle_theme_index(current_index: usize, step_down: bool, total: usize) -> usize {
    if total == 0 {
        return 0;
    }
    if step_down {
        if current_index + 1 >= total { 0 } else { current_index + 1 }
    } else {
        if current_index == 0 { total - 1 } else { current_index - 1 }
    }
}

#[derive(Clone)]
pub struct ThemeListDelegate {
    themes: Vec<SharedString>,
    preview_state: Rc<RefCell<ThemePreviewState>>,
    select_handle: Rc<RefCell<Option<WeakEntity<SelectState<Self>>>>>,
}

impl ThemeListDelegate {
    pub fn new(themes: Vec<SharedString>, preview_state: Rc<RefCell<ThemePreviewState>>) -> Self {
        Self {
            themes,
            preview_state,
            select_handle: Rc::new(RefCell::new(None)),
        }
    }

    pub fn with_select_handle(mut self, select_handle: Rc<RefCell<Option<WeakEntity<SelectState<Self>>>>>) -> Self {
        self.select_handle = select_handle;
        self
    }

    /// Checks whether an item should display the checkmark.
    /// When actively previewing, the checkmark stays firmly on the original saved theme.
    pub fn check_item(&self, item: &SharedString, current_selection: &[(IndexPath, SharedString)]) -> bool {
        if let Some(origin) = self.preview_state.borrow().origin() {
            item == origin
        } else {
            current_selection.iter().any(|(_, selected_item)| selected_item.value() == item.value())
        }
    }
}

impl SearchableListDelegate for ThemeListDelegate {
    type Item = SharedString;

    fn items_count(&self, _section: usize) -> usize {
        self.themes.len()
    }

    fn item(&self, ix: IndexPath) -> Option<&Self::Item> {
        self.themes.get(ix.row)
    }

    fn position<V>(&self, value: &V) -> Option<IndexPath>
    where
        Self::Item: SearchableListItem<Value = V>,
        V: PartialEq,
    {
        self.themes.iter().position(|item| item.value() == value).map(IndexPath::new)
    }

    fn is_item_checked(&self, _ix: IndexPath, item: &Self::Item, current_selection: &[(IndexPath, Self::Item)], _cx: &App) -> bool {
        self.check_item(item, current_selection)
    }

    fn render_item(&self, ix: IndexPath, item: &Self::Item, checked: bool, _window: &mut Window, cx: &mut App) -> Option<AnyElement> {
        let name = item.clone();
        let preview_state = self.preview_state.clone();
        let select_handle = self.select_handle.clone();
        {
            let mut state = preview_state.borrow_mut();
            state.set_open(true);
            if checked && state.current_index().is_none() {
                state.start_preview(cx.theme().theme_name().clone());
                state.set_hover_index(ix.row);
            }
        }
        Some(
            div()
                .id(SharedString::from(format!("theme-item-{}", name)))
                .w_full()
                .h_full()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(div().truncate().child(name.clone()))
                .when(checked, |this| this.child(Icon::new(IconName::Check).size(px(14.0)).flex_shrink_0()))
                .on_hover(move |hovered, window, cx| {
                    if *hovered {
                        let weak = select_handle.borrow().clone();
                        if let Some(weak) = weak
                            && let Some(theme_select) = weak.upgrade()
                        {
                            theme_select.update(cx, |select, cx| {
                                if select.selected_index(cx) != Some(ix) {
                                    select.set_selected_index(Some(ix), window, cx);
                                }
                            });
                        }

                        let (need_apply, theme_name) = {
                            let mut state = preview_state.borrow_mut();
                            state.set_hover_index(ix.row);
                            if cx.theme().theme_name() != name.as_ref() {
                                state.start_preview(cx.theme().theme_name().clone());
                                (true, Some(name.clone()))
                            } else {
                                (false, None)
                            }
                        };
                        if need_apply && let Some(theme_name) = theme_name {
                            crate::theme::apply_theme_by_name(&theme_name, Some(window), cx);
                        }
                    }
                })
                .into_any_element(),
        )
    }
}

#[derive(Clone, PartialEq, Action)]
pub struct UpdateSettingInput;

pub struct SettingsView {
    focus_handle: FocusHandle,
    theme_select: Entity<SelectState<ThemeListDelegate>>,
    font_family_input: Entity<InputState>,
    font_size_input: Entity<InputState>,
    bytes_per_row_input: Entity<InputState>,
    structure_yaml_sha_threshold_input: Entity<InputState>,
    theme_preview_state: Rc<RefCell<ThemePreviewState>>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let theme_preview_state = Rc::new(RefCell::new(ThemePreviewState::default()));

        let all_themes = crate::theme::all_theme_names(cx);
        let active_theme_name = cx.theme().theme_name().clone();
        let current_idx = all_themes.iter().position(|t| t == &active_theme_name).unwrap_or(0);
        let select_handle = Rc::new(RefCell::new(None));
        let delegate = ThemeListDelegate::new(all_themes.clone(), theme_preview_state.clone()).with_select_handle(select_handle.clone());
        let theme_select = cx.new(|cx| SelectState::new(delegate, Some(IndexPath::new(current_idx)), window, cx));
        *select_handle.borrow_mut() = Some(theme_select.downgrade());

        let font_family_input = cx.new(|cx| InputState::new(window, cx));
        let font_size_input = cx.new(|cx| InputState::new(window, cx).step(1.0).min(MIN_FONT_SIZE as f64).max(MAX_FONT_SIZE as f64));
        let bytes_per_row_input = cx.new(|cx| {
            InputState::new(window, cx)
                .step(1.0)
                .min(MIN_BYTES_PER_ROW as f64)
                .max(MAX_BYTES_PER_ROW as f64)
        });
        let structure_yaml_sha_threshold_input = cx.new(|cx| {
            InputState::new(window, cx)
                .step(1.0)
                .min(0.0)
                .max(crate::core::structure::MAX_STRUCTURE_YAML_SHA_THRESHOLD as f64)
        });

        // The global must be retrieved after the entities have been created.
        let (family, size, bytes_per_row, yaml_sha_threshold) = {
            let appearance = cx.global::<Appearance>();
            let bytes_per_row = cx.global::<BytesPerRow>().0;
            let yaml_sha_threshold = cx.global::<StructureYamlShaThreshold>().0;
            (
                appearance.font_family.clone(),
                appearance.font_size.to_string(),
                bytes_per_row.to_string(),
                yaml_sha_threshold.to_string(),
            )
        };

        // Set initial values
        font_family_input.update(cx, |input: &mut InputState, cx| {
            input.set_value(family, window, cx);
        });

        font_size_input.update(cx, |input: &mut InputState, cx| {
            input.set_value(size, window, cx);
        });

        bytes_per_row_input.update(cx, |input: &mut InputState, cx| {
            input.set_value(bytes_per_row, window, cx);
        });

        structure_yaml_sha_threshold_input.update(cx, |input: &mut InputState, cx| {
            input.set_value(yaml_sha_threshold, window, cx);
        });

        let mut subscriptions = Vec::new();

        subscriptions.push(cx.observe_global::<Appearance>(|_, cx| {
            cx.dispatch_action(&UpdateSettingInput);
        }));

        subscriptions.push(cx.observe_global::<BytesPerRow>(|_, cx| {
            cx.dispatch_action(&UpdateSettingInput);
        }));

        subscriptions.push(cx.observe_global::<Theme>(|_, cx| {
            cx.notify();
        }));

        subscriptions.push(cx.observe_global::<Encoding>(|_, cx| {
            cx.notify();
        }));

        subscriptions.push(cx.observe_global::<DisplayRadix>(|_, cx| {
            cx.notify();
        }));

        subscriptions.push(cx.observe_global::<ByteGroupSize>(|_, cx| {
            cx.notify();
        }));

        subscriptions.push(cx.observe_global::<ByteOrder>(|_, cx| {
            cx.notify();
        }));

        subscriptions.push(cx.observe_global::<ByteColoring>(|_, cx| {
            cx.notify();
        }));

        subscriptions.push(cx.observe_global::<ByteCategoryColors>(|_, cx| {
            cx.notify();
        }));

        subscriptions.push(cx.observe_global::<StructureYamlShaThreshold>(|_, cx| {
            cx.dispatch_action(&UpdateSettingInput);
        }));

        subscriptions.push(cx.observe_global::<StructureYamlIncludeOffsets>(|_, cx| {
            cx.notify();
        }));

        subscriptions.push(cx.subscribe(&font_family_input, |_, input: Entity<InputState>, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                let value = input.read(cx).value().to_string();
                cx.update_global::<Appearance, _>(|appearance, _| {
                    appearance.font_family = value;
                });
                crate::settings::save_current(cx);
            }
        }));

        subscriptions.push(cx.subscribe(&font_size_input, |_, input: Entity<InputState>, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                let value = input.read(cx).value().to_string();
                if let Ok(size) = value.parse::<f32>()
                    && (MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&size)
                {
                    cx.update_global::<Appearance, _>(|appearance, _| {
                        appearance.font_size = size;
                    });
                    crate::settings::save_current(cx);
                }
            }
        }));

        subscriptions.push(
            cx.subscribe(&bytes_per_row_input, |_, input: Entity<InputState>, event: &input::InputEvent, cx| {
                if let input::InputEvent::Change = event {
                    let value = input.read(cx).value().to_string();
                    if let Ok(bpr) = value.parse::<usize>()
                        && (MIN_BYTES_PER_ROW..=MAX_BYTES_PER_ROW).contains(&bpr)
                    {
                        cx.update_global::<BytesPerRow, _>(|bytes_per_row, _| {
                            bytes_per_row.0 = bpr;
                        });
                        crate::settings::save_current(cx);
                    }
                }
            }),
        );

        subscriptions.push(cx.subscribe(
            &structure_yaml_sha_threshold_input,
            |_, input: Entity<InputState>, event: &input::InputEvent, cx| {
                if let input::InputEvent::Change = event {
                    let value = input.read(cx).value().to_string();
                    if let Ok(threshold) = value.parse::<usize>()
                        && threshold <= crate::core::structure::MAX_STRUCTURE_YAML_SHA_THRESHOLD
                    {
                        cx.update_global::<StructureYamlShaThreshold, _>(|current, _| {
                            current.0 = threshold;
                        });
                        crate::settings::save_current(cx);
                    }
                }
            },
        ));

        subscriptions.push(cx.subscribe(&theme_select, |this, _, event: &SelectEvent<ThemeListDelegate>, cx| {
            if let SelectEvent::Confirm(Some(theme_name)) = event {
                this.theme_preview_state.borrow_mut().confirm();
                crate::theme::apply_theme_by_name(theme_name, None, cx);
                crate::settings::save_current(cx);
                cx.notify();
            }
        }));

        subscriptions.push(cx.subscribe(&theme_select, |this, _, _event: &DismissEvent, cx| {
            let origin = this.theme_preview_state.borrow_mut().cancel();
            if let Some(origin) = origin {
                crate::theme::apply_theme_by_name(&origin, None, cx);
                cx.dispatch_action(&UpdateSettingInput);
                cx.notify();
            }
        }));

        let preview_state = theme_preview_state.clone();
        let all_themes_for_sub = all_themes.clone();
        let sub = cx.observe_keystrokes(move |_this, event, window, cx| {
            let is_list = event.context_stack.iter().any(|c| c.contains("List"));
            if is_list && !event.keystroke.modifiers.modified() {
                let is_nav = event.keystroke.key == "down"
                    || event.keystroke.key == "up"
                    || event
                        .action
                        .as_ref()
                        .map(|a| a.name() == "SelectDown" || a.name() == "SelectUp")
                        .unwrap_or(false);

                if is_nav
                    && let Some(selected_ix) = _this.theme_select.read(cx).selected_index(cx)
                    && let Some(theme_name) = all_themes_for_sub.get(selected_ix.row)
                {
                    let mut state = preview_state.borrow_mut();
                    state.start_preview(cx.theme().theme_name().clone());
                    state.set_hover_index(selected_ix.row);
                    if cx.theme().theme_name() != theme_name.as_ref() {
                        crate::theme::apply_theme_by_name(theme_name, Some(window), cx);
                    }
                }
            }
        });
        subscriptions.push(sub);

        Self {
            focus_handle,
            theme_select,
            font_family_input,
            font_size_input,
            bytes_per_row_input,
            structure_yaml_sha_threshold_input,
            theme_preview_state,
            _subscriptions: subscriptions,
        }
    }

    fn on_action_update_setting_input(&mut self, _action: &UpdateSettingInput, window: &mut Window, cx: &mut Context<Self>) {
        let appearance = cx.global::<Appearance>();
        let family = appearance.font_family.clone();
        let size_str = appearance.font_size.to_string();
        let bytes_per_row_str = cx.global::<BytesPerRow>().0.to_string();
        let yaml_sha_threshold_str = cx.global::<StructureYamlShaThreshold>().0.to_string();

        let active_theme = cx.theme().theme_name().clone();
        self.theme_select.update(cx, |select, cx| {
            if select.selected_value() != Some(&active_theme) {
                select.set_selected_value(&active_theme, window, cx);
            }
        });

        self.font_family_input.update(cx, |input, cx| {
            if input.value() != family.as_str() {
                input.set_value(family, window, cx);
            }
        });
        self.font_size_input.update(cx, |input, cx| {
            if input.value() != size_str.as_str() {
                input.set_value(size_str, window, cx);
            }
        });
        self.bytes_per_row_input.update(cx, |input, cx| {
            if input.value() != bytes_per_row_str.as_str() {
                input.set_value(bytes_per_row_str, window, cx);
            }
        });
        self.structure_yaml_sha_threshold_input.update(cx, |input, cx| {
            if input.value() != yaml_sha_threshold_str.as_str() {
                input.set_value(yaml_sha_threshold_str, window, cx);
            }
        });
    }
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let label_width = px(312.0);
        let sub_label_width = px(280.0);
        let row_gap = px(20.0);
        let control_width = px(200.0);

        if !self.theme_preview_state.borrow().is_open() {
            let active_theme = cx.theme().theme_name().clone();
            self.theme_select.update(cx, |select, cx| {
                if select.selected_value() != Some(&active_theme) {
                    select.set_selected_value(&active_theme, window, cx);
                }
            });
        }

        let container = div().p_4().flex().flex_col().gap_6();

        container
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_update_setting_input))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_xs().font_semibold().text_color(cx.theme().muted_foreground).child("Appearance"))
                    .child({
                        let active_theme_name = cx.theme().theme_name().clone();
                        let all_themes = crate::theme::all_theme_names(cx);

                        let preview_state = self.theme_preview_state.clone();
                        let saved_theme_name = preview_state.borrow().origin().cloned().unwrap_or_else(|| active_theme_name.clone());
                        let is_modified = saved_theme_name != crate::settings::DEFAULT_LIGHT_THEME;

                        div()
                            .flex()
                            .items_center()
                            .gap(row_gap)
                            .child(div().w(label_width).flex_shrink_0().whitespace_nowrap().child("Theme"))
                            .child(
                                div()
                                    .w(control_width)
                                    .capture_key_down(cx.listener({
                                        let all_themes = all_themes.clone();
                                        move |this, event: &KeyDownEvent, window, cx| {
                                            let is_popup_open = this.theme_preview_state.borrow().is_open();
                                            if is_popup_open || event.keystroke.modifiers.modified() {
                                                return;
                                            }
                                            let step_down = if event.keystroke.key == "down" {
                                                Some(true)
                                            } else if event.keystroke.key == "up" {
                                                Some(false)
                                            } else {
                                                None
                                            };

                                            if let Some(step_down) = step_down {
                                                window.prevent_default();
                                                cx.stop_propagation();

                                                let current_name = cx.theme().theme_name().clone();
                                                let current_idx = all_themes.iter().position(|t| t == &current_name).unwrap_or(0);
                                                let next_idx = cycle_theme_index(current_idx, step_down, all_themes.len());

                                                if let Some(next_theme) = all_themes.get(next_idx) {
                                                    let next_theme = next_theme.clone();
                                                    this.theme_select.update(cx, |select, cx| {
                                                        select.set_selected_index(Some(IndexPath::new(next_idx)), window, cx);
                                                    });
                                                    crate::theme::apply_theme_by_name(&next_theme, Some(window), cx);
                                                    crate::settings::save_current(cx);
                                                    this.theme_preview_state.borrow_mut().confirm();
                                                    cx.notify();
                                                }
                                            }
                                        }
                                    }))
                                    .child(Select::new(&self.theme_select).w_full().with_size(Size::Small)),
                            )
                            .when(is_modified, |row| {
                                row.child(
                                    Button::new("reset-theme")
                                        .icon(IconName::Undo2)
                                        .ghost()
                                        .with_size(Size::Small)
                                        .tooltip("Reset to default")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            let default_theme = SharedString::from(crate::settings::DEFAULT_LIGHT_THEME);
                                            this.theme_select.update(cx, |select, cx| {
                                                select.set_selected_value(&default_theme, window, cx);
                                                select.focus(window, cx);
                                            });
                                            crate::theme::apply_theme_by_name(crate::settings::DEFAULT_LIGHT_THEME, Some(window), cx);
                                            crate::settings::save_current(cx);
                                            this.theme_preview_state.borrow_mut().confirm();
                                            window.push_notification(Notification::info("Reset Theme to default"), cx);
                                        })),
                                )
                            })
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(div().text_xs().font_semibold().text_color(cx.theme().muted_foreground).child("Editor"))
                    .child({
                        let default_family = Appearance::default().font_family;
                        let is_modified = cx.global::<Appearance>().font_family != default_family;

                        div()
                            .flex()
                            .items_center()
                            .gap(row_gap)
                            .child(div().w(label_width).flex_shrink_0().whitespace_nowrap().child("Font Family"))
                            .child(div().w(control_width).child(Input::new(&self.font_family_input)))
                            .when(is_modified, |row| {
                                row.child(
                                    Button::new("reset-font-family")
                                        .icon(IconName::Undo2)
                                        .ghost()
                                        .with_size(Size::Small)
                                        .tooltip("Reset to default")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            let default_family = Appearance::default().font_family;
                                            cx.update_global::<Appearance, _>(|appearance, _| {
                                                appearance.font_family = default_family.clone();
                                            });
                                            this.font_family_input.update(cx, |input, cx| {
                                                input.set_value(default_family, window, cx);
                                            });
                                            crate::settings::save_current(cx);
                                            window.push_notification(Notification::info("Reset Font Family to default"), cx);
                                        })),
                                )
                            })
                    })
                    .child({
                        let default_size = Appearance::default().font_size;
                        let is_modified = (cx.global::<Appearance>().font_size - default_size).abs() > f32::EPSILON;

                        div()
                            .flex()
                            .items_center()
                            .gap(row_gap)
                            .child(div().w(label_width).flex_shrink_0().whitespace_nowrap().child("Font Size"))
                            .child(div().w(control_width).child(NumberInput::new(&self.font_size_input)))
                            .when(is_modified, |row| {
                                row.child(
                                    Button::new("reset-font-size")
                                        .icon(IconName::Undo2)
                                        .ghost()
                                        .with_size(Size::Small)
                                        .tooltip("Reset to default")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            let default_size = Appearance::default().font_size;
                                            cx.update_global::<Appearance, _>(|appearance, _| {
                                                appearance.font_size = default_size;
                                            });
                                            this.font_size_input.update(cx, |input, cx| {
                                                input.set_value(default_size.to_string(), window, cx);
                                            });
                                            crate::settings::save_current(cx);
                                            window.push_notification(Notification::info("Reset Font Size to default"), cx);
                                        })),
                                )
                            })
                    })
                    .child({
                        let default_bytes = DEFAULT_BYTES_PER_ROW;
                        let is_modified = cx.global::<BytesPerRow>().0 != default_bytes;

                        div()
                            .flex()
                            .items_center()
                            .gap(row_gap)
                            .child(div().w(label_width).flex_shrink_0().whitespace_nowrap().child("Bytes Per Row"))
                            .child(div().w(control_width).child(NumberInput::new(&self.bytes_per_row_input)))
                            .when(is_modified, |row| {
                                row.child(
                                    Button::new("reset-bytes-per-row")
                                        .icon(IconName::Undo2)
                                        .ghost()
                                        .with_size(Size::Small)
                                        .tooltip("Reset to default")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            cx.update_global::<BytesPerRow, _>(|bytes_per_row, _| {
                                                bytes_per_row.0 = DEFAULT_BYTES_PER_ROW;
                                            });
                                            this.bytes_per_row_input.update(cx, |input, cx| {
                                                input.set_value(DEFAULT_BYTES_PER_ROW.to_string(), window, cx);
                                            });
                                            crate::settings::save_current(cx);
                                            window.push_notification(Notification::info("Reset Bytes Per Row to default"), cx);
                                        })),
                                )
                            })
                    })
                    .child({
                        let default_encoding = *cx.global::<Encoding>();
                        let is_modified = default_encoding != Encoding::default();

                        div()
                            .flex()
                            .items_center()
                            .gap(row_gap)
                            .child(div().w(label_width).flex_shrink_0().whitespace_nowrap().child("Default Encoding"))
                            .child(
                                div().w(control_width).child(
                                    Button::new("default-encoding")
                                        .label(default_encoding.label())
                                        .outline()
                                        .dropdown_caret(true)
                                        .with_size(Size::Small)
                                        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| {
                                            let menu = Encoding::primary_encodings().iter().copied().fold(menu, |menu, encoding| {
                                                menu.item(PopupMenuItem::new(encoding.label()).checked(encoding == default_encoding).on_click(
                                                    move |_, _, cx| {
                                                        cx.update_global::<Encoding, _>(|current, _| {
                                                            *current = encoding;
                                                        });
                                                        crate::settings::save_current(cx);
                                                    },
                                                ))
                                            });
                                            let menu = menu.separator();
                                            Encoding::secondary_categories().iter().fold(menu, |menu, (cat, encs)| {
                                                menu.submenu(cat.label(), window, cx, move |menu, _window, _cx| {
                                                    encs.iter().copied().fold(menu, |menu, encoding| {
                                                        menu.item(PopupMenuItem::new(encoding.label()).checked(encoding == default_encoding).on_click(
                                                            move |_, _, cx| {
                                                                cx.update_global::<Encoding, _>(|current, _| {
                                                                    *current = encoding;
                                                                });
                                                                crate::settings::save_current(cx);
                                                            },
                                                        ))
                                                    })
                                                })
                                            })
                                        }),
                                ),
                            )
                            .when(is_modified, |row| {
                                row.child(
                                    Button::new("reset-encoding")
                                        .icon(IconName::Undo2)
                                        .ghost()
                                        .with_size(Size::Small)
                                        .tooltip("Reset to default")
                                        .on_click(cx.listener(|_, _, window, cx| {
                                            cx.update_global::<Encoding, _>(|current, _| {
                                                *current = Encoding::default();
                                            });
                                            crate::settings::save_current(cx);
                                            window.push_notification(Notification::info("Reset Default Encoding to default"), cx);
                                        })),
                                )
                            })
                    })
                    .child({
                        let default_radix = *cx.global::<DisplayRadix>();
                        let is_modified = default_radix != DisplayRadix::default();

                        div()
                            .flex()
                            .items_center()
                            .gap(row_gap)
                            .child(div().w(label_width).flex_shrink_0().whitespace_nowrap().child("Default Radix"))
                            .child(
                                div().w(control_width).child(
                                    Button::new("default-radix")
                                        .label(default_radix.label())
                                        .outline()
                                        .dropdown_caret(true)
                                        .with_size(Size::Small)
                                        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                                            DisplayRadix::ALL.iter().copied().fold(menu, |menu, radix| {
                                                menu.item(PopupMenuItem::new(radix.label()).checked(radix == default_radix).on_click(move |_, _, cx| {
                                                    cx.update_global::<DisplayRadix, _>(|current, _| {
                                                        *current = radix;
                                                    });
                                                    crate::settings::save_current(cx);
                                                }))
                                            })
                                        }),
                                ),
                            )
                            .when(is_modified, |row| {
                                row.child(
                                    Button::new("reset-radix")
                                        .icon(IconName::Undo2)
                                        .ghost()
                                        .with_size(Size::Small)
                                        .tooltip("Reset to default")
                                        .on_click(cx.listener(|_, _, window, cx| {
                                            cx.update_global::<DisplayRadix, _>(|current, _| {
                                                *current = DisplayRadix::default();
                                            });
                                            crate::settings::save_current(cx);
                                            window.push_notification(Notification::info("Reset Default Radix to default"), cx);
                                        })),
                                )
                            })
                    })
                    .child({
                        let default_group_size = *cx.global::<ByteGroupSize>();
                        let is_modified = default_group_size != ByteGroupSize::default();

                        div()
                            .flex()
                            .items_center()
                            .gap(row_gap)
                            .child(div().w(label_width).flex_shrink_0().whitespace_nowrap().child("Default Grouping"))
                            .child(
                                div().w(control_width).child(
                                    Button::new("default-grouping")
                                        .label(default_group_size.label())
                                        .outline()
                                        .dropdown_caret(true)
                                        .with_size(Size::Small)
                                        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                                            ByteGroupSize::ALL.iter().copied().fold(menu, |menu, group_size| {
                                                menu.item(PopupMenuItem::new(group_size.label()).checked(group_size == default_group_size).on_click(
                                                    move |_, _, cx| {
                                                        cx.update_global::<ByteGroupSize, _>(|current, _| {
                                                            *current = group_size;
                                                        });
                                                        crate::settings::save_current(cx);
                                                    },
                                                ))
                                            })
                                        }),
                                ),
                            )
                            .when(is_modified, |row| {
                                row.child(
                                    Button::new("reset-grouping")
                                        .icon(IconName::Undo2)
                                        .ghost()
                                        .with_size(Size::Small)
                                        .tooltip("Reset to default")
                                        .on_click(cx.listener(|_, _, window, cx| {
                                            cx.update_global::<ByteGroupSize, _>(|current, _| {
                                                *current = ByteGroupSize::default();
                                            });
                                            crate::settings::save_current(cx);
                                            window.push_notification(Notification::info("Reset Default Grouping to default"), cx);
                                        })),
                                )
                            })
                    })
                    .child({
                        let default_byte_order = *cx.global::<ByteOrder>();
                        let is_modified = default_byte_order != ByteOrder::default();

                        div()
                            .flex()
                            .items_center()
                            .gap(row_gap)
                            .child(div().w(label_width).flex_shrink_0().whitespace_nowrap().child("Default Byte Order"))
                            .child(
                                div().w(control_width).child(
                                    Button::new("default-byte-order")
                                        .label(default_byte_order.label())
                                        .outline()
                                        .dropdown_caret(true)
                                        .with_size(Size::Small)
                                        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                                            ByteOrder::ALL.iter().copied().fold(menu, |menu, byte_order| {
                                                menu.item(PopupMenuItem::new(byte_order.label()).checked(byte_order == default_byte_order).on_click(
                                                    move |_, _, cx| {
                                                        cx.update_global::<ByteOrder, _>(|current, _| {
                                                            *current = byte_order;
                                                        });
                                                        crate::settings::save_current(cx);
                                                    },
                                                ))
                                            })
                                        }),
                                ),
                            )
                            .when(is_modified, |row| {
                                row.child(
                                    Button::new("reset-byte-order")
                                        .icon(IconName::Undo2)
                                        .ghost()
                                        .with_size(Size::Small)
                                        .tooltip("Reset to default")
                                        .on_click(cx.listener(|_, _, window, cx| {
                                            cx.update_global::<ByteOrder, _>(|current, _| {
                                                *current = ByteOrder::default();
                                            });
                                            crate::settings::save_current(cx);
                                            window.push_notification(Notification::info("Reset Default Byte Order to default"), cx);
                                        })),
                                )
                            })
                    })
                    .child({
                        let byte_coloring = *cx.global::<ByteColoring>();
                        let category_colors = *cx.global::<ByteCategoryColors>();
                        let default_colors = ByteCategoryColors::default();
                        let is_modified = byte_coloring != ByteColoring::default() || category_colors != default_colors;
                        let is_custom = byte_coloring == ByteColoring::Custom;
                        let status_text = if is_custom {
                            SharedString::from("Custom overrides")
                        } else {
                            SharedString::from(format!("Preset: {}", byte_coloring.label()))
                        };
                        let border_accent = if is_custom { cx.theme().accent } else { cx.theme().border };

                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(row_gap)
                                    .child(div().w(label_width).flex_shrink_0().whitespace_nowrap().child("Byte Coloring"))
                                    .child(
                                        div().w(control_width).child(
                                            Button::new("byte-coloring")
                                                .label(byte_coloring.label())
                                                .outline()
                                                .dropdown_caret(true)
                                                .with_size(Size::Small)
                                                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                                                    let menu = ByteColoring::PRESETS.iter().copied().fold(menu, |menu, coloring| {
                                                        menu.item(PopupMenuItem::new(coloring.label()).checked(coloring == byte_coloring).on_click(
                                                            move |_, _, cx| {
                                                                cx.update_global::<ByteColoring, _>(|current, _| {
                                                                    *current = coloring;
                                                                });
                                                                if let Some(colors) = coloring.colors() {
                                                                    cx.update_global::<ByteCategoryColors, _>(|current, _| {
                                                                        *current = colors;
                                                                    });
                                                                }
                                                                crate::settings::save_current(cx);
                                                            },
                                                        ))
                                                    });
                                                    if is_custom {
                                                        menu.separator().item(PopupMenuItem::new(ByteColoring::Custom.label()).checked(true))
                                                    } else {
                                                        menu
                                                    }
                                                }),
                                        ),
                                    )
                                    .when(is_modified, |row| {
                                        row.child(
                                            Button::new("reset-byte-coloring")
                                                .icon(IconName::Undo2)
                                                .ghost()
                                                .with_size(Size::Small)
                                                .tooltip("Reset Byte Coloring & Category Colors to default")
                                                .on_click(cx.listener(|_, _, window, cx| {
                                                    cx.update_global::<ByteColoring, _>(|current, _| {
                                                        *current = ByteColoring::default();
                                                    });
                                                    cx.update_global::<ByteCategoryColors, _>(|current, _| {
                                                        current.reset_all();
                                                    });
                                                    crate::settings::save_current(cx);
                                                    window.push_notification(Notification::info("Reset Byte Coloring to default"), cx);
                                                })),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .ml_4()
                                    .pl_4()
                                    .pr_4()
                                    .py_3()
                                    .border_l_2()
                                    .border_color(border_accent)
                                    .bg(cx.theme().secondary)
                                    .rounded_md()
                                    .flex()
                                    .flex_col()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .justify_between()
                                            .pb_1()
                                            .child(div().text_xs().font_semibold().text_color(cx.theme().muted_foreground).child("Category Colors"))
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(if is_custom {
                                                        cx.theme().accent_foreground
                                                    } else {
                                                        cx.theme().muted_foreground
                                                    })
                                                    .child(status_text),
                                            ),
                                    )
                                    .children(ByteCategory::ALL.iter().copied().map(|cat| {
                                        let color = category_colors.color_for(Some(cat));
                                        let default_cat_color = default_colors.color_for(Some(cat));
                                        let is_cat_modified = color != default_cat_color;
                                        let swatch_color = resolve_category_color(color, cx.theme());

                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(row_gap)
                                            .child(
                                                div()
                                                    .w(sub_label_width)
                                                    .flex_shrink_0()
                                                    .flex()
                                                    .items_center()
                                                    .gap_2()
                                                    .child(
                                                        div()
                                                            .w(px(10.0))
                                                            .h(px(10.0))
                                                            .rounded_full()
                                                            .flex_shrink_0()
                                                            .bg(swatch_color)
                                                            .border_1()
                                                            .border_color(cx.theme().border),
                                                    )
                                                    .child(div().text_xs().whitespace_nowrap().child(cat.label())),
                                            )
                                            .child(
                                                div().w(control_width).child(
                                                    Button::new(format!("cat-color-{}", cat.id()))
                                                        .label(color.label())
                                                        .outline()
                                                        .dropdown_caret(true)
                                                        .with_size(Size::Small)
                                                        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                                                            CategoryColor::ALL.iter().copied().fold(menu, |menu, choice| {
                                                                menu.item(PopupMenuItem::new(choice.label()).checked(choice == color).on_click(
                                                                    move |_, _, cx| {
                                                                        let mut next_colors = *cx.global::<ByteCategoryColors>();
                                                                        next_colors.set(cat, choice);
                                                                        let next_coloring = ByteColoring::from_colors(next_colors);
                                                                        cx.update_global::<ByteCategoryColors, _>(|current, _| {
                                                                            *current = next_colors;
                                                                        });
                                                                        cx.update_global::<ByteColoring, _>(|current, _| {
                                                                            *current = next_coloring;
                                                                        });
                                                                        crate::settings::save_current(cx);
                                                                    },
                                                                ))
                                                            })
                                                        }),
                                                ),
                                            )
                                            .when(is_cat_modified, |row| {
                                                row.child(
                                                    Button::new(format!("reset-cat-color-{}", cat.id()))
                                                        .icon(IconName::Undo2)
                                                        .ghost()
                                                        .with_size(Size::Small)
                                                        .tooltip("Reset category to default")
                                                        .on_click(cx.listener(move |_, _, _, cx| {
                                                            let mut next_colors = *cx.global::<ByteCategoryColors>();
                                                            next_colors.set(cat, default_cat_color);
                                                            let next_coloring = ByteColoring::from_colors(next_colors);
                                                            cx.update_global::<ByteCategoryColors, _>(|current, _| {
                                                                *current = next_colors;
                                                            });
                                                            cx.update_global::<ByteColoring, _>(|current, _| {
                                                                *current = next_coloring;
                                                            });
                                                            crate::settings::save_current(cx);
                                                        })),
                                                )
                                            })
                                    })),
                            )
                    }),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .font_semibold()
                            .text_color(cx.theme().muted_foreground)
                            .child("Structure Analysis"),
                    )
                    .child({
                        let default_threshold = crate::settings::DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD;
                        let is_modified = cx.global::<StructureYamlShaThreshold>().0 != default_threshold;

                        div()
                            .flex()
                            .items_center()
                            .gap(row_gap)
                            .child(div().w(label_width).flex_shrink_0().whitespace_nowrap().child("YAML SHA Threshold"))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(div().w(control_width).child(NumberInput::new(&self.structure_yaml_sha_threshold_input)))
                                    .child(div().text_xs().text_color(cx.theme().muted_foreground).child("bytes")),
                            )
                            .when(is_modified, |row| {
                                row.child(
                                    Button::new("reset-yaml-sha-threshold")
                                        .icon(IconName::Undo2)
                                        .ghost()
                                        .with_size(Size::Small)
                                        .tooltip("Reset to default (32 bytes)")
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            cx.update_global::<StructureYamlShaThreshold, _>(|current, _| {
                                                current.0 = crate::settings::DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD;
                                            });
                                            this.structure_yaml_sha_threshold_input.update(cx, |input, cx| {
                                                input.set_value(crate::settings::DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD.to_string(), window, cx);
                                            });
                                            crate::settings::save_current(cx);
                                            window.push_notification(Notification::info("Reset YAML SHA Threshold to default"), cx);
                                        })),
                                )
                            })
                    })
                    .child({
                        let include_offsets = cx.global::<StructureYamlIncludeOffsets>().0;
                        let default_include_offsets = crate::settings::DEFAULT_STRUCTURE_YAML_INCLUDE_OFFSETS;
                        let is_modified = include_offsets != default_include_offsets;

                        div()
                            .flex()
                            .items_center()
                            .gap(row_gap)
                            .child(div().w(label_width).flex_shrink_0().whitespace_nowrap().child("YAML Include Offsets"))
                            .child(
                                div().w(control_width).child(
                                    Button::new("yaml-include-offsets")
                                        .label(if include_offsets { "Enabled" } else { "Disabled" })
                                        .outline()
                                        .dropdown_caret(true)
                                        .with_size(Size::Small)
                                        .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                                            menu.item(PopupMenuItem::new("Enabled").checked(include_offsets).on_click(move |_, _, cx| {
                                                cx.update_global::<StructureYamlIncludeOffsets, _>(|current, _| {
                                                    current.0 = true;
                                                });
                                                crate::settings::save_current(cx);
                                            }))
                                            .item(
                                                PopupMenuItem::new("Disabled").checked(!include_offsets).on_click(move |_, _, cx| {
                                                    cx.update_global::<StructureYamlIncludeOffsets, _>(|current, _| {
                                                        current.0 = false;
                                                    });
                                                    crate::settings::save_current(cx);
                                                }),
                                            )
                                        }),
                                ),
                            )
                            .when(is_modified, |row| {
                                row.child(
                                    Button::new("reset-yaml-include-offsets")
                                        .icon(IconName::Undo2)
                                        .ghost()
                                        .with_size(Size::Small)
                                        .tooltip("Reset to default (Enabled)")
                                        .on_click(cx.listener(|_, _, window, cx| {
                                            cx.update_global::<StructureYamlIncludeOffsets, _>(|current, _| {
                                                current.0 = crate::settings::DEFAULT_STRUCTURE_YAML_INCLUDE_OFFSETS;
                                            });
                                            crate::settings::save_current(cx);
                                            window.push_notification(Notification::info("Reset YAML Include Offsets to default"), cx);
                                        })),
                                )
                            })
                    }),
            )
    }
}

impl EventEmitter<PanelEvent> for SettingsView {}
impl Focusable for SettingsView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}
impl Panel for SettingsView {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        "Settings"
    }
    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some("Settings".into())
    }
    fn zoom_control(&self, _: &App) -> Option<gpui_kit::component::dock::PanelControl> {
        None
    }
}

impl gpui_kit::base::dock::Panel for SettingsView {
    fn panel_name(&self) -> &'static str {
        "SettingsView"
    }
    fn closable(&self, _: &App) -> bool {
        true
    }
    fn zoomable(&self, _: &App) -> bool {
        false
    }
    fn visible(&self, _: &App) -> bool {
        true
    }
    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if active {
            self.focus_handle.focus(window, cx);
        }
    }
    fn set_zoomed(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) {}
}

impl crate::ui::pane::WorkspaceTab for Entity<SettingsView> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn title(&self, _cx: &App) -> String {
        "Settings".to_string()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn render(&self) -> AnyElement {
        self.clone().into_any_element()
    }

    fn create_split(&self, window: &mut Window, cx: &mut App) -> Option<crate::ui::pane::TabContent> {
        let new_settings = cx.new(|cx| SettingsView::new(window, cx));
        Some(crate::ui::pane::TabContent::new(new_settings))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_theme_preview_state_start_and_cancel_reverts_to_origin() {
        let mut state = ThemePreviewState::default();
        assert_eq!(state.origin(), None);

        state.start_preview(SharedString::from("Default Light"));
        assert_eq!(state.origin(), Some(&SharedString::from("Default Light")));

        let restored = state.cancel();
        assert_eq!(restored, Some(SharedString::from("Default Light")));
        assert_eq!(state.origin(), None);

        assert_eq!(state.cancel(), None);
    }

    #[test]
    fn test_theme_preview_state_multiple_start_preview_preserves_initial_origin() {
        let mut state = ThemePreviewState::default();
        state.start_preview(SharedString::from("Default Light"));
        state.start_preview(SharedString::from("Ayu Dark"));
        state.start_preview(SharedString::from("Solarized Light"));

        assert_eq!(state.origin(), Some(&SharedString::from("Default Light")));

        let restored = state.cancel();
        assert_eq!(restored, Some(SharedString::from("Default Light")));
    }

    #[test]
    fn test_theme_preview_state_confirm_prevents_revert_on_cancel() {
        let mut state = ThemePreviewState::default();
        state.start_preview(SharedString::from("Default Light"));

        state.confirm();
        assert_eq!(state.origin(), None);

        assert_eq!(state.cancel(), None);
    }

    #[test]
    fn test_theme_preview_state_step_index_navigation() {
        let mut state = ThemePreviewState::default();
        assert_eq!(state.current_index(), None);

        // Zero items returns None
        assert_eq!(state.step_index(true, 0), None);
        assert_eq!(state.step_index(false, 0), None);

        // Step down from None starts at index 0
        assert_eq!(state.step_index(true, 3), Some(0));
        assert_eq!(state.current_index(), Some(0));

        // Step down advances index
        assert_eq!(state.step_index(true, 3), Some(1));
        assert_eq!(state.step_index(true, 3), Some(2));

        // Step down at the end wraps around to 0
        assert_eq!(state.step_index(true, 3), Some(0));

        // Step up at 0 wraps around to the last index
        assert_eq!(state.step_index(false, 3), Some(2));
        assert_eq!(state.step_index(false, 3), Some(1));
        assert_eq!(state.step_index(false, 3), Some(0));

        // Step up from None starts at the last item
        let mut state_up = ThemePreviewState::default();
        assert_eq!(state_up.step_index(false, 3), Some(2));
        assert_eq!(state_up.current_index(), Some(2));
    }

    #[test]
    fn test_theme_preview_state_hover_sync_and_reset() {
        let mut state = ThemePreviewState::default();
        state.start_preview(SharedString::from("Default Light"));

        // Setting hover index updates current_index
        state.set_hover_index(2);
        assert_eq!(state.current_index(), Some(2));

        // Stepping continues from the hovered index
        assert_eq!(state.step_index(true, 5), Some(3));
        assert_eq!(state.step_index(false, 5), Some(2));

        // Cancel resets current_index
        state.cancel();
        assert_eq!(state.current_index(), None);

        // Confirm also resets current_index
        state.start_preview(SharedString::from("Default Light"));
        state.set_hover_index(1);
        state.confirm();
        assert_eq!(state.current_index(), None);
    }

    #[test]
    fn test_theme_preview_state_scoped_borrow_allows_reborrow() {
        let cell = std::cell::RefCell::new(ThemePreviewState::default());
        let step_result = {
            let mut state = cell.borrow_mut();
            state.start_preview(SharedString::from("Default Light"));
            state.step_index(true, 5)
        };
        assert_eq!(step_result, Some(0));

        // Reborrowing succeeds because the previous borrow was scoped and dropped
        cell.borrow_mut().confirm();
        assert_eq!(cell.borrow().origin(), None);
    }

    #[test]
    fn test_cycle_theme_index() {
        assert_eq!(cycle_theme_index(0, true, 0), 0);
        assert_eq!(cycle_theme_index(0, false, 0), 0);

        // Single item
        assert_eq!(cycle_theme_index(0, true, 1), 0);
        assert_eq!(cycle_theme_index(0, false, 1), 0);

        // Three items - step down
        assert_eq!(cycle_theme_index(0, true, 3), 1);
        assert_eq!(cycle_theme_index(1, true, 3), 2);
        assert_eq!(cycle_theme_index(2, true, 3), 0); // wrap to start

        // Three items - step up
        assert_eq!(cycle_theme_index(0, false, 3), 2); // wrap to end
        assert_eq!(cycle_theme_index(2, false, 3), 1);
        assert_eq!(cycle_theme_index(1, false, 3), 0);
    }

    #[test]
    fn test_theme_preview_state_open_tracking() {
        let mut state = ThemePreviewState::default();
        assert!(!state.is_open());

        state.set_open(true);
        assert!(state.is_open());

        state.confirm();
        assert!(!state.is_open());

        state.set_open(true);
        assert!(state.is_open());

        state.cancel();
        assert!(!state.is_open());
    }

    #[test]
    fn test_theme_list_delegate() {
        let themes = vec![SharedString::from("Light"), SharedString::from("Dark")];
        let state = Rc::new(RefCell::new(ThemePreviewState::default()));
        let delegate = ThemeListDelegate::new(themes, state);

        assert_eq!(delegate.items_count(0), 2);
        assert_eq!(delegate.item(IndexPath::new(0)), Some(&SharedString::from("Light")));
        assert_eq!(delegate.item(IndexPath::new(1)), Some(&SharedString::from("Dark")));
        assert_eq!(delegate.item(IndexPath::new(2)), None);

        assert_eq!(delegate.position(&SharedString::from("Light")), Some(IndexPath::new(0)));
        assert_eq!(delegate.position(&SharedString::from("Dark")), Some(IndexPath::new(1)));
        assert_eq!(delegate.position(&SharedString::from("Unknown")), None);
    }

    #[test]
    fn test_theme_list_delegate_is_item_checked() {
        let themes = vec![SharedString::from("Light"), SharedString::from("Dark")];
        let state = Rc::new(RefCell::new(ThemePreviewState::default()));
        let delegate = ThemeListDelegate::new(themes, state.clone());

        let selection = vec![(IndexPath::new(0), SharedString::from("Light"))];
        assert!(delegate.check_item(&SharedString::from("Light"), &selection));
        assert!(!delegate.check_item(&SharedString::from("Dark"), &selection));

        state.borrow_mut().start_preview(SharedString::from("Dark"));
        assert!(!delegate.check_item(&SharedString::from("Light"), &selection));
        assert!(delegate.check_item(&SharedString::from("Dark"), &selection));
    }
}

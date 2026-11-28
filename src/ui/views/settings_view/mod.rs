mod theme_selector;

#[cfg(test)]
mod tests;

use crate::core::appearance::{Appearance, MAX_FONT_SIZE, MIN_FONT_SIZE};
use crate::core::editor::OverwriteDeleteBehavior;
use crate::core::encoding::Encoding;
use crate::core::layout::{BytesPerRow, DEFAULT_BYTES_PER_ROW, MAX_BYTES_PER_ROW, MIN_BYTES_PER_ROW};
use crate::core::radix::{ByteCategoryColors, ByteColoring, ByteGroupSize, ByteOrder, CategoryColor, DisplayRadix, HexByteCategory, HexCase};
use crate::core::structure::{MAX_STRUCTURE_YAML_SHA_THRESHOLD, StructureYamlIncludeOffsets, StructureYamlShaThreshold};
use crate::ui::icon::IconName;
use crate::ui::views::hex_view::resolve_category_color;
use gpui_kit::component::{
    ActiveTheme, Disableable as _, Icon, IndexPath, Sizable as _, Size, StyledExt, WindowExt as _,
    button::{Button, ButtonVariants as _},
    dialog::DialogFooter,
    dock::{Panel, PanelEvent},
    input::{self, Input, InputState, NumberInput},
    menu::{DropdownMenu as _, PopupMenuItem},
    notification::Notification,
    select::{Select, SelectEvent, SelectState},
    theme::Theme,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    Action, Anchor, AnyElement, App, Context, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, IntoElement, KeyDownEvent, ParentElement, Render,
    SharedString, Subscription, Window, div, px,
};

use std::cell::RefCell;
use std::rc::Rc;

use theme_selector::{ThemeListDelegate, ThemePreviewState, cycle_theme_index};

const CONTROL_WIDTH: f32 = 220.0;
const RESET_SLOT_WIDTH: f32 = 28.0;

#[derive(Clone, PartialEq, Action)]
pub struct UpdateSettingInput;

pub(crate) fn validate_font_family(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err("Font family cannot be empty".to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

pub(crate) fn validate_font_size(value: &str) -> Result<f32, String> {
    let parsed = value
        .trim()
        .parse::<f32>()
        .map_err(|_| format!("Enter a number ({}–{})", MIN_FONT_SIZE, MAX_FONT_SIZE))?;
    if !parsed.is_finite() || !(MIN_FONT_SIZE..=MAX_FONT_SIZE).contains(&parsed) {
        Err(format!("Must be between {} and {}", MIN_FONT_SIZE, MAX_FONT_SIZE))
    } else {
        Ok(parsed)
    }
}

pub(crate) fn validate_bytes_per_row(value: &str) -> Result<usize, String> {
    let parsed = value
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("Enter an integer ({}–{})", MIN_BYTES_PER_ROW, MAX_BYTES_PER_ROW))?;
    if !(MIN_BYTES_PER_ROW..=MAX_BYTES_PER_ROW).contains(&parsed) {
        Err(format!("Must be between {} and {}", MIN_BYTES_PER_ROW, MAX_BYTES_PER_ROW))
    } else {
        Ok(parsed)
    }
}

pub(crate) fn validate_yaml_sha_threshold(value: &str) -> Result<usize, String> {
    let parsed = value
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("Enter an integer (0–{})", MAX_STRUCTURE_YAML_SHA_THRESHOLD))?;
    if parsed > MAX_STRUCTURE_YAML_SHA_THRESHOLD {
        Err(format!("Must be at most {}", MAX_STRUCTURE_YAML_SHA_THRESHOLD))
    } else {
        Ok(parsed)
    }
}

pub(crate) fn matches_settings_filter(query: &str, section: &str, label: &str, description: &str) -> bool {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return true;
    }
    let haystack = format!("{section} {label} {description}").to_ascii_lowercase();
    trimmed.split_whitespace().all(|term| haystack.contains(&term.to_ascii_lowercase()))
}

pub struct SettingsView {
    focus_handle: FocusHandle,
    filter_input: Entity<InputState>,
    theme_select: Entity<SelectState<ThemeListDelegate>>,
    font_family_input: Entity<InputState>,
    font_size_input: Entity<InputState>,
    bytes_per_row_input: Entity<InputState>,
    structure_yaml_sha_threshold_input: Entity<InputState>,
    font_family_error: Option<String>,
    font_size_error: Option<String>,
    bytes_per_row_error: Option<String>,
    yaml_sha_threshold_error: Option<String>,
    theme_preview_state: Rc<RefCell<ThemePreviewState>>,
    _subscriptions: Vec<Subscription>,
}

impl SettingsView {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let theme_preview_state = Rc::new(RefCell::new(ThemePreviewState::default()));

        let filter_input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter settings..."));

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
        let structure_yaml_sha_threshold_input = cx.new(|cx| InputState::new(window, cx).step(1.0).min(0.0).max(MAX_STRUCTURE_YAML_SHA_THRESHOLD as f64));

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

        subscriptions.push(cx.subscribe(&filter_input, |_, _, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                cx.notify();
            }
        }));

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

        subscriptions.push(cx.observe_global::<HexCase>(|_, cx| {
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

        subscriptions.push(cx.observe_global::<OverwriteDeleteBehavior>(|_, cx| {
            cx.notify();
        }));

        subscriptions.push(
            cx.subscribe(&font_family_input, |this, input: Entity<InputState>, event: &input::InputEvent, cx| {
                if let input::InputEvent::Change = event {
                    let value = input.read(cx).value().to_string();
                    match validate_font_family(&value) {
                        Ok(valid) => {
                            this.font_family_error = None;
                            cx.update_global::<Appearance, _>(|appearance, _| {
                                appearance.font_family = valid;
                            });
                            crate::settings::save_current(cx);
                        }
                        Err(err) => {
                            this.font_family_error = Some(err);
                        }
                    }
                    cx.notify();
                }
            }),
        );

        subscriptions.push(
            cx.subscribe(&font_size_input, |this, input: Entity<InputState>, event: &input::InputEvent, cx| {
                if let input::InputEvent::Change = event {
                    let value = input.read(cx).value().to_string();
                    match validate_font_size(&value) {
                        Ok(size) => {
                            this.font_size_error = None;
                            cx.update_global::<Appearance, _>(|appearance, _| {
                                appearance.font_size = size;
                            });
                            crate::settings::save_current(cx);
                        }
                        Err(err) => {
                            this.font_size_error = Some(err);
                        }
                    }
                    cx.notify();
                }
            }),
        );

        subscriptions.push(
            cx.subscribe(&bytes_per_row_input, |this, input: Entity<InputState>, event: &input::InputEvent, cx| {
                if let input::InputEvent::Change = event {
                    let value = input.read(cx).value().to_string();
                    match validate_bytes_per_row(&value) {
                        Ok(bpr) => {
                            this.bytes_per_row_error = None;
                            cx.update_global::<BytesPerRow, _>(|bytes_per_row, _| {
                                bytes_per_row.0 = bpr;
                            });
                            crate::settings::save_current(cx);
                        }
                        Err(err) => {
                            this.bytes_per_row_error = Some(err);
                        }
                    }
                    cx.notify();
                }
            }),
        );

        subscriptions.push(cx.subscribe(
            &structure_yaml_sha_threshold_input,
            |this, input: Entity<InputState>, event: &input::InputEvent, cx| {
                if let input::InputEvent::Change = event {
                    let value = input.read(cx).value().to_string();
                    match validate_yaml_sha_threshold(&value) {
                        Ok(threshold) => {
                            this.yaml_sha_threshold_error = None;
                            cx.update_global::<StructureYamlShaThreshold, _>(|current, _| {
                                current.0 = threshold;
                            });
                            crate::settings::save_current(cx);
                        }
                        Err(err) => {
                            this.yaml_sha_threshold_error = Some(err);
                        }
                    }
                    cx.notify();
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
            filter_input,
            theme_select,
            font_family_input,
            font_size_input,
            bytes_per_row_input,
            structure_yaml_sha_threshold_input,
            font_family_error: None,
            font_size_error: None,
            bytes_per_row_error: None,
            yaml_sha_threshold_error: None,
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
        self.font_family_error = None;
        self.font_size_error = None;
        self.bytes_per_row_error = None;
        self.yaml_sha_threshold_error = None;
    }

    fn has_any_modified_setting(&self, cx: &App) -> bool {
        let active_theme = cx.theme().theme_name().clone();
        let saved_theme = self.theme_preview_state.borrow().origin().cloned().unwrap_or(active_theme);
        let appearance = cx.global::<Appearance>();
        let default_appearance = Appearance::default();

        saved_theme != crate::settings::DEFAULT_LIGHT_THEME
            || appearance.font_family != default_appearance.font_family
            || (appearance.font_size - default_appearance.font_size).abs() > f32::EPSILON
            || cx.global::<BytesPerRow>().0 != DEFAULT_BYTES_PER_ROW
            || *cx.global::<Encoding>() != Encoding::default()
            || *cx.global::<DisplayRadix>() != DisplayRadix::default()
            || *cx.global::<HexCase>() != HexCase::default()
            || *cx.global::<ByteGroupSize>() != ByteGroupSize::default()
            || *cx.global::<ByteOrder>() != ByteOrder::default()
            || *cx.global::<OverwriteDeleteBehavior>() != OverwriteDeleteBehavior::default()
            || *cx.global::<ByteColoring>() != ByteColoring::default()
            || *cx.global::<ByteCategoryColors>() != ByteCategoryColors::default()
            || cx.global::<StructureYamlShaThreshold>().0 != crate::settings::DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD
            || cx.global::<StructureYamlIncludeOffsets>().0 != crate::settings::DEFAULT_STRUCTURE_YAML_INCLUDE_OFFSETS
    }

    fn reset_all_to_defaults(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let default_appearance = Appearance::default();
        let default_theme = SharedString::from(crate::settings::DEFAULT_LIGHT_THEME);
        self.theme_select.update(cx, |select, cx| {
            select.set_selected_value(&default_theme, window, cx);
        });
        crate::theme::apply_theme_by_name(crate::settings::DEFAULT_LIGHT_THEME, Some(window), cx);
        self.theme_preview_state.borrow_mut().confirm();

        cx.update_global::<Appearance, _>(|current, _| {
            *current = default_appearance.clone();
        });
        cx.update_global::<BytesPerRow, _>(|current, _| {
            current.0 = DEFAULT_BYTES_PER_ROW;
        });
        cx.update_global::<Encoding, _>(|current, _| {
            *current = Encoding::default();
        });
        cx.update_global::<DisplayRadix, _>(|current, _| {
            *current = DisplayRadix::default();
        });
        cx.update_global::<HexCase, _>(|current, _| {
            *current = HexCase::default();
        });
        cx.update_global::<ByteGroupSize, _>(|current, _| {
            *current = ByteGroupSize::default();
        });
        cx.update_global::<ByteOrder, _>(|current, _| {
            *current = ByteOrder::default();
        });
        cx.update_global::<OverwriteDeleteBehavior, _>(|current, _| {
            *current = OverwriteDeleteBehavior::default();
        });
        cx.update_global::<ByteColoring, _>(|current, _| {
            *current = ByteColoring::default();
        });
        cx.update_global::<ByteCategoryColors, _>(|current, _| {
            current.reset_all();
        });
        cx.update_global::<StructureYamlShaThreshold, _>(|current, _| {
            current.0 = crate::settings::DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD;
        });
        cx.update_global::<StructureYamlIncludeOffsets, _>(|current, _| {
            current.0 = crate::settings::DEFAULT_STRUCTURE_YAML_INCLUDE_OFFSETS;
        });

        self.font_family_input.update(cx, |input, cx| {
            input.set_value(default_appearance.font_family, window, cx);
        });
        self.font_size_input.update(cx, |input, cx| {
            input.set_value(default_appearance.font_size.to_string(), window, cx);
        });
        self.bytes_per_row_input.update(cx, |input, cx| {
            input.set_value(DEFAULT_BYTES_PER_ROW.to_string(), window, cx);
        });
        self.structure_yaml_sha_threshold_input.update(cx, |input, cx| {
            input.set_value(crate::settings::DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD.to_string(), window, cx);
        });
        self.font_family_error = None;
        self.font_size_error = None;
        self.bytes_per_row_error = None;
        self.yaml_sha_threshold_error = None;

        crate::settings::save_current(cx);
        window.push_notification(Notification::info("Reset all settings to defaults"), cx);
        cx.notify();
    }

    fn open_reset_all_confirmation(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        let view = cx.entity().clone();
        window.open_dialog(cx, move |dialog, _window, cx| {
            let theme = cx.theme();
            let view_for_confirm = view.clone();
            let view_for_ok = view.clone();
            dialog
                .w(px(420.0))
                .title(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(Icon::new(IconName::TriangleAlert).size(px(18.0)).text_color(theme.warning))
                        .child(div().text_sm().font_semibold().text_color(theme.foreground).child("Reset All Settings")),
                )
                .content(|content, _window, cx| {
                    content.child(
                        div()
                            .py_2()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Restore Theme, Editor, Byte Coloring, and Structure Analysis preferences to their default values?"),
                    )
                })
                .footer(
                    DialogFooter::new()
                        .child(
                            Button::new("reset-all-cancel")
                                .label("Cancel")
                                .ghost()
                                .with_size(Size::Small)
                                .on_click(|_, window, cx| {
                                    window.close_dialog(cx);
                                }),
                        )
                        .child(
                            Button::new("reset-all-confirm")
                                .label("Reset All")
                                .danger()
                                .with_size(Size::Small)
                                .on_click(move |_, window, cx| {
                                    window.close_dialog(cx);
                                    view_for_confirm.update(cx, |this, cx| {
                                        this.reset_all_to_defaults(window, cx);
                                    });
                                }),
                        ),
                )
                .on_ok(move |_, window, cx| {
                    view_for_ok.update(cx, |this, cx| {
                        this.reset_all_to_defaults(window, cx);
                    });
                    true
                })
        });
    }
}

fn render_setting_row(
    label: &'static str,
    description: &'static str,
    control: AnyElement,
    error: Option<&str>,
    reset_button: Option<AnyElement>,
    theme: &Theme,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap_4()
        .px_4()
        .py_3()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap_0p5()
                .child(div().text_sm().font_medium().text_color(theme.foreground).child(label))
                .child(div().text_xs().text_color(theme.muted_foreground).child(description)),
        )
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .items_end()
                .gap_1()
                .child(
                    div().flex().items_center().gap_1p5().child(div().w(px(CONTROL_WIDTH)).child(control)).child(
                        div()
                            .w(px(RESET_SLOT_WIDTH))
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .children(reset_button),
                    ),
                )
                .when_some(error, |el, msg| {
                    el.child(
                        div()
                            .w(px(CONTROL_WIDTH + RESET_SLOT_WIDTH + 6.0))
                            .text_xs()
                            .text_color(theme.danger)
                            .child(msg.to_string()),
                    )
                }),
        )
        .into_any_element()
}

fn render_settings_section(title: &'static str, rows: Vec<AnyElement>, theme: &Theme) -> Option<AnyElement> {
    if rows.is_empty() {
        return None;
    }
    let border_subtle = theme.border.opacity(0.5);
    let body_rows = rows
        .into_iter()
        .enumerate()
        .map(|(ix, row)| div().when(ix > 0, |el| el.border_t_1().border_color(border_subtle)).child(row));

    Some(
        div()
            .flex()
            .flex_col()
            .rounded_lg()
            .border_1()
            .border_color(theme.border)
            .bg(theme.tab_bar.opacity(0.4))
            .overflow_hidden()
            .child(
                div()
                    .px_4()
                    .py_2()
                    .bg(theme.secondary.opacity(0.4))
                    .border_b_1()
                    .border_color(theme.border)
                    .text_xs()
                    .font_semibold()
                    .text_color(theme.muted_foreground)
                    .child(title),
            )
            .children(body_rows)
            .into_any_element(),
    )
}

impl Render for SettingsView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.theme_preview_state.borrow().is_open() {
            let active_theme = cx.theme().theme_name().clone();
            self.theme_select.update(cx, |select, cx| {
                if select.selected_value() != Some(&active_theme) {
                    select.set_selected_value(&active_theme, window, cx);
                }
            });
        }

        let theme = cx.theme().clone();
        let filter_query = self.filter_input.read(cx).value().to_string();
        let any_modified = self.has_any_modified_setting(cx);

        // --- Section 1: Appearance ---
        let mut appearance_rows: Vec<AnyElement> = Vec::new();
        if matches_settings_filter(&filter_query, "Appearance", "Theme", "Color theme for the editor and UI") {
            let active_theme_name = cx.theme().theme_name().clone();
            let all_themes = crate::theme::all_theme_names(cx);
            let preview_state = self.theme_preview_state.clone();
            let saved_theme_name = preview_state.borrow().origin().cloned().unwrap_or_else(|| active_theme_name.clone());
            let is_modified = saved_theme_name != crate::settings::DEFAULT_LIGHT_THEME;

            let control = div()
                .w_full()
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
                .child(Select::new(&self.theme_select).w_full().with_size(Size::Small))
                .into_any_element();

            let reset = is_modified.then(|| {
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
                    }))
                    .into_any_element()
            });

            appearance_rows.push(render_setting_row(
                "Theme",
                "Color theme for the editor and workspace",
                control,
                None,
                reset,
                &theme,
            ));
        }

        // --- Section 2: Editor ---
        let mut editor_rows: Vec<AnyElement> = Vec::new();

        if matches_settings_filter(&filter_query, "Editor", "Font Family", "Monospace font used in hex, ASCII, and data views") {
            let default_family = Appearance::default().font_family;
            let is_modified = cx.global::<Appearance>().font_family != default_family;
            let control = Input::new(&self.font_family_input).small().into_any_element();
            let reset = is_modified.then(|| {
                Button::new("reset-font-family")
                    .icon(IconName::Undo2)
                    .ghost()
                    .with_size(Size::Small)
                    .tooltip("Reset to default")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let default_family = Appearance::default().font_family;
                        this.font_family_error = None;
                        cx.update_global::<Appearance, _>(|appearance, _| {
                            appearance.font_family = default_family.clone();
                        });
                        this.font_family_input.update(cx, |input, cx| {
                            input.set_value(default_family, window, cx);
                        });
                        crate::settings::save_current(cx);
                        window.push_notification(Notification::info("Reset Font Family to default"), cx);
                    }))
                    .into_any_element()
            });
            editor_rows.push(render_setting_row(
                "Font Family",
                "Monospace font used in hex, ASCII, and inspector views",
                control,
                self.font_family_error.as_deref(),
                reset,
                &theme,
            ));
        }

        if matches_settings_filter(&filter_query, "Editor", "Font Size", "Editor font size in points (8–72)") {
            let default_size = Appearance::default().font_size;
            let is_modified = (cx.global::<Appearance>().font_size - default_size).abs() > f32::EPSILON;
            let control = NumberInput::new(&self.font_size_input).small().into_any_element();
            let reset = is_modified.then(|| {
                Button::new("reset-font-size")
                    .icon(IconName::Undo2)
                    .ghost()
                    .with_size(Size::Small)
                    .tooltip("Reset to default")
                    .on_click(cx.listener(|this, _, window, cx| {
                        let default_size = Appearance::default().font_size;
                        this.font_size_error = None;
                        cx.update_global::<Appearance, _>(|appearance, _| {
                            appearance.font_size = default_size;
                        });
                        this.font_size_input.update(cx, |input, cx| {
                            input.set_value(default_size.to_string(), window, cx);
                        });
                        crate::settings::save_current(cx);
                        window.push_notification(Notification::info("Reset Font Size to default"), cx);
                    }))
                    .into_any_element()
            });
            editor_rows.push(render_setting_row(
                "Font Size",
                "Editor font size in points (8–72)",
                control,
                self.font_size_error.as_deref(),
                reset,
                &theme,
            ));
        }

        if matches_settings_filter(&filter_query, "Editor", "Bytes Per Row", "Number of bytes displayed per row (1–64)") {
            let default_bytes = DEFAULT_BYTES_PER_ROW;
            let is_modified = cx.global::<BytesPerRow>().0 != default_bytes;
            let control = NumberInput::new(&self.bytes_per_row_input).small().into_any_element();
            let reset = is_modified.then(|| {
                Button::new("reset-bytes-per-row")
                    .icon(IconName::Undo2)
                    .ghost()
                    .with_size(Size::Small)
                    .tooltip("Reset to default")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.bytes_per_row_error = None;
                        cx.update_global::<BytesPerRow, _>(|bytes_per_row, _| {
                            bytes_per_row.0 = DEFAULT_BYTES_PER_ROW;
                        });
                        this.bytes_per_row_input.update(cx, |input, cx| {
                            input.set_value(DEFAULT_BYTES_PER_ROW.to_string(), window, cx);
                        });
                        crate::settings::save_current(cx);
                        window.push_notification(Notification::info("Reset Bytes Per Row to default"), cx);
                    }))
                    .into_any_element()
            });
            editor_rows.push(render_setting_row(
                "Bytes Per Row",
                "Number of bytes displayed per row (1–64)",
                control,
                self.bytes_per_row_error.as_deref(),
                reset,
                &theme,
            ));
        }

        if matches_settings_filter(&filter_query, "Editor", "Default Encoding", "Initial text decoding used for newly opened files") {
            let default_encoding = *cx.global::<Encoding>();
            let is_modified = default_encoding != Encoding::default();
            let control = Button::new("default-encoding")
                .w_full()
                .label(default_encoding.label())
                .outline()
                .dropdown_caret(true)
                .with_size(Size::Small)
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, window, cx| {
                    let menu = Encoding::primary_encodings().iter().copied().fold(menu, |menu, encoding| {
                        menu.item(
                            PopupMenuItem::new(encoding.label())
                                .checked(encoding == default_encoding)
                                .on_click(move |_, _, cx| {
                                    cx.update_global::<Encoding, _>(|current, _| {
                                        *current = encoding;
                                    });
                                    crate::settings::save_current(cx);
                                }),
                        )
                    });
                    let menu = menu.separator();
                    Encoding::secondary_categories().iter().fold(menu, |menu, (cat, encs)| {
                        menu.submenu(cat.label(), window, cx, move |menu, _window, _cx| {
                            encs.iter().copied().fold(menu, |menu, encoding| {
                                menu.item(
                                    PopupMenuItem::new(encoding.label())
                                        .checked(encoding == default_encoding)
                                        .on_click(move |_, _, cx| {
                                            cx.update_global::<Encoding, _>(|current, _| {
                                                *current = encoding;
                                            });
                                            crate::settings::save_current(cx);
                                        }),
                                )
                            })
                        })
                    })
                })
                .into_any_element();
            let reset = is_modified.then(|| {
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
                    }))
                    .into_any_element()
            });
            editor_rows.push(render_setting_row(
                "Default Encoding",
                "Initial text decoding used for newly opened files",
                control,
                None,
                reset,
                &theme,
            ));
        }

        if matches_settings_filter(&filter_query, "Editor", "Default Radix", "Number base for byte values in new tabs") {
            let default_radix = *cx.global::<DisplayRadix>();
            let is_modified = default_radix != DisplayRadix::default();
            let control = Button::new("default-radix")
                .w_full()
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
                })
                .into_any_element();
            let reset = is_modified.then(|| {
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
                    }))
                    .into_any_element()
            });
            editor_rows.push(render_setting_row(
                "Default Radix",
                "Number base for byte values in newly opened tabs",
                control,
                None,
                reset,
                &theme,
            ));
        }

        if matches_settings_filter(&filter_query, "Editor", "Hex Case", "Uppercase or lowercase letters for hex digits") {
            let hex_case = *cx.global::<HexCase>();
            let is_modified = hex_case != HexCase::default();
            let control = Button::new("hex-case")
                .w_full()
                .label(hex_case.label())
                .outline()
                .dropdown_caret(true)
                .with_size(Size::Small)
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                    HexCase::ALL.iter().copied().fold(menu, |menu, case| {
                        menu.item(PopupMenuItem::new(case.label()).checked(case == hex_case).on_click(move |_, _, cx| {
                            cx.update_global::<HexCase, _>(|current, _| {
                                *current = case;
                            });
                            crate::settings::save_current(cx);
                        }))
                    })
                })
                .into_any_element();
            let reset = is_modified.then(|| {
                Button::new("reset-hex-case")
                    .icon(IconName::Undo2)
                    .ghost()
                    .with_size(Size::Small)
                    .tooltip("Reset to default")
                    .on_click(cx.listener(|_, _, window, cx| {
                        cx.update_global::<HexCase, _>(|current, _| {
                            *current = HexCase::default();
                        });
                        crate::settings::save_current(cx);
                        window.push_notification(Notification::info("Reset Hex Case to default"), cx);
                    }))
                    .into_any_element()
            });
            editor_rows.push(render_setting_row(
                "Hex Case",
                "Uppercase (A–F) or lowercase (a–f) digits across the UI",
                control,
                None,
                reset,
                &theme,
            ));
        }

        if matches_settings_filter(&filter_query, "Editor", "Default Grouping", "Byte grouping width in new tabs") {
            let default_group_size = *cx.global::<ByteGroupSize>();
            let is_modified = default_group_size != ByteGroupSize::default();
            let control = Button::new("default-grouping")
                .w_full()
                .label(default_group_size.label())
                .outline()
                .dropdown_caret(true)
                .with_size(Size::Small)
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                    ByteGroupSize::ALL.iter().copied().fold(menu, |menu, group_size| {
                        menu.item(
                            PopupMenuItem::new(group_size.label())
                                .checked(group_size == default_group_size)
                                .on_click(move |_, _, cx| {
                                    cx.update_global::<ByteGroupSize, _>(|current, _| {
                                        *current = group_size;
                                    });
                                    crate::settings::save_current(cx);
                                }),
                        )
                    })
                })
                .into_any_element();
            let reset = is_modified.then(|| {
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
                    }))
                    .into_any_element()
            });
            editor_rows.push(render_setting_row(
                "Default Grouping",
                "Number of bytes grouped together per cell",
                control,
                None,
                reset,
                &theme,
            ));
        }

        if matches_settings_filter(&filter_query, "Editor", "Default Byte Order", "Endianness for multi-byte groups and inspector") {
            let default_byte_order = *cx.global::<ByteOrder>();
            let is_modified = default_byte_order != ByteOrder::default();
            let control = Button::new("default-byte-order")
                .w_full()
                .label(default_byte_order.label())
                .outline()
                .dropdown_caret(true)
                .with_size(Size::Small)
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                    ByteOrder::ALL.iter().copied().fold(menu, |menu, byte_order| {
                        menu.item(
                            PopupMenuItem::new(byte_order.label())
                                .checked(byte_order == default_byte_order)
                                .on_click(move |_, _, cx| {
                                    cx.update_global::<ByteOrder, _>(|current, _| {
                                        *current = byte_order;
                                    });
                                    crate::settings::save_current(cx);
                                }),
                        )
                    })
                })
                .into_any_element();
            let reset = is_modified.then(|| {
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
                    }))
                    .into_any_element()
            });
            editor_rows.push(render_setting_row(
                "Default Byte Order",
                "Endianness for multi-byte grouping and status previews",
                control,
                None,
                reset,
                &theme,
            ));
        }

        if matches_settings_filter(
            &filter_query,
            "Editor",
            "Overwrite Delete / Cut",
            "Whether Delete, Backspace, and Cut remove bytes or zero-fill in Overwrite mode",
        ) {
            let overwrite_delete = *cx.global::<OverwriteDeleteBehavior>();
            let is_modified = overwrite_delete != OverwriteDeleteBehavior::default();
            let control = Button::new("overwrite-delete-behavior")
                .w_full()
                .label(overwrite_delete.label())
                .outline()
                .dropdown_caret(true)
                .with_size(Size::Small)
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                    OverwriteDeleteBehavior::ALL.iter().copied().fold(menu, |menu, behavior| {
                        menu.item(
                            PopupMenuItem::new(behavior.label())
                                .checked(behavior == overwrite_delete)
                                .on_click(move |_, _, cx| {
                                    cx.update_global::<OverwriteDeleteBehavior, _>(|current, _| {
                                        *current = behavior;
                                    });
                                    crate::settings::save_current(cx);
                                }),
                        )
                    })
                })
                .into_any_element();
            let reset = is_modified.then(|| {
                Button::new("reset-overwrite-delete")
                    .icon(IconName::Undo2)
                    .ghost()
                    .with_size(Size::Small)
                    .tooltip("Reset to default")
                    .on_click(cx.listener(|_, _, window, cx| {
                        cx.update_global::<OverwriteDeleteBehavior, _>(|current, _| {
                            *current = OverwriteDeleteBehavior::default();
                        });
                        crate::settings::save_current(cx);
                        window.push_notification(Notification::info("Reset Overwrite Delete / Cut to default"), cx);
                    }))
                    .into_any_element()
            });
            editor_rows.push(render_setting_row(
                "Overwrite Delete / Cut",
                "Choose whether Delete, Backspace, and Cut remove bytes or zero-fill in Overwrite mode",
                control,
                None,
                reset,
                &theme,
            ));
        }

        if matches_settings_filter(
            &filter_query,
            "Editor",
            "Byte Coloring Category Colors",
            "Color bytes by category (null, ASCII, control, extended, FF)",
        ) {
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
            let border_accent = if is_custom { theme.primary } else { theme.border };

            let control = Button::new("byte-coloring")
                .w_full()
                .label(byte_coloring.label())
                .outline()
                .dropdown_caret(true)
                .with_size(Size::Small)
                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                    let menu = ByteColoring::PRESETS.iter().copied().fold(menu, |menu, coloring| {
                        menu.item(
                            PopupMenuItem::new(coloring.label())
                                .checked(coloring == byte_coloring)
                                .on_click(move |_, _, cx| {
                                    cx.update_global::<ByteColoring, _>(|current, _| {
                                        *current = coloring;
                                    });
                                    if let Some(colors) = coloring.colors() {
                                        cx.update_global::<ByteCategoryColors, _>(|current, _| {
                                            *current = colors;
                                        });
                                    }
                                    crate::settings::save_current(cx);
                                }),
                        )
                    });
                    if is_custom {
                        menu.separator().item(PopupMenuItem::new(ByteColoring::Custom.label()).checked(true))
                    } else {
                        menu
                    }
                })
                .into_any_element();

            let reset = is_modified.then(|| {
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
                    }))
                    .into_any_element()
            });

            let category_panel =
                div()
                    .mx_4()
                    .mb_3()
                    .px_3()
                    .py_2p5()
                    .border_l_2()
                    .border_color(border_accent)
                    .bg(theme.secondary.opacity(0.45))
                    .rounded_md()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .pb_1()
                            .child(div().text_xs().font_semibold().text_color(theme.muted_foreground).child("Category Colors"))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if is_custom { theme.primary } else { theme.muted_foreground })
                                    .child(status_text),
                            ),
                    )
                    .children(HexByteCategory::ALL.iter().copied().map(|cat| {
                        let color = category_colors.color_for(Some(cat));
                        let default_cat_color = default_colors.color_for(Some(cat));
                        let is_cat_modified = color != default_cat_color;
                        let swatch_color = resolve_category_color(color, &theme);

                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_4()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
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
                                            .border_color(theme.border),
                                    )
                                    .child(div().text_xs().text_color(theme.foreground).child(cat.label())),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .flex()
                                    .items_center()
                                    .gap_1p5()
                                    .child(
                                        div().w(px(CONTROL_WIDTH)).child(
                                            Button::new(format!("cat-color-{}", cat.id()))
                                                .w_full()
                                                .label(color.label())
                                                .outline()
                                                .dropdown_caret(true)
                                                .with_size(Size::Small)
                                                .dropdown_menu_with_anchor(Anchor::TopRight, move |menu, _window, _cx| {
                                                    CategoryColor::ALL.iter().copied().fold(menu, |menu, choice| {
                                                        menu.item(PopupMenuItem::new(choice.label()).checked(choice == color).on_click(move |_, _, cx| {
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
                                                        }))
                                                    })
                                                }),
                                        ),
                                    )
                                    .child(div().w(px(RESET_SLOT_WIDTH)).flex_shrink_0().flex().items_center().justify_center().when(
                                        is_cat_modified,
                                        |slot| {
                                            slot.child(
                                                Button::new(format!("reset-cat-color-{}", cat.id()))
                                                    .icon(IconName::Undo2)
                                                    .ghost()
                                                    .with_size(Size::Small)
                                                    .tooltip("Reset category to default")
                                                    .on_click(cx.listener(move |_, _, window, cx| {
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
                                                        window.push_notification(Notification::info(format!("Reset {} color to default", cat.label())), cx);
                                                    })),
                                            )
                                        },
                                    )),
                            )
                    }));

            editor_rows.push(
                div()
                    .flex()
                    .flex_col()
                    .child(render_setting_row(
                        "Byte Coloring",
                        "Color hex bytes by value category for faster visual scanning",
                        control,
                        None,
                        reset,
                        &theme,
                    ))
                    .child(category_panel)
                    .into_any_element(),
            );
        }

        // --- Section 3: Structure Analysis ---
        let mut structure_rows: Vec<AnyElement> = Vec::new();

        if matches_settings_filter(
            &filter_query,
            "Structure Analysis",
            "YAML SHA Threshold",
            "Byte length above which YAML export replaces raw hex with SHA-256 hash",
        ) {
            let default_threshold = crate::settings::DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD;
            let is_modified = cx.global::<StructureYamlShaThreshold>().0 != default_threshold;
            let control = div()
                .flex()
                .items_center()
                .gap_2()
                .w_full()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(NumberInput::new(&self.structure_yaml_sha_threshold_input).small()),
                )
                .child(div().flex_shrink_0().text_xs().text_color(theme.muted_foreground).child("bytes"))
                .into_any_element();
            let reset = is_modified.then(|| {
                Button::new("reset-yaml-sha-threshold")
                    .icon(IconName::Undo2)
                    .ghost()
                    .with_size(Size::Small)
                    .tooltip("Reset to default (32 bytes)")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.yaml_sha_threshold_error = None;
                        cx.update_global::<StructureYamlShaThreshold, _>(|current, _| {
                            current.0 = crate::settings::DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD;
                        });
                        this.structure_yaml_sha_threshold_input.update(cx, |input, cx| {
                            input.set_value(crate::settings::DEFAULT_STRUCTURE_YAML_SHA_THRESHOLD.to_string(), window, cx);
                        });
                        crate::settings::save_current(cx);
                        window.push_notification(Notification::info("Reset YAML SHA Threshold to default"), cx);
                    }))
                    .into_any_element()
            });
            structure_rows.push(render_setting_row(
                "YAML SHA Threshold",
                "Byte length above which YAML export summarizes raw bytes as SHA-256 (0 disables)",
                control,
                self.yaml_sha_threshold_error.as_deref(),
                reset,
                &theme,
            ));
        }

        if matches_settings_filter(
            &filter_query,
            "Structure Analysis",
            "YAML Include Offsets",
            "Include byte offset annotations when exporting parsed structures to YAML",
        ) {
            let include_offsets = cx.global::<StructureYamlIncludeOffsets>().0;
            let default_include_offsets = crate::settings::DEFAULT_STRUCTURE_YAML_INCLUDE_OFFSETS;
            let is_modified = include_offsets != default_include_offsets;
            let control = Button::new("yaml-include-offsets")
                .w_full()
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
                    .item(PopupMenuItem::new("Disabled").checked(!include_offsets).on_click(move |_, _, cx| {
                        cx.update_global::<StructureYamlIncludeOffsets, _>(|current, _| {
                            current.0 = false;
                        });
                        crate::settings::save_current(cx);
                    }))
                })
                .into_any_element();
            let reset = is_modified.then(|| {
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
                    }))
                    .into_any_element()
            });
            structure_rows.push(render_setting_row(
                "YAML Include Offsets",
                "Include byte offset comments when exporting parsed structures to YAML",
                control,
                None,
                reset,
                &theme,
            ));
        }

        let sections: Vec<AnyElement> = [
            render_settings_section("APPEARANCE", appearance_rows, &theme),
            render_settings_section("EDITOR", editor_rows, &theme),
            render_settings_section("STRUCTURE ANALYSIS", structure_rows, &theme),
        ]
        .into_iter()
        .flatten()
        .collect();

        let header = div()
            .flex()
            .items_center()
            .justify_between()
            .gap_4()
            .pb_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(div().text_xl().font_bold().text_color(theme.foreground).child("Settings"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Changes are applied and saved automatically"),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div().w(px(220.0)).child(
                            Input::new(&self.filter_input)
                                .small()
                                .prefix(Icon::new(IconName::Search).size_3p5().text_color(theme.muted_foreground))
                                .cleanable(true),
                        ),
                    )
                    .child(
                        Button::new("reset-all-settings")
                            .label("Reset All")
                            .icon(IconName::Undo2)
                            .ghost()
                            .with_size(Size::Small)
                            .disabled(!any_modified)
                            .tooltip("Reset all settings to their default values")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_reset_all_confirmation(window, cx);
                            })),
                    ),
            );

        div()
            .id("settings-view-scroll")
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_y_scroll()
            .bg(theme.background)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::on_action_update_setting_input))
            .child(
                div()
                    .w_full()
                    .max_w(px(780.0))
                    .mx_auto()
                    .px_6()
                    .py_5()
                    .flex()
                    .flex_col()
                    .gap_5()
                    .child(header)
                    .when(sections.is_empty(), |el| {
                        el.child(
                            div()
                                .py_12()
                                .flex()
                                .flex_col()
                                .items_center()
                                .gap_2()
                                .text_sm()
                                .text_color(theme.muted_foreground)
                                .child(Icon::new(IconName::Search).size(px(24.0)).text_color(theme.muted_foreground))
                                .child(format!("No settings matching \"{}\"", filter_query.trim())),
                        )
                    })
                    .children(sections),
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

    fn icon(&self, _cx: &App) -> crate::ui::icon::IconName {
        crate::ui::icon::IconName::Settings
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

use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::strings::{DEFAULT_MIN_STRING_LENGTH, StringMatch, find_strings_segmented_limited};
use crate::ui::appearance::Appearance;
use crate::ui::icon::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{self, Input, InputState, NumberInput};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::table::{Column, ColumnSort, DataTable, TableDelegate, TableEvent, TableState};
use gpui_kit::component::theme::Theme;
use gpui_kit::component::{ActiveTheme as _, Disableable, Sizable, Size, WindowExt as _, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::collections::HashMap;

actions!(strings_panel, [FocusTable, ClearResults, ConfirmSelect, DismissTable]);

#[derive(Clone, PartialEq, Action)]
#[action(namespace = strings_panel, no_json)]
struct CopyAddress {
    value: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = strings_panel, no_json)]
struct CopyValue {
    value: String,
}

const CONTEXT: &str = "StringsPanel";
pub const MAX_STRING_RESULTS: usize = 10_000;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", FocusTable, Some("StringsPanel")),
        KeyBinding::new("enter", ConfirmSelect, Some("StringsPanel")),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScanSignature {
    encoding: Encoding,
    content_state: usize,
    byte_len: usize,
}

struct CachedStringsState {
    results: Vec<StringMatch>,
    is_truncated: bool,
    selected_index: Option<usize>,
    has_scanned: bool,
    scan_signature: Option<ScanSignature>,
    scan_min_length: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StringsPanelEvent {
    NavigateTo { offset: usize, len: usize },
    FocusEditor,
}

pub struct StringsTableDelegate {
    panel: WeakEntity<StringsPanel>,
    results: Vec<StringMatch>,
    visible_indices: Vec<usize>,
    sort_column: Option<(usize, ColumnSort)>,
    editor: Option<Entity<Editor>>,
    selected_index: Option<usize>,
}

impl StringsTableDelegate {
    fn sort_indices(&mut self) {
        let Some((col_ix, sort)) = self.sort_column else {
            self.visible_indices.sort();
            return;
        };
        if sort == ColumnSort::Default {
            self.visible_indices.sort();
            return;
        }

        self.visible_indices.sort_by(|left, right| {
            let left_match = &self.results[*left];
            let right_match = &self.results[*right];
            let ordering = match col_ix {
                0 => left_match.offset.cmp(&right_match.offset),
                1 => left_match.byte_len.cmp(&right_match.byte_len),
                2 => left_match.text.cmp(&right_match.text),
                _ => std::cmp::Ordering::Equal,
            };
            let ordering = if sort == ColumnSort::Descending { ordering.reverse() } else { ordering };
            ordering.then_with(|| left.cmp(right))
        });
    }
}

impl TableDelegate for StringsTableDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        3
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.visible_indices.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        let sort = match self.sort_column {
            Some((ix, s)) if ix == col_ix => s,
            _ => ColumnSort::Default,
        };

        match col_ix {
            0 => Column::new("address", "Address").width(px(88.0)).min_width(px(50.0)).resizable(true).sort(sort),
            1 => Column::new("size", "Size").width(px(58.0)).min_width(px(40.0)).resizable(true).sort(sort),
            2 => Column::new("value", "Value").width(px(220.0)).min_width(px(80.0)).resizable(true).sort(sort),
            _ => Column::default(),
        }
    }

    fn perform_sort(&mut self, col_ix: usize, sort: ColumnSort, _window: &mut Window, cx: &mut Context<TableState<Self>>) {
        let sort_col = if sort == ColumnSort::Default { None } else { Some((col_ix, sort)) };
        self.sort_column = sort_col;
        self.sort_indices();
        if let Some(panel) = self.panel.upgrade() {
            cx.defer(move |cx| {
                panel.update(cx, |panel, cx| {
                    panel.sort_column = sort_col;
                    panel.sync_selection_to_filter(cx);
                    cx.notify();
                });
            });
        }
    }

    fn fit_column(&mut self, col_ix: usize, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> Option<Pixels> {
        let address_map = self
            .editor
            .as_ref()
            .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.address_map.clone()))
            .unwrap_or_default();

        let max_len = self
            .visible_indices
            .iter()
            .take(128)
            .filter_map(|&index| {
                self.results.get(index).map(|item| match col_ix {
                    0 => format!("0x{:08X}", address_map.offset_to_address(item.offset)).chars().count(),
                    1 => item.byte_len.to_string().chars().count(),
                    2 => preview_text(&item.text).chars().count(),
                    _ => 0,
                })
            })
            .max()
            .unwrap_or(0);

        Some(px(max_len as f32 * 7.5 + 24.0))
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(&index) = self.visible_indices.get(row_ix) else {
            return menu;
        };
        let Some(item) = self.results.get(index) else {
            return menu;
        };
        let address_map = self
            .editor
            .as_ref()
            .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.address_map.clone()))
            .unwrap_or_default();
        let offset_value = format!("0x{:08X}", address_map.offset_to_address(item.offset));
        let text_value = item.text.clone();

        menu.menu_with_icon("Copy Address", IconName::Hash, Box::new(CopyAddress { value: offset_value }))
            .menu_with_icon("Copy Value", IconName::Binary, Box::new(CopyValue { value: text_value }))
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let Some(&index) = self.visible_indices.get(row_ix) else {
            return div().into_any_element();
        };
        let Some(item) = self.results.get(index) else {
            return div().into_any_element();
        };

        let theme = cx.theme();
        let font_family = cx.global::<Appearance>().font_family.clone();
        let address_map = self
            .editor
            .as_ref()
            .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.address_map.clone()))
            .unwrap_or_default();

        let offset_value = format!("0x{:08X}", address_map.offset_to_address(item.offset));
        let preview = preview_text(&item.text);

        let is_selected = self.selected_index == Some(index);
        let colors = strings_row_colors(is_selected, false, theme);

        match col_ix {
            0 => div()
                .text_xs()
                .font_family(font_family)
                .text_color(colors.address_color)
                .child(offset_value)
                .into_any_element(),
            1 => div()
                .text_xs()
                .font_family(font_family)
                .text_color(colors.size_color)
                .child(item.byte_len.to_string())
                .into_any_element(),
            2 => div()
                .text_xs()
                .font_family(font_family)
                .text_color(colors.value_color)
                .child(preview)
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }
}

pub struct StringsPanel {
    pub editor: Option<Entity<Editor>>,
    pub focus_handle: FocusHandle,
    pub table_state: Entity<TableState<StringsTableDelegate>>,
    pub min_length_input: Entity<InputState>,
    pub filter_input: Entity<InputState>,
    pub is_scanning: bool,
    pub results: Vec<StringMatch>,
    pub is_truncated: bool,
    pub selected_index: Option<usize>,
    pub has_scanned: bool,
    pub scan_task: Option<Task<()>>,
    pub sort_column: Option<(usize, ColumnSort)>,
    scan_generation: usize,
    scan_signature: Option<ScanSignature>,
    scan_min_length: Option<usize>,
    cached_states: HashMap<EntityId, CachedStringsState>,
    _table_subscription: Option<Subscription>,
    _input_subscription: Option<Subscription>,
    _filter_subscription: Option<Subscription>,
    _editor_subscription: Option<Subscription>,
}

impl EventEmitter<StringsPanelEvent> for StringsPanel {}

impl StringsPanel {
    pub fn new(editor: Option<Entity<Editor>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let view = cx.entity().downgrade();
        let delegate = StringsTableDelegate {
            panel: view,
            results: Vec::new(),
            visible_indices: Vec::new(),
            sort_column: None,
            editor: None,
            selected_index: None,
        };
        let table_state = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .row_selectable(true)
                .col_selectable(false)
                .col_resizable(true)
                .col_movable(false)
        });

        let table_subscription = cx.subscribe_in(&table_state, window, |this, _, event: &TableEvent, window, cx| match event {
            TableEvent::SelectRow(row_ix) => {
                let visible_indices = this.sorted_indices(cx);
                if let Some(&index) = visible_indices.get(*row_ix)
                    && this.selected_index != Some(index)
                {
                    this.select_item(index, cx);
                }
            }
            TableEvent::DoubleClickedRow(row_ix) => {
                let visible_indices = this.sorted_indices(cx);
                if let Some(&index) = visible_indices.get(*row_ix) {
                    this.select_item(index, cx);
                    cx.emit(StringsPanelEvent::FocusEditor);
                    window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
                }
            }
            _ => {}
        });

        let min_length_input = cx.new(|cx| InputState::new(window, cx).step(1.0).min(1.0));
        min_length_input.update(cx, |input, cx| {
            input.set_value(DEFAULT_MIN_STRING_LENGTH.to_string(), window, cx);
        });

        let filter_input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by value..."));

        let input_subscription = cx.subscribe_in(&min_length_input, window, |this, _, event: &input::InputEvent, _window, cx| {
            if let input::InputEvent::Change = event {
                this.invalidate_scan();
                this.sync_selection_to_filter(cx);
                cx.notify();
            }
        });

        let filter_subscription = cx.subscribe_in(&filter_input, window, |this, _, event: &input::InputEvent, _window, cx| {
            if let input::InputEvent::Change = event {
                this.sync_selection_to_filter(cx);
                cx.notify();
            }
        });

        let mut this = Self {
            editor: None,
            focus_handle,
            table_state,
            min_length_input,
            filter_input,
            is_scanning: false,
            results: Vec::new(),
            is_truncated: false,
            selected_index: None,
            has_scanned: false,
            scan_task: None,
            scan_generation: 0,
            scan_signature: None,
            scan_min_length: None,
            cached_states: HashMap::new(),
            sort_column: None,
            _table_subscription: Some(table_subscription),
            _input_subscription: Some(input_subscription),
            _filter_subscription: Some(filter_subscription),
            _editor_subscription: None,
        };

        this.set_editor(editor, cx);
        this
    }

    pub fn set_editor(&mut self, editor: Option<Entity<Editor>>, cx: &mut Context<Self>) {
        let current_editor_id = self.editor.as_ref().map(Entity::entity_id);
        let next_editor_id = editor.as_ref().map(Entity::entity_id);
        if current_editor_id == next_editor_id {
            cx.notify();
            return;
        }

        self.cache_current_state();
        self._editor_subscription = None;
        self.invalidate_scan();
        self.editor = editor.clone();
        self.scan_signature = self.current_signature(cx);
        self.sync_selection_to_filter(cx);

        if let Some(ed) = &editor {
            self._editor_subscription = Some(cx.observe(ed, |this, _, cx| {
                this.scan_if_content_changed(cx);
            }));
        }
        if let Some(editor_id) = next_editor_id {
            self.restore_cached_state(editor_id, cx);
        }
        cx.notify();
    }

    fn cache_current_state(&mut self) {
        let Some(editor) = &self.editor else {
            return;
        };

        self.cached_states.insert(
            editor.entity_id(),
            CachedStringsState {
                results: std::mem::take(&mut self.results),
                is_truncated: self.is_truncated,
                selected_index: self.selected_index,
                has_scanned: self.has_scanned,
                scan_signature: self.scan_signature,
                scan_min_length: self.scan_min_length,
            },
        );
    }

    fn restore_cached_state(&mut self, editor_id: EntityId, cx: &mut Context<Self>) {
        let Some(cached) = self.cached_states.remove(&editor_id) else {
            return;
        };

        let is_current_scan = cached.has_scanned && cached.scan_signature == self.scan_signature && cached.scan_min_length == self.minimum_length(cx);
        if !is_current_scan && cached.has_scanned {
            return;
        }

        self.results = cached.results;
        self.is_truncated = cached.is_truncated;
        self.selected_index = cached.selected_index;
        self.has_scanned = cached.has_scanned;
        self.scan_min_length = cached.scan_min_length;
        self.sync_selection_to_filter(cx);
    }

    fn current_signature(&self, cx: &App) -> Option<ScanSignature> {
        let editor = self.editor.as_ref()?.read(cx);
        let document = editor.document.read().expect("document read lock");
        Some(ScanSignature {
            encoding: editor.options.encoding,
            content_state: document.history.state_id(),
            byte_len: document.buffer.len(),
        })
    }

    fn scan_if_content_changed(&mut self, cx: &mut Context<Self>) {
        let signature = self.current_signature(cx);
        if signature == self.scan_signature {
            cx.notify();
        } else {
            self.invalidate_scan();
            self.scan_signature = signature;
            self.sync_selection_to_filter(cx);
            cx.notify();
        }
    }

    fn invalidate_scan(&mut self) {
        self.scan_generation = self.scan_generation.wrapping_add(1);
        self.scan_task = None;
        self.is_scanning = false;
        self.results.clear();
        self.is_truncated = false;
        self.selected_index = None;
        self.has_scanned = false;
        self.scan_min_length = None;
    }

    pub fn trigger_scan(&mut self, cx: &mut Context<Self>) {
        self.invalidate_scan();
        self.sync_selection_to_filter(cx);
        let generation = self.scan_generation;
        self.scan_signature = self.current_signature(cx);

        let Some(editor_entity) = &self.editor else {
            self.is_scanning = false;
            cx.notify();
            return;
        };

        let Some(min_length) = self.minimum_length(cx) else {
            self.is_scanning = false;
            cx.notify();
            return;
        };
        self.scan_min_length = Some(min_length);

        let (encoding, buffer_data, segment_ranges) = {
            let editor = editor_entity.read(cx);
            let document = editor.document.read().expect("document read lock");
            (editor.options.encoding, document.buffer.clone(), document.address_map.segment_ranges())
        };

        self.is_scanning = true;
        let task = cx.spawn(async move |this, cx| {
            let (results, is_truncated) = cx
                .background_executor()
                .spawn(async move { find_strings_segmented_limited(buffer_data.data(), &segment_ranges, encoding, min_length, MAX_STRING_RESULTS) })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if this.scan_generation != generation {
                        return;
                    }

                    this.scan_task = None;
                    this.is_scanning = false;
                    this.is_truncated = is_truncated;
                    this.results = results;
                    this.has_scanned = true;
                    this.sync_selection_to_filter(cx);
                    cx.notify();
                });
            }
        });

        self.scan_task = Some(task);
        cx.notify();
    }

    fn minimum_length(&self, cx: &App) -> Option<usize> {
        self.min_length_input.read(cx).value().trim().parse::<usize>().ok().filter(|length| *length > 0)
    }

    fn filter_query(&self, cx: &App) -> String {
        self.filter_input.read(cx).value().trim().to_lowercase()
    }

    fn filtered_indices(&self, cx: &App) -> Vec<usize> {
        let filter = self.filter_query(cx);
        if filter.is_empty() {
            return (0..self.results.len()).collect();
        }

        self.results
            .iter()
            .enumerate()
            .filter(|(_, item)| item.text.to_lowercase().contains(&filter))
            .map(|(index, _)| index)
            .collect()
    }

    fn sorted_indices(&self, cx: &App) -> Vec<usize> {
        let mut indices = self.filtered_indices(cx);

        let Some((col_ix, sort)) = self.sort_column else {
            return indices;
        };
        if sort == ColumnSort::Default {
            return indices;
        }

        indices.sort_by(|left, right| {
            let left_match = &self.results[*left];
            let right_match = &self.results[*right];
            let ordering = match col_ix {
                0 => left_match.offset.cmp(&right_match.offset),
                1 => left_match.byte_len.cmp(&right_match.byte_len),
                2 => left_match.text.cmp(&right_match.text),
                _ => std::cmp::Ordering::Equal,
            };
            let ordering = if sort == ColumnSort::Descending { ordering.reverse() } else { ordering };
            ordering.then_with(|| left.cmp(right))
        });
        indices
    }

    fn result_summary(&self) -> String {
        if self.is_truncated {
            format!("{}+ entries found (showing first {} entries)", MAX_STRING_RESULTS, MAX_STRING_RESULTS)
        } else {
            let label = if self.results.len() == 1 { "entry" } else { "entries" };
            format!("{} {label} found", self.results.len())
        }
    }

    fn sync_selection_to_filter(&mut self, cx: &mut Context<Self>) {
        let visible_indices = self.sorted_indices(cx);
        if let Some(index) = self.selected_index {
            if !visible_indices.contains(&index) {
                self.selected_index = visible_indices.first().copied();
            }
        } else {
            self.selected_index = visible_indices.first().copied();
        }

        let row_ix = self.selected_index.and_then(|idx| visible_indices.iter().position(|&i| i == idx));
        let results = self.results.clone();
        let editor = self.editor.clone();
        let sort_column = self.sort_column;
        let selected_index = self.selected_index;

        self.table_state.update(cx, |table, cx| {
            let delegate = table.delegate_mut();
            delegate.results = results;
            delegate.visible_indices = visible_indices;
            delegate.sort_column = sort_column;
            delegate.editor = editor;
            delegate.selected_index = selected_index;
            table.refresh(cx);
            if let Some(row_ix) = row_ix {
                table.set_selected_row(row_ix, cx);
            } else {
                table.clear_selection(cx);
            }
        });
    }

    pub fn select_item(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(item) = self.results.get(index) {
            self.selected_index = Some(index);
            let offset = item.offset;
            let len = item.byte_len;

            let table_state = self.table_state.clone();
            cx.defer(move |cx| {
                table_state.update(cx, |table, cx| {
                    table.delegate_mut().selected_index = Some(index);
                    let row_ix = table.delegate().visible_indices.iter().position(|&i| i == index);
                    if let Some(row_ix) = row_ix
                        && table.selected_row() != Some(row_ix)
                    {
                        table.set_selected_row(row_ix, cx);
                    }
                });
            });

            if let Some(ed) = &self.editor {
                ed.update(cx, |editor, cx| {
                    if len > 0 {
                        editor.set_selection_range(offset..offset.saturating_add(len));
                    } else {
                        editor.set_cursor_offset(offset);
                    }
                    cx.notify();
                });
            }
            cx.emit(StringsPanelEvent::NavigateTo { offset, len });
        }
        cx.notify();
    }

    fn copy_address(&mut self, action: &CopyAddress, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
        window.push_notification(Notification::info(format!("Copied address {}", action.value)), cx);
    }

    fn copy_value(&mut self, action: &CopyValue, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
        window.push_notification(Notification::info("Copied value"), cx);
    }

    fn focus_table(&mut self, _: &FocusTable, window: &mut Window, cx: &mut Context<Self>) {
        let visible_indices = self.sorted_indices(cx);
        if !visible_indices.is_empty() {
            let focus_handle = self.table_state.read(cx).focus_handle(cx);
            focus_handle.focus(window, cx);
        } else {
            cx.emit(StringsPanelEvent::FocusEditor);
            window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
        }
        cx.notify();
    }

    fn confirm_select(&mut self, _: &ConfirmSelect, window: &mut Window, cx: &mut Context<Self>) {
        let visible_indices = self.sorted_indices(cx);
        if let Some(index) = self.selected_index.filter(|index| visible_indices.contains(index)) {
            self.select_item(index, cx);
        } else if let Some(&index) = visible_indices.first() {
            self.select_item(index, cx);
        }
        cx.emit(StringsPanelEvent::FocusEditor);
        window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
    }

    fn dismiss_table(&mut self, _: &DismissTable, window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(StringsPanelEvent::FocusEditor);
        window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
    }

    fn clear_results(&mut self, _: &ClearResults, _: &mut Window, cx: &mut Context<Self>) {
        self.invalidate_scan();
        self.sync_selection_to_filter(cx);
        cx.notify();
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StringsRowColors {
    pub bg_color: Hsla,
    pub border_color: Hsla,
    pub hover_bg: Hsla,
    pub address_color: Hsla,
    pub size_color: Hsla,
    pub value_color: Hsla,
}

pub(crate) fn strings_row_colors(is_selected: bool, is_table_focused: bool, theme: &Theme) -> StringsRowColors {
    if is_selected {
        if is_table_focused {
            StringsRowColors {
                bg_color: theme.selection,
                border_color: theme.accent,
                hover_bg: theme.selection,
                address_color: theme.accent_foreground,
                size_color: theme.accent_foreground,
                value_color: theme.accent_foreground,
            }
        } else {
            StringsRowColors {
                bg_color: theme.muted,
                border_color: theme.border,
                hover_bg: theme.muted,
                address_color: theme.foreground,
                size_color: theme.muted_foreground,
                value_color: theme.foreground,
            }
        }
    } else {
        StringsRowColors {
            bg_color: theme.sidebar,
            border_color: theme.table_row_border,
            hover_bg: theme.table_hover,
            address_color: theme.foreground,
            size_color: theme.muted_foreground,
            value_color: theme.foreground,
        }
    }
}

impl Render for StringsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let is_input_focused =
            self.min_length_input.read(cx).focus_handle(cx).is_focused(window) || self.filter_input.read(cx).focus_handle(cx).is_focused(window);
        let is_table_focused = self.table_state.read(cx).focus_handle(cx).is_focused(window);
        let is_focused = self.focus_handle.is_focused(window) || is_input_focused || is_table_focused;
        let has_editor = self.editor.is_some();
        let minimum_length_is_valid = self.minimum_length(cx).is_some();

        let actions = Button::new("clear-strings")
            .ghost()
            .icon(IconName::Eraser)
            .with_size(Size::XSmall)
            .tooltip("Clear results")
            .disabled(!self.has_scanned && !self.is_scanning)
            .on_click(cx.listener(|this, _, _window, cx| {
                this.invalidate_scan();
                this.sync_selection_to_filter(cx);
                cx.notify();
            }));
        let header = crate::ui::panels::panel_header("STRINGS", is_focused, theme, None, Some(actions.into_any_element()));

        let scan_controls = h_flex()
            .p_2()
            .gap_2()
            .border_b_1()
            .border_color(theme.border)
            .items_center()
            .child(div().text_xs().text_color(theme.muted_foreground).child("Min chars"))
            .child(
                div().w_24().child(
                    NumberInput::new(&self.min_length_input)
                        .with_size(Size::Small)
                        .disabled(!has_editor || self.is_scanning),
                ),
            )
            .child(
                Button::new("scan-strings")
                    .label("Scan")
                    .primary()
                    .with_size(Size::Small)
                    .disabled(self.is_scanning || !has_editor || !minimum_length_is_valid)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.trigger_scan(cx);
                    })),
            );

        let result_summary = div()
            .px_2()
            .py_1()
            .bg(theme.secondary)
            .border_b_1()
            .border_color(theme.border)
            .text_xs()
            .text_color(theme.muted_foreground)
            .child(self.result_summary());

        let filter_controls = h_flex()
            .p_2()
            .gap_1p5()
            .items_center()
            .border_b_1()
            .border_color(theme.border)
            .child(div().flex_1().child(Input::new(&self.filter_input)))
            .child(
                Button::new("clear-strings-filter")
                    .ghost()
                    .icon(IconName::Close)
                    .with_size(Size::XSmall)
                    .tooltip("Clear filter")
                    .disabled(self.filter_query(cx).is_empty())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.filter_input.update(cx, |input, cx| {
                            input.set_value(String::new(), window, cx);
                        });
                        this.sync_selection_to_filter(cx);
                        cx.notify();
                    })),
            );

        let body = if self.is_scanning {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Scanning...")
                .into_any_element()
        } else if self.results.is_empty() {
            let message = if self.has_scanned {
                "No strings found"
            } else if has_editor {
                "Click Scan to find strings"
            } else {
                "Open a file to scan strings"
            };
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child(message)
                .into_any_element()
        } else {
            div()
                .flex_1()
                .overflow_hidden()
                .child(DataTable::new(&self.table_state).bordered(false))
                .into_any_element()
        };

        crate::ui::panels::panel_container(is_focused, theme)
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::focus_table))
            .on_action(cx.listener(Self::confirm_select))
            .on_action(cx.listener(Self::dismiss_table))
            .on_action(cx.listener(Self::copy_address))
            .on_action(cx.listener(Self::copy_value))
            .on_action(cx.listener(Self::clear_results))
            .child(header)
            .child(scan_controls)
            .when(self.has_scanned || self.is_scanning, |this| this.child(result_summary).child(filter_controls))
            .child(body)
    }
}

fn preview_text(text: &str) -> String {
    const PREVIEW_LIMIT: usize = 256;
    let mut characters = text.chars();
    let preview: String = characters.by_ref().take(PREVIEW_LIMIT).collect();
    if characters.next().is_some() { format!("{preview}…") } else { preview }
}

impl Focusable for StringsPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::preview_text;

    #[test]
    fn test_preview_text() {
        let short = "Hello World";
        assert_eq!(preview_text(short), "Hello World");

        let long = "a".repeat(300);
        let preview = preview_text(&long);
        assert_eq!(preview.chars().count(), 257); // 256 + '…'
        assert!(preview.ends_with('…'));
    }

    #[test]
    fn test_navigation_positions() {
        let len = 20;
        // move_up
        let prev_pos = |curr: Option<usize>| match curr {
            Some(0) | None => len - 1,
            Some(p) => p - 1,
        };
        assert_eq!(prev_pos(Some(5)), 4);
        assert_eq!(prev_pos(Some(0)), 19);

        // move_down
        let next_pos = |curr: Option<usize>| match curr {
            Some(p) if p + 1 < len => p + 1,
            _ => 0,
        };
        assert_eq!(next_pos(Some(5)), 6);
        assert_eq!(next_pos(Some(19)), 0);

        // page_up & page_down
        let page_size = 10;
        let page_up = |curr: usize| curr.saturating_sub(page_size);
        let page_down = |curr: usize| (curr + page_size).min(len - 1);
        assert_eq!(page_up(15), 5);
        assert_eq!(page_up(4), 0);
        assert_eq!(page_down(5), 15);
        assert_eq!(page_down(15), 19);
    }

    #[test]
    fn test_strings_row_colors_unselected() {
        use super::strings_row_colors;
        let theme = super::Theme::default();
        let colors = strings_row_colors(false, false, &theme);
        assert_eq!(colors.bg_color, theme.sidebar);
        assert_eq!(colors.border_color, theme.table_row_border);
        assert_eq!(colors.address_color, theme.foreground);
        assert_eq!(colors.size_color, theme.muted_foreground);
        assert_eq!(colors.value_color, theme.foreground);
    }

    #[test]
    fn test_strings_row_colors_selected_focused() {
        use super::strings_row_colors;
        let theme = super::Theme::default();
        let colors = strings_row_colors(true, true, &theme);
        assert_eq!(colors.bg_color, theme.selection);
        assert_eq!(colors.border_color, theme.accent);
        assert_eq!(colors.address_color, theme.accent_foreground);
        assert_eq!(colors.size_color, theme.accent_foreground);
        assert_eq!(colors.value_color, theme.accent_foreground);
    }

    #[test]
    fn test_strings_row_colors_selected_unfocused() {
        use super::strings_row_colors;
        let theme = super::Theme::default();
        let colors = strings_row_colors(true, false, &theme);
        assert_eq!(colors.bg_color, theme.muted);
        assert_eq!(colors.border_color, theme.border);
        assert_eq!(colors.address_color, theme.foreground);
        assert_eq!(colors.size_color, theme.muted_foreground);
        assert_eq!(colors.value_color, theme.foreground);
    }

    #[test]
    fn test_strings_address_contrast_across_all_embedded_themes() {
        use super::strings_row_colors;
        use crate::assets::Assets;
        use crate::theme::{load_themes_from_assets, theme_names};
        use gpui_kit::{Hsla, Rgba};

        let registry = load_themes_from_assets(&Assets);
        let names = theme_names(registry.themes());
        assert!(!names.is_empty());

        fn channel_to_linear(c: f32) -> f32 {
            if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
        }

        fn rel_luminance(c: Hsla) -> f32 {
            let rgba = Rgba::from(c);
            0.2126 * channel_to_linear(rgba.r) + 0.7152 * channel_to_linear(rgba.g) + 0.0722 * channel_to_linear(rgba.b)
        }

        fn contrast_ratio(c1: Hsla, c2: Hsla) -> f32 {
            let l1 = rel_luminance(c1);
            let l2 = rel_luminance(c2);
            let (lighter, darker) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
            (lighter + 0.05) / (darker + 0.05)
        }

        for name in names {
            let config = registry.themes().get(&name).unwrap();
            let mut theme = super::Theme::default();
            theme.apply_config(config);

            let colors = strings_row_colors(false, false, &theme);
            let ratio = contrast_ratio(colors.address_color, colors.bg_color);

            // WCAG 2.1 Level AA requirement for normal text is 4.5:1
            assert!(
                ratio >= 4.5,
                "Theme '{}' failed WCAG AA contrast for address: ratio was {:.2}:1 (expected >= 4.5:1)",
                name,
                ratio
            );
        }
    }
}

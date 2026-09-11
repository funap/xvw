use crate::core::appearance::Appearance;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::strings::{DEFAULT_MIN_STRING_LENGTH, StringMatch, find_strings_segmented_limited};
use crate::ui::components::data_table::{self as table, TableColumn, TableSortDirection, VirtualTable, VirtualTableState};
use crate::ui::icon::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::theme::Theme;
use gpui_kit::component::{ActiveTheme as _, Disableable, Sizable, Size, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::collections::HashMap;

actions!(strings_panel, [FocusTable, ClearResults]);

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

const STRINGS_ROW_HEIGHT: f32 = 24.0;

pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", FocusTable, Some("StringsPanel"))]);
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

pub struct StringsPanel {
    pub editor: Option<Entity<Editor>>,
    pub focus_handle: FocusHandle,
    pub table_state: VirtualTableState,
    pub min_length_input: Entity<InputState>,
    pub filter_input: Entity<InputState>,
    pub is_scanning: bool,
    pub results: Vec<StringMatch>,
    pub is_truncated: bool,
    pub selected_index: Option<usize>,
    pub has_scanned: bool,
    pub scan_task: Option<Task<()>>,
    scan_generation: usize,
    scan_signature: Option<ScanSignature>,
    scan_min_length: Option<usize>,
    cached_states: HashMap<EntityId, CachedStringsState>,
    last_container_width: Pixels,
    _input_subscription: Option<Subscription>,
    _filter_subscription: Option<Subscription>,
    _editor_subscription: Option<Subscription>,
}

impl EventEmitter<StringsPanelEvent> for StringsPanel {}

fn default_strings_columns() -> Vec<TableColumn> {
    vec![
        TableColumn::new("address", "Address", px(88.0))
            .min_width(px(50.0))
            .sortable(true)
            .resizable(true),
        TableColumn::new("size", "Size", px(58.0)).min_width(px(40.0)).sortable(true).resizable(true),
        TableColumn::new("value", "Value", px(220.0)).min_width(px(80.0)).sortable(true).resizable(true),
    ]
}

impl StringsPanel {
    pub fn new(editor: Option<Entity<Editor>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let table_focus_handle = cx.focus_handle();
        let table_state = VirtualTableState::new("strings-table", default_strings_columns(), table_focus_handle);
        let min_length_input = cx.new(|cx| InputState::new(window, cx));
        min_length_input.update(cx, |input, cx| {
            input.set_value(DEFAULT_MIN_STRING_LENGTH.to_string(), window, cx);
        });

        let filter_input = cx.new(|cx| InputState::new(window, cx).placeholder("Filter by value..."));

        let input_subscription = cx.subscribe_in(&min_length_input, window, |this, _, event: &input::InputEvent, _window, cx| {
            if let input::InputEvent::Change = event {
                this.invalidate_scan();
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
            last_container_width: px(300.0),
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
                    this.selected_index = this.sorted_indices(cx).first().copied();
                    if this.selected_index.is_some() {
                        this.table_state.scroll_to_row(0, ScrollStrategy::Top);
                    }
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

        // Check which column is sorted
        let sorted_col = self
            .table_state
            .columns
            .iter()
            .enumerate()
            .find_map(|(ix, col)| col.sort_direction.map(|dir| (ix, dir)));

        let Some((col_ix, direction)) = sorted_col else {
            return indices;
        };

        indices.sort_by(|left, right| {
            let left_match = &self.results[*left];
            let right_match = &self.results[*right];
            let ordering = match col_ix {
                0 => left_match.offset.cmp(&right_match.offset),
                1 => left_match.byte_len.cmp(&right_match.byte_len),
                2 => left_match.text.cmp(&right_match.text),
                _ => std::cmp::Ordering::Equal,
            };
            let ordering = if direction == TableSortDirection::Descending {
                ordering.reverse()
            } else {
                ordering
            };
            ordering.then_with(|| left.cmp(right))
        });
        indices
    }

    fn sort_by(&mut self, col_ix: usize, cx: &mut Context<Self>) {
        self.table_state.toggle_column_sort(col_ix);
        self.sync_selection_to_filter(cx);
        cx.notify();
    }

    fn auto_fit_column(&mut self, col_ix: usize, cx: &mut Context<Self>) {
        let visible_indices = self.sorted_indices(cx);
        let address_map = self
            .editor
            .as_ref()
            .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.address_map.clone()))
            .unwrap_or_default();
        let texts = visible_indices.iter().take(128).filter_map(|&index| {
            self.results.get(index).map(|item| match col_ix {
                0 => format!("0x{:08X}", address_map.offset_to_address(item.offset)),
                1 => item.byte_len.to_string(),
                2 => preview_text(&item.text),
                _ => String::new(),
            })
        });

        self.table_state.auto_fit_column_with_texts(col_ix, texts);
        cx.notify();
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

        if let Some(index) = self.selected_index
            && let Some(position) = visible_indices.iter().position(|&visible_index| visible_index == index)
        {
            self.table_state.scroll_to_row(position, ScrollStrategy::Top);
        }
    }

    fn adjust_minimum_length(&mut self, delta: i32, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.minimum_length(cx).unwrap_or(DEFAULT_MIN_STRING_LENGTH);
        let next = if delta < 0 {
            current.saturating_sub(delta.unsigned_abs() as usize).max(1)
        } else {
            current.saturating_add(delta as usize)
        };

        self.min_length_input.update(cx, |input, cx| {
            input.set_value(next.to_string(), window, cx);
        });
        self.invalidate_scan();
        cx.notify();
    }

    pub fn select_item(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(item) = self.results.get(index) {
            self.selected_index = Some(index);
            let offset = item.offset;
            let len = item.byte_len;
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

    fn table_move_up(&mut self, _: &table::MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        let visible_indices = self.sorted_indices(cx);
        if visible_indices.is_empty() {
            return;
        }
        let current_position = self
            .selected_index
            .and_then(|index| visible_indices.iter().position(|&visible_index| visible_index == index));
        let new_position = match current_position {
            Some(0) | None => visible_indices.len().saturating_sub(1),
            Some(position) => position - 1,
        };
        self.table_state.scroll_to_row(new_position, ScrollStrategy::Top);
        if let Some(&index) = visible_indices.get(new_position) {
            self.select_item(index, cx);
        }
    }

    fn table_move_down(&mut self, _: &table::MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        let visible_indices = self.sorted_indices(cx);
        if visible_indices.is_empty() {
            return;
        }
        let current_position = self
            .selected_index
            .and_then(|index| visible_indices.iter().position(|&visible_index| visible_index == index));
        let new_position = match current_position {
            Some(position) if position + 1 < visible_indices.len() => position + 1,
            _ => 0,
        };
        self.table_state.scroll_to_row(new_position, ScrollStrategy::Top);
        if let Some(&index) = visible_indices.get(new_position) {
            self.select_item(index, cx);
        }
    }

    fn table_move_top(&mut self, _: &table::MoveTop, _: &mut Window, cx: &mut Context<Self>) {
        let visible_indices = self.sorted_indices(cx);
        if visible_indices.is_empty() {
            return;
        }
        self.table_state.scroll_to_row(0, ScrollStrategy::Top);
        if let Some(&index) = visible_indices.first() {
            self.select_item(index, cx);
        }
    }

    fn table_move_bottom(&mut self, _: &table::MoveBottom, _: &mut Window, cx: &mut Context<Self>) {
        let visible_indices = self.sorted_indices(cx);
        if visible_indices.is_empty() {
            return;
        }
        let last_pos = visible_indices.len().saturating_sub(1);
        self.table_state.scroll_to_row(last_pos, ScrollStrategy::Top);
        if let Some(&index) = visible_indices.get(last_pos) {
            self.select_item(index, cx);
        }
    }

    fn table_page_up(&mut self, _: &table::PageUp, _: &mut Window, cx: &mut Context<Self>) {
        let visible_indices = self.sorted_indices(cx);
        if visible_indices.is_empty() {
            return;
        }
        let current_position = self
            .selected_index
            .and_then(|index| visible_indices.iter().position(|&visible_index| visible_index == index))
            .unwrap_or(0);
        let page_size = 10;
        let new_position = current_position.saturating_sub(page_size);
        self.table_state.scroll_to_row(new_position, ScrollStrategy::Top);
        if let Some(&index) = visible_indices.get(new_position) {
            self.select_item(index, cx);
        }
    }

    fn table_page_down(&mut self, _: &table::PageDown, _: &mut Window, cx: &mut Context<Self>) {
        let visible_indices = self.sorted_indices(cx);
        if visible_indices.is_empty() {
            return;
        }
        let current_position = self
            .selected_index
            .and_then(|index| visible_indices.iter().position(|&visible_index| visible_index == index))
            .unwrap_or(0);
        let page_size = 10;
        let last_pos = visible_indices.len().saturating_sub(1);
        let new_position = (current_position + page_size).min(last_pos);
        self.table_state.scroll_to_row(new_position, ScrollStrategy::Top);
        if let Some(&index) = visible_indices.get(new_position) {
            self.select_item(index, cx);
        }
    }

    fn table_select_current(&mut self, _: &table::SelectCurrent, _: &mut Window, cx: &mut Context<Self>) {
        let visible_indices = self.sorted_indices(cx);
        if let Some(index) = self.selected_index.filter(|index| visible_indices.contains(index)) {
            self.select_item(index, cx);
        } else if let Some(&index) = visible_indices.first() {
            self.select_item(index, cx);
        }
    }

    fn table_dismiss(&mut self, _: &table::Dismiss, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(StringsPanelEvent::FocusEditor);
    }

    fn copy_address(&mut self, action: &CopyAddress, _: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
    }

    fn copy_value(&mut self, action: &CopyValue, _: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
    }

    fn focus_table(&mut self, _: &FocusTable, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handle) = &self.table_state.focus_handle {
            handle.focus(window, cx);
        }
        cx.notify();
    }

    fn clear_results(&mut self, _: &ClearResults, _: &mut Window, cx: &mut Context<Self>) {
        self.invalidate_scan();
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
                hover_bg: theme.selection.opacity(0.7),
                address_color: theme.accent_foreground,
                size_color: theme.accent_foreground,
                value_color: theme.accent_foreground,
            }
        } else {
            StringsRowColors {
                bg_color: theme.muted,
                border_color: theme.muted_foreground.opacity(0.4),
                hover_bg: theme.muted.opacity(0.8),
                address_color: theme.foreground,
                size_color: theme.muted_foreground,
                value_color: theme.foreground,
            }
        }
    } else {
        StringsRowColors {
            bg_color: theme.sidebar,
            border_color: theme.border.opacity(0.5),
            hover_bg: theme.selection.opacity(0.3),
            address_color: theme.foreground,
            size_color: theme.muted_foreground,
            value_color: theme.foreground,
        }
    }
}

impl Render for StringsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let table_overlay = VirtualTable::render_table_overlay(&self.table_state, cx, |this| &mut this.table_state);

        let theme = cx.theme();
        let font_family = cx.global::<Appearance>().font_family.clone();
        let is_table_focused = self.table_state.focus_handle.as_ref().is_some_and(|h| h.is_focused(window));
        let is_focused = self.focus_handle.is_focused(window) || is_table_focused;
        let has_editor = self.editor.is_some();
        let minimum_length_is_valid = self.minimum_length(cx).is_some();
        let visible_indices = self.sorted_indices(cx);

        let actions = h_flex().items_center().gap_1().child(
            Button::new("clear-strings")
                .ghost()
                .icon(IconName::Eraser)
                .with_size(Size::XSmall)
                .tooltip("Clear results")
                .disabled(!self.has_scanned && !self.is_scanning)
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.invalidate_scan();
                    cx.notify();
                })),
        );
        let header = crate::ui::panels::panel_header("STRINGS", is_focused, theme, None, Some(actions.into_any_element()));

        let minimum_length = self.minimum_length(cx).unwrap_or(DEFAULT_MIN_STRING_LENGTH);
        let scan_controls = h_flex()
            .p_2()
            .gap_1()
            .border_b_1()
            .border_color(theme.border)
            .items_center()
            .child(div().text_xs().text_color(theme.muted_foreground).child("Min chars"))
            .child(div().w_16().child(Input::new(&self.min_length_input)))
            .child(
                Button::new("strings-min-decrease")
                    .ghost()
                    .icon(IconName::Minus)
                    .with_size(Size::XSmall)
                    .tooltip("Decrease minimum length")
                    .disabled(!has_editor || self.is_scanning || minimum_length <= 1)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.adjust_minimum_length(-1, window, cx);
                    })),
            )
            .child(
                Button::new("strings-min-increase")
                    .ghost()
                    .icon(IconName::Plus)
                    .with_size(Size::XSmall)
                    .tooltip("Increase minimum length")
                    .disabled(!has_editor || self.is_scanning)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.adjust_minimum_length(1, window, cx);
                    })),
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
            .bg(theme.accent.opacity(0.1))
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
            let total_visible_width = self.table_state.total_visible_width();
            let scroll_offset_x = self.table_state.scroll_offset_x;
            let visible_cols = self.table_state.visible_columns().map(|(ix, col)| (ix, col.width)).collect::<Vec<_>>();

            let header_row = VirtualTable::render_header_row(
                &self.table_state,
                "strings-column-header",
                theme,
                cx,
                None::<fn(usize, &TableColumn) -> Option<AnyElement>>,
                Self::auto_fit_column,
                Some(Self::sort_by),
                |this| &mut this.table_state,
            );

            let row_count = visible_indices.len();
            let view = cx.entity().clone();
            let list_view = view.clone();
            let context_focus_handle = self.focus_handle.clone();

            let list = uniform_list("strings-virtual-table", row_count, move |visible_range, window, cx| {
                let this = list_view.read(cx);
                let theme = cx.theme();
                let address_map = this
                    .editor
                    .as_ref()
                    .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.address_map.clone()))
                    .unwrap_or_default();

                visible_range
                    .map(|row_ix| {
                        if let Some(&index) = visible_indices.get(row_ix)
                            && let Some(item) = this.results.get(index)
                        {
                            let is_selected = this.selected_index == Some(index);
                            let colors = strings_row_colors(is_selected, is_table_focused, theme);

                            let preview = preview_text(&item.text);
                            let offset = item.offset;
                            let byte_len = item.byte_len;
                            let offset_value = format!("0x{:08X}", address_map.offset_to_address(offset));

                            h_flex()
                                .id(("strings-result-item", index))
                                .w_full()
                                .h(px(STRINGS_ROW_HEIGHT))
                                .flex_shrink_0()
                                .overflow_hidden()
                                .border_b_1()
                                .border_color(colors.border_color)
                                .bg(colors.bg_color)
                                .cursor_pointer()
                                .hover(move |style| style.bg(colors.hover_bg))
                                .on_click(window.listener_for(&list_view, move |this, _, window, cx| {
                                    if let Some(handle) = &this.table_state.focus_handle {
                                        handle.focus(window, cx);
                                    }
                                    this.select_item(index, cx);
                                }))
                                .on_mouse_down(
                                    MouseButton::Right,
                                    window.listener_for(&list_view, move |this, _, window, cx| {
                                        if let Some(handle) = &this.table_state.focus_handle {
                                            handle.focus(window, cx);
                                        }
                                        this.select_item(index, cx);
                                    }),
                                )
                                .child(
                                    h_flex()
                                        .w(total_visible_width)
                                        .h_full()
                                        .ml(-scroll_offset_x)
                                        .children(visible_cols.iter().enumerate().map(|(vis_ix, &(col_ix, width))| {
                                            let is_first = vis_ix == 0;
                                            let cell_content = match col_ix {
                                                0 => div()
                                                    .text_xs()
                                                    .font_family(font_family.clone())
                                                    .text_color(colors.address_color)
                                                    .child(offset_value.clone())
                                                    .into_any_element(),
                                                1 => div()
                                                    .text_xs()
                                                    .font_family(font_family.clone())
                                                    .text_color(colors.size_color)
                                                    .child(byte_len.to_string())
                                                    .into_any_element(),
                                                2 => div()
                                                    .text_xs()
                                                    .font_family(font_family.clone())
                                                    .text_color(colors.value_color)
                                                    .child(preview.clone())
                                                    .into_any_element(),
                                                _ => div().into_any_element(),
                                            };
                                            VirtualTable::render_data_cell(col_ix, width, is_first, theme.border.opacity(0.35), cell_content)
                                        })),
                                )
                                .into_any_element()
                        } else {
                            div().into_any_element()
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .track_scroll(&self.table_state.vertical_scroll_handle)
            .size_full();

            let horizontal_scrollbar = VirtualTable::render_horizontal_scrollbar(&self.table_state, self.last_container_width, theme);
            let vertical_scrollbar = VirtualTable::render_vertical_scrollbar(&self.table_state, theme);
            let context_view = view.clone();

            let table_focus_handle = self.table_state.focus_handle.clone();
            let mut table_container = v_flex()
                .id(self.table_state.id.clone())
                .key_context(table::CONTEXT)
                .flex_1()
                .overflow_hidden()
                .relative();

            if let Some(focus_handle) = &table_focus_handle {
                table_container = table_container.track_focus(focus_handle).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        if let Some(handle) = &this.table_state.focus_handle {
                            handle.focus(window, cx);
                        }
                    }),
                );
            }

            table_container = table_container
                .on_action(cx.listener(Self::table_move_up))
                .on_action(cx.listener(Self::table_move_down))
                .on_action(cx.listener(Self::table_move_top))
                .on_action(cx.listener(Self::table_move_bottom))
                .on_action(cx.listener(Self::table_page_up))
                .on_action(cx.listener(Self::table_page_down))
                .on_action(cx.listener(Self::table_select_current))
                .on_action(cx.listener(Self::table_dismiss));

            table_container
                .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _window, cx| {
                    if this.table_state.resizing_column.is_some() {
                        if event.pressed_button != Some(MouseButton::Left) {
                            this.table_state.end_resize();
                            cx.notify();
                        } else {
                            this.table_state.update_resize(event.position.x);
                            cx.notify();
                        }
                    }
                }))
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                        if this.table_state.resizing_column.is_some() {
                            this.table_state.end_resize();
                            cx.notify();
                        }
                    }),
                )
                .on_mouse_up_out(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseUpEvent, _window, cx| {
                        if this.table_state.resizing_column.is_some() {
                            this.table_state.end_resize();
                            cx.notify();
                        }
                    }),
                )
                .child(table_overlay)
                .child(header_row)
                .child(
                    div()
                        .id("strings-results-container")
                        .flex_1()
                        .overflow_hidden()
                        .relative()
                        .child(list)
                        .child(vertical_scrollbar)
                        .children(horizontal_scrollbar),
                )
                .context_menu(move |menu, _window, cx| {
                    let selected_info = {
                        let this = context_view.read(cx);
                        this.selected_index.and_then(|idx| {
                            let item = this.results.get(idx)?;
                            let address_map = this
                                .editor
                                .as_ref()
                                .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.address_map.clone()))
                                .unwrap_or_default();
                            let offset_value = format!("0x{:08X}", address_map.offset_to_address(item.offset));
                            let value = item.text.clone();
                            Some((offset_value, value))
                        })
                    };
                    let Some((menu_offset, value)) = selected_info else {
                        return menu;
                    };
                    menu.action_context(context_focus_handle.clone())
                        .menu_with_icon("Copy Address", IconName::Hash, Box::new(CopyAddress { value: menu_offset }))
                        .menu_with_icon("Copy Value", IconName::TextInitial, Box::new(CopyValue { value }))
                })
                .into_any_element()
        };

        crate::ui::panels::panel_container(is_focused, theme)
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::focus_table))
            .on_action(cx.listener(Self::copy_address))
            .on_action(cx.listener(Self::copy_value))
            .on_action(cx.listener(Self::clear_results))
            .child(header)
            .child(scan_controls)
            .child(result_summary)
            .child(filter_controls)
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
        assert_eq!(colors.border_color, theme.border.opacity(0.5));
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
        assert_eq!(colors.border_color, theme.muted_foreground.opacity(0.4));
        assert_eq!(colors.address_color, theme.foreground);
        assert_eq!(colors.size_color, theme.muted_foreground);
        assert_eq!(colors.value_color, theme.foreground);
    }

    #[test]
    fn test_strings_address_contrast_across_all_embedded_themes() {
        use super::strings_row_colors;
        use crate::assets::Assets;
        use crate::theme::EmbeddedThemes;
        use gpui_kit::{Hsla, Rgba};

        let embedded = EmbeddedThemes::load_from_assets(&Assets);
        assert!(!embedded.theme_names().is_empty());

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

        for name in embedded.theme_names() {
            let config = embedded.get(&name).unwrap();
            let mut theme = super::Theme::default();
            theme.apply_config(&config);

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

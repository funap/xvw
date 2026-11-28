use crate::core::address_map::AddressMap;
use crate::core::appearance::Appearance;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::format::HexCase;
use crate::core::strings::{DEFAULT_MIN_STRING_LENGTH, StringMatch, find_strings_segmented_limited};
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
use std::sync::Arc;

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
        KeyBinding::new("escape", DismissTable, Some("StringsPanel")),
        KeyBinding::new("escape", DismissTable, Some("StringsPanel > DataTable")),
        KeyBinding::new("enter", ConfirmSelect, Some("StringsPanel > DataTable")),
        KeyBinding::new("down", FocusTable, Some("StringsPanel > Input")),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ScanSignature {
    encoding: Encoding,
    content_state: usize,
    byte_len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct StringCellMeta {
    size_label: SharedString,
    preview: SharedString,
    preview_char_count: usize,
}

fn build_string_cell_meta(results: &[StringMatch]) -> Arc<[StringCellMeta]> {
    results
        .iter()
        .map(|item| {
            let preview_cow = preview_text(&item.text);
            let preview_char_count = preview_cow.chars().count();
            let preview = match preview_cow {
                std::borrow::Cow::Borrowed(_) => SharedString::from(item.text.clone()),
                std::borrow::Cow::Owned(s) => SharedString::from(s),
            };
            StringCellMeta {
                size_label: SharedString::from(item.byte_len.to_string()),
                preview,
                preview_char_count,
            }
        })
        .collect::<Vec<_>>()
        .into()
}

struct AddressLabelCache {
    results_id: u64,
    editor_id: Option<EntityId>,
    hex_case: HexCase,
    address_map: AddressMap,
    labels: Vec<Option<SharedString>>,
}

impl AddressLabelCache {
    fn new(results_id: u64, editor_id: Option<EntityId>, hex_case: HexCase, address_map: AddressMap, len: usize) -> Self {
        Self {
            results_id,
            editor_id,
            hex_case,
            address_map,
            labels: vec![None; len],
        }
    }

    fn matches(&self, results_id: u64, editor_id: Option<EntityId>, hex_case: HexCase, len: usize) -> bool {
        self.results_id == results_id && self.editor_id == editor_id && self.hex_case == hex_case && self.labels.len() == len
    }

    fn get_or_format(&mut self, index: usize, offset: usize) -> SharedString {
        if let Some(Some(cached)) = self.labels.get(index) {
            return cached.clone();
        }
        let formatted: SharedString = self.hex_case.format_prefixed_pad(self.address_map.offset_to_address(offset), 8).into();
        if let Some(slot) = self.labels.get_mut(index) {
            *slot = Some(formatted.clone());
        }
        formatted
    }
}

struct CachedStringsState {
    results_id: u64,
    results: Arc<[StringMatch]>,
    cell_meta: Arc<[StringCellMeta]>,
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

#[inline]
fn contains_ignore_case(haystack: &str, lowercase_needle: &str) -> bool {
    if lowercase_needle.is_empty() {
        return true;
    }
    if haystack.is_ascii() && lowercase_needle.is_ascii() {
        let h = haystack.as_bytes();
        let n = lowercase_needle.as_bytes();
        return h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n));
    }
    haystack.to_lowercase().contains(lowercase_needle)
}

fn sort_string_indices(results: &[StringMatch], indices: &mut [usize], sort_column: Option<(usize, ColumnSort)>) {
    let Some((col_ix, sort)) = sort_column else {
        indices.sort_unstable();
        return;
    };
    if sort == ColumnSort::Default {
        indices.sort_unstable();
        return;
    }

    indices.sort_by(|left, right| {
        let left_match = &results[*left];
        let right_match = &results[*right];
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

pub struct StringsTableDelegate {
    panel: WeakEntity<StringsPanel>,
    results_id: u64,
    results: Arc<[StringMatch]>,
    cell_meta: Arc<[StringCellMeta]>,
    visible_indices: Vec<usize>,
    sort_column: Option<(usize, ColumnSort)>,
    editor: Option<Entity<Editor>>,
    address_cache: Option<AddressLabelCache>,
    selected_index: Option<usize>,
    focus_handle: FocusHandle,
}

impl StringsTableDelegate {
    fn sort_indices(&mut self) {
        sort_string_indices(&self.results, &mut self.visible_indices, self.sort_column);
    }

    fn ensure_address_cache(&mut self, hex_case: HexCase, cx: &App) -> &mut AddressLabelCache {
        let editor_id = self.editor.as_ref().map(Entity::entity_id);
        let len = self.results.len();
        let needs_rebuild = !self
            .address_cache
            .as_ref()
            .is_some_and(|cache| cache.matches(self.results_id, editor_id, hex_case, len));
        if needs_rebuild {
            let address_map = self
                .editor
                .as_ref()
                .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.address_map.clone()))
                .unwrap_or_default();
            self.address_cache = Some(AddressLabelCache::new(self.results_id, editor_id, hex_case, address_map, len));
        }
        self.address_cache.as_mut().expect("address cache initialized")
    }

    fn cached_address_label(&mut self, index: usize, hex_case: HexCase, cx: &App) -> Option<SharedString> {
        let offset = self.results.get(index)?.offset;
        let cache = self.ensure_address_cache(hex_case, cx);
        Some(cache.get_or_format(index, offset))
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
        let max_len = match col_ix {
            0 => {
                let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
                let mut max_chars = 0usize;
                for row_pos in 0..self.visible_indices.len().min(128) {
                    let index = self.visible_indices[row_pos];
                    if let Some(label) = self.cached_address_label(index, hex_case, cx) {
                        max_chars = max_chars.max(label.chars().count());
                    }
                }
                max_chars
            }
            1 => self
                .visible_indices
                .iter()
                .take(128)
                .filter_map(|&index| self.cell_meta.get(index))
                .map(|meta| meta.size_label.len())
                .max()
                .unwrap_or(0),
            2 => self
                .visible_indices
                .iter()
                .take(128)
                .filter_map(|&index| self.cell_meta.get(index))
                .map(|meta| meta.preview_char_count)
                .max()
                .unwrap_or(0),
            _ => 0,
        };

        Some(px(max_len as f32 * 7.5 + 24.0))
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(&index) = self.visible_indices.get(row_ix) else {
            return menu;
        };
        let text_value = match self.results.get(index) {
            Some(item) => item.text.clone(),
            None => return menu,
        };
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let offset_value = self
            .cached_address_label(index, hex_case, cx)
            .map(|label| label.to_string())
            .unwrap_or_default();

        menu.action_context(self.focus_handle.clone())
            .menu_with_icon("Copy Address", IconName::Hash, Box::new(CopyAddress { value: offset_value }))
            .menu_with_icon("Copy Value", IconName::Copy, Box::new(CopyValue { value: text_value }))
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        if col_ix > 2 {
            return div().into_any_element();
        }
        let Some(&index) = self.visible_indices.get(row_ix) else {
            return div().into_any_element();
        };
        if index >= self.results.len() {
            return div().into_any_element();
        }

        let theme = cx.theme();
        let font_family = cx.global::<Appearance>().font_family.clone();
        let is_selected = self.selected_index == Some(index);
        let colors = strings_row_colors(is_selected, false, theme);

        let (color, text): (Hsla, SharedString) = match col_ix {
            0 => {
                let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
                let Some(addr) = self.cached_address_label(index, hex_case, cx) else {
                    return div().into_any_element();
                };
                (colors.address_color, addr)
            }
            1 => {
                let Some(meta) = self.cell_meta.get(index) else {
                    return div().into_any_element();
                };
                (colors.size_color, meta.size_label.clone())
            }
            2 => {
                let Some(meta) = self.cell_meta.get(index) else {
                    return div().into_any_element();
                };
                (colors.value_color, meta.preview.clone())
            }
            _ => return div().into_any_element(),
        };

        div().text_xs().font_family(font_family).text_color(color).child(text).into_any_element()
    }
}

pub struct StringsPanel {
    pub editor: Option<Entity<Editor>>,
    pub focus_handle: FocusHandle,
    pub table_state: Entity<TableState<StringsTableDelegate>>,
    pub min_length_input: Entity<InputState>,
    pub filter_input: Entity<InputState>,
    pub is_scanning: bool,
    pub results: Arc<[StringMatch]>,
    cell_meta: Arc<[StringCellMeta]>,
    results_id: u64,
    next_results_id: u64,
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
    _hex_case_subscription: Subscription,
}

impl EventEmitter<StringsPanelEvent> for StringsPanel {}

impl StringsPanel {
    pub fn new(editor: Option<Entity<Editor>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let view = cx.entity().downgrade();
        let delegate = StringsTableDelegate {
            panel: view,
            results_id: 0,
            results: Arc::default(),
            cell_meta: Arc::default(),
            visible_indices: Vec::new(),
            sort_column: None,
            editor: None,
            address_cache: None,
            selected_index: None,
            focus_handle: focus_handle.clone(),
        };
        let table_state = cx.new(|cx| {
            TableState::new(delegate, window, cx)
                .row_selectable(true)
                .col_selectable(false)
                .col_resizable(true)
                .col_movable(false)
                .loop_selection(false)
        });

        let table_subscription = cx.subscribe_in(&table_state, window, |this, _, event: &TableEvent, window, cx| match event {
            TableEvent::SelectRow(row_ix) => {
                if let Some(index) = this.resolve_visible_row_index(*row_ix, cx)
                    && this.selected_index != Some(index)
                {
                    this.select_item_at_row(index, Some(*row_ix), true, cx);
                }
            }
            TableEvent::DoubleClickedRow(row_ix) => {
                if let Some(index) = this.resolve_visible_row_index(*row_ix, cx) {
                    this.select_item_at_row(index, Some(*row_ix), false, cx);
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

        let input_subscription = cx.subscribe_in(&min_length_input, window, |this, _, event: &input::InputEvent, _window, cx| match event {
            input::InputEvent::PressEnter { .. } => {
                this.trigger_scan(cx);
            }
            input::InputEvent::Change => {
                this.invalidate_scan();
                this.sync_selection_to_filter(cx);
                cx.notify();
            }
            _ => {}
        });

        let filter_subscription = cx.subscribe_in(&filter_input, window, |this, _, event: &input::InputEvent, _window, cx| {
            if let input::InputEvent::Change = event {
                this.sync_selection_to_filter(cx);
                cx.notify();
            }
        });

        let hex_case_subscription = cx.observe_global::<HexCase>(|this, cx| {
            this.table_state.update(cx, |table, cx| {
                table.refresh(cx);
            });
            cx.notify();
        });

        let mut this = Self {
            editor: None,
            focus_handle,
            table_state,
            min_length_input,
            filter_input,
            is_scanning: false,
            results: Arc::default(),
            cell_meta: Arc::default(),
            results_id: 0,
            next_results_id: 1,
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
            _hex_case_subscription: hex_case_subscription,
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

    fn alloc_results_id(&mut self) -> u64 {
        let id = self.next_results_id;
        self.next_results_id = self.next_results_id.wrapping_add(1).max(1);
        id
    }

    fn cache_current_state(&mut self) {
        let Some(editor) = &self.editor else {
            return;
        };

        self.cached_states.insert(
            editor.entity_id(),
            CachedStringsState {
                results_id: self.results_id,
                results: std::mem::take(&mut self.results),
                cell_meta: std::mem::take(&mut self.cell_meta),
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

        self.results_id = cached.results_id;
        self.results = cached.results;
        self.cell_meta = cached.cell_meta;
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
            return;
        }
        self.invalidate_scan();
        self.scan_signature = signature;
        self.sync_selection_to_filter(cx);
        cx.notify();
    }

    fn invalidate_scan(&mut self) {
        self.scan_generation = self.scan_generation.wrapping_add(1);
        self.scan_task = None;
        self.is_scanning = false;
        self.results_id = self.alloc_results_id();
        self.results = Arc::default();
        self.cell_meta = Arc::default();
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
            let (results, cell_meta, is_truncated) = cx
                .background_executor()
                .spawn(async move {
                    let (results, is_truncated) = find_strings_segmented_limited(buffer_data.data(), &segment_ranges, encoding, min_length, MAX_STRING_RESULTS);
                    let cell_meta = build_string_cell_meta(&results);
                    (Arc::from(results), cell_meta, is_truncated)
                })
                .await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if this.scan_generation != generation {
                        return;
                    }

                    this.scan_task = None;
                    this.is_scanning = false;
                    this.is_truncated = is_truncated;
                    this.results_id = this.alloc_results_id();
                    this.results = results;
                    this.cell_meta = cell_meta;
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
            .filter(|(_, item)| contains_ignore_case(&item.text, &filter))
            .map(|(index, _)| index)
            .collect()
    }

    fn sorted_indices(&self, cx: &App) -> Vec<usize> {
        let mut indices = self.filtered_indices(cx);
        if self.sort_column.is_some_and(|(_, sort)| sort != ColumnSort::Default) {
            sort_string_indices(&self.results, &mut indices, self.sort_column);
        }
        indices
    }

    fn resolve_visible_row_index(&self, row_ix: usize, cx: &App) -> Option<usize> {
        self.table_state.read(cx).delegate().visible_indices.get(row_ix).copied()
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
        let results_id = self.results_id;
        let results = Arc::clone(&self.results);
        let cell_meta = Arc::clone(&self.cell_meta);
        let editor = self.editor.clone();
        let sort_column = self.sort_column;
        let selected_index = self.selected_index;

        self.table_state.update(cx, |table, cx| {
            let delegate = table.delegate_mut();
            let editor_changed = delegate.editor.as_ref().map(Entity::entity_id) != editor.as_ref().map(Entity::entity_id);
            if delegate.results_id != results_id || editor_changed {
                delegate.address_cache = None;
            }
            delegate.results_id = results_id;
            delegate.results = results;
            delegate.cell_meta = cell_meta;
            delegate.visible_indices = visible_indices;
            delegate.sort_column = sort_column;
            delegate.editor = editor;
            delegate.selected_index = selected_index;
            table.refresh(cx);
            if let Some(row_ix) = row_ix {
                table.set_selected_row(row_ix, cx);
                table.scroll_to_row(row_ix, cx);
            } else {
                table.clear_selection(cx);
            }
        });
    }

    fn select_item_at_row(&mut self, index: usize, known_row_ix: Option<usize>, from_table_selection: bool, cx: &mut Context<Self>) {
        if let Some(item) = self.results.get(index) {
            let selection_changed = self.selected_index != Some(index);
            self.selected_index = Some(index);
            let offset = item.offset;
            let len = item.byte_len;

            self.table_state.update(cx, |table, cx| {
                let delegate = table.delegate_mut();
                if delegate.selected_index != Some(index) {
                    delegate.selected_index = Some(index);
                }
                if !from_table_selection {
                    let row_ix = known_row_ix
                        .filter(|&row| delegate.visible_indices.get(row) == Some(&index))
                        .or_else(|| delegate.visible_indices.iter().position(|&i| i == index));
                    if let Some(row_ix) = row_ix
                        && table.selected_row() != Some(row_ix)
                    {
                        table.set_selected_row(row_ix, cx);
                        table.scroll_to_row(row_ix, cx);
                    }
                }
            });

            if let Some(ed) = &self.editor {
                ed.update(cx, |editor, cx| {
                    let prev_cursor = editor.cursor.offset;
                    let prev_selection = editor.selection_range();
                    if len > 0 {
                        editor.set_selection_range(offset..offset.saturating_add(len));
                    } else {
                        editor.set_cursor_offset(offset);
                    }
                    if editor.cursor.offset != prev_cursor || editor.selection_range() != prev_selection {
                        cx.notify();
                    }
                });
            }
            cx.emit(StringsPanelEvent::NavigateTo { offset, len });
            if selection_changed && !from_table_selection {
                cx.notify();
            }
        } else if !from_table_selection {
            cx.notify();
        }
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
        if !self.table_state.read(cx).delegate().visible_indices.is_empty() {
            let focus_handle = self.table_state.read(cx).focus_handle(cx);
            focus_handle.focus(window, cx);
            cx.notify();
        }
    }

    fn confirm_select(&mut self, _: &ConfirmSelect, window: &mut Window, cx: &mut Context<Self>) {
        let target = {
            let table = self.table_state.read(cx);
            let visible_indices = &table.delegate().visible_indices;
            self.selected_index
                .and_then(|index| visible_indices.iter().position(|&i| i == index).map(|row| (index, row)))
                .or_else(|| visible_indices.first().copied().map(|index| (index, 0)))
        };
        if let Some((index, row_ix)) = target {
            self.select_item_at_row(index, Some(row_ix), false, cx);
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
                border_color: theme.list_active_border,
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
        let total_count = self.results.len();
        let visible_count = self.table_state.read(cx).delegate().visible_indices.len();
        let is_filtered = !self.filter_query(cx).is_empty();

        let badge = if self.is_scanning {
            Some(crate::ui::panels::panel_badge("Scanning...", theme).into_any_element())
        } else if self.has_scanned {
            let text = if is_filtered {
                if self.is_truncated {
                    format!("{visible_count}/{MAX_STRING_RESULTS}+")
                } else {
                    format!("{visible_count}/{total_count}")
                }
            } else if self.is_truncated {
                format!("{MAX_STRING_RESULTS}+")
            } else {
                total_count.to_string()
            };
            Some(crate::ui::panels::panel_badge(text, theme).into_any_element())
        } else {
            None
        };

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
        let header = crate::ui::panels::panel_header("STRINGS", is_focused, theme, badge, Some(actions.into_any_element()));

        let scan_button = if self.is_scanning {
            Button::new("cancel-strings-scan")
                .label("Cancel")
                .danger()
                .with_size(Size::Small)
                .tooltip("Cancel string scan")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.invalidate_scan();
                    this.sync_selection_to_filter(cx);
                    cx.notify();
                }))
        } else {
            Button::new("scan-strings")
                .label("Scan")
                .primary()
                .with_size(Size::Small)
                .disabled(!has_editor || !minimum_length_is_valid)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.trigger_scan(cx);
                }))
        };

        let scan_controls = h_flex()
            .p_2()
            .gap_2()
            .flex_wrap()
            .border_b_1()
            .border_color(theme.border)
            .items_center()
            .child(div().text_xs().text_color(theme.muted_foreground).whitespace_nowrap().child("Min chars"))
            .child(
                div().w_24().child(
                    NumberInput::new(&self.min_length_input)
                        .with_size(Size::Small)
                        .disabled(!has_editor || self.is_scanning),
                ),
            )
            .child(scan_button);

        let truncation_notice = (self.has_scanned && !self.is_scanning && self.is_truncated).then(|| {
            div()
                .w_full()
                .px_2()
                .py_1()
                .bg(theme.secondary)
                .border_b_1()
                .border_color(theme.border)
                .text_xs()
                .text_color(theme.muted_foreground)
                .truncate()
                .child(format!("{}+ strings found (showing first {} entries)", MAX_STRING_RESULTS, MAX_STRING_RESULTS))
        });

        let filter_controls = (self.has_scanned && !self.is_scanning && !self.results.is_empty()).then(|| {
            h_flex().w_full().p_2().items_center().border_b_1().border_color(theme.border).child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(Input::new(&self.filter_input).prefix(IconName::Search).cleanable(true).with_size(Size::Small)),
            )
        });

        let body = if !has_editor {
            crate::ui::panels::panel_empty_state(
                IconName::TextInitial,
                "No Active File",
                Some("Open a binary file to extract printable strings"),
                None,
                theme,
            )
            .into_any_element()
        } else if self.is_scanning {
            let cancel_btn = Button::new("empty-cancel-strings-scan")
                .label("Cancel Scan")
                .ghost()
                .with_size(Size::Small)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.invalidate_scan();
                    this.sync_selection_to_filter(cx);
                    cx.notify();
                }));
            crate::ui::panels::panel_empty_state(
                IconName::LoaderCircle,
                "Scanning Strings...",
                Some("Extracting printable strings in the background"),
                Some(cancel_btn.into_any_element()),
                theme,
            )
            .into_any_element()
        } else if self.results.is_empty() {
            if self.has_scanned {
                crate::ui::panels::panel_empty_state(
                    IconName::TextInitial,
                    "No Strings Found",
                    Some("Try lowering Min chars or switching the text encoding"),
                    None,
                    theme,
                )
                .into_any_element()
            } else {
                crate::ui::panels::panel_empty_state(
                    IconName::TextInitial,
                    "Ready to Scan",
                    Some("Click Scan to find printable strings in this file"),
                    None,
                    theme,
                )
                .into_any_element()
            }
        } else if visible_count == 0 {
            let clear_filter_btn = Button::new("clear-strings-filter-empty")
                .label("Clear Filter")
                .ghost()
                .with_size(Size::Small)
                .on_click(cx.listener(|this, _, window, cx| {
                    this.filter_input.update(cx, |input, cx| {
                        input.set_value(String::new(), window, cx);
                    });
                    this.sync_selection_to_filter(cx);
                    cx.notify();
                }));
            crate::ui::panels::panel_empty_state(
                IconName::Search,
                "No Matching Strings",
                Some("No scanned strings match the current filter"),
                Some(clear_filter_btn.into_any_element()),
                theme,
            )
            .into_any_element()
        } else {
            div()
                .flex_1()
                .min_h_0()
                .overflow_hidden()
                .child(DataTable::new(&self.table_state).bordered(false))
                .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        this.dismiss_table(&DismissTable, window, cx);
                        cx.stop_propagation();
                    }
                }))
                .into_any_element()
        };

        crate::ui::panels::panel_container(is_focused, theme)
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    if !this.min_length_input.read(cx).focus_handle(cx).is_focused(window) && !this.filter_input.read(cx).focus_handle(cx).is_focused(window) {
                        this.focus_handle.focus(window, cx);
                    }
                }),
            )
            .on_action(cx.listener(Self::focus_table))
            .on_action(cx.listener(Self::confirm_select))
            .on_action(cx.listener(Self::dismiss_table))
            .on_action(cx.listener(|this, _: &gpui_kit::base::actions::Cancel, window, cx| {
                this.dismiss_table(&DismissTable, window, cx);
            }))
            .on_action(cx.listener(Self::copy_address))
            .on_action(cx.listener(Self::copy_value))
            .on_action(cx.listener(Self::clear_results))
            .child(header)
            .child(scan_controls)
            .children(truncation_notice)
            .children(filter_controls)
            .child(body)
    }
}

fn preview_text(text: &str) -> std::borrow::Cow<'_, str> {
    const PREVIEW_LIMIT: usize = 256;
    if text.len() <= PREVIEW_LIMIT {
        return std::borrow::Cow::Borrowed(text);
    }
    if let Some((end_byte, _)) = text.char_indices().nth(PREVIEW_LIMIT) {
        let mut preview = String::with_capacity(end_byte + '…'.len_utf8());
        preview.push_str(&text[..end_byte]);
        preview.push('…');
        std::borrow::Cow::Owned(preview)
    } else {
        std::borrow::Cow::Borrowed(text)
    }
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

        let utf8_exact = "あ".repeat(256);
        assert!(matches!(preview_text(&utf8_exact), std::borrow::Cow::Borrowed(_)));

        let utf8_over = "あ".repeat(260);
        let utf8_preview = preview_text(&utf8_over);
        assert_eq!(utf8_preview.chars().count(), 257);
        assert!(utf8_preview.ends_with('…'));
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
        assert_eq!(colors.border_color, theme.list_active_border);
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

    #[test]
    fn test_strings_panel_event_focus_editor() {
        let event = super::StringsPanelEvent::FocusEditor;
        assert_eq!(event, super::StringsPanelEvent::FocusEditor);

        let parent_ctx = gpui_kit::KeyContext::try_from("StringsPanel").unwrap();
        let child_ctx = gpui_kit::KeyContext::try_from("DataTable").unwrap();
        let contexts = vec![parent_ctx, child_ctx];

        let pred_desc = gpui_kit::KeyBindingContextPredicate::parse("StringsPanel > DataTable").unwrap();
        assert!(pred_desc.eval(&contexts));
        assert_eq!(pred_desc.depth_of(&contexts), Some(2));
    }

    #[test]
    fn test_contains_ignore_case_ascii_and_unicode() {
        use super::contains_ignore_case;
        assert!(contains_ignore_case("HelloWorld", ""));
        assert!(contains_ignore_case("HelloWorld", "world"));
        assert!(!contains_ignore_case("Hi", "hello"));
        assert!(contains_ignore_case("Café_MENU", "café"));
        assert!(contains_ignore_case("日本語Test", "test"));
    }

    #[test]
    fn test_sort_string_indices_all_columns() {
        use super::{ColumnSort, StringMatch, sort_string_indices};
        let results = vec![
            StringMatch {
                offset: 0x30,
                byte_len: 3,
                text: "bbb".into(),
            },
            StringMatch {
                offset: 0x10,
                byte_len: 10,
                text: "ccc".into(),
            },
            StringMatch {
                offset: 0x20,
                byte_len: 5,
                text: "aaa".into(),
            },
        ];

        let mut indices = vec![0, 1, 2];
        sort_string_indices(&results, &mut indices, Some((0, ColumnSort::Ascending)));
        assert_eq!(indices, vec![1, 2, 0]);

        sort_string_indices(&results, &mut indices, Some((1, ColumnSort::Descending)));
        assert_eq!(indices, vec![1, 2, 0]);

        sort_string_indices(&results, &mut indices, Some((2, ColumnSort::Ascending)));
        assert_eq!(indices, vec![2, 0, 1]);

        sort_string_indices(&results, &mut indices, Some((2, ColumnSort::Default)));
        assert_eq!(indices, vec![0, 1, 2]);
    }

    #[test]
    fn test_build_string_cell_meta_precomputes_size_and_truncated_preview() {
        use super::{StringMatch, build_string_cell_meta};

        let results = [
            StringMatch {
                offset: 0x10,
                byte_len: 5,
                text: "hello".into(),
            },
            StringMatch {
                offset: 0x20,
                byte_len: 300,
                text: "あ".repeat(260),
            },
        ];

        let meta = build_string_cell_meta(&results);
        assert_eq!(meta.len(), 2);
        assert_eq!(meta[0].size_label.as_ref(), "5");
        assert_eq!(meta[0].preview.as_ref(), "hello");
        assert_eq!(meta[0].preview_char_count, 5);

        assert_eq!(meta[1].size_label.as_ref(), "300");
        assert_eq!(meta[1].preview_char_count, 257);
        assert!(meta[1].preview.ends_with('…'));
    }

    #[test]
    fn test_address_label_cache_caches_hits_and_invalidates_on_case_or_results_change() {
        use super::{AddressLabelCache, AddressMap, HexCase};
        use crate::core::address_map::MemorySegment;

        let map = AddressMap::from_segments(vec![MemorySegment {
            buffer_offset: 0,
            address: 0x0800_0000,
            length: 64,
        }]);
        let mut cache = AddressLabelCache::new(1, None, HexCase::Lower, map, 2);
        assert!(cache.matches(1, None, HexCase::Lower, 2));
        assert!(!cache.matches(1, None, HexCase::Upper, 2));
        assert!(!cache.matches(2, None, HexCase::Lower, 2));
        assert!(!cache.matches(1, None, HexCase::Lower, 3));

        let label = cache.get_or_format(1, 0x1A);
        assert_eq!(label.as_ref(), "0x0800001a");
        assert!(cache.labels[0].is_none());
        assert_eq!(cache.labels[1].as_ref().map(|s| s.as_ref()), Some("0x0800001a"));
        assert_eq!(cache.get_or_format(1, 0x1A).as_ref(), "0x0800001a");
    }
}

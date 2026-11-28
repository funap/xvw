use crate::core::appearance::Appearance;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::format::HexCase;
use crate::core::search::{SearchMode, TextSearchFlags, compile_search};
use crate::ui::icon::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::table::{Column, DataTable, TableDelegate, TableEvent, TableState};
use gpui_kit::component::{ActiveTheme as _, Disableable, Sizable, Size, StyledExt, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub trait SearchModeExt {
    fn placeholder(&self) -> &'static str;
}

impl SearchModeExt for SearchMode {
    fn placeholder(&self) -> &'static str {
        match self {
            SearchMode::Hex => "Hex (e.g. 48 89 ?? 24, FF, ?)...",
            SearchMode::Text => "Text (e.g. hello, magic, HTTP)...",
        }
    }
}

actions!(search_panel, [FocusTable, ClearResults, ConfirmSelect, DismissTable]);

#[derive(Clone, PartialEq, Action)]
#[action(namespace = search_panel, no_json)]
struct CopyAddress {
    value: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = search_panel, no_json)]
struct CopyValue {
    value: String,
}

#[derive(Clone, PartialEq, Action)]
#[action(namespace = search_panel, no_json)]
struct CopyText {
    value: String,
}

const CONTEXT: &str = "SearchPanel";
pub const MAX_SEARCH_RESULTS: usize = 10_000;

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", DismissTable, Some("SearchPanel")),
        KeyBinding::new("escape", DismissTable, Some("SearchPanel > DataTable")),
        KeyBinding::new("enter", ConfirmSelect, Some("SearchPanel > DataTable")),
        KeyBinding::new("down", FocusTable, Some("SearchPanel > Input")),
    ]);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchResultItem {
    pub offset: usize,
    pub len: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchPanelEvent {
    NavigateTo { offset: usize, len: usize },
    FocusEditor,
}

pub struct SearchTableDelegate {
    results: Vec<SearchResultItem>,
    editor: Option<Entity<Editor>>,
    selected_index: Option<usize>,
    focus_handle: FocusHandle,
}

impl TableDelegate for SearchTableDelegate {
    fn columns_count(&self, _cx: &App) -> usize {
        3
    }

    fn rows_count(&self, _cx: &App) -> usize {
        self.results.len()
    }

    fn column(&self, col_ix: usize, _cx: &App) -> Column {
        match col_ix {
            0 => Column::new("address", "Address").width(px(88.0)).min_width(px(50.0)).resizable(true),
            1 => Column::new("hex", "Hex").width(px(180.0)).min_width(px(80.0)).resizable(true),
            2 => Column::new("text", "Text").width(px(140.0)).min_width(px(60.0)).resizable(true),
            _ => Column::default(),
        }
    }

    fn fit_column(&mut self, col_ix: usize, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> Option<Pixels> {
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let (encoding, buffer, address_map) = if let Some(ed) = &self.editor {
            let ed_ref = ed.read(cx);
            let doc = ed_ref.document.read().ok();
            let buf = doc.as_ref().map(|d| d.buffer.clone());
            let map = doc.as_ref().map(|d| d.address_map.clone()).unwrap_or_default();
            (ed_ref.options.encoding, buf, map)
        } else {
            (Encoding::Ascii, None, crate::core::address_map::AddressMap::default())
        };
        let buffer_slice = buffer.as_ref().map(|b| b.data());

        let max_len = self
            .results
            .iter()
            .take(128)
            .map(|item| match col_ix {
                0 => hex_case.format_prefixed_pad(address_map.offset_to_address(item.offset), 8).chars().count(),
                1 => {
                    let (hex, _) = format_row_previews(buffer_slice, item.offset, item.len, encoding, hex_case);
                    hex.chars().count()
                }
                2 => {
                    let (_, text) = format_row_previews(buffer_slice, item.offset, item.len, encoding, hex_case);
                    text.chars().count()
                }
                _ => 0,
            })
            .max()
            .unwrap_or(0);

        Some(px(max_len as f32 * 7.5 + 24.0))
    }

    fn context_menu(&mut self, row_ix: usize, menu: PopupMenu, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> PopupMenu {
        let Some(item) = self.results.get(row_ix) else {
            return menu;
        };
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let offset = item.offset;
        let match_len = item.len;
        let (encoding, buffer, address_map) = if let Some(ed) = &self.editor {
            let ed_ref = ed.read(cx);
            let doc = ed_ref.document.read().ok();
            let buf = doc.as_ref().map(|d| d.buffer.clone());
            let map = doc.as_ref().map(|d| d.address_map.clone()).unwrap_or_default();
            (ed_ref.options.encoding, buf, map)
        } else {
            (Encoding::Ascii, None, crate::core::address_map::AddressMap::default())
        };
        let offset_value = hex_case.format_prefixed_pad(address_map.offset_to_address(offset), 8);
        let buffer_data = buffer.as_ref().map(|b| b.data());
        let (preview_hex, preview_text) = format_row_previews(buffer_data, offset, match_len, encoding, hex_case);

        menu.action_context(self.focus_handle.clone())
            .menu_with_icon("Copy Address", IconName::Hash, Box::new(CopyAddress { value: offset_value }))
            .menu_with_icon("Copy Hex Value", IconName::Binary, Box::new(CopyValue { value: preview_hex }))
            .menu_with_icon("Copy Text", IconName::TextInitial, Box::new(CopyText { value: preview_text }))
    }

    fn render_td(&mut self, row_ix: usize, col_ix: usize, _window: &mut Window, cx: &mut Context<TableState<Self>>) -> impl IntoElement {
        let Some(item) = self.results.get(row_ix) else {
            return div().into_any_element();
        };
        let theme = cx.theme();
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let font_family = cx.global::<Appearance>().font_family.clone();
        let (encoding, buffer, address_map) = if let Some(ed) = &self.editor {
            let ed_ref = ed.read(cx);
            let doc = ed_ref.document.read().ok();
            let buf = doc.as_ref().map(|d| d.buffer.clone());
            let map = doc.as_ref().map(|d| d.address_map.clone()).unwrap_or_default();
            (ed_ref.options.encoding, buf, map)
        } else {
            (Encoding::Ascii, None, crate::core::address_map::AddressMap::default())
        };
        let buffer_data = buffer.as_ref().map(|b| b.data());
        let offset = item.offset;
        let offset_value = hex_case.format_prefixed_pad(address_map.offset_to_address(offset), 8);
        let (preview_hex, preview_text) = format_row_previews(buffer_data, offset, item.len, encoding, hex_case);

        let is_selected = self.selected_index == Some(row_ix);

        match col_ix {
            0 => div()
                .text_xs()
                .font_family(font_family)
                .font_semibold()
                .text_color(if is_selected { theme.accent_foreground } else { theme.muted_foreground })
                .child(offset_value)
                .into_any_element(),
            1 => div()
                .text_xs()
                .font_family(font_family)
                .text_color(theme.muted_foreground)
                .child(preview_hex)
                .into_any_element(),
            2 => div()
                .text_xs()
                .font_family(font_family)
                .text_color(theme.foreground)
                .child(preview_text)
                .into_any_element(),
            _ => div().into_any_element(),
        }
    }
}

pub struct SearchPanel {
    pub editor: Option<Entity<Editor>>,
    pub focus_handle: FocusHandle,
    pub table_state: Entity<TableState<SearchTableDelegate>>,
    pub input: Entity<InputState>,
    pub mode: SearchMode,
    pub flags: TextSearchFlags,
    pub is_searching: bool,
    pub results: Vec<SearchResultItem>,
    pub is_truncated: bool,
    pub selected_index: Option<usize>,
    pub last_query: String,
    pub error_message: Option<String>,
    pub search_task: Option<Task<()>>,
    cancel_flag: Option<Arc<AtomicBool>>,
    is_panel_active: bool,
    needs_editor_sync: bool,
    _table_subscription: Option<Subscription>,
    _input_subscription: Option<Subscription>,
    _editor_subscription: Option<Subscription>,
    _hex_case_subscription: Subscription,
}

impl EventEmitter<SearchPanelEvent> for SearchPanel {}

impl SearchPanel {
    pub fn new(editor: Option<Entity<Editor>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let delegate = SearchTableDelegate {
            results: Vec::new(),
            editor: None,
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
                if this.selected_index != Some(*row_ix) {
                    this.select_item(*row_ix, cx);
                }
            }
            TableEvent::DoubleClickedRow(row_ix) => {
                this.select_item(*row_ix, cx);
                cx.emit(SearchPanelEvent::FocusEditor);
                window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
            }
            _ => {}
        });

        let input = cx.new(|cx| InputState::new(window, cx).placeholder(SearchMode::Hex.placeholder()));

        let input_sub = cx.subscribe_in(&input, window, |this, _, event: &input::InputEvent, _window, cx| match event {
            input::InputEvent::PressEnter { .. } => {
                this.trigger_search(cx);
            }
            input::InputEvent::Change => {
                this.error_message = None;
                cx.notify();
            }
            _ => {}
        });

        let hex_case_sub = cx.observe_global::<HexCase>(|this, cx| {
            this.table_state.update(cx, |table, cx| {
                table.refresh(cx);
            });
            cx.notify();
        });

        let mut this = Self {
            editor: None,
            focus_handle,
            table_state,
            input,
            mode: SearchMode::Hex,
            flags: TextSearchFlags::default(),
            is_searching: false,
            results: Vec::new(),
            is_truncated: false,
            selected_index: None,
            last_query: String::new(),
            error_message: None,
            search_task: None,
            cancel_flag: None,
            is_panel_active: true,
            needs_editor_sync: false,
            _table_subscription: Some(table_subscription),
            _input_subscription: Some(input_sub),
            _editor_subscription: None,
            _hex_case_subscription: hex_case_sub,
        };

        this.set_editor(editor, cx);
        this
    }

    /// Controls whether editor cursor updates sync search selection immediately or defer until activated.
    pub fn set_panel_active(&mut self, active: bool, cx: &mut Context<Self>) {
        if self.is_panel_active == active {
            return;
        }
        self.is_panel_active = active;
        if active && self.needs_editor_sync {
            self.needs_editor_sync = false;
            if let Some(editor) = self.editor.clone() {
                self.sync_selected_index_from_editor(&editor, cx);
            }
            cx.notify();
        }
    }

    fn cancel_active_task(&mut self) {
        if let Some(flag) = self.cancel_flag.take() {
            flag.store(true, Ordering::Relaxed);
        }
        self.search_task = None;
    }

    pub fn cancel_search(&mut self, cx: &mut Context<Self>) {
        if !self.is_searching {
            return;
        }
        self.cancel_active_task();
        self.is_searching = false;
        cx.notify();
    }

    pub fn set_editor(&mut self, editor: Option<Entity<Editor>>, cx: &mut Context<Self>) {
        self._editor_subscription = None;
        self.cancel_active_task();
        self.editor = editor.clone();
        self.results.clear();
        self.is_truncated = false;
        self.selected_index = None;
        self.is_searching = false;
        self.error_message = None;
        self.needs_editor_sync = false;
        self.sync_table_state(cx);

        if let Some(ed) = &editor {
            self._editor_subscription = Some(cx.observe(ed, |this, ed, cx| {
                if !this.is_panel_active {
                    this.needs_editor_sync = true;
                    return;
                }
                if this.sync_selected_index_from_editor(&ed, cx) {
                    cx.notify();
                }
            }));
        }
        if self.is_panel_active {
            cx.notify();
        } else {
            self.needs_editor_sync = true;
        }
    }

    pub fn set_mode(&mut self, mode: SearchMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.mode = mode;
            self.input.update(cx, |input, cx| {
                input.set_placeholder(mode.placeholder(), window, cx);
            });
            self.clear_results(cx);
        }
    }

    fn toggle_case_insensitive(&mut self, cx: &mut Context<Self>) {
        self.flags.case_insensitive = !self.flags.case_insensitive;
        self.error_message = None;
        cx.notify();
    }

    fn toggle_regex(&mut self, cx: &mut Context<Self>) {
        self.flags.regex = !self.flags.regex;
        self.error_message = None;
        cx.notify();
    }

    pub fn trigger_search(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value().to_string();
        if query.trim().is_empty() {
            self.clear_results(cx);
            return;
        }

        let Some(editor_entity) = self.editor.clone() else {
            return;
        };

        let mode = self.mode;
        let flags = self.flags;
        let encoding = editor_entity.read(cx).options.encoding;

        if let Err(err) = compile_search(&query, mode, encoding, flags) {
            self.clear_results(cx);
            self.last_query = query;
            self.error_message = Some(err.to_string());
            cx.notify();
            return;
        }

        self.cancel_active_task();
        self.last_query = query.clone();
        self.error_message = None;
        self.is_searching = true;
        self.results.clear();
        self.is_truncated = false;
        self.selected_index = None;
        self.sync_table_state(cx);

        // Update editor search query so visible ranges in HexView are highlighted
        editor_entity.update(cx, |editor, cx| {
            editor.search_state_mut().set_query_mode_and_flags(query.clone(), mode, flags);
            cx.notify();
        });

        let (buffer_data, segment_ranges) = {
            let editor = editor_entity.read(cx);
            let doc = editor.document.read().expect("document read lock");
            (doc.buffer.clone(), doc.address_map.segment_ranges())
        };

        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel_flag = Some(cancel.clone());
        let cancel_for_check = cancel.clone();

        let task = cx.spawn(async move |this, cx| {
            let search_task = cx.update(|cx| {
                let service = crate::app_state::AppState::global(cx).search_service.clone();
                service.search_with_limit(
                    crate::service::search_service::LimitedSearchQuery {
                        buffer: buffer_data,
                        query,
                        mode,
                        encoding,
                        flags,
                        segments: segment_ranges,
                        max_results: MAX_SEARCH_RESULTS,
                        cancel: Some(cancel),
                    },
                    cx,
                )
            });

            let result = search_task.await;
            if cancel_for_check.load(Ordering::Relaxed) {
                return;
            }

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.is_searching = false;
                    this.cancel_flag = None;
                    match result {
                        Ok((ranges, is_truncated)) => {
                            let items: Vec<SearchResultItem> = ranges.iter().map(|r| SearchResultItem { offset: r.start, len: r.len() }).collect();
                            this.error_message = None;
                            this.is_truncated = is_truncated;
                            this.results = items;
                            if !this.results.is_empty() {
                                this.selected_index = Some(0);
                            }
                            this.sync_table_state(cx);
                            if let Some(ed) = &this.editor {
                                let generation = ed.read(cx).search_state.generation;
                                ed.update(cx, |editor, cx| {
                                    editor.search_state_mut().set_result_ranges(ranges, generation, !is_truncated);
                                    cx.notify();
                                });
                            }
                        }
                        Err(err) => {
                            this.error_message = Some(err.to_string());
                            this.results.clear();
                            this.is_truncated = false;
                            this.selected_index = None;
                            this.sync_table_state(cx);
                        }
                    }
                    cx.notify();
                });
            }
        });

        self.search_task = Some(task);
        cx.notify();
    }

    pub fn select_item(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.results.len() {
            self.selected_index = Some(index);
            let item = self.results[index];
            let offset = item.offset;
            let len = item.len;

            let table_state = self.table_state.clone();
            cx.defer(move |cx| {
                table_state.update(cx, |table, cx| {
                    table.delegate_mut().selected_index = Some(index);
                    if table.selected_row() != Some(index) {
                        table.set_selected_row(index, cx);
                        table.scroll_to_row(index, cx);
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
                    editor.search_state.current_result_index = Some(index);
                    cx.notify();
                });
            }

            cx.emit(SearchPanelEvent::NavigateTo { offset, len });
            cx.notify();
        }
    }

    fn focus_table(&mut self, _: &FocusTable, window: &mut Window, cx: &mut Context<Self>) {
        if !self.results.is_empty() {
            let focus_handle = self.table_state.read(cx).focus_handle(cx);
            focus_handle.focus(window, cx);
        }
    }

    fn confirm_select(&mut self, _: &ConfirmSelect, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(idx) = self.selected_index {
            self.select_item(idx, cx);
            cx.emit(SearchPanelEvent::FocusEditor);
            window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
        }
    }

    fn dismiss_table(&mut self, _: &DismissTable, window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(SearchPanelEvent::FocusEditor);
        window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
    }

    fn clear_results_action(&mut self, _: &ClearResults, _: &mut Window, cx: &mut Context<Self>) {
        self.clear_results(cx);
    }

    fn clear_results(&mut self, cx: &mut Context<Self>) {
        self.cancel_active_task();
        self.is_searching = false;
        self.results.clear();
        self.is_truncated = false;
        self.selected_index = None;
        self.last_query.clear();
        self.error_message = None;
        self.sync_table_state(cx);
        if let Some(ed) = &self.editor {
            ed.update(cx, |ed, cx| {
                ed.search_state_mut().clear();
                cx.notify();
            });
        }
        cx.notify();
    }

    fn sync_table_state(&mut self, cx: &mut Context<Self>) {
        let results = self.results.clone();
        let editor = self.editor.clone();
        let selected_index = self.selected_index;

        self.table_state.update(cx, |table, cx| {
            let delegate = table.delegate_mut();
            delegate.results = results;
            delegate.editor = editor;
            delegate.selected_index = selected_index;
            table.refresh(cx);
            if let Some(idx) = selected_index {
                table.set_selected_row(idx, cx);
                table.scroll_to_row(idx, cx);
            } else {
                table.clear_selection(cx);
            }
        });
    }

    fn copy_address(&mut self, action: &CopyAddress, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
        window.push_notification(Notification::info(format!("Copied address {}", action.value)), cx);
    }

    fn copy_value(&mut self, action: &CopyValue, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
        window.push_notification(Notification::info("Copied hex value"), cx);
    }

    fn copy_text(&mut self, action: &CopyText, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
        window.push_notification(Notification::info("Copied text value"), cx);
    }

    fn sync_selected_index_from_editor(&mut self, editor: &Entity<Editor>, cx: &mut Context<Self>) -> bool {
        if self.results.is_empty() {
            return false;
        }
        let editor_ref = editor.read(cx);
        let found_index = find_matching_result_index(&self.results, editor_ref.selection_range().map(|range| range.start), editor_ref.cursor.offset);

        if self.selected_index == found_index {
            return false;
        }

        self.selected_index = found_index;
        self.table_state.update(cx, |table, cx| {
            table.delegate_mut().selected_index = found_index;
            if let Some(idx) = found_index {
                if table.selected_row() != Some(idx) {
                    table.set_selected_row(idx, cx);
                    table.scroll_to_row(idx, cx);
                }
            } else {
                table.clear_selection(cx);
            }
        });
        true
    }
}

fn find_matching_result_index(results: &[SearchResultItem], selection_start: Option<usize>, cursor_offset: usize) -> Option<usize> {
    if results.is_empty() {
        return None;
    }
    let lookup = |offset: usize| -> Option<usize> {
        match results.binary_search_by_key(&offset, |item| item.offset) {
            Ok(exact) => Some(exact),
            Err(insert_idx) => {
                if insert_idx > 0 {
                    let prev_idx = insert_idx - 1;
                    let item = results[prev_idx];
                    let effective_len = item.len.max(1);
                    if offset < item.offset.saturating_add(effective_len) {
                        Some(prev_idx)
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
        }
    };

    selection_start.and_then(lookup).or_else(|| lookup(cursor_offset))
}

fn format_row_previews(buffer_data: Option<&[u8]>, offset: usize, match_len: usize, encoding: Encoding, hex_case: HexCase) -> (String, String) {
    let snippet_len = match_len.clamp(4, 16);
    if let Some(data) = buffer_data {
        let end = (offset + snippet_len).min(data.len());
        let slice = if offset < data.len() { &data[offset..end] } else { &[] };
        let digits = hex_case.digits();
        let mut hex = String::with_capacity(slice.len() * 3);
        for (i, &b) in slice.iter().enumerate() {
            if i > 0 {
                hex.push(' ');
            }
            hex.push(digits[(b >> 4) as usize] as char);
            hex.push(digits[(b & 0x0F) as usize] as char);
        }
        let text = encoding.format_preview(data, offset, snippet_len);
        (hex, text)
    } else {
        (String::new(), String::new())
    }
}

impl Render for SearchPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let is_input_focused = self.input.read(cx).focus_handle(cx).is_focused(window);
        let is_table_focused = self.table_state.read(cx).focus_handle(cx).is_focused(window);
        let is_focused = self.focus_handle.is_focused(window) || is_input_focused || is_table_focused;
        let has_editor = self.editor.is_some();
        let is_query_empty = self.input.read(cx).value().trim().is_empty();
        let count = self.results.len();

        let badge = if self.is_searching {
            Some(crate::ui::panels::panel_badge("Scanning...", theme).into_any_element())
        } else if self.error_message.is_some() {
            Some(crate::ui::panels::panel_badge("Error", theme).into_any_element())
        } else if self.is_truncated {
            if let Some(idx) = self.selected_index {
                Some(crate::ui::panels::panel_badge(format!("{}/{}+ matches", idx + 1, MAX_SEARCH_RESULTS), theme).into_any_element())
            } else {
                Some(crate::ui::panels::panel_badge(format!("{}+ matches", MAX_SEARCH_RESULTS), theme).into_any_element())
            }
        } else if count > 0 {
            if let Some(idx) = self.selected_index {
                Some(crate::ui::panels::panel_badge(format!("{}/{} matches", idx + 1, count), theme).into_any_element())
            } else {
                Some(crate::ui::panels::panel_badge(format!("{} matches", count), theme).into_any_element())
            }
        } else if !self.last_query.is_empty() {
            Some(crate::ui::panels::panel_badge("0 matches", theme).into_any_element())
        } else {
            None
        };

        let actions = Button::new("clear-search")
            .ghost()
            .icon(IconName::Eraser)
            .with_size(Size::XSmall)
            .tooltip("Clear results")
            .disabled(!has_editor || (self.results.is_empty() && self.last_query.is_empty() && self.error_message.is_none() && !self.is_searching))
            .on_click(cx.listener(|this, _, _window, cx| {
                this.clear_results(cx);
            }));

        let header = crate::ui::panels::panel_header("SEARCH", is_focused, theme, badge, Some(actions.into_any_element()));

        let action_button = if self.is_searching {
            Button::new("cancel_scan_btn")
                .label("Cancel")
                .danger()
                .with_size(Size::Small)
                .tooltip("Cancel ongoing search")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.cancel_search(cx);
                }))
        } else {
            Button::new("scan_btn")
                .label("Find All")
                .primary()
                .with_size(Size::Small)
                .disabled(!has_editor || is_query_empty)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.trigger_search(cx);
                }))
        };

        // Search controls: Mode toggle + Text options + Search Input + Scan/Cancel Button
        let search_controls = v_flex()
            .p_2()
            .gap_2()
            .border_b_1()
            .border_color(theme.border)
            .child(
                h_flex()
                    .gap_1()
                    .items_center()
                    .child(
                        if self.mode == SearchMode::Hex {
                            Button::new("mode_hex").label("Hex").primary().with_size(Size::XSmall)
                        } else {
                            Button::new("mode_hex").label("Hex").ghost().with_size(Size::XSmall)
                        }
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.set_mode(SearchMode::Hex, window, cx);
                        })),
                    )
                    .child(
                        if self.mode == SearchMode::Text {
                            Button::new("mode_text").label("Text").primary().with_size(Size::XSmall)
                        } else {
                            Button::new("mode_text").label("Text").ghost().with_size(Size::XSmall)
                        }
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.set_mode(SearchMode::Text, window, cx);
                        })),
                    )
                    .when(self.mode == SearchMode::Text, |el| {
                        let match_case = !self.flags.case_insensitive;
                        let regex_enabled = self.flags.regex;
                        el.child(div().w(px(6.0)))
                            .child(
                                if match_case {
                                    Button::new("panel_match_case").label("Aa").primary().with_size(Size::XSmall)
                                } else {
                                    Button::new("panel_match_case").label("Aa").ghost().with_size(Size::XSmall)
                                }
                                .tooltip(if match_case {
                                    "Match Case: On (click for case-insensitive)"
                                } else {
                                    "Match Case: Off (click for case-sensitive)"
                                })
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_case_insensitive(cx);
                                })),
                            )
                            .child(
                                if regex_enabled {
                                    Button::new("panel_regex").label(".*").primary().with_size(Size::XSmall)
                                } else {
                                    Button::new("panel_regex").label(".*").ghost().with_size(Size::XSmall)
                                }
                                .tooltip("Use Regular Expression (bytes)")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.toggle_regex(cx);
                                })),
                            )
                    }),
            )
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(div().flex_1().child(Input::new(&self.input).prefix(IconName::Search).cleanable(true)))
                    .child(action_button),
            );

        let truncation_notice = if self.is_truncated {
            Some(
                div()
                    .px_2()
                    .py_1()
                    .bg(theme.secondary)
                    .border_b_1()
                    .border_color(theme.border)
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(format!(
                        "{}+ occurrences found (showing first {} entries)",
                        MAX_SEARCH_RESULTS, MAX_SEARCH_RESULTS
                    )),
            )
        } else {
            None
        };

        let body = if !has_editor {
            crate::ui::panels::panel_empty_state(
                IconName::Search,
                "No Active File",
                Some("Open a binary file to search its contents"),
                None,
                theme,
            )
            .into_any_element()
        } else if self.is_searching {
            let cancel_btn = Button::new("empty_cancel_scan")
                .label("Cancel Search")
                .ghost()
                .with_size(Size::Small)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.cancel_search(cx);
                }));
            crate::ui::panels::panel_empty_state(
                IconName::Loader,
                "Searching File...",
                Some("Finding matching occurrences in the background"),
                Some(cancel_btn.into_any_element()),
                theme,
            )
            .into_any_element()
        } else if let Some(err) = &self.error_message {
            crate::ui::panels::panel_empty_state(IconName::CircleAlert, "Invalid Pattern", Some(err.as_str()), None, theme).into_any_element()
        } else if self.results.is_empty() {
            let message = if self.last_query.is_empty() {
                "Enter a pattern and click Find All"
            } else {
                "No occurrences found for query"
            };
            crate::ui::panels::panel_empty_state(IconName::Search, "No Results", Some(message), None, theme).into_any_element()
        } else {
            v_flex()
                .flex_1()
                .overflow_hidden()
                .children(truncation_notice)
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
            .on_action(cx.listener(Self::focus_table))
            .on_action(cx.listener(Self::confirm_select))
            .on_action(cx.listener(Self::dismiss_table))
            .on_action(cx.listener(|this, _: &gpui_kit::base::actions::Cancel, window, cx| {
                this.dismiss_table(&DismissTable, window, cx);
            }))
            .on_action(cx.listener(Self::clear_results_action))
            .on_action(cx.listener(Self::copy_address))
            .on_action(cx.listener(Self::copy_value))
            .on_action(cx.listener(Self::copy_text))
            .child(header)
            .child(search_controls)
            .child(body)
    }
}

impl Focusable for SearchPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::format_row_previews;
    use crate::core::encoding::Encoding;
    use crate::core::format::HexCase;

    #[test]
    fn test_format_row_previews_respects_hex_case() {
        let data = b"Hello, World!\x00\x01\x02\x03";
        let (hex_upper, text_upper) = format_row_previews(Some(data), 0, 5, Encoding::Ascii, HexCase::Upper);
        assert_eq!(hex_upper, "48 65 6C 6C 6F");
        assert_eq!(text_upper, "Hello");

        let (hex_lower, text_lower) = format_row_previews(Some(data), 0, 5, Encoding::Ascii, HexCase::Lower);
        assert_eq!(hex_lower, "48 65 6c 6c 6f");
        assert_eq!(text_lower, "Hello");
    }

    #[test]
    fn test_navigation_index_calculations() {
        let results_len = 25;
        // move_up
        let prev_idx = |current: Option<usize>| match current {
            Some(0) | None => results_len - 1,
            Some(idx) => idx - 1,
        };
        assert_eq!(prev_idx(Some(5)), 4);
        assert_eq!(prev_idx(Some(0)), 24);
        assert_eq!(prev_idx(None), 24);

        // move_down
        let next_idx = |current: Option<usize>| match current {
            Some(idx) if idx + 1 < results_len => idx + 1,
            _ => 0,
        };
        assert_eq!(next_idx(Some(5)), 6);
        assert_eq!(next_idx(Some(24)), 0);
        assert_eq!(next_idx(None), 0);

        // page_up & page_down
        let page_size = 10;
        let page_up_idx = |current: usize| current.saturating_sub(page_size);
        let page_down_idx = |current: usize| (current + page_size).min(results_len - 1);
        assert_eq!(page_up_idx(15), 5);
        assert_eq!(page_up_idx(4), 0);
        assert_eq!(page_down_idx(5), 15);
        assert_eq!(page_down_idx(20), 24);
    }

    #[test]
    fn test_search_panel_event_focus_editor() {
        let event = super::SearchPanelEvent::FocusEditor;
        assert_eq!(event, super::SearchPanelEvent::FocusEditor);

        let parent_ctx = gpui_kit::KeyContext::try_from("SearchPanel").unwrap();
        let child_ctx = gpui_kit::KeyContext::try_from("DataTable").unwrap();
        let contexts = vec![parent_ctx, child_ctx];

        let pred_and = gpui_kit::KeyBindingContextPredicate::parse("SearchPanel && DataTable").unwrap();
        assert!(!pred_and.eval(&contexts), "SearchPanel && DataTable should NOT match across separate elements!");

        let pred_desc = gpui_kit::KeyBindingContextPredicate::parse("SearchPanel > DataTable").unwrap();
        assert!(
            pred_desc.eval(&contexts),
            "SearchPanel > DataTable MUST match when DataTable is child of SearchPanel!"
        );
        assert_eq!(
            pred_desc.depth_of(&contexts),
            Some(2),
            "Depth of SearchPanel > DataTable should be 2 (the DataTable depth)"
        );

        let table_only = gpui_kit::KeyBindingContextPredicate::parse("DataTable").unwrap();
        assert_eq!(table_only.depth_of(&contexts), Some(2));
    }

    #[test]
    fn test_find_matching_result_index_variable_lengths() {
        use super::{SearchResultItem, find_matching_result_index};

        let results = [
            SearchResultItem { offset: 0x10, len: 4 },
            SearchResultItem { offset: 0x40, len: 2 },
            SearchResultItem { offset: 0x80, len: 6 },
        ];

        assert_eq!(find_matching_result_index(&[], None, 0x10), None);
        assert_eq!(find_matching_result_index(&results, None, 0x10), Some(0));
        assert_eq!(find_matching_result_index(&results, None, 0x13), Some(0));
        assert_eq!(find_matching_result_index(&results, None, 0x14), None);
        assert_eq!(find_matching_result_index(&results, Some(0x41), 0x80), Some(1));
        assert_eq!(find_matching_result_index(&results, None, 0x42), None);
        assert_eq!(find_matching_result_index(&results, Some(0x20), 0x85), Some(2));
    }
}

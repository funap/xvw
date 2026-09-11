use crate::core::appearance::Appearance;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::search::{SearchLimit, SearchMode, find_occurrences_segmented, parse_hex_pattern, parse_text_pattern};
use crate::ui::components::data_table::{self as table, TableColumn, VirtualTable, VirtualTableState};
use crate::ui::icon::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::component::menu::ContextMenuExt as _;
use gpui_kit::component::{ActiveTheme as _, Disableable, Sizable, Size, StyledExt, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

actions!(search_panel, [FocusTable, ClearResults]);

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

const SEARCH_ROW_HEIGHT: f32 = 24.0;

pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("escape", FocusTable, Some("SearchPanel"))]);
}

fn default_search_columns() -> Vec<TableColumn> {
    vec![
        TableColumn::new("address", "Address", px(88.0))
            .min_width(px(50.0))
            .sortable(true)
            .resizable(true),
        TableColumn::new("hex", "Hex", px(180.0)).min_width(px(80.0)).resizable(true),
        TableColumn::new("text", "Text", px(140.0)).min_width(px(60.0)).resizable(true),
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SearchResultItem {
    pub offset: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchPanelEvent {
    NavigateTo { offset: usize, len: usize },
    FocusEditor,
}

pub struct SearchPanel {
    pub editor: Option<Entity<Editor>>,
    pub focus_handle: FocusHandle,
    pub table_state: VirtualTableState,
    pub last_container_width: Pixels,
    pub input: Entity<InputState>,
    pub mode: SearchMode,
    pub is_searching: bool,
    pub results: Vec<SearchResultItem>,
    pub is_truncated: bool,
    pub selected_index: Option<usize>,
    pub last_query: String,
    pub match_len: usize,
    pub search_task: Option<Task<()>>,
    _input_subscription: Option<Subscription>,
    _editor_subscription: Option<Subscription>,
}

impl EventEmitter<SearchPanelEvent> for SearchPanel {}

impl SearchPanel {
    pub fn new(editor: Option<Entity<Editor>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let table_focus_handle = cx.focus_handle();
        let table_state = VirtualTableState::new("search-table", default_search_columns(), table_focus_handle);
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(SearchMode::Hex.placeholder()));

        let input_sub = cx.subscribe_in(&input, window, |this, _, event: &input::InputEvent, _window, cx| match event {
            input::InputEvent::PressEnter { .. } => {
                this.trigger_search(cx);
            }
            input::InputEvent::Change => {
                cx.notify();
            }
            _ => {}
        });

        let mut this = Self {
            editor: None,
            focus_handle,
            table_state,
            last_container_width: px(300.0),
            input,
            mode: SearchMode::Hex,
            is_searching: false,
            results: Vec::new(),
            is_truncated: false,
            selected_index: None,
            last_query: String::new(),
            match_len: 0,
            search_task: None,
            _input_subscription: Some(input_sub),
            _editor_subscription: None,
        };

        this.set_editor(editor, cx);
        this
    }

    pub fn set_editor(&mut self, editor: Option<Entity<Editor>>, cx: &mut Context<Self>) {
        self._editor_subscription = None;
        self.editor = editor.clone();
        self.results.clear();
        self.is_truncated = false;
        self.selected_index = None;
        self.is_searching = false;
        self.search_task = None;

        if let Some(ed) = &editor {
            self._editor_subscription = Some(cx.observe(ed, |this, ed, cx| {
                this.sync_selected_index_from_editor(&ed, cx);
                cx.notify();
            }));
        }
        cx.notify();
    }

    pub fn set_mode(&mut self, mode: SearchMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.mode = mode;
            self.input.update(cx, |input, cx| {
                input.set_placeholder(mode.placeholder(), window, cx);
            });
            cx.notify();
        }
    }

    pub fn trigger_search(&mut self, cx: &mut Context<Self>) {
        let query = self.input.read(cx).value().to_string();
        if query.trim().is_empty() {
            self.results.clear();
            self.is_truncated = false;
            self.selected_index = None;
            self.last_query.clear();
            self.match_len = 0;
            if let Some(ed) = &self.editor {
                ed.update(cx, |ed, cx| {
                    ed.search_state_mut().clear();
                    cx.notify();
                });
            }
            cx.notify();
            return;
        }

        let Some(editor_entity) = &self.editor else {
            return;
        };

        let mode = self.mode;
        let encoding = editor_entity.read(cx).options.encoding;
        let pattern_len = match mode {
            SearchMode::Text => parse_text_pattern(&query, encoding).map(|p| p.len()).unwrap_or(0),
            SearchMode::Hex => parse_hex_pattern(&query).map(|p| p.len()).unwrap_or(0),
        };

        if pattern_len == 0 {
            self.results.clear();
            self.is_truncated = false;
            self.selected_index = None;
            self.last_query = query.clone();
            self.match_len = 0;
            if let Some(ed) = &self.editor {
                ed.update(cx, |ed, cx| {
                    ed.search_state_mut().clear();
                    cx.notify();
                });
            }
            cx.notify();
            return;
        }

        self.last_query = query.clone();
        self.match_len = pattern_len;
        self.is_searching = true;
        self.results.clear();
        self.is_truncated = false;
        self.selected_index = None;

        // Update editor search query so visible ranges in HexView are highlighted
        editor_entity.update(cx, |editor, cx| {
            editor.search_state_mut().set_query_and_mode(query.clone(), mode);
            cx.notify();
        });

        let (buffer_data, segment_ranges) = {
            let editor = editor_entity.read(cx);
            let doc = editor.document.read().expect("document read lock");
            (doc.buffer.clone(), doc.address_map.segment_ranges())
        };

        let task = cx.spawn(async move |this, cx| {
            let buffer_for_search = buffer_data.clone();
            let (offsets, is_truncated) = cx
                .background_executor()
                .spawn(async move {
                    let pattern = match mode {
                        SearchMode::Text => {
                            if let Some(p) = parse_text_pattern(&query, encoding) {
                                p
                            } else {
                                return (Vec::new(), false);
                            }
                        }
                        SearchMode::Hex => {
                            if let Some(p) = parse_hex_pattern(&query) {
                                p
                            } else {
                                return (Vec::new(), false);
                            }
                        }
                    };
                    let raw_offsets = find_occurrences_segmented(
                        buffer_for_search.data(),
                        &pattern,
                        SearchLimit::Count(MAX_SEARCH_RESULTS + 1),
                        &segment_ranges,
                        None,
                    );
                    let truncated = raw_offsets.len() > MAX_SEARCH_RESULTS;
                    let capped_offsets = if truncated {
                        raw_offsets.into_iter().take(MAX_SEARCH_RESULTS).collect()
                    } else {
                        raw_offsets
                    };
                    (capped_offsets, truncated)
                })
                .await;

            let items: Vec<SearchResultItem> = offsets.iter().map(|&offset| SearchResultItem { offset }).collect();

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.is_searching = false;
                    this.is_truncated = is_truncated;
                    this.results = items;
                    if !this.results.is_empty() {
                        this.selected_index = Some(0);
                        this.table_state.scroll_to_row(0, ScrollStrategy::Top);
                    }
                    if let Some(ed) = &this.editor {
                        let generation = ed.read(cx).search_state.generation;
                        ed.update(cx, |editor, cx| {
                            editor.search_state_mut().set_results(offsets, generation, true);
                            cx.notify();
                        });
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
            let offset = self.results[index].offset;
            let len = self.match_len;

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

    fn table_move_up(&mut self, _: &table::MoveUp, _: &mut Window, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        let new_idx = match self.selected_index {
            Some(0) | None => self.results.len().saturating_sub(1),
            Some(idx) => idx - 1,
        };
        self.table_state.scroll_to_row(new_idx, ScrollStrategy::Top);
        self.select_item(new_idx, cx);
    }

    fn table_move_down(&mut self, _: &table::MoveDown, _: &mut Window, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        let new_idx = match self.selected_index {
            Some(idx) if idx + 1 < self.results.len() => idx + 1,
            _ => 0,
        };
        self.table_state.scroll_to_row(new_idx, ScrollStrategy::Top);
        self.select_item(new_idx, cx);
    }

    fn table_move_top(&mut self, _: &table::MoveTop, _: &mut Window, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        self.table_state.scroll_to_row(0, ScrollStrategy::Top);
        self.select_item(0, cx);
    }

    fn table_move_bottom(&mut self, _: &table::MoveBottom, _: &mut Window, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        let last_idx = self.results.len().saturating_sub(1);
        self.table_state.scroll_to_row(last_idx, ScrollStrategy::Top);
        self.select_item(last_idx, cx);
    }

    fn table_page_up(&mut self, _: &table::PageUp, _: &mut Window, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        let current = self.selected_index.unwrap_or(0);
        let page_size = 10;
        let new_idx = current.saturating_sub(page_size);
        self.table_state.scroll_to_row(new_idx, ScrollStrategy::Top);
        self.select_item(new_idx, cx);
    }

    fn table_page_down(&mut self, _: &table::PageDown, _: &mut Window, cx: &mut Context<Self>) {
        if self.results.is_empty() {
            return;
        }
        let current = self.selected_index.unwrap_or(0);
        let page_size = 10;
        let last_idx = self.results.len().saturating_sub(1);
        let new_idx = (current + page_size).min(last_idx);
        self.table_state.scroll_to_row(new_idx, ScrollStrategy::Top);
        self.select_item(new_idx, cx);
    }

    fn table_select_current(&mut self, _: &table::SelectCurrent, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(idx) = self.selected_index {
            self.select_item(idx, cx);
        }
    }

    fn table_dismiss(&mut self, _: &table::Dismiss, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(SearchPanelEvent::FocusEditor);
    }

    fn focus_table(&mut self, _: &FocusTable, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handle) = &self.table_state.focus_handle {
            handle.focus(window, cx);
        }
        cx.notify();
    }

    fn clear_results_action(&mut self, _: &ClearResults, _: &mut Window, cx: &mut Context<Self>) {
        self.results.clear();
        self.is_truncated = false;
        self.selected_index = None;
        self.last_query.clear();
        self.match_len = 0;
        if let Some(ed) = &self.editor {
            ed.update(cx, |ed, cx| {
                ed.search_state_mut().clear();
                cx.notify();
            });
        }
        cx.notify();
    }

    fn copy_address(&mut self, action: &CopyAddress, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
        window.push_notification(
            gpui_kit::component::notification::Notification::info(format!("Copied address {}", action.value)),
            cx,
        );
    }

    fn copy_value(&mut self, action: &CopyValue, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
        window.push_notification(gpui_kit::component::notification::Notification::info("Copied hex value"), cx);
    }

    fn copy_text(&mut self, action: &CopyText, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
        window.push_notification(gpui_kit::component::notification::Notification::info("Copied text value"), cx);
    }

    fn auto_fit_column(&mut self, col_ix: usize, cx: &mut Context<Self>) {
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

        let texts = self.results.iter().take(128).map(|item| match col_ix {
            0 => format!("0x{:08X}", address_map.offset_to_address(item.offset)),
            1 => {
                let (hex, _) = format_row_previews(buffer_slice, item.offset, self.match_len, encoding);
                hex
            }
            2 => {
                let (_, text) = format_row_previews(buffer_slice, item.offset, self.match_len, encoding);
                text
            }
            _ => String::new(),
        });

        self.table_state.auto_fit_column_with_texts(col_ix, texts);
        cx.notify();
    }

    fn sync_selected_index_from_editor(&mut self, editor: &Entity<Editor>, cx: &App) {
        if self.results.is_empty() {
            return;
        }
        let editor_ref = editor.read(cx);
        let match_len = self.match_len.max(1);

        let target_offsets = if let Some(range) = editor_ref.selection_range() {
            vec![range.start, editor_ref.cursor.offset]
        } else {
            vec![editor_ref.cursor.offset]
        };

        let mut found_index = None;
        for offset in target_offsets {
            let res = match self.results.binary_search_by_key(&offset, |item| item.offset) {
                Ok(exact) => Some(exact),
                Err(insert_idx) => {
                    if insert_idx > 0 {
                        let prev_idx = insert_idx - 1;
                        let item_offset = self.results[prev_idx].offset;
                        if offset < item_offset.saturating_add(match_len) {
                            Some(prev_idx)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                }
            };
            if res.is_some() {
                found_index = res;
                break;
            }
        }

        if self.selected_index != found_index {
            self.selected_index = found_index;
        }
    }
}

fn format_row_previews(buffer_data: Option<&[u8]>, offset: usize, match_len: usize, encoding: Encoding) -> (String, String) {
    let snippet_len = match_len.clamp(4, 16);
    if let Some(data) = buffer_data {
        let end = (offset + snippet_len).min(data.len());
        let slice = if offset < data.len() { &data[offset..end] } else { &[] };
        let mut hex = String::with_capacity(slice.len() * 3);
        for (i, b) in slice.iter().enumerate() {
            if i > 0 {
                hex.push(' ');
            }
            use std::fmt::Write as _;
            let _ = write!(&mut hex, "{:02X}", b);
        }
        let text = encoding.format_preview(data, offset, snippet_len);
        (hex, text)
    } else {
        (String::new(), String::new())
    }
}

impl Render for SearchPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let table_overlay = VirtualTable::render_table_overlay(&self.table_state, cx, |this| &mut this.table_state);

        let theme = cx.theme();
        let font_family = cx.global::<Appearance>().font_family.clone();
        let is_table_focused = self.table_state.focus_handle.as_ref().is_some_and(|h| h.is_focused(window));
        let is_focused = self.focus_handle.is_focused(window) || is_table_focused;
        let has_editor = self.editor.is_some();
        let is_query_empty = self.input.read(cx).value().trim().is_empty();
        let count = self.results.len();

        let badge = if self.is_searching {
            Some(crate::ui::panels::panel_badge("Scanning...", theme).into_any_element())
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

        let actions = h_flex().items_center().gap_1().child(
            Button::new("clear-search")
                .ghost()
                .icon(IconName::Eraser)
                .with_size(Size::XSmall)
                .tooltip("Clear results")
                .disabled(!has_editor || (self.results.is_empty() && self.last_query.is_empty()))
                .on_click(cx.listener(|this, _, _window, cx| {
                    this.results.clear();
                    this.is_truncated = false;
                    this.selected_index = None;
                    this.last_query.clear();
                    this.match_len = 0;
                    if let Some(ed) = &this.editor {
                        ed.update(cx, |ed, cx| {
                            ed.search_state_mut().clear();
                            cx.notify();
                        });
                    }
                    cx.notify();
                })),
        );

        let header = crate::ui::panels::panel_header("SEARCH", is_focused, theme, badge, Some(actions.into_any_element()));

        // Search controls: Mode toggle + Search Input + Scan Button
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
                    ),
            )
            .child(
                h_flex()
                    .gap_1p5()
                    .items_center()
                    .child(div().flex_1().child(Input::new(&self.input).prefix(IconName::Search).cleanable(true)))
                    .child(
                        Button::new("scan_btn")
                            .label(if self.is_searching { "Scanning..." } else { "Find All" })
                            .primary()
                            .with_size(Size::Small)
                            .disabled(self.is_searching || !has_editor || is_query_empty)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.trigger_search(cx);
                            })),
                    ),
            );

        let truncation_notice = if self.is_truncated {
            Some(
                div()
                    .px_2()
                    .py_1()
                    .bg(theme.accent.opacity(0.1))
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
            crate::ui::panels::panel_empty_state(
                IconName::Loader,
                "Searching File...",
                Some("Finding matching occurrences in the background"),
                None,
                theme,
            )
            .into_any_element()
        } else if self.results.is_empty() {
            let message = if self.last_query.is_empty() {
                "Enter a pattern and click Search"
            } else {
                "No occurrences found for query"
            };
            crate::ui::panels::panel_empty_state(IconName::Search, "No Results", Some(message), None, theme).into_any_element()
        } else {
            let header_row = VirtualTable::render_header_row(
                &self.table_state,
                "search-column-header",
                theme,
                cx,
                None::<fn(usize, &TableColumn) -> Option<AnyElement>>,
                Self::auto_fit_column,
                None::<fn(&mut Self, usize, &mut Context<Self>)>,
                |this| &mut this.table_state,
            );

            let visible_cols: Vec<(usize, Pixels)> = self.table_state.visible_columns().map(|(ix, col)| (ix, col.width)).collect();
            let total_visible_width = self.table_state.total_visible_width();
            let scroll_offset_x = self.table_state.scroll_offset_x;
            let row_count = self.results.len();
            let view = cx.entity().clone();
            let list_view = view.clone();
            let context_focus_handle = self.focus_handle.clone();

            let list = uniform_list("search-virtual-table", row_count, move |visible_range, window, cx| {
                let this = list_view.read(cx);
                let theme = cx.theme();

                let (encoding, buffer, address_map) = if let Some(ed) = &this.editor {
                    let ed_ref = ed.read(cx);
                    let doc = ed_ref.document.read().ok();
                    let buf = doc.as_ref().map(|d| d.buffer.clone());
                    let map = doc.as_ref().map(|d| d.address_map.clone()).unwrap_or_default();
                    (ed_ref.options.encoding, buf, map)
                } else {
                    (Encoding::Ascii, None, crate::core::address_map::AddressMap::default())
                };
                let buffer_data = buffer.as_ref().map(|b| b.data());

                visible_range
                    .map(|idx| {
                        if let Some(item) = this.results.get(idx) {
                            let is_selected = this.selected_index == Some(idx);
                            let (bg_color, border_color) = if is_selected {
                                if is_table_focused {
                                    (theme.selection, theme.accent)
                                } else {
                                    (theme.muted, theme.muted_foreground.opacity(0.4))
                                }
                            } else {
                                (theme.sidebar, theme.border.opacity(0.5))
                            };
                            let hover_bg = if is_selected {
                                if is_table_focused {
                                    theme.selection.opacity(0.7)
                                } else {
                                    theme.muted.opacity(0.8)
                                }
                            } else {
                                theme.selection.opacity(0.3)
                            };

                            let offset = item.offset;
                            let offset_value = format!("0x{:08X}", address_map.offset_to_address(offset));
                            let (preview_hex, preview_text) = format_row_previews(buffer_data, offset, this.match_len, encoding);
                            h_flex()
                                .id(("search-result-item", idx))
                                .w_full()
                                .h(px(SEARCH_ROW_HEIGHT))
                                .flex_shrink_0()
                                .overflow_hidden()
                                .border_b_1()
                                .border_color(border_color)
                                .bg(bg_color)
                                .cursor_pointer()
                                .hover(move |style| style.bg(hover_bg))
                                .on_click(window.listener_for(&list_view, move |this, _, window, cx| {
                                    if let Some(handle) = &this.table_state.focus_handle {
                                        handle.focus(window, cx);
                                    }
                                    this.select_item(idx, cx);
                                }))
                                .on_mouse_down(
                                    MouseButton::Right,
                                    window.listener_for(&list_view, move |this, _, window, cx| {
                                        if let Some(handle) = &this.table_state.focus_handle {
                                            handle.focus(window, cx);
                                        }
                                        this.select_item(idx, cx);
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
                                                    .font_semibold()
                                                    .text_color(if is_selected { theme.accent_foreground } else { theme.muted_foreground })
                                                    .child(offset_value.clone())
                                                    .into_any_element(),
                                                1 => div()
                                                    .text_xs()
                                                    .font_family(font_family.clone())
                                                    .text_color(theme.muted_foreground)
                                                    .child(preview_hex.clone())
                                                    .into_any_element(),
                                                2 => div()
                                                    .text_xs()
                                                    .font_family(font_family.clone())
                                                    .text_color(theme.foreground)
                                                    .child(preview_text.clone())
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
                .on_action(cx.listener(Self::clear_results_action))
                .on_action(cx.listener(Self::copy_address))
                .on_action(cx.listener(Self::copy_value))
                .on_action(cx.listener(Self::copy_text))
                .child(table_overlay)
                .children(truncation_notice)
                .child(header_row)
                .child(
                    div()
                        .id("search-results-container")
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
                            let offset = item.offset;
                            let (encoding, buffer, address_map) = if let Some(ed) = &this.editor {
                                let ed_ref = ed.read(cx);
                                let doc = ed_ref.document.read().ok();
                                let buf = doc.as_ref().map(|d| d.buffer.clone());
                                let map = doc.as_ref().map(|d| d.address_map.clone()).unwrap_or_default();
                                (ed_ref.options.encoding, buf, map)
                            } else {
                                (Encoding::Ascii, None, crate::core::address_map::AddressMap::default())
                            };
                            let offset_value = format!("0x{:08X}", address_map.offset_to_address(offset));
                            let buffer_data = buffer.as_ref().map(|b| b.data());
                            let (preview_hex, preview_text) = format_row_previews(buffer_data, offset, this.match_len, encoding);
                            Some((offset_value, preview_hex, preview_text))
                        })
                    };
                    let Some((menu_offset, menu_hex, menu_text)) = selected_info else {
                        return menu;
                    };
                    menu.action_context(context_focus_handle.clone())
                        .menu_with_icon("Copy Address", IconName::Hash, Box::new(CopyAddress { value: menu_offset }))
                        .menu_with_icon("Copy Hex Value", IconName::Binary, Box::new(CopyValue { value: menu_hex }))
                        .menu_with_icon("Copy Text", IconName::TextInitial, Box::new(CopyText { value: menu_text }))
                })
                .into_any_element()
        };

        crate::ui::panels::panel_container(is_focused, theme)
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
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
            .on_action(cx.listener(Self::focus_table))
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

    #[test]
    fn test_format_row_previews() {
        let data = b"Hello, World!\x00\x01\x02\x03";
        let (hex, text) = format_row_previews(Some(data), 0, 5, Encoding::Ascii);
        assert_eq!(hex, "48 65 6C 6C 6F");
        assert_eq!(text, "Hello");
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
}

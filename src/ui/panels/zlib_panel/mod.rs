mod model;
mod table;
#[cfg(test)]
mod tests;

pub use table::ZlibTableDelegate;

use self::model::{
    CachedZlibState, MAX_CACHED_EDITORS, ScanSignature, ZlibResultRow, ZlibStateCache, build_zlib_rows, is_at_vertical_boundary, sort_zlib_indices,
};
use self::table::{CopyAddress, CopyPreview, DecompressEntry};
use crate::app_state::AppState;
use crate::core::buffer::Buffer;
use crate::core::dealloc::discard_in_background;
use crate::core::editor::Editor;
use crate::core::zlib::ZlibError;
use crate::ui::icon::IconName;
use gpui_kit::base::actions::{Cancel, SelectDown, SelectPageDown, SelectPageUp, SelectUp};
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::notification::Notification;
use gpui_kit::component::table::{ColumnSort, DataTable, TableEvent, TableState};
use gpui_kit::component::{ActiveTheme as _, Disableable, Sizable, Size, WindowExt as _, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::fmt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

actions!(zlib_panel, [ClearResults, ConfirmSelect, DismissTable]);

const CONTEXT: &str = "ZlibPanel";

/// Maximum number of zlib stream matches displayed per scan.
pub const MAX_ZLIB_RESULTS: usize = 10_000;

/// Registers keybindings for the Zlib search panel.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", DismissTable, Some("ZlibPanel")),
        KeyBinding::new("escape", DismissTable, Some("ZlibPanel > DataTable")),
        KeyBinding::new("enter", ConfirmSelect, Some("ZlibPanel > DataTable")),
    ]);
}

/// Events emitted by [`ZlibPanel`] to coordinate editor navigation and new decompressed tabs.
#[derive(Clone)]
pub enum ZlibPanelEvent {
    NavigateTo { offset: usize, len: usize },
    FocusEditor,
    OpenDecompressedDocument { source_path: PathBuf, address: usize, buffer: Buffer },
}

impl fmt::Debug for ZlibPanelEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NavigateTo { offset, len } => f.debug_struct("NavigateTo").field("offset", offset).field("len", len).finish(),
            Self::FocusEditor => write!(f, "FocusEditor"),
            Self::OpenDecompressedDocument { source_path, address, buffer } => f
                .debug_struct("OpenDecompressedDocument")
                .field("source_path", source_path)
                .field("address", address)
                .field("byte_len", &buffer.len())
                .finish(),
        }
    }
}

/// Sidebar panel for finding embedded zlib streams and opening decompressed payloads in new tabs.
pub struct ZlibPanel {
    editor: Option<Entity<Editor>>,
    focus_handle: FocusHandle,
    table_state: Entity<TableState<ZlibTableDelegate>>,
    is_scanning: bool,
    is_decompressing: bool,
    rows: Arc<[ZlibResultRow]>,
    results_id: u64,
    next_results_id: u64,
    is_truncated: bool,
    selected_index: Option<usize>,
    has_scanned: bool,
    scan_task: Option<Task<()>>,
    scan_cancel: Option<Arc<AtomicBool>>,
    decompress_task: Option<Task<()>>,
    decompress_cancel: Option<Arc<AtomicBool>>,
    sort_column: Option<(usize, ColumnSort)>,
    scan_generation: usize,
    decompress_generation: usize,
    scan_signature: Option<ScanSignature>,
    cached_states: ZlibStateCache,
    _table_subscription: Option<Subscription>,
    _editor_subscription: Option<Subscription>,
}

impl Drop for ZlibPanel {
    fn drop(&mut self) {
        if let Some(cancel) = self.scan_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        if let Some(cancel) = self.decompress_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
    }
}

impl EventEmitter<ZlibPanelEvent> for ZlibPanel {}

impl ZlibPanel {
    /// Creates a new [`ZlibPanel`] optionally bound to an initial [`Editor`].
    pub fn new(editor: Option<Entity<Editor>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let delegate = ZlibTableDelegate::new(cx.entity().downgrade(), focus_handle.clone());
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
                    cx.emit(ZlibPanelEvent::FocusEditor);
                    window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
                }
            }
            _ => {}
        });

        let mut this = Self {
            editor: None,
            focus_handle,
            table_state,
            is_scanning: false,
            is_decompressing: false,
            rows: Arc::default(),
            results_id: 0,
            next_results_id: 1,
            is_truncated: false,
            selected_index: None,
            has_scanned: false,
            scan_task: None,
            scan_cancel: None,
            decompress_task: None,
            decompress_cancel: None,
            sort_column: None,
            scan_generation: 0,
            decompress_generation: 0,
            scan_signature: None,
            cached_states: ZlibStateCache::new(MAX_CACHED_EDITORS),
            _table_subscription: Some(table_subscription),
            _editor_subscription: None,
        };

        this.set_editor(editor, cx);
        this
    }

    /// Rebinds the panel to `editor`, caching previous scanned results per editor instance.
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
                this.invalidate_if_content_changed(cx);
            }));
        }

        let restored = next_editor_id.is_some_and(|editor_id| self.restore_cached_state(editor_id, cx));
        if !restored {
            self.sync_table_state(cx);
        }
        cx.notify();
    }

    fn alloc_results_id(&mut self) -> u64 {
        let id = self.next_results_id;
        self.next_results_id = self.next_results_id.wrapping_add(1).max(1);
        id
    }

    fn cache_current_state(&mut self) {
        if !self.has_scanned {
            return;
        }
        let Some(editor) = &self.editor else {
            return;
        };

        self.cached_states.insert(
            editor.entity_id(),
            CachedZlibState {
                results_id: self.results_id,
                rows: std::mem::take(&mut self.rows),
                is_truncated: self.is_truncated,
                selected_index: self.selected_index,
                scan_signature: self.scan_signature,
            },
        );
    }

    fn restore_cached_state(&mut self, editor_id: EntityId, cx: &mut Context<Self>) -> bool {
        let Some(cached) = self.cached_states.take(editor_id) else {
            return false;
        };

        if cached.scan_signature != self.scan_signature {
            discard_in_background(cached.rows);
            return false;
        }

        self.results_id = cached.results_id;
        self.rows = cached.rows;
        self.is_truncated = cached.is_truncated;
        self.selected_index = cached.selected_index;
        self.has_scanned = true;
        self.sync_table_state(cx);
        true
    }

    fn current_signature(&self, cx: &App) -> Option<ScanSignature> {
        let editor = self.editor.as_ref()?.read(cx);
        let document = editor.document.read().expect("document read lock");
        Some(ScanSignature {
            content_state: document.history.state_id(),
            byte_len: document.buffer.len(),
        })
    }

    fn invalidate_if_content_changed(&mut self, cx: &mut Context<Self>) {
        let signature = self.current_signature(cx);
        if signature == self.scan_signature {
            return;
        }
        self.invalidate_scan();
        self.scan_signature = signature;
        self.sync_table_state(cx);
        cx.notify();
    }

    fn invalidate_scan(&mut self) {
        self.scan_generation = self.scan_generation.wrapping_add(1);
        if let Some(cancel) = self.scan_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.scan_task = None;
        self.is_scanning = false;

        self.decompress_generation = self.decompress_generation.wrapping_add(1);
        if let Some(cancel) = self.decompress_cancel.take() {
            cancel.store(true, Ordering::Relaxed);
        }
        self.decompress_task = None;
        self.is_decompressing = false;

        self.results_id = self.alloc_results_id();
        let old_rows = std::mem::take(&mut self.rows);
        if !old_rows.is_empty() {
            discard_in_background(old_rows);
        }
        self.is_truncated = false;
        self.selected_index = None;
        self.has_scanned = false;
    }

    fn reset_results(&mut self, cx: &mut Context<Self>) {
        self.invalidate_scan();
        self.sync_table_state(cx);
        cx.notify();
    }

    /// Scans the active document on a background thread via [`crate::service::ZlibService`].
    pub fn trigger_scan(&mut self, cx: &mut Context<Self>) {
        self.invalidate_scan();
        self.sync_table_state(cx);
        let generation = self.scan_generation;
        self.scan_signature = self.current_signature(cx);

        let Some(editor_entity) = &self.editor else {
            cx.notify();
            return;
        };

        let (buffer_data, segment_ranges) = {
            let editor = editor_entity.read(cx);
            let document = editor.document.read().expect("document read lock");
            (document.buffer.clone(), document.address_map.segment_ranges())
        };

        let cancel = Arc::new(AtomicBool::new(false));
        self.scan_cancel = Some(Arc::clone(&cancel));
        self.is_scanning = true;

        let service = AppState::global(cx).zlib_service.clone();
        let bg_task = service.scan_document_with(
            buffer_data,
            segment_ranges,
            MAX_ZLIB_RESULTS,
            cancel,
            |scan| {
                if scan.cancelled {
                    return (Arc::default(), false, true);
                }
                let rows = build_zlib_rows(&scan.matches);
                (rows, scan.is_truncated, false)
            },
            cx,
        );

        let task = cx.spawn(async move |this, cx| {
            let (rows, is_truncated, cancelled) = bg_task.await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if this.scan_generation != generation {
                        discard_in_background(rows);
                        return;
                    }

                    this.scan_task = None;
                    this.scan_cancel = None;
                    this.is_scanning = false;
                    if cancelled {
                        discard_in_background(rows);
                        cx.notify();
                        return;
                    }

                    this.is_truncated = is_truncated;
                    this.results_id = this.alloc_results_id();
                    this.rows = rows;
                    this.has_scanned = true;
                    this.sync_table_state(cx);
                    cx.notify();
                });
            } else {
                discard_in_background(rows);
            }
        });

        self.scan_task = Some(task);
        cx.notify();
    }

    /// Decompresses the currently selected zlib result on a background thread and emits
    /// [`ZlibPanelEvent::OpenDecompressedDocument`].
    pub fn decompress_selected(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.selected_index {
            self.decompress_index(index, window, cx);
        }
    }

    fn decompress_index(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_scanning || self.is_decompressing {
            return;
        }
        let Some(editor_entity) = self.editor.clone() else {
            return;
        };
        let Some(row) = self.rows.get(index) else {
            return;
        };
        let offset = row.offset;
        let compressed_len = row.compressed_len;
        let expected_decompressed_len = row.decompressed_len;

        let (buffer_data, source_path, address) = {
            let editor = editor_entity.read(cx);
            let document = editor.document.read().expect("document read lock");
            (
                document.buffer.clone(),
                document.path().to_path_buf(),
                document.address_map.offset_to_address(offset),
            )
        };

        self.decompress_generation = self.decompress_generation.wrapping_add(1);
        let generation = self.decompress_generation;
        let cancel = Arc::new(AtomicBool::new(false));
        self.decompress_cancel = Some(Arc::clone(&cancel));
        self.is_decompressing = true;

        let service = AppState::global(cx).zlib_service.clone();
        let bg_task = service.decompress_stream(buffer_data, offset, compressed_len, Some(expected_decompressed_len), cancel, cx);

        let task = cx.spawn_in(window, async move |this, window| {
            let result = bg_task.await;
            let _ = window.update(|window, cx| {
                if let Some(this) = this.upgrade() {
                    this.update(cx, |this, cx| {
                        if this.decompress_generation != generation {
                            return;
                        }
                        this.decompress_task = None;
                        this.decompress_cancel = None;
                        this.is_decompressing = false;
                        match result {
                            Ok(buffer) => {
                                cx.emit(ZlibPanelEvent::OpenDecompressedDocument { source_path, address, buffer });
                            }
                            Err(ZlibError::Cancelled) => {}
                            Err(err) => {
                                window.push_notification(Notification::error(format!("Failed to decompress zlib stream: {err}")), cx);
                            }
                        }
                        cx.notify();
                    });
                }
            });
        });

        self.decompress_task = Some(task);
        cx.notify();
    }

    pub(super) fn on_table_sorted(&mut self, sort_col: Option<(usize, ColumnSort)>, selected_row: Option<usize>, cx: &mut Context<Self>) {
        self.sort_column = sort_col;
        self.table_state.update(cx, |table, cx| {
            if let Some(row_ix) = selected_row {
                if table.selected_row() != Some(row_ix) {
                    table.set_selected_row(row_ix, cx);
                }
            } else {
                table.clear_selection(cx);
            }
        });
        cx.notify();
    }

    fn is_at_vertical_boundary(&self, moving_down: bool, cx: &App) -> bool {
        let table = self.table_state.read(cx);
        let rows_count = table.delegate().visible_indices().len();
        is_at_vertical_boundary(moving_down, table.selected_row(), rows_count)
    }

    fn sorted_indices(&self) -> Vec<usize> {
        let mut indices: Vec<usize> = (0..self.rows.len()).collect();
        if self.sort_column.is_some_and(|(_, sort)| sort != ColumnSort::Default) {
            sort_zlib_indices(&self.rows, &mut indices, self.sort_column);
        }
        indices
    }

    fn resolve_visible_row_index(&self, row_ix: usize, cx: &App) -> Option<usize> {
        self.table_state.read(cx).delegate().visible_indices().get(row_ix).copied()
    }

    fn result_summary(&self) -> String {
        if self.is_scanning {
            "Searching...".to_string()
        } else if self.is_truncated {
            format!("{MAX_ZLIB_RESULTS}+ streams found (showing first {MAX_ZLIB_RESULTS} streams)")
        } else {
            let total = self.rows.len();
            let label = if total == 1 { "stream" } else { "streams" };
            format!("{total} {label} found")
        }
    }

    fn sync_table_state(&mut self, cx: &mut Context<Self>) {
        let visible_indices = self.sorted_indices();
        if let Some(index) = self.selected_index {
            if !visible_indices.contains(&index) {
                self.selected_index = visible_indices.first().copied();
            }
        } else {
            self.selected_index = visible_indices.first().copied();
        }

        let row_ix = self.selected_index.and_then(|idx| visible_indices.iter().position(|&i| i == idx));
        let results_id = self.results_id;
        let rows = Arc::clone(&self.rows);
        let editor = self.editor.clone();
        let sort_column = self.sort_column;
        let selected_index = self.selected_index;

        self.table_state.update(cx, |table, cx| {
            table
                .delegate_mut()
                .sync_state(results_id, rows, visible_indices, sort_column, editor, selected_index);
            table.refresh(cx);
            if let Some(row_ix) = row_ix {
                table.set_selected_row(row_ix, cx);
            } else {
                table.clear_selection(cx);
            }
        });
    }

    fn select_item_at_row(&mut self, index: usize, known_row_ix: Option<usize>, from_table_selection: bool, cx: &mut Context<Self>) {
        if let Some(row) = self.rows.get(index) {
            let selection_changed = self.selected_index != Some(index);
            self.selected_index = Some(index);
            let offset = row.offset;
            let len = row.compressed_len;

            self.table_state.update(cx, |table, cx| {
                let delegate = table.delegate_mut();
                delegate.set_selected_index(Some(index));
                if !from_table_selection {
                    let row_ix = known_row_ix
                        .filter(|&row| delegate.visible_indices().get(row) == Some(&index))
                        .or_else(|| delegate.visible_indices().iter().position(|&i| i == index));
                    if let Some(row_ix) = row_ix
                        && table.selected_row() != Some(row_ix)
                    {
                        table.set_selected_row(row_ix, cx);
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
            cx.emit(ZlibPanelEvent::NavigateTo { offset, len });
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

    fn copy_preview(&mut self, action: &CopyPreview, window: &mut Window, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(action.value.clone()));
        window.push_notification(Notification::info("Copied preview"), cx);
    }

    fn decompress_entry(&mut self, action: &DecompressEntry, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.rows.get(action.index) else {
            return;
        };
        if row.offset != action.offset || row.compressed_len != action.compressed_len {
            return;
        }
        self.select_item_at_row(action.index, None, false, cx);
        self.decompress_index(action.index, window, cx);
    }

    fn confirm_select(&mut self, _: &ConfirmSelect, window: &mut Window, cx: &mut Context<Self>) {
        let target = {
            let table = self.table_state.read(cx);
            let visible_indices = table.delegate().visible_indices();
            self.selected_index
                .and_then(|index| visible_indices.iter().position(|&i| i == index).map(|row| (index, row)))
                .or_else(|| visible_indices.first().copied().map(|index| (index, 0)))
        };
        if let Some((index, row_ix)) = target {
            self.select_item_at_row(index, Some(row_ix), false, cx);
        }
        cx.emit(ZlibPanelEvent::FocusEditor);
        window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
    }

    fn dismiss_table(&mut self, _: &DismissTable, window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(ZlibPanelEvent::FocusEditor);
        window.dispatch_action(Box::new(crate::actions::FocusHexView), cx);
    }

    fn clear_results(&mut self, _: &ClearResults, _: &mut Window, cx: &mut Context<Self>) {
        self.reset_results(cx);
    }
}

impl Render for ZlibPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let is_table_focused = self.table_state.read(cx).focus_handle(cx).is_focused(window);
        let is_focused = self.focus_handle.is_focused(window) || is_table_focused;
        let has_editor = self.editor.is_some();
        let can_decompress_selected = has_editor && self.selected_index.is_some() && !self.is_scanning && !self.is_decompressing;

        let actions = Button::new("clear-zlib")
            .ghost()
            .icon(IconName::Eraser)
            .with_size(Size::XSmall)
            .tooltip("Clear results")
            .disabled(!self.has_scanned && !self.is_scanning)
            .on_click(cx.listener(|this, _, _window, cx| {
                this.reset_results(cx);
            }));
        let header = crate::ui::panels::panel_header("ZLIB", is_focused, theme, None, Some(actions.into_any_element()));

        let scan_controls = h_flex().p_2().gap_2().border_b_1().border_color(theme.border).items_center().child(
            div().flex_1().child(
                Button::new("search-zlib")
                    .label(if self.is_scanning { "Searching..." } else { "Search" })
                    .icon(IconName::Search)
                    .primary()
                    .with_size(Size::Small)
                    .w_full()
                    .disabled(self.is_scanning || !has_editor)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.trigger_scan(cx);
                    })),
            ),
        );

        let result_summary_text = (self.has_scanned || self.is_scanning).then(|| self.result_summary());

        let body = if self.is_scanning {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(theme.muted_foreground)
                .child("Searching...")
                .into_any_element()
        } else if self.rows.is_empty() {
            let message = if self.has_scanned {
                "No zlib streams found"
            } else if has_editor {
                "Click Search to find zlib streams"
            } else {
                "Open a file to search zlib streams"
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
                // Stop vertical selection actions at the first/last row before `DataTable`
                // handles them so selection neither wraps nor triggers `set_selected_row`
                // scrolling at the boundary.
                .capture_action(cx.listener(|this, _: &SelectUp, _window, cx| {
                    if this.is_at_vertical_boundary(false, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|this, _: &SelectDown, _window, cx| {
                    if this.is_at_vertical_boundary(true, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|this, _: &SelectPageUp, _window, cx| {
                    if this.is_at_vertical_boundary(false, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_action(cx.listener(|this, _: &SelectPageDown, _window, cx| {
                    if this.is_at_vertical_boundary(true, cx) {
                        cx.stop_propagation();
                    }
                }))
                .capture_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                    if event.keystroke.key == "escape" {
                        this.dismiss_table(&DismissTable, window, cx);
                        cx.stop_propagation();
                    }
                }))
                .into_any_element()
        };

        let show_footer = !self.is_scanning && !self.rows.is_empty();

        crate::ui::panels::panel_container(is_focused, theme)
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::confirm_select))
            .on_action(cx.listener(Self::dismiss_table))
            .on_action(cx.listener(|this, _: &Cancel, window, cx| {
                this.dismiss_table(&DismissTable, window, cx);
            }))
            .on_action(cx.listener(Self::copy_address))
            .on_action(cx.listener(Self::copy_preview))
            .on_action(cx.listener(Self::decompress_entry))
            .on_action(cx.listener(Self::clear_results))
            .child(header)
            .child(scan_controls)
            .when_some(result_summary_text, |this, summary| {
                this.child(
                    div()
                        .px_2()
                        .py_1()
                        .bg(theme.secondary)
                        .border_b_1()
                        .border_color(theme.border)
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(summary),
                )
            })
            .child(body)
            .when(show_footer, |this| {
                let footer = h_flex()
                    .p_2()
                    .gap_2()
                    .border_t_1()
                    .border_color(theme.border)
                    .bg(theme.sidebar)
                    .items_center()
                    .child(
                        div().flex_1().child(
                            Button::new("open-decompressed-zlib")
                                .label(if self.is_decompressing { "Decompressing..." } else { "Open Decompressed" })
                                .icon(IconName::ExternalLink)
                                .primary()
                                .with_size(Size::Small)
                                .w_full()
                                .tooltip("Decompress selected zlib stream into a new document tab")
                                .disabled(!can_decompress_selected)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.decompress_selected(window, cx);
                                })),
                        ),
                    );
                this.child(footer)
            })
    }
}

impl Focusable for ZlibPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

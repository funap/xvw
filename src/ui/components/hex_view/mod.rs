pub mod actions;
pub mod clipboard_handler;
pub mod input_controller;
pub mod layout;
pub mod paint;
pub mod scroll_controller;
pub mod types;

pub use clipboard_handler::ClipboardHandler;
pub use input_controller::InputController;
use input_controller::{HexCommit, HexInputResult};
pub use scroll_controller::ScrollController;

#[cfg(test)]
mod layout_tests;

pub use actions::init;
pub use actions::*;
pub use layout::{
    ascii_byte_index_from_world_x, bounded_auto_fit_range, build_hex_text_source, calculate_scroll_top_for_range, can_chain_to_outer, hex_grid_width,
    hex_grid_x, hex_group_x, make_hex_view_layout, measure_hex_cell_width, weighted_text_width,
};
pub use paint::{RowPaintParams, paint_hex_row, paint_scrollbar};
pub use types::*;

use crate::actions::{
    AddCustomBreak, BookmarkBlue, BookmarkCyan, BookmarkGreen, BookmarkOrange, BookmarkPink, BookmarkPurple, BookmarkRed, BookmarkYellow, ClearAllBookmarks,
    ClearAllCustomBreaks, ClearBookmark, ClearStructureDefinition, Copy, CopyAsBase64, CopyAsBinary, CopyAsCppArray, CopyAsEscapedString, CopyAsHexDump,
    CopyAsHexSpaces, CopyAsHexStream, CopyAsJsonArray, CopyAsPrintableText, CopyAsRustArray, Cut, ExportBookmarks, FillSelection, HideAllBookmarks,
    ImportBookmarks, JoinLine, LoadStructureDefinition, Paste, Redo, RemoveCustomBreakBackward, RemoveCustomBreakForward, SearchNext, SearchPrev,
    SelectAll as AppSelectAll, SetByteOrderBigEndian, SetByteOrderLittleEndian, SetGroupSize1, SetGroupSize2, SetGroupSize4, SetGroupSize8, SetRadixBin,
    SetRadixDec, SetRadixHex, SetRadixOct, ShowAllBookmarks, ShowBookmarksTab, ShowOnlyBookmarkBlue, ShowOnlyBookmarkCyan, ShowOnlyBookmarkGreen,
    ShowOnlyBookmarkOrange, ShowOnlyBookmarkPink, ShowOnlyBookmarkPurple, ShowOnlyBookmarkRed, ShowOnlyBookmarkYellow, ShowStructureTab, ToggleBookmarkBlue,
    ToggleBookmarkCyan, ToggleBookmarkGreen, ToggleBookmarkOrange, ToggleBookmarkPink, ToggleBookmarkPurple, ToggleBookmarkRed, ToggleBookmarkYellow,
    ToggleByteOrder, ToggleHideUnbookmarked, ToggleInlineStructureView, ToggleSearch, Undo, UnfoldBookmarkAtCursor,
};
use crate::app_state::InsertModeState;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::format::CopyFormat;
use crate::core::radix::{ByteGroupSize, DisplayRadix};

use crate::core::structure::{IndexedField, ParseResult};
use gpui_kit::component::menu::ContextMenuExt;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

const HEX_CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);
const HEX_CURSOR_PAUSE_DELAY: Duration = Duration::from_millis(300);

/// Keeps the Insert Mode cursor in sync with the blinking behavior of
/// `gpui-kit`'s text inputs.
struct HexCursorBlink {
    visible: bool,
    paused: bool,
    epoch: usize,
    _task: Task<()>,
}

impl HexCursorBlink {
    fn new() -> Self {
        Self {
            visible: false,
            paused: false,
            epoch: 0,
            _task: Task::ready(()),
        }
    }

    fn start(&mut self, cx: &mut Context<Self>) {
        self.blink(self.epoch, cx);
    }

    fn stop(&mut self, cx: &mut Context<Self>) {
        self.epoch = 0;
        cx.notify();
    }

    fn next_epoch(&mut self) -> usize {
        self.epoch += 1;
        self.epoch
    }

    fn blink(&mut self, epoch: usize, cx: &mut Context<Self>) {
        if self.paused || epoch != self.epoch {
            self.visible = true;
            return;
        }

        self.visible = !self.visible;
        cx.notify();

        let epoch = self.next_epoch();
        self._task = cx.spawn(async move |this, cx| {
            tokio::time::sleep(HEX_CURSOR_BLINK_INTERVAL).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| this.blink(epoch, cx));
            }
        });
    }

    fn visible(&self) -> bool {
        self.paused || self.visible
    }

    fn pause(&mut self, cx: &mut Context<Self>) {
        self.paused = true;
        self.visible = true;
        cx.notify();

        let epoch = self.next_epoch();
        self._task = cx.spawn(async move |this, cx| {
            tokio::time::sleep(HEX_CURSOR_PAUSE_DELAY).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.paused = false;
                    this.blink(epoch, cx);
                });
            }
        });
    }
}

pub struct HexView {
    editor: Entity<Editor>,
    focus_handle: FocusHandle,
    cursor_blink: Entity<HexCursorBlink>,
    pub scroll: ScrollController,
    is_selecting: bool,
    mouse_selection_anchor: Option<usize>,
    pub input: InputController,
    bounds: std::cell::Cell<Option<Bounds<Pixels>>>,
    list_bounds: std::cell::Cell<Option<Bounds<Pixels>>>,
    highlights: Arc<Vec<(Range<usize>, Hsla)>>,
    max_highlight_len: usize,
    show_offset: bool,
    show_header: bool,
    show_ascii: bool,
    encoding: Encoding,
    radix: DisplayRadix,
    group_size: ByteGroupSize,
    is_big_endian: bool,
    font_family_prop: SharedString,
    font_size_prop: Pixels,
    pub address_col_width: f32,
    pub hex_col_width: f32,
    hex_cell_width: f32,
    hex_content_width: f32,
    pub ascii_col_width: f32,
    pub desc_col_width: f32,
    pub comment_col_width: f32,
    cached_comment_content_width: std::cell::Cell<Option<f32>>,
    cached_desc_content_width: std::cell::Cell<Option<f32>>,
    resizing_column: Option<(ResizingColumn, f32, f32)>,
    last_cursor_offset: Option<usize>,
    last_max_bytes_per_row: Option<usize>,
    cursor_reveal_pending: bool,
    _editor_subscription: Subscription,
    _insert_mode_subscription: Subscription,
    _cursor_blink_subscription: Subscription,
    _window_activation_subscription: Subscription,
}

impl EventEmitter<HexViewEvent> for HexView {}

#[allow(dead_code)]
impl HexView {
    pub fn new(editor: Entity<Editor>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (radix, group_size, is_big_endian, encoding) = {
            let ed = editor.read(cx);
            (ed.options.radix, ed.options.group_size, ed.options.is_big_endian, ed.options.encoding)
        };
        let font_size_prop = px(14.0);
        let hex_col_width = 0.0;
        let focus_handle = cx.focus_handle();
        let cursor_blink = cx.new(|_| HexCursorBlink::new());
        let _cursor_blink_subscription = cx.observe(&cursor_blink, |_, _, cx| cx.notify());
        let _window_activation_subscription = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() && this.focus_handle.is_focused(window) {
                this.cursor_blink.update(cx, |cursor, cx| {
                    cursor.start(cx);
                });
            }
        });

        cx.on_focus_in(&focus_handle, window, |this, _, cx| {
            this.cursor_blink.update(cx, |cursor, cx| {
                cursor.start(cx);
            });
            cx.notify();
        })
        .detach();
        cx.on_focus_out(&focus_handle, window, |this, _, _, cx| {
            this.cursor_blink.update(cx, |cursor, cx| {
                cursor.stop(cx);
            });
            cx.notify();
        })
        .detach();

        let _editor_subscription = cx.observe(&editor, |this, editor_entity, cx| {
            this.cached_comment_content_width.set(None);
            this.clear_desc_content_width_cache();
            let ed = editor_entity.read(cx);
            if ed.is_read_only() {
                this.clear_pending_hex_input();
            }
            let new_encoding = ed.options.encoding;
            let new_radix = ed.options.radix;
            let new_group_size = ed.options.group_size;
            let new_endian = ed.options.is_big_endian;
            let cursor_offset = if InsertModeState::is_enabled(cx) {
                ed.insert_cursor_offset()
            } else {
                ed.cursor.offset
            };
            let cursor_changed = this.last_cursor_offset != Some(cursor_offset);
            if cursor_changed {
                this.cursor_reveal_pending = true;
                this.cursor_blink.update(cx, |cursor, cx| {
                    cursor.pause(cx);
                });
            }

            if this.encoding != new_encoding {
                this.encoding = new_encoding;
            }
            if this.radix != new_radix || this.group_size != new_group_size || this.is_big_endian != new_endian {
                this.radix = new_radix;
                this.group_size = new_group_size;
                this.is_big_endian = new_endian;
                this.hex_content_width = 0.0;
                this.cursor_reveal_pending = true;
            }
            this.clamp_scroll_offsets(cx);
            if cursor_changed {
                this.ensure_cursor_visible(cx);
            }
            cx.notify();
        });

        let _insert_mode_subscription = cx.observe_global::<InsertModeState>(|this, cx| {
            this.pause_cursor_blink(cx);
            this.clear_pending_hex_input();
            this.cursor_reveal_pending = true;
            this.ensure_cursor_visible(cx);
            cx.notify();
        });

        Self {
            editor,
            focus_handle,
            cursor_blink,
            scroll: ScrollController::default(),
            is_selecting: false,
            mouse_selection_anchor: None,
            input: InputController::new(),
            bounds: std::cell::Cell::new(None),
            list_bounds: std::cell::Cell::new(None),
            highlights: Arc::new(Vec::new()),
            max_highlight_len: 0,
            show_offset: true,
            show_header: true,
            show_ascii: true,
            encoding,
            radix,
            group_size,
            is_big_endian,
            font_family_prop: "Zed Sans Mono".into(),
            font_size_prop,
            address_col_width: ADDRESS_WIDTH,
            hex_col_width,
            hex_cell_width: 0.0,
            hex_content_width: 0.0,
            ascii_col_width: 0.0,
            desc_col_width: DESC_WIDTH,
            comment_col_width: COMMENT_WIDTH,
            cached_comment_content_width: std::cell::Cell::new(None),
            cached_desc_content_width: std::cell::Cell::new(None),
            resizing_column: None,
            last_cursor_offset: None,
            last_max_bytes_per_row: None,
            cursor_reveal_pending: true,
            _editor_subscription,
            _insert_mode_subscription,
            _cursor_blink_subscription,
            _window_activation_subscription,
        }
    }

    pub fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }

    pub fn layout_state(&self) -> HexViewLayoutState {
        HexViewLayoutState {
            address_col_width: self.address_col_width,
            hex_col_width: self.hex_col_width,
            desc_col_width: self.desc_col_width,
            comment_col_width: self.comment_col_width,
            ascii_col_width: self.ascii_col_width,
            show_offset: self.show_offset,
            show_ascii: self.show_ascii,
            show_header: self.show_header,
            scroll_offset: self.scroll.scroll_offset,
            outer_scroll_x: self.scroll.outer_scroll_x,
            hex_scroll_x: self.scroll.hex_scroll_x,
            ascii_scroll_x: self.scroll.ascii_scroll_x,
            desc_scroll_x: self.scroll.desc_scroll_x,
            comment_scroll_x: self.scroll.comment_scroll_x,
        }
    }

    pub fn apply_layout_state(&mut self, state: &HexViewLayoutState) {
        self.address_col_width = state.address_col_width;
        self.hex_col_width = state.hex_col_width;
        self.desc_col_width = state.desc_col_width;
        self.comment_col_width = state.comment_col_width;
        self.ascii_col_width = state.ascii_col_width;
        self.show_offset = state.show_offset;
        self.show_ascii = state.show_ascii;
        self.show_header = state.show_header;
        self.scroll.scroll_offset = state.scroll_offset;
        self.scroll.outer_scroll_x = state.outer_scroll_x;
        self.scroll.hex_scroll_x = state.hex_scroll_x;
        self.scroll.ascii_scroll_x = state.ascii_scroll_x;
        self.scroll.desc_scroll_x = state.desc_scroll_x;
        self.scroll.comment_scroll_x = state.comment_scroll_x;
        self.cached_comment_content_width.set(None);
        self.clear_desc_content_width_cache();
    }

    pub fn copy_layout_from(&mut self, source: &HexView) {
        self.apply_layout_state(&source.layout_state());
    }

    pub fn max_hex_scroll(&self, cx: &App) -> f32 {
        let _ = cx;
        (self.hex_content_width - self.hex_col_width).max(0.0)
    }

    fn clear_desc_content_width_cache(&mut self) {
        self.cached_desc_content_width.set(None);
    }

    pub fn max_desc_scroll(&self, cx: &App) -> f32 {
        let _ = cx;
        if let Some(max_content_w) = self.cached_desc_content_width.get() {
            let max_w = (max_content_w + 32.0).max(self.desc_col_width);
            return (max_w - self.desc_col_width).max(0.0);
        }

        // Description measurement is an explicit user action. Until the
        // header is double-clicked, avoid scanning the parse result from the
        // render path altogether.
        0.0
    }

    fn auto_fit_scan_range(&self, cx: &App) -> Range<usize> {
        let (visible_start, visible_end) = self.viewport_byte_range(cx);
        let total_size = self.editor.read(cx).total_size();
        bounded_auto_fit_range(total_size, visible_start, visible_end)
    }

    fn visible_auto_fit_scan_range(&self, cx: &App) -> Range<usize> {
        let (visible_start, visible_end) = self.viewport_byte_range(cx);
        let total_size = self.editor.read(cx).total_size();
        let start = visible_start.min(total_size);
        let end = visible_end.clamp(start, total_size);
        start..end
    }

    pub fn description_content_width_in_range(editor: &Editor, parse_result: &ParseResult, scan_range: &Range<usize>, char_w: f32) -> f32 {
        let line_starts = editor.line_starts();
        let total_size = editor.total_size();
        Self::description_content_width_for_line_map(
            &line_starts,
            total_size,
            parse_result,
            scan_range,
            &editor.structure.collapsed_struct_ids,
            char_w,
        )
    }

    fn description_content_width_for_line_map(
        line_starts: &crate::core::layout::LineMap,
        total_size: usize,
        parse_result: &ParseResult,
        scan_range: &Range<usize>,
        collapsed_structs: &std::collections::HashSet<String>,
        char_w: f32,
    ) -> f32 {
        let scan_len = scan_range.end.saturating_sub(scan_range.start);
        if scan_len == 0 {
            return 0.0;
        }

        // Include a possible structure header immediately before the first
        // data row in the scan range.
        let start_row = Editor::find_line_index(scan_range.start, line_starts).saturating_sub(1);
        let end_row = if scan_range.end >= total_size {
            line_starts.len()
        } else {
            Editor::find_line_index(scan_range.end, line_starts) + 1
        };

        let mut max_width: f32 = 0.0;

        for (row_count, row) in (start_row..end_row).enumerate() {
            if row_count >= AUTO_FIT_MAX_ITEMS {
                break;
            }

            let Some(offset) = line_starts.get(row) else {
                continue;
            };
            let next_offset = line_starts.get(row + 1).unwrap_or(total_size);
            let chunk_len = next_offset.saturating_sub(offset);
            if chunk_len == 0 {
                let containers: Cow<'_, [IndexedField]> = if parse_result.is_live() {
                    Cow::Owned(parse_result.find_live_container_structs_starting_at(offset, 1))
                } else {
                    Cow::Borrowed(parse_result.find_container_structs_starting_at(offset, 1))
                };
                for container in containers.iter().filter(|container| !container.is_instance && container.size > 0) {
                    let text = if collapsed_structs.contains(&container.id) {
                        format!("▶ {} ({} bytes)", container.format_container_label(), container.size)
                    } else {
                        format!("▼ {}", container.format_container_label())
                    };
                    let row_width = container.depth as f32 * 14.0 + weighted_text_width(&text, char_w) + 8.0;
                    max_width = max_width.max(row_width);
                }
                continue;
            }

            let container_structs: Cow<'_, [IndexedField]> = if parse_result.is_live() {
                Cow::Owned(parse_result.find_live_container_structs_starting_at(offset, chunk_len))
            } else {
                Cow::Borrowed(parse_result.find_container_structs_starting_at(offset, chunk_len))
            };
            let leaf_fields: Cow<'_, [IndexedField]> = if parse_result.is_live() {
                Cow::Owned(parse_result.find_live_leaf_fields_starting_at(offset, chunk_len))
            } else {
                Cow::Borrowed(parse_result.find_leaf_fields_starting_at(offset, chunk_len))
            };

            if container_structs.is_empty() && leaf_fields.is_empty() {
                continue;
            }

            let active_ranges = if parse_result.is_live() {
                Vec::new()
            } else {
                parse_result.find_active_struct_ranges(offset, chunk_len)
            };
            let is_collapsed = container_structs.first().map(|c| collapsed_structs.contains(&c.id)).unwrap_or(false);

            let struct_depth = active_ranges.len().saturating_sub(1);
            let indent_level = if !container_structs.is_empty() {
                active_ranges
                    .iter()
                    .find(|r| container_structs.first().map(|c| c.id == r.id).unwrap_or(false))
                    .map(|r| r.depth)
                    .or_else(|| container_structs.first().map(|container| container.depth))
                    .unwrap_or(struct_depth)
            } else if active_ranges.is_empty() {
                leaf_fields.first().map(|field| field.depth).unwrap_or(0)
            } else {
                active_ranges.len()
            };
            let indent_px = indent_level as f32 * 14.0;

            let mut parts_width = 0.0;
            let mut part_count = 0;

            if let Some(container) = container_structs.first() {
                let text = if is_collapsed {
                    format!("▶ {} ({} bytes)", container.format_container_label(), container.size)
                } else {
                    format!("▼ {}", container.format_container_label())
                };
                parts_width += weighted_text_width(&text, char_w);
                part_count += 1;
            }

            if !is_collapsed {
                for f in leaf_fields.iter() {
                    let expr = f.format_expression();
                    parts_width += weighted_text_width(expr, char_w);
                    part_count += 1;
                }
            }

            if part_count > 0 {
                let spacing_width = (part_count - 1) as f32 * (char_w * 2.0);
                let row_width = indent_px + parts_width + spacing_width + 8.0;
                max_width = max_width.max(row_width);
            }
        }

        max_width
    }

    pub fn comment_content_width_in_range(editor: &Editor, scan_range: &Range<usize>, char_w: f32) -> f32 {
        if scan_range.is_empty() {
            return 0.0;
        }

        let line_starts = editor.line_starts();
        let bookmarks = editor.bookmarks().snapshot();
        let mut start_idx = bookmarks.partition_point(|bookmark| bookmark.offset < scan_range.start);
        start_idx = start_idx.saturating_sub(1);
        let end_idx = bookmarks.partition_point(|bookmark| bookmark.offset < scan_range.end);

        use std::collections::HashMap;
        let mut row_widths: HashMap<usize, f32> = HashMap::new();
        for bookmark in bookmarks[start_idx..end_idx].iter().take(AUTO_FIT_MAX_ITEMS) {
            let trimmed = bookmark.comment.trim();
            if trimmed.is_empty() {
                continue;
            }
            let row = Editor::find_line_index(bookmark.offset, &line_starts);
            let item_width = 8.0 + 5.0 + weighted_text_width(trimmed, char_w) + 14.0;
            *row_widths.entry(row).or_insert(8.0) += item_width;
        }

        row_widths.values().copied().fold(0.0, f32::max)
    }

    pub fn max_comment_scroll(&self, cx: &App) -> f32 {
        if let Some(max_content_w) = self.cached_comment_content_width.get() {
            let max_w = (max_content_w + 32.0).max(self.comment_col_width);
            return (max_w - self.comment_col_width).max(0.0);
        }

        let scan_range = self.auto_fit_scan_range(cx);
        let editor = self.editor.read(cx);
        let char_w = f32::from(self.font_size_prop) * 0.61;
        let max_content_w = Self::comment_content_width_in_range(editor, &scan_range, char_w);
        self.cached_comment_content_width.set(Some(max_content_w));
        let max_w = (max_content_w + 32.0).max(self.comment_col_width);
        (max_w - self.comment_col_width).max(0.0)
    }

    fn default_ascii_col_width(max_bytes_per_row: usize) -> f32 {
        max_bytes_per_row.max(1) as f32 * ASCII_CELL_WIDTH + ASCII_EXTRA_WIDTH
    }

    fn effective_ascii_col_width(&self, max_bytes_per_row: usize) -> f32 {
        if self.ascii_col_width > 0.0 {
            self.ascii_col_width
        } else {
            Self::default_ascii_col_width(max_bytes_per_row)
        }
    }

    fn current_layout(&self, cx: &App) -> HexViewLayout {
        let (max_bytes_per_row, is_struct_mode) = {
            let editor = self.editor.read(cx);
            let line_starts = editor.line_starts();
            (
                line_starts.max_bytes_per_row(),
                editor.structure.show_inline_structure_view && editor.parse_result().is_some(),
            )
        };
        let bounds_width = self
            .list_bounds
            .get()
            .or_else(|| self.bounds.get())
            .map(|bounds| f32::from(bounds.size.width))
            .unwrap_or(800.0);

        make_hex_view_layout(LayoutInput {
            bounds_width,
            is_struct_mode,
            show_ascii: self.show_ascii,
            ascii_col_width: self.effective_ascii_col_width(max_bytes_per_row),
            ascii_inner_max: if !is_struct_mode && self.show_ascii {
                (Self::default_ascii_col_width(max_bytes_per_row) - self.effective_ascii_col_width(max_bytes_per_row)).max(0.0)
            } else {
                0.0
            },
            fixed_column_width: if is_struct_mode {
                self.address_col_width
            } else if self.show_offset {
                OFFSET_WIDTH
            } else {
                0.0
            },
            hex_col_width: self.hex_col_width,
            desc_col_width: self.desc_col_width,
            comment_col_width: self.comment_col_width,
            hex_inner_max: self.max_hex_scroll(cx),
            desc_inner_max: if is_struct_mode { self.max_desc_scroll(cx) } else { 0.0 },
            comment_inner_max: self.max_comment_scroll(cx),
            section_gap: SECTION_GAP,
            content_padding: 8.0,
            scrollbar_width: VERTICAL_SCROLLBAR_WIDTH,
        })
    }

    fn horizontal_offset(&self, target: HorizontalScrollTarget) -> f32 {
        self.scroll.horizontal_offset(target)
    }

    fn set_horizontal_offset(&mut self, target: HorizontalScrollTarget, offset: f32, layout: HexViewLayout, emit: bool, cx: &mut Context<Self>) -> bool {
        let (changed, new_offset) = self.scroll.set_horizontal_offset(target, offset, layout);
        if !changed {
            return false;
        }

        cx.notify();
        if emit {
            cx.emit(HexViewEvent::HorizontalScrolled {
                target,
                progress: layout.progress(target, new_offset),
            });
        }
        true
    }

    fn sync_outer_scroll_from_handle(&mut self, layout: HexViewLayout, cx: &mut Context<Self>) {
        let handle_offset = (-f32::from(self.scroll.outer_scroll_handle.offset().x)).max(0.0);
        let _ = self.set_horizontal_offset(HorizontalScrollTarget::View, handle_offset, layout, true, cx);
    }

    pub fn set_horizontal_scroll(&mut self, target: HorizontalScrollTarget, progress: f32, cx: &mut Context<Self>) {
        let layout = self.current_layout(cx);
        let offset = layout.max_offset(target) * progress.clamp(0.0, 1.0);
        let _ = self.set_horizontal_offset(target, offset, layout, false, cx);
    }

    pub fn clamp_scroll_offsets(&mut self, cx: &App) {
        let max_hex = self.max_hex_scroll(cx);
        let max_desc = self.max_desc_scroll(cx);
        let max_comment = self.max_comment_scroll(cx);
        let layout = self.current_layout(cx);
        self.scroll.clamp_scroll_offsets(max_hex, max_desc, max_comment, layout);
    }

    pub fn auto_fit_column(&mut self, col: ResizingColumn, cx: &mut Context<Self>) {
        match col {
            ResizingColumn::Address => {
                self.address_col_width = ADDRESS_WIDTH;
            }
            ResizingColumn::Hex => {
                if self.hex_content_width > 0.0 {
                    self.hex_col_width = self.hex_content_width;
                }
            }
            ResizingColumn::Ascii => {
                let max_bytes_per_row = self.editor.read(cx).line_starts().max_bytes_per_row();
                self.ascii_col_width = Self::default_ascii_col_width(max_bytes_per_row);
                self.scroll.ascii_scroll_x = 0.0;
            }
            ResizingColumn::Description => {
                let scan_range = self.visible_auto_fit_scan_range(cx);
                let editor = self.editor.read(cx);
                if let Some(parse_res) = editor.parse_result() {
                    let char_w = f32::from(self.font_size_prop) * 0.61;
                    let max_content_w = Self::description_content_width_in_range(editor, &parse_res, &scan_range, char_w);
                    self.cached_desc_content_width.set(Some(max_content_w));
                    self.desc_col_width = if max_content_w > 0.0 {
                        (max_content_w + 24.0).max(DESC_WIDTH)
                    } else {
                        DESC_WIDTH
                    };
                    self.scroll.desc_scroll_x = 0.0;
                } else {
                    self.desc_col_width = DESC_WIDTH;
                    self.scroll.desc_scroll_x = 0.0;
                }
            }
            ResizingColumn::Comment => {
                let scan_range = self.auto_fit_scan_range(cx);
                let editor = self.editor.read(cx);
                let char_w = f32::from(self.font_size_prop) * 0.61;
                let max_content_w = Self::comment_content_width_in_range(editor, &scan_range, char_w);
                self.cached_comment_content_width.set(Some(max_content_w));
                self.comment_col_width = if max_content_w > 0.0 {
                    (max_content_w + 24.0).max(COMMENT_WIDTH)
                } else {
                    COMMENT_WIDTH
                };
                self.scroll.comment_scroll_x = 0.0;
            }
        }
        self.cursor_reveal_pending = true;
        self.clamp_scroll_offsets(cx);
        cx.notify();
    }

    fn on_scroll_wheel(&mut self, event: &ScrollWheelEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let now = std::time::Instant::now();
        let pixel_delta = event.delta.pixel_delta(px(ROW_HEIGHT));
        let mut delta_x = f32::from(pixel_delta.x);
        let delta_y = f32::from(pixel_delta.y);
        let column_only_horizontal = event.modifiers.shift;

        if delta_x == 0.0 && column_only_horizontal && delta_y != 0.0 {
            delta_x = delta_y;
        }

        if let Some(last_time) = self.scroll.last_scroll_time
            && now.duration_since(last_time).as_millis() > 120
        {
            self.scroll.scroll_lock_axis = None;
        }
        self.scroll.last_scroll_time = Some(now);

        let abs_x = delta_x.abs();
        let abs_y = delta_y.abs();

        if column_only_horizontal {
            self.scroll.scroll_lock_axis = Some(ScrollAxisLock::Horizontal);
            self.scroll.scroll_lock_top_row = self.current_scroll_top_row();
        } else if self.scroll.scroll_lock_axis.is_none() && (abs_x > 0.5 || abs_y > 0.5) {
            if abs_x > abs_y * 1.1 {
                self.scroll.scroll_lock_axis = Some(ScrollAxisLock::Horizontal);
                self.scroll.scroll_lock_top_row = self.current_scroll_top_row();
            } else if abs_y > abs_x * 1.1 {
                self.scroll.scroll_lock_axis = Some(ScrollAxisLock::Vertical);
            }
        }

        if self.scroll.scroll_lock_axis == Some(ScrollAxisLock::Vertical) {
            let total_rows = self.effective_total_rows(cx);
            let list_h = self.list_bounds.get().map(|b| f32::from(b.size.height)).unwrap_or(600.0);
            let visible_rows = (list_h / ROW_HEIGHT).floor() as usize;
            let max_top_row = total_rows.saturating_sub(visible_rows.max(1));

            if let Some(new_offset) = self.scroll.handle_wheel_vertical(delta_y, max_top_row) {
                self.cached_comment_content_width.set(None);
                cx.notify();
                cx.emit(HexViewEvent::Scrolled(new_offset));
            }
            return;
        }

        let is_horizontal = self.scroll.scroll_lock_axis == Some(ScrollAxisLock::Horizontal) || column_only_horizontal || abs_x > abs_y;

        if is_horizontal && abs_x > 0.01 {
            if self.scroll.scroll_lock_axis == Some(ScrollAxisLock::Horizontal) {
                let lock_row = self.scroll.scroll_lock_top_row;
                self.scroll_to_row(lock_row, cx);
            }

            let bounds = if let Some(b) = self.list_bounds.get().or_else(|| self.bounds.get()) {
                b
            } else {
                return;
            };

            let layout = self.current_layout(cx);
            let base_x = f32::from(bounds.left()) + 8.0;
            let relative_x = f32::from(event.position.x) - base_x;
            let target = layout
                .column_at(relative_x, self.scroll.outer_scroll_x)
                .map(HorizontalScrollTarget::Column)
                .unwrap_or(HorizontalScrollTarget::View);

            let current_offset = self.horizontal_offset(target);
            let max_offset = layout.max_offset(target);
            let new_offset = (current_offset - delta_x).clamp(0.0, max_offset);
            let consumed_delta = current_offset - new_offset;
            let residual_delta = delta_x - consumed_delta;
            let _ = self.set_horizontal_offset(target, new_offset, layout, true, cx);

            if can_chain_to_outer(target, residual_delta) {
                let current_outer = self.scroll.outer_scroll_x;
                let new_outer = (current_outer - residual_delta).clamp(0.0, layout.outer_max);
                let _ = self.set_horizontal_offset(HorizontalScrollTarget::View, new_outer, layout, true, cx);
            }
        } else if !is_horizontal && abs_y > 0.01 {
            let total_rows = self.effective_total_rows(cx);
            let list_h = self.list_bounds.get().map(|b| f32::from(b.size.height)).unwrap_or(600.0);
            let visible_rows = (list_h / ROW_HEIGHT).floor() as usize;
            let max_top_row = total_rows.saturating_sub(visible_rows.max(1));

            if let Some(new_offset) = self.scroll.handle_wheel_vertical(delta_y, max_top_row) {
                self.cached_comment_content_width.set(None);
                cx.notify();
                cx.emit(HexViewEvent::Scrolled(new_offset));
            }
        }
    }

    fn update_scrollbar_drag(&mut self, current_y: f32, cx: &mut Context<Self>) {
        let total_rows = self.effective_total_rows(cx);
        let list_h = self.list_bounds.get().map(|b| f32::from(b.size.height)).unwrap_or(600.0);
        if let Some(new_row) = self.scroll.update_scrollbar_drag(current_y, total_rows, list_h) {
            self.scroll_to_row(new_row, cx);
        }
    }

    fn on_mouse_up(&mut self, _event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.scroll.is_dragging_scrollbar {
            self.scroll.is_dragging_scrollbar = false;
            cx.notify();
        }
        if self.resizing_column.is_some() {
            self.resizing_column = None;
            cx.notify();
        }
        if self.is_selecting {
            self.is_selecting = false;
            self.mouse_selection_anchor = None;
            let collapsed_selection = {
                let ed = self.editor.read(cx);
                ed.selection().is_collapsed()
            };
            if collapsed_selection {
                self.editor.update(cx, |editor, cx| {
                    editor.clear_selection();
                    cx.notify();
                });
            }
            let (start, end, cursor_offset) = {
                let ed = self.editor.read(cx);
                let range = ed.selection_range();
                (
                    range.as_ref().map(|range| range.start),
                    range.as_ref().map(|range| range.end.saturating_sub(1)),
                    ed.cursor.offset,
                )
            };
            if start.is_some() {
                self.clear_pending_hex_input();
            }
            cx.emit(HexViewEvent::SelectionChanged { start, end });
            cx.emit(HexViewEvent::CursorMoved(cursor_offset));
        }
    }

    pub fn font_family(mut self, font_family: impl Into<SharedString>) -> Self {
        self.font_family_prop = font_family.into();
        self.hex_cell_width = 0.0;
        self
    }

    pub fn font_size(mut self, font_size: impl Into<Pixels>) -> Self {
        self.font_size_prop = font_size.into();
        self.hex_cell_width = 0.0;
        self
    }

    pub fn set_font_family(&mut self, font_family: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.font_family_prop = font_family.into();
        self.hex_cell_width = 0.0;
        self.hex_content_width = 0.0;
        self.cached_comment_content_width.set(None);
        self.clear_desc_content_width_cache();
        self.cursor_reveal_pending = true;
        cx.notify();
    }

    pub fn set_font_size(&mut self, font_size: impl Into<Pixels>, cx: &mut Context<Self>) {
        self.font_size_prop = font_size.into();
        self.hex_cell_width = 0.0;
        self.hex_content_width = 0.0;
        self.cached_comment_content_width.set(None);
        self.clear_desc_content_width_cache();
        self.cursor_reveal_pending = true;
        cx.notify();
    }

    pub fn with_offset(mut self, show: bool) -> Self {
        self.show_offset = show;
        self
    }

    pub fn with_header(mut self, show: bool) -> Self {
        self.show_header = show;
        self
    }

    pub fn with_ascii(mut self, show: bool) -> Self {
        self.show_ascii = show;
        self
    }

    pub fn set_show_offset(&mut self, show: bool, cx: &mut Context<Self>) {
        self.show_offset = show;
        self.cursor_reveal_pending = true;
        cx.notify();
    }

    pub fn set_show_header(&mut self, show: bool, cx: &mut Context<Self>) {
        self.show_header = show;
        cx.notify();
    }

    pub fn set_show_ascii(&mut self, show: bool, cx: &mut Context<Self>) {
        self.show_ascii = show;
        self.cursor_reveal_pending = true;
        cx.notify();
    }

    pub fn set_encoding(&mut self, encoding: Encoding, cx: &mut Context<Self>) {
        self.encoding = encoding;
        cx.notify();
    }

    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    pub fn set_highlights(&mut self, mut highlights: Vec<(Range<usize>, Hsla)>, cx: &mut Context<Self>) {
        highlights.sort_by_key(|(range, _)| range.start);
        self.max_highlight_len = highlights.iter().map(|(r, _)| r.end.saturating_sub(r.start)).max().unwrap_or(0);
        self.highlights = Arc::new(highlights);
        cx.notify();
    }

    pub fn set_highlights_arc(&mut self, highlights: Arc<Vec<(Range<usize>, Hsla)>>, cx: &mut Context<Self>) {
        self.max_highlight_len = highlights.iter().map(|(r, _)| r.end.saturating_sub(r.start)).max().unwrap_or(0);
        self.highlights = highlights;
        cx.notify();
    }

    pub fn set_highlight_ranges(&mut self, ranges: Vec<Range<usize>>, cx: &mut Context<Self>) {
        let is_dark = cx.theme().mode.is_dark();
        let highlight_color = if is_dark { hsla(0.0, 0.75, 0.55, 0.35) } else { hsla(0.0, 0.75, 0.50, 0.35) };
        let highlights: Vec<_> = ranges.into_iter().map(|range| (range, highlight_color)).collect();
        self.set_highlights(highlights, cx);
    }

    pub fn scroll_to_byte(&mut self, byte_offset: usize, cx: &mut Context<Self>) {
        let line_starts = self.editor.read(cx).line_starts();
        let row = Editor::find_line_index(byte_offset, &line_starts);
        self.scroll_to_row(row, cx);
    }

    pub fn scroll_to_range_if_needed(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let line_starts = self.editor.read(cx).line_starts();
        if line_starts.is_empty() {
            return;
        }
        let start = range.start.min(range.end);
        let end = range.start.max(range.end);
        let start_row = Editor::find_line_index(start, &line_starts);
        let end_byte_last = if end > start { end.saturating_sub(1) } else { start };
        let end_row = Editor::find_line_index(end_byte_last, &line_starts);

        let list_h = self.list_bounds.get().map(|b| f32::from(b.size.height)).unwrap_or(600.0);
        let visible_rows = (list_h / ROW_HEIGHT).floor() as usize;
        let total_rows = line_starts.len().max(1);

        if let Some(target_top) = calculate_scroll_top_for_range(self.scroll.scroll_offset, visible_rows, total_rows, start_row, end_row) {
            self.scroll_to_row(target_top, cx);
        }
    }

    pub fn scroll_to_byte_if_needed(&mut self, byte_offset: usize, cx: &mut Context<Self>) {
        self.scroll_to_range_if_needed(byte_offset..byte_offset, cx);
    }

    pub fn current_scroll_top_row(&self) -> usize {
        self.scroll.scroll_offset
    }

    pub fn viewport_byte_range(&self, cx: &App) -> (usize, usize) {
        let editor = self.editor.read(cx);
        let line_starts = editor.line_starts();
        let current_top = self.scroll.scroll_offset;
        let start_byte = line_starts.get(current_top).unwrap_or(0);
        let visible_rows = if let Some(bounds) = self.list_bounds.get() {
            (f32::from(bounds.size.height) / ROW_HEIGHT).ceil() as usize
        } else {
            30
        };
        let end_row = (current_top + visible_rows + 2).min(line_starts.len());
        let end_byte = if end_row < line_starts.len() {
            line_starts.get(end_row).unwrap_or_else(|| editor.total_size())
        } else {
            editor.total_size()
        };
        (start_byte, end_byte)
    }

    fn effective_total_rows(&self, cx: &App) -> usize {
        let editor = self.editor.read(cx);
        let line_starts = editor.line_starts();
        let total_size = editor.total_size();
        let cursor_offset = if InsertModeState::is_enabled(cx) {
            editor.insert_cursor_offset()
        } else {
            editor.cursor.offset
        };
        let bytes_per_row = line_starts.max_bytes_per_row();
        let last_line_idx = line_starts.len().saturating_sub(1);
        let last_line_start = line_starts.get(last_line_idx).unwrap_or(0);
        let is_eof_on_new_line = total_size > last_line_start && (total_size - last_line_start) >= bytes_per_row;
        if is_eof_on_new_line && cursor_offset == total_size {
            line_starts.len() + 1
        } else {
            line_starts.len().max(1)
        }
    }

    pub fn scroll_to_row(&mut self, row: usize, cx: &mut Context<Self>) {
        let total_rows = self.effective_total_rows(cx);
        let list_h = self.list_bounds.get().map(|b| f32::from(b.size.height)).unwrap_or(600.0);
        let visible_rows = (list_h / ROW_HEIGHT).floor() as usize;

        if self.scroll.scroll_to_row(row, total_rows, visible_rows) {
            self.cached_comment_content_width.set(None);
            cx.notify();
            cx.emit(HexViewEvent::Scrolled(self.scroll.scroll_offset));
        }
    }

    pub fn scroll_to_bottom_row(&mut self, row: usize, cx: &mut Context<Self>) {
        let list_h = self.list_bounds.get().map(|b| f32::from(b.size.height)).unwrap_or(600.0);
        let visible_rows = (list_h / ROW_HEIGHT).floor() as usize;
        let target_top = row.saturating_sub(visible_rows.saturating_sub(1));
        self.scroll_to_row(target_top, cx);
    }

    fn ensure_cursor_visible(&mut self, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        let editor = self.editor.read(cx);
        let cursor_offset = if insert_mode { editor.insert_cursor_offset() } else { editor.cursor.offset };
        let line_starts = editor.line_starts();
        let total_size = editor.total_size();
        let bytes_per_row = line_starts.max_bytes_per_row();
        let last_line_idx = line_starts.len().saturating_sub(1);
        let last_line_start = line_starts.get(last_line_idx).unwrap_or(0);
        let is_eof_on_new_line = total_size > last_line_start && (total_size - last_line_start) >= bytes_per_row;
        let cursor_row = if is_eof_on_new_line && cursor_offset == total_size {
            line_starts.len()
        } else {
            Editor::find_line_index(cursor_offset, &line_starts)
        };

        let list_h = self.list_bounds.get().map(|b| f32::from(b.size.height)).unwrap_or(600.0);
        let visible_rows = (list_h / ROW_HEIGHT).floor() as usize;
        let top_row = self.scroll.scroll_offset;
        let bottom_row = top_row + visible_rows.saturating_sub(1);

        if cursor_row < top_row {
            self.scroll_to_row(cursor_row, cx);
        } else if cursor_row > bottom_row {
            self.scroll_to_bottom_row(cursor_row, cx);
        }
    }

    fn clear_pending_hex_input(&mut self) {
        self.input.clear_pending();
    }

    fn apply_hex_commit(&mut self, commit: HexCommit, advance_cursor: bool, cx: &mut Context<Self>) -> bool {
        let insert_mode = InsertModeState::is_enabled(cx);
        let (group_size, is_big_endian) = {
            let ed = self.editor.read(cx);
            (ed.options.group_size, ed.options.is_big_endian)
        };
        let changed = self.editor.update(cx, |ed, editor_cx| {
            let total = ed.total_size();
            let changed = if let Some(range) = commit.replacement_range {
                let cursor_after = if advance_cursor { range.start.saturating_add(1) } else { commit.offset };
                ed.replace_range_with_cursor(range, vec![commit.value], cursor_after)
            } else if insert_mode {
                let pos = commit.offset.min(total);
                let cursor_after = if advance_cursor { pos.saturating_add(1) } else { pos };
                ed.replace_range_with_cursor(pos..pos, vec![commit.value], cursor_after)
            } else if commit.offset >= total {
                let cursor_after = if advance_cursor { total.saturating_add(1) } else { total };
                ed.replace_range_with_cursor(total..total, vec![commit.value], cursor_after)
            } else {
                let next_cursor = if advance_cursor {
                    crate::core::radix::next_visual_byte(commit.offset, total, group_size, is_big_endian)
                } else {
                    commit.offset
                };
                ed.replace_range_with_cursor(commit.offset..commit.offset + 1, vec![commit.value], next_cursor)
            };
            editor_cx.notify();
            changed
        });
        self.edit_changed(changed, cx);
        changed
    }

    fn commit_pending_hex_with_zero(&mut self, cx: &mut Context<Self>) {
        if !self.input.has_pending() {
            return;
        }
        let is_read_only = self.editor.read(cx).is_read_only();
        if let Some(commit) = self.input.commit_pending_with_zero(self.radix, is_read_only) {
            self.apply_hex_commit(commit, false, cx);
        }
    }

    fn edit_column_is_hex(&self) -> bool {
        self.input.is_hex()
    }

    fn edit_column_is_ascii(&self) -> bool {
        self.input.is_ascii()
    }

    fn edit_changed(&mut self, changed: bool, cx: &mut Context<Self>) {
        self.pause_cursor_blink(cx);
        if changed {
            self.notify_document_changed(cx);
        }
        cx.notify();
    }

    fn pause_cursor_blink(&mut self, cx: &mut Context<Self>) {
        self.cursor_blink.update(cx, |cursor, cx| {
            cursor.pause(cx);
        });
    }

    fn handle_hex_digit(&mut self, digit: u8, window: &mut Window, cx: &mut Context<Self>) {
        if self.radix != DisplayRadix::Hexadecimal || self.editor.read(cx).is_read_only() {
            return;
        }

        cx.focus_self(window);
        let (cursor_offset, selected_range, is_read_only) = {
            let ed = self.editor.read(cx);
            let sel = ed.has_selection().then(|| ed.edit_range()).flatten();
            (ed.cursor.offset, sel, ed.is_read_only())
        };

        if let Some(ref range) = selected_range {
            self.editor.update(cx, |ed, cx| {
                ed.set_cursor_offset_exact(range.start);
                cx.notify();
            });
        }

        match self.input.handle_hex_digit(digit, cursor_offset, selected_range, is_read_only, self.radix) {
            HexInputResult::Pending(_) => {
                cx.notify();
            }
            HexInputResult::Commit(commit) => {
                self.apply_hex_commit(commit, true, cx);
            }
            HexInputResult::Ignored => {}
        }
    }

    fn handle_ascii_character(&mut self, character: char, window: &mut Window, cx: &mut Context<Self>) {
        if character.is_control() || self.editor.read(cx).is_read_only() {
            return;
        }

        cx.focus_self(window);
        let is_read_only = self.editor.read(cx).is_read_only();
        let Some(replacement) = self.input.encode_ascii_char(character, self.encoding, is_read_only) else {
            return;
        };

        let insert_mode = InsertModeState::is_enabled(cx);
        let changed = self.editor.update(cx, |ed, editor_cx| {
            let has_selection = ed.has_selection();
            let changed = if insert_mode && !has_selection {
                let position = ed.cursor.offset;
                ed.insert_bytes(position, replacement)
            } else if has_selection {
                let range = ed.edit_range().expect("selection has an edit range");
                if insert_mode {
                    let cursor_after = range.start.saturating_add(replacement.len());
                    ed.replace_range_with_cursor(range, replacement, cursor_after)
                } else {
                    ed.replace_range(range, replacement)
                }
            } else {
                let position = ed.cursor.offset;
                let total = ed.total_size();
                if position >= total {
                    let cursor_after = total.saturating_add(replacement.len());
                    ed.replace_range_with_cursor(total..total, replacement, cursor_after)
                } else {
                    let range = position..position.saturating_add(replacement.len()).min(total);
                    ed.replace_range(range, replacement)
                }
            };
            editor_cx.notify();
            changed
        });
        self.edit_changed(changed, cx);
    }

    fn delete_backward_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        if self.edit_column_is_hex() && self.input.cancel_pending() {
            cx.notify();
            return;
        }
        self.input.clear_pending();
        let changed = self.editor.update(cx, |ed, editor_cx| {
            let changed = ed.delete_backward();
            if changed {
                editor_cx.notify();
            }
            changed
        });
        self.edit_changed(changed, cx);
    }

    fn delete_forward_key(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.input.clear_pending();
        let changed = self.editor.update(cx, |ed, editor_cx| {
            let changed = ed.delete_forward();
            if changed {
                editor_cx.notify();
            }
            changed
        });
        self.edit_changed(changed, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.pause_cursor_blink(cx);
        let modifiers = event.keystroke.modifiers;
        if modifiers.control || modifiers.platform || modifiers.alt || modifiers.function {
            return;
        }

        match event.keystroke.key.as_str() {
            "backspace" => {
                self.delete_backward_key(window, cx);
                return;
            }
            "delete" => {
                self.delete_forward_key(window, cx);
                return;
            }
            "escape" => {
                self.clear_pending_hex_input();
                cx.notify();
                return;
            }
            _ => {}
        }

        let Some(key_char) = event.keystroke.key_char.as_deref() else {
            return;
        };
        let mut characters = key_char.chars();
        let Some(character) = characters.next() else {
            return;
        };
        if characters.next().is_some() {
            return;
        }

        if self.edit_column_is_hex()
            && let Some(digit) = hex_digit(character)
        {
            self.handle_hex_digit(digit, window, cx);
        } else if self.edit_column_is_ascii() {
            self.handle_ascii_character(character, window, cx);
        }
    }

    pub fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        if self.editor.read(cx).is_read_only() {
            return;
        }
        self.clear_pending_hex_input();
        let changed = self.editor.update(cx, |editor, editor_cx| {
            let changed = editor.undo();
            if changed {
                editor_cx.notify();
            }
            changed
        });
        self.edit_changed(changed, cx);
    }

    pub fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        if self.editor.read(cx).is_read_only() {
            return;
        }
        self.clear_pending_hex_input();
        let changed = self.editor.update(cx, |editor, editor_cx| {
            let changed = editor.redo();
            if changed {
                editor_cx.notify();
            }
            changed
        });
        self.edit_changed(changed, cx);
    }

    pub fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_pending_hex_input();
        let changed = ClipboardHandler::cut(&self.editor, &self.focus_handle, window, cx);
        self.edit_changed(changed, cx);
    }

    pub fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        self.clear_pending_hex_input();
        let changed = ClipboardHandler::paste(&self.editor, &self.focus_handle, window, cx);
        self.edit_changed(changed, cx);
    }

    fn can_handle_vi_action(&self, cx: &App) -> bool {
        self.edit_column_is_hex() || self.editor.read(cx).is_read_only()
    }

    fn vi_move_left(&mut self, _: &ViMoveLeft, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            self.handle_move_left(window, cx);
        } else {
            cx.propagate();
        }
    }

    fn vi_move_right(&mut self, _: &ViMoveRight, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            self.handle_move_right(window, cx);
        } else {
            cx.propagate();
        }
    }

    fn vi_move_up(&mut self, _: &ViMoveUp, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            self.exec_move(window, cx, |editor| editor.move_up());
        } else {
            cx.propagate();
        }
    }

    fn vi_move_down(&mut self, _: &ViMoveDown, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_move(window, cx, move |editor| {
                if insert_mode {
                    editor.move_down_for_insert();
                } else {
                    editor.move_down();
                }
            });
        } else {
            cx.propagate();
        }
    }

    fn vi_select_left(&mut self, _: &ViSelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            self.handle_select_left(window, cx);
        } else {
            cx.propagate();
        }
    }

    fn vi_select_right(&mut self, _: &ViSelectRight, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            self.handle_select_right(window, cx);
        } else {
            cx.propagate();
        }
    }

    fn vi_select_up(&mut self, _: &ViSelectUp, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_select(window, cx, move |editor| {
                if insert_mode {
                    editor.select_up_for_insert();
                } else {
                    editor.select_up();
                }
            });
        } else {
            cx.propagate();
        }
    }

    fn vi_select_down(&mut self, _: &ViSelectDown, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_select(window, cx, move |editor| {
                if insert_mode {
                    editor.select_down_for_insert();
                } else {
                    editor.select_down();
                }
            });
        } else {
            cx.propagate();
        }
    }

    fn exec_move(&mut self, window: &mut Window, cx: &mut Context<Self>, f: impl FnOnce(&mut Editor)) {
        cx.focus_self(window);
        self.pause_cursor_blink(cx);
        self.commit_pending_hex_with_zero(cx);
        self.editor.update(cx, |editor, cx| {
            f(editor);
            cx.notify();
        });
        let cursor_offset = self.editor.read(cx).cursor.offset;
        cx.emit(HexViewEvent::CursorMoved(cursor_offset));
    }

    fn exec_select(&mut self, window: &mut Window, cx: &mut Context<Self>, f: impl FnOnce(&mut Editor)) {
        cx.focus_self(window);
        self.pause_cursor_blink(cx);
        self.commit_pending_hex_with_zero(cx);
        self.editor.update(cx, |editor, cx| {
            f(editor);
            cx.notify();
        });
        let (start, end) = {
            let editor = self.editor.read(cx);
            let range = editor.selection_range();
            (range.as_ref().map(|range| range.start), range.as_ref().map(|range| range.end.saturating_sub(1)))
        };
        cx.emit(HexViewEvent::SelectionChanged { start, end });
    }

    fn handle_move_left(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.is_ascii() {
            let encoding = self.encoding;
            let insert_mode = InsertModeState::is_enabled(cx);
            let action = {
                let editor = self.editor.read(cx);
                if let Ok(doc) = editor.document.read() {
                    let buf = doc.buffer.data();
                    let current = editor.cursor.offset;
                    let target = encoding.prev_char_boundary(buf, current);
                    let char_range = encoding.char_range_at(buf, target);
                    Some((target, char_range))
                } else {
                    None
                }
            };
            self.exec_move(window, cx, |editor| {
                if let Some((target, char_range)) = action {
                    if insert_mode {
                        editor.set_cursor_offset_exact(target);
                    } else if char_range.end > char_range.start {
                        editor.set_selection_range(char_range);
                    } else {
                        editor.set_cursor_offset_exact(target);
                    }
                } else if insert_mode {
                    editor.move_left_for_insert();
                } else {
                    editor.move_left();
                }
            });
        } else {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_move(window, cx, move |editor| {
                if insert_mode {
                    editor.move_left_for_insert();
                } else {
                    let cur = editor.cursor.offset;
                    let prev = crate::core::radix::prev_visual_byte(cur, editor.total_size(), editor.options.group_size, editor.options.is_big_endian);
                    editor.set_cursor_offset_exact(prev);
                }
            });
        }
    }

    fn handle_move_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.is_ascii() {
            let encoding = self.encoding;
            let insert_mode = InsertModeState::is_enabled(cx);
            let action = {
                let editor = self.editor.read(cx);
                if let Ok(doc) = editor.document.read() {
                    let buf = doc.buffer.data();
                    let current = editor.cursor.offset;
                    let target = encoding.next_char_boundary(buf, current);
                    let char_range = encoding.char_range_at(buf, target);
                    let buf_len = buf.len();
                    Some((target, char_range, buf_len))
                } else {
                    None
                }
            };
            self.exec_move(window, cx, |editor| {
                if let Some((target, char_range, buf_len)) = action {
                    if target < buf_len {
                        if insert_mode {
                            editor.set_cursor_offset_exact(target);
                        } else if char_range.end > char_range.start {
                            editor.set_selection_range(char_range);
                        } else {
                            editor.set_cursor_offset_exact(target);
                        }
                    } else {
                        editor.set_cursor_offset_exact(buf_len);
                    }
                } else if insert_mode {
                    editor.move_right_for_insert();
                } else {
                    editor.move_right();
                }
            });
        } else {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_move(window, cx, move |editor| {
                if insert_mode {
                    editor.move_right_for_insert();
                } else {
                    let cur = editor.cursor.offset;
                    let next = crate::core::radix::next_visual_byte(cur, editor.total_size(), editor.options.group_size, editor.options.is_big_endian);
                    editor.set_cursor_offset_exact(next);
                }
            });
        }
    }

    fn handle_select_left(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.is_ascii() {
            let encoding = self.encoding;
            let insert_mode = InsertModeState::is_enabled(cx);
            let drag_params = {
                let editor = self.editor.read(cx);
                if let Ok(doc) = editor.document.read() {
                    let buf = doc.buffer.data();
                    let anchor = if editor.has_selection() {
                        editor.selection().anchor()
                    } else {
                        editor.cursor.offset
                    };
                    let active = if editor.has_selection() {
                        editor.selection().active()
                    } else {
                        editor.cursor.offset
                    };
                    let target = encoding.prev_char_boundary(buf, active);
                    Some((anchor, target))
                } else {
                    None
                }
            };
            self.exec_select(window, cx, move |editor| {
                if let Some((anchor, target)) = drag_params {
                    editor.continue_drag(anchor, target);
                } else if insert_mode {
                    editor.select_left_for_insert();
                } else {
                    editor.select_left();
                }
            });
        } else {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_select(window, cx, move |editor| {
                if insert_mode {
                    editor.select_left_for_insert();
                } else {
                    editor.select_left();
                }
            });
        }
    }

    fn handle_select_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.input.is_ascii() {
            let encoding = self.encoding;
            let insert_mode = InsertModeState::is_enabled(cx);
            let drag_params = {
                let editor = self.editor.read(cx);
                if let Ok(doc) = editor.document.read() {
                    let buf = doc.buffer.data();
                    let anchor = if editor.has_selection() {
                        editor.selection().anchor()
                    } else {
                        editor.cursor.offset
                    };
                    let active = if editor.has_selection() {
                        editor.selection().active()
                    } else {
                        editor.cursor.offset
                    };
                    let target = encoding.next_char_boundary(buf, active);
                    Some((anchor, target))
                } else {
                    None
                }
            };
            self.exec_select(window, cx, move |editor| {
                if let Some((anchor, target)) = drag_params {
                    editor.continue_drag(anchor, target);
                } else if insert_mode {
                    editor.select_right_for_insert();
                } else {
                    editor.select_right();
                }
            });
        } else {
            let insert_mode = InsertModeState::is_enabled(cx);
            self.exec_select(window, cx, move |editor| {
                if insert_mode {
                    editor.select_right_for_insert();
                } else {
                    editor.select_right();
                }
            });
        }
    }

    fn move_left(&mut self, _: &MoveLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_move_left(window, cx);
    }

    fn move_right(&mut self, _: &MoveRight, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_move_right(window, cx);
    }

    fn move_up(&mut self, _: &MoveUp, window: &mut Window, cx: &mut Context<Self>) {
        self.exec_move(window, cx, |e| e.move_up());
    }

    fn move_down(&mut self, _: &MoveDown, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_move(window, cx, move |editor| {
            if insert_mode {
                editor.move_down_for_insert();
            } else {
                editor.move_down();
            }
        });
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_select_left(window, cx);
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.handle_select_right(window, cx);
    }

    fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |editor| {
            if insert_mode {
                editor.select_up_for_insert();
            } else {
                editor.select_up();
            }
        });
    }

    fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |editor| {
            if insert_mode {
                editor.select_down_for_insert();
            } else {
                editor.select_down();
            }
        });
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.exec_select(window, cx, |e| e.select_all());
    }

    fn page_up(&mut self, _: &PageUp, window: &mut Window, cx: &mut Context<Self>) {
        let visible_rows = if let Some(bounds) = self.list_bounds.get() {
            (f32::from(bounds.size.height) / ROW_HEIGHT).floor() as usize
        } else {
            30
        };
        let count = visible_rows.saturating_sub(2).max(1);
        self.exec_move(window, cx, |e| {
            for _ in 0..count {
                e.move_up();
            }
        });
    }

    fn page_down(&mut self, _: &PageDown, window: &mut Window, cx: &mut Context<Self>) {
        let visible_rows = if let Some(bounds) = self.list_bounds.get() {
            (f32::from(bounds.size.height) / ROW_HEIGHT).floor() as usize
        } else {
            30
        };
        let count = visible_rows.saturating_sub(2).max(1);
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_move(window, cx, |e| {
            for _ in 0..count {
                if insert_mode {
                    e.move_down_for_insert();
                } else {
                    e.move_down();
                }
            }
        });
    }

    fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        self.exec_move(window, cx, |e| e.set_cursor_offset(0));
    }

    fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_move(window, cx, |e| {
            let total = e.total_size();
            if insert_mode {
                e.set_cursor_offset_exact(total);
            } else {
                e.set_cursor_offset(total.saturating_sub(1));
            }
        });
    }

    fn select_page_up(&mut self, _: &SelectPageUp, window: &mut Window, cx: &mut Context<Self>) {
        let visible_rows = if let Some(bounds) = self.list_bounds.get() {
            (f32::from(bounds.size.height) / ROW_HEIGHT).floor() as usize
        } else {
            30
        };
        let count = visible_rows.saturating_sub(2).max(1);
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |editor| {
            for _ in 0..count {
                if insert_mode {
                    editor.select_up_for_insert();
                } else {
                    editor.select_up();
                }
            }
        });
    }

    fn select_page_down(&mut self, _: &SelectPageDown, window: &mut Window, cx: &mut Context<Self>) {
        let visible_rows = if let Some(bounds) = self.list_bounds.get() {
            (f32::from(bounds.size.height) / ROW_HEIGHT).floor() as usize
        } else {
            30
        };
        let count = visible_rows.saturating_sub(2).max(1);
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |editor| {
            for _ in 0..count {
                if insert_mode {
                    editor.select_down_for_insert();
                } else {
                    editor.select_down();
                }
            }
        });
    }

    fn select_home(&mut self, _: &SelectHome, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |editor| {
            if insert_mode {
                editor.select_home_for_insert();
            } else {
                editor.select_home();
            }
        });
    }

    fn select_end(&mut self, _: &SelectEnd, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.exec_select(window, cx, move |editor| {
            if insert_mode {
                editor.select_end_for_insert();
            } else {
                editor.select_end();
            }
        });
    }

    fn trigger_search(&mut self, _: &TriggerSearch, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            window.dispatch_action(Box::new(ToggleSearch), cx);
        } else {
            cx.propagate();
        }
    }

    fn trigger_search_next(&mut self, _: &TriggerSearchNext, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            window.dispatch_action(Box::new(SearchNext), cx);
        } else {
            cx.propagate();
        }
    }

    fn trigger_search_prev(&mut self, _: &TriggerSearchPrev, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_handle_vi_action(cx) {
            window.dispatch_action(Box::new(SearchPrev), cx);
        } else {
            cx.propagate();
        }
    }

    fn notify_document_changed(&self, cx: &mut App) {
        let path = self.editor.read(cx).document.read().ok().map(|d| d.path().to_path_buf());
        if let Some(path) = path {
            let service = crate::app_state::AppState::global(cx).document_service.clone();
            service.notify_document_changed(&path, cx);
        }
    }

    pub fn add_custom_break(&mut self, _: &AddCustomBreak, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.editor.update(cx, |editor, cx| {
            let offset = editor.cursor.offset;
            if offset > 0 {
                editor.custom_layout_mut().add_break(offset);
            }
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn remove_custom_break_backward(&mut self, _: &RemoveCustomBreakBackward, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.editor.update(cx, |editor, cx| {
            let offset = editor.cursor.offset;
            if offset > 0 && editor.custom_layout().has_break(offset - 1) {
                editor.custom_layout_mut().remove_break(offset - 1);
            }
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn remove_custom_break_forward(&mut self, _: &RemoveCustomBreakForward, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.editor.update(cx, |editor, cx| {
            let offset = editor.cursor.offset;
            if editor.custom_layout().has_break(offset) {
                editor.custom_layout_mut().remove_break(offset);
            }
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn join_line(&mut self, _: &JoinLine, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            editor.custom_layout_mut().join_line();
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn clear_all_custom_breaks(&mut self, _: &ClearAllCustomBreaks, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.editor.update(cx, |editor, cx| {
            editor.custom_layout_mut().clear_all();
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn show_all_bookmarks(&mut self, _: &ShowAllBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            editor.bookmarks_mut().show_all();
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn hide_all_bookmarks(&mut self, _: &HideAllBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            editor.bookmarks_mut().hide_all();
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn toggle_bookmark_color(&mut self, color: crate::core::bookmark::BookmarkColor, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            editor.bookmarks_mut().toggle_color(color);
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn show_only_bookmark_color(&mut self, color: crate::core::bookmark::BookmarkColor, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            editor.bookmarks_mut().show_only_color(color);
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn toggle_hide_unbookmarked(&mut self, _: &ToggleHideUnbookmarked, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            editor.bookmarks_mut().toggle_hide_unbookmarked();
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    pub fn unfold_bookmark_at_cursor(&mut self, _: &UnfoldBookmarkAtCursor, window: &mut Window, cx: &mut Context<Self>) {
        cx.focus_self(window);
        self.cursor_reveal_pending = true;
        self.editor.update(cx, |editor, cx| {
            let offset = editor.cursor.offset;
            editor.unfold_bookmark_at(offset);
            cx.notify();
        });
        self.notify_document_changed(cx);
    }

    fn copy_formatted(&self, format: CopyFormat, window: &mut Window, cx: &mut Context<Self>) {
        ClipboardHandler::copy_formatted(&self.editor, &self.focus_handle, format, window, cx);
    }

    pub fn copy(&mut self, _: &Copy, window: &mut Window, cx: &mut Context<Self>) {
        ClipboardHandler::copy(&self.editor, &self.focus_handle, window, cx);
    }

    pub fn copy_as_hexdump(&mut self, _: &CopyAsHexDump, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::HexDump, window, cx);
    }

    pub fn copy_as_cpp_array(&mut self, _: &CopyAsCppArray, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::CppArray, window, cx);
    }

    pub fn copy_as_hex_stream(&mut self, _: &CopyAsHexStream, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::HexStream, window, cx);
    }

    pub fn copy_as_hex_spaces(&mut self, _: &CopyAsHexSpaces, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::HexWithSpaces, window, cx);
    }

    pub fn copy_as_printable_text(&mut self, _: &CopyAsPrintableText, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::PrintableText, window, cx);
    }

    pub fn copy_as_base64(&mut self, _: &CopyAsBase64, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::Base64, window, cx);
    }

    pub fn copy_as_escaped_string(&mut self, _: &CopyAsEscapedString, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::EscapedString, window, cx);
    }

    pub fn copy_as_binary(&mut self, _: &CopyAsBinary, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::Binary, window, cx);
    }

    pub fn copy_as_rust_array(&mut self, _: &CopyAsRustArray, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::RustArray, window, cx);
    }

    pub fn copy_as_json_array(&mut self, _: &CopyAsJsonArray, window: &mut Window, cx: &mut Context<Self>) {
        self.copy_formatted(CopyFormat::JsonArray, window, cx);
    }

    fn apply_bookmark(&mut self, color: Option<Hsla>, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);
        self.editor.update(cx, |editor, cx| {
            if let Some(range) = editor.selected_range_or_cursor() {
                if let Some(color) = color {
                    editor.bookmarks_mut().add_custom(range, color.into());
                } else {
                    editor.bookmarks_mut().clear_custom(range);
                }
                cx.notify();
            }
        });
        self.notify_document_changed(cx);
    }

    pub fn bookmark_red(&mut self, _: &BookmarkRed, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(hsla(0.0, 0.75, 0.55, 0.35)), window, cx);
    }

    pub fn bookmark_orange(&mut self, _: &BookmarkOrange, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(hsla(30.0 / 360.0, 0.85, 0.55, 0.35)), window, cx);
    }

    pub fn bookmark_yellow(&mut self, _: &BookmarkYellow, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(hsla(50.0 / 360.0, 0.85, 0.50, 0.35)), window, cx);
    }

    pub fn bookmark_green(&mut self, _: &BookmarkGreen, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(hsla(120.0 / 360.0, 0.65, 0.45, 0.35)), window, cx);
    }

    pub fn bookmark_cyan(&mut self, _: &BookmarkCyan, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(hsla(180.0 / 360.0, 0.70, 0.45, 0.35)), window, cx);
    }

    pub fn bookmark_blue(&mut self, _: &BookmarkBlue, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(hsla(215.0 / 360.0, 0.75, 0.55, 0.35)), window, cx);
    }

    pub fn bookmark_purple(&mut self, _: &BookmarkPurple, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(hsla(280.0 / 360.0, 0.70, 0.55, 0.35)), window, cx);
    }

    pub fn bookmark_pink(&mut self, _: &BookmarkPink, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(Some(hsla(330.0 / 360.0, 0.75, 0.55, 0.35)), window, cx);
    }

    pub fn clear_bookmark(&mut self, _: &ClearBookmark, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_bookmark(None, window, cx);
    }

    pub fn clear_all_bookmarks(&mut self, _: &ClearAllBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.editor.read(cx).bookmarks().snapshot().len();
        if count == 0 {
            return;
        }

        let prompt = window.prompt(
            PromptLevel::Warning,
            "Clear all bookmarks?",
            Some(&format!(
                "Are you sure you want to clear all {} bookmark{} and comments? This action cannot be undone.",
                count,
                if count == 1 { "" } else { "s" }
            )),
            &["Clear All", "Cancel"],
            cx,
        );

        let editor = self.editor.clone();
        let doc_path = editor.read(cx).document.read().ok().map(|d| d.path().to_path_buf());
        cx.spawn_in(window, async move |_this, window| {
            if let Ok(0) = prompt.await {
                window
                    .update(|_, cx| {
                        editor.update(cx, |editor, cx| {
                            editor.bookmarks_mut().clear_all();
                            cx.notify();
                        });
                        if let Some(ref path) = doc_path {
                            let service = crate::app_state::AppState::global(cx).document_service.clone();
                            service.notify_document_changed(path, cx);
                        }
                    })
                    .ok();
            }
        })
        .detach();
    }

    fn set_radix_hex(&mut self, _: &SetRadixHex, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_radix(DisplayRadix::Hexadecimal);
            cx.notify();
        });
    }

    fn set_radix_dec(&mut self, _: &SetRadixDec, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_radix(DisplayRadix::Decimal);
            cx.notify();
        });
    }

    fn set_radix_oct(&mut self, _: &SetRadixOct, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_radix(DisplayRadix::Octal);
            cx.notify();
        });
    }

    fn set_radix_bin(&mut self, _: &SetRadixBin, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_radix(DisplayRadix::Binary);
            cx.notify();
        });
    }

    fn set_group_size_1(&mut self, _: &SetGroupSize1, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_group_size(ByteGroupSize::One);
            cx.notify();
        });
    }

    fn set_group_size_2(&mut self, _: &SetGroupSize2, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_group_size(ByteGroupSize::Two);
            cx.notify();
        });
    }

    fn set_group_size_4(&mut self, _: &SetGroupSize4, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_group_size(ByteGroupSize::Four);
            cx.notify();
        });
    }

    fn set_group_size_8(&mut self, _: &SetGroupSize8, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_group_size(ByteGroupSize::Eight);
            cx.notify();
        });
    }

    fn set_byte_order_le(&mut self, _: &SetByteOrderLittleEndian, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_is_big_endian(false);
            cx.notify();
        });
    }

    fn set_byte_order_be(&mut self, _: &SetByteOrderBigEndian, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.set_is_big_endian(true);
            cx.notify();
        });
    }

    fn toggle_byte_order(&mut self, _: &ToggleByteOrder, _window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |ed, cx| {
            ed.toggle_byte_order();
            cx.notify();
        });
    }

    fn edit_target_from_point(&self, point: Point<Pixels>, window: &Window, cx: &App) -> Option<EditTarget> {
        let insert_mode = InsertModeState::is_enabled(cx);
        let offset = self.offset_from_point(point, window, cx)?;
        let list_bounds = self.list_bounds.get()?;
        let editor = self.editor.read(cx);
        let doc = editor.document.read().ok()?;
        let line_starts = editor.line_starts();
        let rel_y = f32::from(point.y - list_bounds.top()).max(0.0);
        let row_offset_in_view = (rel_y / ROW_HEIGHT).floor() as usize;
        let row_idx = self.scroll.scroll_offset + row_offset_in_view;
        let total_size = doc.buffer.len();
        let bytes_per_row = line_starts.max_bytes_per_row();
        let last_line_idx = line_starts.len().saturating_sub(1);
        let last_line_start = line_starts.get(last_line_idx).unwrap_or(0);
        let is_eof_on_new_line = total_size > last_line_start && (total_size - last_line_start) >= bytes_per_row;
        let is_eof_row = row_idx == line_starts.len() && is_eof_on_new_line;
        if row_idx > line_starts.len() || (row_idx == line_starts.len() && !is_eof_row) {
            return None;
        }
        let line_offset = if is_eof_row { total_size } else { line_starts.get(row_idx)? };
        let next_offset = if is_eof_row {
            total_size
        } else {
            line_starts.get(row_idx + 1).unwrap_or(doc.buffer.len())
        };
        let chunk_len = next_offset.saturating_sub(line_offset);
        if !is_eof_row && editor.is_folded(line_offset) {
            return None;
        }
        if chunk_len == 0 {
            let parse_result = editor.parse_result();
            let is_struct_mode = editor.structure.show_inline_structure_view && parse_result.is_some();
            let base_x = f32::from(list_bounds.left()) + 8.0;
            let layout = self.current_layout(cx);
            let relative_x = f32::from(point.x) - base_x;
            if relative_x >= layout.fixed_width {
                let world_x = relative_x + self.scroll.outer_scroll_x;
                if !is_struct_mode
                    && let Some(ascii_column) = layout.ascii
                    && world_x >= ascii_column.start
                    && world_x <= ascii_column.end()
                {
                    return Some(EditTarget::Ascii { offset: line_offset });
                }
                if world_x >= layout.hex.start && world_x <= layout.hex.end() {
                    return Some(EditTarget::Hex {
                        offset: line_offset,
                        nibble: 0,
                    });
                }
            }
            return None;
        }

        let parse_result = editor.parse_result();
        let is_struct_mode = editor.structure.show_inline_structure_view && parse_result.is_some();
        let base_x = f32::from(list_bounds.left()) + 8.0;
        let layout = self.current_layout(cx);
        let relative_x = f32::from(point.x) - base_x;
        if relative_x < layout.fixed_width {
            return None;
        }
        let world_x = relative_x + self.scroll.outer_scroll_x;

        if !is_struct_mode
            && let Some(ascii_column) = layout.ascii
            && world_x >= ascii_column.start
            && world_x < ascii_column.end()
        {
            let raw_idx = ascii_byte_index_from_world_x(world_x, ascii_column, self.scroll.ascii_scroll_x);
            if raw_idx < chunk_len {
                let abs_offset = line_offset + raw_idx;
                let char_range = self.encoding.char_range_at(doc.buffer.data(), abs_offset);
                return Some(EditTarget::Ascii { offset: char_range.start });
            }
            let is_last_row = row_idx + 1 == line_starts.len();
            if is_last_row && raw_idx >= chunk_len {
                return Some(EditTarget::Ascii {
                    offset: line_offset + chunk_len,
                });
            }
            if !insert_mode && chunk_len > 0 && raw_idx >= chunk_len {
                let abs_offset = line_offset + chunk_len - 1;
                let char_range = self.encoding.char_range_at(doc.buffer.data(), abs_offset);
                return Some(EditTarget::Ascii { offset: char_range.start });
            }
            return None;
        }

        if world_x < layout.hex.start || world_x > layout.hex.end() {
            return None;
        }

        let source = build_hex_text_source(
            doc.buffer.get_range(line_offset, chunk_len),
            line_offset,
            self.radix,
            self.group_size,
            self.is_big_endian,
        );
        let cell_width = px(self.hex_cell_width.max(1.0));
        let origin_x = px(base_x + layout.hex.start - self.scroll.outer_scroll_x - self.scroll.hex_scroll_x);
        let mut selected_group = None;
        for (index, group) in source.groups.iter().enumerate() {
            let (group_start, group_end) = hex_group_x(*group, origin_x, cell_width);
            let next_start = source
                .groups
                .get(index + 1)
                .map(|next| origin_x + hex_grid_x(next.text_start, cell_width))
                .unwrap_or(group_end);
            if point.x >= group_start && point.x < next_start {
                selected_group = Some(*group);
                break;
            }
        }
        if selected_group.is_none()
            && let Some(last_group) = source.groups.last().copied()
        {
            let is_last_row = row_idx + 1 == line_starts.len();
            let (_, last_group_end) = hex_group_x(last_group, origin_x, cell_width);
            if is_last_row && point.x >= last_group_end && world_x <= layout.hex.end() {
                return Some(EditTarget::Hex {
                    offset: line_offset + chunk_len,
                    nibble: 0,
                });
            }
        }
        let group = selected_group.or_else(|| {
            source
                .groups
                .iter()
                .find(|group| group.chunk_start <= offset.saturating_sub(line_offset) && offset.saturating_sub(line_offset) < group.chunk_end)
                .copied()
        })?;

        let (group_start, _) = hex_group_x(group, origin_x, cell_width);
        let text_index = ((f32::from(point.x - group_start) / f32::from(cell_width)).floor() as usize)
            .min(group.text_end.saturating_sub(group.text_start).saturating_sub(1));
        let display_byte_index = if self.radix == DisplayRadix::Hexadecimal { text_index / 2 } else { 0 };
        let group_len = group.chunk_end.saturating_sub(group.chunk_start);
        let local_byte_index = if self.radix == DisplayRadix::Hexadecimal && !self.is_big_endian && group_len > 1 {
            group_len.saturating_sub(display_byte_index + 1)
        } else {
            display_byte_index.min(group_len.saturating_sub(1))
        };
        let target_offset = line_offset + group.chunk_start + local_byte_index.min(group_len.saturating_sub(1));
        let nibble = if self.radix == DisplayRadix::Hexadecimal { (text_index % 2) as u8 } else { 0 };
        Some(EditTarget::Hex { offset: target_offset, nibble })
    }

    fn offset_from_point(&self, point: Point<Pixels>, _window: &Window, cx: &App) -> Option<usize> {
        let root_bounds = self.bounds.get()?;
        let header_h = if self.show_header { HEADER_HEIGHT } else { 0.0 };

        if point.x < root_bounds.left() || point.y < root_bounds.top() + px(header_h) {
            return None;
        }

        let list_bounds = self.list_bounds.get()?;

        let editor = self.editor.read(cx);
        let doc = editor.document.read().ok()?;
        let buffer_len = doc.buffer.len();
        if buffer_len == 0 {
            return Some(0);
        }
        let line_starts = editor.line_starts();
        if line_starts.is_empty() {
            return Some(0);
        }

        let rel_y = f32::from(point.y - list_bounds.top()).max(0.0);
        let row_offset_in_view = (rel_y / ROW_HEIGHT).floor() as usize;
        let row_idx = self.scroll.scroll_offset + row_offset_in_view;
        let total_size = buffer_len;
        let bytes_per_row = line_starts.max_bytes_per_row();
        let last_line_idx = line_starts.len().saturating_sub(1);
        let last_line_start = line_starts.get(last_line_idx).unwrap_or(0);
        let is_eof_on_new_line = total_size > last_line_start && (total_size - last_line_start) >= bytes_per_row;
        let is_eof_row = row_idx == line_starts.len() && is_eof_on_new_line;
        if row_idx > line_starts.len() || (row_idx == line_starts.len() && !is_eof_row) {
            return None;
        }
        let line_offset = if is_eof_row { total_size } else { line_starts.get(row_idx)? };
        if !is_eof_row && editor.is_folded(line_offset) {
            return Some(line_offset);
        }

        let next_offset = if is_eof_row {
            total_size
        } else if row_idx + 1 < line_starts.len() {
            line_starts.get(row_idx + 1).unwrap_or(buffer_len)
        } else {
            buffer_len
        };
        let chunk_len = next_offset.saturating_sub(line_offset);
        if chunk_len == 0 {
            return Some(line_offset);
        }

        let parse_result = editor.parse_result();
        let is_struct_mode = editor.structure.show_inline_structure_view && parse_result.is_some();
        let base_x = f32::from(list_bounds.left()) + 8.0;
        let layout = self.current_layout(cx);
        let relative_x = f32::from(point.x) - base_x;
        if relative_x < layout.fixed_width {
            return Some(line_offset);
        }
        let world_x = relative_x + self.scroll.outer_scroll_x;

        if is_struct_mode && layout.description.map(|column| world_x >= column.start).unwrap_or(false) {
            if let Some(parse_res) = parse_result {
                let leaf_fields = parse_res.find_leaf_fields_starting_at(line_offset, chunk_len);
                if let Some(first) = leaf_fields.first() {
                    return Some(first.offset);
                }
            }
            return Some(line_offset);
        }

        let byte_offset_in_row = if !is_struct_mode
            && let Some(ascii_column) = layout.ascii
            && world_x >= ascii_column.start
            && world_x < ascii_column.end()
        {
            let raw_idx = ascii_byte_index_from_world_x(world_x, ascii_column, self.scroll.ascii_scroll_x);
            let is_last_row = row_idx + 1 == line_starts.len();
            if is_last_row && raw_idx >= chunk_len {
                return Some(buffer_len);
            }
            let raw_idx = raw_idx.min(chunk_len.saturating_sub(1));
            let abs_offset = line_offset + raw_idx;
            let char_range = self.encoding.char_range_at(doc.buffer.data(), abs_offset);
            char_range.start.saturating_sub(line_offset)
        } else {
            let source = build_hex_text_source(
                doc.buffer.get_range(line_offset, chunk_len),
                line_offset,
                self.radix,
                self.group_size,
                self.is_big_endian,
            );
            let cell_width = px(self.hex_cell_width.max(1.0));
            let origin_x = px(base_x + layout.hex.start - self.scroll.outer_scroll_x - self.scroll.hex_scroll_x);

            let mut target_group_idx = 0;
            for (idx, group) in source.groups.iter().enumerate() {
                let (group_start, group_end) = hex_group_x(*group, origin_x, cell_width);
                let next_start = source
                    .groups
                    .get(idx + 1)
                    .map(|next| origin_x + hex_grid_x(next.text_start, cell_width))
                    .unwrap_or(group_end);
                if point.x < next_start || idx + 1 == source.groups.len() {
                    target_group_idx = idx;
                    break;
                }
                if point.x >= group_start {
                    target_group_idx = idx;
                }
            }

            let is_last_row = row_idx + 1 == line_starts.len();
            if is_last_row && let Some(last_group) = source.groups.last().copied() {
                let (_, last_group_end) = hex_group_x(last_group, origin_x, cell_width);
                if point.x >= last_group_end && world_x <= layout.hex.end() {
                    return Some(buffer_len);
                }
            }

            let group = source.groups[target_group_idx];
            group.chunk_start
        };

        let byte_idx = byte_offset_in_row.min(chunk_len.saturating_sub(1));
        Some(line_offset + byte_idx)
    }
}

impl Focusable for HexView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for HexView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().clone();
        let is_focused = self.focus_handle.is_focused(window);
        let insert_mode = InsertModeState::is_enabled(cx);
        let cursor_visible = insert_mode && is_focused && window.is_window_active() && self.cursor_blink.read(cx).visible();
        let theme = cx.theme();
        let font_family = self.font_family_prop.clone();
        let font_size = self.font_size_prop;

        let total_rows = self.effective_total_rows(cx);
        let (max_bytes_per_row, is_struct_mode) = {
            let editor = self.editor.read(cx);
            let line_starts = editor.line_starts();
            (
                line_starts.max_bytes_per_row(),
                editor.structure.show_inline_structure_view && editor.parse_result().is_some(),
            )
        };
        let max_bytes_per_row_changed = self.last_max_bytes_per_row != Some(max_bytes_per_row);
        if max_bytes_per_row_changed {
            self.last_max_bytes_per_row = Some(max_bytes_per_row);
            self.ascii_col_width = Self::default_ascii_col_width(max_bytes_per_row);
        }
        let ascii_col_width = self.effective_ascii_col_width(max_bytes_per_row);

        let container = div()
            .flex()
            .flex_col()
            .bg(theme.background)
            .font_family(font_family.clone())
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .key_context(CONTEXT);

        let group_bytes = self.group_size.byte_count();
        let items_in_row = max_bytes_per_row.div_ceil(group_bytes).max(1);
        let hex_cell_width = if self.hex_cell_width > 0.0 {
            px(self.hex_cell_width)
        } else {
            let measured = measure_hex_cell_width(window, font(font_family.clone()), font_size);
            self.hex_cell_width = f32::from(measured);
            measured
        };
        const ZERO_BUFFER: [u8; 512] = [0u8; 512];
        let probe_len = items_in_row * group_bytes;
        let probe_source = if probe_len <= ZERO_BUFFER.len() {
            build_hex_text_source(&ZERO_BUFFER[..probe_len], 0, self.radix, self.group_size, self.is_big_endian)
        } else {
            let probe_bytes = vec![0u8; probe_len];
            build_hex_text_source(&probe_bytes, 0, self.radix, self.group_size, self.is_big_endian)
        };
        let total_data_width = f32::from(hex_grid_width(probe_source.text.len(), hex_cell_width));
        self.hex_content_width = total_data_width;
        if max_bytes_per_row_changed || self.hex_col_width <= 0.0 {
            self.hex_col_width = total_data_width;
        }
        let max_hex_scroll = (total_data_width - self.hex_col_width).max(0.0);
        self.scroll.hex_scroll_x = self.scroll.hex_scroll_x.clamp(0.0, max_hex_scroll);
        let layout = self.current_layout(cx);
        self.scroll.outer_scroll_x = self.scroll.outer_scroll_x.clamp(0.0, layout.outer_max);
        self.scroll.outer_scroll_handle.set_offset(point(-px(self.scroll.outer_scroll_x), px(0.0)));

        // Keep the cursor visible using the same fixed grid as the paint pass.
        let (cursor_offset, insert_cursor_offset) = {
            let editor = self.editor.read(cx);
            (editor.cursor.offset, editor.insert_cursor_offset())
        };
        let reveal_cursor_offset = if insert_mode { insert_cursor_offset } else { cursor_offset };
        let should_reveal_cursor = self.cursor_reveal_pending || self.last_cursor_offset != Some(reveal_cursor_offset);
        if should_reveal_cursor {
            let cursor_layout = {
                let editor = self.editor.read(cx);
                let total_size = editor.total_size();
                let line_starts = editor.line_starts();
                let row = Editor::find_line_index(reveal_cursor_offset, &line_starts);
                let line_offset = line_starts.get(row).unwrap_or(0);
                let next_offset = line_starts.get(row + 1).unwrap_or(total_size);
                if editor.is_folded(line_offset) {
                    None
                } else {
                    let chunk_len = next_offset.saturating_sub(line_offset).min(64);
                    editor.document.read().ok().map(|doc| {
                        let source = build_hex_text_source(
                            doc.buffer.get_range(line_offset, chunk_len),
                            line_offset,
                            self.radix,
                            self.group_size,
                            self.is_big_endian,
                        );
                        (reveal_cursor_offset.saturating_sub(line_offset), source, total_size, next_offset)
                    })
                }
            };
            if let Some((cursor_in_row, source, total_size, next_offset)) = cursor_layout {
                let cursor_range = source
                    .groups
                    .iter()
                    .find(|group| group.chunk_start <= cursor_in_row && cursor_in_row < group.chunk_end)
                    .map(|group| {
                        (
                            f32::from(hex_grid_x(group.text_start, hex_cell_width)),
                            f32::from(hex_grid_x(group.text_end, hex_cell_width)),
                        )
                    })
                    .or_else(|| {
                        if cursor_in_row == source.groups.last().map(|group| group.chunk_end).unwrap_or(0)
                            && reveal_cursor_offset == total_size
                            && next_offset == total_size
                        {
                            let end = source.groups.last().map(|group| hex_grid_x(group.text_end, hex_cell_width))?;
                            let end = f32::from(end);
                            Some((end, end))
                        } else {
                            None
                        }
                    });

                if let Some((cursor_left, cursor_right)) = cursor_range {
                    self.scroll
                        .reveal_cursor(cursor_left, cursor_right, self.hex_col_width, max_hex_scroll, &layout);
                }
            }
            self.last_cursor_offset = Some(reveal_cursor_offset);
            self.cursor_reveal_pending = false;
        }
        let is_hex_clipped_left = self.scroll.hex_scroll_x > 1.0;
        let is_hex_clipped_right = self.scroll.hex_scroll_x < max_hex_scroll - 1.0;
        let is_ascii_clipped_left = self.scroll.ascii_scroll_x > 1.0;
        let is_ascii_clipped_right = layout.ascii.map(|column| self.scroll.ascii_scroll_x < column.inner_max - 1.0).unwrap_or(false);
        let is_comment_clipped_left = self.scroll.comment_scroll_x > 1.0;
        let is_comment_clipped_right = self.scroll.comment_scroll_x < layout.comment.inner_max - 1.0;
        let is_desc_clipped_left = self.scroll.desc_scroll_x > 1.0;
        let is_desc_clipped_right = layout
            .description
            .map(|column| self.scroll.desc_scroll_x < column.inner_max - 1.0)
            .unwrap_or(false);

        let header = if self.show_header {
            let mut hex_cols = Vec::with_capacity(items_in_row);
            for (i, group) in probe_source.groups.iter().enumerate() {
                let byte_offset = i * group_bytes;
                let label = SharedString::from(format!("+{:X}", byte_offset));
                let group_start = f32::from(hex_grid_x(group.text_start, hex_cell_width));
                let group_end = f32::from(hex_grid_x(group.text_end, hex_cell_width));
                hex_cols.push(
                    div()
                        .absolute()
                        .left(px(group_start - self.scroll.hex_scroll_x))
                        .top_0()
                        .h_full()
                        .w(px(group_end - group_start))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_center()
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(label),
                );
            }

            let comment_header_el = |width: f32, can_scroll_left: bool, can_scroll_right: bool, theme: &gpui_kit::component::Theme| {
                h_flex()
                    .w(px(width + SECTION_GAP))
                    .child(
                        div()
                            .w(px(width))
                            .overflow_hidden()
                            .relative()
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child("Comment")
                            .when(can_scroll_left, |el| {
                                el.child(
                                    div()
                                        .absolute()
                                        .left_0()
                                        .top_0()
                                        .bottom_0()
                                        .w(px(14.0))
                                        .bg(theme.sidebar.opacity(0.85))
                                        .text_color(theme.muted_foreground)
                                        .child("…"),
                                )
                            })
                            .when(can_scroll_right, |el| {
                                el.child(
                                    div()
                                        .absolute()
                                        .right_0()
                                        .top_0()
                                        .bottom_0()
                                        .w(px(14.0))
                                        .bg(theme.sidebar.opacity(0.85))
                                        .text_color(theme.muted_foreground)
                                        .child("…"),
                                )
                            })
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                    if event.click_count >= 2 {
                                        this.auto_fit_column(ResizingColumn::Comment, cx);
                                    }
                                }),
                            ),
                    )
                    .child(
                        div()
                            .w(px(SECTION_GAP))
                            .h_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor(CursorStyle::ResizeLeftRight)
                            .hover(|s| s.bg(theme.accent.opacity(0.2)))
                            .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                    if event.click_count >= 2 {
                                        this.resizing_column = None;
                                        this.auto_fit_column(ResizingColumn::Comment, cx);
                                    } else {
                                        this.resizing_column = Some((ResizingColumn::Comment, event.position.x.into(), this.comment_col_width));
                                        cx.notify();
                                    }
                                }),
                            ),
                    )
            };

            div()
                .flex()
                .flex_row()
                .items_center()
                .h(px(HEADER_HEIGHT))
                .bg(theme.sidebar)
                .border_b_1()
                .border_color(theme.border)
                .font_family(font_family.clone())
                .px_2()
                .min_w_0()
                .overflow_hidden()
                .child(if is_struct_mode {
                    h_flex()
                        .flex_shrink_0()
                        .w(px(self.address_col_width + SECTION_GAP))
                        .child(
                            div()
                                .w(px(self.address_col_width))
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child("Address")
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                        if event.click_count >= 2 {
                                            this.auto_fit_column(ResizingColumn::Address, cx);
                                        }
                                    }),
                                ),
                        )
                        .child(
                            div()
                                .w(px(SECTION_GAP))
                                .h_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor(CursorStyle::ResizeLeftRight)
                                .hover(|s| s.bg(theme.accent.opacity(0.2)))
                                .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                        if event.click_count >= 2 {
                                            this.resizing_column = None;
                                            this.auto_fit_column(ResizingColumn::Address, cx);
                                        } else {
                                            this.resizing_column = Some((ResizingColumn::Address, event.position.x.into(), this.address_col_width));
                                            cx.notify();
                                        }
                                    }),
                                ),
                        )
                        .into_any_element()
                } else if self.show_offset {
                    h_flex()
                        .flex_shrink_0()
                        .w(px(OFFSET_WIDTH + SECTION_GAP))
                        .child(div().w(px(OFFSET_WIDTH)).text_xs().text_color(theme.muted_foreground).child("Address"))
                        .child(
                            div()
                                .w(px(SECTION_GAP))
                                .h_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border)),
                        )
                        .into_any_element()
                } else {
                    div().flex_shrink_0().w(px(SECTION_GAP)).into_any_element()
                })
                .child(
                    h_flex()
                        .relative()
                        .left(px(-self.scroll.outer_scroll_x))
                        .h(px(HEADER_HEIGHT))
                        .flex_shrink_0()
                        .w(px(self.hex_col_width + SECTION_GAP))
                        .child(
                            div()
                                .w(px(self.hex_col_width))
                                .h(px(HEADER_HEIGHT))
                                .overflow_hidden()
                                .relative()
                                .child(div().relative().w(px(self.hex_col_width)).h(px(HEADER_HEIGHT)).children(hex_cols))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                        if event.click_count >= 2 {
                                            this.auto_fit_column(ResizingColumn::Hex, cx);
                                        }
                                    }),
                                )
                                .when(is_hex_clipped_left, |el| {
                                    el.child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .left_0()
                                            .bottom_0()
                                            .w(px(18.0))
                                            .flex()
                                            .items_center()
                                            .justify_start()
                                            .pl_1()
                                            .bg(theme.sidebar.opacity(0.85))
                                            .text_xs()
                                            .font_semibold()
                                            .text_color(theme.muted_foreground)
                                            .child("…"),
                                    )
                                })
                                .when(is_hex_clipped_right, |el| {
                                    el.child(
                                        div()
                                            .absolute()
                                            .top_0()
                                            .right_0()
                                            .bottom_0()
                                            .w(px(18.0))
                                            .flex()
                                            .items_center()
                                            .justify_end()
                                            .pr_1()
                                            .bg(theme.sidebar.opacity(0.85))
                                            .text_xs()
                                            .font_semibold()
                                            .text_color(theme.muted_foreground)
                                            .child("…"),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .w(px(SECTION_GAP))
                                .h_full()
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor(CursorStyle::ResizeLeftRight)
                                .hover(|s| s.bg(theme.accent.opacity(0.2)))
                                .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                        if event.click_count >= 2 {
                                            this.resizing_column = None;
                                            this.auto_fit_column(ResizingColumn::Hex, cx);
                                        } else {
                                            this.resizing_column = Some((ResizingColumn::Hex, event.position.x.into(), this.hex_col_width));
                                            cx.notify();
                                        }
                                    }),
                                ),
                        ),
                )
                .child(
                    div()
                        .relative()
                        .left(px(-self.scroll.outer_scroll_x))
                        .flex_shrink_0()
                        .child(if is_struct_mode {
                            h_flex()
                                .child(
                                    h_flex()
                                        .w(px(self.desc_col_width + SECTION_GAP))
                                        .child(
                                            div()
                                                .w(px(self.desc_col_width))
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .overflow_hidden()
                                                .relative()
                                                .child("Description")
                                                .when(is_desc_clipped_left, |el| {
                                                    el.child(
                                                        div()
                                                            .absolute()
                                                            .left_0()
                                                            .top_0()
                                                            .bottom_0()
                                                            .w(px(14.0))
                                                            .bg(theme.sidebar.opacity(0.85))
                                                            .child("…"),
                                                    )
                                                })
                                                .when(is_desc_clipped_right, |el| {
                                                    el.child(
                                                        div()
                                                            .absolute()
                                                            .right_0()
                                                            .top_0()
                                                            .bottom_0()
                                                            .w(px(14.0))
                                                            .bg(theme.sidebar.opacity(0.85))
                                                            .child("…"),
                                                    )
                                                })
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                                        if event.click_count >= 2 {
                                                            this.auto_fit_column(ResizingColumn::Description, cx);
                                                        }
                                                    }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .w(px(SECTION_GAP))
                                                .h_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .cursor(CursorStyle::ResizeLeftRight)
                                                .hover(|s| s.bg(theme.accent.opacity(0.2)))
                                                .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                                        if event.click_count >= 2 {
                                                            this.resizing_column = None;
                                                            this.auto_fit_column(ResizingColumn::Description, cx);
                                                        } else {
                                                            this.resizing_column =
                                                                Some((ResizingColumn::Description, event.position.x.into(), this.desc_col_width));
                                                            cx.notify();
                                                        }
                                                    }),
                                                ),
                                        ),
                                )
                                .child(comment_header_el(
                                    self.comment_col_width,
                                    is_comment_clipped_left,
                                    is_comment_clipped_right,
                                    theme,
                                ))
                                .into_any_element()
                        } else if self.show_ascii {
                            let label = self.encoding.label();
                            h_flex()
                                .child(
                                    h_flex()
                                        .w(px(ascii_col_width + SECTION_GAP))
                                        .child(
                                            div()
                                                .w(px(ascii_col_width))
                                                .overflow_hidden()
                                                .relative()
                                                .text_xs()
                                                .text_color(theme.muted_foreground)
                                                .child(label)
                                                .when(is_ascii_clipped_left, |el| {
                                                    el.child(
                                                        div()
                                                            .absolute()
                                                            .left_0()
                                                            .top_0()
                                                            .bottom_0()
                                                            .w(px(18.0))
                                                            .bg(theme.sidebar.opacity(0.85))
                                                            .text_xs()
                                                            .font_semibold()
                                                            .text_color(theme.muted_foreground)
                                                            .child("…"),
                                                    )
                                                })
                                                .when(is_ascii_clipped_right, |el| {
                                                    el.child(
                                                        div()
                                                            .absolute()
                                                            .right_0()
                                                            .top_0()
                                                            .bottom_0()
                                                            .w(px(18.0))
                                                            .bg(theme.sidebar.opacity(0.85))
                                                            .text_xs()
                                                            .font_semibold()
                                                            .text_color(theme.muted_foreground)
                                                            .child("…"),
                                                    )
                                                })
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                                                        if event.click_count >= 2 {
                                                            this.auto_fit_column(ResizingColumn::Ascii, cx);
                                                        }
                                                    }),
                                                ),
                                        )
                                        .child(
                                            div()
                                                .w(px(SECTION_GAP))
                                                .h_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .cursor(CursorStyle::ResizeLeftRight)
                                                .hover(|s| s.bg(theme.accent.opacity(0.2)))
                                                .child(div().w(px(1.0)).h(px(16.0)).bg(theme.border))
                                                .on_mouse_down(
                                                    MouseButton::Left,
                                                    cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                                        if event.click_count >= 2 {
                                                            this.resizing_column = None;
                                                            this.auto_fit_column(ResizingColumn::Ascii, cx);
                                                        } else {
                                                            this.resizing_column = Some((ResizingColumn::Ascii, event.position.x.into(), ascii_col_width));
                                                            cx.notify();
                                                        }
                                                    }),
                                                ),
                                        ),
                                )
                                .child(comment_header_el(
                                    self.comment_col_width,
                                    is_comment_clipped_left,
                                    is_comment_clipped_right,
                                    theme,
                                ))
                                .into_any_element()
                        } else {
                            comment_header_el(self.comment_col_width, is_comment_clipped_left, is_comment_clipped_right, theme).into_any_element()
                        })
                        .into_any_element(),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        };

        let bounds_view = view.clone();
        let list_bounds_view = view.clone();

        container
            .track_focus(&self.focus_handle(cx))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .on_action(cx.listener(Self::move_left))
            .on_action(cx.listener(Self::move_right))
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::vi_move_left))
            .on_action(cx.listener(Self::vi_move_right))
            .on_action(cx.listener(Self::vi_move_up))
            .on_action(cx.listener(Self::vi_move_down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::vi_select_left))
            .on_action(cx.listener(Self::vi_select_right))
            .on_action(cx.listener(Self::vi_select_up))
            .on_action(cx.listener(Self::vi_select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(|this, _: &AppSelectAll, window, cx| {
                this.select_all(&SelectAll, window, cx);
            }))
            .on_action(cx.listener(Self::set_radix_hex))
            .on_action(cx.listener(Self::set_radix_dec))
            .on_action(cx.listener(Self::set_radix_oct))
            .on_action(cx.listener(Self::set_radix_bin))
            .on_action(cx.listener(Self::set_group_size_1))
            .on_action(cx.listener(Self::set_group_size_2))
            .on_action(cx.listener(Self::set_group_size_4))
            .on_action(cx.listener(Self::set_group_size_8))
            .on_action(cx.listener(Self::set_byte_order_le))
            .on_action(cx.listener(Self::set_byte_order_be))
            .on_action(cx.listener(Self::toggle_byte_order))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::copy_as_hexdump))
            .on_action(cx.listener(Self::copy_as_cpp_array))
            .on_action(cx.listener(Self::copy_as_hex_stream))
            .on_action(cx.listener(Self::copy_as_hex_spaces))
            .on_action(cx.listener(Self::copy_as_printable_text))
            .on_action(cx.listener(Self::copy_as_base64))
            .on_action(cx.listener(Self::copy_as_escaped_string))
            .on_action(cx.listener(Self::copy_as_binary))
            .on_action(cx.listener(Self::copy_as_rust_array))
            .on_action(cx.listener(Self::copy_as_json_array))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_page_up))
            .on_action(cx.listener(Self::select_page_down))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::trigger_search))
            .on_action(cx.listener(Self::trigger_search_next))
            .on_action(cx.listener(Self::trigger_search_prev))
            .on_action(cx.listener(Self::add_custom_break))
            .on_action(cx.listener(Self::remove_custom_break_backward))
            .on_action(cx.listener(Self::remove_custom_break_forward))
            .on_action(cx.listener(Self::join_line))
            .on_action(cx.listener(Self::clear_all_custom_breaks))
            .on_action(cx.listener(Self::show_all_bookmarks))
            .on_action(cx.listener(Self::hide_all_bookmarks))
            .on_action(cx.listener(Self::toggle_hide_unbookmarked))
            .on_action(cx.listener(Self::unfold_bookmark_at_cursor))
            .on_action(cx.listener(|this, _: &ToggleBookmarkRed, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Red, window, cx)))
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkOrange, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Orange, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkYellow, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Yellow, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkGreen, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Green, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkCyan, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Cyan, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkBlue, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Blue, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkPurple, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Purple, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ToggleBookmarkPink, window, cx| this.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Pink, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ShowOnlyBookmarkRed, window, cx| this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Red, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ShowOnlyBookmarkOrange, window, cx| {
                this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Orange, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ShowOnlyBookmarkYellow, window, cx| {
                this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Yellow, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ShowOnlyBookmarkGreen, window, cx| {
                    this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Green, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &ShowOnlyBookmarkCyan, window, cx| this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Cyan, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ShowOnlyBookmarkBlue, window, cx| this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Blue, window, cx)),
            )
            .on_action(cx.listener(|this, _: &ShowOnlyBookmarkPurple, window, cx| {
                this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Purple, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &ShowOnlyBookmarkPink, window, cx| this.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Pink, window, cx)),
            )
            .on_action(cx.listener(Self::bookmark_red))
            .on_action(cx.listener(Self::bookmark_orange))
            .on_action(cx.listener(Self::bookmark_yellow))
            .on_action(cx.listener(Self::bookmark_green))
            .on_action(cx.listener(Self::bookmark_cyan))
            .on_action(cx.listener(Self::bookmark_blue))
            .on_action(cx.listener(Self::bookmark_purple))
            .on_action(cx.listener(Self::bookmark_pink))
            .on_action(cx.listener(Self::clear_bookmark))
            .on_action(cx.listener(Self::clear_all_bookmarks))
            .on_key_down(cx.listener(Self::on_key_down))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.focus_handle.focus(window, cx);
                    this.pause_cursor_blink(cx);

                    // スクロールバー領域（右端12px）でのクリック判定
                    if let Some(list_b) = this.list_bounds.get() {
                        if event.position.y >= list_b.bottom() {
                            return;
                        }
                        let click_x = f32::from(event.position.x);
                        let bar_x = f32::from(list_b.right()) - 12.0;
                        if click_x >= bar_x {
                            let click_y = f32::from(event.position.y);
                            let list_top = f32::from(list_b.top());
                            let rel_y = click_y - list_top;
                            let list_h = f32::from(list_b.size.height);

                            let total_rows = this.effective_total_rows(cx);
                            let visible_rows = (list_h / ROW_HEIGHT).floor() as usize;
                            let max_top_row = total_rows.saturating_sub(visible_rows.max(1));
                            let ratio = (visible_rows as f64 / total_rows as f64).clamp(0.0, 1.0);
                            let thumb_h = (list_h as f64 * ratio).clamp(24.0, list_h as f64) as f32;
                            let max_thumb_top = (list_h - thumb_h).max(0.0);

                            let cur_thumb_top = if max_top_row > 0 {
                                ((this.scroll.scroll_offset as f64 / max_top_row as f64) * max_thumb_top as f64) as f32
                            } else {
                                0.0
                            };

                            if rel_y >= cur_thumb_top && rel_y <= cur_thumb_top + thumb_h {
                                this.scroll.is_dragging_scrollbar = true;
                                this.scroll.scrollbar_drag_start_y = click_y;
                                this.scroll.scrollbar_drag_start_row = this.scroll.scroll_offset;
                            } else {
                                let target_thumb_top = (rel_y - thumb_h / 2.0).clamp(0.0, max_thumb_top);
                                let new_ratio = if max_thumb_top > 0.0 {
                                    target_thumb_top as f64 / max_thumb_top as f64
                                } else {
                                    0.0
                                };
                                let new_row = (new_ratio * max_top_row as f64).round() as usize;
                                this.scroll_to_row(new_row, cx);
                                this.scroll.is_dragging_scrollbar = true;
                                this.scroll.scrollbar_drag_start_y = click_y;
                                this.scroll.scrollbar_drag_start_row = new_row;
                            }
                            cx.notify();
                            return;
                        }
                    }

                    if let Some(edit_target) = this.edit_target_from_point(event.position, window, cx) {
                        this.commit_pending_hex_with_zero(cx);
                        this.input.set_active_column(edit_target.column());
                        let target_pos = edit_target.offset();
                        let selection_anchor = {
                            let editor = this.editor.read(cx);
                            editor.selection().anchor()
                        };
                        let is_ascii = matches!(edit_target, EditTarget::Ascii { .. });
                        let (char_start, char_end) = if is_ascii {
                            let editor = this.editor.read(cx);
                            if let Ok(doc) = editor.document.read() {
                                let range = this.encoding.char_range_at(doc.buffer.data(), target_pos);
                                (range.start, range.end)
                            } else {
                                (target_pos, target_pos.saturating_add(1))
                            }
                        } else {
                            (target_pos, target_pos.saturating_add(1))
                        };

                        this.mouse_selection_anchor = if event.modifiers.shift {
                            Some(selection_anchor)
                        } else if is_ascii {
                            Some(char_start)
                        } else {
                            Some(target_pos)
                        };
                        this.is_selecting = true;
                        let insert_mode = InsertModeState::is_enabled(cx);
                        this.editor.update(cx, |editor, cx| {
                            if event.modifiers.shift {
                                let drag_target = if is_ascii {
                                    if target_pos >= selection_anchor { char_end } else { char_start }
                                } else {
                                    target_pos
                                };
                                editor.continue_drag(selection_anchor, drag_target);
                            } else if is_ascii && char_end > char_start && !insert_mode {
                                editor.set_selection_range(char_start..char_end);
                            } else {
                                editor.set_cursor_offset_exact(target_pos);
                            }
                            cx.notify();
                        });
                    } else if let Some(target_pos) = this.offset_from_point(event.position, window, cx) {
                        if this.editor.read(cx).is_folded(target_pos) {
                            this.editor.update(cx, |editor, cx| {
                                editor.unfold_bookmark_at(target_pos);
                                cx.notify();
                            });
                            this.notify_document_changed(cx);
                            return;
                        }
                        let selection_anchor = {
                            let editor = this.editor.read(cx);
                            editor.selection().anchor()
                        };
                        this.mouse_selection_anchor = if event.modifiers.shift { Some(selection_anchor) } else { Some(target_pos) };
                        this.is_selecting = true;
                        this.clear_pending_hex_input();
                        this.editor.update(cx, |editor, cx| {
                            if event.modifiers.shift {
                                editor.continue_drag(selection_anchor, target_pos);
                            } else {
                                editor.set_cursor_offset_exact(target_pos);
                            }
                            cx.notify();
                        });
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    this.focus_handle.focus(window, cx);
                    this.pause_cursor_blink(cx);
                    if this.list_bounds.get().map(|bounds| event.position.y >= bounds.bottom()).unwrap_or(false) {
                        return;
                    }
                    if let Some(edit_target) = this.edit_target_from_point(event.position, window, cx) {
                        this.commit_pending_hex_with_zero(cx);
                        this.input.set_active_column(edit_target.column());
                        let target_pos = edit_target.offset();
                        let is_ascii = matches!(edit_target, EditTarget::Ascii { .. });
                        let (char_start, char_end) = if is_ascii {
                            let editor = this.editor.read(cx);
                            if let Ok(doc) = editor.document.read() {
                                let range = this.encoding.char_range_at(doc.buffer.data(), target_pos);
                                (range.start, range.end)
                            } else {
                                (target_pos, target_pos.saturating_add(1))
                            }
                        } else {
                            (target_pos, target_pos.saturating_add(1))
                        };
                        let insert_mode = InsertModeState::is_enabled(cx);
                        this.editor.update(cx, |editor, cx| {
                            let in_selection = editor
                                .selection_range()
                                .is_some_and(|range| target_pos >= range.start && target_pos < range.end);
                            if !in_selection {
                                if is_ascii && !insert_mode && char_end > char_start {
                                    editor.set_selection_range(char_start..char_end);
                                } else {
                                    editor.set_cursor_offset_exact(target_pos);
                                }
                                cx.notify();
                            }
                        });
                    } else if let Some(target_pos) = this.offset_from_point(event.position, window, cx) {
                        this.clear_pending_hex_input();
                        this.editor.update(cx, |editor, cx| {
                            let in_selection = editor
                                .selection_range()
                                .is_some_and(|range| target_pos >= range.start && target_pos < range.end);
                            if !in_selection {
                                editor.set_cursor_offset(target_pos);
                                cx.notify();
                            }
                        });
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, window, cx| {
                let layout = this.current_layout(cx);
                this.sync_outer_scroll_from_handle(layout, cx);

                if let Some(list_b) = this.list_bounds.get() {
                    let pos = event.position;
                    let is_in_bar = pos.x >= list_b.right() - px(12.0) && pos.x <= list_b.right() && pos.y >= list_b.top() && pos.y <= list_b.bottom();
                    if this.scroll.scrollbar_hovered != is_in_bar {
                        this.scroll.scrollbar_hovered = is_in_bar;
                        cx.notify();
                    }
                }

                if let Some((col, start_x, start_w)) = this.resizing_column {
                    let current_x: f32 = event.position.x.into();
                    let delta = current_x - start_x;
                    match col {
                        ResizingColumn::Address => {
                            this.address_col_width = (start_w + delta).max(60.0);
                        }
                        ResizingColumn::Hex => {
                            this.hex_col_width = (start_w + delta).max(100.0);
                        }
                        ResizingColumn::Ascii => {
                            this.ascii_col_width = (start_w + delta).max(MIN_ASCII_COLUMN_WIDTH);
                        }
                        ResizingColumn::Description => {
                            this.desc_col_width = (start_w + delta).max(80.0);
                        }
                        ResizingColumn::Comment => {
                            this.comment_col_width = (start_w + delta).max(80.0);
                        }
                    }
                    this.cursor_reveal_pending = true;
                    this.clamp_scroll_offsets(cx);
                    cx.notify();
                    return;
                }
                if this.is_selecting && event.dragging() {
                    if let Some(list_b) = this.list_bounds.get() {
                        let y = f32::from(event.position.y);
                        let list_top = f32::from(list_b.top());
                        let list_bottom = f32::from(list_b.bottom());
                        if y < list_top {
                            let rows_up = ((list_top - y) / ROW_HEIGHT).ceil() as usize;
                            let new_row = this.scroll.scroll_offset.saturating_sub(rows_up.min(5));
                            this.scroll_to_row(new_row, cx);
                        } else if y > list_bottom {
                            let rows_down = ((y - list_bottom) / ROW_HEIGHT).ceil() as usize;
                            let total_rows = this.effective_total_rows(cx);
                            let max_top_row = total_rows.saturating_sub(1);
                            let new_row = (this.scroll.scroll_offset + rows_down.min(5)).min(max_top_row);
                            this.scroll_to_row(new_row, cx);
                        }
                    }

                    let target_pos = this.offset_from_point(event.position, window, cx).or_else(|| {
                        if let Some(list_b) = this.list_bounds.get()
                            && f32::from(event.position.y) >= f32::from(list_b.top())
                        {
                            let editor = this.editor.read(cx);
                            Some(editor.total_size())
                        } else {
                            None
                        }
                    });

                    if let Some(target_pos) = target_pos {
                        let mouse_selection_anchor = this.mouse_selection_anchor;
                        let is_ascii = this.input.is_ascii();
                        let insert_mode = InsertModeState::is_enabled(cx);
                        let target_drag_pos = if is_ascii && !insert_mode {
                            let editor = this.editor.read(cx);
                            if let Ok(doc) = editor.document.read() {
                                let char_range = this.encoding.char_range_at(doc.buffer.data(), target_pos);
                                let anchor = mouse_selection_anchor.unwrap_or(editor.cursor.offset);
                                if target_pos >= anchor { char_range.end } else { char_range.start }
                            } else {
                                target_pos
                            }
                        } else {
                            target_pos
                        };
                        this.editor.update(cx, |editor, cx| {
                            let anchor = mouse_selection_anchor.unwrap_or(editor.cursor.offset);
                            let prev_selection = editor.selection();
                            let prev_cursor = editor.cursor.offset;
                            editor.continue_drag(anchor, target_drag_pos);
                            if editor.selection() != prev_selection || editor.cursor.offset != prev_cursor {
                                cx.notify();
                            }
                        });
                    }
                }
            }))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .child(
                canvas(
                    move |bounds, _window, cx| {
                        bounds_view.update(cx, |this, _cx| {
                            this.bounds.set(Some(bounds));
                        });
                    },
                    |_bounds, _prepaint, _window, _cx| {},
                )
                .absolute()
                .inset_0(),
            )
            .child(header)
            .child(div().flex_1().w_full().min_w_0().min_h_0().relative().overflow_hidden().child({
                let editor_entity = self.editor.clone();
                let scroll_offset = self.scroll.scroll_offset;
                let outer_scroll_x = self.scroll.outer_scroll_x;
                let hex_scroll_x = self.scroll.hex_scroll_x;
                let ascii_scroll_x = self.scroll.ascii_scroll_x;
                let desc_scroll_x = self.scroll.desc_scroll_x;
                let comment_scroll_x = self.scroll.comment_scroll_x;
                let address_col_width = self.address_col_width;
                let hex_col_width = self.hex_col_width;
                let desc_col_width = self.desc_col_width;
                let comment_col_width = self.comment_col_width;
                let show_offset = self.show_offset;
                let show_ascii = self.show_ascii;
                let encoding = self.encoding;
                let radix = self.radix;
                let group_size = self.group_size;
                let is_big_endian = self.is_big_endian;
                let active_column = self.input.active_column();
                let pending_hex_digit = self.input.pending_hex_digit();
                let _max_highlight_len = self.max_highlight_len;
                let highlights = self.highlights.clone();
                let is_dragging_scrollbar = self.scroll.is_dragging_scrollbar;
                let scrollbar_hovered = self.scroll.scrollbar_hovered;
                let scrollbar_view = view.clone();

                canvas(
                    move |bounds, _window, cx| {
                        list_bounds_view.update(cx, |this, _cx| {
                            let changed = this.list_bounds.get() != Some(bounds);
                            this.list_bounds.set(Some(bounds));
                            if changed {
                                _cx.notify();
                            }
                        });
                    },
                    move |bounds, _prepaint, window, cx| {
                        window.on_mouse_event({
                            let scrollbar_view = scrollbar_view.clone();
                            move |event: &MouseMoveEvent, phase, _window, cx| {
                                if !phase.bubble() || !event.dragging() {
                                    return;
                                }

                                scrollbar_view.update(cx, |this, cx| {
                                    this.update_scrollbar_drag(f32::from(event.position.y), cx);
                                });
                            }
                        });
                        window.on_mouse_event({
                            let scrollbar_view = scrollbar_view.clone();
                            move |event: &MouseUpEvent, phase, _window, cx| {
                                if !phase.bubble() || event.button != MouseButton::Left {
                                    return;
                                }

                                scrollbar_view.update(cx, |this, cx| {
                                    if this.scroll.is_dragging_scrollbar {
                                        this.scroll.is_dragging_scrollbar = false;
                                        cx.notify();
                                    }
                                });
                            }
                        });

                        let (parse_result, collapsed_structs, bookmark_items, doc_arc, line_starts, cursor_offset, insert_cursor_offset, min_sel, max_sel) = {
                            let editor = editor_entity.read(cx);
                            let (min_sel, max_sel) = editor
                                .selection_range()
                                .map(|range| (range.start, range.end.saturating_sub(1)))
                                .unwrap_or((usize::MAX, usize::MIN));
                            (
                                if editor.structure.show_inline_structure_view {
                                    editor.parse_result()
                                } else {
                                    None
                                },
                                Arc::new(editor.structure.collapsed_struct_ids.clone()),
                                Arc::new(editor.bookmarks().snapshot()),
                                editor.document.clone(),
                                editor.line_starts(),
                                editor.cursor.offset,
                                editor.insert_cursor_offset(),
                                min_sel,
                                max_sel,
                            )
                        };
                        let doc = doc_arc.read().expect("document read lock");

                        // Construct combined highlights from the shared snapshot and search results
                        let mut effective_highlights: Vec<(Range<usize>, Hsla)> = bookmark_items.iter().map(|h| (h.range(), h.rgba_color().into())).collect();
                        for (search_range, search_color) in highlights.iter() {
                            if !effective_highlights.iter().any(|(r, _)| r == search_range) {
                                effective_highlights.push((search_range.clone(), *search_color));
                            }
                        }
                        effective_highlights.sort_by_key(|(range, _)| range.start);
                        let effective_max_hl_len = effective_highlights.iter().map(|(r, _)| r.end.saturating_sub(r.start)).max().unwrap_or(0);

                        let list_h = f32::from(bounds.size.height);
                        let visible_rows = (list_h / ROW_HEIGHT).ceil() as usize + 1;
                        let top_row = scroll_offset.min(total_rows.saturating_sub(1));
                        let end_row = (top_row + visible_rows).min(total_rows);

                        for (k, row_idx) in (top_row..end_row).enumerate() {
                            let row_y = bounds.top() + px(k as f32 * ROW_HEIGHT);
                            let row_bounds = Bounds::new(point(bounds.left(), row_y), size(bounds.size.width, px(ROW_HEIGHT)));
                            paint_hex_row(
                                RowPaintParams {
                                    row_idx,
                                    bounds: row_bounds,
                                    top_visible_row: top_row,
                                    doc: &doc,
                                    line_starts: &line_starts,
                                    parse_result: parse_result.as_deref(),
                                    collapsed_structs: &collapsed_structs,
                                    encoding,
                                    radix,
                                    group_size,
                                    is_big_endian,
                                    cursor_offset,
                                    insert_cursor_offset,
                                    min_sel,
                                    max_sel,
                                    highlights: effective_highlights.as_slice(),
                                    bookmark_items: bookmark_items.as_slice(),
                                    max_highlight_len: effective_max_hl_len,
                                    show_offset,
                                    show_ascii,
                                    ascii_col_width,
                                    ascii_scroll_x,
                                    is_focused,
                                    insert_mode,
                                    active_column,
                                    cursor_visible,
                                    pending_hex_digit,
                                    outer_scroll_x,
                                    hex_scroll_x,
                                    desc_scroll_x,
                                    comment_scroll_x,
                                    address_col_width,
                                    hex_col_width,
                                    hex_cell_width,
                                    desc_col_width,
                                    comment_col_width,
                                    font_family: font_family.clone(),
                                    font_size,
                                },
                                window,
                                cx,
                            );
                        }

                        let theme = cx.theme();
                        paint_scrollbar(bounds, top_row, total_rows, is_dragging_scrollbar, scrollbar_hovered, theme, window);
                    },
                )
                .size_full()
            }))
            .child(if layout.outer_max > 0.01 {
                let theme = cx.theme();
                div()
                    .h(px(HORIZONTAL_SCROLLBAR_HEIGHT))
                    .flex_shrink_0()
                    .flex()
                    .child(div().w(px(8.0 + layout.fixed_width)))
                    .child(
                        div().flex_1().mr(px(VERTICAL_SCROLLBAR_WIDTH)).relative().child(
                            Scrollbar::horizontal(&self.scroll.outer_scroll_handle)
                                .mode(ScrollbarMode::Always)
                                .scroll_size(size(px(layout.content_width), px(0.0)))
                                .styles(|_| crate::ui::scrollbar::common_scrollbar_styles(theme)),
                        ),
                    )
                    .into_any_element()
            } else {
                div().h(px(0.0)).into_any_element()
            })
            .context_menu({
                let focus_handle = self.focus_handle.clone();
                let editor = self.editor.clone();
                move |menu, window, cx| {
                    let (is_read_only, can_undo, can_redo, has_selection) = {
                        let ed = editor.read(cx);
                        (ed.is_read_only(), ed.can_undo(), ed.can_redo(), ed.has_selection())
                    };
                    menu.action_context(focus_handle.clone())
                        .menu_with_disabled("Undo", Box::new(Undo), is_read_only || !can_undo)
                        .menu_with_disabled("Redo", Box::new(Redo), is_read_only || !can_redo)
                        .menu_with_disabled("Cut", Box::new(Cut), is_read_only || !has_selection)
                        .menu_with_disabled("Copy", Box::new(Copy), !has_selection)
                        .submenu("Copy As", window, cx, move |menu, _window, _cx| {
                            menu.menu_with_disabled("as Hex Dump", Box::new(CopyAsHexDump), !has_selection)
                                .menu_with_disabled("as C++ Array", Box::new(CopyAsCppArray), !has_selection)
                                .menu_with_disabled("as Hex Stream", Box::new(CopyAsHexStream), !has_selection)
                                .menu_with_disabled("as Hex with Spaces", Box::new(CopyAsHexSpaces), !has_selection)
                                .menu_with_disabled("as Printable Text", Box::new(CopyAsPrintableText), !has_selection)
                                .menu_with_disabled("as Base64", Box::new(CopyAsBase64), !has_selection)
                                .menu_with_disabled("as Escaped String", Box::new(CopyAsEscapedString), !has_selection)
                                .menu_with_disabled("as Binary", Box::new(CopyAsBinary), !has_selection)
                                .menu_with_disabled("as Rust Array", Box::new(CopyAsRustArray), !has_selection)
                                .menu_with_disabled("as JSON Array", Box::new(CopyAsJsonArray), !has_selection)
                        })
                        .menu_with_disabled("Paste", Box::new(Paste), is_read_only)
                        .menu_with_disabled("Fill Selection...", Box::new(FillSelection), is_read_only || !has_selection)
                        .separator()
                        .submenu("Bookmark", window, cx, move |menu, _window, _cx| {
                            menu.menu("Red", Box::new(BookmarkRed))
                                .menu("Orange", Box::new(BookmarkOrange))
                                .menu("Yellow", Box::new(BookmarkYellow))
                                .menu("Green", Box::new(BookmarkGreen))
                                .menu("Cyan", Box::new(BookmarkCyan))
                                .menu("Blue", Box::new(BookmarkBlue))
                                .menu("Purple", Box::new(BookmarkPurple))
                                .menu("Pink", Box::new(BookmarkPink))
                                .separator()
                                .menu("Clear Bookmark", Box::new(ClearBookmark))
                                .menu("Clear All Bookmarks", Box::new(ClearAllBookmarks))
                                .separator()
                                .menu("Show Bookmarks Panel", Box::new(ShowBookmarksTab))
                                .menu("Export Bookmarks...", Box::new(ExportBookmarks))
                                .menu("Import Bookmarks...", Box::new(ImportBookmarks))
                        })
                        .submenu("Bookmark Visibility", window, cx, move |menu, window, cx| {
                            menu.menu("Show All Bookmarks", Box::new(ShowAllBookmarks))
                                .menu("Hide All Bookmarks", Box::new(HideAllBookmarks))
                                .separator()
                                .menu("Show Only Bookmarked Regions", Box::new(ToggleHideUnbookmarked))
                                .separator()
                                .menu("Unfold at Cursor", Box::new(UnfoldBookmarkAtCursor))
                                .separator()
                                .submenu("Toggle by Color", window, cx, move |m, _window, _cx| {
                                    m.menu("Red", Box::new(ToggleBookmarkRed))
                                        .menu("Orange", Box::new(ToggleBookmarkOrange))
                                        .menu("Yellow", Box::new(ToggleBookmarkYellow))
                                        .menu("Green", Box::new(ToggleBookmarkGreen))
                                        .menu("Cyan", Box::new(ToggleBookmarkCyan))
                                        .menu("Blue", Box::new(ToggleBookmarkBlue))
                                        .menu("Purple", Box::new(ToggleBookmarkPurple))
                                        .menu("Pink", Box::new(ToggleBookmarkPink))
                                })
                                .submenu("Show Only Color", window, cx, move |m, _window, _cx| {
                                    m.menu("Only Red", Box::new(ShowOnlyBookmarkRed))
                                        .menu("Only Orange", Box::new(ShowOnlyBookmarkOrange))
                                        .menu("Only Yellow", Box::new(ShowOnlyBookmarkYellow))
                                        .menu("Only Green", Box::new(ShowOnlyBookmarkGreen))
                                        .menu("Only Cyan", Box::new(ShowOnlyBookmarkCyan))
                                        .menu("Only Blue", Box::new(ShowOnlyBookmarkBlue))
                                        .menu("Only Purple", Box::new(ShowOnlyBookmarkPurple))
                                        .menu("Only Pink", Box::new(ShowOnlyBookmarkPink))
                                })
                        })
                        .submenu("Structure", window, cx, move |menu, _window, _cx| {
                            menu.menu("Toggle Inline Structure View", Box::new(ToggleInlineStructureView))
                                .menu("Load Structure Definition...", Box::new(LoadStructureDefinition))
                                .menu("Clear Structure Definition", Box::new(ClearStructureDefinition))
                                .separator()
                                .menu("Show Structure Panel", Box::new(ShowStructureTab))
                        })
                        .separator()
                        .menu("Find / Replace...", Box::new(ToggleSearch))
                        .menu("Select All", Box::new(SelectAll))
                        .separator()
                        .menu("Break Line", Box::new(AddCustomBreak))
                        .menu("Join Lines", Box::new(JoinLine))
                        .menu("Reset Layout", Box::new(ClearAllCustomBreaks))
                }
            })
    }
}

fn hex_digit(character: char) -> Option<u8> {
    match character {
        '0'..='9' => Some(character as u8 - b'0'),
        'a'..='f' => Some(character as u8 - b'a' + 10),
        'A'..='F' => Some(character as u8 - b'A' + 10),
        _ => None,
    }
}

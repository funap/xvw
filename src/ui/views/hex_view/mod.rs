pub mod actions;
pub mod clipboard_handler;
mod cursor_blink;
mod handlers;
mod hit_test;
pub mod input_controller;
pub mod layout;
pub mod paint;
pub mod row_cache;
pub mod scroll_controller;
pub mod types;

pub use row_cache::*;

pub use input_controller::InputController;
pub use scroll_controller::ScrollController;

#[cfg(test)]
mod layout_tests;

pub use actions::init;
pub use actions::*;
#[cfg(test)]
pub use layout::ascii_byte_index_from_world_x;
pub use layout::{
    bounded_auto_fit_range, build_hex_text_source, calculate_scroll_top_for_range, can_chain_to_outer, cursor_in_row_offset, hex_grid_width, hex_grid_x,
    is_cursor_header_column, make_hex_view_layout, measure_hex_cell_width, weighted_text_width,
};
pub use paint::{
    CHEVRON_ICON_GAP, CHEVRON_ICON_SIZE, DescriptionPalette, RowPaintParams, calculate_description_indent_level, flush_column_fade_runs, paint_hex_row,
    paint_scrollbar, row_byte_range,
};
pub use types::*;

use crate::actions::{
    AddCustomBreak, BookmarkBlue, BookmarkCyan, BookmarkGreen, BookmarkOrange, BookmarkPink, BookmarkPurple, BookmarkRed, BookmarkYellow, ClearAllBookmarks,
    ClearAllCustomBreaks, ClearBookmark, ClearStructureDefinition, Copy, CopyAsBase64, CopyAsBinary, CopyAsCppArray, CopyAsEscapedString, CopyAsHexDump,
    CopyAsHexSpaces, CopyAsHexStream, CopyAsJsonArray, CopyAsPrintableText, CopyAsRustArray, Cut, ExportBookmarks, FillSelection, HideAllBookmarks,
    ImportBookmarks, JoinLine, LoadStructureDefinition, Paste, Redo, RemoveCustomBreaksInRange, SelectAll as AppSelectAll, ShowAllBookmarks, ShowBookmarksTab,
    ShowOnlyBookmarkBlue, ShowOnlyBookmarkCyan, ShowOnlyBookmarkGreen, ShowOnlyBookmarkOrange, ShowOnlyBookmarkPink, ShowOnlyBookmarkPurple,
    ShowOnlyBookmarkRed, ShowOnlyBookmarkYellow, ShowStructureTab, ToggleBookmarkBlue, ToggleBookmarkCyan, ToggleBookmarkGreen, ToggleBookmarkOrange,
    ToggleBookmarkPink, ToggleBookmarkPurple, ToggleBookmarkRed, ToggleBookmarkYellow, ToggleHideUnbookmarked, ToggleInlineStructureView, ToggleSearch, Undo,
    UnfoldBookmarkAtCursor,
};
use crate::app_state::InsertModeState;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::radix::{ByteCategoryColors, ByteColoring, ByteGroupSize, DisplayRadix, HexCase};

use crate::core::structure::{IndexedField, ParseResult};
use crate::ui::icon::IconName;
use gpui_kit::component::menu::ContextMenuExt;
use gpui_kit::component::scroll::{Scrollbar, ScrollbarMode};
use gpui_kit::component::{ActiveTheme, StyledExt, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

use cursor_blink::HexCursorBlink;

type CachedDescPalette = std::rc::Rc<std::cell::Cell<Option<(gpui_kit::component::ThemeMode, Hsla, Hsla, DescriptionPalette)>>>;

struct CachedHeaderProbe {
    max_bytes_per_row: usize,
    radix: DisplayRadix,
    group_size: ByteGroupSize,
    is_big_endian: bool,
    hex_case: HexCase,
    source: HexTextSource,
    col_labels: Vec<SharedString>,
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
    highlights_version: u64,
    effective_highlights_cache: std::rc::Rc<std::cell::RefCell<EffectiveHighlightsCache>>,
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
    cached_comment_content_width: std::cell::Cell<Option<CachedCommentContentWidth>>,
    cached_desc_content_width: std::cell::Cell<Option<f32>>,
    cached_header_probe: Option<CachedHeaderProbe>,
    resizing_column: Option<(ResizingColumn, f32, f32)>,
    last_cursor_offset: Option<usize>,
    last_bytes_per_row: Option<usize>,
    last_max_bytes_per_row: Option<usize>,
    last_bookmark_version: u64,
    last_layout_version: usize,
    last_structure_generation: usize,
    last_structure_layout_version: u64,
    cached_desc_palette: CachedDescPalette,
    cached_active_leaf_field: std::rc::Rc<std::cell::RefCell<CachedActiveLeafField>>,
    row_cache: std::rc::Rc<std::cell::RefCell<RowRenderCache>>,
    paint_row_scratch: std::rc::Rc<std::cell::RefCell<PaintRowScratch>>,
    cursor_reveal_pending: bool,
    pub category_palette: Option<ByteCategoryPalette>,
    _editor_subscription: Subscription,
    _insert_mode_subscription: Subscription,
    _cursor_blink_subscription: Subscription,
    _window_activation_subscription: Subscription,
    _byte_coloring_subscription: Subscription,
    _byte_category_colors_subscription: Subscription,
    _hex_case_subscription: Subscription,
}

impl EventEmitter<HexViewEvent> for HexView {}

#[allow(dead_code)]
impl HexView {
    pub fn new(editor: Entity<Editor>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (radix, group_size, is_big_endian, encoding, last_bookmark_version, last_layout_version, last_structure_generation, last_structure_layout_version) = {
            let ed = editor.read(cx);
            (
                ed.options.radix,
                ed.options.group_size,
                ed.options.is_big_endian,
                ed.options.encoding,
                ed.bookmarks().version(),
                ed.document.read().ok().map(|d| d.layout_version()).unwrap_or(0),
                ed.structure.generation,
                ed.structure.layout_version(),
            )
        };
        let cached_desc_palette = std::rc::Rc::new(std::cell::Cell::new(None));
        let cached_active_leaf_field = std::rc::Rc::new(std::cell::RefCell::new(CachedActiveLeafField::default()));
        let row_cache = std::rc::Rc::new(std::cell::RefCell::new(RowRenderCache::default()));
        let paint_row_scratch = std::rc::Rc::new(std::cell::RefCell::new(PaintRowScratch::default()));
        let effective_highlights_cache = std::rc::Rc::new(std::cell::RefCell::new(EffectiveHighlightsCache::default()));
        let font_size_prop = px(14.0);
        let hex_col_width = 0.0;
        let focus_handle = cx.focus_handle();
        let cursor_blink = cx.new(|_| HexCursorBlink::new());
        let _cursor_blink_subscription = cx.observe(&cursor_blink, |_, _, cx| {
            if InsertModeState::is_enabled(cx) {
                cx.notify();
            }
        });
        let _window_activation_subscription = cx.observe_window_activation(window, |this, window, cx| {
            let should_blink = window.is_window_active() && this.focus_handle.is_focused(window) && InsertModeState::is_enabled(cx);
            this.cursor_blink.update(cx, |cursor, cx| {
                if should_blink {
                    cursor.start(cx);
                } else {
                    cursor.stop(cx);
                }
            });
        });

        cx.on_focus_in(&focus_handle, window, |this, _, cx| {
            let insert_mode = InsertModeState::is_enabled(cx);
            this.cursor_blink.update(cx, |cursor, cx| {
                if insert_mode {
                    cursor.start(cx);
                } else {
                    cursor.stop(cx);
                }
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
            let (
                current_bm_version,
                current_layout_version,
                current_generation,
                current_structure_layout_version,
                is_read_only,
                new_encoding,
                new_radix,
                new_group_size,
                new_endian,
                new_bytes_per_row,
                cursor_offset,
            ) = {
                let ed = editor_entity.read(cx);
                let cursor_offset = if InsertModeState::is_enabled(cx) {
                    ed.insert_cursor_offset()
                } else {
                    ed.cursor.offset
                };
                (
                    ed.bookmarks().version(),
                    ed.document.read().ok().map(|d| d.layout_version()),
                    ed.structure.generation,
                    ed.structure.layout_version(),
                    ed.is_read_only(),
                    ed.options.encoding,
                    ed.options.radix,
                    ed.options.group_size,
                    ed.options.is_big_endian,
                    ed.bytes_per_row(),
                    cursor_offset,
                )
            };

            if this.last_bookmark_version != current_bm_version {
                this.last_bookmark_version = current_bm_version;
                this.cached_comment_content_width.set(None);
            }
            if let Some(current_lv) = current_layout_version
                && this.last_layout_version != current_lv
            {
                this.last_layout_version = current_lv;
                this.cached_comment_content_width.set(None);
            }
            if this.last_structure_generation != current_generation || this.last_structure_layout_version != current_structure_layout_version {
                this.last_structure_generation = current_generation;
                this.last_structure_layout_version = current_structure_layout_version;
                this.clear_desc_content_width_cache();
            }
            if is_read_only {
                this.clear_pending_hex_input();
            }
            let cursor_changed = this.last_cursor_offset != Some(cursor_offset);
            if cursor_changed {
                this.cursor_reveal_pending = true;
                this.pause_cursor_blink(cx);
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
            if this.last_bytes_per_row != Some(new_bytes_per_row) {
                this.last_bytes_per_row = Some(new_bytes_per_row);
                this.cached_comment_content_width.set(None);
                this.hex_content_width = 0.0;
                this.cursor_reveal_pending = true;
            }
            if cursor_changed {
                this.ensure_cursor_visible(cx);
            }
            this.clamp_scroll_offsets(cx);
            cx.notify();
        });

        let _insert_mode_subscription = cx.observe_global::<InsertModeState>(|this, cx| {
            this.pause_cursor_blink(cx);
            this.clear_pending_hex_input();
            this.cursor_reveal_pending = true;
            this.ensure_cursor_visible(cx);
            cx.notify();
        });

        let _byte_coloring_subscription = cx.observe_global::<ByteColoring>(|_, cx| {
            cx.notify();
        });

        let _byte_category_colors_subscription = cx.observe_global::<ByteCategoryColors>(|_, cx| {
            cx.notify();
        });

        let _hex_case_subscription = cx.observe_global::<HexCase>(|this, cx| {
            this.row_cache.borrow_mut().clear();
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
            highlights_version: 0,
            effective_highlights_cache,
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
            cached_header_probe: None,
            resizing_column: None,
            last_cursor_offset: None,
            last_bytes_per_row: None,
            last_max_bytes_per_row: None,
            last_bookmark_version,
            last_layout_version,
            last_structure_generation,
            last_structure_layout_version,
            cached_desc_palette,
            cached_active_leaf_field,
            row_cache,
            paint_row_scratch,
            cursor_reveal_pending: true,
            category_palette: None,
            _editor_subscription,
            _insert_mode_subscription,
            _cursor_blink_subscription,
            _window_activation_subscription,
            _byte_coloring_subscription,
            _byte_category_colors_subscription,
            _hex_case_subscription,
        }
    }

    /// Sets or clears the custom category palette override for this `HexView`.
    /// When `None`, the palette is derived dynamically from global `ByteCategoryColors` and current theme.
    pub fn set_category_palette(&mut self, palette: Option<ByteCategoryPalette>, cx: &mut Context<Self>) {
        self.category_palette = palette;
        cx.notify();
    }

    /// Returns the current custom category palette override, if set.
    pub fn category_palette(&self) -> Option<ByteCategoryPalette> {
        self.category_palette
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
        let (visible_start, visible_end, total_size, _) = self.viewport_byte_range_with_doc_state(self.editor.read(cx));
        bounded_auto_fit_range(total_size, visible_start, visible_end)
    }

    fn visible_auto_fit_scan_range(&self, cx: &App) -> Range<usize> {
        let (visible_start, visible_end, total_size, _) = self.viewport_byte_range_with_doc_state(self.editor.read(cx));
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
                if parse_result.is_live() {
                    continue;
                }
                let containers = parse_result.find_container_structs_starting_at(offset, 1);
                for container in containers.iter().filter(|container| !container.is_instance && container.size > 0) {
                    let text = if collapsed_structs.contains(&container.collapse_key) || collapsed_structs.contains(&container.id) {
                        format!("{} ({} bytes)", container.format_container_label(), container.size)
                    } else {
                        container.format_container_label()
                    };
                    let row_width = container.depth as f32 * 14.0 + (CHEVRON_ICON_SIZE + CHEVRON_ICON_GAP) + weighted_text_width(&text, char_w) + 8.0;
                    max_width = max_width.max(row_width);
                }
                continue;
            }

            let container_structs: &[IndexedField] = if parse_result.is_live() {
                &[]
            } else {
                parse_result.find_container_structs_starting_at(offset, chunk_len)
            };
            let leaf_fields: Cow<'_, [IndexedField]> = if parse_result.is_live() {
                Cow::Owned(parse_result.find_live_leaf_fields_starting_at(offset, chunk_len))
            } else {
                Cow::Borrowed(parse_result.find_leaf_fields_starting_at(offset, chunk_len))
            };

            if container_structs.is_empty() && leaf_fields.is_empty() {
                continue;
            }

            let is_collapsed = container_structs
                .first()
                .map(|c| collapsed_structs.contains(&c.collapse_key) || collapsed_structs.contains(&c.id))
                .unwrap_or(false);

            let container_for_indent = if is_collapsed || leaf_fields.is_empty() {
                container_structs.first()
            } else {
                None
            };
            let active_range_count = if container_for_indent.is_none() && leaf_fields.is_empty() && !parse_result.is_live() {
                parse_result.count_active_struct_ranges(offset, chunk_len)
            } else {
                0
            };
            let indent_level = calculate_description_indent_level(None, container_for_indent, leaf_fields.first(), active_range_count);
            let indent_px = indent_level as f32 * 14.0;

            let mut parts_width = 0.0;
            let mut part_count = 0;

            if let Some(container) = container_structs.first() {
                let text = if is_collapsed {
                    format!("{} ({} bytes)", container.format_container_label(), container.size)
                } else {
                    container.format_container_label()
                };
                parts_width += (CHEVRON_ICON_SIZE + CHEVRON_ICON_GAP) + weighted_text_width(&text, char_w);
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

        let bookmarks_ref = editor.bookmarks();
        let bookmarks = bookmarks_ref.items();
        if bookmarks.is_empty() {
            return 0.0;
        }

        let start_idx = bookmarks.partition_point(|bookmark| bookmark.offset < scan_range.start).saturating_sub(1);
        let end_idx = bookmarks.partition_point(|bookmark| bookmark.offset < scan_range.end);
        let candidates = &bookmarks[start_idx..end_idx];

        let mut line_starts: Option<crate::core::layout::LineMap> = None;
        let mut max_width: f32 = 0.0;
        let mut current_row: Option<(usize, f32)> = None;
        for bookmark in candidates.iter().take(AUTO_FIT_MAX_ITEMS) {
            let trimmed = bookmark.comment.trim();
            if trimmed.is_empty() {
                continue;
            }
            let starts = line_starts.get_or_insert_with(|| editor.line_starts());
            let row = Editor::find_line_index(bookmark.offset, starts);
            let item_width = 8.0 + 5.0 + weighted_text_width(trimmed, char_w) + 14.0;
            // Bookmarks are ordered by `(offset, size)` and `find_line_index` is non-decreasing.
            match current_row {
                Some((prev_row, prev_w)) if prev_row == row => {
                    let next_w = prev_w + item_width;
                    current_row = Some((row, next_w));
                    max_width = max_width.max(next_w);
                }
                _ => {
                    let next_w = 8.0 + item_width;
                    current_row = Some((row, next_w));
                    max_width = max_width.max(next_w);
                }
            }
        }

        max_width
    }

    #[inline]
    fn comment_scroll_for_width(&self, max_content_w: f32) -> f32 {
        let max_w = (max_content_w + 32.0).max(self.comment_col_width);
        (max_w - self.comment_col_width).max(0.0)
    }

    fn store_comment_content_width(&self, editor: &Editor, scan_range: &Range<usize>, total_size: usize, layout_version: usize, width: f32) {
        self.cached_comment_content_width.set(Some(CachedCommentContentWidth {
            width,
            scan_start: scan_range.start,
            scan_end: scan_range.end,
            total_size,
            bookmark_version: editor.bookmarks().version(),
            layout_version,
            bytes_per_row: editor.bytes_per_row(),
            top_row: self.scroll.scroll_offset,
            visible_rows: self.viewport_visible_rows(),
        }));
    }

    pub fn max_comment_scroll(&self, cx: &App) -> f32 {
        let editor = self.editor.read(cx);
        if editor.bookmarks().items().is_empty() {
            self.cached_comment_content_width.set(None);
            return 0.0;
        }

        let bookmark_version = editor.bookmarks().version();
        let bytes_per_row = editor.bytes_per_row();
        let top_row = self.scroll.scroll_offset;
        let visible_rows = self.viewport_visible_rows();

        if let Some(cached) = self.cached_comment_content_width.get()
            && cached.matches_row_state(bookmark_version, bytes_per_row, top_row, visible_rows)
        {
            return self.comment_scroll_for_width(cached.width);
        }

        let (visible_start, visible_end, total_size, layout_version) = self.viewport_byte_range_with_doc_state(editor);
        let clamped_start = visible_start.min(total_size);
        let clamped_end = visible_end.clamp(clamped_start, total_size);

        if let Some(cached) = self.cached_comment_content_width.get()
            && cached.covers_viewport(clamped_start, clamped_end, total_size, bookmark_version, layout_version, bytes_per_row)
        {
            self.cached_comment_content_width.set(Some(CachedCommentContentWidth {
                top_row,
                visible_rows,
                ..cached
            }));
            return self.comment_scroll_for_width(cached.width);
        }

        let scan_range = bounded_auto_fit_range(total_size, clamped_start, clamped_end);
        let char_w = f32::from(self.font_size_prop) * 0.61;
        let max_content_w = Self::comment_content_width_in_range(editor, &scan_range, char_w);
        self.store_comment_content_width(editor, &scan_range, total_size, layout_version, max_content_w);
        self.comment_scroll_for_width(max_content_w)
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

    fn ensure_header_probe(&mut self, max_bytes_per_row: usize, hex_case: HexCase) {
        let is_valid = self.cached_header_probe.as_ref().is_some_and(|probe| {
            probe.max_bytes_per_row == max_bytes_per_row
                && probe.radix == self.radix
                && probe.group_size == self.group_size
                && probe.is_big_endian == self.is_big_endian
                && probe.hex_case == hex_case
        });
        if is_valid {
            return;
        }

        const ZERO_BUFFER: [u8; 512] = [0u8; 512];
        let group_bytes = self.group_size.byte_count();
        let items_in_row = max_bytes_per_row.div_ceil(group_bytes).max(1);
        let probe_len = items_in_row * group_bytes;
        let source = if probe_len <= ZERO_BUFFER.len() {
            build_hex_text_source(&ZERO_BUFFER[..probe_len], 0, self.radix, self.group_size, self.is_big_endian, hex_case)
        } else {
            let probe_bytes = vec![0u8; probe_len];
            build_hex_text_source(&probe_bytes, 0, self.radix, self.group_size, self.is_big_endian, hex_case)
        };
        let col_labels = (0..source.groups.len())
            .map(|i| SharedString::from(format!("+{}", hex_case.format_offset(i * group_bytes, 1))))
            .collect();

        self.cached_header_probe = Some(CachedHeaderProbe {
            max_bytes_per_row,
            radix: self.radix,
            group_size: self.group_size,
            is_big_endian: self.is_big_endian,
            hex_case,
            source,
            col_labels,
        });
    }

    pub(super) fn current_layout(&self, cx: &App) -> HexViewLayout {
        let (max_bytes_per_row, is_struct_mode) = {
            let editor = self.editor.read(cx);
            let line_starts = editor.line_starts();
            (
                line_starts.max_bytes_per_row(),
                editor.structure.show_inline_structure_view && editor.parse_result().is_some(),
            )
        };
        self.current_layout_with(max_bytes_per_row, is_struct_mode, cx)
    }

    fn current_layout_with(&self, max_bytes_per_row: usize, is_struct_mode: bool, cx: &App) -> HexViewLayout {
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
            ascii_inner_max: if self.show_ascii {
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
        if !self.scroll.has_nonzero_horizontal_scroll() {
            return;
        }
        let layout = self.current_layout(cx);
        self.scroll.clamp_scroll_offsets(layout);
    }

    pub fn set_top_overlay_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        let max_overscroll = if visible { 3 } else { 0 };
        let changed = self.scroll.set_max_top_overscroll_rows(max_overscroll);
        if changed || (!visible && self.scroll.top_overscroll_rows > 0) {
            if !visible {
                self.scroll.top_overscroll_rows = 0;
            }
            cx.notify();
        }
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
                let editor = self.editor.read(cx);
                let (visible_start, visible_end, total_size, layout_version) = self.viewport_byte_range_with_doc_state(editor);
                let scan_range = bounded_auto_fit_range(total_size, visible_start, visible_end);
                let char_w = f32::from(self.font_size_prop) * 0.61;
                let max_content_w = Self::comment_content_width_in_range(editor, &scan_range, char_w);
                self.store_comment_content_width(editor, &scan_range, total_size, layout_version, max_content_w);
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

    fn on_scroll(&mut self, event: &ScrollEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let now = std::time::Instant::now();
        let pixel_delta = event.delta.pixel_delta(px(ROW_HEIGHT));
        let mut delta_x = f32::from(pixel_delta.x);
        let delta_y = f32::from(pixel_delta.y);
        let column_only_horizontal = event.modifiers.shift;

        if delta_x == 0.0 && column_only_horizontal && delta_y != 0.0 {
            delta_x = delta_y;
        }

        if event.touch_phase == TouchPhase::Ended {
            self.scroll.scroll_lock_axis = None;
        } else if let Some(last_time) = self.scroll.last_scroll_time
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
        if self.highlights.as_ref() == &highlights {
            return;
        }
        self.highlights = Arc::new(highlights);
        self.highlights_version = self.highlights_version.wrapping_add(1);
        cx.notify();
    }

    pub fn set_highlights_arc(&mut self, highlights: Arc<Vec<(Range<usize>, Hsla)>>, cx: &mut Context<Self>) {
        if Arc::ptr_eq(&self.highlights, &highlights) || self.highlights.as_ref() == highlights.as_ref() {
            return;
        }
        self.highlights = highlights;
        self.highlights_version = self.highlights_version.wrapping_add(1);
        cx.notify();
    }

    pub fn set_highlight_ranges(&mut self, ranges: Vec<Range<usize>>, cx: &mut Context<Self>) {
        let highlight_color = cx.theme().danger;
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

        if self.scroll.max_top_overscroll_rows > 0 && self.scroll.scroll_offset == 0 && start_row < self.scroll.max_top_overscroll_rows {
            let needed = self.scroll.max_top_overscroll_rows.saturating_sub(start_row);
            if self.scroll.top_overscroll_rows < needed {
                self.scroll.top_overscroll_rows = needed;
                cx.notify();
            }
        }
    }

    pub fn scroll_to_byte_if_needed(&mut self, byte_offset: usize, cx: &mut Context<Self>) {
        self.scroll_to_range_if_needed(byte_offset..byte_offset, cx);
    }

    pub fn current_scroll_top_row(&self) -> usize {
        self.scroll.scroll_offset
    }

    #[inline]
    fn viewport_visible_rows(&self) -> usize {
        if let Some(bounds) = self.list_bounds.get() {
            (f32::from(bounds.size.height) / ROW_HEIGHT).ceil() as usize
        } else {
            30
        }
    }

    #[inline]
    fn byte_range_for_rows(line_starts: &crate::core::layout::LineMap, total_size: usize, current_top: usize, visible_rows: usize) -> (usize, usize) {
        let start_byte = line_starts.get(current_top).unwrap_or(total_size);
        let end_row = (current_top + visible_rows + 2).min(line_starts.len());
        let end_byte = if end_row < line_starts.len() {
            line_starts.get(end_row).unwrap_or(total_size)
        } else {
            total_size
        };
        (start_byte, end_byte)
    }

    fn viewport_byte_range_with_doc_state(&self, editor: &Editor) -> (usize, usize, usize, usize) {
        let doc = editor.document.read().expect("document read lock");
        let total_size = doc.buffer.len();
        let layout_version = doc.layout_version();
        let line_starts = editor.layout.line_starts(
            &doc,
            editor.structure.show_inline_structure_view,
            editor.structure.is_parse_in_progress(),
            &editor.structure.collapsed_struct_ids,
        );
        let (start_byte, end_byte) = Self::byte_range_for_rows(&line_starts, total_size, self.scroll.scroll_offset, self.viewport_visible_rows());
        (start_byte, end_byte, total_size, layout_version)
    }

    pub fn viewport_byte_range(&self, cx: &App) -> (usize, usize) {
        let (start_byte, end_byte, _, _) = self.viewport_byte_range_with_doc_state(self.editor.read(cx));
        (start_byte, end_byte)
    }

    fn effective_total_rows_for(line_starts: &crate::core::layout::LineMap, total_size: usize, cursor_offset: usize) -> usize {
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

    fn effective_total_rows(&self, cx: &App) -> usize {
        let editor = self.editor.read(cx);
        let line_starts = editor.line_starts();
        let total_size = editor.total_size();
        let cursor_offset = if InsertModeState::is_enabled(cx) {
            editor.insert_cursor_offset()
        } else {
            editor.cursor.offset
        };
        Self::effective_total_rows_for(&line_starts, total_size, cursor_offset)
    }

    pub fn scroll_to_row(&mut self, row: usize, cx: &mut Context<Self>) {
        let total_rows = self.effective_total_rows(cx);
        let list_h = self.list_bounds.get().map(|b| f32::from(b.size.height)).unwrap_or(600.0);
        let visible_rows = (list_h / ROW_HEIGHT).floor() as usize;

        if self.scroll.scroll_to_row(row, total_rows, visible_rows) {
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
        if let Some(range) = editor.selection_range() {
            self.scroll_to_range_if_needed(range, cx);
            return;
        }
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

        let (total_rows, bytes_per_row, max_bytes_per_row, is_struct_mode, total_size, line_starts, reveal_cursor_offset) = {
            let editor = self.editor.read(cx);
            let line_starts = editor.line_starts();
            let total_size = editor.total_size();
            let reveal_cursor_offset = if insert_mode { editor.insert_cursor_offset() } else { editor.cursor.offset };
            let total_rows = Self::effective_total_rows_for(&line_starts, total_size, reveal_cursor_offset);
            (
                total_rows,
                line_starts.bytes_per_row(),
                line_starts.max_bytes_per_row(),
                editor.structure.show_inline_structure_view && editor.parse_result().is_some(),
                total_size,
                line_starts,
                reveal_cursor_offset,
            )
        };
        let bytes_per_row_changed = self.last_bytes_per_row != Some(bytes_per_row) || self.last_max_bytes_per_row != Some(max_bytes_per_row);
        if bytes_per_row_changed {
            self.last_bytes_per_row = Some(bytes_per_row);
            self.last_max_bytes_per_row = Some(max_bytes_per_row);
            self.ascii_col_width = Self::default_ascii_col_width(max_bytes_per_row);
            self.cursor_reveal_pending = true;
            self.row_cache.borrow_mut().clear();
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

        let hex_cell_width = if self.hex_cell_width > 0.0 {
            px(self.hex_cell_width)
        } else {
            let measured = measure_hex_cell_width(window, font(font_family.clone()), font_size);
            self.hex_cell_width = f32::from(measured);
            measured
        };
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        self.ensure_header_probe(max_bytes_per_row, hex_case);
        let probe_text_len = self.cached_header_probe.as_ref().expect("header probe cached").source.text.len();
        let total_data_width = f32::from(hex_grid_width(probe_text_len, hex_cell_width));
        self.hex_content_width = total_data_width;
        if bytes_per_row_changed || self.hex_col_width <= 0.0 {
            self.hex_col_width = total_data_width;
        }
        let max_hex_scroll = (total_data_width - self.hex_col_width).max(0.0);
        self.scroll.hex_scroll_x = self.scroll.hex_scroll_x.clamp(0.0, max_hex_scroll);
        let layout = self.current_layout_with(max_bytes_per_row, is_struct_mode, cx);
        self.scroll.outer_scroll_x = self.scroll.outer_scroll_x.clamp(0.0, layout.outer_max);
        self.scroll.outer_scroll_handle.set_offset(point(-px(self.scroll.outer_scroll_x), px(0.0)));

        // Keep the cursor visible using the same fixed grid as the paint pass.
        let should_reveal_cursor = self.cursor_reveal_pending || self.last_cursor_offset != Some(reveal_cursor_offset);
        if should_reveal_cursor {
            let row = Editor::find_line_index(reveal_cursor_offset, &line_starts);
            let line_offset = line_starts.get(row).unwrap_or(0);
            let next_offset = line_starts.get(row + 1).unwrap_or(total_size);
            let cursor_layout = {
                let editor = self.editor.read(cx);
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
                            hex_case,
                        );
                        (reveal_cursor_offset.saturating_sub(line_offset), source)
                    })
                }
            };
            if let Some((cursor_in_row, source)) = cursor_layout {
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
            let cursor_in_row = cursor_in_row_offset(reveal_cursor_offset, total_size, &line_starts);
            let probe = self.cached_header_probe.as_ref().expect("header probe cached");
            let total_groups = probe.source.groups.len();
            let mut hex_cols = Vec::with_capacity(probe.col_labels.len());
            for (i, group) in probe.source.groups.iter().enumerate() {
                let label = probe.col_labels[i].clone();
                let group_start = f32::from(hex_grid_x(group.text_start, hex_cell_width));
                let group_end = f32::from(hex_grid_x(group.text_end, hex_cell_width));
                let is_cursor_col = is_cursor_header_column(cursor_in_row, i, *group, total_groups);
                let col_color = if is_cursor_col { theme.foreground } else { theme.muted_foreground };
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
                        .text_color(col_color)
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
                                        .bg(theme.sidebar)
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
                                        .bg(theme.sidebar)
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
                            .hover(|s| s.bg(theme.accent))
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
                .pl_2()
                .pr(px(VERTICAL_SCROLLBAR_WIDTH))
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
                                .hover(|s| s.bg(theme.accent))
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
                    h_flex().flex_1().min_w_0().h(px(HEADER_HEIGHT)).overflow_hidden().relative().child(
                        h_flex()
                            .relative()
                            .left(px(-self.scroll.outer_scroll_x))
                            .h(px(HEADER_HEIGHT))
                            .flex_shrink_0()
                            .child(
                                h_flex()
                                    .h(px(HEADER_HEIGHT))
                                    .flex_shrink_0()
                                    .w(px(self.hex_col_width + SECTION_GAP))
                                    .child(
                                        div()
                                            .w(px(self.hex_col_width))
                                            .h(px(HEADER_HEIGHT))
                                            .overflow_hidden()
                                            .relative()
                                            .children(hex_cols)
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
                                                        .bg(theme.sidebar)
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
                                                        .bg(theme.sidebar)
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
                                            .hover(|s| s.bg(theme.accent))
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
                            .when(self.show_ascii, |el| {
                                let label = self.encoding.label();
                                el.child(
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
                                                            .bg(theme.sidebar)
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
                                                            .bg(theme.sidebar)
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
                                                .hover(|s| s.bg(theme.accent))
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
                            })
                            .when(is_struct_mode, |el| {
                                el.child(
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
                                                    el.child(div().absolute().left_0().top_0().bottom_0().w(px(14.0)).bg(theme.sidebar).child("…"))
                                                })
                                                .when(is_desc_clipped_right, |el| {
                                                    el.child(div().absolute().right_0().top_0().bottom_0().w(px(14.0)).bg(theme.sidebar).child("…"))
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
                                                .hover(|s| s.bg(theme.accent))
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
                            })
                            .child(comment_header_el(
                                self.comment_col_width,
                                is_comment_clipped_left,
                                is_comment_clipped_right,
                                theme,
                            )),
                    ),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        };

        let bounds_view = view.clone();
        let list_bounds_view = view.clone();

        container
            .track_focus(&self.focus_handle(cx))
            .on_scroll(cx.listener(Self::on_scroll))
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
                        let (is_struct_collapse_toggle, struct_collapse_target, is_desc_field_selection) = {
                            let editor = this.editor.read(cx);
                            let line_starts = editor.line_starts();
                            let layout = this.current_layout(cx);
                            let mut is_toggle = false;
                            let mut collapse_key = None;
                            let mut selected_field_range = None;

                            if editor.structure.show_inline_structure_view
                                && !editor.structure.is_parse_in_progress()
                                && let Some(parse_res) = editor.parse_result()
                                && !parse_res.is_live()
                                && let Some(list_bounds) = this.list_bounds.get()
                            {
                                let rel_y = f32::from(event.position.y - list_bounds.top()).max(0.0);
                                let top_overscroll_px = this.scroll.top_overscroll_rows as f32 * ROW_HEIGHT;
                                if rel_y >= top_overscroll_px {
                                    let row_offset_in_view = ((rel_y - top_overscroll_px) / ROW_HEIGHT).floor() as usize;
                                    let row_idx = this.scroll.scroll_offset + row_offset_in_view;
                                    if row_idx < line_starts.len() {
                                        let line_offset = line_starts.get(row_idx).unwrap_or(0);
                                        let next_offset = if row_idx + 1 < line_starts.len() {
                                            line_starts.get(row_idx + 1).unwrap_or(line_offset)
                                        } else {
                                            editor.total_size()
                                        };
                                        let chunk_len = next_offset.saturating_sub(line_offset);
                                        let base_x = f32::from(list_bounds.left()) + 8.0;
                                        let relative_x = f32::from(event.position.x) - base_x;
                                        let world_x = relative_x + this.scroll.outer_scroll_x;
                                        let in_desc = layout.description.is_some_and(|col| world_x >= col.start);

                                        let is_collapsed_container_row = parse_res.find_container_structs_starting_at(line_offset, chunk_len).iter().any(|c| {
                                            !c.is_instance
                                                && c.offset == line_offset
                                                && (c.size == chunk_len || chunk_len == c.size.min(editor.total_size().saturating_sub(line_offset)))
                                                && editor.structure.is_struct_collapsed(&c.collapse_key, &c.id)
                                        });

                                        if chunk_len == 0 {
                                            let containers: Vec<_> = parse_res
                                                .find_container_structs_starting_at(line_offset, 1)
                                                .iter()
                                                .filter(|container| !container.is_instance && container.size > 0)
                                                .cloned()
                                                .collect();
                                            let mut same_offset_rows = 0;
                                            let mut previous_row = row_idx;
                                            while previous_row > 0 && line_starts.get(previous_row - 1) == Some(line_offset) {
                                                same_offset_rows += 1;
                                                previous_row -= 1;
                                            }
                                            if let Some(c) = containers.get(same_offset_rows) {
                                                is_toggle = true;
                                                collapse_key = Some(c.collapse_key.clone());
                                            }
                                        } else if is_collapsed_container_row {
                                            let containers = parse_res.find_container_structs_starting_at(line_offset, chunk_len);
                                            if let Some(c) = containers.iter().find(|c| {
                                                !c.is_instance
                                                    && c.offset == line_offset
                                                    && (c.size == chunk_len || chunk_len == c.size.min(editor.total_size().saturating_sub(line_offset)))
                                                    && editor.structure.is_struct_collapsed(&c.collapse_key, &c.id)
                                            }) {
                                                is_toggle = true;
                                                collapse_key = Some(c.collapse_key.clone());
                                            }
                                        } else if in_desc {
                                            let containers = parse_res.find_container_structs_starting_at(line_offset, chunk_len);
                                            if let Some(c) = containers.iter().find(|c| {
                                                !c.is_instance && c.offset == line_offset && editor.structure.is_struct_collapsed(&c.collapse_key, &c.id)
                                            }) {
                                                is_toggle = true;
                                                collapse_key = Some(c.collapse_key.clone());
                                            } else if event.click_count >= 2 {
                                                let leaves = parse_res.find_leaf_fields_starting_at(line_offset, chunk_len);
                                                let leaf = leaves.first().or_else(|| parse_res.find_deepest_leaf_at(target_pos));
                                                if let Some(l) = leaf {
                                                    selected_field_range = Some(l.offset..l.offset + l.size);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            (is_toggle, collapse_key, selected_field_range)
                        };

                        if is_struct_collapse_toggle && let Some(key) = struct_collapse_target {
                            this.editor.update(cx, |editor, cx| {
                                editor.toggle_struct_collapsed(&key);
                                cx.notify();
                            });
                            this.notify_document_changed(cx);
                            return;
                        }

                        if this.editor.read(cx).is_folded(target_pos) {
                            let unfolded = this.editor.update(cx, |editor, cx| {
                                let res = editor.unfold_bookmark_at(target_pos);
                                if res {
                                    cx.notify();
                                }
                                res
                            });
                            if unfolded {
                                this.notify_document_changed(cx);
                                return;
                            }
                        }

                        if let Some(range) = is_desc_field_selection {
                            this.editor.update(cx, |editor, cx| {
                                editor.set_selection_range(range.clone());
                                editor.set_cursor_offset_exact(range.start);
                                cx.notify();
                            });
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
                let handle_offset = (-f32::from(this.scroll.outer_scroll_handle.offset().x)).max(0.0);
                if (handle_offset - this.scroll.outer_scroll_x).abs() > 0.01 {
                    let layout = this.current_layout(cx);
                    this.sync_outer_scroll_from_handle(layout, cx);
                }

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
                let top_overscroll_rows = self.scroll.top_overscroll_rows;
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
                let highlights = self.highlights.clone();
                let highlights_version = self.highlights_version;
                let effective_highlights_cache = self.effective_highlights_cache.clone();
                let is_dragging_scrollbar = self.scroll.is_dragging_scrollbar;
                let scrollbar_hovered = self.scroll.scrollbar_hovered;
                let scrollbar_view = view.clone();
                let custom_category_palette = self.category_palette;
                let cached_desc_palette = self.cached_desc_palette.clone();
                let cached_active_leaf_field = self.cached_active_leaf_field.clone();
                let row_cache = self.row_cache.clone();
                let paint_row_scratch = self.paint_row_scratch.clone();

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

                        let (
                            parse_result,
                            collapsed_structs,
                            structure_layout_version,
                            is_parsing_structure,
                            bookmark_items,
                            bookmark_version,
                            doc_arc,
                            line_starts,
                            cursor_offset,
                            insert_cursor_offset,
                            min_sel,
                            max_sel,
                        ) = {
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
                                editor.structure.collapsed_struct_ids.clone(),
                                editor.structure.layout_version(),
                                editor.structure.is_parse_in_progress(),
                                editor.bookmarks().snapshot_arc(),
                                editor.bookmarks().version(),
                                editor.document.clone(),
                                editor.line_starts(),
                                editor.cursor.offset,
                                editor.insert_cursor_offset(),
                                min_sel,
                                max_sel,
                            )
                        };
                        let doc = doc_arc.read().expect("document read lock");

                        let list_h = f32::from(bounds.size.height);
                        let visible_rows = (list_h / ROW_HEIGHT).ceil() as usize + 1;
                        let top_row = scroll_offset.min(total_rows.saturating_sub(1));
                        let end_row = (top_row + visible_rows).min(total_rows);
                        let total_size = doc.buffer.len();
                        let has_folded_bookmarks = doc.has_folded_regions();
                        let has_address_gaps = doc.address_map.has_gaps();
                        let vp_start = line_starts.get(top_row).unwrap_or(total_size);
                        let vp_end = if end_row < line_starts.len() {
                            line_starts.get(end_row).unwrap_or(total_size)
                        } else {
                            total_size
                        };
                        let mut hl_cache = effective_highlights_cache.borrow_mut();
                        hl_cache.ensure_updated(bookmark_version, &bookmark_items, highlights_version, &highlights);
                        let mut hl_sweep = hl_cache.viewport_sweep(vp_start, vp_end);
                        let category_palette = custom_category_palette.unwrap_or_else(|| {
                            let colors = *cx.global::<ByteCategoryColors>();
                            ByteCategoryPalette::from_colors(colors, cx.theme())
                        });
                        let desc_palette = {
                            let theme = cx.theme();
                            let key = (theme.mode, theme.background, theme.foreground);
                            if let Some((m, bg, fg, pal)) = cached_desc_palette.get() {
                                if m == key.0 && bg == key.1 && fg == key.2 {
                                    pal
                                } else {
                                    let pal = DescriptionPalette::from_theme(theme);
                                    cached_desc_palette.set(Some((key.0, key.1, key.2, pal)));
                                    pal
                                }
                            } else {
                                let pal = DescriptionPalette::from_theme(theme);
                                cached_desc_palette.set(Some((key.0, key.1, key.2, pal)));
                                pal
                            }
                        };
                        let active_cursor_offset = if insert_mode { insert_cursor_offset } else { cursor_offset };
                        let mut active_leaf_cache = cached_active_leaf_field.borrow_mut();
                        let active_leaf_field = active_leaf_cache.resolve(parse_result.as_ref(), active_cursor_offset);
                        let ascii_cursor_char_range = if active_column == EditColumn::Ascii && !insert_mode {
                            Some(encoding.char_range_at(doc.buffer.data(), cursor_offset))
                        } else {
                            None
                        };

                        // Pre-bucket visible comment bookmarks once per frame by display row
                        let visible_comment_bookmarks =
                            collect_visible_comment_refs(&bookmark_items, &line_starts, total_size, top_row, end_row, vp_start, vp_end);
                        let mut comment_cursor = 0usize;

                        let theme = cx.theme();
                        let theme_colors = RowThemeColors::from_theme(theme, is_focused);
                        let row_font = font(font_family.clone());
                        let parse_result_ptr = parse_result.as_ref().map(|p| Arc::as_ptr(p) as usize).unwrap_or(0);
                        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
                        let mut cache_borrow = row_cache.borrow_mut();
                        let mut scratch_borrow = paint_row_scratch.borrow_mut();
                        scratch_borrow.fade_runs.clear();
                        cache_borrow.validate(RowRenderCacheKey {
                            generation: doc.history.state_id(),
                            layout_version: doc.layout_version(),
                            bytes_per_row: line_starts.bytes_per_row(),
                            structure_layout_version,
                            is_parsing_structure,
                            show_offset,
                            show_ascii,
                            parse_result_ptr,
                            font_size,
                            font_family: &font_family,
                            radix,
                            group_size,
                            is_big_endian,
                            hex_case,
                            encoding,
                            theme_bg: theme_colors.bg_color_theme,
                            theme_fg: theme_colors.fg_color,
                        });

                        for (k, row_idx) in (top_row..end_row).enumerate() {
                            let row_y = bounds.top() + px((k + top_overscroll_rows) as f32 * ROW_HEIGHT);
                            if row_y >= bounds.bottom() {
                                break;
                            }
                            let Some((row_offset, row_next_offset)) = row_byte_range(&line_starts, row_idx, total_size) else {
                                continue;
                            };
                            let active_row_highlights = hl_sweep.advance_to(row_offset, row_next_offset);
                            while visible_comment_bookmarks.get(comment_cursor).is_some_and(|c| c.row_idx < row_idx) {
                                comment_cursor += 1;
                            }
                            let mut comment_end = comment_cursor;
                            while visible_comment_bookmarks.get(comment_end).is_some_and(|c| c.row_idx == row_idx) {
                                comment_end += 1;
                            }
                            let row_bookmark_comments = &visible_comment_bookmarks[comment_cursor..comment_end];
                            comment_cursor = comment_end;
                            let row_bounds = Bounds::new(point(bounds.left(), row_y), size(bounds.size.width, px(ROW_HEIGHT)));
                            paint_hex_row(
                                RowPaintParams {
                                    row_idx,
                                    bounds: row_bounds,
                                    doc: &doc,
                                    line_starts: &line_starts,
                                    parse_result: parse_result.as_deref(),
                                    is_parsing_structure,
                                    collapsed_structs: &collapsed_structs,
                                    encoding,
                                    radix,
                                    group_size,
                                    is_big_endian,
                                    hex_case,
                                    category_palette,
                                    cursor_offset,
                                    insert_cursor_offset,
                                    min_sel,
                                    max_sel,
                                    highlights: active_row_highlights,
                                    bookmark_comments: row_bookmark_comments,
                                    show_offset,
                                    show_ascii,
                                    ascii_col_width,
                                    ascii_scroll_x,
                                    is_focused,
                                    insert_mode,
                                    active_column,
                                    ascii_cursor_char_range: ascii_cursor_char_range.clone(),
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
                                    font: &row_font,
                                    font_size,
                                    desc_palette,
                                    active_leaf_field,
                                    has_folded_bookmarks,
                                    has_address_gaps,
                                    theme_colors: &theme_colors,
                                    row_cache: &mut cache_borrow,
                                    scratch: &mut scratch_borrow,
                                },
                                window,
                                cx,
                            );
                        }

                        flush_column_fade_runs(&scratch_borrow.fade_runs, theme_colors.bg_color_theme, window);

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
                                .styles(|_| crate::ui::components::scrollbar::common_scrollbar_styles(theme)),
                        ),
                    )
                    .into_any_element()
            } else {
                div().h(px(0.0)).into_any_element()
            })
            .context_menu({
                let focus_handle = self.focus_handle.clone();
                let editor = self.editor.clone();
                let view = view.clone();
                move |menu, window, cx| {
                    let (is_read_only, can_undo, can_redo, has_selection, can_copy) = {
                        let ed = editor.read(cx);
                        let can_copy = view.read(cx).copy_range(cx).is_some();
                        (ed.is_read_only(), ed.can_undo(), ed.can_redo(), ed.has_selection(), can_copy)
                    };
                    menu.action_context(focus_handle.clone())
                        .menu_with_disabled("Undo", Box::new(Undo), is_read_only || !can_undo)
                        .menu_with_disabled("Redo", Box::new(Redo), is_read_only || !can_redo)
                        .menu_with_disabled("Cut", Box::new(Cut), is_read_only || !has_selection)
                        .menu_with_disabled("Copy", Box::new(Copy), !can_copy)
                        .submenu("Copy As", window, cx, move |menu, _window, _cx| {
                            menu.menu_with_disabled("as Hex Dump", Box::new(CopyAsHexDump), !can_copy)
                                .menu_with_disabled("as C++ Array", Box::new(CopyAsCppArray), !can_copy)
                                .menu_with_disabled("as Hex Stream", Box::new(CopyAsHexStream), !can_copy)
                                .menu_with_disabled("as Hex with Spaces", Box::new(CopyAsHexSpaces), !can_copy)
                                .menu_with_disabled("as Printable Text", Box::new(CopyAsPrintableText), !can_copy)
                                .menu_with_disabled("as Base64", Box::new(CopyAsBase64), !can_copy)
                                .menu_with_disabled("as Escaped String", Box::new(CopyAsEscapedString), !can_copy)
                                .menu_with_disabled("as Binary", Box::new(CopyAsBinary), !can_copy)
                                .menu_with_disabled("as Rust Array", Box::new(CopyAsRustArray), !can_copy)
                                .menu_with_disabled("as JSON Array", Box::new(CopyAsJsonArray), !can_copy)
                        })
                        .menu_with_disabled("Paste", Box::new(Paste), is_read_only)
                        .menu_with_disabled("Fill Selection...", Box::new(FillSelection), is_read_only || !has_selection)
                        .separator()
                        .submenu("Scratchpad", window, cx, move |menu, _window, _cx| {
                            menu.menu_with_icon("as Offset", IconName::Hash, Box::new(crate::actions::InsertActiveOffsetOnly))
                                .menu_with_icon("as Hex Bytes", IconName::Binary, Box::new(crate::actions::InsertActiveHexBytes))
                                .menu_with_icon("as Text / String", IconName::FileText, Box::new(crate::actions::InsertActiveText))
                        })
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
                        .menu_with_disabled("Reset Breaks in Selection", Box::new(RemoveCustomBreaksInRange), !has_selection)
                        .menu("Reset Layout", Box::new(ClearAllCustomBreaks))
                }
            })
    }
}

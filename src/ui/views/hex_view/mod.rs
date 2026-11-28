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
mod render;

pub use actions::init;
#[cfg(test)]
pub use layout::ascii_byte_index_from_world_x;
pub use layout::{
    bounded_auto_fit_range, build_hex_text_source, calculate_scroll_top_for_range, can_chain_to_outer, make_hex_view_layout, weighted_text_width,
};
#[cfg(test)]
pub use layout::{hex_grid_width, hex_grid_x};
pub use paint::{CHEVRON_ICON_GAP, CHEVRON_ICON_SIZE, DescriptionPalette, calculate_description_indent_level};
pub use types::*;

use crate::app_state::InsertModeState;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::radix::{ByteCategoryColors, ByteColoring, ByteGroupSize, DisplayRadix, HexCase};

use crate::core::structure::{IndexedField, ParseResult};
use gpui_kit::component::ActiveTheme;
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
    hovered_desc_tooltip: Option<(usize, SharedString, Point<Pixels>)>,
    column_focus_flash: Option<EditColumn>,
    column_focus_flash_epoch: usize,
    _column_focus_flash_task: Task<()>,
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
            this.flash_active_column_header(cx);
        })
        .detach();
        cx.on_focus_out(&focus_handle, window, |this, _, _, cx| {
            this.cursor_blink.update(cx, |cursor, cx| {
                cursor.stop(cx);
            });
            this.clear_column_focus_flash();
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
            hovered_desc_tooltip: None,
            column_focus_flash: None,
            column_focus_flash_epoch: 0,
            _column_focus_flash_task: Task::ready(()),
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

    /// Briefly highlights the active Hex/ASCII column header after focus moves.
    pub(super) fn flash_active_column_header(&mut self, cx: &mut Context<Self>) {
        let is_struct_mode = {
            let ed = self.editor.read(cx);
            ed.structure.show_inline_structure_view && ed.parse_result().is_some()
        };
        if !self.show_ascii || is_struct_mode {
            self.clear_column_focus_flash();
            cx.notify();
            return;
        }
        self.column_focus_flash = Some(self.input.active_column());
        self.column_focus_flash_epoch = self.column_focus_flash_epoch.wrapping_add(1);
        let epoch = self.column_focus_flash_epoch;
        self._column_focus_flash_task = cx.spawn(async move |this, cx| {
            tokio::time::sleep(crate::ui::panels::FOCUS_FLASH_DURATION + std::time::Duration::from_millis(50)).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if this.column_focus_flash_epoch == epoch && this.column_focus_flash.is_some() {
                        this.column_focus_flash = None;
                        cx.notify();
                    }
                });
            }
        });
        cx.notify();
    }

    pub(super) fn clear_column_focus_flash(&mut self) {
        self.column_focus_flash = None;
        self.column_focus_flash_epoch = self.column_focus_flash_epoch.wrapping_add(1);
        self._column_focus_flash_task = Task::ready(());
    }

    pub(super) fn set_active_column_with_flash(&mut self, column: EditColumn, cx: &mut Context<Self>) {
        let changed = self.input.active_column() != column;
        self.input.set_active_column(column);
        if changed {
            self.flash_active_column_header(cx);
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

    #[inline]
    pub(super) fn row_height(&self) -> f32 {
        row_height_for_font_size(f32::from(self.font_size_prop))
    }

    #[inline]
    pub(super) fn header_height(&self) -> f32 {
        header_height_for_font_size(f32::from(self.font_size_prop))
    }

    #[inline]
    pub(super) fn ascii_cell_width(&self) -> f32 {
        ascii_cell_width_for_font_size(f32::from(self.font_size_prop))
    }

    fn default_ascii_col_width(&self, max_bytes_per_row: usize) -> f32 {
        let cell_w = self.ascii_cell_width();
        max_bytes_per_row.max(1) as f32 * cell_w + cell_w * 0.5
    }

    fn effective_ascii_col_width(&self, max_bytes_per_row: usize) -> f32 {
        if self.ascii_col_width > 0.0 {
            self.ascii_col_width
        } else {
            self.default_ascii_col_width(max_bytes_per_row)
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

    fn current_layout(&self, cx: &App) -> HexViewLayout {
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
                (self.default_ascii_col_width(max_bytes_per_row) - self.effective_ascii_col_width(max_bytes_per_row)).max(0.0)
            } else {
                0.0
            },
            fixed_column_width: if is_struct_mode || self.show_offset { self.address_col_width } else { 0.0 },
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
                let max_address = {
                    let editor = self.editor.read(cx);
                    editor
                        .document
                        .read()
                        .ok()
                        .map(|doc| doc.offset_to_address(doc.buffer.len().saturating_sub(1)))
                        .unwrap_or(0)
                };
                self.address_col_width = address_col_width_for_font_size(f32::from(self.font_size_prop), max_address);
            }
            ResizingColumn::Hex => {
                if self.hex_content_width > 0.0 {
                    self.hex_col_width = self.hex_content_width;
                }
            }
            ResizingColumn::Ascii => {
                let max_bytes_per_row = self.editor.read(cx).line_starts().max_bytes_per_row();
                self.ascii_col_width = self.default_ascii_col_width(max_bytes_per_row);
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
        self.hovered_desc_tooltip = None;
        self.cursor_reveal_pending = true;
        self.clamp_scroll_offsets(cx);
        cx.notify();
    }

    fn on_scroll(&mut self, event: &ScrollEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.hovered_desc_tooltip.is_some() {
            self.hovered_desc_tooltip = None;
        }
        let now = std::time::Instant::now();
        let row_height = self.row_height();
        let pixel_delta = event.delta.pixel_delta(px(row_height));
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
            let visible_rows = (list_h / row_height).floor() as usize;
            let max_top_row = total_rows.saturating_sub(visible_rows.max(1));

            if let Some(new_offset) = self.scroll.handle_wheel_vertical(delta_y, max_top_row, row_height) {
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
            let visible_rows = (list_h / row_height).floor() as usize;
            let max_top_row = total_rows.saturating_sub(visible_rows.max(1));

            if let Some(new_offset) = self.scroll.handle_wheel_vertical(delta_y, max_top_row, row_height) {
                cx.notify();
                cx.emit(HexViewEvent::Scrolled(new_offset));
            }
        }
    }

    fn update_scrollbar_drag(&mut self, current_y: f32, cx: &mut Context<Self>) {
        let total_rows = self.effective_total_rows(cx);
        let list_h = self.list_bounds.get().map(|b| f32::from(b.size.height)).unwrap_or(600.0);
        if let Some(new_row) = self.scroll.update_scrollbar_drag(current_y, total_rows, list_h, self.row_height()) {
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
        let new_font_size = font_size.into();
        if self.font_size_prop == new_font_size {
            return;
        }
        let old_font_px = f32::from(self.font_size_prop);
        let new_font_px = f32::from(new_font_size);
        let max_bytes_per_row = self.editor.read(cx).line_starts().max_bytes_per_row();
        let old_default_ascii = self.default_ascii_col_width(max_bytes_per_row);
        let old_default_addr = address_col_width_for_font_size(old_font_px, 0);

        let hex_was_auto = self.hex_col_width <= 0.0 || (self.hex_col_width - self.hex_content_width).abs() < 1.0;
        let ascii_was_auto = self.ascii_col_width <= 0.0 || (self.ascii_col_width - old_default_ascii).abs() < 1.0;
        let addr_was_auto = (self.address_col_width - old_default_addr).abs() < 1.0 || self.address_col_width <= old_default_addr;

        self.font_size_prop = new_font_size;
        self.hex_cell_width = 0.0;
        self.hex_content_width = 0.0;
        if hex_was_auto {
            self.hex_col_width = 0.0;
        }
        if ascii_was_auto {
            self.ascii_col_width = 0.0;
        }
        if addr_was_auto {
            self.address_col_width = address_col_width_for_font_size(new_font_px, 0);
        }
        self.cached_comment_content_width.set(None);
        self.clear_desc_content_width_cache();
        self.hovered_desc_tooltip = None;
        self.row_cache.borrow_mut().clear();
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
        let visible_rows = (list_h / self.row_height()).floor() as usize;
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
            (f32::from(bounds.size.height) / self.row_height()).ceil() as usize
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
        let visible_rows = (list_h / self.row_height()).floor() as usize;

        if self.scroll.scroll_to_row(row, total_rows, visible_rows) {
            self.hovered_desc_tooltip = None;
            cx.notify();
            cx.emit(HexViewEvent::Scrolled(self.scroll.scroll_offset));
        }
    }

    pub fn scroll_to_bottom_row(&mut self, row: usize, cx: &mut Context<Self>) {
        let list_h = self.list_bounds.get().map(|b| f32::from(b.size.height)).unwrap_or(600.0);
        let visible_rows = (list_h / self.row_height()).floor() as usize;
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
        let visible_rows = (list_h / self.row_height()).floor() as usize;
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

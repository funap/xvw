use gpui_kit::*;

pub const CONTEXT: &str = "HexView";

// HexView layout constants
pub const HEADER_HEIGHT: f32 = 28.0;
pub const ROW_HEIGHT: f32 = 22.0;
pub const SECTION_GAP: f32 = 16.0;

pub const ADDRESS_WIDTH: f32 = 80.0;
pub const DESC_WIDTH: f32 = 240.0;
pub const COMMENT_WIDTH: f32 = 300.0;
pub const VERTICAL_SCROLLBAR_WIDTH: f32 = 12.0;
pub const HORIZONTAL_SCROLLBAR_HEIGHT: f32 = 12.0;
pub const ASCII_CELL_WIDTH: f32 = 10.0;
pub const CURSOR_BORDER_WIDTH: f32 = 1.0;
pub const CURSOR_PADDING_X: f32 = 0.0;
pub const CURSOR_PADDING_Y: f32 = 0.0;
pub const MIN_ASCII_COLUMN_WIDTH: f32 = 80.0;
pub const AUTO_FIT_SCAN_BYTES: usize = 64 * 1024;
pub const AUTO_FIT_MAX_ITEMS: usize = 16 * 1024;
pub const AUTO_FIT_MAX_TEXT_CHARS: usize = 4096;

/// Returns the scaled row height in pixels for the given font size (22px at 14px font).
#[inline]
pub fn row_height_for_font_size(font_size: f32) -> f32 {
    (font_size * (ROW_HEIGHT / 14.0)).round().max(18.0)
}

/// Returns the scaled header height in pixels for the given font size (at least 28px).
#[inline]
pub fn header_height_for_font_size(font_size: f32) -> f32 {
    row_height_for_font_size(font_size).max(HEADER_HEIGHT)
}

/// Returns the scaled ASCII cell width in pixels for the given font size (10px at 14px font).
#[inline]
pub fn ascii_cell_width_for_font_size(font_size: f32) -> f32 {
    (font_size * (ASCII_CELL_WIDTH / 14.0)).round().max(7.0)
}

/// Returns the recommended Address column width in pixels for the given font size and maximum address.
#[inline]
pub fn address_col_width_for_font_size(font_size: f32, max_address: usize) -> f32 {
    let digits = if max_address <= 0xFFFF_FFFF {
        8
    } else {
        format!("{max_address:x}").len().max(8)
    };
    let char_w = font_size * 0.61;
    (digits as f32 * char_w + 11.0).ceil().max(ADDRESS_WIDTH)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum EditColumn {
    #[default]
    Hex,
    Ascii,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditTarget {
    Hex { offset: usize, nibble: u8 },
    Ascii { offset: usize },
}

impl EditTarget {
    pub fn offset(self) -> usize {
        match self {
            Self::Hex { offset, .. } | Self::Ascii { offset } => offset,
        }
    }

    pub fn column(self) -> EditColumn {
        match self {
            Self::Hex { .. } => EditColumn::Hex,
            Self::Ascii { .. } => EditColumn::Ascii,
        }
    }
}

#[allow(dead_code)]
pub enum HexViewEvent {
    Scrolled(usize),
    HorizontalScrolled { target: HorizontalScrollTarget, progress: f32 },
    SelectionChanged { start: Option<usize>, end: Option<usize> },
    CursorMoved(usize),
    EscapePressed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollColumn {
    Hex,
    Ascii,
    Description,
    Comment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorizontalScrollTarget {
    View,
    Column(ScrollColumn),
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ColumnLayout {
    pub start: f32,
    pub width: f32,
    pub inner_max: f32,
}

impl ColumnLayout {
    pub fn end(self) -> f32 {
        self.start + self.width
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct HexViewLayout {
    pub fixed_width: f32,
    pub content_width: f32,
    pub viewport_width: f32,
    pub outer_max: f32,
    pub hex: ColumnLayout,
    pub ascii: Option<ColumnLayout>,
    pub description: Option<ColumnLayout>,
    pub comment: ColumnLayout,
}

impl HexViewLayout {
    pub fn column(self, target: ScrollColumn) -> Option<ColumnLayout> {
        match target {
            ScrollColumn::Hex => Some(self.hex),
            ScrollColumn::Ascii => self.ascii,
            ScrollColumn::Description => self.description,
            ScrollColumn::Comment => Some(self.comment),
        }
    }

    pub fn max_offset(self, target: HorizontalScrollTarget) -> f32 {
        match target {
            HorizontalScrollTarget::View => self.outer_max,
            HorizontalScrollTarget::Column(column) => self.column(column).map(|col| col.inner_max).unwrap_or(0.0),
        }
    }

    pub fn progress(self, target: HorizontalScrollTarget, offset: f32) -> f32 {
        let max_offset = self.max_offset(target);
        if max_offset > 0.0 { (offset / max_offset).clamp(0.0, 1.0) } else { 0.0 }
    }

    pub fn column_at(self, relative_x: f32, outer_scroll_x: f32) -> Option<ScrollColumn> {
        if relative_x < self.fixed_width {
            return None;
        }
        let world_x = relative_x + outer_scroll_x;

        if world_x >= self.hex.start && world_x <= self.hex.end() {
            return Some(ScrollColumn::Hex);
        }
        if let Some(column) = self.ascii
            && world_x >= column.start
            && world_x <= column.end()
        {
            return Some(ScrollColumn::Ascii);
        }
        if let Some(column) = self.description
            && world_x >= column.start
            && world_x <= column.end()
        {
            return Some(ScrollColumn::Description);
        }
        if world_x >= self.comment.start && world_x <= self.comment.end() {
            return Some(ScrollColumn::Comment);
        }
        None
    }
}

#[derive(Clone, Debug)]
pub struct HexViewLayoutState {
    pub address_col_width: f32,
    pub hex_col_width: f32,
    pub desc_col_width: f32,
    pub comment_col_width: f32,
    pub ascii_col_width: f32,
    pub show_offset: bool,
    pub show_ascii: bool,
    pub show_header: bool,
    pub scroll_offset: usize,
    pub outer_scroll_x: f32,
    pub hex_scroll_x: f32,
    pub ascii_scroll_x: f32,
    pub desc_scroll_x: f32,
    pub comment_scroll_x: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResizingColumn {
    Address,
    Hex,
    Ascii,
    Description,
    Comment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScrollAxisLock {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, Debug)]
pub struct HexGroupInfo {
    pub chunk_start: usize,
    pub chunk_end: usize,
    #[allow(dead_code)]
    pub start_slot: usize,
    pub text_start: usize,
    pub text_end: usize,
}

#[derive(Clone, Debug)]
pub struct HexTextSource {
    pub text: SharedString,
    pub groups: Vec<HexGroupInfo>,
}

/// A mapped character in the ASCII column of the hex view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AsciiChar {
    /// A printable character decoded from the underlying encoding.
    Printable(char, usize),
    /// A non-printable byte or invalid character sequence, displayed as `.` placeholder.
    NonPrintable,
}

impl AsciiChar {
    /// Returns the character to display for this cell.
    #[inline]
    pub fn character(&self) -> char {
        match self {
            Self::Printable(c, _) => *c,
            Self::NonPrintable => '.',
        }
    }

    /// Returns the byte length consumed by this character.
    #[inline]
    #[allow(dead_code)]
    pub fn byte_len(&self) -> usize {
        match self {
            Self::Printable(_, len) => *len,
            Self::NonPrintable => 1,
        }
    }

    /// Returns `true` if this cell contains a decoded printable character.
    #[inline]
    pub fn is_printable(&self) -> bool {
        matches!(self, Self::Printable(..))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct LayoutInput {
    pub bounds_width: f32,
    pub is_struct_mode: bool,
    pub show_ascii: bool,
    pub ascii_col_width: f32,
    pub ascii_inner_max: f32,
    pub fixed_column_width: f32,
    pub hex_col_width: f32,
    pub desc_col_width: f32,
    pub comment_col_width: f32,
    pub hex_inner_max: f32,
    pub desc_inner_max: f32,
    pub comment_inner_max: f32,
    pub section_gap: f32,
    pub content_padding: f32,
    pub scrollbar_width: f32,
}

use crate::core::radix::{ByteCategoryColors, CategoryColor, HexByteCategory};
use gpui_kit::component::theme::Theme;

/// Resolves an individual category color choice into an `Hsla` color from the active theme.
#[inline]
pub fn resolve_category_color(choice: CategoryColor, theme: &Theme) -> Hsla {
    match choice {
        CategoryColor::Default => theme.foreground,
        CategoryColor::Muted => theme.muted_foreground,
        CategoryColor::Red => theme.red,
        CategoryColor::Green => theme.green,
        CategoryColor::Blue => theme.blue,
        CategoryColor::Yellow => theme.yellow,
        CategoryColor::Cyan => theme.cyan,
        CategoryColor::Magenta => theme.magenta,
    }
}

/// Color configuration for each byte category in `HexView`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ByteCategoryPalette {
    pub null: Hsla,
    pub ascii_printable: Hsla,
    pub ascii_whitespace: Hsla,
    pub ascii_control: Hsla,
    pub non_ascii: Hsla,
    pub max: Hsla,
    pub default: Hsla,
}

impl ByteCategoryPalette {
    /// Returns the color configured for the given byte category, or the default foreground color if `None`.
    #[inline]
    pub fn color_for(&self, category: Option<HexByteCategory>) -> Hsla {
        match category {
            Some(HexByteCategory::Null) => self.null,
            Some(HexByteCategory::AsciiPrintable) => self.ascii_printable,
            Some(HexByteCategory::AsciiWhitespace) => self.ascii_whitespace,
            Some(HexByteCategory::AsciiControl) => self.ascii_control,
            Some(HexByteCategory::NonAscii) => self.non_ascii,
            Some(HexByteCategory::Max) => self.max,
            None => self.default,
        }
    }

    /// Resolves a category palette from explicit category colors and the active theme.
    pub fn from_colors(colors: ByteCategoryColors, theme: &Theme) -> Self {
        Self {
            null: resolve_category_color(colors.null, theme),
            ascii_printable: resolve_category_color(colors.ascii_printable, theme),
            ascii_whitespace: resolve_category_color(colors.ascii_whitespace, theme),
            ascii_control: resolve_category_color(colors.ascii_control, theme),
            non_ascii: resolve_category_color(colors.non_ascii, theme),
            max: resolve_category_color(colors.max, theme),
            default: theme.foreground,
        }
    }
}

/// Pre-resolved theme colors hoisted out of the row painting loop.
#[derive(Clone, Copy, Debug)]
pub struct RowThemeColors {
    pub selection_bg: Hsla,
    pub active_row_bg: Hsla,
    pub caret_color: Hsla,
    pub muted_color: Hsla,
    pub fg_color: Hsla,
    pub accent_fg_color: Hsla,
    pub border_color: Hsla,
    pub border_line_color: Hsla,
    pub bg_color_theme: Hsla,
}

impl RowThemeColors {
    pub fn from_theme(theme: &Theme, is_focused: bool) -> Self {
        let is_dark = theme.mode.is_dark();
        let active_row_bg = Hsla {
            a: if is_dark { 0.35 } else { 0.50 },
            ..theme.muted
        };
        Self {
            selection_bg: crate::ui::views::hex_view::paint::selection_background_color(theme.selection, is_focused, is_dark),
            active_row_bg,
            caret_color: theme.caret,
            muted_color: theme.muted_foreground,
            fg_color: theme.foreground,
            accent_fg_color: theme.accent_foreground,
            border_color: theme.border,
            border_line_color: theme.border,
            bg_color_theme: theme.background,
        }
    }
}

/// Cached comment column content width covering a bounded byte scan window.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CachedCommentContentWidth {
    pub width: f32,
    pub scan_start: usize,
    pub scan_end: usize,
    pub total_size: usize,
    pub bookmark_version: u64,
    pub layout_version: usize,
    pub bytes_per_row: usize,
    pub top_row: usize,
    pub visible_rows: usize,
}

impl CachedCommentContentWidth {
    /// Returns true when the cached width remains valid for the exact same visible row state.
    #[inline]
    pub fn matches_row_state(&self, bookmark_version: u64, bytes_per_row: usize, top_row: usize, visible_rows: usize) -> bool {
        self.bookmark_version == bookmark_version && self.bytes_per_row == bytes_per_row && self.top_row == top_row && self.visible_rows == visible_rows
    }

    /// Returns true when `visible_start..visible_end` remains inside the scanned byte window
    /// and bookmark/layout inputs have not changed.
    #[inline]
    pub fn covers_viewport(
        &self,
        visible_start: usize,
        visible_end: usize,
        total_size: usize,
        bookmark_version: u64,
        layout_version: usize,
        bytes_per_row: usize,
    ) -> bool {
        self.bookmark_version == bookmark_version
            && self.layout_version == layout_version
            && self.bytes_per_row == bytes_per_row
            && self.total_size == total_size
            && visible_start >= self.scan_start
            && visible_end <= self.scan_end
    }
}

use crate::core::structure::{IndexedField, ParseResult};
use std::sync::{Arc, Weak};

/// Caches the deepest structure leaf field at `cursor_offset` for the current `ParseResult` snapshot.
#[derive(Default)]
pub struct CachedActiveLeafField {
    parse_result: Option<Weak<ParseResult>>,
    cursor_offset: usize,
    field: Option<IndexedField>,
}

impl CachedActiveLeafField {
    /// Resolves the deepest leaf field at `cursor_offset`, reusing the previous lookup across non-cursor frames.
    pub fn resolve(&mut self, parse_result: Option<&Arc<ParseResult>>, cursor_offset: usize) -> Option<&IndexedField> {
        let Some(pr) = parse_result else {
            self.parse_result = None;
            self.field = None;
            return None;
        };
        let same_parse = self
            .parse_result
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|cached| Arc::ptr_eq(&cached, pr));
        if !same_parse || self.cursor_offset != cursor_offset {
            self.parse_result = Some(Arc::downgrade(pr));
            self.cursor_offset = cursor_offset;
            self.field = pr.query_deepest_leaf_at(cursor_offset).map(|f| f.into_owned());
        }
        self.field.as_ref()
    }
}

/// Identifies which horizontally-scrollable column edge a coalesced fade gradient belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FadeEdgeKind {
    HexLeft = 0,
    HexRight = 1,
    AsciiLeft = 2,
    AsciiRight = 3,
    DescLeft = 4,
    DescRight = 5,
    CommentLeft = 6,
    CommentRight = 7,
}

impl FadeEdgeKind {
    pub const COUNT: usize = 8;
    pub const ALL: [Self; Self::COUNT] = [
        Self::HexLeft,
        Self::HexRight,
        Self::AsciiLeft,
        Self::AsciiRight,
        Self::DescLeft,
        Self::DescRight,
        Self::CommentLeft,
        Self::CommentRight,
    ];

    /// Gradient strip parameters `(steps, step_dx, strip_w, left_edge)` matching column styling.
    #[inline]
    pub const fn gradient_spec(self) -> (usize, f32, f32, bool) {
        match self {
            Self::HexLeft | Self::AsciiLeft | Self::DescLeft | Self::CommentLeft => (5, 3.2, 3.4, true),
            Self::HexRight | Self::AsciiRight => (6, 3.6, 3.8, false),
            Self::DescRight | Self::CommentRight => (5, 4.0, 4.2, false),
        }
    }
}

/// Contiguous vertical span of rows that share an edge fade strip and horizontal clip mask.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VerticalFadeRun {
    pub fade_start_x: f32,
    pub mask_left: f32,
    pub mask_width: f32,
    pub top_y: f32,
    pub bottom_y: f32,
    pub last_row_idx: usize,
}

/// Coalesces per-row horizontal column edge fades into vertical runs across contiguous rows.
#[derive(Default)]
pub struct ColumnFadeRuns {
    runs: [Vec<VerticalFadeRun>; FadeEdgeKind::COUNT],
}

impl ColumnFadeRuns {
    /// Clears accumulated fade runs while preserving allocated vector capacity across frames.
    #[inline]
    pub fn clear(&mut self) {
        for slot in &mut self.runs {
            slot.clear();
        }
    }

    /// Records a horizontal fade strip on `row_idx` (`top_y..bottom_y`), merging into the
    /// previous run when row index, vertical bounds, fade origin, and horizontal mask match.
    #[inline]
    pub fn push(&mut self, edge: FadeEdgeKind, row_idx: usize, fade_start_x: Pixels, mask_bounds: Bounds<Pixels>, top_y: f32, bottom_y: f32) {
        if bottom_y <= top_y {
            return;
        }
        let fade_x = f32::from(fade_start_x);
        let mask_left = f32::from(mask_bounds.origin.x);
        let mask_width = f32::from(mask_bounds.size.width);
        if mask_width <= 0.0 {
            return;
        }
        let list = &mut self.runs[edge as usize];
        if let Some(last) = list.last_mut()
            && last.last_row_idx.checked_add(1) == Some(row_idx)
            && (last.bottom_y - top_y).abs() < 0.01
            && (last.fade_start_x - fade_x).abs() < 0.01
            && (last.mask_left - mask_left).abs() < 0.01
            && (last.mask_width - mask_width).abs() < 0.01
        {
            last.bottom_y = bottom_y;
            last.last_row_idx = row_idx;
            return;
        }
        list.push(VerticalFadeRun {
            fade_start_x: fade_x,
            mask_left,
            mask_width,
            top_y,
            bottom_y,
            last_row_idx: row_idx,
        });
    }

    /// Returns the coalesced vertical runs for `edge`.
    #[inline]
    pub fn runs_for(&self, edge: FadeEdgeKind) -> &[VerticalFadeRun] {
        &self.runs[edge as usize]
    }
}

/// Reusable per-row scratch buffers shared across rows within a paint frame.
#[derive(Default)]
pub struct PaintRowScratch {
    pub group_visuals: Vec<(Option<Hsla>, bool, bool)>,
    pub group_text_colors: Vec<Hsla>,
    pub ascii_cell_colors: Vec<Hsla>,
    pub fade_runs: ColumnFadeRuns,
}

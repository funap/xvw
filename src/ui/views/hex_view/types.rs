use gpui_kit::*;

pub const CONTEXT: &str = "HexView";

// HexView layout constants
pub const HEADER_HEIGHT: f32 = 28.0;
pub const ROW_HEIGHT: f32 = 22.0;
pub const OFFSET_WIDTH: f32 = 80.0;
pub const SECTION_GAP: f32 = 16.0;

pub const ADDRESS_WIDTH: f32 = 80.0;
pub const DESC_WIDTH: f32 = 240.0;
pub const COMMENT_WIDTH: f32 = 300.0;
pub const VERTICAL_SCROLLBAR_WIDTH: f32 = 12.0;
pub const HORIZONTAL_SCROLLBAR_HEIGHT: f32 = 12.0;
pub const ASCII_CELL_WIDTH: f32 = 10.0;
pub const ASCII_EXTRA_WIDTH: f32 = ASCII_CELL_WIDTH * 0.5;
pub const CURSOR_BORDER_WIDTH: f32 = 1.0;
pub const CURSOR_PADDING_X: f32 = 0.0;
pub const CURSOR_PADDING_Y: f32 = 0.0;
pub const MIN_ASCII_COLUMN_WIDTH: f32 = 80.0;
pub const AUTO_FIT_SCAN_BYTES: usize = 64 * 1024;
pub const AUTO_FIT_MAX_ITEMS: usize = 16 * 1024;
pub const AUTO_FIT_MAX_TEXT_CHARS: usize = 4096;

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
pub struct AsciiCellEntry {
    pub cell_idx: usize,
    pub text_byte_start: usize,
    pub text_byte_end: usize,
    pub color: Hsla,
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

use crate::core::radix::{ByteCategory, ByteCategoryColors, CategoryColor};
use gpui_kit::component::theme::Theme;

/// Resolves an individual category color choice into an `Hsla` color from the active theme.
#[inline]
pub fn resolve_category_color(choice: CategoryColor, theme: &Theme) -> Hsla {
    match choice {
        CategoryColor::Default => theme.foreground,
        CategoryColor::Muted => theme.muted_foreground.opacity(0.5),
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
    pub fn color_for(&self, category: Option<ByteCategory>) -> Hsla {
        match category {
            Some(ByteCategory::Null) => self.null,
            Some(ByteCategory::AsciiPrintable) => self.ascii_printable,
            Some(ByteCategory::AsciiWhitespace) => self.ascii_whitespace,
            Some(ByteCategory::AsciiControl) => self.ascii_control,
            Some(ByteCategory::NonAscii) => self.non_ascii,
            Some(ByteCategory::Max) => self.max,
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

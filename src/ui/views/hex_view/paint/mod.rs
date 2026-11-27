mod color;
mod description;
mod glyphs;
mod row;
mod scrollbar;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub use color::highlight_color_for_range;
pub use color::selection_background_color;
#[cfg(test)]
pub use description::resolve_structure_row_flags;
pub use description::{CHEVRON_ICON_GAP, CHEVRON_ICON_SIZE, DescriptionPalette, calculate_description_indent_level};
pub use glyphs::flush_column_fade_runs;
#[cfg(test)]
pub use row::is_cursor_row;
pub use row::{RowPaintParams, paint_hex_row, row_byte_range};
pub use scrollbar::paint_scrollbar;

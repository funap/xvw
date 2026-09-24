//! Geometric calculations for 2D visual map rendering and interaction.
//!
//! Provides pure helpers for mapping between buffer offsets, pixel indices,
//! and 2D grid (row, col) coordinates without GPUI dependencies.

use super::VisualMapColorMode;
use std::cmp;

/// 8x8 planar tile location and in-tile pixel coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlanarTileCoord {
    pub tile_idx: usize,
    pub tile_row: usize,
    pub tile_col: usize,
    pub in_tile_x: usize,
    pub in_tile_y: usize,
    pub byte_offset: usize,
}

/// Helper for mapping between buffer offsets, pixel indices, and grid (row, col) coordinates.
pub struct VisualMapGeometry;

impl VisualMapGeometry {
    /// Calculates the row of a cursor offset given column count and color mode.
    #[inline]
    pub fn cursor_row(cursor_offset: usize, header_offset: usize, cols: usize, mode: VisualMapColorMode) -> usize {
        let (row, _) = Self::cursor_grid_pos(cursor_offset, header_offset, cols, mode);
        row
    }

    /// Calculates the (row, col) grid position and cell width multiplier of a cursor offset.
    #[inline]
    pub fn cursor_grid_pos(cursor_offset: usize, header_offset: usize, cols: usize, mode: VisualMapColorMode) -> (usize, usize) {
        let rel_cursor = cursor_offset.saturating_sub(header_offset);
        if mode == VisualMapColorMode::Planar4bpp {
            let tiles_per_row = (cols / 8).max(1);
            let tile_idx = rel_cursor / 32;
            let tile_byte = rel_cursor % 32;
            let in_tile_y = if tile_byte < 16 { tile_byte / 2 } else { (tile_byte - 16) / 2 };
            let tile_row = tile_idx / tiles_per_row;
            let tile_col = tile_idx % tiles_per_row;
            (tile_row * 8 + in_tile_y, tile_col * 8)
        } else {
            let cur_pix = mode.byte_offset_to_pixel(rel_cursor);
            let safe_cols = cols.max(1);
            (cur_pix / safe_cols, cur_pix % safe_cols)
        }
    }

    /// Computes planar tile coordinates from grid (row, col) coordinates.
    #[inline]
    pub fn planar_tile_at_grid(row: usize, col: usize, cols: usize, header_offset: usize) -> PlanarTileCoord {
        let tiles_per_row = (cols / 8).max(1);
        let tile_col = (col / 8).min(tiles_per_row.saturating_sub(1));
        let tile_row = row / 8;
        let in_tile_x = col % 8;
        let in_tile_y = row % 8;
        let tile_idx = tile_row * tiles_per_row + tile_col;
        let byte_offset = header_offset + tile_idx * 32 + 2 * in_tile_y;
        PlanarTileCoord {
            tile_idx,
            tile_row,
            tile_col,
            in_tile_x,
            in_tile_y,
            byte_offset,
        }
    }

    /// Converts relative pixel (x, y) coordinates within visual bounds to a clamped buffer offset.
    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn point_to_offset(
        rel_x: f32,
        rel_y: f32,
        pixel_size: f32,
        scroll_offset: usize,
        cols: usize,
        header_offset: usize,
        mode: VisualMapColorMode,
        buffer_len: usize,
    ) -> Option<usize> {
        if pixel_size <= 0.0 || cols == 0 {
            return None;
        }
        if buffer_len == 0 {
            return Some(0);
        }

        let col = ((rel_x / pixel_size) as usize).min(cols.saturating_sub(1));
        let row = (rel_y / pixel_size) as usize + scroll_offset;

        let offset = if mode == VisualMapColorMode::Planar4bpp {
            let tile_coord = Self::planar_tile_at_grid(row, col, cols, header_offset);
            tile_coord.byte_offset
        } else {
            let pixel_idx = row * cols + col;
            header_offset + mode.pixel_to_byte_offset(pixel_idx)
        };

        Some(offset.min(buffer_len.saturating_sub(1)))
    }

    /// Converts a hovered pixel position into a linear pixel index for highlight rendering.
    #[inline]
    pub fn hovered_to_linear_pixel(
        hov_offset: usize,
        header_offset: usize,
        cols: usize,
        mode: VisualMapColorMode,
        sub_idx: Option<usize>,
        tile_coord: Option<(usize, usize, usize)>, // (tile_idx, in_tile_x, in_tile_y)
    ) -> Option<usize> {
        if hov_offset < header_offset {
            return None;
        }
        let rel_offset = hov_offset - header_offset;
        Some(if let Some((tile_idx, in_tile_x, in_tile_y)) = tile_coord {
            let tiles_per_row = (cols / 8).max(1);
            let tile_col = tile_idx % tiles_per_row;
            let tile_row = tile_idx / tiles_per_row;
            let px_x = tile_col * 8 + in_tile_x;
            let px_y = tile_row * 8 + in_tile_y;
            px_y * cols + px_x
        } else if let Some(sub) = sub_idx {
            rel_offset.saturating_mul(mode.pixels_per_byte()) + sub
        } else {
            mode.byte_offset_to_pixel(rel_offset)
        })
    }

    /// Calculates visible selection row ranges `(c_start, c_count, sel_x_offset, r)`.
    #[inline]
    pub fn selection_row_range(
        sel_start_byte: usize,
        sel_end_byte: usize,
        cols: usize,
        row: usize,
        mode: VisualMapColorMode,
        total_pixels: usize,
    ) -> Option<(usize, usize)> {
        let sel_pix_start = mode.byte_offset_to_pixel(sel_start_byte);
        let sel_pix_end = if mode.pixels_per_byte() > 1 {
            mode.byte_offset_to_pixel(sel_end_byte)
        } else {
            sel_end_byte.div_ceil(mode.bytes_per_pixel())
        }
        .min(total_pixels);

        let row_pix_start = row * cols;
        let row_pix_end = (row + 1) * cols;

        let sel_row_start = cmp::max(sel_pix_start, row_pix_start);
        let sel_row_end = cmp::min(sel_pix_end, row_pix_end);

        if sel_row_start < sel_row_end {
            Some((sel_row_start - row_pix_start, sel_row_end - sel_row_start))
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_row_calculation() {
        let mode = VisualMapColorMode::Grayscale;
        assert_eq!(VisualMapGeometry::cursor_row(0, 0, 16, mode), 0);
        assert_eq!(VisualMapGeometry::cursor_row(15, 0, 16, mode), 0);
        assert_eq!(VisualMapGeometry::cursor_row(16, 0, 16, mode), 1);
        assert_eq!(VisualMapGeometry::cursor_row(32, 0, 16, mode), 2);

        // With header offset
        assert_eq!(VisualMapGeometry::cursor_row(20, 10, 16, mode), 0);
        assert_eq!(VisualMapGeometry::cursor_row(26, 10, 16, mode), 1);
    }

    #[test]
    fn test_cursor_row_planar_4bpp() {
        let mode = VisualMapColorMode::Planar4bpp;
        // Tile 0: bytes 0..32.
        // in_tile_byte 0..15 -> in_tile_y = in_tile_byte / 2
        // byte 0 -> in_tile_y = 0 -> row = 0
        assert_eq!(VisualMapGeometry::cursor_row(0, 0, 16, mode), 0);
        // byte 2 -> in_tile_y = 1 -> row = 1
        assert_eq!(VisualMapGeometry::cursor_row(2, 0, 16, mode), 1);
        // byte 14 -> in_tile_y = 7 -> row = 7
        assert_eq!(VisualMapGeometry::cursor_row(14, 0, 16, mode), 7);
        // byte 16 -> in_tile_y = 0 -> row = 0
        assert_eq!(VisualMapGeometry::cursor_row(16, 0, 16, mode), 0);

        // Tile 1 (with cols = 16 => tiles_per_row = 2)
        // tile_idx = 32 / 32 = 1. tile_row = 1 / 2 = 0.
        assert_eq!(VisualMapGeometry::cursor_row(32, 0, 16, mode), 0);
        // Tile 2: byte 64. tile_row = 2 / 2 = 1. row = 1 * 8 + 0 = 8.
        assert_eq!(VisualMapGeometry::cursor_row(64, 0, 16, mode), 8);
    }

    #[test]
    fn test_planar_tile_at_grid() {
        let coord = VisualMapGeometry::planar_tile_at_grid(0, 0, 16, 0);
        assert_eq!(coord.tile_idx, 0);
        assert_eq!(coord.tile_row, 0);
        assert_eq!(coord.tile_col, 0);
        assert_eq!(coord.in_tile_x, 0);
        assert_eq!(coord.in_tile_y, 0);
        assert_eq!(coord.byte_offset, 0);

        // Col 8 (Tile 1, x=0, y=0)
        let coord1 = VisualMapGeometry::planar_tile_at_grid(0, 8, 16, 0);
        assert_eq!(coord1.tile_idx, 1);
        assert_eq!(coord1.tile_row, 0);
        assert_eq!(coord1.tile_col, 1);
        assert_eq!(coord1.in_tile_x, 0);
        assert_eq!(coord1.byte_offset, 32);

        // Row 8 (Tile 2, x=0, y=0)
        let coord2 = VisualMapGeometry::planar_tile_at_grid(8, 0, 16, 0);
        assert_eq!(coord2.tile_idx, 2);
        assert_eq!(coord2.tile_row, 1);
        assert_eq!(coord2.tile_col, 0);
        assert_eq!(coord2.byte_offset, 64);
    }

    #[test]
    fn test_point_to_offset() {
        let mode = VisualMapColorMode::Grayscale;
        let offset = VisualMapGeometry::point_to_offset(10.0, 10.0, 2.0, 0, 16, 0, mode, 100);
        // rel_x = 10, pixel_size = 2 -> col = 5
        // rel_y = 10, pixel_size = 2 -> row = 5
        // 5 * 16 + 5 = 85
        assert_eq!(offset, Some(85));
    }

    #[test]
    fn test_selection_row_range() {
        let mode = VisualMapColorMode::Grayscale;
        // Selection: 10..25, cols = 16
        // Row 0 (0..16): overlap is 10..16 -> c_start = 10, c_count = 6
        assert_eq!(VisualMapGeometry::selection_row_range(10, 25, 16, 0, mode, 100), Some((10, 6)));
        // Row 1 (16..32): overlap is 16..25 -> c_start = 0, c_count = 9
        assert_eq!(VisualMapGeometry::selection_row_range(10, 25, 16, 1, mode, 100), Some((0, 9)));
        // Row 2 (32..48): no overlap
        assert_eq!(VisualMapGeometry::selection_row_range(10, 25, 16, 2, mode, 100), None);
    }
}

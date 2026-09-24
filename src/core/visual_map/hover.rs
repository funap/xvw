//! Pure hover inspection and pixel decoding logic for the visual map.
//!
//! Decodes byte, sub-byte, direct RGB, and planar bitplane values at a hovered
//! cell position without any UI dependencies.

use super::{VisualMapColorMode as ColorMode, decode_planar_4bpp_pixel};
use crate::core::visual_map::geometry::VisualMapGeometry;

/// Hover information for a visual map cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HoveredPixel {
    Byte(usize, u8),
    SubByte {
        offset: usize,
        sub_idx: usize,
        bit_val: u8,
        mode: ColorMode,
    },
    Rgb16 {
        offset: usize,
        raw_val: u16,
        b0: u8,
        b1: u8,
        mode: ColorMode,
    },
    Rgb24 {
        offset: usize,
        b0: u8,
        b1: u8,
        b2: u8,
        mode: ColorMode,
    },
    Rgb32 {
        offset: usize,
        b0: u8,
        b1: u8,
        b2: u8,
        b3: u8,
        mode: ColorMode,
    },
    PlanarTile {
        offset: usize,
        tile_idx: usize,
        in_tile_x: usize,
        in_tile_y: usize,
        color_idx: u8,
        bp0: u8,
        bp1: u8,
        bp2: u8,
        bp3: u8,
    },
}

impl HoveredPixel {
    /// Returns the primary byte offset in the buffer for this hovered pixel.
    #[inline]
    pub fn offset(&self) -> usize {
        match *self {
            Self::Byte(off, _) => off,
            Self::SubByte { offset, .. } => offset,
            Self::PlanarTile { offset, .. } => offset,
            Self::Rgb16 { offset, .. } | Self::Rgb24 { offset, .. } | Self::Rgb32 { offset, .. } => offset,
        }
    }
}

/// Decodes hover information from a buffer slice given (row, col) grid coordinates.
pub fn decode_hovered_pixel(
    buffer: &[u8],
    row: usize,
    col: usize,
    cols: usize,
    header_offset: usize,
    mode: ColorMode,
    is_big_endian: bool,
) -> Option<HoveredPixel> {
    let buffer_len = buffer.len();
    if buffer_len == 0 || col >= cols {
        return None;
    }

    if mode == ColorMode::Planar4bpp {
        let tile_coord = VisualMapGeometry::planar_tile_at_grid(row, col, cols, header_offset);
        let offset = tile_coord.byte_offset;
        let tile_byte_offset = header_offset + tile_coord.tile_idx * 32;

        if offset < buffer_len && tile_byte_offset < buffer_len {
            let tile_end = (tile_byte_offset + 32).min(buffer_len);
            let tile_slice = &buffer[tile_byte_offset..tile_end];
            let (color_idx, bp0, bp1, bp2, bp3) = decode_planar_4bpp_pixel(tile_slice, tile_coord.in_tile_x, tile_coord.in_tile_y);
            Some(HoveredPixel::PlanarTile {
                offset,
                tile_idx: tile_coord.tile_idx,
                in_tile_x: tile_coord.in_tile_x,
                in_tile_y: tile_coord.in_tile_y,
                color_idx,
                bp0,
                bp1,
                bp2,
                bp3,
            })
        } else {
            None
        }
    } else {
        let pixel_idx = row * cols + col;
        let offset = header_offset + mode.pixel_to_byte_offset(pixel_idx);
        let bpp = mode.bytes_per_pixel();

        if offset >= buffer_len {
            return None;
        }

        if mode.is_sub_byte() {
            let byte = buffer[offset];
            let ppb = mode.pixels_per_byte();
            let sub_idx = pixel_idx % ppb;
            let bit_val = match mode {
                ColorMode::Mono1bpp => (byte >> (7 - sub_idx)) & 1,
                ColorMode::Indexed2bpp => (byte >> (6 - sub_idx * 2)) & 0x03,
                ColorMode::Indexed4bpp => (byte >> (4 - sub_idx * 4)) & 0x0F,
                _ => 0,
            };
            Some(HoveredPixel::SubByte {
                offset,
                sub_idx,
                bit_val,
                mode,
            })
        } else if bpp == 2 {
            let b0 = buffer[offset];
            let b1 = buffer.get(offset + 1).copied().unwrap_or(0);
            let raw_val = if is_big_endian {
                u16::from_be_bytes([b0, b1])
            } else {
                u16::from_le_bytes([b0, b1])
            };
            Some(HoveredPixel::Rgb16 { offset, raw_val, b0, b1, mode })
        } else if bpp == 3 {
            let b0 = buffer[offset];
            let b1 = buffer.get(offset + 1).copied().unwrap_or(0);
            let b2 = buffer.get(offset + 2).copied().unwrap_or(0);
            Some(HoveredPixel::Rgb24 { offset, b0, b1, b2, mode })
        } else if bpp == 4 {
            let b0 = buffer[offset];
            let b1 = buffer.get(offset + 1).copied().unwrap_or(0);
            let b2 = buffer.get(offset + 2).copied().unwrap_or(0);
            let b3 = buffer.get(offset + 3).copied().unwrap_or(0);
            Some(HoveredPixel::Rgb32 { offset, b0, b1, b2, b3, mode })
        } else {
            let byte = buffer[offset];
            Some(HoveredPixel::Byte(offset, byte))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_hovered_pixel_byte() {
        let buffer = vec![0x10, 0x20, 0x30, 0x40];
        let hov = decode_hovered_pixel(&buffer, 0, 2, 4, 0, ColorMode::Grayscale, false);
        assert_eq!(hov, Some(HoveredPixel::Byte(2, 0x30)));
    }

    #[test]
    fn test_decode_hovered_pixel_sub_byte() {
        // Mono 1bpp: 0b10110000
        let buffer = vec![0b10110000];
        let hov0 = decode_hovered_pixel(&buffer, 0, 0, 8, 0, ColorMode::Mono1bpp, false);
        assert_eq!(
            hov0,
            Some(HoveredPixel::SubByte {
                offset: 0,
                sub_idx: 0,
                bit_val: 1,
                mode: ColorMode::Mono1bpp
            })
        );

        let hov1 = decode_hovered_pixel(&buffer, 0, 1, 8, 0, ColorMode::Mono1bpp, false);
        assert_eq!(
            hov1,
            Some(HoveredPixel::SubByte {
                offset: 0,
                sub_idx: 1,
                bit_val: 0,
                mode: ColorMode::Mono1bpp
            })
        );
    }

    #[test]
    fn test_decode_hovered_pixel_rgb16() {
        let buffer = vec![0x00, 0xF8]; // Red in LE
        let hov = decode_hovered_pixel(&buffer, 0, 0, 1, 0, ColorMode::Rgb565, false);
        assert_eq!(
            hov,
            Some(HoveredPixel::Rgb16 {
                offset: 0,
                raw_val: 0xF800,
                b0: 0x00,
                b1: 0xF8,
                mode: ColorMode::Rgb565
            })
        );
    }
}

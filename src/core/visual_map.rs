//! Pure data and rendering logic for visual map / byte distribution visualizations.
//!
//! Provides color categorization, color map LUT generation, and pixel buffer
//! rendering independent of any GUI framework.

use crate::core::color::RgbaColor;
use serde::{Deserialize, Serialize};
use std::cmp;

/// Visual display color modes for byte / word map rendering.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum VisualMapColorMode {
    Grayscale,
    DataCategory,
    Rainbow,
    Entropy,
    Rgb565,
    Rgb555,
    Rgb888,
    Bgr888,
    Rgba,
    Argb,
    Bgra,
}

impl VisualMapColorMode {
    /// Number of bytes consumed per displayed pixel.
    #[inline]
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            Self::Rgb565 | Self::Rgb555 => 2,
            Self::Rgb888 | Self::Bgr888 => 3,
            Self::Rgba | Self::Argb | Self::Bgra => 4,
            _ => 1,
        }
    }

    /// Whether this mode interprets data as direct color RGB/RGBA pixels.
    #[inline]
    pub fn is_rgb(self) -> bool {
        matches!(
            self,
            Self::Rgb565 | Self::Rgb555 | Self::Rgb888 | Self::Bgr888 | Self::Rgba | Self::Argb | Self::Bgra
        )
    }

    /// Whether this mode interprets data as 16-bit word values subject to endianness.
    #[inline]
    pub fn is_rgb_16(self) -> bool {
        matches!(self, Self::Rgb565 | Self::Rgb555)
    }
}

/// Categorization of byte values into semantic character and data groups.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ByteCategory {
    Null,
    Control,
    Space,
    Ascii,
    Extended,
}

impl ByteCategory {
    /// Categorizes a single byte.
    #[inline]
    pub fn of(byte: u8) -> Self {
        match byte {
            0 => ByteCategory::Null,
            1..=31 | 127 => ByteCategory::Control,
            32 => ByteCategory::Space,
            33..=126 => ByteCategory::Ascii,
            _ => ByteCategory::Extended,
        }
    }

    /// Returns human-readable label for this category.
    pub fn label(self) -> &'static str {
        match self {
            ByteCategory::Null => "Null (00)",
            ByteCategory::Control => "Control",
            ByteCategory::Space => "Space (20)",
            ByteCategory::Ascii => "ASCII",
            ByteCategory::Extended => "Extended",
        }
    }
}

/// Color palette (in BGRA `[b, g, r, a]` format) for the 5 byte categories.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CategoryPalette {
    pub null: [u8; 4],
    pub control: [u8; 4],
    pub space: [u8; 4],
    pub ascii: [u8; 4],
    pub extended: [u8; 4],
}

impl Default for CategoryPalette {
    fn default() -> Self {
        Self {
            null: [120, 120, 120, 46],      // muted (dim)
            control: [40, 40, 220, 191],    // red-ish
            space: [220, 140, 40, 140],     // blue-ish
            ascii: [40, 200, 40, 217],      // green-ish
            extended: [200, 100, 180, 204], // accent/purple
        }
    }
}

/// Generates a 256-entry BGRA lookup table for `VisualMapColorMode::Grayscale`.
pub fn grayscale_bgra_lut() -> [[u8; 4]; 256] {
    let mut lut = [[0u8; 4]; 256];
    for byte in 0..=255 {
        let val = byte as f32 / 255.0;
        let lum = ((val * 0.8 + 0.1) * 255.0).clamp(0.0, 255.0) as u8;
        lut[byte as usize] = [lum, lum, lum, 255];
    }
    lut
}

/// Generates a 256-entry BGRA lookup table for `VisualMapColorMode::Rainbow`.
pub fn rainbow_bgra_lut() -> [[u8; 4]; 256] {
    let mut lut = [[0u8; 4]; 256];
    for byte in 0..=255 {
        let val = byte as f32 / 255.0;
        let rgba = RgbaColor::from_hsla_f32(val, 0.8, 0.5, 1.0);
        lut[byte as usize] = [rgba.b, rgba.g, rgba.r, rgba.a];
    }
    lut
}

/// Generates a 256-entry BGRA lookup table for `VisualMapColorMode::DataCategory` using the given palette.
pub fn category_bgra_lut(palette: &CategoryPalette) -> [[u8; 4]; 256] {
    let mut lut = [[0u8; 4]; 256];
    for byte in 0..=255 {
        lut[byte as usize] = match ByteCategory::of(byte) {
            ByteCategory::Null => palette.null,
            ByteCategory::Control => palette.control,
            ByteCategory::Space => palette.space,
            ByteCategory::Ascii => palette.ascii,
            ByteCategory::Extended => palette.extended,
        };
    }
    lut
}

/// Converts a 16-bit RGB 565 value into 8-bit RGB components `(r, g, b)`.
///
/// Bit layout (MSB to LSB):
/// - Bits 15..=11: Red (5 bits)
/// - Bits 10..=5:  Green (6 bits)
/// - Bits 4..=0:   Blue (5 bits)
#[inline]
pub fn rgb565_to_rgb888(val: u16) -> (u8, u8, u8) {
    let r5 = ((val >> 11) & 0x1F) as u8;
    let g6 = ((val >> 5) & 0x3F) as u8;
    let b5 = (val & 0x1F) as u8;
    let r8 = (r5 << 3) | (r5 >> 2);
    let g8 = (g6 << 2) | (g6 >> 4);
    let b8 = (b5 << 3) | (b5 >> 2);
    (r8, g8, b8)
}

/// Converts a 16-bit RGB 565 value into BGRA format `[b, g, r, a]` with full opacity (255).
#[inline]
pub fn rgb565_to_bgra(val: u16) -> [u8; 4] {
    let (r, g, b) = rgb565_to_rgb888(val);
    [b, g, r, 255]
}

/// Converts a 16-bit RGB 555 value into 8-bit RGB components `(r, g, b)`.
///
/// Bit layout (MSB to LSB):
/// - Bit 15:       Unused / ignored
/// - Bits 14..=10: Red (5 bits)
/// - Bits 9..=5:   Green (5 bits)
/// - Bits 4..=0:   Blue (5 bits)
#[inline]
pub fn rgb555_to_rgb888(val: u16) -> (u8, u8, u8) {
    let r5 = ((val >> 10) & 0x1F) as u8;
    let g5 = ((val >> 5) & 0x1F) as u8;
    let b5 = (val & 0x1F) as u8;
    let r8 = (r5 << 3) | (r5 >> 2);
    let g8 = (g5 << 3) | (g5 >> 2);
    let b8 = (b5 << 3) | (b5 >> 2);
    (r8, g8, b8)
}

/// Converts a 16-bit RGB 555 value into BGRA format `[b, g, r, a]` with full opacity (255).
#[inline]
pub fn rgb555_to_bgra(val: u16) -> [u8; 4] {
    let (r, g, b) = rgb555_to_rgb888(val);
    [b, g, r, 255]
}

/// Parameters for rendering a visual map pixel buffer.
#[derive(Clone, Debug)]
pub struct VisualMapRenderParams {
    pub cols: usize,
    pub start_row: usize,
    pub visible_rows: usize,
    pub max_visible_cols: usize,
    pub cell_width: usize,
    pub cell_height: usize,
    pub physical_width: usize,
    pub physical_height: usize,
    pub color_mode: VisualMapColorMode,
    pub entropy_window: usize,
    pub custom_lut: Option<[[u8; 4]; 256]>,
    pub is_big_endian: bool,
}

/// Renders a raw BGRA pixel buffer (`Vec<u8>`) from binary data according to the given parameters.
pub fn render_visual_map_bgra(buffer: &[u8], params: &VisualMapRenderParams) -> Vec<u8> {
    let buffer_len = buffer.len();
    let physical_width = params.physical_width;
    let physical_height = params.physical_height;

    if buffer_len == 0 || physical_width == 0 || physical_height == 0 || params.cols == 0 {
        return Vec::new();
    }

    let bpp = params.color_mode.bytes_per_pixel();
    let total_pixels = buffer_len.div_ceil(bpp);
    let total_rows = total_pixels.div_ceil(params.cols);
    let start_row = params.start_row;
    let end_row = (start_row + params.visible_rows).min(total_rows);

    let mut pixels = vec![0u8; physical_width * physical_height * 4];

    if params.color_mode.is_rgb() {
        for r in start_row..end_row {
            let row_y = r - start_row;
            let row_pixel_start = r * params.cols;
            let chunk_len = cmp::min(params.cols, total_pixels.saturating_sub(row_pixel_start));
            let chunk_len = cmp::min(chunk_len, params.max_visible_cols);
            if chunk_len == 0 {
                break;
            }

            for c in 0..chunk_len {
                let pixel_idx = row_pixel_start + c;
                let byte_offset = pixel_idx * bpp;

                let color = match params.color_mode {
                    VisualMapColorMode::Rgb565 | VisualMapColorMode::Rgb555 => {
                        let val = if byte_offset + 1 < buffer_len {
                            let b0 = buffer[byte_offset];
                            let b1 = buffer[byte_offset + 1];
                            if params.is_big_endian {
                                u16::from_be_bytes([b0, b1])
                            } else {
                                u16::from_le_bytes([b0, b1])
                            }
                        } else if byte_offset < buffer_len {
                            let b0 = buffer[byte_offset];
                            if params.is_big_endian { (b0 as u16) << 8 } else { b0 as u16 }
                        } else {
                            0
                        };
                        if params.color_mode == VisualMapColorMode::Rgb565 {
                            rgb565_to_bgra(val)
                        } else {
                            rgb555_to_bgra(val)
                        }
                    }
                    VisualMapColorMode::Rgb888 => {
                        let b0 = if byte_offset < buffer_len { buffer[byte_offset] } else { 0 };
                        let b1 = if byte_offset + 1 < buffer_len { buffer[byte_offset + 1] } else { 0 };
                        let b2 = if byte_offset + 2 < buffer_len { buffer[byte_offset + 2] } else { 0 };
                        [b2, b1, b0, 255]
                    }
                    VisualMapColorMode::Bgr888 => {
                        let b0 = if byte_offset < buffer_len { buffer[byte_offset] } else { 0 };
                        let b1 = if byte_offset + 1 < buffer_len { buffer[byte_offset + 1] } else { 0 };
                        let b2 = if byte_offset + 2 < buffer_len { buffer[byte_offset + 2] } else { 0 };
                        [b0, b1, b2, 255]
                    }
                    VisualMapColorMode::Rgba => {
                        let b0 = if byte_offset < buffer_len { buffer[byte_offset] } else { 0 };
                        let b1 = if byte_offset + 1 < buffer_len { buffer[byte_offset + 1] } else { 0 };
                        let b2 = if byte_offset + 2 < buffer_len { buffer[byte_offset + 2] } else { 0 };
                        let b3 = if byte_offset + 3 < buffer_len { buffer[byte_offset + 3] } else { 255 };
                        [b2, b1, b0, b3]
                    }
                    VisualMapColorMode::Argb => {
                        let b0 = if byte_offset < buffer_len { buffer[byte_offset] } else { 255 };
                        let b1 = if byte_offset + 1 < buffer_len { buffer[byte_offset + 1] } else { 0 };
                        let b2 = if byte_offset + 2 < buffer_len { buffer[byte_offset + 2] } else { 0 };
                        let b3 = if byte_offset + 3 < buffer_len { buffer[byte_offset + 3] } else { 0 };
                        [b3, b2, b1, b0]
                    }
                    VisualMapColorMode::Bgra => {
                        let b0 = if byte_offset < buffer_len { buffer[byte_offset] } else { 0 };
                        let b1 = if byte_offset + 1 < buffer_len { buffer[byte_offset + 1] } else { 0 };
                        let b2 = if byte_offset + 2 < buffer_len { buffer[byte_offset + 2] } else { 0 };
                        let b3 = if byte_offset + 3 < buffer_len { buffer[byte_offset + 3] } else { 255 };
                        [b0, b1, b2, b3]
                    }
                    _ => unreachable!(),
                };

                blit_cell(&mut pixels, row_y, c, params, color);
            }
        }
    } else if params.color_mode == VisualMapColorMode::Entropy {
        let visible_start_offset = start_row * params.cols;
        let visible_end_offset = cmp::min(buffer_len, end_row * params.cols);

        let entropies = crate::core::entropy::compute_sliding_entropy(buffer, visible_start_offset, visible_end_offset, params.entropy_window);
        let lut = crate::core::entropy::entropy_bgra_lut();

        for r in start_row..end_row {
            let row_y = r - start_row;
            let row_offset = r * params.cols;
            let chunk_len = cmp::min(params.cols, buffer_len.saturating_sub(row_offset));
            let chunk_len = cmp::min(chunk_len, params.max_visible_cols);
            if chunk_len == 0 {
                break;
            }

            for c in 0..chunk_len {
                let byte_idx = row_offset + c;
                let color = if byte_idx >= visible_start_offset && byte_idx < visible_end_offset && (byte_idx - visible_start_offset) < entropies.len() {
                    let norm = entropies[byte_idx - visible_start_offset];
                    let lut_idx = crate::core::entropy::normalized_to_lut_index(norm);
                    lut[lut_idx]
                } else {
                    [0, 0, 0, 255]
                };

                blit_cell(&mut pixels, row_y, c, params, color);
            }
        }
    } else {
        let bgra_lut = if let Some(custom) = params.custom_lut {
            custom
        } else {
            match params.color_mode {
                VisualMapColorMode::Grayscale => grayscale_bgra_lut(),
                VisualMapColorMode::Rainbow => rainbow_bgra_lut(),
                VisualMapColorMode::DataCategory => category_bgra_lut(&CategoryPalette::default()),
                _ => unreachable!(),
            }
        };

        for r in start_row..end_row {
            let row_y = r - start_row;
            let row_offset = r * params.cols;
            let chunk_len = cmp::min(params.cols, buffer_len.saturating_sub(row_offset));
            let chunk_len = cmp::min(chunk_len, params.max_visible_cols);
            if chunk_len == 0 {
                break;
            }

            let chunk_end = (row_offset + chunk_len).min(buffer_len);
            let chunk = &buffer[row_offset..chunk_end];

            for (c, &byte) in chunk.iter().enumerate() {
                let color = bgra_lut[byte as usize];
                blit_cell(&mut pixels, row_y, c, params, color);
            }
        }
    }

    pixels
}

#[inline]
fn blit_cell(pixels: &mut [u8], row_y: usize, col_x: usize, params: &VisualMapRenderParams, color: [u8; 4]) {
    let cell_height = params.cell_height;
    let cell_width = params.cell_width;
    let physical_height = params.physical_height;
    let physical_width = params.physical_width;

    for dy in 0..cell_height {
        let py = row_y * cell_height + dy;
        if py >= physical_height {
            continue;
        }
        for dx in 0..cell_width {
            let px_idx = col_x * cell_width + dx;
            if px_idx >= physical_width {
                continue;
            }
            let pixel_offset = (py * physical_width + px_idx) * 4;
            pixels[pixel_offset] = color[0];
            pixels[pixel_offset + 1] = color[1];
            pixels[pixel_offset + 2] = color[2];
            pixels[pixel_offset + 3] = color[3];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_byte_category_mapping() {
        assert_eq!(ByteCategory::of(0), ByteCategory::Null);
        assert_eq!(ByteCategory::of(1), ByteCategory::Control);
        assert_eq!(ByteCategory::of(31), ByteCategory::Control);
        assert_eq!(ByteCategory::of(127), ByteCategory::Control);
        assert_eq!(ByteCategory::of(32), ByteCategory::Space);
        assert_eq!(ByteCategory::of(33), ByteCategory::Ascii);
        assert_eq!(ByteCategory::of(b'A'), ByteCategory::Ascii);
        assert_eq!(ByteCategory::of(126), ByteCategory::Ascii);
        assert_eq!(ByteCategory::of(128), ByteCategory::Extended);
        assert_eq!(ByteCategory::of(255), ByteCategory::Extended);
    }

    #[test]
    fn test_grayscale_lut() {
        let lut = grayscale_bgra_lut();
        assert_eq!(lut.len(), 256);
        // Alpha is 255
        assert_eq!(lut[0][3], 255);
        assert_eq!(lut[255][3], 255);
        // R=G=B
        assert_eq!(lut[128][0], lut[128][1]);
        assert_eq!(lut[128][1], lut[128][2]);
    }

    #[test]
    fn test_rainbow_lut() {
        let lut = rainbow_bgra_lut();
        assert_eq!(lut.len(), 256);
        assert_eq!(lut[0][3], 255);
        assert_eq!(lut[255][3], 255);
    }

    #[test]
    fn test_category_lut() {
        let palette = CategoryPalette {
            null: [1, 2, 3, 4],
            control: [5, 6, 7, 8],
            space: [9, 10, 11, 12],
            ascii: [13, 14, 15, 16],
            extended: [17, 18, 19, 20],
        };
        let lut = category_bgra_lut(&palette);
        assert_eq!(lut[0], [1, 2, 3, 4]);
        assert_eq!(lut[10], [5, 6, 7, 8]);
        assert_eq!(lut[32], [9, 10, 11, 12]);
        assert_eq!(lut[b'x' as usize], [13, 14, 15, 16]);
        assert_eq!(lut[200], [17, 18, 19, 20]);
    }

    #[test]
    fn test_render_empty_buffer() {
        let params = VisualMapRenderParams {
            cols: 16,
            start_row: 0,
            visible_rows: 10,
            max_visible_cols: 16,
            cell_width: 2,
            cell_height: 2,
            physical_width: 32,
            physical_height: 20,
            color_mode: VisualMapColorMode::Grayscale,
            entropy_window: 64,
            custom_lut: None,
            is_big_endian: false,
        };
        let pixels = render_visual_map_bgra(&[], &params);
        assert!(pixels.is_empty());
    }

    #[test]
    fn test_render_visual_map_pixels() {
        let data = vec![0u8, 32, b'A', 255];
        let params = VisualMapRenderParams {
            cols: 2,
            start_row: 0,
            visible_rows: 2,
            max_visible_cols: 2,
            cell_width: 1,
            cell_height: 1,
            physical_width: 2,
            physical_height: 2,
            color_mode: VisualMapColorMode::Grayscale,
            entropy_window: 64,
            custom_lut: None,
            is_big_endian: false,
        };
        let pixels = render_visual_map_bgra(&data, &params);
        assert_eq!(pixels.len(), 2 * 2 * 4);
    }

    #[test]
    fn test_rgb565_conversions() {
        // Red: 0xF800 (bits 15..11 = 31)
        assert_eq!(rgb565_to_rgb888(0xF800), (255, 0, 0));
        assert_eq!(rgb565_to_bgra(0xF800), [0, 0, 255, 255]);

        // Green: 0x07E0 (bits 10..5 = 63)
        assert_eq!(rgb565_to_rgb888(0x07E0), (0, 255, 0));
        assert_eq!(rgb565_to_bgra(0x07E0), [0, 255, 0, 255]);

        // Blue: 0x001F (bits 4..0 = 31)
        assert_eq!(rgb565_to_rgb888(0x001F), (0, 0, 255));
        assert_eq!(rgb565_to_bgra(0x001F), [255, 0, 0, 255]);

        // White: 0xFFFF
        assert_eq!(rgb565_to_rgb888(0xFFFF), (255, 255, 255));
        assert_eq!(rgb565_to_bgra(0xFFFF), [255, 255, 255, 255]);

        // Black: 0x0000
        assert_eq!(rgb565_to_rgb888(0x0000), (0, 0, 0));
        assert_eq!(rgb565_to_bgra(0x0000), [0, 0, 0, 255]);
    }

    #[test]
    fn test_rgb555_conversions() {
        // Red: 0x7C00 (bits 14..10 = 31, bit 15 = 0)
        assert_eq!(rgb555_to_rgb888(0x7C00), (255, 0, 0));
        assert_eq!(rgb555_to_bgra(0x7C00), [0, 0, 255, 255]);

        // Red with bit 15 set: 0xFC00 -> should ignore bit 15
        assert_eq!(rgb555_to_rgb888(0xFC00), (255, 0, 0));

        // Green: 0x03E0 (bits 9..5 = 31)
        assert_eq!(rgb555_to_rgb888(0x03E0), (0, 255, 0));
        assert_eq!(rgb555_to_bgra(0x03E0), [0, 255, 0, 255]);

        // Blue: 0x001F (bits 4..0 = 31)
        assert_eq!(rgb555_to_rgb888(0x001F), (0, 0, 255));
        assert_eq!(rgb555_to_bgra(0x001F), [255, 0, 0, 255]);

        // White: 0x7FFF
        assert_eq!(rgb555_to_rgb888(0x7FFF), (255, 255, 255));
        assert_eq!(rgb555_to_bgra(0x7FFF), [255, 255, 255, 255]);

        // Black: 0x0000
        assert_eq!(rgb555_to_rgb888(0x0000), (0, 0, 0));
        assert_eq!(rgb555_to_bgra(0x0000), [0, 0, 0, 255]);
    }

    #[test]
    fn test_render_visual_map_rgb565_le_and_be() {
        // Pixel 0: Red 0xF800, Pixel 1: Blue 0x001F
        // LE bytes: [0x00, 0xF8, 0x1F, 0x00]
        let data_le = vec![0x00, 0xF8, 0x1F, 0x00];
        let params_le = VisualMapRenderParams {
            cols: 2,
            start_row: 0,
            visible_rows: 1,
            max_visible_cols: 2,
            cell_width: 1,
            cell_height: 1,
            physical_width: 2,
            physical_height: 1,
            color_mode: VisualMapColorMode::Rgb565,
            entropy_window: 64,
            custom_lut: None,
            is_big_endian: false,
        };
        let pixels_le = render_visual_map_bgra(&data_le, &params_le);
        assert_eq!(pixels_le.len(), 8);
        // Pixel 0 BGRA: Red [0, 0, 255, 255]
        assert_eq!(&pixels_le[0..4], &[0, 0, 255, 255]);
        // Pixel 1 BGRA: Blue [255, 0, 0, 255]
        assert_eq!(&pixels_le[4..8], &[255, 0, 0, 255]);

        // BE bytes: [0xF8, 0x00, 0x00, 0x1F]
        let data_be = vec![0xF8, 0x00, 0x00, 0x1F];
        let mut params_be = params_le;
        params_be.is_big_endian = true;
        let pixels_be = render_visual_map_bgra(&data_be, &params_be);
        assert_eq!(&pixels_be[0..4], &[0, 0, 255, 255]);
        assert_eq!(&pixels_be[4..8], &[255, 0, 0, 255]);
    }

    #[test]
    fn test_render_visual_map_rgb555_odd_bytes() {
        // 3 bytes: 1 full pixel + 1 partial pixel (should not panic)
        // Pixel 0: Green 0x03E0 in LE -> [0xE0, 0x03]
        // Trailing byte: 0x1F
        let data = vec![0xE0, 0x03, 0x1F];
        let params = VisualMapRenderParams {
            cols: 2,
            start_row: 0,
            visible_rows: 1,
            max_visible_cols: 2,
            cell_width: 1,
            cell_height: 1,
            physical_width: 2,
            physical_height: 1,
            color_mode: VisualMapColorMode::Rgb555,
            entropy_window: 64,
            custom_lut: None,
            is_big_endian: false,
        };
        let pixels = render_visual_map_bgra(&data, &params);
        assert_eq!(pixels.len(), 8);
        // Pixel 0 BGRA: Green [0, 255, 0, 255]
        assert_eq!(&pixels[0..4], &[0, 255, 0, 255]);
    }

    #[test]
    fn test_render_visual_map_rgb888_and_bgr888() {
        let data = vec![255, 0, 0, 0, 0, 255];
        let mut params = VisualMapRenderParams {
            cols: 2,
            start_row: 0,
            visible_rows: 1,
            max_visible_cols: 2,
            cell_width: 1,
            cell_height: 1,
            physical_width: 2,
            physical_height: 1,
            color_mode: VisualMapColorMode::Rgb888,
            entropy_window: 64,
            custom_lut: None,
            is_big_endian: false,
        };
        let pixels = render_visual_map_bgra(&data, &params);
        assert_eq!(pixels.len(), 8);
        // Pixel 0 (R=255, G=0, B=0) -> BGRA [0, 0, 255, 255]
        assert_eq!(&pixels[0..4], &[0, 0, 255, 255]);
        // Pixel 1 (R=0, G=0, B=255) -> BGRA [255, 0, 0, 255]
        assert_eq!(&pixels[4..8], &[255, 0, 0, 255]);

        params.color_mode = VisualMapColorMode::Bgr888;
        let pixels = render_visual_map_bgra(&data, &params);
        // Pixel 0 in BGR: B=255, G=0, R=0 -> BGRA [255, 0, 0, 255]
        assert_eq!(&pixels[0..4], &[255, 0, 0, 255]);
        // Pixel 1 in BGR: B=0, G=0, R=255 -> BGRA [0, 0, 255, 255]
        assert_eq!(&pixels[4..8], &[0, 0, 255, 255]);

        // Boundary trailing test (4 bytes: 1 full 3-byte pixel + 1 partial)
        let trailing_data = vec![255, 128, 64, 10];
        let pixels_trailing = render_visual_map_bgra(&trailing_data, &params);
        assert_eq!(pixels_trailing.len(), 8);
    }

    #[test]
    fn test_render_visual_map_32bit_modes() {
        // RGBA test: [R=255, G=128, B=64, A=200]
        let data_rgba = vec![255, 128, 64, 200];
        let mut params = VisualMapRenderParams {
            cols: 1,
            start_row: 0,
            visible_rows: 1,
            max_visible_cols: 1,
            cell_width: 1,
            cell_height: 1,
            physical_width: 1,
            physical_height: 1,
            color_mode: VisualMapColorMode::Rgba,
            entropy_window: 64,
            custom_lut: None,
            is_big_endian: false,
        };
        let pixels = render_visual_map_bgra(&data_rgba, &params);
        assert_eq!(&pixels[0..4], &[64, 128, 255, 200]);

        // ARGB test: [A=200, R=255, G=128, B=64]
        let data_argb = vec![200, 255, 128, 64];
        params.color_mode = VisualMapColorMode::Argb;
        let pixels = render_visual_map_bgra(&data_argb, &params);
        assert_eq!(&pixels[0..4], &[64, 128, 255, 200]);

        // BGRA test: [B=64, G=128, R=255, A=200]
        let data_bgra = vec![64, 128, 255, 200];
        params.color_mode = VisualMapColorMode::Bgra;
        let pixels = render_visual_map_bgra(&data_bgra, &params);
        assert_eq!(&pixels[0..4], &[64, 128, 255, 200]);
    }
}

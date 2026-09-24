//! Cache key and container for pre-rendered visual map raster images.

use crate::core::visual_map::VisualMapColorMode as ColorMode;
use gpui_kit::RenderImage;
use std::sync::Arc;

/// Type-safe cache key for rendered visual map GPU images.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VisualMapCacheKey {
    pub cols: usize,
    pub pixel_size: usize,
    pub scroll_offset: usize,
    pub color_mode: ColorMode,
    pub entropy_window: usize,
    pub active_len: usize,
    pub state_id: usize,
    pub width_bits: u32,
    pub height_bits: u32,
    pub scale_factor_bits: u32,
    pub is_big_endian: bool,
    pub header_offset: usize,
}

impl VisualMapCacheKey {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        cols: usize,
        pixel_size: usize,
        scroll_offset: usize,
        color_mode: ColorMode,
        entropy_window: usize,
        active_len: usize,
        state_id: usize,
        width: f32,
        height: f32,
        scale_factor: f32,
        is_big_endian: bool,
        header_offset: usize,
    ) -> Self {
        Self {
            cols,
            pixel_size,
            scroll_offset,
            color_mode,
            entropy_window,
            active_len,
            state_id,
            width_bits: width.to_bits(),
            height_bits: height.to_bits(),
            scale_factor_bits: scale_factor.to_bits(),
            is_big_endian,
            header_offset,
        }
    }
}

pub type CachedImage = (Arc<RenderImage>, VisualMapCacheKey);

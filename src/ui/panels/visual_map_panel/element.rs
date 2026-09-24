//! Low-level GPUI custom element rendering for the 2D visual map pixel buffer and overlays.

use super::VisualMapPanel;
use super::cache::VisualMapCacheKey;
use crate::core::visual_map::geometry::VisualMapGeometry;
use crate::core::visual_map::{CategoryPalette, VisualMapColorMode as ColorMode, VisualMapRenderParams, category_bgra_lut, render_visual_map_bgra};
use crate::ui::components::scrollbar::CanvasScrollbar;
use gpui_kit::component::ActiveTheme;
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::ops::Range;
use std::sync::Arc;

pub(crate) fn category_palette_from_theme(theme: &gpui_kit::component::Theme) -> CategoryPalette {
    let to_bgra = |hsla: Hsla| {
        let rgb = hsla.to_rgb();
        [
            (rgb.b * 255.0).clamp(0.0, 255.0) as u8,
            (rgb.g * 255.0).clamp(0.0, 255.0) as u8,
            (rgb.r * 255.0).clamp(0.0, 255.0) as u8,
            (rgb.a * 255.0).clamp(0.0, 255.0) as u8,
        ]
    };
    CategoryPalette {
        null: to_bgra(theme.muted_foreground.opacity(0.18)),
        control: to_bgra(theme.red.opacity(0.75)),
        space: to_bgra(theme.blue.opacity(0.55)),
        ascii: to_bgra(theme.green.opacity(0.85)),
        extended: to_bgra(theme.accent.opacity(0.8)),
    }
}

pub(crate) struct VisualMapElement {
    pub(crate) panel: WeakEntity<VisualMapPanel>,
    pub(crate) document: Arc<std::sync::RwLock<crate::core::document::Document>>,
    pub(crate) cols: usize,
    pub(crate) pixel_size: usize,
    pub(crate) scroll_offset: usize,
    pub(crate) color_mode: ColorMode,
    pub(crate) entropy_window: usize,
    pub(crate) is_big_endian: bool,
    pub(crate) header_offset: usize,
    pub(crate) state_id: usize,
    pub(crate) cursor_offset: Option<usize>,
    pub(crate) selection_range: Option<Range<usize>>,
    pub(crate) hovered_pixel: Option<usize>,
    pub(crate) is_dragging_scrollbar: bool,
    pub(crate) scrollbar_hovered: bool,
}

impl Element for VisualMapElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = relative(1.).into();
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        _bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Self::PrepaintState {
    }

    fn paint(
        &mut self,
        _id: Option<&GlobalElementId>,
        _inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _request_layout: &mut Self::RequestLayoutState,
        _prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(panel) = self.panel.upgrade() {
            panel.read(cx).last_bounds.set(Some(bounds));
        }

        let Some(doc) = self.document.read().ok() else {
            return;
        };
        let buffer = &doc.buffer;
        let buffer_len = buffer.len();
        let active_len = buffer_len.saturating_sub(self.header_offset);

        let theme = cx.theme();

        if active_len == 0 {
            return;
        }

        let pixel_size = self.pixel_size as f32;
        let cols = self.cols;

        let total_pixels = self.color_mode.total_pixels(active_len);
        let total_rows = total_pixels.div_ceil(cols);
        let visible_rows = (bounds.size.height.as_f32() / pixel_size).ceil() as usize + 1;
        let max_visible_cols = (bounds.size.width.as_f32() / pixel_size).ceil() as usize + 1;

        let start_row = self.scroll_offset;
        let end_row = (start_row + visible_rows).min(total_rows);

        let scale_factor = window.scale_factor();
        let cell_width = (pixel_size * scale_factor).round().max(1.0) as usize;
        let cell_height = (pixel_size * scale_factor).round().max(1.0) as usize;

        let physical_width = cols * cell_width;
        let physical_height = visible_rows * cell_height;

        let mut cached_image = None;
        if let Some(panel) = self.panel.upgrade() {
            let panel_ref = panel.read(cx);
            let cache_key = VisualMapCacheKey::new(
                self.cols,
                self.pixel_size,
                self.scroll_offset,
                self.color_mode,
                self.entropy_window,
                active_len,
                self.state_id,
                bounds.size.width.as_f32(),
                bounds.size.height.as_f32(),
                scale_factor,
                self.is_big_endian,
                self.header_offset,
            );

            let mut cache = panel_ref.cached_image.borrow_mut();
            if let Some((img, key)) = &*cache
                && key == &cache_key
            {
                cached_image = Some(img.clone());
            }

            if cached_image.is_none() && physical_width > 0 && physical_height > 0 {
                let custom_lut = if self.color_mode == ColorMode::DataCategory {
                    let palette = category_palette_from_theme(theme);
                    Some(category_bgra_lut(&palette))
                } else {
                    None
                };

                let params = VisualMapRenderParams {
                    cols,
                    start_row,
                    visible_rows,
                    max_visible_cols,
                    cell_width,
                    cell_height,
                    physical_width,
                    physical_height,
                    color_mode: self.color_mode,
                    entropy_window: self.entropy_window,
                    custom_lut,
                    is_big_endian: self.is_big_endian,
                };

                let active_data = &buffer.data()[self.header_offset.min(buffer.data().len())..];
                let pixels = render_visual_map_bgra(active_data, &params);

                if let Some(rgba_img) = image::RgbaImage::from_raw(physical_width as u32, physical_height as u32, pixels) {
                    let frame = image::Frame::new(rgba_img);
                    let render_img = Arc::new(RenderImage::new(vec![frame]));
                    *cache = Some((render_img.clone(), cache_key));
                    cached_image = Some(render_img);
                }
            }
        }

        if let Some(img) = cached_image {
            let logical_width = physical_width as f32 / scale_factor;
            let logical_height = physical_height as f32 / scale_factor;
            let image_bounds = Bounds::new(bounds.origin, size(px(logical_width), px(logical_height)));
            window.paint_image(image_bounds, image_bounds, Corners::default(), img, 0, false).ok();
        }

        // Selection Highlight Overlay
        if let Some(sel) = &self.selection_range
            && sel.start < sel.end
        {
            for r in start_row..end_row {
                if let Some((c_start, c_count)) = VisualMapGeometry::selection_row_range(sel.start, sel.end, cols, r, self.color_mode, total_pixels) {
                    let sel_x = bounds.origin.x + px(c_start as f32 * pixel_size);
                    let sel_y = bounds.origin.y + px((r - start_row) as f32 * pixel_size);
                    let sel_w = px(c_count as f32 * pixel_size);
                    let sel_h = px(pixel_size);
                    window.paint_quad(fill(Bounds::new(point(sel_x, sel_y), size(sel_w, sel_h)), theme.accent.opacity(0.35)));
                }
            }
        }

        // Hover Highlight
        if let Some(hov_pix) = self.hovered_pixel {
            let hov_row = hov_pix / cols;
            let hov_col = hov_pix % cols;
            if hov_row >= start_row && hov_row < end_row && hov_pix < total_pixels {
                let cell_x = bounds.origin.x + px(hov_col as f32 * pixel_size);
                let cell_y = bounds.origin.y + px((hov_row - start_row) as f32 * pixel_size);
                let cell_w = px(pixel_size);
                let cell_h = px(pixel_size);
                let cell_bounds = Bounds::new(point(cell_x, cell_y), size(cell_w, cell_h));

                let outline_color = theme.foreground.opacity(0.75);
                let border_w = if pixel_size >= 4.0 { px(1.0) } else { px(0.5) };
                window.paint_quad(outline(cell_bounds, outline_color, BorderStyle::Solid).border_widths(border_w));
            }
        }

        // Cursor Highlight
        if let Some(cursor) = self.cursor_offset {
            let (cur_row, cur_col) = VisualMapGeometry::cursor_grid_pos(cursor, 0, cols, self.color_mode);
            let cell_width_multiplier = if self.color_mode == ColorMode::Planar4bpp { 8.0 } else { 1.0 };

            if cur_row >= start_row && cur_row < end_row && cursor <= active_len {
                let cell_x = bounds.origin.x + px(cur_col as f32 * pixel_size);
                let cell_y = bounds.origin.y + px((cur_row - start_row) as f32 * pixel_size);
                let w = px(pixel_size * cell_width_multiplier);
                let h = px(pixel_size);

                if pixel_size <= 2.0 && cell_width_multiplier == 1.0 {
                    let indicator_size = px(6.0);
                    let center_x = cell_x + px(pixel_size * 0.5);
                    let center_y = cell_y + px(pixel_size * 0.5);
                    let cur_bounds = Bounds::new(
                        point(center_x - indicator_size * 0.5, center_y - indicator_size * 0.5),
                        size(indicator_size, indicator_size),
                    );
                    window.paint_quad(outline(cur_bounds, theme.accent, BorderStyle::Solid).border_widths(px(1.5)));
                    window.paint_quad(fill(Bounds::new(point(cell_x, cell_y), size(px(pixel_size), px(pixel_size))), theme.foreground));
                } else {
                    let cur_bounds = Bounds::new(point(cell_x, cell_y), size(w, h));
                    window.paint_quad(outline(cur_bounds, theme.accent, BorderStyle::Solid).border_widths(px(1.5)));
                    window.paint_quad(fill(cur_bounds, theme.accent.opacity(0.3)));
                }
            }
        }

        // Vertical scrollbar
        CanvasScrollbar::new(bounds, self.scroll_offset, total_rows, pixel_size)
            .dragging(self.is_dragging_scrollbar)
            .hovered(self.scrollbar_hovered)
            .paint(theme, window);

        if self.is_dragging_scrollbar {
            let panel_weak = self.panel.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
                if !phase.bubble() {
                    return;
                }
                if let Some(panel) = panel_weak.upgrade() {
                    panel.update(cx, |this, cx| {
                        if !event.dragging() {
                            this.is_dragging_scrollbar = false;
                            cx.notify();
                        } else {
                            this.update_scrollbar_drag(f32::from(event.position.y), cx);
                        }
                    });
                }
            });
            let panel_weak = self.panel.clone();
            window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
                if !phase.bubble() || event.button != MouseButton::Left {
                    return;
                }
                if let Some(panel) = panel_weak.upgrade() {
                    panel.update(cx, |this, cx| {
                        if this.is_dragging_scrollbar {
                            this.is_dragging_scrollbar = false;
                            cx.notify();
                        }
                    });
                }
            });
        }
    }
}

impl IntoElement for VisualMapElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

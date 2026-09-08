use gpui_kit::component::scroll::ScrollbarStyles;
use gpui_kit::component::theme::Theme;
use gpui_kit::{Bounds, Corners, Hsla, Pixels, Window, fill, point, px, size};

/// Standard scrollbar track thickness (width for vertical, height for horizontal).
pub const SCROLLBAR_WIDTH: Pixels = px(12.0);
/// Standard scrollbar thumb thickness (width for vertical, height for horizontal).
pub const SCROLLBAR_THUMB_WIDTH: Pixels = px(8.0);
/// Inset of the thumb from the track edge ((SCROLLBAR_WIDTH - SCROLLBAR_THUMB_WIDTH) / 2).
pub const SCROLLBAR_THUMB_INSET: Pixels = px(2.0);
/// Corner radius for scrollbar thumb (fully rounded pill).
pub const SCROLLBAR_THUMB_RADIUS: Pixels = px(4.0);
/// Minimum thumb length in pixels to ensure it remains visible and draggable.
pub const SCROLLBAR_MIN_THUMB_LENGTH: Pixels = px(24.0);

/// Returns the scrollbar track background color for the given state.
pub fn scrollbar_track_color(is_hovered: bool, is_dragging: bool, theme: &Theme) -> Hsla {
    if is_dragging {
        theme.foreground.opacity(0.14)
    } else if is_hovered {
        theme.foreground.opacity(0.10)
    } else {
        theme.foreground.opacity(0.05)
    }
}

/// Returns the scrollbar thumb background color with strong contrast against backgrounds.
pub fn scrollbar_thumb_color(is_hovered: bool, is_dragging: bool, theme: &Theme) -> Hsla {
    if is_dragging {
        theme.muted_foreground.opacity(1.0)
    } else if is_hovered {
        theme.muted_foreground.opacity(0.85)
    } else {
        theme.muted_foreground.opacity(0.65)
    }
}

/// Constructs a unified `ScrollbarStyles` configured to match the application's
/// scrollbar dimensions, colors, and corner radii.
pub fn common_scrollbar_styles(theme: &Theme) -> ScrollbarStyles {
    let track_idle = scrollbar_track_color(false, false, theme);
    let track_hover = scrollbar_track_color(true, false, theme);
    let track_active = scrollbar_track_color(false, true, theme);

    let thumb_idle = scrollbar_thumb_color(false, false, theme);
    let thumb_hover = scrollbar_thumb_color(true, false, theme);
    let thumb_active = scrollbar_thumb_color(false, true, theme);

    ScrollbarStyles::default()
        .track(move |s| s.bg(track_idle).width(SCROLLBAR_WIDTH))
        .track_hover(move |s| s.bg(track_hover).width(SCROLLBAR_WIDTH))
        .track_active(move |s| s.bg(track_active).width(SCROLLBAR_WIDTH))
        .thumb(move |s| {
            s.bg(thumb_idle)
                .width(SCROLLBAR_THUMB_WIDTH)
                .inset(SCROLLBAR_THUMB_INSET)
                .radius(SCROLLBAR_THUMB_RADIUS)
                .min_length(SCROLLBAR_MIN_THUMB_LENGTH)
        })
        .thumb_hover(move |s| {
            s.bg(thumb_hover)
                .width(SCROLLBAR_THUMB_WIDTH)
                .inset(SCROLLBAR_THUMB_INSET)
                .radius(SCROLLBAR_THUMB_RADIUS)
                .min_length(SCROLLBAR_MIN_THUMB_LENGTH)
        })
        .thumb_active(move |s| {
            s.bg(thumb_active)
                .width(SCROLLBAR_THUMB_WIDTH)
                .inset(SCROLLBAR_THUMB_INSET)
                .radius(SCROLLBAR_THUMB_RADIUS)
                .min_length(SCROLLBAR_MIN_THUMB_LENGTH)
        })
}

/// Calculated geometry for drawing a scrollbar thumb.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScrollbarGeometry {
    pub visible_rows: usize,
    pub max_top_row: usize,
    pub thumb_top: f32,
    pub thumb_height: f32,
}

/// Calculates scrollbar metrics for a given viewport height and row layout.
/// Returns None if the content fits entirely within the viewport.
pub fn calculate_scrollbar_geometry(viewport_height: f32, scroll_offset: usize, total_rows: usize, row_height: f32) -> Option<ScrollbarGeometry> {
    if viewport_height <= 0.0 || row_height <= 0.0 {
        return None;
    }
    let visible_rows = (viewport_height / row_height).floor() as usize;
    if total_rows <= visible_rows {
        return None;
    }
    let max_top_row = total_rows.saturating_sub(visible_rows.max(1));
    let ratio = (visible_rows as f64 / total_rows as f64).clamp(0.0, 1.0);
    let min_length_f64 = f64::from(f32::from(SCROLLBAR_MIN_THUMB_LENGTH));
    let thumb_h = (viewport_height as f64 * ratio).clamp(min_length_f64, viewport_height as f64) as f32;
    let max_thumb_top = (viewport_height - thumb_h).max(0.0);

    let scroll_ratio = if max_top_row > 0 {
        (scroll_offset as f64 / max_top_row as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let thumb_top = (scroll_ratio * max_thumb_top as f64) as f32;

    Some(ScrollbarGeometry {
        visible_rows,
        max_top_row,
        thumb_top,
        thumb_height: thumb_h,
    })
}

/// Paints a vertical scrollbar with a custom row height on a GPUI canvas.
#[allow(clippy::too_many_arguments)]
pub fn paint_scrollbar_with_row_height(
    list_bounds: Bounds<Pixels>,
    scroll_offset: usize,
    total_rows: usize,
    row_height: f32,
    is_dragging: bool,
    is_hovered: bool,
    theme: &Theme,
    window: &mut Window,
) {
    let list_h = f32::from(list_bounds.size.height);
    let Some(geom) = calculate_scrollbar_geometry(list_h, scroll_offset, total_rows, row_height) else {
        return;
    };

    let bar_w = SCROLLBAR_WIDTH;
    let bar_x = list_bounds.right() - bar_w;
    let bar_bounds = Bounds::new(point(bar_x, list_bounds.top()), size(bar_w, list_bounds.size.height));

    let track_color = scrollbar_track_color(is_hovered, is_dragging, theme);
    window.paint_quad(fill(bar_bounds, track_color));

    let thumb_inset = SCROLLBAR_THUMB_INSET;
    let thumb_w = SCROLLBAR_THUMB_WIDTH;
    let thumb_radius = SCROLLBAR_THUMB_RADIUS;

    let thumb_bounds = Bounds::new(
        point(bar_x + thumb_inset, list_bounds.top() + px(geom.thumb_top)),
        size(thumb_w, px(geom.thumb_height)),
    );
    let thumb_color = scrollbar_thumb_color(is_hovered, is_dragging, theme);
    let mut quad = fill(thumb_bounds, thumb_color);
    quad.corner_radii = Corners::all(thumb_radius);
    window.paint_quad(quad);
}

/// Paints a vertical scrollbar using HexView's standard row height on a GPUI canvas.
pub fn paint_scrollbar(
    list_bounds: Bounds<Pixels>,
    scroll_offset: usize,
    total_rows: usize,
    is_dragging: bool,
    is_hovered: bool,
    theme: &Theme,
    window: &mut Window,
) {
    paint_scrollbar_with_row_height(
        list_bounds,
        scroll_offset,
        total_rows,
        crate::ui::components::hex_view::ROW_HEIGHT,
        is_dragging,
        is_hovered,
        theme,
        window,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_scrollbar_geometry_hidden_when_content_fits() {
        // Viewport 500px, 20 rows of height 20px = 400px total, fits inside 500px
        assert_eq!(calculate_scrollbar_geometry(500.0, 0, 20, 20.0), None);

        // Exactly fits (25 rows * 20px = 500px)
        assert_eq!(calculate_scrollbar_geometry(500.0, 0, 25, 20.0), None);

        // Zero or negative viewport height
        assert_eq!(calculate_scrollbar_geometry(0.0, 0, 100, 20.0), None);
        assert_eq!(calculate_scrollbar_geometry(-100.0, 0, 100, 20.0), None);
    }

    #[test]
    fn test_calculate_scrollbar_geometry_visible_metrics() {
        // Viewport 400px, 100 total rows, 20px row height -> 20 visible rows, max_top_row = 80
        let geom = calculate_scrollbar_geometry(400.0, 0, 100, 20.0).expect("should be visible");
        assert_eq!(geom.visible_rows, 20);
        assert_eq!(geom.max_top_row, 80);
        assert_eq!(geom.thumb_top, 0.0);
        // ratio = 20 / 100 = 0.2, thumb_h = 400 * 0.2 = 80.0
        assert_eq!(geom.thumb_height, 80.0);

        // At end: scroll_offset = 80
        let geom_end = calculate_scrollbar_geometry(400.0, 80, 100, 20.0).expect("should be visible");
        assert_eq!(geom_end.thumb_top, 400.0 - 80.0);

        // Visual map with small pixel_size (e.g. 2px, 500 rows, 200px viewport -> 100 visible rows)
        let geom_map = calculate_scrollbar_geometry(200.0, 50, 500, 2.0).expect("should be visible");
        assert_eq!(geom_map.visible_rows, 100);
        assert_eq!(geom_map.max_top_row, 400);
        // ratio = 100 / 500 = 0.2, thumb_h = 200 * 0.2 = 40.0
        assert_eq!(geom_map.thumb_height, 40.0);
        // thumb_top = (50 / 400) * (200 - 40) = 0.125 * 160 = 20.0
        assert_eq!(geom_map.thumb_top, 20.0);
    }

    #[test]
    fn test_scrollbar_colors_and_styles() {
        let theme = Theme::default();
        let track_idle = scrollbar_track_color(false, false, &theme);
        let track_hover = scrollbar_track_color(true, false, &theme);
        let track_active = scrollbar_track_color(false, true, &theme);

        assert_eq!(track_idle, theme.foreground.opacity(0.05));
        assert_eq!(track_hover, theme.foreground.opacity(0.10));
        assert_eq!(track_active, theme.foreground.opacity(0.14));

        let thumb_idle = scrollbar_thumb_color(false, false, &theme);
        let thumb_hover = scrollbar_thumb_color(true, false, &theme);
        let thumb_active = scrollbar_thumb_color(false, true, &theme);

        assert_eq!(thumb_idle, theme.muted_foreground.opacity(0.65));
        assert_eq!(thumb_hover, theme.muted_foreground.opacity(0.85));
        assert_eq!(thumb_active, theme.muted_foreground.opacity(1.0));

        let _styles = common_scrollbar_styles(&theme);
        assert_eq!(SCROLLBAR_WIDTH, px(12.0));
        assert_eq!(SCROLLBAR_THUMB_WIDTH, px(8.0));
        assert_eq!(SCROLLBAR_THUMB_INSET, px(2.0));
        assert_eq!(SCROLLBAR_THUMB_RADIUS, px(4.0));
    }
}

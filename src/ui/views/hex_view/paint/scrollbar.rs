/// Paints a vertical scrollbar using HexView's dynamic row height on a GPUI canvas.
#[allow(clippy::too_many_arguments)]
pub fn paint_scrollbar(
    list_bounds: gpui_kit::Bounds<gpui_kit::Pixels>,
    scroll_offset: usize,
    total_rows: usize,
    row_height: f32,
    is_dragging: bool,
    is_hovered: bool,
    theme: &gpui_kit::component::theme::Theme,
    window: &mut gpui_kit::Window,
) {
    crate::ui::components::scrollbar::CanvasScrollbar::new(list_bounds, scroll_offset, total_rows, row_height)
        .dragging(is_dragging)
        .hovered(is_hovered)
        .paint(theme, window);
}

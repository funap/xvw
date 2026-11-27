/// Paints a vertical scrollbar using HexView's standard row height on a GPUI canvas.
pub fn paint_scrollbar(
    list_bounds: gpui_kit::Bounds<gpui_kit::Pixels>,
    scroll_offset: usize,
    total_rows: usize,
    is_dragging: bool,
    is_hovered: bool,
    theme: &gpui_kit::component::theme::Theme,
    window: &mut gpui_kit::Window,
) {
    crate::ui::components::scrollbar::CanvasScrollbar::new(list_bounds, scroll_offset, total_rows, super::super::types::ROW_HEIGHT)
        .dragging(is_dragging)
        .hovered(is_hovered)
        .paint(theme, window);
}

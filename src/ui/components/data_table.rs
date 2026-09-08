use gpui_kit::component::scroll::{Scrollbar, ScrollbarAxis, ScrollbarMode};
use gpui_kit::component::theme::Theme;
use gpui_kit::component::{StyledExt, h_flex};
use gpui_kit::{
    AnyElement, App, Context, Div, ElementId, FocusHandle, Hsla, InteractiveElement, IntoElement, KeyBinding, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement, Pixels, ScrollHandle, ScrollStrategy, ScrollWheelEvent, SharedString, Stateful, Styled, UniformListScrollHandle, actions,
    canvas, div, point, px, size,
};

pub const CONTEXT: &str = "VirtualTable";
pub const TABLE_SCROLLBAR_WIDTH: Pixels = crate::ui::scrollbar::SCROLLBAR_WIDTH;
pub const DEFAULT_AUTOFIT_CHAR_WIDTH: f32 = 7.2;
pub const DEFAULT_AUTOFIT_PADDING: f32 = 16.0;

actions!(table, [MoveUp, MoveDown, MoveTop, MoveBottom, PageUp, PageDown, SelectCurrent, Dismiss]);

/// Registers default keybindings for table navigation.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, Some("VirtualTable")),
        KeyBinding::new("down", MoveDown, Some("VirtualTable")),
        KeyBinding::new("k", MoveUp, Some("VirtualTable")),
        KeyBinding::new("j", MoveDown, Some("VirtualTable")),
        KeyBinding::new("home", MoveTop, Some("VirtualTable")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-home", MoveTop, Some("VirtualTable")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-home", MoveTop, Some("VirtualTable")),
        KeyBinding::new("end", MoveBottom, Some("VirtualTable")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-end", MoveBottom, Some("VirtualTable")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-end", MoveBottom, Some("VirtualTable")),
        KeyBinding::new("pageup", PageUp, Some("VirtualTable")),
        KeyBinding::new("pagedown", PageDown, Some("VirtualTable")),
        KeyBinding::new("enter", SelectCurrent, Some("VirtualTable")),
        KeyBinding::new("escape", Dismiss, Some("VirtualTable")),
    ]);
}

/// Represents sort direction for a table column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableSortDirection {
    Ascending,
    Descending,
}

impl TableSortDirection {
    /// Returns the reversed sort direction.
    #[allow(dead_code)]
    pub fn reversed(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }

    /// Returns an arrow indicator string for the header.
    pub fn indicator(self) -> &'static str {
        match self {
            Self::Ascending => " ↑",
            Self::Descending => " ↓",
        }
    }
}

/// Defines a column in the virtual table.
#[derive(Clone, Debug, PartialEq)]
pub struct TableColumn {
    pub id: SharedString,
    pub name: SharedString,
    pub width: Pixels,
    pub min_width: Pixels,
    pub max_width: Option<Pixels>,
    pub resizable: bool,
    pub sortable: bool,
    pub sort_direction: Option<TableSortDirection>,
    pub visible: bool,
}

impl TableColumn {
    /// Creates a new table column with default properties.
    pub fn new(id: impl Into<SharedString>, name: impl Into<SharedString>, width: impl Into<Pixels>) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            width: width.into(),
            min_width: px(36.0),
            max_width: Some(px(1200.0)),
            resizable: true,
            sortable: false,
            sort_direction: None,
            visible: true,
        }
    }

    /// Sets the minimum width of the column.
    pub fn min_width(mut self, min_width: impl Into<Pixels>) -> Self {
        self.min_width = min_width.into();
        self
    }

    /// Sets the maximum width of the column.
    #[allow(dead_code)]
    pub fn max_width(mut self, max_width: impl Into<Pixels>) -> Self {
        self.max_width = Some(max_width.into());
        self
    }

    /// Sets whether the column can be resized by dragging.
    pub fn resizable(mut self, resizable: bool) -> Self {
        self.resizable = resizable;
        self
    }

    /// Sets whether the column supports sorting.
    pub fn sortable(mut self, sortable: bool) -> Self {
        self.sortable = sortable;
        self
    }

    /// Sets the initial sort direction of the column.
    #[allow(dead_code)]
    pub fn sort_direction(mut self, sort_direction: Option<TableSortDirection>) -> Self {
        self.sort_direction = sort_direction;
        self
    }

    /// Sets the initial visibility of the column.
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }
}

/// State of an active column resize drag operation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColumnResizeState {
    pub col_ix: usize,
    pub start_x: Pixels,
    pub initial_width: Pixels,
}

/// Holds layout, focus, and scroll state for a `VirtualTable`.
pub struct VirtualTableState {
    pub id: ElementId,
    pub focus_handle: Option<FocusHandle>,
    pub columns: Vec<TableColumn>,
    pub vertical_scroll_handle: UniformListScrollHandle,
    pub horizontal_scroll_handle: ScrollHandle,
    pub resizing_column: Option<ColumnResizeState>,
    pub scroll_offset_x: Pixels,
    pub last_container_width: Pixels,
}

impl VirtualTableState {
    /// Creates a new `VirtualTableState` with the provided id, columns, and focus handle.
    pub fn new(id: impl Into<ElementId>, columns: Vec<TableColumn>, focus_handle: FocusHandle) -> Self {
        Self {
            id: id.into(),
            focus_handle: Some(focus_handle),
            columns,
            vertical_scroll_handle: UniformListScrollHandle::new(),
            horizontal_scroll_handle: ScrollHandle::new(),
            resizing_column: None,
            scroll_offset_x: px(0.0),
            last_container_width: px(0.0),
        }
    }

    /// Creates a test `VirtualTableState` with placeholder id and without focus handle.
    #[cfg(test)]
    pub fn for_test(columns: Vec<TableColumn>) -> Self {
        Self {
            id: "test-table".into(),
            focus_handle: None,
            columns,
            vertical_scroll_handle: UniformListScrollHandle::new(),
            horizontal_scroll_handle: ScrollHandle::new(),
            resizing_column: None,
            scroll_offset_x: px(0.0),
            last_container_width: px(0.0),
        }
    }

    /// Returns an iterator over all visible columns with their original indices.
    pub fn visible_columns(&self) -> impl Iterator<Item = (usize, &TableColumn)> {
        self.columns.iter().enumerate().filter(|(_, col)| col.visible)
    }

    /// Returns the number of currently visible columns.
    #[allow(dead_code)]
    pub fn visible_columns_count(&self) -> usize {
        self.columns.iter().filter(|col| col.visible).count()
    }

    /// Returns the total width of all visible columns.
    pub fn total_visible_width(&self) -> Pixels {
        self.visible_columns().map(|(_, col)| col.width).fold(px(0.0), |acc, w| acc + w)
    }

    /// Returns a reference to the column at the given index.
    pub fn column(&self, col_ix: usize) -> Option<&TableColumn> {
        self.columns.get(col_ix)
    }

    /// Returns a mutable reference to the column at the given index.
    #[allow(dead_code)]
    pub fn column_mut(&mut self, col_ix: usize) -> Option<&mut TableColumn> {
        self.columns.get_mut(col_ix)
    }

    /// Updates the width of the column at `col_ix`, clamping within `[min_width, max_width]`.
    pub fn set_column_width(&mut self, col_ix: usize, width: Pixels) {
        if let Some(col) = self.columns.get_mut(col_ix) {
            let mut clamped = width.max(col.min_width);
            if let Some(max_w) = col.max_width {
                clamped = clamped.min(max_w);
            }
            col.width = clamped;
        }
    }

    /// Sets the visibility of the column at `col_ix`.
    pub fn set_column_visible(&mut self, col_ix: usize, visible: bool) {
        if let Some(col) = self.columns.get_mut(col_ix) {
            col.visible = visible;
        }
    }

    /// Sets the sort direction for the column at `col_ix` and clears other sortable columns.
    pub fn set_column_sort(&mut self, col_ix: usize, sort: Option<TableSortDirection>) {
        for (i, col) in self.columns.iter_mut().enumerate() {
            if i == col_ix {
                col.sort_direction = sort;
            } else {
                col.sort_direction = None;
            }
        }
    }

    /// Toggles the sort direction for the column at `col_ix` (None -> Ascending -> Descending -> None).
    pub fn toggle_column_sort(&mut self, col_ix: usize) -> Option<TableSortDirection> {
        let next_sort = match self.columns.get(col_ix).and_then(|c| c.sort_direction) {
            None => Some(TableSortDirection::Ascending),
            Some(TableSortDirection::Ascending) => Some(TableSortDirection::Descending),
            Some(TableSortDirection::Descending) => None,
        };
        self.set_column_sort(col_ix, next_sort);
        next_sort
    }

    /// Initiates a column resize drag operation.
    pub fn start_resize(&mut self, col_ix: usize, start_x: Pixels) {
        if let Some(col) = self.columns.get(col_ix)
            && col.resizable
        {
            self.resizing_column = Some(ColumnResizeState {
                col_ix,
                start_x,
                initial_width: col.width,
            });
        }
    }

    /// Updates the column width during an active resize drag operation.
    pub fn update_resize(&mut self, current_x: Pixels) -> Option<usize> {
        let resize = self.resizing_column?;
        let delta = current_x - resize.start_x;
        let new_width = resize.initial_width + delta;
        self.set_column_width(resize.col_ix, new_width);
        Some(resize.col_ix)
    }

    /// Ends the active column resize drag operation.
    pub fn end_resize(&mut self) -> Option<(usize, Pixels)> {
        let resize = self.resizing_column.take()?;
        let width = self.columns.get(resize.col_ix).map(|c| c.width)?;
        Some((resize.col_ix, width))
    }

    /// Auto-fits the width of the column at `col_ix` based on its header name and an iterator of text values.
    pub fn auto_fit_column_with_texts(&mut self, col_ix: usize, texts: impl IntoIterator<Item = impl AsRef<str>>) {
        self.end_resize();
        let Some(col) = self.columns.get(col_ix) else {
            return;
        };
        let mut max_width = col
            .name
            .chars()
            .take(128)
            .map(|c| {
                if c.is_ascii() {
                    DEFAULT_AUTOFIT_CHAR_WIDTH
                } else {
                    DEFAULT_AUTOFIT_CHAR_WIDTH * 1.8
                }
            })
            .sum::<f32>();

        for text in texts {
            let t = text.as_ref();
            let text_w = t
                .chars()
                .take(256)
                .map(|c| {
                    if c.is_ascii() {
                        DEFAULT_AUTOFIT_CHAR_WIDTH
                    } else {
                        DEFAULT_AUTOFIT_CHAR_WIDTH * 1.8
                    }
                })
                .sum::<f32>();
            max_width = max_width.max(text_w);
        }

        let total_w = max_width + DEFAULT_AUTOFIT_PADDING;
        self.set_column_width(col_ix, px(total_w));
    }

    /// Scrolls to a specific row index.
    pub fn scroll_to_row(&mut self, row_ix: usize, strategy: ScrollStrategy) {
        self.vertical_scroll_handle.scroll_to_item(row_ix, strategy);
    }

    /// Adjusts horizontal scroll offset with delta X, clamping to `[0, max(0, total_width - container_width)]`.
    pub fn scroll_horizontally(&mut self, delta_x: Pixels, container_width: Pixels) {
        let new_offset = self.scroll_offset_x - delta_x;
        self.set_horizontal_scroll(new_offset, container_width);
    }

    /// Sets horizontal scroll offset, clamping it to `[0, max(0, total_width - container_width)]`.
    pub fn set_horizontal_scroll(&mut self, offset: Pixels, container_width: Pixels) {
        let total_w = self.total_visible_width();
        let max_scroll = (total_w - container_width).max(px(0.0));
        let clamped = offset.clamp(px(0.0), max_scroll);
        self.scroll_offset_x = clamped;
        self.horizontal_scroll_handle.set_offset(point(-clamped, px(0.0)));
    }
}

/// Reusable table renderer component.
pub struct VirtualTable;

impl VirtualTable {
    /// Returns a canvas overlay that intercepts Shift+scroll wheel events during the Capture phase
    /// (ensuring purely horizontal scrolling without unintended diagonal/vertical movement in uniform_list),
    /// tracks container bounds, and manages column resize drag operations.
    pub fn render_table_overlay<V: 'static>(
        state: &VirtualTableState,
        cx: &Context<V>,
        get_state_mut: impl Fn(&mut V) -> &mut VirtualTableState + 'static + Copy,
    ) -> AnyElement {
        let is_resizing = state.resizing_column.is_some();
        let view = cx.entity().clone();

        canvas(
            {
                let view = view.clone();
                move |bounds, _window, cx| {
                    view.update(cx, |this, _| {
                        let table_state = get_state_mut(this);
                        table_state.last_container_width = bounds.size.width;
                    });
                }
            },
            move |bounds, _prepaint, window, _cx| {
                let view_scroll = view.clone();
                window.on_mouse_event(move |event: &ScrollWheelEvent, phase, _window, cx| {
                    if !phase.capture() {
                        return;
                    }
                    if !bounds.contains(&event.position) {
                        return;
                    }

                    if event.modifiers.shift {
                        let pixel_delta = event.delta.pixel_delta(px(20.0));
                        let delta_x = pixel_delta.x;
                        let delta_y = pixel_delta.y;
                        let scroll_x = if delta_x != px(0.0) { delta_x } else { delta_y };

                        // Stop propagation so uniform_list cannot receive this event and scroll vertically
                        cx.stop_propagation();

                        if scroll_x != px(0.0) {
                            view_scroll.update(cx, |this, cx| {
                                let table_state = get_state_mut(this);
                                table_state.scroll_horizontally(scroll_x, bounds.size.width);
                                cx.notify();
                            });
                        }
                    } else {
                        let pixel_delta = event.delta.pixel_delta(px(20.0));
                        if pixel_delta.x != px(0.0) && pixel_delta.y == px(0.0) {
                            // Pure horizontal gesture from trackpad
                            cx.stop_propagation();
                            view_scroll.update(cx, |this, cx| {
                                let table_state = get_state_mut(this);
                                table_state.scroll_horizontally(pixel_delta.x, bounds.size.width);
                                cx.notify();
                            });
                        }
                    }
                });

                if is_resizing {
                    let view_move = view.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _window, cx| {
                        if !phase.bubble() {
                            return;
                        }
                        view_move.update(cx, |this, cx| {
                            let table_state = get_state_mut(this);
                            if table_state.resizing_column.is_some() {
                                if event.pressed_button != Some(MouseButton::Left) {
                                    table_state.end_resize();
                                    cx.notify();
                                } else {
                                    table_state.update_resize(event.position.x);
                                    cx.notify();
                                }
                            }
                        });
                    });

                    let view_up = view.clone();
                    window.on_mouse_event(move |event: &MouseUpEvent, phase, _window, cx| {
                        if !phase.bubble() {
                            return;
                        }
                        if event.button == MouseButton::Left {
                            view_up.update(cx, |this, cx| {
                                let table_state = get_state_mut(this);
                                if table_state.resizing_column.is_some() {
                                    table_state.end_resize();
                                    cx.notify();
                                }
                            });
                        }
                    });
                }
            },
        )
        .absolute()
        .inset_0()
        .into_any_element()
    }

    /// Legacy resize overlay renderer for backward compatibility.
    #[allow(dead_code)]
    pub fn render_resize_overlay<V: 'static>(
        state: &VirtualTableState,
        cx: &Context<V>,
        get_state_mut: impl Fn(&mut V) -> &mut VirtualTableState + 'static + Copy,
    ) -> Option<AnyElement> {
        if state.resizing_column.is_none() {
            None
        } else {
            Some(Self::render_table_overlay(state, cx, get_state_mut))
        }
    }

    /// Builds a table header container div with horizontal translation applied.
    #[allow(dead_code)]
    pub fn header_container(scroll_offset_x: Pixels, total_width: Pixels, theme: &gpui_kit::component::Theme) -> Div {
        h_flex()
            .w_full()
            .h(px(24.0))
            .flex_shrink_0()
            .overflow_hidden()
            .border_b_1()
            .border_color(theme.border.opacity(0.7))
            .bg(theme.muted.opacity(0.18))
            .text_xs()
            .font_semibold()
            .text_color(theme.muted_foreground)
            .child(h_flex().w(total_width).h_full().ml(-scroll_offset_x))
    }

    /// Builds a row container div with horizontal translation applied.
    #[allow(dead_code)]
    pub fn row_container(scroll_offset_x: Pixels, total_width: Pixels, row_height: Pixels) -> Div {
        h_flex()
            .w_full()
            .h(row_height)
            .flex_shrink_0()
            .overflow_hidden()
            .child(h_flex().w(total_width).h_full().ml(-scroll_offset_x))
    }

    /// Builds a table header cell with optional resize handle.
    pub fn render_header_cell(
        col_ix: usize,
        width: Pixels,
        _is_first: bool,
        border_color: Hsla,
        content: impl IntoElement,
        resize_handle: Option<impl IntoElement>,
    ) -> Stateful<Div> {
        let mut cell = h_flex()
            .id(("table-header-col", col_ix))
            .w(width)
            .h_full()
            .flex_shrink_0()
            .items_center()
            .min_w_0()
            .relative()
            .border_r_1()
            .border_color(border_color);

        cell = cell.child(content);

        if let Some(handle) = resize_handle {
            cell = cell.child(handle);
        }

        cell
    }

    /// Builds a draggable column resize handle positioned on the right edge.
    pub fn render_resize_handle(col_ix: usize, theme: &gpui_kit::component::Theme) -> Stateful<Div> {
        div()
            .id(("table-resize-handle", col_ix))
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .w(px(6.0))
            .cursor_col_resize()
            .hover(|style| style.bg(theme.accent.opacity(0.4)))
    }

    /// Builds a complete table header row with automatic column label rendering, sorting triggers, and resize handles.
    #[allow(clippy::too_many_arguments)]
    pub fn render_header_row<V: 'static>(
        state: &VirtualTableState,
        header_id: impl Into<ElementId>,
        theme: &gpui_kit::component::Theme,
        cx: &Context<V>,
        custom_label_fn: Option<impl Fn(usize, &TableColumn) -> Option<AnyElement> + 'static>,
        on_auto_fit: impl Fn(&mut V, usize, &mut Context<V>) + 'static + Copy,
        on_sort: Option<impl Fn(&mut V, usize, &mut Context<V>) + 'static + Copy>,
        get_state_mut: impl Fn(&mut V) -> &mut VirtualTableState + 'static + Copy,
    ) -> Stateful<Div> {
        let total_visible_width = state.total_visible_width();
        let scroll_offset_x = state.scroll_offset_x;

        h_flex()
            .id(header_id.into())
            .w_full()
            .h(px(24.0))
            .flex_shrink_0()
            .overflow_hidden()
            .border_b_1()
            .border_color(theme.border.opacity(0.7))
            .bg(theme.muted.opacity(0.18))
            .text_xs()
            .font_semibold()
            .text_color(theme.muted_foreground)
            .child(
                h_flex()
                    .w(total_visible_width)
                    .h_full()
                    .ml(-scroll_offset_x)
                    .children(state.visible_columns().enumerate().map(|(vis_ix, (col_ix, col))| {
                        let is_first = vis_ix == 0;
                        let is_sortable = col.sortable && on_sort.is_some();
                        let is_resizable = col.resizable;

                        let sort_indicator = col.sort_direction.map(TableSortDirection::indicator).unwrap_or("");
                        let title_text = format!("{}{sort_indicator}", col.name);

                        let label_content: AnyElement = if let Some(ref custom) = custom_label_fn
                            && let Some(el) = custom(col_ix, col)
                        {
                            el
                        } else {
                            title_text.into_any_element()
                        };

                        let mut label_area = h_flex()
                            .id(("table-header-label", col_ix))
                            .flex_1()
                            .h_full()
                            .items_center()
                            .pl_1()
                            .pr_1()
                            .min_w_0()
                            .truncate()
                            .whitespace_nowrap()
                            .child(label_content);

                        if is_sortable {
                            label_area = label_area.cursor_pointer().hover(|style| style.text_color(theme.foreground));
                        }

                        label_area = label_area.on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                                cx.stop_propagation();
                                if is_sortable && let Some(sort_fn) = on_sort {
                                    sort_fn(this, col_ix, cx);
                                }
                            }),
                        );

                        let resize_handle = if is_resizable {
                            Some(VirtualTable::render_resize_handle(col_ix, theme).on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, event: &MouseDownEvent, _window, cx| {
                                    cx.stop_propagation();
                                    if event.click_count >= 2 {
                                        get_state_mut(this).end_resize();
                                        on_auto_fit(this, col_ix, cx);
                                    } else {
                                        get_state_mut(this).start_resize(col_ix, event.position.x);
                                        cx.notify();
                                    }
                                }),
                            ))
                        } else {
                            None
                        };

                        VirtualTable::render_header_cell(col_ix, col.width, is_first, theme.border.opacity(0.7), label_area, resize_handle)
                    })),
            )
    }

    /// Builds a row cell with standard right border and width matching the column.
    pub fn render_data_cell(_col_ix: usize, width: Pixels, _is_first: bool, border_color: Hsla, content: impl IntoElement) -> Div {
        h_flex()
            .w(width)
            .h_full()
            .flex_shrink_0()
            .items_center()
            .pl_1()
            .pr_1()
            .min_w_0()
            .truncate()
            .whitespace_nowrap()
            .border_r_1()
            .border_color(border_color)
            .child(content)
    }

    /// Builds horizontal scrollbar element if content exceeds container width.
    pub fn render_horizontal_scrollbar(state: &VirtualTableState, container_width: Pixels, theme: &Theme) -> Option<impl IntoElement> {
        let width = if container_width > px(0.0) {
            container_width
        } else {
            state.last_container_width
        };
        let total_w = state.total_visible_width();
        if total_w <= width || width <= px(0.0) {
            return None;
        }

        Some(
            div()
                .id("table-horizontal-scrollbar")
                .absolute()
                .left_0()
                .right_0()
                .bottom_0()
                .h(TABLE_SCROLLBAR_WIDTH)
                .child(
                    Scrollbar::horizontal(&state.horizontal_scroll_handle)
                        .axis(ScrollbarAxis::Horizontal)
                        .mode(ScrollbarMode::Always)
                        .scroll_size(size(total_w, px(0.0)))
                        .styles(|_| crate::ui::scrollbar::common_scrollbar_styles(theme)),
                ),
        )
    }

    /// Builds vertical scrollbar element for the virtual list.
    pub fn render_vertical_scrollbar(state: &VirtualTableState, theme: &Theme) -> impl IntoElement {
        div()
            .id("table-vertical-scrollbar")
            .absolute()
            .top_0()
            .right_0()
            .bottom_0()
            .w(TABLE_SCROLLBAR_WIDTH)
            .child(
                Scrollbar::vertical(&state.vertical_scroll_handle)
                    .axis(ScrollbarAxis::Vertical)
                    .mode(ScrollbarMode::Always)
                    .styles(|_| crate::ui::scrollbar::common_scrollbar_styles(theme)),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{ColumnResizeState, TableColumn, TableSortDirection, VirtualTableState};
    use gpui_kit::px;

    #[test]
    fn test_table_column_builder() {
        let col = TableColumn::new("offset", "Offset", px(80.0))
            .min_width(px(40.0))
            .max_width(px(200.0))
            .sortable(true)
            .resizable(true)
            .visible(true);

        assert_eq!(col.id.as_ref(), "offset");
        assert_eq!(col.name.as_ref(), "Offset");
        assert_eq!(col.width, px(80.0));
        assert_eq!(col.min_width, px(40.0));
        assert_eq!(col.max_width, Some(px(200.0)));
        assert!(col.sortable);
        assert!(col.resizable);
        assert!(col.visible);
    }

    #[test]
    fn test_virtual_table_state_width_and_visibility() {
        let columns = vec![
            TableColumn::new("col1", "Col 1", px(100.0)),
            TableColumn::new("col2", "Col 2", px(150.0)).visible(false),
            TableColumn::new("col3", "Col 3", px(80.0)),
        ];

        let mut state = VirtualTableState::for_test(columns);
        assert_eq!(state.visible_columns_count(), 2);
        assert_eq!(state.total_visible_width(), px(180.0));

        state.set_column_visible(1, true);
        assert_eq!(state.visible_columns_count(), 3);
        assert_eq!(state.total_visible_width(), px(330.0));

        state.set_column_width(0, px(120.0));
        assert_eq!(state.column(0).unwrap().width, px(120.0));
        assert_eq!(state.total_visible_width(), px(350.0));
    }

    #[test]
    fn test_column_width_clamping() {
        let columns = vec![TableColumn::new("col", "Col", px(100.0)).min_width(px(50.0)).max_width(px(200.0))];

        let mut state = VirtualTableState::for_test(columns);

        // Below minimum
        state.set_column_width(0, px(20.0));
        assert_eq!(state.column(0).unwrap().width, px(50.0));

        // Above maximum
        state.set_column_width(0, px(300.0));
        assert_eq!(state.column(0).unwrap().width, px(200.0));

        // Within range
        state.set_column_width(0, px(150.0));
        assert_eq!(state.column(0).unwrap().width, px(150.0));
    }

    #[test]
    fn test_column_sorting_toggle() {
        let columns = vec![
            TableColumn::new("col1", "Col 1", px(100.0)).sortable(true),
            TableColumn::new("col2", "Col 2", px(100.0)).sortable(true),
        ];

        let mut state = VirtualTableState::for_test(columns);

        let sort1 = state.toggle_column_sort(0);
        assert_eq!(sort1, Some(TableSortDirection::Ascending));
        assert_eq!(state.column(0).unwrap().sort_direction, Some(TableSortDirection::Ascending));

        let sort2 = state.toggle_column_sort(0);
        assert_eq!(sort2, Some(TableSortDirection::Descending));
        assert_eq!(state.column(0).unwrap().sort_direction, Some(TableSortDirection::Descending));

        let sort3 = state.toggle_column_sort(0);
        assert_eq!(sort3, None);
        assert_eq!(state.column(0).unwrap().sort_direction, None);

        // Sorting another column resets the first
        state.set_column_sort(0, Some(TableSortDirection::Ascending));
        state.set_column_sort(1, Some(TableSortDirection::Descending));
        assert_eq!(state.column(0).unwrap().sort_direction, None);
        assert_eq!(state.column(1).unwrap().sort_direction, Some(TableSortDirection::Descending));
    }

    #[test]
    fn test_column_resize_drag_lifecycle() {
        let columns = vec![TableColumn::new("col1", "Col 1", px(100.0)).min_width(px(40.0)).max_width(px(300.0))];

        let mut state = VirtualTableState::for_test(columns);

        state.start_resize(0, px(150.0));
        assert_eq!(
            state.resizing_column,
            Some(ColumnResizeState {
                col_ix: 0,
                start_x: px(150.0),
                initial_width: px(100.0),
            })
        );

        // Drag right +50px
        let updated = state.update_resize(px(200.0));
        assert_eq!(updated, Some(0));
        assert_eq!(state.column(0).unwrap().width, px(150.0));

        // Drag left -80px (initial 100 - 30 = 70px)
        state.update_resize(px(120.0));
        assert_eq!(state.column(0).unwrap().width, px(70.0));

        let finished = state.end_resize();
        assert_eq!(finished, Some((0, px(70.0))));
        assert_eq!(state.resizing_column, None);
    }

    #[test]
    fn test_auto_fit_column_with_texts() {
        let columns = vec![TableColumn::new("id", "Field Name", px(80.0)).min_width(px(40.0)).max_width(px(300.0))];

        let mut state = VirtualTableState::for_test(columns);
        let sample_texts = vec!["short", "a much longer field value that expands column", "med"];
        state.auto_fit_column_with_texts(0, sample_texts);

        // Max text length: 45 chars * 7.2 + 16.0 = 340.0 -> clamped to 300.0 max_width
        assert_eq!(state.column(0).unwrap().width, px(300.0));

        // Test with short text
        state.auto_fit_column_with_texts(0, ["abc", "xy"]);
        // Header "Field Name" (10 chars * 7.2 = 72.0) + 16.0 = 88.0px
        assert_eq!(state.column(0).unwrap().width, px(88.0));
    }

    #[test]
    fn test_horizontal_scrolling() {
        let columns = vec![TableColumn::new("col1", "Col 1", px(150.0)), TableColumn::new("col2", "Col 2", px(150.0))];

        let mut state = VirtualTableState::for_test(columns);
        assert_eq!(state.total_visible_width(), px(300.0));

        let container_width = px(200.0);
        // Max scroll offset is 300 - 200 = 100px

        // Scroll right 40px (delta_x = -40)
        state.scroll_horizontally(px(-40.0), container_width);
        assert_eq!(state.scroll_offset_x, px(40.0));

        // Scroll right another 80px -> should clamp to 100px
        state.scroll_horizontally(px(-80.0), container_width);
        assert_eq!(state.scroll_offset_x, px(100.0));

        // Scroll left 50px
        state.scroll_horizontally(px(50.0), container_width);
        assert_eq!(state.scroll_offset_x, px(50.0));

        // Scroll left 100px -> should clamp to 0px
        state.scroll_horizontally(px(100.0), container_width);
        assert_eq!(state.scroll_offset_x, px(0.0));
    }
}

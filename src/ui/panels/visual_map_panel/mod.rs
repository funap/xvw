//! 2D visual byte map dock panel for inspecting binary layout, patterns, and entropy.

pub mod cache;
pub mod element;
pub mod footer;
pub mod legend;
pub mod repeat;
pub mod toolbar;

#[allow(unused_imports)]
pub use self::cache::{CachedImage, VisualMapCacheKey};
use self::element::VisualMapElement;
use crate::core::editor::Editor;
use crate::core::visual_map::geometry::VisualMapGeometry;
#[allow(unused_imports)]
pub use crate::core::visual_map::{ByteCategory, HoveredPixel};
use crate::core::visual_map::{VisualMapColorMode as ColorMode, decode_hovered_pixel};
use crate::ui::components::scrollbar::{SCROLLBAR_WIDTH, calculate_scrollbar_geometry};
use crate::ui::icon::IconName;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dock::{Panel, PanelEvent};
use gpui_kit::component::{ActiveTheme, Icon, Sizable, Size, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::cell::RefCell;
use std::cmp;

pub const DEFAULT_ENTROPY_WINDOW: usize = 256;

fn default_entropy_window() -> usize {
    DEFAULT_ENTROPY_WINDOW
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VisualMapPanelState {
    pub path: std::path::PathBuf,
    pub cols: usize,
    pub pixel_size: usize,
    pub color_mode: ColorMode,
    #[serde(default = "default_entropy_window")]
    pub entropy_window: usize,
    #[serde(default)]
    pub is_big_endian: bool,
}

pub struct VisualMapPanel {
    pub editor: Option<Entity<Editor>>,
    focus_handle: FocusHandle,
    cols: usize,
    pixel_size: usize,
    pub header_offset: usize,
    scroll_offset: usize,
    scroll_remainder: f32,
    is_dragging_scrollbar: bool,
    scrollbar_hovered: bool,
    scrollbar_drag_start_y: f32,
    scrollbar_drag_start_row: usize,
    color_mode: ColorMode,
    pub is_big_endian: bool,
    entropy_window: usize,
    hovered_info: Option<HoveredPixel>,
    last_bounds: std::cell::Cell<Option<Bounds<Pixels>>>,
    cached_image: RefCell<Option<CachedImage>>,
    is_dragging: bool,
    _editor_subscription: Option<Subscription>,
    _width_repeat_task: Option<Task<()>>,
    _offset_repeat_task: Option<Task<()>>,
}

impl EventEmitter<PanelEvent> for VisualMapPanel {}

impl VisualMapPanel {
    pub fn new(editor: Option<Entity<Editor>>, cx: &mut Context<Self>) -> Self {
        let _editor_subscription = editor.as_ref().map(|ed| {
            cx.observe(ed, |_, _, cx| {
                cx.notify();
            })
        });

        let is_big_endian = editor.as_ref().map(|ed| ed.read(cx).options.is_big_endian).unwrap_or(false);

        Self {
            editor,
            focus_handle: cx.focus_handle(),
            cols: 64,
            pixel_size: 2,
            header_offset: 0,
            scroll_offset: 0,
            scroll_remainder: 0.0,
            is_dragging_scrollbar: false,
            scrollbar_hovered: false,
            scrollbar_drag_start_y: 0.0,
            scrollbar_drag_start_row: 0,
            color_mode: ColorMode::DataCategory,
            is_big_endian,
            entropy_window: DEFAULT_ENTROPY_WINDOW,
            hovered_info: None,
            last_bounds: std::cell::Cell::new(None),
            cached_image: RefCell::new(None),
            is_dragging: false,
            _editor_subscription,
            _width_repeat_task: None,
            _offset_repeat_task: None,
        }
    }

    pub fn set_editor(&mut self, editor: Option<Entity<Editor>>, cx: &mut Context<Self>) {
        self._editor_subscription = None;
        self.editor = editor.clone();
        if let Some(ed) = &editor {
            self._editor_subscription = Some(cx.observe(ed, |_, _, cx| {
                cx.notify();
            }));
        }
        self.cached_image.borrow_mut().take();
        cx.notify();
    }

    fn file_path(&self, cx: &App) -> Option<std::path::PathBuf> {
        self.editor
            .as_ref()
            .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.path().to_path_buf()))
    }

    fn buffer_len(&self, cx: &App) -> usize {
        self.editor
            .as_ref()
            .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.buffer.len()))
            .unwrap_or(0)
    }

    pub fn active_buffer_len(&self, cx: &App) -> usize {
        self.buffer_len(cx).saturating_sub(self.header_offset)
    }

    pub fn max_header_offset(&self) -> usize {
        self.cols.saturating_sub(1)
    }

    fn state_id(&self, cx: &App) -> usize {
        self.editor
            .as_ref()
            .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.history.state_id()))
            .unwrap_or(0)
    }

    pub fn scroll_to_cursor(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = &self.editor else { return };
        let cursor_offset = editor.read(cx).cursor.offset;
        let active_len = self.active_buffer_len(cx);
        if active_len == 0 {
            self.scroll_offset = 0;
            cx.notify();
            return;
        }
        let total_pixels = self.color_mode.total_pixels(active_len);
        let total_rows = total_pixels.div_ceil(self.cols);
        let cursor_row = VisualMapGeometry::cursor_row(cursor_offset, self.header_offset, self.cols, self.color_mode);
        let visible_rows = if let Some(bounds) = self.last_bounds.get() {
            (bounds.size.height.as_f32() / self.pixel_size as f32).floor() as usize
        } else {
            30
        };
        let max_offset = total_rows.saturating_sub(visible_rows.max(1));
        let target_scroll = cursor_row.saturating_sub(visible_rows / 2);
        self.scroll_offset = cmp::min(target_scroll, max_offset);
        cx.notify();
    }

    fn update_scrollbar(&mut self, cx: &App) {
        let active_len = self.active_buffer_len(cx);
        let total_pixels = self.color_mode.total_pixels(active_len);
        let total_rows = total_pixels.div_ceil(self.cols);
        let max_offset = if let Some(bounds) = self.last_bounds.get() {
            let visible_rows = (bounds.size.height.as_f32() / self.pixel_size as f32).floor() as usize;
            total_rows.saturating_sub(visible_rows.max(1))
        } else {
            total_rows.saturating_sub(1)
        };
        self.scroll_offset = self.scroll_offset.min(max_offset);
    }

    fn update_scrollbar_drag(&mut self, current_y: f32, cx: &mut Context<Self>) {
        if !self.is_dragging_scrollbar {
            return;
        }

        let delta_y = current_y - self.scrollbar_drag_start_y;
        let active_len = self.active_buffer_len(cx);
        if active_len == 0 {
            return;
        }
        let total_pixels = self.color_mode.total_pixels(active_len);
        let total_rows = total_pixels.div_ceil(self.cols);
        let row_height = self.pixel_size as f32;
        let list_h = self.last_bounds.get().map(|b| f32::from(b.size.height)).unwrap_or(600.0);

        if let Some(geom) = calculate_scrollbar_geometry(list_h, self.scroll_offset, total_rows, row_height) {
            let max_thumb_top = (list_h - geom.thumb_height).max(0.0);
            if max_thumb_top > 0.0 && geom.max_top_row > 0 {
                let delta_ratio = delta_y as f64 / max_thumb_top as f64;
                let delta_rows = delta_ratio * geom.max_top_row as f64;
                let new_row = ((self.scrollbar_drag_start_row as f64 + delta_rows).round() as isize).clamp(0, geom.max_top_row as isize) as usize;
                if self.scroll_offset != new_row {
                    self.scroll_offset = new_row;
                    cx.notify();
                }
            }
        }
    }

    fn on_scroll_wheel(&mut self, event: &ScrollWheelEvent, _window: &mut Window, cx: &mut Context<Self>) {
        if event.modifiers.platform || event.modifiers.control {
            let delta = event.delta.pixel_delta(px(16.0)).y.as_f32();
            if delta.abs() > 0.5 {
                let current = self.pixel_size;
                let new_size = if delta > 0.0 {
                    match current {
                        1 => 2,
                        2 => 4,
                        4 => 8,
                        _ => 8,
                    }
                } else {
                    match current {
                        8 => 4,
                        4 => 2,
                        2 => 1,
                        _ => 1,
                    }
                };
                if new_size != self.pixel_size {
                    self.pixel_size = new_size;
                    self.update_scrollbar(cx);
                    self.cached_image.borrow_mut().take();
                    cx.notify();
                }
            }
            return;
        }

        let pixel_size_px = px(self.pixel_size as f32);
        let active_len = self.active_buffer_len(cx);
        if active_len == 0 {
            return;
        }
        let total_pixels = self.color_mode.total_pixels(active_len);
        let total_rows = total_pixels.div_ceil(self.cols);
        let visible_rows = if let Some(bounds) = self.last_bounds.get() {
            (bounds.size.height.as_f32() / pixel_size_px.as_f32()).floor() as usize
        } else {
            30
        };
        let max_offset = total_rows.saturating_sub(visible_rows.max(1)) as i32;

        let delta_y_pixels = event.delta.pixel_delta(pixel_size_px).y.as_f32();
        let total_delta = delta_y_pixels + self.scroll_remainder;
        let delta_rows = (total_delta / pixel_size_px.as_f32()) as i32;
        self.scroll_remainder = total_delta - (delta_rows as f32 * pixel_size_px.as_f32());

        let new_scroll_offset = self.scroll_offset as i32 - delta_rows;

        self.scroll_offset = cmp::max(0, cmp::min(new_scroll_offset, max_offset)) as usize;
        cx.notify();
    }

    fn offset_from_point_clamped(&self, point: Point<Pixels>, cx: &App) -> Option<usize> {
        let bounds = self.last_bounds.get()?;
        let rel_x = (point.x - bounds.left()).max(px(0.)).min(bounds.size.width - px(1.)).as_f32();
        let rel_y = (point.y - bounds.top()).max(px(0.)).min(bounds.size.height - px(1.)).as_f32();

        VisualMapGeometry::point_to_offset(
            rel_x,
            rel_y,
            self.pixel_size as f32,
            self.scroll_offset,
            self.cols,
            self.header_offset,
            self.color_mode,
            self.buffer_len(cx),
        )
    }

    fn on_mouse_down(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_handle.focus(window, cx);

        // Scrollbar hit-testing on right edge
        if let Some(bounds) = self.last_bounds.get() {
            let click_x = f32::from(event.position.x);
            let bar_w = f32::from(SCROLLBAR_WIDTH);
            let bar_x = f32::from(bounds.right()) - bar_w;
            if click_x >= bar_x && click_x <= f32::from(bounds.right()) && event.position.y >= bounds.top() && event.position.y <= bounds.bottom() {
                let active_len = self.active_buffer_len(cx);
                if active_len > 0 {
                    let total_pixels = self.color_mode.total_pixels(active_len);
                    let total_rows = total_pixels.div_ceil(self.cols);
                    let row_height = self.pixel_size as f32;
                    let list_h = f32::from(bounds.size.height);
                    if let Some(geom) = calculate_scrollbar_geometry(list_h, self.scroll_offset, total_rows, row_height) {
                        let click_y = f32::from(event.position.y);
                        let rel_y = click_y - f32::from(bounds.top());
                        let cur_thumb_top = geom.thumb_top;
                        let thumb_h = geom.thumb_height;
                        let max_thumb_top = (list_h - thumb_h).max(0.0);

                        if rel_y >= cur_thumb_top && rel_y <= cur_thumb_top + thumb_h {
                            self.is_dragging_scrollbar = true;
                            self.scrollbar_drag_start_y = click_y;
                            self.scrollbar_drag_start_row = self.scroll_offset;
                        } else {
                            let target_thumb_top = (rel_y - thumb_h / 2.0).clamp(0.0, max_thumb_top);
                            let new_ratio = if max_thumb_top > 0.0 {
                                target_thumb_top as f64 / max_thumb_top as f64
                            } else {
                                0.0
                            };
                            let new_row = (new_ratio * geom.max_top_row as f64).round() as usize;
                            self.scroll_offset = new_row;
                            self.is_dragging_scrollbar = true;
                            self.scrollbar_drag_start_y = click_y;
                            self.scrollbar_drag_start_row = new_row;
                        }
                        cx.notify();
                        return;
                    }
                }
            }
        }

        self.is_dragging = true;
        if let Some(offset) = self.offset_from_point_clamped(event.position, cx)
            && let Some(editor) = &self.editor
        {
            editor.update(cx, |ed, _cx| {
                ed.set_cursor_offset(offset);
                ed.clear_selection();
            });
            cx.notify();
        }
    }

    fn on_mouse_up(&mut self, _event: &MouseUpEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.is_dragging = false;
        if self.is_dragging_scrollbar {
            self.is_dragging_scrollbar = false;
            cx.notify();
        }
    }

    fn on_mouse_move(&mut self, event: &MouseMoveEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let buffer_len = self.buffer_len(cx);
        if buffer_len == 0 {
            return;
        }

        if let Some(bounds) = self.last_bounds.get() {
            let pos = event.position;
            let bar_w = SCROLLBAR_WIDTH;
            let is_in_bar = pos.x >= bounds.right() - bar_w && pos.x <= bounds.right() && pos.y >= bounds.top() && pos.y <= bounds.bottom();
            if self.scrollbar_hovered != is_in_bar {
                self.scrollbar_hovered = is_in_bar;
                cx.notify();
            }
        }

        if self.is_dragging_scrollbar {
            self.update_scrollbar_drag(f32::from(event.position.y), cx);
            return;
        }

        if self.is_dragging
            && let Some(offset) = self.offset_from_point_clamped(event.position, cx)
            && let Some(editor) = &self.editor
        {
            editor.update(cx, |editor, cx| {
                editor.set_cursor_offset(offset);
                cx.notify();
            });
        }

        let mut hovered = None;
        if !self.scrollbar_hovered
            && let Some(bounds) = self.last_bounds.get()
            && bounds.contains(&event.position)
        {
            let rel_x = event.position.x - bounds.left();
            let rel_y = event.position.y - bounds.top();

            let col = (rel_x.as_f32() / self.pixel_size as f32) as usize;
            if col < self.cols {
                let row = (rel_y.as_f32() / self.pixel_size as f32) as usize + self.scroll_offset;

                if let Some(editor) = &self.editor
                    && let Ok(doc) = editor.read(cx).document.read()
                {
                    hovered = decode_hovered_pixel(doc.buffer.data(), row, col, self.cols, self.header_offset, self.color_mode, self.is_big_endian);
                }
            }
        }

        if self.hovered_info != hovered {
            self.hovered_info = hovered;
            cx.notify();
        }
    }

    pub(crate) fn increment_width(&mut self, cx: &mut Context<Self>) {
        let step = if self.color_mode == ColorMode::Planar4bpp { 8 } else { 1 };
        if self.cols < 4096 {
            self.cols = cmp::min(4096, self.cols.saturating_add(step));
            self.cached_image.borrow_mut().take();
            if self.editor.is_some() {
                self.scroll_to_cursor(cx);
            } else {
                self.update_scrollbar(cx);
            }
            cx.notify();
        }
    }

    pub(crate) fn decrement_width(&mut self, cx: &mut Context<Self>) {
        let min_cols = if self.color_mode == ColorMode::Planar4bpp { 8 } else { 1 };
        let step = if self.color_mode == ColorMode::Planar4bpp { 8 } else { 1 };
        if self.cols > min_cols {
            self.cols = cmp::max(min_cols, self.cols.saturating_sub(step));
            let max_offset = self.max_header_offset();
            if self.header_offset > max_offset {
                self.header_offset = max_offset;
            }
            self.cached_image.borrow_mut().take();
            if self.editor.is_some() {
                self.scroll_to_cursor(cx);
            } else {
                self.update_scrollbar(cx);
            }
            cx.notify();
        }
    }

    pub(crate) fn increment_header_offset(&mut self, cx: &mut Context<Self>) {
        let max = self.max_header_offset();
        if self.header_offset < max {
            self.header_offset = self.header_offset.saturating_add(1).min(max);
            self.cached_image.borrow_mut().take();
            self.update_scrollbar(cx);
            cx.notify();
        }
    }

    pub(crate) fn decrement_header_offset(&mut self, cx: &mut Context<Self>) {
        if self.header_offset > 0 {
            self.header_offset = self.header_offset.saturating_sub(1);
            self.cached_image.borrow_mut().take();
            self.update_scrollbar(cx);
            cx.notify();
        }
    }

    pub fn set_header_offset(&mut self, offset: usize, cx: &mut Context<Self>) {
        let clamped = offset.min(self.max_header_offset());
        if self.header_offset != clamped {
            self.header_offset = clamped;
            self.cached_image.borrow_mut().take();
            self.update_scrollbar(cx);
            cx.notify();
        }
    }
}

impl Focusable for VisualMapPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for VisualMapPanel {
    fn title(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let path = self.file_path(cx);
        let name = path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "(untitled)".to_string());
        let title = format!("2D Map: {}", name);
        let theme = cx.theme();

        h_flex().gap_2().items_center().child(title).child(
            div()
                .id("close-icon")
                .cursor_pointer()
                .rounded_md()
                .hover(|style| style.bg(theme.accent).text_color(theme.accent_foreground))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.focus_handle.focus(window, cx);
                    window.dispatch_action(Box::new(crate::actions::CloseActivePanel), cx);
                }))
                .child(Icon::new(IconName::Close).size(px(14.0))),
        )
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        Some("Visual Map".into())
    }

    fn zoom_control(&self, _cx: &App) -> Option<gpui_kit::component::dock::PanelControl> {
        None
    }

    fn inner_padding(&self, _cx: &App) -> bool {
        false
    }
}

impl gpui_kit::base::dock::Panel for VisualMapPanel {
    fn panel_name(&self) -> &'static str {
        "VisualMapPanel"
    }

    fn closable(&self, _cx: &App) -> bool {
        true
    }

    fn zoomable(&self, _cx: &App) -> bool {
        false
    }

    fn visible(&self, _cx: &App) -> bool {
        true
    }

    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if active {
            self.focus_handle.focus(window, cx);
        }
    }

    fn set_zoomed(&mut self, _zoomed: bool, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn dump(&self, cx: &App) -> gpui_kit::component::dock::PanelState {
        let mut state = gpui_kit::component::dock::PanelState::new(self.panel_name());
        let path = self.file_path(cx).unwrap_or_default();
        let map_state = VisualMapPanelState {
            path,
            cols: self.cols,
            pixel_size: self.pixel_size,
            color_mode: self.color_mode,
            entropy_window: self.entropy_window,
            is_big_endian: self.is_big_endian,
        };
        state.info = gpui_kit::component::dock::PanelInfo::panel(serde_json::to_value(map_state).expect("serialize VisualMapPanelState"));
        state
    }
}

impl Render for VisualMapPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let is_focused = self.focus_handle.is_focused(window);

        let header_actions = self.editor.as_ref().map(|_| {
            Button::new("center_cursor")
                .ghost()
                .with_size(Size::XSmall)
                .icon(IconName::ScanEye)
                .tooltip("Center on Cursor")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.scroll_to_cursor(cx);
                }))
                .into_any_element()
        });

        let header = crate::ui::panels::panel_header("2D VISUAL MAP", is_focused, &theme, None, header_actions);

        let editor = match &self.editor {
            Some(ed) => ed,
            None => {
                let container = crate::ui::panels::panel_container(is_focused, &theme);

                return container
                    .id("visual-map-panel")
                    .track_focus(&self.focus_handle)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, window, cx| {
                            this.focus_handle.focus(window, cx);
                        }),
                    )
                    .child(header)
                    .child(div().flex_1().min_h_0().w_full().overflow_hidden().child(crate::ui::panels::panel_empty_state(
                        IconName::Map,
                        "No Active File",
                        Some("Open a binary file to visualize byte distribution"),
                        None,
                        &theme,
                    )));
            }
        };

        let buffer_len = self.buffer_len(cx);
        let active_len = self.active_buffer_len(cx);
        let state_id = self.state_id(cx);
        let total_pixels = self.color_mode.total_pixels(active_len);
        let total_rows = total_pixels.div_ceil(self.cols);

        let toolbar = self.render_toolbar(&theme, cx);
        let legend = legend::render_legend(self.color_mode, &theme);
        let footer = footer::render_footer(
            self.hovered_info,
            self.editor.as_ref(),
            buffer_len,
            total_rows,
            self.cols,
            self.pixel_size,
            self.header_offset,
            self.color_mode,
            self.is_big_endian,
            self.entropy_window,
            &theme,
            cx,
        );

        let ed_ref = editor.read(cx);
        let cursor_offset = ed_ref.cursor.offset.checked_sub(self.header_offset);
        let selection_range = ed_ref.selection_range().and_then(|sel| {
            if sel.end > self.header_offset {
                let start = sel.start.saturating_sub(self.header_offset);
                let end = sel.end - self.header_offset;
                Some(start..end)
            } else {
                None
            }
        });
        let hovered_pixel = self.hovered_info.and_then(|hov| {
            let hov_offset = hov.offset();
            let sub_idx = match hov {
                HoveredPixel::SubByte { sub_idx, .. } => Some(sub_idx),
                _ => None,
            };
            let tile_coord = match hov {
                HoveredPixel::PlanarTile {
                    tile_idx,
                    in_tile_x,
                    in_tile_y,
                    ..
                } => Some((tile_idx, in_tile_x, in_tile_y)),
                _ => None,
            };
            VisualMapGeometry::hovered_to_linear_pixel(hov_offset, self.header_offset, self.cols, self.color_mode, sub_idx, tile_coord)
        });

        let canvas = div()
            .flex_1()
            .relative()
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .child(VisualMapElement {
                panel: cx.entity().downgrade(),
                document: editor.read(cx).document.clone(),
                cols: self.cols,
                pixel_size: self.pixel_size,
                scroll_offset: self.scroll_offset,
                color_mode: self.color_mode,
                entropy_window: self.entropy_window,
                is_big_endian: self.is_big_endian,
                header_offset: self.header_offset,
                state_id,
                cursor_offset,
                selection_range,
                hovered_pixel,
                is_dragging_scrollbar: self.is_dragging_scrollbar,
                scrollbar_hovered: self.scrollbar_hovered,
            });

        let container = crate::ui::panels::panel_container(is_focused, &theme);

        container
            .id("visual-map-panel")
            .track_focus(&self.focus_handle)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    this.focus_handle.focus(window, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if let Some(bounds) = this.last_bounds.get()
                    && !bounds.contains(&event.position)
                {
                    let mut changed = false;
                    if this.scrollbar_hovered {
                        this.scrollbar_hovered = false;
                        changed = true;
                    }
                    if this.hovered_info.is_some() {
                        this.hovered_info = None;
                        changed = true;
                    }
                    if changed {
                        cx.notify();
                    }
                }
            }))
            .child(header)
            .child(toolbar)
            .child(canvas)
            .children(legend)
            .child(footer)
    }
}

impl crate::ui::pane::WorkspaceTab for Entity<VisualMapPanel> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn title(&self, _cx: &App) -> String {
        "Visual Map".to_string()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn render(&self) -> AnyElement {
        self.clone().into_any_element()
    }

    fn create_split(&self, _window: &mut Window, cx: &mut App) -> Option<crate::ui::pane::TabContent> {
        let (ed, cols, pixel_size, header_offset, color_mode, is_big_endian, entropy_window) = {
            let panel = self.read(cx);
            (
                panel.editor.clone(),
                panel.cols,
                panel.pixel_size,
                panel.header_offset,
                panel.color_mode,
                panel.is_big_endian,
                panel.entropy_window,
            )
        };
        let new_vm = cx.new(|cx| {
            let mut panel = VisualMapPanel::new(ed, cx);
            panel.cols = cols;
            panel.pixel_size = pixel_size;
            panel.header_offset = header_offset;
            panel.color_mode = color_mode;
            panel.is_big_endian = is_big_endian;
            panel.entropy_window = entropy_window;
            panel
        });
        Some(crate::ui::pane::TabContent::new(new_vm))
    }
}

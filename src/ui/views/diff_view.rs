use crate::core::diff::{DiffChunk, DiffResult};
use crate::core::document::Document;
use crate::ui::icon::IconName;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dock::{Panel, PanelEvent};
use gpui_kit::prelude::*;
use gpui_kit::*;
use std::sync::{Arc, RwLock};

use crate::actions::{NextDifference, PrevDifference, RefreshDiff, SwapDiffFiles, ToggleSyncScroll};
use crate::app_state::AppState;
use crate::core::appearance::Appearance;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::ui::views::hex_view::{HexView, HexViewEvent, HorizontalScrollTarget, ScrollColumn};

const CONTEXT: &str = "DiffView";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("f3", NextDifference, Some(CONTEXT)),
        KeyBinding::new("shift-f3", PrevDifference, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-l", ToggleSyncScroll, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-l", ToggleSyncScroll, Some(CONTEXT)),
    ]);
}

pub struct DiffView {
    pub left_document: Arc<RwLock<Document>>,
    pub right_document: Arc<RwLock<Document>>,
    left_view: Entity<HexView>,
    right_view: Entity<HexView>,
    diff_result: Option<DiffResult>,
    current_diff_index: usize,
    focus_handle: FocusHandle,
    sync_scroll: bool,
    is_syncing: bool,
    _subscriptions: Vec<Subscription>,
}

impl DiffView {
    pub fn new(left_document: Arc<RwLock<Document>>, right_document: Arc<RwLock<Document>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let default_encoding = *cx.global::<Encoding>();
        let bytes_per_row = cx.global::<crate::core::layout::BytesPerRow>().0;
        let left_editor = cx.new(|_cx| {
            let mut editor = Editor::new(left_document.clone());
            editor.set_encoding(default_encoding);
            editor.set_bytes_per_row(bytes_per_row);
            editor
        });
        let right_editor = cx.new(|_cx| {
            let mut editor = Editor::new(right_document.clone());
            editor.set_encoding(default_encoding);
            editor.set_bytes_per_row(bytes_per_row);
            editor
        });
        let appearance = cx.global::<Appearance>().clone();
        let left_view = cx.new(|cx| {
            HexView::new(left_editor.clone(), window, cx)
                .font_family(appearance.font_family.clone())
                .font_size(px(appearance.font_size))
        });
        let right_view = cx.new(|cx| {
            HexView::new(right_editor.clone(), window, cx)
                .font_family(appearance.font_family.clone())
                .font_size(px(appearance.font_size))
        });

        let focus_handle = cx.focus_handle();

        let left_focus_handle = left_view.read(cx).focus_handle(cx);
        cx.on_focus_in(&focus_handle, window, {
            let left_focus_handle = left_focus_handle.clone();
            let focus_handle = focus_handle.clone();
            move |_, window, cx| {
                if window.focused(cx).as_ref() == Some(&focus_handle) {
                    left_focus_handle.focus(window, cx);
                }
            }
        })
        .detach();

        let mut subscriptions = Vec::new();

        subscriptions.push(cx.subscribe_in(&left_view, window, |this, _left_view, event, _window, cx| {
            if this.sync_scroll && !this.is_syncing {
                this.is_syncing = true;
                match event {
                    HexViewEvent::Scrolled(offset) => {
                        this.right_view.update(cx, |view, cx| {
                            view.scroll_to_row(*offset, cx);
                        });
                    }
                    HexViewEvent::HorizontalScrolled { target, progress }
                        if matches!(target, HorizontalScrollTarget::View | HorizontalScrollTarget::Column(ScrollColumn::Hex)) =>
                    {
                        this.right_view.update(cx, |view, cx| {
                            view.set_horizontal_scroll(*target, *progress, cx);
                        });
                    }
                    _ => {}
                }
                this.is_syncing = false;
            }
        }));

        subscriptions.push(cx.subscribe_in(&right_view, window, |this, _right_view, event, _window, cx| {
            if this.sync_scroll && !this.is_syncing {
                this.is_syncing = true;
                match event {
                    HexViewEvent::Scrolled(offset) => {
                        this.left_view.update(cx, |view, cx| {
                            view.scroll_to_row(*offset, cx);
                        });
                    }
                    HexViewEvent::HorizontalScrolled { target, progress }
                        if matches!(target, HorizontalScrollTarget::View | HorizontalScrollTarget::Column(ScrollColumn::Hex)) =>
                    {
                        this.left_view.update(cx, |view, cx| {
                            view.set_horizontal_scroll(*target, *progress, cx);
                        });
                    }
                    _ => {}
                }
                this.is_syncing = false;
            }
        }));

        subscriptions.push(cx.observe_global::<Appearance>(|this, cx| {
            let appearance = cx.global::<Appearance>();
            let font_family = appearance.font_family.clone();
            let font_size = appearance.font_size;
            this.left_view.update(cx, |view, cx| {
                view.set_font_family(font_family.clone(), cx);
                view.set_font_size(px(font_size), cx);
            });
            this.right_view.update(cx, |view, cx| {
                view.set_font_family(font_family, cx);
                view.set_font_size(px(font_size), cx);
            });
        }));

        let left_editor_clone = left_editor.clone();
        let right_editor_clone = right_editor.clone();
        subscriptions.push(cx.observe_global::<crate::core::layout::BytesPerRow>(move |this, cx| {
            let bytes_per_row = cx.global::<crate::core::layout::BytesPerRow>().0;
            left_editor_clone.update(cx, |editor, cx| {
                if editor.bytes_per_row() != bytes_per_row {
                    editor.set_bytes_per_row(bytes_per_row);
                    cx.notify();
                }
            });
            right_editor_clone.update(cx, |editor, cx| {
                if editor.bytes_per_row() != bytes_per_row {
                    editor.set_bytes_per_row(bytes_per_row);
                    cx.notify();
                }
            });
            this.left_view.update(cx, |_, cx| cx.notify());
            this.right_view.update(cx, |_, cx| cx.notify());
            cx.notify();
        }));

        Self {
            left_document,
            right_document,
            left_view,
            right_view,
            diff_result: None,
            current_diff_index: 0,
            focus_handle,
            sync_scroll: true,
            is_syncing: false,
            _subscriptions: subscriptions,
        }
    }

    pub fn set_diff_result(&mut self, result: DiffResult, cx: &mut Context<Self>) {
        self.diff_result = Some(result);
        self.current_diff_index = 0;
        self.update_highlights(cx);
    }

    fn update_highlights(&mut self, cx: &mut Context<Self>) {
        if let Some(diff_result) = &self.diff_result {
            let mut left_highlights = Vec::with_capacity(diff_result.chunks.len());
            let mut right_highlights = Vec::with_capacity(diff_result.chunks.len());

            for chunk in &diff_result.chunks {
                if let DiffChunk::Modified { offset, length } = chunk {
                    left_highlights.push(*offset..*offset + *length);
                    right_highlights.push(*offset..*offset + *length);
                }
            }

            self.left_view.update(cx, |view, cx| {
                view.set_highlight_ranges(left_highlights, cx);
            });

            self.right_view.update(cx, |view, cx| {
                view.set_highlight_ranges(right_highlights, cx);
            });
        }
    }

    fn next_difference(&mut self, _: &NextDifference, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(diff_result) = &self.diff_result {
            let modified_chunks: Vec<_> = diff_result.chunks.iter().filter(|c| matches!(c, DiffChunk::Modified { .. })).collect();

            if !modified_chunks.is_empty() {
                self.current_diff_index = (self.current_diff_index + 1) % modified_chunks.len();
                self.scroll_to_current_diff(cx);
            }
        }
    }

    fn prev_difference(&mut self, _: &PrevDifference, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(diff_result) = &self.diff_result {
            let modified_chunks: Vec<_> = diff_result.chunks.iter().filter(|c| matches!(c, DiffChunk::Modified { .. })).collect();

            if !modified_chunks.is_empty() {
                if self.current_diff_index == 0 {
                    self.current_diff_index = modified_chunks.len() - 1;
                } else {
                    self.current_diff_index -= 1;
                }
                self.scroll_to_current_diff(cx);
            }
        }
    }

    fn toggle_sync_scroll(&mut self, _: &ToggleSyncScroll, _window: &mut Window, cx: &mut Context<Self>) {
        self.sync_scroll = !self.sync_scroll;
        cx.notify();
    }

    pub fn swap_documents(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        std::mem::swap(&mut self.left_document, &mut self.right_document);
        std::mem::swap(&mut self.left_view, &mut self.right_view);
        self.refresh_diff(window, cx);
        cx.notify();
    }

    pub fn refresh_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let left_doc = self.left_document.clone();
        let right_doc = self.right_document.clone();
        let app = AppState::global(cx).clone();
        let task = app.diff_service.compute_diff(left_doc, right_doc, cx);
        let view = cx.entity().downgrade();
        cx.spawn_in(window, async move |_, window| {
            let result = task.await;
            let _ = window.update(|_, cx| {
                if let Some(view) = view.upgrade() {
                    view.update(cx, |this, cx| {
                        this.set_diff_result(result, cx);
                        cx.notify();
                    });
                }
            });
        })
        .detach();
    }

    fn on_action_swap_diff(&mut self, _: &SwapDiffFiles, window: &mut Window, cx: &mut Context<Self>) {
        self.swap_documents(window, cx);
    }

    fn on_action_refresh_diff(&mut self, _: &RefreshDiff, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_diff(window, cx);
    }

    fn scroll_to_current_diff(&mut self, cx: &mut Context<Self>) {
        if let Some(diff_result) = &self.diff_result {
            let modified_chunks: Vec<_> = diff_result.chunks.iter().filter(|c| matches!(c, DiffChunk::Modified { .. })).collect();

            if let Some(DiffChunk::Modified { offset, length }) = modified_chunks.get(self.current_diff_index) {
                let offset = *offset;
                let length = *length;
                self.left_view.update(cx, |view, cx| {
                    view.scroll_to_range_if_needed(offset..offset.saturating_add(length.max(1)), cx);
                });
                self.right_view.update(cx, |view, cx| {
                    view.scroll_to_range_if_needed(offset..offset.saturating_add(length.max(1)), cx);
                });
            }
        }
    }

    pub fn left_path(&self) -> std::path::PathBuf {
        self.left_document.read().expect("left document read lock").path().to_path_buf()
    }

    pub fn right_path(&self) -> std::path::PathBuf {
        self.right_document.read().expect("right document read lock").path().to_path_buf()
    }
}

impl EventEmitter<PanelEvent> for DiffView {}

impl Focusable for DiffView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for DiffView {
    fn title(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let left_name = self
            .left_path()
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or("Unknown".to_string());
        let right_name = self
            .right_path()
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or("Unknown".to_string());
        format!("Diff: {} ↔ {}", left_name, right_name)
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        let left_name = self
            .left_path()
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or("Unknown".to_string());
        let right_name = self
            .right_path()
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or("Unknown".to_string());
        Some(format!("Diff: {} ↔ {}", left_name, right_name).into())
    }

    fn zoom_control(&self, _cx: &App) -> Option<gpui_kit::component::dock::PanelControl> {
        None
    }

    fn inner_padding(&self, _cx: &App) -> bool {
        false
    }
}

impl gpui_kit::base::dock::Panel for DiffView {
    fn panel_name(&self) -> &'static str {
        "DiffView"
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

    fn dump(&self, _cx: &App) -> gpui_kit::component::dock::PanelState {
        let mut state = gpui_kit::component::dock::PanelState::new(self.panel_name());
        let diff_state = DiffViewState {
            left_path: self.left_path().to_string_lossy().to_string(),
            right_path: self.right_path().to_string_lossy().to_string(),
        };
        state.info = gpui_kit::component::dock::PanelInfo::panel(serde_json::to_value(diff_state).expect("serialize diff_state"));
        state
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct DiffViewState {
    pub left_path: String,
    pub right_path: String,
}

impl Render for DiffView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let sync_scroll = self.sync_scroll;
        let diff_count = self
            .diff_result
            .as_ref()
            .map(|r| r.chunks.iter().filter(|c| matches!(c, DiffChunk::Modified { .. })).count())
            .unwrap_or(0);
        let current_index = if diff_count > 0 { self.current_diff_index + 1 } else { 0 };
        let container = div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(theme.background)
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle);

        container
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .min_w_0()
                    .flex_shrink_0()
                    .gap_2()
                    .px_2()
                    .py_1()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        if sync_scroll {
                            Button::new("sync-scroll").icon(IconName::Check).primary().label("Sync")
                        } else {
                            Button::new("sync-scroll").icon(IconName::Minus).ghost().label("Sync")
                        }
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.sync_scroll = !this.sync_scroll;
                            cx.notify();
                        })),
                    )
                    .child(
                        Button::new("swap-diff")
                            .icon(IconName::GitCompare)
                            .ghost()
                            .label("Swap")
                            .tooltip("Swap Left and Right (⇄)")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.swap_documents(window, cx);
                            })),
                    )
                    .child(
                        Button::new("refresh-diff")
                            .icon(IconName::Redo)
                            .ghost()
                            .label("Refresh")
                            .tooltip("Recompute Diff (↻)")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.refresh_diff(window, cx);
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_sm()
                            .text_color(theme.muted_foreground)
                            .child(format!("Diff {}/{}", current_index, diff_count)),
                    )
                    .child(
                        Button::new("prev-diff")
                            .icon(IconName::ChevronUp)
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.prev_difference(&PrevDifference, window, cx);
                            })),
                    )
                    .child(
                        Button::new("next-diff")
                            .icon(IconName::ChevronDown)
                            .ghost()
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.next_difference(&NextDifference, window, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .h_full()
                            .overflow_hidden()
                            .border_r_1()
                            .border_color(theme.border)
                            .child(self.left_view.clone()),
                    )
                    .child(div().flex_1().min_w_0().min_h_0().h_full().overflow_hidden().child(self.right_view.clone())),
            )
            .on_action(cx.listener(Self::next_difference))
            .on_action(cx.listener(Self::prev_difference))
            .on_action(cx.listener(Self::toggle_sync_scroll))
            .on_action(cx.listener(Self::on_action_swap_diff))
            .on_action(cx.listener(Self::on_action_refresh_diff))
    }
}

impl crate::ui::pane::WorkspaceTab for Entity<DiffView> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn title(&self, cx: &App) -> String {
        let dp = self.read(cx);
        let left_name = dp
            .left_path()
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Left".to_string());
        let right_name = dp
            .right_path()
            .file_name()
            .and_then(|n| n.to_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "Right".to_string());
        format!("Diff: {} ↔ {}", left_name, right_name)
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn render(&self) -> AnyElement {
        self.clone().into_any_element()
    }

    fn create_split(&self, window: &mut Window, cx: &mut App) -> Option<crate::ui::pane::TabContent> {
        let (left_doc, right_doc) = {
            let dp = self.read(cx);
            (dp.left_document.clone(), dp.right_document.clone())
        };
        let new_diff = cx.new(|cx| DiffView::new(left_doc, right_doc, window, cx));
        Some(crate::ui::pane::TabContent::new(new_diff))
    }
}

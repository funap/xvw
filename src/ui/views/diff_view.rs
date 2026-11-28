use std::ops::Range;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::dock::{Panel, PanelEvent};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable, Size, StyledExt, Theme, WindowExt};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::actions::{
    Copy, Cut, GoToBeginning, GoToEnd, NextDifference, Paste, PrevDifference, Redo, RefreshDiff, SelectAll, SwapDiffFiles, ToggleEditColumn, ToggleSyncScroll,
    Undo,
};
use crate::app_state::{AppState, InsertModeState};
use crate::core::appearance::Appearance;
use crate::core::diff::{DiffChunk, DiffResult};
use crate::core::document::Document;
use crate::core::editor::Editor;
use crate::core::encoding::Encoding;
use crate::core::format::{HexCase, format_size_friendly};
use crate::core::radix::{ByteGroupSize, ByteOrder, DisplayRadix};
use crate::service::DocumentService;
use crate::ui::icon::IconName;
use crate::ui::notification;
use crate::ui::views::hex_view::{HexView, HexViewEvent, HorizontalScrollTarget, ScrollColumn};

const CONTEXT: &str = "DiffView";
const LIVE_DIFF_DEBOUNCE_MS: u64 = 60;
const PASSIVE_DIFF_BLEND_ALPHA: f32 = 0.35;

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

/// Releases a diff pane's editor registration from `DocumentService` on drop.
///
/// Stores the `Arc<RwLock<Document>>` rather than a static path so `Save As`
/// retargeting releases the current canonical path cleanly.
struct DiffDocumentLease {
    service: DocumentService,
    document: Arc<RwLock<Document>>,
    editor_id: EntityId,
}

impl Drop for DiffDocumentLease {
    fn drop(&mut self) {
        if let Ok(doc) = self.document.read() {
            self.service.release_editor(doc.path(), self.editor_id);
        }
    }
}

/// Blends `fg` over opaque `bg` at `alpha` and returns an opaque `Hsla` (`a = 1.0`).
///
/// Returning an opaque background ensures `HexView`'s WCAG contrast resolver calculates
/// exact luminance against the real composited highlight fill on both dark and light themes.
pub(crate) fn blend_opaque(bg: Hsla, fg: Hsla, alpha: f32) -> Hsla {
    let a = alpha.clamp(0.0, 1.0);
    let bg_rgba = Rgba::from(bg);
    let fg_rgba = Rgba::from(fg);
    Hsla::from(Rgba {
        r: bg_rgba.r * (1.0 - a) + fg_rgba.r * a,
        g: bg_rgba.g * (1.0 - a) + fg_rgba.g * a,
        b: bg_rgba.b * (1.0 - a) + fg_rgba.b * a,
        a: 1.0,
    })
}

/// Returns the number of modified diff chunks in `result`.
pub(crate) fn modified_chunk_count(result: &DiffResult) -> usize {
    result.chunks.iter().filter(|c| matches!(c, DiffChunk::Modified { .. })).count()
}

/// Returns `(offset, length)` for the `index`-th modified chunk in `result`.
pub(crate) fn modified_chunk_at(result: &DiffResult, index: usize) -> Option<(usize, usize)> {
    result
        .chunks
        .iter()
        .filter_map(|c| match c {
            DiffChunk::Modified { offset, length } => Some((*offset, *length)),
            DiffChunk::Equal { .. } => None,
        })
        .nth(index)
}

/// Clamps `current_index` to a valid modified-chunk index after live diff recomputation.
pub(crate) fn clamp_diff_index(current_index: usize, diff_count: usize) -> usize {
    if diff_count == 0 { 0 } else { current_index.min(diff_count - 1) }
}

/// Steps the active diff index forward or backward with wrap-around.
pub(crate) fn step_diff_index(current_index: usize, diff_count: usize, forward: bool) -> Option<usize> {
    if diff_count == 0 {
        return None;
    }
    let clamped = current_index.min(diff_count - 1);
    Some(if forward {
        (clamped + 1) % diff_count
    } else if clamped == 0 {
        diff_count - 1
    } else {
        clamped - 1
    })
}

/// Builds sorted highlight ranges from a `DiffResult`, highlighting the active chunk distinctly.
pub(crate) fn collect_diff_highlights(result: &DiffResult, current_index: usize, passive_color: Hsla, active_color: Hsla) -> Vec<(Range<usize>, Hsla)> {
    let mut highlights = Vec::new();
    let mut modified_idx = 0usize;
    for chunk in &result.chunks {
        if let DiffChunk::Modified { offset, length } = *chunk {
            let color = if modified_idx == current_index { active_color } else { passive_color };
            highlights.push((offset..offset.saturating_add(length), color));
            modified_idx += 1;
        }
    }
    highlights
}

/// Formats total differing bytes as a concise human-readable label.
pub(crate) fn format_diff_bytes_summary(total_differences: usize) -> String {
    match total_differences {
        0 => "Identical".to_string(),
        1 => "1 byte differs".to_string(),
        n if n < 1024 => format!("{n} bytes differ"),
        n => format!("{} differ", format_size_friendly(n)),
    }
}

/// Formats the byte-length difference between `this_len` and `other_len`, if any.
pub(crate) fn format_size_delta(this_len: usize, other_len: usize) -> Option<String> {
    if this_len == other_len {
        return None;
    }
    let diff = this_len.abs_diff(other_len);
    let sign = if this_len > other_len { '+' } else { '-' };
    let amount = if diff < 1024 { format!("{diff} B") } else { format_size_friendly(diff) };
    Some(format!("{sign}{amount}"))
}

fn file_display_name(doc: &Arc<RwLock<Document>>, fallback: &str) -> String {
    doc.read()
        .ok()
        .and_then(|d| d.path().file_name().and_then(|n| n.to_str()).map(|s| s.to_string()))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn document_state_id(doc: &Arc<RwLock<Document>>) -> usize {
    doc.read().map(|d| d.history.state_id()).unwrap_or(0)
}

#[cfg(target_os = "macos")]
const SYNC_SHORTCUT_HINT: &str = "⌘L";
#[cfg(not(target_os = "macos"))]
const SYNC_SHORTCUT_HINT: &str = "Ctrl+L";

#[cfg(target_os = "macos")]
const SAVE_SHORTCUT_HINT: &str = "⌘S";
#[cfg(not(target_os = "macos"))]
const SAVE_SHORTCUT_HINT: &str = "Ctrl+S";

pub struct DiffView {
    pub left_document: Arc<RwLock<Document>>,
    pub right_document: Arc<RwLock<Document>>,
    left_editor: Entity<Editor>,
    right_editor: Entity<Editor>,
    left_view: Entity<HexView>,
    right_view: Entity<HexView>,
    _left_lease: DiffDocumentLease,
    _right_lease: DiffDocumentLease,
    encoding: Encoding,
    last_focused_view_id: EntityId,
    last_state_ids: (usize, usize),
    diff_result: Option<DiffResult>,
    current_diff_index: usize,
    diff_generation: u64,
    is_computing: bool,
    diff_task: Option<Task<()>>,
    focus_handle: FocusHandle,
    sync_scroll: bool,
    is_syncing: bool,
    _subscriptions: Vec<Subscription>,
}

impl DiffView {
    pub fn new(left_document: Arc<RwLock<Document>>, right_document: Arc<RwLock<Document>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let default_encoding = *cx.global::<Encoding>();
        let default_radix = *cx.global::<DisplayRadix>();
        let default_group_size = *cx.global::<ByteGroupSize>();
        let default_byte_order = *cx.global::<ByteOrder>();
        let bytes_per_row = cx.global::<crate::core::layout::BytesPerRow>().0;
        let left_editor = cx.new(|_cx| {
            let mut editor = Editor::new(left_document.clone());
            editor.set_encoding(default_encoding);
            editor.set_radix(default_radix);
            editor.set_group_size(default_group_size);
            editor.set_is_big_endian(default_byte_order.is_big_endian());
            editor.set_bytes_per_row(bytes_per_row);
            editor
        });
        let right_editor = cx.new(|_cx| {
            let mut editor = Editor::new(right_document.clone());
            editor.set_encoding(default_encoding);
            editor.set_radix(default_radix);
            editor.set_group_size(default_group_size);
            editor.set_is_big_endian(default_byte_order.is_big_endian());
            editor.set_bytes_per_row(bytes_per_row);
            editor
        });

        let document_service = AppState::global(cx).document_service.clone();
        if let Ok(doc) = left_document.read() {
            document_service.register_editor(doc.path().to_path_buf(), left_editor.downgrade());
        }
        if let Ok(doc) = right_document.read() {
            document_service.register_editor(doc.path().to_path_buf(), right_editor.downgrade());
        }
        let left_lease = DiffDocumentLease {
            service: document_service.clone(),
            document: left_document.clone(),
            editor_id: left_editor.entity_id(),
        };
        let right_lease = DiffDocumentLease {
            service: document_service,
            document: right_document.clone(),
            editor_id: right_editor.entity_id(),
        };

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
        let right_focus_handle = right_view.read(cx).focus_handle(cx);
        let left_view_id = left_view.entity_id();
        let right_view_id = right_view.entity_id();
        let last_state_ids = (document_state_id(&left_document), document_state_id(&right_document));

        let mut subscriptions = Vec::new();

        subscriptions.push(cx.on_focus_in(&left_focus_handle, window, move |this, _, cx| {
            if this.last_focused_view_id != left_view_id {
                this.last_focused_view_id = left_view_id;
                cx.notify();
            }
        }));
        subscriptions.push(cx.on_focus_in(&right_focus_handle, window, move |this, _, cx| {
            if this.last_focused_view_id != right_view_id {
                this.last_focused_view_id = right_view_id;
                cx.notify();
            }
        }));
        subscriptions.push(cx.on_focus_in(&focus_handle, window, {
            let focus_handle = focus_handle.clone();
            move |this, window, cx| {
                if window.focused(cx).as_ref() == Some(&focus_handle) {
                    this.focus_active_side(window, cx);
                }
            }
        }));

        subscriptions.push(cx.observe(&left_editor, |this, _, cx| {
            this.on_editor_updated(cx);
        }));
        subscriptions.push(cx.observe(&right_editor, |this, _, cx| {
            this.on_editor_updated(cx);
        }));

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
                        if matches!(
                            target,
                            HorizontalScrollTarget::View | HorizontalScrollTarget::Column(ScrollColumn::Hex | ScrollColumn::Ascii)
                        ) =>
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
                        if matches!(
                            target,
                            HorizontalScrollTarget::View | HorizontalScrollTarget::Column(ScrollColumn::Hex | ScrollColumn::Ascii)
                        ) =>
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
            cx.notify();
        }));

        subscriptions.push(cx.observe_global::<HexCase>(|_, cx| {
            cx.notify();
        }));

        subscriptions.push(cx.observe_global::<Theme>(|this, cx| {
            this.update_highlights(cx);
            cx.notify();
        }));

        subscriptions.push(cx.observe_global::<crate::core::layout::BytesPerRow>(move |this, cx| {
            let bytes_per_row = cx.global::<crate::core::layout::BytesPerRow>().0;
            this.left_editor.update(cx, |editor, cx| {
                if editor.bytes_per_row() != bytes_per_row {
                    editor.set_bytes_per_row(bytes_per_row);
                    cx.notify();
                }
            });
            this.right_editor.update(cx, |editor, cx| {
                if editor.bytes_per_row() != bytes_per_row {
                    editor.set_bytes_per_row(bytes_per_row);
                    cx.notify();
                }
            });
            this.left_view.update(cx, |_, cx| cx.notify());
            this.right_view.update(cx, |_, cx| cx.notify());
            this.update_highlights(cx);
            cx.notify();
        }));

        Self {
            left_document,
            right_document,
            left_editor,
            right_editor,
            left_view,
            right_view,
            _left_lease: left_lease,
            _right_lease: right_lease,
            encoding: default_encoding,
            last_focused_view_id: left_view_id,
            last_state_ids,
            diff_result: None,
            current_diff_index: 0,
            diff_generation: 0,
            is_computing: true,
            diff_task: None,
            focus_handle,
            sync_scroll: true,
            is_syncing: false,
            _subscriptions: subscriptions,
        }
    }

    fn on_editor_updated(&mut self, cx: &mut Context<Self>) {
        let current_states = (document_state_id(&self.left_document), document_state_id(&self.right_document));
        if current_states != self.last_state_ids {
            self.last_state_ids = current_states;
            self.schedule_diff_recompute(true, true, cx);
        } else {
            cx.notify();
        }
    }

    fn schedule_diff_recompute(&mut self, debounce: bool, preserve_index: bool, cx: &mut Context<Self>) {
        self.diff_generation = self.diff_generation.wrapping_add(1);
        let generation = self.diff_generation;
        self.is_computing = true;

        let left_doc = self.left_document.clone();
        let right_doc = self.right_document.clone();
        let diff_service = AppState::global(cx).diff_service.clone();

        self.diff_task = Some(cx.spawn(async move |this, cx| {
            if debounce {
                cx.background_executor().timer(Duration::from_millis(LIVE_DIFF_DEBOUNCE_MS)).await;
            }
            let diff_task = cx.update(|cx| diff_service.compute_diff(left_doc, right_doc, cx));
            let result = diff_task.await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if this.diff_generation != generation {
                        return;
                    }
                    this.is_computing = false;
                    this.apply_diff_result(result, preserve_index, cx);
                });
            }
        }));
        cx.notify();
    }

    /// Returns `true` if the left diff pane is currently the active/focused side.
    pub fn is_left_focused(&self) -> bool {
        self.right_view.entity_id() != self.last_focused_view_id
    }

    /// Returns the `Editor` entity of whichever diff pane was most recently focused.
    pub fn active_side_editor(&self) -> Entity<Editor> {
        if self.is_left_focused() {
            self.left_editor.clone()
        } else {
            self.right_editor.clone()
        }
    }

    /// Returns the `HexView` entity of whichever diff pane was most recently focused.
    pub fn active_side_hex_view(&self) -> Entity<HexView> {
        if self.is_left_focused() {
            self.left_view.clone()
        } else {
            self.right_view.clone()
        }
    }

    /// Returns `true` if the left document has unsaved edits.
    pub fn is_left_dirty(&self) -> bool {
        self.left_document.read().map(|d| d.is_dirty()).unwrap_or(false)
    }

    /// Returns `true` if the right document has unsaved edits.
    pub fn is_right_dirty(&self) -> bool {
        self.right_document.read().map(|d| d.is_dirty()).unwrap_or(false)
    }

    /// Returns `true` if either left or right document has unsaved edits.
    pub fn is_any_dirty(&self) -> bool {
        self.is_left_dirty() || self.is_right_dirty()
    }

    /// Returns `true` if the left document is read-only.
    pub fn is_left_read_only(&self) -> bool {
        self.left_document.read().map(|d| d.is_read_only()).unwrap_or(false)
    }

    /// Returns `true` if the right document is read-only.
    pub fn is_right_read_only(&self) -> bool {
        self.right_document.read().map(|d| d.is_read_only()).unwrap_or(false)
    }

    /// Returns `true` if the currently focused diff side is read-only.
    pub fn is_active_side_read_only(&self) -> bool {
        if self.is_left_focused() {
            self.is_left_read_only()
        } else {
            self.is_right_read_only()
        }
    }

    /// Returns the formatted tab/panel title (`Diff: <left> ↔ <right>`).
    pub fn display_title(&self) -> String {
        let left_name = file_display_name(&self.left_document, "Left");
        let right_name = file_display_name(&self.right_document, "Right");
        format!("Diff: {left_name} ↔ {right_name}")
    }

    /// Returns the current text encoding used by both diff panes.
    pub fn encoding(&self) -> Encoding {
        self.encoding
    }

    /// Updates the text encoding for both diff panes.
    pub fn set_encoding(&mut self, encoding: Encoding, cx: &mut Context<Self>) {
        if self.encoding == encoding {
            return;
        }
        self.encoding = encoding;
        self.left_editor.update(cx, |editor, cx| {
            editor.set_encoding(encoding);
            cx.notify();
        });
        self.right_editor.update(cx, |editor, cx| {
            editor.set_encoding(encoding);
            cx.notify();
        });
        self.left_view.update(cx, |view, cx| {
            view.set_encoding(encoding, cx);
        });
        self.right_view.update(cx, |view, cx| {
            view.set_encoding(encoding, cx);
        });
        cx.notify();
    }

    /// Updates the display radix for both diff panes.
    pub fn set_radix(&mut self, radix: DisplayRadix, cx: &mut Context<Self>) {
        self.left_editor.update(cx, |editor, cx| {
            editor.set_radix(radix);
            cx.notify();
        });
        self.right_editor.update(cx, |editor, cx| {
            editor.set_radix(radix);
            cx.notify();
        });
        cx.notify();
    }

    /// Updates the byte grouping size for both diff panes.
    pub fn set_group_size(&mut self, group_size: ByteGroupSize, cx: &mut Context<Self>) {
        self.left_editor.update(cx, |editor, cx| {
            editor.set_group_size(group_size);
            cx.notify();
        });
        self.right_editor.update(cx, |editor, cx| {
            editor.set_group_size(group_size);
            cx.notify();
        });
        cx.notify();
    }

    /// Updates byte endianness for both diff panes.
    pub fn set_is_big_endian(&mut self, is_big_endian: bool, cx: &mut Context<Self>) {
        self.left_editor.update(cx, |editor, cx| {
            editor.set_is_big_endian(is_big_endian);
            cx.notify();
        });
        self.right_editor.update(cx, |editor, cx| {
            editor.set_is_big_endian(is_big_endian);
            cx.notify();
        });
        cx.notify();
    }

    /// Toggles byte endianness for both diff panes.
    pub fn toggle_byte_order(&mut self, cx: &mut Context<Self>) {
        let next_big_endian = !self.left_editor.read(cx).options.is_big_endian;
        self.set_is_big_endian(next_big_endian, cx);
    }

    /// Returns the focus handle of whichever diff pane (`left_view` or `right_view`) was most recently focused.
    pub fn active_side_focus_handle(&self, cx: &App) -> FocusHandle {
        self.active_side_hex_view().read(cx).focus_handle(cx)
    }

    /// Focuses whichever diff pane (`left_view` or `right_view`) was most recently focused.
    pub fn focus_active_side(&self, window: &mut Window, cx: &mut App) {
        let handle = self.active_side_focus_handle(cx);
        handle.focus(window, cx);
    }

    /// Focuses either the left (`is_left = true`) or right (`is_left = false`) pane.
    pub fn focus_side(&mut self, is_left: bool, window: &mut Window, cx: &mut Context<Self>) {
        let target_view = if is_left { &self.left_view } else { &self.right_view };
        self.last_focused_view_id = target_view.entity_id();
        let handle = target_view.read(cx).focus_handle(cx);
        handle.focus(window, cx);
        cx.notify();
    }

    /// Saves the document on the specified side (`is_left = true` for left, `false` for right).
    pub fn save_side(&mut self, is_left: bool, window: &mut Window, cx: &mut Context<Self>) {
        let (document, editor) = if is_left {
            (self.left_document.clone(), self.left_editor.clone())
        } else {
            (self.right_document.clone(), self.right_editor.clone())
        };

        let (state_id, is_read_only, path) = {
            let doc_read = document.read().expect("document read lock");
            (doc_read.history.state_id(), doc_read.is_read_only(), doc_read.path().to_path_buf())
        };

        if is_read_only {
            window.push_notification(
                notification::warning("Cannot save: document is in read-only mode. Toggle read-only mode or use Save As."),
                cx,
            );
            return;
        }

        let service = AppState::global(cx).document_service.clone();
        let diff_view = cx.entity().downgrade();

        if !path.exists() {
            let parent_dir = path
                .parent()
                .filter(|p| p.exists())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")));
            let default_name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "untitled.bin".to_string());
            let prompt = cx.prompt_for_new_path(&parent_dir, Some(&default_name));

            cx.spawn_in(window, async move |_, window| {
                let Some(new_path) = prompt.await.ok().and_then(|r| r.ok()).flatten() else {
                    return;
                };
                let Some(task) = window
                    .update(|_, cx| service.save_document_to_path(document.clone(), new_path.clone(), cx))
                    .ok()
                else {
                    return;
                };
                let result = task.await;
                let _ = window.update(|window, cx| match result {
                    Ok(()) => {
                        service.finalize_save_as_state(&document, new_path.clone(), state_id, cx);
                        let file_name = new_path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "document".into());
                        window.push_notification(notification::info(format!("Saved {file_name}")), cx);
                        editor.update(cx, |_, cx| cx.notify());
                        if let Some(view) = diff_view.upgrade() {
                            view.update(cx, |_, cx| cx.notify());
                        }
                    }
                    Err(error) => {
                        eprintln!("Failed to save diff document: {error}");
                        window.push_notification(notification::error(format!("Failed to save document: {error}")), cx);
                    }
                });
            })
            .detach();
            return;
        }

        let task = service.save_document(document.clone(), cx);
        cx.spawn_in(window, async move |_, window| {
            let result = task.await;
            let _ = window.update(|window, cx| match result {
                Ok(()) => {
                    let _ = service.finalize_save_state(&document, state_id, cx);
                    let file_name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "document".into());
                    window.push_notification(notification::info(format!("Saved {file_name}")), cx);
                    editor.update(cx, |_, cx| cx.notify());
                    if let Some(view) = diff_view.upgrade() {
                        view.update(cx, |_, cx| cx.notify());
                    }
                }
                Err(error) => {
                    eprintln!("Failed to save diff document: {error}");
                    window.push_notification(notification::error(format!("Failed to save document: {error}")), cx);
                }
            });
        })
        .detach();
    }

    pub fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.active_side_editor();
        editor.update(cx, |editor, cx| {
            editor.select_all();
            cx.notify();
        });
        self.focus_active_side(window, cx);
        cx.notify();
    }

    pub fn go_to_beginning(&mut self, _: &GoToBeginning, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self.active_side_editor();
        editor.update(cx, |editor, cx| {
            editor.go_to_beginning();
            cx.notify();
        });
        let cursor_offset = editor.read(cx).cursor.offset;
        let hex_view = self.active_side_hex_view();
        hex_view.update(cx, |view, cx| {
            view.scroll_to_byte(cursor_offset, cx);
        });
        self.focus_active_side(window, cx);
        cx.notify();
    }

    pub fn go_to_end(&mut self, _: &GoToEnd, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        let editor = self.active_side_editor();
        editor.update(cx, |editor, cx| {
            if insert_mode {
                let from = editor.cursor.offset;
                let total = editor.total_size();
                editor.set_cursor_offset_exact(total);
                editor.nav_history.record_jump(from, total);
            } else {
                editor.go_to_end();
            }
            cx.notify();
        });
        let cursor_offset = editor.read(cx).cursor.offset;
        let hex_view = self.active_side_hex_view();
        hex_view.update(cx, |view, cx| {
            view.scroll_to_byte(cursor_offset, cx);
        });
        self.focus_active_side(window, cx);
        cx.notify();
    }

    pub fn toggle_edit_column(&mut self, action: &ToggleEditColumn, window: &mut Window, cx: &mut Context<Self>) {
        let hv = self.active_side_hex_view();
        hv.update(cx, |hv, cx| hv.toggle_edit_column(action, window, cx));
    }

    pub fn copy(&mut self, action: &Copy, window: &mut Window, cx: &mut Context<Self>) {
        let hv = self.active_side_hex_view();
        hv.update(cx, |hv, cx| hv.copy(action, window, cx));
    }

    pub fn cut(&mut self, action: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        let hv = self.active_side_hex_view();
        hv.update(cx, |hv, cx| hv.cut(action, window, cx));
    }

    pub fn paste(&mut self, action: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let hv = self.active_side_hex_view();
        hv.update(cx, |hv, cx| hv.paste(action, window, cx));
    }

    pub fn undo(&mut self, action: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        let hv = self.active_side_hex_view();
        hv.update(cx, |hv, cx| hv.undo(action, window, cx));
    }

    pub fn redo(&mut self, action: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        let hv = self.active_side_hex_view();
        hv.update(cx, |hv, cx| hv.redo(action, window, cx));
    }

    pub fn set_diff_result(&mut self, result: DiffResult, cx: &mut Context<Self>) {
        self.is_computing = false;
        self.apply_diff_result(result, false, cx);
    }

    fn apply_diff_result(&mut self, result: DiffResult, preserve_index: bool, cx: &mut Context<Self>) {
        let count = modified_chunk_count(&result);
        self.current_diff_index = if preserve_index {
            clamp_diff_index(self.current_diff_index, count)
        } else {
            0
        };
        self.diff_result = Some(result);
        self.update_highlights(cx);
        cx.notify();
    }

    fn update_highlights(&mut self, cx: &mut Context<Self>) {
        let Some(diff_result) = &self.diff_result else {
            return;
        };

        let theme = cx.theme();
        let active_color = theme.danger;
        let passive_color = blend_opaque(theme.background, theme.danger, PASSIVE_DIFF_BLEND_ALPHA);
        let highlights = Arc::new(collect_diff_highlights(diff_result, self.current_diff_index, passive_color, active_color));

        self.left_view.update(cx, |view, cx| {
            view.set_highlights_arc(highlights.clone(), cx);
        });
        self.right_view.update(cx, |view, cx| {
            view.set_highlights_arc(highlights, cx);
        });
    }

    pub(crate) fn next_difference(&mut self, _: &NextDifference, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(diff_result) = &self.diff_result else {
            return;
        };
        let count = modified_chunk_count(diff_result);
        if let Some(next_idx) = step_diff_index(self.current_diff_index, count, true) {
            self.current_diff_index = next_idx;
            self.update_highlights(cx);
            self.scroll_to_current_diff(cx);
            cx.notify();
        }
    }

    pub(crate) fn prev_difference(&mut self, _: &PrevDifference, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(diff_result) = &self.diff_result else {
            return;
        };
        let count = modified_chunk_count(diff_result);
        if let Some(prev_idx) = step_diff_index(self.current_diff_index, count, false) {
            self.current_diff_index = prev_idx;
            self.update_highlights(cx);
            self.scroll_to_current_diff(cx);
            cx.notify();
        }
    }

    pub(crate) fn toggle_sync_scroll(&mut self, _: &ToggleSyncScroll, _window: &mut Window, cx: &mut Context<Self>) {
        self.sync_scroll = !self.sync_scroll;
        cx.notify();
    }

    pub fn swap_documents(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        std::mem::swap(&mut self.left_document, &mut self.right_document);
        std::mem::swap(&mut self.left_editor, &mut self.right_editor);
        std::mem::swap(&mut self.left_view, &mut self.right_view);
        std::mem::swap(&mut self._left_lease, &mut self._right_lease);
        self.last_state_ids = (self.last_state_ids.1, self.last_state_ids.0);
        self.schedule_diff_recompute(false, true, cx);
    }

    pub fn refresh_diff(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.last_state_ids = (document_state_id(&self.left_document), document_state_id(&self.right_document));
        self.schedule_diff_recompute(false, true, cx);
    }

    fn on_action_swap_diff(&mut self, _: &SwapDiffFiles, window: &mut Window, cx: &mut Context<Self>) {
        self.swap_documents(window, cx);
    }

    fn on_action_refresh_diff(&mut self, _: &RefreshDiff, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_diff(window, cx);
    }

    fn scroll_to_current_diff(&mut self, cx: &mut Context<Self>) {
        if let Some(diff_result) = &self.diff_result
            && let Some((offset, length)) = modified_chunk_at(diff_result, self.current_diff_index)
        {
            let range = offset..offset.saturating_add(length.max(1));
            self.left_view.update(cx, |view, cx| {
                view.scroll_to_range_if_needed(range.clone(), cx);
            });
            self.right_view.update(cx, |view, cx| {
                view.scroll_to_range_if_needed(range, cx);
            });
        }
    }

    pub fn left_path(&self) -> PathBuf {
        self.left_document.read().expect("left document read lock").path().to_path_buf()
    }

    pub fn right_path(&self) -> PathBuf {
        self.right_document.read().expect("right document read lock").path().to_path_buf()
    }

    fn render_pane_header(&self, is_left: bool, this_len: usize, other_len: usize, cx: &Context<Self>) -> impl IntoElement + use<> {
        let theme = cx.theme();
        let is_active = self.is_left_focused() == is_left;
        let doc = if is_left { &self.left_document } else { &self.right_document };

        let (file_name, full_path, is_dirty, is_read_only) = doc
            .read()
            .map(|d| {
                let path = d.path().to_path_buf();
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| if is_left { "Left".to_string() } else { "Right".to_string() });
                (name, path.display().to_string(), d.is_dirty(), d.is_read_only())
            })
            .unwrap_or_else(|_| {
                let fallback = if is_left { "Left" } else { "Right" }.to_string();
                (fallback.clone(), fallback, false, false)
            });

        let size_label = format_size_friendly(this_len);
        let size_delta = format_size_delta(this_len, other_len);
        let side_label = if is_left { "LEFT" } else { "RIGHT" };
        let header_id: SharedString = if is_left { "diff-header-left" } else { "diff-header-right" }.into();
        let save_id: SharedString = if is_left { "diff-save-left" } else { "diff-save-right" }.into();
        let name_id: SharedString = if is_left { "diff-name-left" } else { "diff-name-right" }.into();

        let header_bg = if is_active {
            theme.secondary.opacity(0.45)
        } else {
            theme.tab_bar.opacity(0.65)
        };
        let save_tooltip = format!("Save {file_name} ({SAVE_SHORTCUT_HINT})");
        let full_path_for_tooltip = full_path.clone();

        div()
            .id(header_id)
            .flex()
            .flex_row()
            .items_center()
            .gap_2()
            .h(px(28.0))
            .px_2p5()
            .flex_shrink_0()
            .bg(header_bg)
            .border_b_1()
            .border_color(if is_active { theme.primary.opacity(0.65) } else { theme.border })
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.focus_side(is_left, window, cx);
                }),
            )
            .child(
                div()
                    .px_1p5()
                    .py(px(1.0))
                    .rounded_sm()
                    .flex_shrink_0()
                    .text_size(px(10.0))
                    .font_semibold()
                    .bg(if is_active { theme.primary.opacity(0.18) } else { theme.muted.opacity(0.7) })
                    .text_color(if is_active { theme.primary } else { theme.muted_foreground })
                    .child(side_label),
            )
            .child(
                div()
                    .id(name_id)
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .flex_1()
                    .min_w_0()
                    .tooltip(move |_window, cx| cx.new(|_| Tooltip::new(full_path_for_tooltip.clone())).into())
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .when(is_active, |el| el.font_medium().text_color(theme.foreground))
                            .when(!is_active, |el| el.text_color(theme.muted_foreground))
                            .child(file_name),
                    )
                    .when(is_dirty, |el| el.child(div().size(px(6.0)).rounded_full().flex_shrink_0().bg(theme.warning)))
                    .when(is_read_only, |el| {
                        el.child(Icon::new(IconName::PenOff).size_3().flex_shrink_0().text_color(theme.muted_foreground))
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .flex_shrink_0()
                    .text_xs()
                    .child(div().text_color(theme.muted_foreground).child(size_label))
                    .when_some(size_delta, |el, delta| {
                        let is_larger = this_len > other_len;
                        el.child(
                            div()
                                .px_1()
                                .rounded_sm()
                                .text_size(px(10.0))
                                .font_medium()
                                .bg(if is_larger { theme.success.opacity(0.15) } else { theme.danger.opacity(0.15) })
                                .text_color(if is_larger { theme.success } else { theme.danger })
                                .child(delta),
                        )
                    })
                    .when(is_dirty && !is_read_only, |el| {
                        el.child(
                            Button::new(save_id)
                                .label("Save")
                                .primary()
                                .with_size(Size::XSmall)
                                .tooltip(save_tooltip)
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.save_side(is_left, window, cx);
                                })),
                        )
                    }),
            )
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
        self.display_title()
    }

    fn tab_name(&self, _cx: &App) -> Option<SharedString> {
        Some(self.display_title().into())
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
        let theme = cx.theme().clone();
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let font_family = cx.global::<Appearance>().font_family.clone();
        let sync_scroll = self.sync_scroll;
        let current_encoding = self.encoding;
        let diff_view_weak = cx.entity().downgrade();

        let left_len = self.left_document.read().map(|d| d.buffer.len()).unwrap_or(0);
        let right_len = self.right_document.read().map(|d| d.buffer.len()).unwrap_or(0);

        let diff_count = self.diff_result.as_ref().map(modified_chunk_count).unwrap_or(0);
        let total_diff_bytes = self.diff_result.as_ref().map(|r| r.total_differences).unwrap_or(0);
        let current_chunk = self.diff_result.as_ref().and_then(|r| modified_chunk_at(r, self.current_diff_index));
        let current_index = if diff_count > 0 { self.current_diff_index + 1 } else { 0 };
        let is_computing = self.is_computing || self.diff_result.is_none();

        let sync_tooltip = if sync_scroll {
            format!("Synchronized scrolling ON ({SYNC_SHORTCUT_HINT})")
        } else {
            format!("Synchronized scrolling OFF ({SYNC_SHORTCUT_HINT})")
        };

        let left_header = self.render_pane_header(true, left_len, right_len, cx);
        let right_header = self.render_pane_header(false, right_len, left_len, cx);

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
                    .gap_1p5()
                    .px_2()
                    .py_1()
                    .bg(theme.tab_bar)
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        if sync_scroll {
                            Button::new("sync-scroll").icon(IconName::Check).primary().small().label("Sync")
                        } else {
                            Button::new("sync-scroll").icon(IconName::Minus).ghost().small().label("Sync")
                        }
                        .tooltip(sync_tooltip)
                        .on_click(cx.listener(|this, _, _window, cx| {
                            this.sync_scroll = !this.sync_scroll;
                            cx.notify();
                        })),
                    )
                    .child(
                        Button::new("swap-diff")
                            .icon(IconName::GitCompare)
                            .ghost()
                            .small()
                            .label("Swap")
                            .tooltip("Swap Left and Right panes")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.swap_documents(window, cx);
                            })),
                    )
                    .child(
                        Button::new("refresh-diff")
                            .icon(IconName::Redo)
                            .ghost()
                            .small()
                            .label("Refresh")
                            .tooltip("Recompute differences")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.refresh_diff(window, cx);
                            })),
                    )
                    .child(div().w_px().h_4().bg(theme.border))
                    .child(
                        Button::new("diff-encoding")
                            .ghost()
                            .small()
                            .label(current_encoding.label())
                            .dropdown_caret(true)
                            .tooltip("Change text encoding for both panes")
                            .dropdown_menu(move |menu, window, cx| {
                                let menu = Encoding::primary_encodings().iter().copied().fold(menu, |menu, encoding| {
                                    let view = diff_view_weak.clone();
                                    menu.item(
                                        PopupMenuItem::new(encoding.label())
                                            .checked(encoding == current_encoding)
                                            .on_click(move |_, window, cx| {
                                                if let Some(view) = view.upgrade() {
                                                    view.update(cx, |this, cx| {
                                                        this.set_encoding(encoding, cx);
                                                        this.focus_active_side(window, cx);
                                                    });
                                                }
                                            }),
                                    )
                                });
                                let menu = menu.separator();
                                Encoding::secondary_categories().iter().fold(menu, |menu, (cat, encs)| {
                                    let view = diff_view_weak.clone();
                                    menu.submenu(cat.label(), window, cx, move |menu, _window, _cx| {
                                        encs.iter().copied().fold(menu, |menu, encoding| {
                                            let view = view.clone();
                                            menu.item(PopupMenuItem::new(encoding.label()).checked(encoding == current_encoding).on_click(
                                                move |_, window, cx| {
                                                    if let Some(view) = view.upgrade() {
                                                        view.update(cx, |this, cx| {
                                                            this.set_encoding(encoding, cx);
                                                            this.focus_active_side(window, cx);
                                                        });
                                                    }
                                                },
                                            ))
                                        })
                                    })
                                })
                            }),
                    )
                    .child(div().flex_1().min_w_0())
                    .child(if is_computing {
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .bg(theme.muted.opacity(0.6))
                            .text_xs()
                            .text_color(theme.muted_foreground)
                            .child(Icon::new(IconName::LoaderCircle).size_3())
                            .child("Comparing...")
                            .into_any_element()
                    } else if diff_count == 0 {
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .px_2()
                            .py_0p5()
                            .rounded_sm()
                            .bg(theme.success.opacity(0.15))
                            .text_xs()
                            .font_medium()
                            .text_color(theme.success)
                            .child(Icon::new(IconName::Check).size_3())
                            .child("Identical")
                            .into_any_element()
                    } else {
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .flex_shrink_0()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(format_diff_bytes_summary(total_diff_bytes)),
                            )
                            .when_some(current_chunk, |el, (offset, len)| {
                                let offset_str = hex_case.format_prefixed_pad(offset, 4);
                                let chunk_label = format!("@ {offset_str} ({len} B)");
                                el.child(
                                    div()
                                        .id("diff-current-offset-pill")
                                        .px_1p5()
                                        .py_0p5()
                                        .rounded_sm()
                                        .bg(theme.muted)
                                        .font_family(font_family)
                                        .text_xs()
                                        .text_color(theme.foreground)
                                        .cursor_pointer()
                                        .hover(|s| s.bg(theme.secondary))
                                        .tooltip(|_window, cx| cx.new(|_| Tooltip::new("Click to scroll to current difference")).into())
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _, _window, cx| {
                                                this.scroll_to_current_diff(cx);
                                            }),
                                        )
                                        .child(chunk_label),
                                )
                            })
                            .child(
                                div()
                                    .text_xs()
                                    .font_medium()
                                    .text_color(theme.foreground)
                                    .child(format!("Diff {current_index}/{diff_count}")),
                            )
                            .into_any_element()
                    })
                    .child(
                        Button::new("prev-diff")
                            .icon(IconName::ChevronUp)
                            .ghost()
                            .small()
                            .disabled(diff_count == 0)
                            .tooltip("Previous Difference (Shift+F3)")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.prev_difference(&PrevDifference, window, cx);
                            })),
                    )
                    .child(
                        Button::new("next-diff")
                            .icon(IconName::ChevronDown)
                            .ghost()
                            .small()
                            .disabled(diff_count == 0)
                            .tooltip("Next Difference (F3)")
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
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .h_full()
                            .overflow_hidden()
                            .border_r_1()
                            .border_color(theme.border)
                            .child(left_header)
                            .child(div().flex_1().min_w_0().min_h_0().overflow_hidden().child(self.left_view.clone())),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .h_full()
                            .overflow_hidden()
                            .child(right_header)
                            .child(div().flex_1().min_w_0().min_h_0().overflow_hidden().child(self.right_view.clone())),
                    ),
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
        self.read(cx).display_title()
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.read(cx).is_any_dirty()
    }

    fn is_read_only(&self, cx: &App) -> bool {
        self.read(cx).is_active_side_read_only()
    }

    fn documents(&self, cx: &App) -> Vec<(Arc<RwLock<Document>>, String)> {
        let dp = self.read(cx);
        vec![
            (dp.left_document.clone(), file_display_name(&dp.left_document, "Left")),
            (dp.right_document.clone(), file_display_name(&dp.right_document, "Right")),
        ]
    }

    fn icon(&self, _cx: &App) -> crate::ui::icon::IconName {
        crate::ui::icon::IconName::GitCompare
    }

    fn tooltip(&self, cx: &App) -> Option<String> {
        let dp = self.read(cx);
        Some(format!("{} ↔ {}", dp.left_path().display(), dp.right_path().display()))
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn render(&self) -> AnyElement {
        self.clone().into_any_element()
    }

    fn create_split(&self, window: &mut Window, cx: &mut App) -> Option<crate::ui::pane::TabContent> {
        let (left_doc, right_doc, encoding, sync_scroll, diff_result) = {
            let dp = self.read(cx);
            (
                dp.left_document.clone(),
                dp.right_document.clone(),
                dp.encoding(),
                dp.sync_scroll,
                dp.diff_result.clone(),
            )
        };
        let new_diff = cx.new(|cx| {
            let mut view = DiffView::new(left_doc, right_doc, window, cx);
            view.set_encoding(encoding, cx);
            view.sync_scroll = sync_scroll;
            if let Some(result) = diff_result {
                view.set_diff_result(result, cx);
            }
            view
        });
        Some(crate::ui::pane::TabContent::new(new_diff))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Hsla, Rgba, blend_opaque, clamp_diff_index, collect_diff_highlights, format_diff_bytes_summary, format_size_delta, hsla, modified_chunk_at,
        modified_chunk_count, step_diff_index,
    };
    use crate::core::diff::compute_simple_diff;

    #[test]
    fn test_step_and_clamp_diff_index() {
        assert_eq!(step_diff_index(0, 0, true), None);
        assert_eq!(step_diff_index(0, 0, false), None);

        assert_eq!(step_diff_index(0, 3, true), Some(1));
        assert_eq!(step_diff_index(2, 3, true), Some(0));
        assert_eq!(step_diff_index(0, 3, false), Some(2));
        assert_eq!(step_diff_index(2, 3, false), Some(1));
        assert_eq!(step_diff_index(99, 3, true), Some(0));

        assert_eq!(clamp_diff_index(5, 0), 0);
        assert_eq!(clamp_diff_index(5, 3), 2);
        assert_eq!(clamp_diff_index(1, 3), 1);
    }

    #[test]
    fn test_collect_diff_highlights_distinguishes_active_chunk() {
        let result = compute_simple_diff(b"A_B_C_D", b"A!B!C!D");
        assert_eq!(modified_chunk_count(&result), 3);
        assert_eq!(modified_chunk_at(&result, 0), Some((1, 1)));
        assert_eq!(modified_chunk_at(&result, 1), Some((3, 1)));
        assert_eq!(modified_chunk_at(&result, 2), Some((5, 1)));
        assert_eq!(modified_chunk_at(&result, 3), None);

        let passive = hsla(0.0, 0.5, 0.3, 1.0);
        let active = hsla(0.0, 0.9, 0.5, 1.0);
        let highlights = collect_diff_highlights(&result, 1, passive, active);

        assert_eq!(highlights, vec![(1..2, passive), (3..4, active), (5..6, passive)]);
    }

    #[test]
    fn test_format_diff_bytes_summary_and_size_delta() {
        assert_eq!(format_diff_bytes_summary(0), "Identical");
        assert_eq!(format_diff_bytes_summary(1), "1 byte differs");
        assert_eq!(format_diff_bytes_summary(42), "42 bytes differ");
        assert!(format_diff_bytes_summary(2048).ends_with(" differ"));

        assert_eq!(format_size_delta(100, 100), None);
        assert_eq!(format_size_delta(116, 100), Some("+16 B".to_string()));
        assert_eq!(format_size_delta(84, 100), Some("-16 B".to_string()));
    }

    #[test]
    fn test_blend_opaque_returns_opaque_color_between_bg_and_fg() {
        let bg = Hsla::from(Rgba {
            r: 0.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        });
        let fg = Hsla::from(Rgba {
            r: 1.0,
            g: 0.0,
            b: 0.0,
            a: 1.0,
        });
        let blended = blend_opaque(bg, fg, 0.5);
        let rgba = Rgba::from(blended);
        assert!((rgba.a - 1.0).abs() < 1e-4);
        assert!((rgba.r - 0.5).abs() < 1e-3);
        assert!(rgba.g.abs() < 1e-3);
        assert!(rgba.b.abs() < 1e-3);
    }
}

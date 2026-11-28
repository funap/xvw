use gpui_kit::base::dock::TabGroup;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{Panel, PanelEvent};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::notification::Notification;
use gpui_kit::component::{ActiveTheme, Sizable, WindowExt as _};
use gpui_kit::prelude::*;
use gpui_kit::{
    AnyElement, App, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable, Hsla, IntoElement, KeyBinding, SharedString, Subscription, Task,
    WeakEntity, Window, div, px,
};

use crate::actions::{
    AddCustomBreak, BookmarkBlue, BookmarkCyan, BookmarkGreen, BookmarkOrange, BookmarkPink, BookmarkPurple, BookmarkRed, BookmarkYellow, ClearAllBookmarks,
    ClearAllCustomBreaks, ClearBookmark, Copy, CopyAsBase64, CopyAsBinary, CopyAsCppArray, CopyAsEscapedString, CopyAsHexDump, CopyAsHexSpaces,
    CopyAsHexStream, CopyAsJsonArray, CopyAsPrintableText, CopyAsRustArray, Cut, FocusHexView, GoToBeginning, GoToEnd, HideAllBookmarks, JoinLine,
    NavigateBack, NavigateForward, Paste, Redo, RemoveCustomBreakBackward, RemoveCustomBreakForward, RemoveCustomBreaksInRange, SearchNext, SearchPrev,
    SelectAll, SelectToBeginning, SelectToEnd, ShowAllBookmarks, ShowOnlyBookmarkBlue, ShowOnlyBookmarkCyan, ShowOnlyBookmarkGreen, ShowOnlyBookmarkOrange,
    ShowOnlyBookmarkPink, ShowOnlyBookmarkPurple, ShowOnlyBookmarkRed, ShowOnlyBookmarkYellow, ToggleBookmarkBlue, ToggleBookmarkCyan, ToggleBookmarkGreen,
    ToggleBookmarkOrange, ToggleBookmarkPink, ToggleBookmarkPurple, ToggleBookmarkRed, ToggleBookmarkYellow, ToggleEditColumn, ToggleGoToAddress,
    ToggleHideUnbookmarked, ToggleReplace, ToggleSearch, Undo, UnfoldBookmarkAtCursor,
};
use crate::app_state::{AppState, InsertModeState};
use crate::core::appearance::Appearance;
use crate::core::editor::Editor;
use crate::core::format::HexCase;
use crate::core::search::{SearchMode, TextSearchFlags, compile_search, parse_replacement_bytes};
use crate::service::DocumentService;
use crate::service::search_service::LimitedSearchQuery;
use crate::ui::icon::IconName;
use crate::ui::views::hex_view::{self, HexView};
#[cfg(not(test))]
use highlights::compose_search_highlights;
#[cfg(test)]
pub(crate) use highlights::compose_search_highlights;
use highlights::{CachedSearchPattern, HighlightSyncState, scan_viewport_ranges, viewport_match_slice, viewport_range_slice};
use search_bar::{SearchMatchStatus, format_selection_for_find};
use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub mod goto_offset_bar;
mod highlights;
mod history;
mod search_bar;

#[cfg(test)]
mod tests;

pub use goto_offset_bar::{GotoBarEvent, GotoOffsetBar};
pub use search_bar::{SearchBar, SearchBarEvent};

const CONTEXT: &str = "EditorView";
const MAX_FIND_COUNT_RESULTS: usize = 10_000;

struct EditorDocumentLease {
    service: DocumentService,
    path: PathBuf,
    editor_id: EntityId,
}

impl Drop for EditorDocumentLease {
    fn drop(&mut self) {
        self.service.release_editor(&self.path, self.editor_id);
    }
}

pub fn init(cx: &mut App) {
    // Initialize HexView actions and keybindings
    hex_view::init(cx);
    goto_offset_bar::init(cx);
    search_bar::init(cx);

    // EditorView-scoped Find / Replace / Go to Address shortcuts so they work
    // regardless of whether HexView, SearchBar, or GotoOffsetBar has focus.
    cx.bind_keys([
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-f", ToggleSearch, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-f", ToggleSearch, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("alt-cmd-f", ToggleReplace, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-h", ToggleReplace, Some(CONTEXT)),
        KeyBinding::new("f3", SearchNext, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-g", SearchNext, Some(CONTEXT)),
        KeyBinding::new("shift-f3", SearchPrev, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-shift-g", SearchPrev, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-shift-g", SearchPrev, Some(CONTEXT)),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-l", ToggleGoToAddress, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-l", ToggleGoToAddress, Some(CONTEXT)),
        KeyBinding::new("ctrl-g", ToggleGoToAddress, Some(CONTEXT)),
    ]);
}

pub struct EditorView {
    editor: Entity<Editor>,
    focus_handle: FocusHandle,
    hex_view: Entity<HexView>,
    is_search_visible: bool,
    search_bar: Entity<SearchBar>,
    is_goto_visible: bool,
    goto_bar: Entity<GotoOffsetBar>,
    structure_reparse_task: Option<Task<()>>,
    find_count_task: Option<Task<()>>,
    find_count_cancel: Option<Arc<AtomicBool>>,
    find_count_ranges: Vec<Range<usize>>,
    find_count_truncated: bool,
    find_count_generation: usize,
    tab_group: Option<WeakEntity<TabGroup>>,
    last_highlight_sync: HighlightSyncState,
    cached_search_pattern: CachedSearchPattern,
    cached_find_pattern: CachedSearchPattern,
    _appearance_subscription: Subscription,
    _bytes_per_row_subscription: Subscription,
    _editor_subscription: Subscription,
    _document_lease: Option<EditorDocumentLease>,
}

impl EditorView {
    pub fn new(editor: Entity<Editor>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let appearance = cx.global::<Appearance>().clone();
        let hex_view = cx.new(|cx| {
            HexView::new(editor.clone(), window, cx)
                .font_family(appearance.font_family.clone())
                .font_size(px(appearance.font_size))
        });
        let search_bar = cx.new(|cx| SearchBar::new(window, cx));
        let goto_bar = cx.new(|cx| GotoOffsetBar::new(window, cx));

        cx.subscribe_in(&search_bar, window, |this, _, event: &SearchBarEvent, window, cx| match event {
            SearchBarEvent::IncrementalSearch { query, mode, flags } => {
                this.perform_incremental_search(query, *mode, *flags, cx);
            }
            SearchBarEvent::Next { query, mode, flags } => {
                this.sync_find_query_from_bar(query, *mode, *flags, cx);
                this.perform_search_next(cx);
            }
            SearchBarEvent::Prev { query, mode, flags } => {
                this.sync_find_query_from_bar(query, *mode, *flags, cx);
                this.perform_search_prev(cx);
            }
            SearchBarEvent::Replace {
                query,
                mode,
                flags,
                replacement,
            } => {
                this.perform_replace(query, *mode, *flags, replacement, window, cx);
            }
            SearchBarEvent::ReplaceAll {
                query,
                mode,
                flags,
                replacement,
            } => {
                this.perform_replace_all(query, *mode, *flags, replacement, window, cx);
            }
            SearchBarEvent::Dismiss => {
                this.dismiss_search(cx);
                cx.dispatch_action(&FocusHexView);
                cx.notify();
            }
        })
        .detach();

        cx.subscribe(&goto_bar, |this, _, event: &GotoBarEvent, cx| match event {
            GotoBarEvent::Jump { offset, extend_selection } => {
                let target = *offset;
                let extend = *extend_selection;
                this.is_goto_visible = false;
                this.update_top_overlay_state(cx);
                this.editor.update(cx, |editor, cx| {
                    editor.go_to_offset(target, extend);
                    cx.notify();
                });
                let cursor_offset = this.editor.read(cx).cursor.offset;
                this.hex_view.update(cx, |view, cx| {
                    view.scroll_to_byte_if_needed(cursor_offset, cx);
                });
                cx.dispatch_action(&FocusHexView);
                cx.notify();
            }
            GotoBarEvent::SelectRange { range } => {
                let r = range.clone();
                this.is_goto_visible = false;
                this.update_top_overlay_state(cx);
                this.editor.update(cx, |editor, cx| {
                    editor.go_to_range(r);
                    cx.notify();
                });
                let cursor_offset = this.editor.read(cx).cursor.offset;
                this.hex_view.update(cx, |view, cx| {
                    view.scroll_to_byte_if_needed(cursor_offset, cx);
                });
                cx.dispatch_action(&FocusHexView);
                cx.notify();
            }
            GotoBarEvent::Dismiss => {
                this.is_goto_visible = false;
                this.update_top_overlay_state(cx);
                cx.dispatch_action(&FocusHexView);
                cx.notify();
            }
        })
        .detach();

        let hex_focus_handle = hex_view.read(cx).focus_handle(cx);
        cx.on_focus_in(&focus_handle, window, {
            let hex_focus_handle = hex_focus_handle.clone();
            let focus_handle = focus_handle.clone();
            move |_, window, cx| {
                if window.focused(cx).as_ref() == Some(&focus_handle) {
                    hex_focus_handle.focus(window, cx);
                }
            }
        })
        .detach();

        cx.on_focus_in(&hex_focus_handle, window, |_, _, cx| {
            cx.notify();
        })
        .detach();

        // Subscribe to HexView scroll and escape events
        cx.subscribe(&hex_view, |this, _, event: &crate::ui::views::hex_view::HexViewEvent, cx| match event {
            crate::ui::views::hex_view::HexViewEvent::Scrolled(scroll_row) => {
                let current = this.capture_highlight_sync_state(this.editor.read(cx), Some(*scroll_row), cx);
                if current.needs_scroll_refresh(this.last_highlight_sync) {
                    this.update_highlights(cx);
                }
            }
            crate::ui::views::hex_view::HexViewEvent::EscapePressed => {
                let mut dismissed = false;
                if this.is_search_visible {
                    this.dismiss_search(cx);
                    dismissed = true;
                }
                if this.is_goto_visible {
                    this.is_goto_visible = false;
                    dismissed = true;
                }
                if dismissed {
                    this.update_top_overlay_state(cx);
                    this.update_highlights(cx);
                    cx.notify();
                }
            }
            _ => {}
        })
        .detach();

        let bytes_per_row = cx.global::<crate::core::layout::BytesPerRow>().0;
        editor.update(cx, |editor, _| {
            editor.set_bytes_per_row(bytes_per_row);
        });
        goto_bar.update(cx, |bar, cx| {
            bar.set_bytes_per_row(bytes_per_row, cx);
        });

        let _appearance_subscription = cx.observe_global::<Appearance>(|this, cx| {
            let appearance = cx.global::<Appearance>();
            let font_family = appearance.font_family.clone();
            let font_size = appearance.font_size;
            this.hex_view.update(cx, |this_hex_view, cx| {
                this_hex_view.set_font_family(font_family, cx);
                this_hex_view.set_font_size(px(font_size), cx);
            });
        });

        let _bytes_per_row_subscription = cx.observe_global::<crate::core::layout::BytesPerRow>(|this, cx| {
            let bytes_per_row = cx.global::<crate::core::layout::BytesPerRow>().0;
            this.editor.update(cx, |editor, cx| {
                if editor.bytes_per_row() != bytes_per_row {
                    editor.set_bytes_per_row(bytes_per_row);
                    cx.notify();
                }
            });
            this.goto_bar.update(cx, |bar, cx| {
                bar.set_bytes_per_row(bytes_per_row, cx);
            });
            this.hex_view.update(cx, |_, cx| {
                cx.notify();
            });
            this.update_highlights(cx);
            cx.notify();
        });

        let _editor_subscription = cx.observe(&editor, |this, editor, cx| {
            let (current, pending_reparse, is_read_only, find_generation) = {
                let ed = editor.read(cx);
                let scroll_top_row = Some(this.hex_view.read(cx).current_scroll_top_row());
                (
                    this.capture_highlight_sync_state(ed, scroll_top_row, cx),
                    ed.pending_structure_reparse(),
                    ed.is_read_only(),
                    ed.find_state.generation,
                )
            };

            this.search_bar.update(cx, |bar, cx| {
                bar.set_read_only(is_read_only, cx);
            });

            if this.is_search_visible {
                if find_generation != this.find_count_generation {
                    this.schedule_find_match_count(cx);
                } else if !this.find_count_ranges.is_empty() {
                    this.refresh_find_match_status(cx);
                }
            }

            if current.needs_editor_refresh(this.last_highlight_sync) {
                this.update_highlights(cx);
            } else {
                this.last_highlight_sync.cursor_offset = current.cursor_offset;
            }

            if let Some((_, generation)) = pending_reparse {
                this.schedule_structure_reparse(generation, cx);
            }

            cx.notify();
        });

        // Register the editor as a document lease. The lease removes the
        // cached document only after the last split/tab for that path is
        // dropped, so closing one duplicate tab keeps the shared state alive.
        let document_lease = editor.read(cx).document.read().ok().map(|document| document.path().to_path_buf()).map(|path| {
            let service = AppState::global(cx).document_service.clone();
            service.register_editor(path.clone(), editor.downgrade());
            EditorDocumentLease {
                service,
                path,
                editor_id: editor.entity_id(),
            }
        });

        let last_highlight_sync = {
            let ed = editor.read(cx);
            HighlightSyncState::from_snapshot(ed, !ed.search_state.query.is_empty(), None)
        };

        Self {
            editor,
            focus_handle,
            hex_view,
            is_search_visible: false,
            search_bar,
            is_goto_visible: false,
            goto_bar,
            structure_reparse_task: None,
            find_count_task: None,
            find_count_cancel: None,
            find_count_ranges: Vec::new(),
            find_count_truncated: false,
            find_count_generation: 0,
            tab_group: None,
            last_highlight_sync,
            cached_search_pattern: CachedSearchPattern::default(),
            cached_find_pattern: CachedSearchPattern::default(),
            _appearance_subscription,
            _bytes_per_row_subscription,
            _editor_subscription,
            _document_lease: document_lease,
        }
    }

    pub fn editor(&self) -> Entity<Editor> {
        self.editor.clone()
    }

    #[allow(dead_code)]
    pub fn hex_view(&self) -> Entity<HexView> {
        self.hex_view.clone()
    }

    fn schedule_structure_reparse(&mut self, generation: usize, cx: &mut Context<Self>) {
        // Replacing the task cancels the previous debounce timer. The editor
        // generation check below is still required because a timer may have
        // already resumed while a newer edit was being applied.
        self.structure_reparse_task = None;
        let editor = self.editor.clone();
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(75)).await;

            if let Some(this) = this.upgrade() {
                this.update(cx, |_, cx| {
                    if let Some(ksy) = editor.update(cx, |editor, _| editor.take_structure_reparse_request(generation)) {
                        let service = crate::app_state::AppState::global(cx).structure_service.clone();
                        service.start_parse(&editor, ksy, cx);
                    }
                });
            }
        });
        self.structure_reparse_task = Some(task);
    }

    #[allow(dead_code)]
    pub fn scroll_to_byte(&mut self, byte_offset: usize, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |view, cx| {
            view.scroll_to_byte(byte_offset, cx);
        });
    }

    pub fn scroll_to_range_if_needed(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |view, cx| {
            view.scroll_to_range_if_needed(range, cx);
        });
    }

    #[allow(dead_code)]
    pub fn scroll_to_byte_if_needed(&mut self, byte_offset: usize, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |view, cx| {
            view.scroll_to_byte_if_needed(byte_offset, cx);
        });
    }

    pub fn path(&self, cx: &App) -> std::path::PathBuf {
        self.editor.read(cx).document.read().expect("document read lock").path().to_path_buf()
    }

    #[allow(dead_code)]
    pub fn tab_group(&self) -> Option<WeakEntity<TabGroup>> {
        self.tab_group.clone()
    }

    #[allow(dead_code)]
    pub fn create_split_clone(&self, window: &mut Window, cx: &mut App) -> Entity<EditorView> {
        let (doc, options, show_inline_structure_view, collapsed_struct_ids, cursor_state, bytes_per_row) = {
            let ed = self.editor.read(cx);
            (
                ed.document.clone(),
                ed.options,
                ed.structure.show_inline_structure_view,
                ed.structure.collapsed_struct_ids.clone(),
                ed.cursor_state(),
                ed.bytes_per_row(),
            )
        };

        let layout_state = self.hex_view.read(cx).layout_state();

        let new_editor = cx.new(|_| {
            let mut editor = Editor::new(doc);
            editor.options = options;
            editor.cursor.group_size = options.group_size;
            editor.structure.show_inline_structure_view = show_inline_structure_view;
            editor.structure.collapsed_struct_ids = collapsed_struct_ids;
            editor.restore_cursor_state(cursor_state);
            editor.set_bytes_per_row(bytes_per_row);
            editor
        });

        cx.new(|cx| {
            let view = EditorView::new(new_editor, window, cx);
            view.hex_view.update(cx, |hv, _| {
                hv.apply_layout_state(&layout_state);
            });
            view
        })
    }

    fn update_top_overlay_state(&self, cx: &mut Context<Self>) {
        let is_visible = self.is_search_visible || self.is_goto_visible;
        self.hex_view.update(cx, |view, cx| {
            view.set_top_overlay_visible(is_visible, cx);
        });
    }

    fn cancel_find_count(&mut self) {
        if let Some(flag) = self.find_count_cancel.take() {
            flag.store(true, Ordering::Relaxed);
        }
        self.find_count_task = None;
    }

    fn dismiss_search(&mut self, cx: &mut Context<Self>) {
        self.cancel_find_count();
        self.find_count_ranges.clear();
        self.find_count_truncated = false;
        self.is_search_visible = false;
        self.search_bar.update(cx, |bar, cx| {
            bar.set_wrapped_notice(None, cx);
            bar.set_replace_error(None, cx);
        });
        self.update_top_overlay_state(cx);
        self.update_highlights(cx);
    }

    fn prefill_search_from_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let mode = self.search_bar.read(cx).mode();
        let hex_case = cx.try_global::<HexCase>().copied().unwrap_or_default();
        let prefill = {
            let editor = self.editor.read(cx);
            let encoding = editor.options.encoding;
            editor.selection_range().and_then(|range| {
                let doc = editor.document.read().ok()?;
                let data = doc.buffer.data();
                let end = range.end.min(data.len());
                if range.start >= end {
                    return None;
                }
                format_selection_for_find(&data[range.start..end], mode, encoding, hex_case)
            })
        };
        if let Some(query) = prefill {
            self.search_bar.update(cx, |bar, cx| {
                bar.set_query_and_select_all(&query, window, cx);
            });
            true
        } else {
            false
        }
    }

    pub fn toggle_search(&mut self, _: &ToggleSearch, window: &mut Window, cx: &mut Context<Self>) {
        let is_read_only = self.editor.read(cx).is_read_only();
        self.search_bar.update(cx, |bar, cx| {
            bar.set_read_only(is_read_only, cx);
        });

        if !self.is_search_visible {
            self.is_search_visible = true;
            self.is_goto_visible = false;
            if !self.prefill_search_from_selection(window, cx) {
                let (query, mode, flags) = {
                    let bar = self.search_bar.read(cx);
                    (bar.query(cx), bar.mode(), bar.flags())
                };
                self.search_bar.update(cx, |bar, cx| {
                    bar.focus_and_select_all(window, cx);
                });
                if !query.is_empty() {
                    self.perform_incremental_search(&query, mode, flags, cx);
                }
            }
        } else if !self.search_bar.read(cx).is_find_focused(window, cx) {
            if !self.prefill_search_from_selection(window, cx) {
                self.search_bar.update(cx, |bar, cx| {
                    bar.focus_and_select_all(window, cx);
                });
            }
        } else {
            self.search_bar.update(cx, |bar, cx| {
                bar.focus_and_select_all(window, cx);
            });
        }
        self.update_top_overlay_state(cx);
        self.update_highlights(cx);
        cx.notify();
    }

    pub fn toggle_replace(&mut self, _: &ToggleReplace, window: &mut Window, cx: &mut Context<Self>) {
        let is_read_only = self.editor.read(cx).is_read_only();
        self.search_bar.update(cx, |bar, cx| {
            bar.set_read_only(is_read_only, cx);
        });

        if !self.is_search_visible {
            self.is_search_visible = true;
            self.is_goto_visible = false;
            self.search_bar.update(cx, |bar, cx| {
                bar.set_replace_visible(true, cx);
            });
            let prefilled = self.prefill_search_from_selection(window, cx);
            let (query, mode, flags) = {
                let bar = self.search_bar.read(cx);
                (bar.query(cx), bar.mode(), bar.flags())
            };
            if prefilled || !query.is_empty() {
                if !prefilled {
                    self.perform_incremental_search(&query, mode, flags, cx);
                }
                self.search_bar.update(cx, |bar, cx| {
                    bar.focus_replace_and_select_all(window, cx);
                });
            } else {
                self.search_bar.update(cx, |bar, cx| {
                    bar.focus_and_select_all(window, cx);
                });
            }
        } else {
            let (replace_visible, replace_focused, has_query) = {
                let bar = self.search_bar.read(cx);
                (bar.replace_visible(), bar.is_replace_focused(window, cx), !bar.query(cx).is_empty())
            };
            if !replace_visible {
                self.search_bar.update(cx, |bar, cx| {
                    bar.set_replace_visible(true, cx);
                    if has_query {
                        bar.focus_replace_and_select_all(window, cx);
                    } else {
                        bar.focus_and_select_all(window, cx);
                    }
                });
            } else if !replace_focused {
                self.search_bar.update(cx, |bar, cx| {
                    bar.focus_replace_and_select_all(window, cx);
                });
            } else {
                self.search_bar.update(cx, |bar, cx| {
                    bar.set_replace_visible(false, cx);
                    bar.focus_and_select_all(window, cx);
                });
            }
        }
        self.update_top_overlay_state(cx);
        self.update_highlights(cx);
        cx.notify();
    }

    pub fn toggle_goto_address(&mut self, _: &ToggleGoToAddress, window: &mut Window, cx: &mut Context<Self>) {
        self.is_goto_visible = !self.is_goto_visible;
        if self.is_goto_visible {
            if self.is_search_visible {
                self.dismiss_search(cx);
            }
            let (cursor_offset, total_size, bytes_per_row, address_map) = {
                let editor = self.editor.read(cx);
                let map = editor.document.read().expect("doc read").address_map.clone();
                (editor.cursor.offset, editor.total_size(), editor.bytes_per_row(), map)
            };
            self.goto_bar.update(cx, |bar, cx| {
                bar.set_context_info(cursor_offset, total_size, bytes_per_row, address_map, cx);
                bar.focus(window, cx);
            });
        } else {
            self.hex_view.read(cx).focus_handle(cx).focus(window, cx);
        }
        self.update_top_overlay_state(cx);
        cx.notify();
    }

    fn sync_find_query_from_bar(&mut self, query: &str, mode: SearchMode, flags: TextSearchFlags, cx: &mut Context<Self>) {
        let changed = self.editor.update(cx, |editor: &mut Editor, cx| {
            let before = editor.find_state.generation;
            if query.is_empty() {
                if !editor.find_state.query.is_empty() {
                    editor.find_state_mut().clear();
                }
            } else {
                editor.find_state_mut().set_query_mode_and_flags(query.to_string(), mode, flags);
            }
            let changed = editor.find_state.generation != before;
            if changed {
                cx.notify();
            }
            changed
        });
        if changed {
            self.schedule_find_match_count(cx);
            self.update_highlights(cx);
        }
    }

    fn perform_incremental_search(&mut self, query: &str, mode: SearchMode, flags: TextSearchFlags, cx: &mut Context<Self>) {
        if query.is_empty() {
            self.cancel_find_count();
            self.find_count_ranges.clear();
            self.find_count_truncated = false;
            self.editor.update(cx, |editor: &mut Editor, cx| {
                editor.find_state_mut().clear();
                cx.notify();
            });
            self.search_bar.update(cx, |bar, cx| {
                bar.set_match_status(SearchMatchStatus::None, cx);
                bar.set_wrapped_notice(None, cx);
            });
            self.update_highlights(cx);
            return;
        }

        self.editor.update(cx, |editor: &mut Editor, cx| {
            editor.find_state_mut().set_query_mode_and_flags(query.to_string(), mode, flags);
            cx.notify();
        });

        self.schedule_find_match_count(cx);
        self.update_highlights(cx);
    }

    fn schedule_find_match_count(&mut self, cx: &mut Context<Self>) {
        self.cancel_find_count();
        if !self.is_search_visible {
            self.find_count_ranges.clear();
            self.find_count_truncated = false;
            return;
        }

        let (query, mode, flags, encoding, generation, buffer_and_segments) = {
            let ed = self.editor.read(cx);
            let q = ed.find_state.query.clone();
            let m = ed.find_state.mode;
            let f = ed.find_state.flags;
            let enc = ed.options.encoding;
            let find_gen = ed.find_state.generation;
            let data = if q.is_empty() {
                None
            } else {
                ed.document.read().ok().map(|doc| (doc.buffer.clone(), doc.address_map.segment_ranges()))
            };
            (q, m, f, enc, find_gen, data)
        };

        self.find_count_generation = generation;

        if query.is_empty() {
            self.find_count_ranges.clear();
            self.find_count_truncated = false;
            self.search_bar.update(cx, |bar, cx| {
                bar.set_match_status(SearchMatchStatus::None, cx);
            });
            return;
        }

        if let Err(err) = compile_search(&query, mode, encoding, flags) {
            self.find_count_ranges.clear();
            self.find_count_truncated = false;
            self.search_bar.update(cx, |bar, cx| {
                bar.set_match_status(SearchMatchStatus::Error(err.to_string()), cx);
            });
            return;
        }

        let Some((buffer, segments)) = buffer_and_segments else {
            return;
        };

        self.search_bar.update(cx, |bar, cx| {
            bar.set_match_status(SearchMatchStatus::Scanning, cx);
        });

        let cancel = Arc::new(AtomicBool::new(false));
        self.find_count_cancel = Some(cancel.clone());
        let cancel_check = cancel.clone();

        let task = cx.spawn(async move |this, cx| {
            let search_task = cx.update(|cx| {
                let service = AppState::global(cx).search_service.clone();
                service.search_with_limit(
                    LimitedSearchQuery {
                        buffer,
                        query,
                        mode,
                        encoding,
                        flags,
                        segments,
                        max_results: MAX_FIND_COUNT_RESULTS,
                        cancel: Some(cancel),
                    },
                    cx,
                )
            });

            let result = search_task.await;
            if cancel_check.load(Ordering::Relaxed) {
                return;
            }

            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    if !this.is_search_visible || this.editor.read(cx).find_state.generation != generation {
                        return;
                    }
                    this.find_count_cancel = None;
                    match result {
                        Ok((ranges, truncated)) => {
                            this.find_count_ranges = ranges;
                            this.find_count_truncated = truncated;
                            this.refresh_find_match_status(cx);
                        }
                        Err(err) => {
                            this.find_count_ranges.clear();
                            this.find_count_truncated = false;
                            this.search_bar.update(cx, |bar, cx| {
                                bar.set_match_status(SearchMatchStatus::Error(err.to_string()), cx);
                            });
                        }
                    }
                });
            }
        });
        self.find_count_task = Some(task);
    }

    fn refresh_find_match_status(&mut self, cx: &mut Context<Self>) {
        if !self.is_search_visible {
            return;
        }
        let anchor = {
            let ed = self.editor.read(cx);
            ed.selection_range().map(|r| r.start).unwrap_or(ed.cursor.offset)
        };
        let current = find_index_containing_offset(&self.find_count_ranges, anchor);
        let status = SearchMatchStatus::Count {
            current,
            total: self.find_count_ranges.len(),
            truncated: self.find_count_truncated,
        };
        self.search_bar.update(cx, |bar, cx| {
            bar.set_match_status(status, cx);
        });
    }

    fn has_active_search(&self, editor: &Editor, cx: &App) -> bool {
        (self.is_search_visible && (!self.search_bar.read(cx).query(cx).is_empty() || !editor.find_state.query.is_empty()))
            || !editor.search_state.query.is_empty()
    }

    fn capture_highlight_sync_state(&self, editor: &Editor, scroll_top_row: Option<usize>, cx: &App) -> HighlightSyncState {
        let has_search = self.has_active_search(editor, cx);
        HighlightSyncState::from_snapshot(editor, has_search, scroll_top_row)
    }

    fn update_highlights(&mut self, cx: &mut Context<Self>) {
        let scroll_top_row = Some(self.hex_view.read(cx).current_scroll_top_row());
        let editor = self.editor.read(cx);
        let sync_state = self.capture_highlight_sync_state(editor, scroll_top_row, cx);
        self.last_highlight_sync = sync_state;

        if !sync_state.has_search {
            self.hex_view.update(cx, |view, cx| {
                view.set_highlights(Vec::new(), cx);
            });
            return;
        }

        let (search_highlights, find_highlights) = {
            let (start, end) = self.hex_view.read(cx).viewport_byte_range(cx);
            let editor = self.editor.read(cx);
            let encoding = editor.options.encoding;
            let search_compiled =
                self.cached_search_pattern
                    .resolve_compiled(&editor.search_state.query, editor.search_state.mode, encoding, editor.search_state.flags);
            let find_query = if self.is_search_visible {
                let bar = self.search_bar.read(cx);
                let query = bar.query(cx);
                if query.is_empty() { None } else { Some((query, bar.mode(), bar.flags())) }
            } else {
                None
            };
            let find_compiled = find_query
                .as_ref()
                .and_then(|(q, mode, flags)| self.cached_find_pattern.resolve_compiled(q, *mode, encoding, *flags));

            let search_needs_scan = search_compiled.is_some() && !editor.search_state.is_full_search_complete;
            let find_needs_scan = find_compiled.is_some();
            let doc_guard = if search_needs_scan || find_needs_scan {
                editor.document.read().ok()
            } else {
                None
            };
            let segments = doc_guard.as_ref().map(|doc| doc.address_map.segment_ranges()).unwrap_or_default();
            let theme = cx.theme();

            // 1. SearchPanel highlights: slice completed results in O(log R), or scan viewport when streaming/truncated
            let search_hl = if let Some(compiled) = search_compiled {
                let search_panel_color = theme.warning;
                let search_panel_active_color = theme.warning_active;
                let current_result = editor.search_state.current_result();
                let selection_range = editor.selection_range();

                let build_range_hl = |ranges: &[Range<usize>]| -> Vec<(Range<usize>, Hsla)> {
                    let mut list = Vec::with_capacity(ranges.len());
                    for range in ranges {
                        let is_current = Some(range.start) == current_result || selection_range.as_ref() == Some(range);
                        let color = if is_current { search_panel_active_color } else { search_panel_color };
                        list.push((range.clone(), color));
                    }
                    list
                };

                if editor.search_state.is_full_search_complete {
                    if !editor.search_state.result_ranges.is_empty() {
                        build_range_hl(viewport_range_slice(&editor.search_state.result_ranges, start..end))
                    } else if let Some(pattern_len) = compiled.fixed_len() {
                        let offsets = viewport_match_slice(&editor.search_state.results, pattern_len, start..end);
                        let mut list = Vec::with_capacity(offsets.len());
                        for &result_offset in offsets {
                            let range = result_offset..result_offset + pattern_len;
                            let is_current = Some(result_offset) == current_result || selection_range.as_ref() == Some(&range);
                            let color = if is_current { search_panel_active_color } else { search_panel_color };
                            list.push((range, color));
                        }
                        list
                    } else {
                        Vec::new()
                    }
                } else if let Some(doc) = doc_guard.as_deref() {
                    let ranges = scan_viewport_ranges(doc.buffer.data(), compiled, &segments, start, end);
                    build_range_hl(&ranges)
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            // 2. Find Widget highlights (ONLY IF self.is_search_visible && search_bar has query)
            let find_hl = if let (Some(compiled), Some(doc)) = (find_compiled, doc_guard.as_deref()) {
                let ranges = scan_viewport_ranges(doc.buffer.data(), compiled, &segments, start, end);
                let (search_color, current_result_color) = (theme.warning, theme.warning_active);
                let anchor = editor.selection_range().map(|r| r.start).unwrap_or(editor.cursor.offset);
                let mut list = Vec::with_capacity(ranges.len());
                for range in ranges {
                    let is_current = anchor >= range.start && anchor < range.end;
                    let color = if is_current { current_result_color } else { search_color };
                    list.push((range, color));
                }
                list
            } else {
                Vec::new()
            };

            (search_hl, find_hl)
        };

        // Merge highlights: Find Widget highlights take precedence over SearchPanel highlights where they overlap
        let highlights = compose_search_highlights(search_highlights, find_highlights);

        self.hex_view.update(cx, |view, cx| {
            view.set_highlights(highlights, cx);
        });
    }

    pub fn search_next(&mut self, _: &SearchNext, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_search_visible {
            let (query, mode, flags) = {
                let bar = self.search_bar.read(cx);
                (bar.query(cx), bar.mode(), bar.flags())
            };
            self.sync_find_query_from_bar(&query, mode, flags, cx);
        }
        self.perform_search_next(cx);
        if !self.is_search_visible {
            self.hex_view.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    fn perform_search_next(&mut self, cx: &mut Context<Self>) {
        let is_search_visible = self.is_search_visible;
        let (target_range, wrapped) = self.editor.update(cx, |editor: &mut Editor, cx| {
            let res = if is_search_visible || editor.search_state.query.is_empty() {
                match editor.next_find_result_range() {
                    Some((range, wrapped)) => (Some(range), wrapped),
                    None => (None, false),
                }
            } else {
                let offset = editor.next_search_result();
                let range = offset.map(|off| {
                    editor.search_state.current_result_range().unwrap_or_else(|| {
                        let len = editor.search_pattern().map(|p| p.len()).unwrap_or(1);
                        off..off.saturating_add(len)
                    })
                });
                (range, false)
            };
            cx.notify();
            res
        });

        if is_search_visible {
            self.search_bar.update(cx, |bar, cx| {
                bar.set_wrapped_notice(wrapped.then_some("Wrapped to top"), cx);
            });
            self.refresh_find_match_status(cx);
        }

        if let Some(range) = target_range {
            self.hex_view.update(cx, |view, cx| {
                view.scroll_to_range_if_needed(range, cx);
            });
        }
        self.update_highlights(cx);
        cx.notify();
    }

    pub fn search_prev(&mut self, _: &SearchPrev, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_search_visible {
            let (query, mode, flags) = {
                let bar = self.search_bar.read(cx);
                (bar.query(cx), bar.mode(), bar.flags())
            };
            self.sync_find_query_from_bar(&query, mode, flags, cx);
        }
        self.perform_search_prev(cx);
        if !self.is_search_visible {
            self.hex_view.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    fn perform_search_prev(&mut self, cx: &mut Context<Self>) {
        let is_search_visible = self.is_search_visible;
        let (target_range, wrapped) = self.editor.update(cx, |editor: &mut Editor, cx| {
            let res = if is_search_visible || editor.search_state.query.is_empty() {
                match editor.prev_find_result_range() {
                    Some((range, wrapped)) => (Some(range), wrapped),
                    None => (None, false),
                }
            } else {
                let offset = editor.prev_search_result();
                let range = offset.map(|off| {
                    editor.search_state.current_result_range().unwrap_or_else(|| {
                        let len = editor.search_pattern().map(|p| p.len()).unwrap_or(1);
                        off..off.saturating_add(len)
                    })
                });
                (range, false)
            };
            cx.notify();
            res
        });

        if is_search_visible {
            self.search_bar.update(cx, |bar, cx| {
                bar.set_wrapped_notice(wrapped.then_some("Wrapped to bottom"), cx);
            });
            self.refresh_find_match_status(cx);
        }

        if let Some(range) = target_range {
            self.hex_view.update(cx, |view, cx| {
                view.scroll_to_range_if_needed(range, cx);
            });
        }
        self.update_highlights(cx);
        cx.notify();
    }

    fn perform_replace(&mut self, query: &str, mode: SearchMode, flags: TextSearchFlags, replacement: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_find_query_from_bar(query, mode, flags, cx);
        let (is_read_only, encoding) = {
            let ed = self.editor.read(cx);
            (ed.is_read_only(), ed.options.encoding)
        };
        if is_read_only {
            self.search_bar.update(cx, |bar, cx| {
                bar.set_read_only(true, cx);
            });
            window.push_notification(Notification::warning("File is read-only. Disable read-only mode to replace bytes."), cx);
            return;
        }
        let replacement_bytes = match parse_replacement_bytes(replacement, mode, encoding) {
            Ok(bytes) => {
                self.search_bar.update(cx, |bar, cx| {
                    bar.set_replace_error(None, cx);
                });
                bytes
            }
            Err(err) => {
                self.search_bar.update(cx, |bar, cx| {
                    bar.set_replace_error(Some(err.to_string()), cx);
                });
                return;
            }
        };

        let result = self.editor.update(cx, |editor: &mut Editor, cx| {
            let res = editor.replace_current_find_match(&replacement_bytes);
            if res.is_some() {
                cx.notify();
            }
            res
        });

        match result {
            Some((_replaced, next_range, wrapped)) => {
                self.search_bar.update(cx, |bar, cx| {
                    bar.set_wrapped_notice(wrapped.then_some("Wrapped to top"), cx);
                });
                if let Some(range) = next_range {
                    self.hex_view.update(cx, |view, cx| {
                        view.scroll_to_range_if_needed(range, cx);
                    });
                }
                self.schedule_find_match_count(cx);
                self.update_highlights(cx);
            }
            None => {
                self.search_bar.update(cx, |bar, cx| {
                    bar.set_wrapped_notice(None, cx);
                });
            }
        }
        cx.notify();
    }

    fn perform_replace_all(&mut self, query: &str, mode: SearchMode, flags: TextSearchFlags, replacement: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_find_query_from_bar(query, mode, flags, cx);
        let (is_read_only, encoding) = {
            let ed = self.editor.read(cx);
            (ed.is_read_only(), ed.options.encoding)
        };
        if is_read_only {
            self.search_bar.update(cx, |bar, cx| {
                bar.set_read_only(true, cx);
            });
            window.push_notification(Notification::warning("File is read-only. Disable read-only mode to replace bytes."), cx);
            return;
        }
        let replacement_bytes = match parse_replacement_bytes(replacement, mode, encoding) {
            Ok(bytes) => {
                self.search_bar.update(cx, |bar, cx| {
                    bar.set_replace_error(None, cx);
                });
                bytes
            }
            Err(err) => {
                self.search_bar.update(cx, |bar, cx| {
                    bar.set_replace_error(Some(err.to_string()), cx);
                });
                return;
            }
        };

        let replaced_count = self.editor.update(cx, |editor: &mut Editor, cx| {
            let count = editor.replace_all_find_matches(&replacement_bytes);
            if count > 0 {
                cx.notify();
            }
            count
        });

        if replaced_count > 0 {
            let cursor_offset = self.editor.read(cx).cursor.offset;
            self.hex_view.update(cx, |view, cx| {
                view.scroll_to_byte_if_needed(cursor_offset, cx);
            });
            let word = if replaced_count == 1 { "occurrence" } else { "occurrences" };
            window.push_notification(Notification::info(format!("Replaced {replaced_count} {word}")), cx);
        } else {
            window.push_notification(Notification::info("No matches replaced"), cx);
        }

        self.search_bar.update(cx, |bar, cx| {
            bar.set_wrapped_notice(None, cx);
        });
        self.schedule_find_match_count(cx);
        self.update_highlights(cx);
        cx.notify();
    }

    pub fn focus_hex_view(&mut self, _: &FocusHexView, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.read(cx).focus_handle(cx).focus(window, cx);
    }

    pub fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor: &mut Editor, _| {
            editor.select_all();
        });
        self.hex_view.read(cx).focus_handle(cx).focus(window, cx);
        cx.notify();
    }

    pub fn go_to_beginning(&mut self, _: &GoToBeginning, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor: &mut Editor, _| {
            editor.go_to_beginning();
        });
        let cursor_offset = self.editor.read(cx).cursor.offset;
        self.hex_view.update(cx, |view, cx| {
            view.scroll_to_byte(cursor_offset, cx);
            view.focus_handle(cx).focus(window, cx);
        });
        cx.notify();
    }

    pub fn go_to_end(&mut self, _: &GoToEnd, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.editor.update(cx, |editor: &mut Editor, _| {
            if insert_mode {
                let from = editor.cursor.offset;
                let total = editor.total_size();
                editor.set_cursor_offset_exact(total);
                editor.nav_history.record_jump(from, total);
            } else {
                editor.go_to_end();
            }
        });
        let cursor_offset = self.editor.read(cx).cursor.offset;
        self.hex_view.update(cx, |view, cx| {
            view.scroll_to_byte(cursor_offset, cx);
            view.focus_handle(cx).focus(window, cx);
        });
        cx.notify();
    }

    pub fn select_to_beginning(&mut self, _: &SelectToBeginning, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.editor.update(cx, |editor: &mut Editor, _| {
            if insert_mode {
                editor.select_home_for_insert();
            } else {
                editor.select_home();
            }
        });
        let cursor_offset = self.editor.read(cx).cursor.offset;
        self.hex_view.update(cx, |view, cx| {
            view.scroll_to_byte(cursor_offset, cx);
            view.focus_handle(cx).focus(window, cx);
        });
        cx.notify();
    }

    pub fn select_to_end(&mut self, _: &SelectToEnd, window: &mut Window, cx: &mut Context<Self>) {
        let insert_mode = InsertModeState::is_enabled(cx);
        self.editor.update(cx, |editor: &mut Editor, _| {
            if insert_mode {
                editor.select_end_for_insert();
            } else {
                editor.select_end();
            }
        });
        let cursor_offset = self.editor.read(cx).cursor.offset;
        self.hex_view.update(cx, |view, cx| {
            view.scroll_to_byte(cursor_offset, cx);
            view.focus_handle(cx).focus(window, cx);
        });
        cx.notify();
    }

    pub fn navigate_back(&mut self, _: &NavigateBack, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.editor.update(cx, |editor: &mut Editor, cx| {
            let offset = editor.navigate_back();
            if offset.is_some() {
                cx.notify();
            }
            offset
        });
        if let Some(offset) = target {
            self.hex_view.update(cx, |view, cx| {
                view.scroll_to_byte_if_needed(offset, cx);
                view.focus_handle(cx).focus(window, cx);
            });
            cx.notify();
        }
    }

    pub fn navigate_forward(&mut self, _: &NavigateForward, window: &mut Window, cx: &mut Context<Self>) {
        let target = self.editor.update(cx, |editor: &mut Editor, cx| {
            let offset = editor.navigate_forward();
            if offset.is_some() {
                cx.notify();
            }
            offset
        });
        if let Some(offset) = target {
            self.hex_view.update(cx, |view, cx| {
                view.scroll_to_byte_if_needed(offset, cx);
                view.focus_handle(cx).focus(window, cx);
            });
            cx.notify();
        }
    }

    pub fn toggle_edit_column(&mut self, action: &ToggleEditColumn, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.toggle_edit_column(action, window, cx));
    }

    pub fn copy(&mut self, action: &Copy, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.copy(action, window, cx));
    }

    pub fn cut(&mut self, action: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.cut(action, window, cx));
    }

    pub fn paste(&mut self, action: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.paste(action, window, cx));
    }

    pub fn undo(&mut self, action: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.undo(action, window, cx));
    }

    pub fn redo(&mut self, action: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.redo(action, window, cx));
    }

    pub fn copy_as_hexdump(&mut self, action: &CopyAsHexDump, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.copy_as_hexdump(action, window, cx));
    }

    pub fn copy_as_cpp_array(&mut self, action: &CopyAsCppArray, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.copy_as_cpp_array(action, window, cx));
    }

    pub fn copy_as_hex_stream(&mut self, action: &CopyAsHexStream, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.copy_as_hex_stream(action, window, cx));
    }

    pub fn copy_as_hex_spaces(&mut self, action: &CopyAsHexSpaces, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.copy_as_hex_spaces(action, window, cx));
    }

    pub fn copy_as_printable_text(&mut self, action: &CopyAsPrintableText, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.copy_as_printable_text(action, window, cx));
    }

    pub fn copy_as_base64(&mut self, action: &CopyAsBase64, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.copy_as_base64(action, window, cx));
    }

    pub fn copy_as_escaped_string(&mut self, action: &CopyAsEscapedString, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.copy_as_escaped_string(action, window, cx));
    }

    pub fn copy_as_binary(&mut self, action: &CopyAsBinary, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.copy_as_binary(action, window, cx));
    }

    pub fn copy_as_rust_array(&mut self, action: &CopyAsRustArray, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.copy_as_rust_array(action, window, cx));
    }

    pub fn copy_as_json_array(&mut self, action: &CopyAsJsonArray, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.copy_as_json_array(action, window, cx));
    }

    pub fn bookmark_red(&mut self, action: &BookmarkRed, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.bookmark_red(action, window, cx));
    }

    pub fn bookmark_orange(&mut self, action: &BookmarkOrange, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.bookmark_orange(action, window, cx));
    }

    pub fn bookmark_yellow(&mut self, action: &BookmarkYellow, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.bookmark_yellow(action, window, cx));
    }

    pub fn bookmark_green(&mut self, action: &BookmarkGreen, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.bookmark_green(action, window, cx));
    }

    pub fn bookmark_cyan(&mut self, action: &BookmarkCyan, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.bookmark_cyan(action, window, cx));
    }

    pub fn bookmark_blue(&mut self, action: &BookmarkBlue, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.bookmark_blue(action, window, cx));
    }

    pub fn bookmark_purple(&mut self, action: &BookmarkPurple, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.bookmark_purple(action, window, cx));
    }

    pub fn bookmark_pink(&mut self, action: &BookmarkPink, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.bookmark_pink(action, window, cx));
    }

    pub fn clear_bookmark(&mut self, action: &ClearBookmark, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.clear_bookmark(action, window, cx));
    }

    pub fn clear_all_bookmarks(&mut self, action: &ClearAllBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.clear_all_bookmarks(action, window, cx));
    }

    pub fn add_custom_break(&mut self, action: &AddCustomBreak, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.add_custom_break(action, window, cx));
    }

    pub fn remove_custom_break_backward(&mut self, action: &RemoveCustomBreakBackward, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.remove_custom_break_backward(action, window, cx));
    }

    pub fn remove_custom_break_forward(&mut self, action: &RemoveCustomBreakForward, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.remove_custom_break_forward(action, window, cx));
    }

    pub fn remove_custom_breaks_in_range(&mut self, action: &RemoveCustomBreaksInRange, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.remove_custom_breaks_in_range(action, window, cx));
    }

    pub fn join_line(&mut self, action: &JoinLine, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.join_line(action, window, cx));
    }

    pub fn clear_all_custom_breaks(&mut self, action: &ClearAllCustomBreaks, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.clear_all_custom_breaks(action, window, cx));
    }

    pub fn show_all_bookmarks(&mut self, action: &ShowAllBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.show_all_bookmarks(action, window, cx));
    }

    pub fn hide_all_bookmarks(&mut self, action: &HideAllBookmarks, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.hide_all_bookmarks(action, window, cx));
    }

    pub fn toggle_hide_unbookmarked(&mut self, action: &ToggleHideUnbookmarked, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.toggle_hide_unbookmarked(action, window, cx));
    }

    pub fn unfold_bookmark_at_cursor(&mut self, action: &UnfoldBookmarkAtCursor, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| hv.unfold_bookmark_at_cursor(action, window, cx));
    }

    pub fn toggle_bookmark_red(&mut self, _action: &ToggleBookmarkRed, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view
            .update(cx, |hv, cx| hv.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Red, window, cx));
    }

    pub fn toggle_bookmark_orange(&mut self, _action: &ToggleBookmarkOrange, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view
            .update(cx, |hv, cx| hv.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Orange, window, cx));
    }

    pub fn toggle_bookmark_yellow(&mut self, _action: &ToggleBookmarkYellow, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view
            .update(cx, |hv, cx| hv.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Yellow, window, cx));
    }

    pub fn toggle_bookmark_green(&mut self, _action: &ToggleBookmarkGreen, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view
            .update(cx, |hv, cx| hv.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Green, window, cx));
    }

    pub fn toggle_bookmark_cyan(&mut self, _action: &ToggleBookmarkCyan, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view
            .update(cx, |hv, cx| hv.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Cyan, window, cx));
    }

    pub fn toggle_bookmark_blue(&mut self, _action: &ToggleBookmarkBlue, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view
            .update(cx, |hv, cx| hv.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Blue, window, cx));
    }

    pub fn toggle_bookmark_purple(&mut self, _action: &ToggleBookmarkPurple, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view
            .update(cx, |hv, cx| hv.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Purple, window, cx));
    }

    pub fn toggle_bookmark_pink(&mut self, _action: &ToggleBookmarkPink, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view
            .update(cx, |hv, cx| hv.toggle_bookmark_color(crate::core::bookmark::BookmarkColor::Pink, window, cx));
    }

    pub fn show_only_bookmark_red(&mut self, _action: &ShowOnlyBookmarkRed, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view
            .update(cx, |hv, cx| hv.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Red, window, cx));
    }

    pub fn show_only_bookmark_orange(&mut self, _action: &ShowOnlyBookmarkOrange, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| {
            hv.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Orange, window, cx)
        });
    }

    pub fn show_only_bookmark_yellow(&mut self, _action: &ShowOnlyBookmarkYellow, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| {
            hv.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Yellow, window, cx)
        });
    }

    pub fn show_only_bookmark_green(&mut self, _action: &ShowOnlyBookmarkGreen, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| {
            hv.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Green, window, cx)
        });
    }

    pub fn show_only_bookmark_cyan(&mut self, _action: &ShowOnlyBookmarkCyan, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view
            .update(cx, |hv, cx| hv.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Cyan, window, cx));
    }

    pub fn show_only_bookmark_blue(&mut self, _action: &ShowOnlyBookmarkBlue, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view
            .update(cx, |hv, cx| hv.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Blue, window, cx));
    }

    pub fn show_only_bookmark_purple(&mut self, _action: &ShowOnlyBookmarkPurple, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view.update(cx, |hv, cx| {
            hv.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Purple, window, cx)
        });
    }

    pub fn show_only_bookmark_pink(&mut self, _action: &ShowOnlyBookmarkPink, window: &mut Window, cx: &mut Context<Self>) {
        self.hex_view
            .update(cx, |hv, cx| hv.show_only_bookmark_color(crate::core::bookmark::BookmarkColor::Pink, window, cx));
    }

    /// Jumps the editor to a specific byte offset or mapped address, optionally extending current selection.
    pub fn jump_to_offset(&mut self, offset: usize, extend: bool, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            let target_offset = {
                let doc = editor.document.read().expect("document read lock");
                doc.address_map.resolve_jump_offset(offset)
            };
            editor.go_to_offset(target_offset, extend);
            cx.notify();
        });
        let cursor_offset = self.editor.read(cx).cursor.offset;
        self.hex_view.update(cx, |view, cx| {
            view.scroll_to_byte_if_needed(cursor_offset, cx);
        });
        cx.notify();
    }

    /// Jumps the editor to and selects a contiguous byte range `[start..end)`.
    pub fn jump_to_range(&mut self, range: std::ops::Range<usize>, cx: &mut Context<Self>) {
        self.editor.update(cx, |editor, cx| {
            let target_range = {
                let doc = editor.document.read().expect("document read lock");
                doc.address_map.resolve_jump_range(range, doc.buffer.len())
            };
            editor.go_to_range(target_range);
            cx.notify();
        });
        let cursor_offset = self.editor.read(cx).cursor.offset;
        self.hex_view.update(cx, |view, cx| {
            view.scroll_to_byte_if_needed(cursor_offset, cx);
        });
        cx.notify();
    }
}

impl EventEmitter<PanelEvent> for EditorView {}

impl Focusable for EditorView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for EditorView {
    fn title(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let editor = self.editor.read(cx);
        let doc = editor.document.read().expect("document read lock");

        let mut name = doc
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "(untitled)".to_string());

        if doc.is_dirty() {
            name.push_str(" *");
        }

        name
    }

    fn tab_name(&self, cx: &App) -> Option<SharedString> {
        let editor = self.editor.read(cx);
        let doc = editor.document.read().expect("document read lock");

        let mut name = doc
            .path()
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| "(untitled)".to_string());

        if doc.is_dirty() {
            name.push_str(" *");
        }

        Some(name.into())
    }

    fn zoom_control(&self, _cx: &App) -> Option<gpui_kit::component::dock::PanelControl> {
        Some(gpui_kit::component::dock::PanelControl::Both)
    }

    fn inner_padding(&self, _cx: &App) -> bool {
        false
    }

    fn toolbar_buttons(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> Option<Vec<Button>> {
        Some(vec![
            Button::new("split-right")
                .icon(crate::ui::icon::IconName::PanelRight)
                .xsmall()
                .ghost()
                .tab_stop(false)
                .tooltip(if cfg!(target_os = "macos") {
                    "Split Right (cmd-\\)"
                } else {
                    "Split Right (ctrl-\\)"
                })
                .on_click(cx.listener(|_, _, window, cx| {
                    window.dispatch_action(Box::new(crate::actions::SplitRight), cx);
                })),
            Button::new("split-down")
                .icon(crate::ui::icon::IconName::PanelBottom)
                .xsmall()
                .ghost()
                .tab_stop(false)
                .tooltip(if cfg!(target_os = "macos") {
                    "Split Down (cmd-shift-d)"
                } else {
                    "Split Down (ctrl-shift-d)"
                })
                .on_click(cx.listener(|_, _, window, cx| {
                    window.dispatch_action(Box::new(crate::actions::SplitDown), cx);
                })),
        ])
    }

    fn dropdown_menu(&mut self, this: PopupMenu, _window: &mut Window, _cx: &mut Context<Self>) -> PopupMenu {
        this.menu_with_icon("Split Right", IconName::PanelRight, Box::new(crate::actions::SplitRight))
            .menu_with_icon("Split Down", IconName::PanelBottom, Box::new(crate::actions::SplitDown))
            .separator()
            .menu_with_icon("Close Tab", IconName::Close, Box::new(crate::actions::CloseActivePanel))
    }
}

impl gpui_kit::base::dock::Panel for EditorView {
    fn panel_name(&self) -> &'static str {
        "EditorView"
    }

    fn closable(&self, _cx: &App) -> bool {
        true
    }

    fn zoomable(&self, _cx: &App) -> bool {
        true
    }

    fn visible(&self, _cx: &App) -> bool {
        true
    }

    fn on_added_to(&mut self, tab_group: WeakEntity<TabGroup>, _window: &mut Window, _cx: &mut Context<Self>) {
        self.tab_group = Some(tab_group);
    }

    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if active {
            self.focus_handle.focus(window, cx);
        }
    }

    fn set_zoomed(&mut self, _zoomed: bool, _window: &mut Window, _cx: &mut Context<Self>) {}

    fn dump(&self, cx: &App) -> gpui_kit::component::dock::PanelState {
        let mut state = gpui_kit::component::dock::PanelState::new(self.panel_name());
        let panel_state = EditorViewState {
            path: Some(self.editor.read(cx).document.read().expect("document read lock").path().to_path_buf()),
        };
        state.info = gpui_kit::component::dock::PanelInfo::panel(panel_state.to_value());
        state
    }
}

fn find_index_containing_offset(ranges: &[Range<usize>], offset: usize) -> Option<usize> {
    if ranges.is_empty() {
        return None;
    }
    let idx = ranges.partition_point(|r| r.end <= offset);
    let range = ranges.get(idx)?;
    if offset >= range.start && offset < range.end { Some(idx) } else { None }
}

impl Render for EditorView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let container = div().size_full().relative().key_context(CONTEXT).track_focus(&self.focus_handle);

        container
            .on_action(cx.listener(Self::toggle_search))
            .on_action(cx.listener(Self::toggle_replace))
            .on_action(cx.listener(Self::toggle_goto_address))
            .on_action(cx.listener(Self::search_next))
            .on_action(cx.listener(Self::search_prev))
            .on_action(cx.listener(Self::focus_hex_view))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::go_to_beginning))
            .on_action(cx.listener(Self::go_to_end))
            .on_action(cx.listener(Self::select_to_beginning))
            .on_action(cx.listener(Self::select_to_end))
            .on_action(cx.listener(Self::navigate_back))
            .on_action(cx.listener(Self::navigate_forward))
            .on_action(cx.listener(Self::toggle_edit_column))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::copy_as_hexdump))
            .on_action(cx.listener(Self::copy_as_cpp_array))
            .on_action(cx.listener(Self::copy_as_hex_stream))
            .on_action(cx.listener(Self::copy_as_hex_spaces))
            .on_action(cx.listener(Self::copy_as_printable_text))
            .on_action(cx.listener(Self::copy_as_base64))
            .on_action(cx.listener(Self::copy_as_escaped_string))
            .on_action(cx.listener(Self::copy_as_binary))
            .on_action(cx.listener(Self::copy_as_rust_array))
            .on_action(cx.listener(Self::copy_as_json_array))
            .on_action(cx.listener(Self::bookmark_red))
            .on_action(cx.listener(Self::bookmark_orange))
            .on_action(cx.listener(Self::bookmark_yellow))
            .on_action(cx.listener(Self::bookmark_green))
            .on_action(cx.listener(Self::bookmark_cyan))
            .on_action(cx.listener(Self::bookmark_blue))
            .on_action(cx.listener(Self::bookmark_purple))
            .on_action(cx.listener(Self::bookmark_pink))
            .on_action(cx.listener(Self::clear_bookmark))
            .on_action(cx.listener(Self::clear_all_bookmarks))
            .on_action(cx.listener(Self::add_custom_break))
            .on_action(cx.listener(Self::remove_custom_break_backward))
            .on_action(cx.listener(Self::remove_custom_break_forward))
            .on_action(cx.listener(Self::remove_custom_breaks_in_range))
            .on_action(cx.listener(Self::join_line))
            .on_action(cx.listener(Self::clear_all_custom_breaks))
            .on_action(cx.listener(Self::show_all_bookmarks))
            .on_action(cx.listener(Self::hide_all_bookmarks))
            .on_action(cx.listener(Self::toggle_hide_unbookmarked))
            .on_action(cx.listener(Self::unfold_bookmark_at_cursor))
            .on_action(cx.listener(Self::toggle_bookmark_red))
            .on_action(cx.listener(Self::toggle_bookmark_orange))
            .on_action(cx.listener(Self::toggle_bookmark_yellow))
            .on_action(cx.listener(Self::toggle_bookmark_green))
            .on_action(cx.listener(Self::toggle_bookmark_cyan))
            .on_action(cx.listener(Self::toggle_bookmark_blue))
            .on_action(cx.listener(Self::toggle_bookmark_purple))
            .on_action(cx.listener(Self::toggle_bookmark_pink))
            .on_action(cx.listener(Self::show_only_bookmark_red))
            .on_action(cx.listener(Self::show_only_bookmark_orange))
            .on_action(cx.listener(Self::show_only_bookmark_yellow))
            .on_action(cx.listener(Self::show_only_bookmark_green))
            .on_action(cx.listener(Self::show_only_bookmark_cyan))
            .on_action(cx.listener(Self::show_only_bookmark_blue))
            .on_action(cx.listener(Self::show_only_bookmark_purple))
            .on_action(cx.listener(Self::show_only_bookmark_pink))
            .child(self.hex_view.clone())
            .when(self.is_search_visible, |el| {
                el.child(div().absolute().top(px(8.0)).right(px(20.0)).child(self.search_bar.clone()))
            })
            .when(self.is_goto_visible, |el| {
                el.child(div().absolute().top(px(8.0)).right(px(20.0)).child(self.goto_bar.clone()))
            })
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct EditorViewState {
    pub path: Option<std::path::PathBuf>,
}

impl EditorViewState {
    #[allow(dead_code)]
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("serialize EditorViewState")
    }

    #[allow(dead_code)]
    pub fn from_value(value: serde_json::Value) -> Option<Self> {
        serde_json::from_value(value).ok()
    }
}

impl crate::ui::pane::WorkspaceTab for Entity<EditorView> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn title(&self, cx: &App) -> String {
        let path = self.read(cx).path(cx);
        path.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "Untitled".to_string())
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle(cx)
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.read(cx).editor().read(cx).document.read().map(|d| d.is_dirty()).unwrap_or(false)
    }

    fn is_read_only(&self, cx: &App) -> bool {
        self.read(cx).editor().read(cx).document.read().map(|d| d.is_read_only()).unwrap_or(false)
    }

    fn path(&self, cx: &App) -> Option<PathBuf> {
        Some(self.read(cx).path(cx))
    }

    fn render(&self) -> AnyElement {
        self.clone().into_any_element()
    }

    fn editor(&self, cx: &App) -> Option<Entity<Editor>> {
        Some(self.read(cx).editor())
    }

    fn document(&self, cx: &App) -> Option<std::sync::Arc<std::sync::RwLock<crate::core::document::Document>>> {
        Some(self.read(cx).editor().read(cx).document.clone())
    }

    fn create_split(&self, window: &mut Window, cx: &mut App) -> Option<crate::ui::pane::TabContent> {
        let new_editor_view = self.update(cx, |ep, cx| ep.create_split_clone(window, cx));
        Some(crate::ui::pane::TabContent::new(new_editor_view))
    }
}

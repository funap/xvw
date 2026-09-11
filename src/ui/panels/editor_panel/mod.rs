use gpui_kit::base::dock::TabGroup;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::dock::{Panel, PanelEvent};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::{ActiveTheme, Sizable};
use gpui_kit::prelude::*;
use gpui_kit::{
    App, Context, Entity, EntityId, EventEmitter, FocusHandle, Focusable, IntoElement, SharedString, Subscription, Task, WeakEntity, Window, div, hsla, px,
};

use crate::actions::{
    AddCustomBreak, BookmarkBlue, BookmarkCyan, BookmarkGreen, BookmarkOrange, BookmarkPink, BookmarkPurple, BookmarkRed, BookmarkYellow, ClearAllBookmarks,
    ClearAllCustomBreaks, ClearBookmark, Copy, CopyAsBase64, CopyAsBinary, CopyAsCppArray, CopyAsEscapedString, CopyAsHexDump, CopyAsHexSpaces,
    CopyAsHexStream, CopyAsJsonArray, CopyAsPrintableText, CopyAsRustArray, Cut, FocusHexView, GoToBeginning, GoToEnd, HideAllBookmarks, JoinLine, Paste, Redo,
    RemoveCustomBreakBackward, RemoveCustomBreakForward, SearchNext, SearchPrev, SelectAll, ShowAllBookmarks, ToggleGoToAddress, ToggleHideUnbookmarked,
    ToggleSearch, Undo, UnfoldBookmarkAtCursor,
};
use crate::app_state::{AppState, InsertModeState};
use crate::core::appearance::Appearance;
use crate::core::editor::Editor;
use crate::core::search::SearchMode;
use crate::service::document_service::DocumentService;
use crate::ui::icon::IconName;
use crate::ui::views::hex_view::{self, HexView};
use std::ops::Range;
use std::path::PathBuf;

pub mod goto_offset_bar;
pub mod search_bar;

pub use goto_offset_bar::{GotoBarEvent, GotoOffsetBar};
pub use search_bar::{SearchBar, SearchBarEvent};

const CONTEXT: &str = "EditorPanel";

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
}

pub struct EditorPanel {
    editor: Entity<Editor>,
    focus_handle: FocusHandle,
    hex_view: Entity<HexView>,
    is_search_visible: bool,
    search_bar: Entity<SearchBar>,
    is_goto_visible: bool,
    goto_bar: Entity<GotoOffsetBar>,
    structure_reparse_task: Option<Task<()>>,
    tab_group: Option<WeakEntity<TabGroup>>,
    _appearance_subscription: Subscription,
    _bytes_per_row_subscription: Subscription,
    _editor_subscription: Subscription,
    _document_lease: Option<EditorDocumentLease>,
}

impl EditorPanel {
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

        cx.subscribe(&search_bar, |this, _, event: &SearchBarEvent, cx| match event {
            SearchBarEvent::IncrementalSearch(query, mode) => {
                this.perform_incremental_search(query, *mode, cx);
            }
            SearchBarEvent::FullSearch(query, mode) => {
                this.perform_incremental_search(query, *mode, cx);
            }
            SearchBarEvent::Next => {
                this.perform_search_next(cx);
            }
            SearchBarEvent::Prev => {
                this.perform_search_prev(cx);
            }
            SearchBarEvent::Dismiss => {
                this.is_search_visible = false;
                this.update_highlights(cx);
                cx.dispatch_action(&FocusHexView);
                cx.notify();
            }
        })
        .detach();

        cx.subscribe(&goto_bar, |this, _, event: &GotoBarEvent, cx| match event {
            GotoBarEvent::Jump { offset, extend_selection } => {
                let target = *offset;
                let extend = *extend_selection;
                this.editor.update(cx, |editor, cx| {
                    editor.go_to_offset(target, extend);
                    cx.notify();
                });
                let cursor_offset = this.editor.read(cx).cursor.offset;
                this.hex_view.update(cx, |view, cx| {
                    view.scroll_to_byte_if_needed(cursor_offset, cx);
                });
                this.is_goto_visible = false;
                cx.dispatch_action(&FocusHexView);
                cx.notify();
            }
            GotoBarEvent::SelectRange { range } => {
                let r = range.clone();
                this.editor.update(cx, |editor, cx| {
                    editor.go_to_range(r);
                    cx.notify();
                });
                let cursor_offset = this.editor.read(cx).cursor.offset;
                this.hex_view.update(cx, |view, cx| {
                    view.scroll_to_byte_if_needed(cursor_offset, cx);
                });
                this.is_goto_visible = false;
                cx.dispatch_action(&FocusHexView);
                cx.notify();
            }
            GotoBarEvent::Dismiss => {
                this.is_goto_visible = false;
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

        // Subscribe to HexView scroll events to update highlights when scrolling
        cx.subscribe(&hex_view, |this, _, event: &crate::ui::views::hex_view::HexViewEvent, cx| {
            if let crate::ui::views::hex_view::HexViewEvent::Scrolled(_) = event {
                this.update_highlights(cx);
            }
        })
        .detach();

        let bytes_per_row = cx.global::<crate::core::layout::BytesPerRow>().0;
        editor.update(cx, |editor, _| {
            editor.set_bytes_per_row(bytes_per_row);
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
            this.hex_view.update(cx, |_, cx| {
                cx.notify();
            });
            cx.notify();
        });

        let _editor_subscription = cx.observe(&editor, |this, editor, cx| {
            this.update_highlights(cx);

            if let Some((_, generation)) = editor.read(cx).pending_structure_reparse() {
                this.schedule_structure_reparse(generation, cx);
            }

            cx.notify();
        });

        // Observe search bar for incremental search
        cx.observe(&search_bar, |this, search_bar, cx| {
            if this.is_search_visible {
                let query = search_bar.read(cx).query(cx);
                let mode = search_bar.read(cx).mode();
                if query != this.editor.read(cx).search_state.query {
                    this.perform_incremental_search(&query, mode, cx);
                }
            }
        })
        .detach();

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

        Self {
            editor,
            focus_handle,
            hex_view,
            is_search_visible: false,
            search_bar,
            is_goto_visible: false,
            goto_bar,
            structure_reparse_task: None,
            tab_group: None,
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
    pub fn create_split_clone(&self, window: &mut Window, cx: &mut App) -> Entity<EditorPanel> {
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
            let panel = EditorPanel::new(new_editor, window, cx);
            panel.hex_view.update(cx, |hv, _| {
                hv.apply_layout_state(&layout_state);
            });
            panel
        })
    }

    pub fn toggle_search(&mut self, _: &ToggleSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.is_search_visible = !self.is_search_visible;
        if self.is_search_visible {
            self.is_goto_visible = false;
            self.search_bar.update(cx, |bar, cx| {
                bar.focus(window, cx);
            });
        } else {
            self.hex_view.read(cx).focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    pub fn toggle_goto_address(&mut self, _: &ToggleGoToAddress, window: &mut Window, cx: &mut Context<Self>) {
        self.is_goto_visible = !self.is_goto_visible;
        if self.is_goto_visible {
            self.is_search_visible = false;
            let cursor_offset = self.editor.read(cx).cursor.offset;
            let total_size = self.editor.read(cx).total_size();
            let address_map = self.editor.read(cx).document.read().expect("doc read").address_map.clone();
            self.goto_bar.update(cx, |bar, cx| {
                bar.set_context_info(cursor_offset, total_size, address_map, cx);
                bar.focus(window, cx);
            });
        } else {
            self.hex_view.read(cx).focus_handle(cx).focus(window, cx);
        }
        cx.notify();
    }

    fn perform_incremental_search(&mut self, query: &str, mode: SearchMode, cx: &mut Context<Self>) {
        if query.is_empty() {
            self.editor.update(cx, |editor: &mut Editor, cx| {
                editor.search_state_mut().clear();
                cx.notify();
            });
            self.update_highlights(cx);
            return;
        }

        self.editor.update(cx, |editor: &mut Editor, cx| {
            editor.search_state_mut().set_query_and_mode(query.to_string(), mode);
            cx.notify();
        });

        self.update_highlights(cx);
    }

    fn update_highlights(&mut self, cx: &mut Context<Self>) {
        let mut highlights = Vec::new();

        // 1. Add user custom bookmarks from editor
        let editor = self.editor.read(cx);
        highlights.extend(
            editor
                .bookmarks()
                .custom_bookmarks_for_rendering()
                .into_iter()
                .map(|(range, color)| (range, color.into())),
        );

        // 2. Add search highlights if search is active (either in search bar or search state)
        let (query, mode) = if self.is_search_visible && !self.search_bar.read(cx).query(cx).is_empty() {
            (self.search_bar.read(cx).query(cx), self.search_bar.read(cx).mode())
        } else if !editor.search_state.query.is_empty() {
            (editor.search_state.query.clone(), editor.search_state.mode)
        } else {
            (String::new(), SearchMode::Hex)
        };

        if !query.is_empty() {
            let pattern = match mode {
                crate::core::search::SearchMode::Text => crate::core::search::parse_text_pattern(&query, editor.options.encoding),
                crate::core::search::SearchMode::Hex => crate::core::search::parse_hex_pattern(&query),
            };

            if let Some(pattern) = pattern
                && !pattern.is_empty()
            {
                let pattern_len = pattern.len();
                let (start, end) = self.hex_view.read(cx).viewport_byte_range(cx);
                let scan_start = start.saturating_sub(pattern_len);
                let scan_end = end.saturating_add(pattern_len);

                if let Ok(doc) = editor.document.read() {
                    let data = doc.buffer.data();
                    let segments = doc.address_map.segment_ranges();
                    let matches = crate::core::search::find_occurrences_segmented(
                        data,
                        &pattern,
                        crate::core::search::SearchLimit::Unlimited,
                        &segments,
                        Some(scan_start..scan_end),
                    );
                    let is_dark = cx.theme().mode.is_dark();
                    let (search_color, current_result_color) = if is_dark {
                        (
                            // Translucent amber/gold for matches in dark mode
                            hsla(48.0 / 360.0, 0.85, 0.45, 0.35),
                            // Richer warm amber/orange for the current match at cursor
                            hsla(36.0 / 360.0, 0.95, 0.50, 0.55),
                        )
                    } else {
                        (
                            // Translucent yellow for matches in light mode
                            hsla(48.0 / 360.0, 0.90, 0.55, 0.35),
                            // Warm amber for the current match at cursor in light mode
                            hsla(36.0 / 360.0, 0.95, 0.50, 0.50),
                        )
                    };
                    let cursor_offset = editor.cursor.offset;

                    for result_offset in matches {
                        let is_current = result_offset == cursor_offset || (cursor_offset >= result_offset && cursor_offset < result_offset + pattern_len);
                        let color = if is_current { current_result_color } else { search_color };
                        highlights.push((result_offset..result_offset + pattern_len, color));
                    }
                }
            }
        }

        self.hex_view.update(cx, |view, cx| {
            view.set_highlights(highlights, cx);
        });
    }

    pub fn search_next(&mut self, _: &SearchNext, window: &mut Window, cx: &mut Context<Self>) {
        self.perform_search_next(cx);
        self.hex_view.read(cx).focus_handle(cx).focus(window, cx);
    }

    fn perform_search_next(&mut self, cx: &mut Context<Self>) {
        let (next_offset, pattern_len) = self.editor.update(cx, |editor: &mut Editor, cx| {
            let pattern_len = editor.search_pattern().map(|p| p.len()).unwrap_or(1);
            let offset = editor.next_search_result();
            cx.notify();
            (offset, pattern_len)
        });
        if let Some(offset) = next_offset {
            self.hex_view.update(cx, |view, cx| {
                view.scroll_to_range_if_needed(offset..offset.saturating_add(pattern_len), cx);
            });
        }
        self.update_highlights(cx);
        cx.notify();
    }

    pub fn search_prev(&mut self, _: &SearchPrev, window: &mut Window, cx: &mut Context<Self>) {
        self.perform_search_prev(cx);
        self.hex_view.read(cx).focus_handle(cx).focus(window, cx);
    }

    fn perform_search_prev(&mut self, cx: &mut Context<Self>) {
        let (prev_offset, pattern_len) = self.editor.update(cx, |editor: &mut Editor, cx| {
            let pattern_len = editor.search_pattern().map(|p| p.len()).unwrap_or(1);
            let offset = editor.prev_search_result();
            cx.notify();
            (offset, pattern_len)
        });
        if let Some(offset) = prev_offset {
            self.hex_view.update(cx, |view, cx| {
                view.scroll_to_range_if_needed(offset..offset.saturating_add(pattern_len), cx);
            });
        }
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
                editor.set_cursor_offset_exact(editor.total_size());
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
}

impl EventEmitter<PanelEvent> for EditorPanel {}

impl Focusable for EditorPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Panel for EditorPanel {
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

impl gpui_kit::base::dock::Panel for EditorPanel {
    fn panel_name(&self) -> &'static str {
        "EditorPanel"
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
        let panel_state = EditorPanelState {
            path: Some(self.editor.read(cx).document.read().expect("document read lock").path().to_path_buf()),
        };
        state.info = gpui_kit::component::dock::PanelInfo::panel(panel_state.to_value());
        state
    }
}

impl Render for EditorPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let container = div().size_full().flex().flex_col().key_context(CONTEXT).track_focus(&self.focus_handle);

        container
            .on_action(cx.listener(Self::toggle_search))
            .on_action(cx.listener(Self::toggle_goto_address))
            .on_action(cx.listener(Self::search_next))
            .on_action(cx.listener(Self::search_prev))
            .on_action(cx.listener(Self::focus_hex_view))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::go_to_beginning))
            .on_action(cx.listener(Self::go_to_end))
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
            .on_action(cx.listener(Self::join_line))
            .on_action(cx.listener(Self::clear_all_custom_breaks))
            .on_action(cx.listener(Self::show_all_bookmarks))
            .on_action(cx.listener(Self::hide_all_bookmarks))
            .on_action(cx.listener(Self::toggle_hide_unbookmarked))
            .on_action(cx.listener(Self::unfold_bookmark_at_cursor))
            .when(self.is_search_visible, |el| el.child(self.search_bar.clone()))
            .when(self.is_goto_visible, |el| el.child(self.goto_bar.clone()))
            .child(div().flex_1().w_full().min_h_0().child(self.hex_view.clone()))
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct EditorPanelState {
    pub path: Option<std::path::PathBuf>,
}

impl EditorPanelState {
    #[allow(dead_code)]
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self).expect("serialize EditorPanelState")
    }

    #[allow(dead_code)]
    pub fn from_value(value: serde_json::Value) -> Option<Self> {
        serde_json::from_value(value).ok()
    }
}

use crate::core::appearance::Appearance;
use crate::core::bookmark::{BookmarkColor, BookmarkItem};
use crate::core::editor::Editor;
use crate::ui::icon::IconName;
use crate::ui::style::BookmarkColorExt;
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Disableable, Sizable, Size, StyledExt, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

actions!(
    bookmark_panel,
    [
        MoveUp,
        MoveDown,
        MoveTop,
        MoveBottom,
        PageUp,
        PageDown,
        SelectCurrent,
        EditComment,
        CancelEdit,
        SaveEdit,
        DeleteSelected
    ]
);

const CONTEXT: &str = "BookmarkPanel";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", MoveUp, Some("BookmarkPanel && !CommentEdit && !Input")),
        KeyBinding::new("down", MoveDown, Some("BookmarkPanel && !CommentEdit && !Input")),
        KeyBinding::new("k", MoveUp, Some("BookmarkPanel && !CommentEdit && !Input")),
        KeyBinding::new("j", MoveDown, Some("BookmarkPanel && !CommentEdit && !Input")),
        KeyBinding::new("home", MoveTop, Some("BookmarkPanel && !CommentEdit && !Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-home", MoveTop, Some("BookmarkPanel && !CommentEdit && !Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-home", MoveTop, Some("BookmarkPanel && !CommentEdit && !Input")),
        KeyBinding::new("end", MoveBottom, Some("BookmarkPanel && !CommentEdit && !Input")),
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-end", MoveBottom, Some("BookmarkPanel && !CommentEdit && !Input")),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-end", MoveBottom, Some("BookmarkPanel && !CommentEdit && !Input")),
        KeyBinding::new("pageup", PageUp, Some("BookmarkPanel && !CommentEdit && !Input")),
        KeyBinding::new("pagedown", PageDown, Some("BookmarkPanel && !CommentEdit && !Input")),
        KeyBinding::new("f2", EditComment, Some("BookmarkPanel && !CommentEdit && !Input")),
        KeyBinding::new("enter", SelectCurrent, Some("BookmarkPanel && !CommentEdit && !Input")),
        KeyBinding::new("backspace", DeleteSelected, Some("BookmarkPanel && !CommentEdit && !Input")),
        KeyBinding::new("delete", DeleteSelected, Some("BookmarkPanel && !CommentEdit && !Input")),
        KeyBinding::new("escape", CancelEdit, Some("CommentEdit")),
        KeyBinding::new("enter", SaveEdit, Some("CommentEdit")),
    ]);
}

#[allow(dead_code)]
pub enum BookmarkPanelEvent {
    NavigateTo { offset: usize, size: usize },
    Export,
    Import,
}

pub struct BookmarkPanel {
    pub editor: Option<Entity<Editor>>,
    pub focus_handle: FocusHandle,
    pub selected_id: Option<String>,
    pub editing_id: Option<String>,
    pub comment_input: Entity<InputState>,
    pub color_picker_id: Option<String>,
    _editor_subscription: Option<Subscription>,
    _input_subscription: Option<Subscription>,
}

impl EventEmitter<BookmarkPanelEvent> for BookmarkPanel {}

impl BookmarkPanel {
    pub fn new(editor: Option<Entity<Editor>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let comment_input = cx.new(|cx| InputState::new(window, cx).placeholder("Add a comment..."));

        let input_sub = cx.subscribe_in(&comment_input, window, |this, _, event: &input::InputEvent, window, cx| {
            if let input::InputEvent::PressEnter { .. } = event {
                this.save_editing_comment(window, cx);
            }
        });

        let mut this = Self {
            editor: None,
            focus_handle,
            selected_id: None,
            editing_id: None,
            comment_input,
            color_picker_id: None,
            _editor_subscription: None,
            _input_subscription: Some(input_sub),
        };

        this.set_editor(editor, cx);
        this
    }

    pub fn set_editor(&mut self, editor: Option<Entity<Editor>>, cx: &mut Context<Self>) {
        self._editor_subscription = None;
        self.editor = editor.clone();
        self.selected_id = None;
        self.editing_id = None;
        self.color_picker_id = None;

        if let Some(ed) = &editor {
            self._editor_subscription = Some(cx.observe(ed, |this, editor, cx| {
                this.sync_selected_from_cursor(&editor, cx);
                cx.notify();
            }));
            self.sync_selected_from_cursor(ed, cx);
        }
        cx.notify();
    }

    fn sync_selected_from_cursor(&mut self, editor: &Entity<Editor>, cx: &mut Context<Self>) {
        if self.editing_id.is_some() {
            return;
        }
        let (cursor_offset, bookmarks) = {
            let ed = editor.read(cx);
            (ed.cursor.offset, ed.bookmarks().snapshot())
        };

        if bookmarks.is_empty() {
            self.selected_id = None;
            return;
        }

        if let Some(ref sel_id) = self.selected_id
            && let Some(item) = bookmarks.iter().find(|h| &h.id == sel_id)
        {
            let range = item.offset..item.offset + item.size;
            if (item.size > 0 && range.contains(&cursor_offset)) || (item.size == 0 && item.offset == cursor_offset) {
                return;
            }
        }

        if let Some(item) = bookmarks.iter().find(|h| {
            let range = h.offset..h.offset + h.size;
            (h.size > 0 && range.contains(&cursor_offset)) || (h.size == 0 && h.offset == cursor_offset)
        }) {
            self.selected_id = Some(item.id.clone());
        }
    }

    fn add_bookmark_from_selection(&mut self, cx: &mut Context<Self>) {
        let Some(editor_entity) = &self.editor else { return };
        let (range, _) = {
            let editor = editor_entity.read(cx);
            if let Some(r) = editor.selected_range_or_cursor() {
                (r, editor.total_size())
            } else {
                return;
            }
        };

        let new_item = BookmarkItem::new(range.start, range.len(), BookmarkColor::Yellow, "");
        let mut actual_id = String::new();

        editor_entity.update(cx, |editor, cx| {
            actual_id = editor.bookmarks_mut().add(new_item);
            cx.notify();
        });

        if !actual_id.is_empty() {
            self.selected_id = Some(actual_id);
        }
        cx.notify();
    }

    fn start_editing_comment(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let current_comment = self
            .editor
            .as_ref()
            .and_then(|ed| {
                let editor = ed.read(cx);
                editor.bookmarks().by_id(&id).map(|h| h.comment)
            })
            .unwrap_or_default();

        self.editing_id = Some(id.clone());
        self.selected_id = Some(id);
        self.color_picker_id = None;

        self.comment_input.update(cx, |input, cx| {
            input.set_value(current_comment, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn notify_document_changed(&self, cx: &mut App) {
        let path = self
            .editor
            .as_ref()
            .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.path().to_path_buf()));
        if let Some(path) = path {
            let service = crate::app_state::AppState::global(cx).document_service.clone();
            service.notify_document_changed(&path, cx);
        }
    }

    fn save_editing_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editing_id) = self.editing_id.take() else { return };
        let new_comment = self.comment_input.read(cx).value().to_string();

        if let Some(ed) = &self.editor {
            ed.update(cx, |editor, cx| {
                editor.bookmarks_mut().update_comment(&editing_id, new_comment);
                cx.notify();
            });
            self.notify_document_changed(cx);
        }
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn cancel_editing_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing_id = None;
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    fn set_bookmark_color(&mut self, id: &str, color: BookmarkColor, cx: &mut Context<Self>) {
        if let Some(ed) = &self.editor {
            ed.update(cx, |editor, cx| {
                editor.bookmarks_mut().update_color(id, color);
                cx.notify();
            });
            self.notify_document_changed(cx);
        }
        self.color_picker_id = None;
        cx.notify();
    }

    fn delete_bookmark(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(ed) = &self.editor {
            ed.update(cx, |editor, cx| {
                editor.bookmarks_mut().remove_by_id(id);
                cx.notify();
            });
            self.notify_document_changed(cx);
        }
        if self.selected_id.as_deref() == Some(id) {
            self.selected_id = None;
        }
        if self.editing_id.as_deref() == Some(id) {
            self.editing_id = None;
        }
        if self.color_picker_id.as_deref() == Some(id) {
            self.color_picker_id = None;
        }
        cx.notify();
    }

    fn clear_all_bookmarks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor_entity) = &self.editor else { return };
        let count = editor_entity.read(cx).bookmarks().snapshot().len();
        if count == 0 {
            return;
        }

        let prompt = window.prompt(
            PromptLevel::Warning,
            "Clear all bookmarks?",
            Some(&format!(
                "Are you sure you want to clear all {} bookmark{} and comments? This action cannot be undone.",
                count,
                if count == 1 { "" } else { "s" }
            )),
            &["Clear All", "Cancel"],
            cx,
        );

        let editor_entity = editor_entity.clone();
        let doc_path = editor_entity.read(cx).document.read().ok().map(|d| d.path().to_path_buf());
        cx.spawn_in(window, async move |this, window| {
            if let Ok(0) = prompt.await {
                window
                    .update(|_, cx| {
                        editor_entity.update(cx, |editor, cx| {
                            editor.bookmarks_mut().clear_all();
                            cx.notify();
                        });
                        if let Some(ref path) = doc_path {
                            let service = crate::app_state::AppState::global(cx).document_service.clone();
                            service.notify_document_changed(path, cx);
                        }
                        let _ = this.update(cx, |panel, cx| {
                            panel.selected_id = None;
                            panel.editing_id = None;
                            panel.color_picker_id = None;
                            cx.notify();
                        });
                    })
                    .ok();
            }
        })
        .detach();
    }

    fn navigate_to_bookmark(&mut self, offset: usize, size: usize, cx: &mut Context<Self>) {
        if let Some(ed) = &self.editor {
            ed.update(cx, |editor, cx| {
                if size > 0 {
                    editor.set_selection_range(offset..offset.saturating_add(size));
                } else {
                    editor.set_cursor_offset(offset);
                }
                cx.notify();
            });
        }
        cx.emit(BookmarkPanelEvent::NavigateTo { offset, size });
        cx.notify();
    }

    fn move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_id.is_some() {
            return;
        }
        let Some(ed) = &self.editor else { return };
        let bookmarks = ed.read(cx).bookmarks().snapshot();
        if bookmarks.is_empty() {
            return;
        }

        let curr_idx = self.selected_id.as_ref().and_then(|id| bookmarks.iter().position(|h| &h.id == id));

        let next_idx = match curr_idx {
            Some(idx) => idx.saturating_sub(1),
            None => 0,
        };

        let target_item = &bookmarks[next_idx];
        self.selected_id = Some(target_item.id.clone());
        self.color_picker_id = None;
        self.navigate_to_bookmark(target_item.offset, target_item.size, cx);
    }

    fn move_down(&mut self, _: &MoveDown, _window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_id.is_some() {
            return;
        }
        let Some(ed) = &self.editor else { return };
        let bookmarks = ed.read(cx).bookmarks().snapshot();
        if bookmarks.is_empty() {
            return;
        }

        let max_idx = bookmarks.len() - 1;
        let curr_idx = self.selected_id.as_ref().and_then(|id| bookmarks.iter().position(|h| &h.id == id));

        let next_idx = match curr_idx {
            Some(idx) => (idx + 1).min(max_idx),
            None => 0,
        };

        let target_item = &bookmarks[next_idx];
        self.selected_id = Some(target_item.id.clone());
        self.color_picker_id = None;
        self.navigate_to_bookmark(target_item.offset, target_item.size, cx);
    }

    fn move_top(&mut self, _: &MoveTop, _window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_id.is_some() {
            return;
        }
        let Some(ed) = &self.editor else { return };
        let bookmarks = ed.read(cx).bookmarks().snapshot();
        if bookmarks.is_empty() {
            return;
        }

        let target_item = &bookmarks[0];
        self.selected_id = Some(target_item.id.clone());
        self.color_picker_id = None;
        self.navigate_to_bookmark(target_item.offset, target_item.size, cx);
    }

    fn move_bottom(&mut self, _: &MoveBottom, _window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_id.is_some() {
            return;
        }
        let Some(ed) = &self.editor else { return };
        let bookmarks = ed.read(cx).bookmarks().snapshot();
        if bookmarks.is_empty() {
            return;
        }

        let last_idx = bookmarks.len() - 1;
        let target_item = &bookmarks[last_idx];
        self.selected_id = Some(target_item.id.clone());
        self.color_picker_id = None;
        self.navigate_to_bookmark(target_item.offset, target_item.size, cx);
    }

    fn page_up(&mut self, _: &PageUp, _window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_id.is_some() {
            return;
        }
        let Some(ed) = &self.editor else { return };
        let bookmarks = ed.read(cx).bookmarks().snapshot();
        if bookmarks.is_empty() {
            return;
        }

        let curr_idx = self.selected_id.as_ref().and_then(|id| bookmarks.iter().position(|h| &h.id == id)).unwrap_or(0);
        let next_idx = curr_idx.saturating_sub(10);
        let target_item = &bookmarks[next_idx];
        self.selected_id = Some(target_item.id.clone());
        self.color_picker_id = None;
        self.navigate_to_bookmark(target_item.offset, target_item.size, cx);
    }

    fn page_down(&mut self, _: &PageDown, _window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_id.is_some() {
            return;
        }
        let Some(ed) = &self.editor else { return };
        let bookmarks = ed.read(cx).bookmarks().snapshot();
        if bookmarks.is_empty() {
            return;
        }

        let max_idx = bookmarks.len() - 1;
        let curr_idx = self.selected_id.as_ref().and_then(|id| bookmarks.iter().position(|h| &h.id == id)).unwrap_or(0);
        let next_idx = (curr_idx + 10).min(max_idx);
        let target_item = &bookmarks[next_idx];
        self.selected_id = Some(target_item.id.clone());
        self.color_picker_id = None;
        self.navigate_to_bookmark(target_item.offset, target_item.size, cx);
    }

    fn select_current(&mut self, _: &SelectCurrent, _window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_id.is_some() {
            return;
        }
        let Some(ed) = &self.editor else { return };
        let bookmarks = ed.read(cx).bookmarks().snapshot();
        if let Some(item) = self.selected_id.as_ref().and_then(|id| bookmarks.iter().find(|h| &h.id == id)) {
            self.navigate_to_bookmark(item.offset, item.size, cx);
        } else if let Some(first) = bookmarks.first() {
            self.selected_id = Some(first.id.clone());
            self.navigate_to_bookmark(first.offset, first.size, cx);
        }
    }

    fn edit_comment(&mut self, _: &EditComment, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_id.is_some() {
            return;
        }
        if let Some(id) = self.selected_id.clone() {
            self.start_editing_comment(id, window, cx);
        }
    }

    fn delete_selected(&mut self, _: &DeleteSelected, _window: &mut Window, cx: &mut Context<Self>) {
        if self.editing_id.is_some() {
            return;
        }
        if let Some(id) = self.selected_id.clone() {
            self.delete_bookmark(&id, cx);
        }
    }
}

impl Focusable for BookmarkPanel {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for BookmarkPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let is_focused = self.focus_handle.is_focused(window);

        let (bookmarks, has_editor) = if let Some(ed) = &self.editor {
            let editor = ed.read(cx);
            (editor.bookmarks().snapshot(), true)
        } else {
            (Vec::new(), false)
        };

        let count = bookmarks.len();

        // Header toolbar
        let badge = Some(crate::ui::style::panel_badge(count.to_string(), &theme).into_any_element());

        let actions = h_flex()
            .items_center()
            .gap_1()
            .child(
                Button::new("add-bm")
                    .ghost()
                    .icon(IconName::BookmarkPlus)
                    .with_size(Size::XSmall)
                    .tooltip("Add bookmark at current selection / cursor")
                    .disabled(!has_editor)
                    .on_click(cx.listener(|this, _, _window, cx| {
                        this.add_bookmark_from_selection(cx);
                    })),
            )
            .child(
                Button::new("import-bm")
                    .ghost()
                    .icon(IconName::Import)
                    .with_size(Size::XSmall)
                    .tooltip("Import bookmarks from YAML file")
                    .disabled(!has_editor)
                    .on_click(cx.listener(|_, _, _window, cx| {
                        cx.emit(BookmarkPanelEvent::Import);
                    })),
            )
            .child(
                Button::new("export-bm")
                    .ghost()
                    .icon(IconName::HardDriveDownload)
                    .with_size(Size::XSmall)
                    .tooltip("Export bookmarks to YAML file")
                    .disabled(!has_editor || count == 0)
                    .on_click(cx.listener(|_, _, _window, cx| {
                        cx.emit(BookmarkPanelEvent::Export);
                    })),
            )
            .child(
                Button::new("clear-bm")
                    .ghost()
                    .icon(IconName::Eraser)
                    .with_size(Size::XSmall)
                    .tooltip("Clear all bookmarks")
                    .disabled(!has_editor || count == 0)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.clear_all_bookmarks(window, cx);
                    })),
            );

        let header = crate::ui::style::panel_header("BOOKMARKS", is_focused, &theme, badge, Some(actions.into_any_element()));

        let filter_toolbar = if !has_editor || bookmarks.is_empty() {
            None
        } else {
            let mut chips_row = h_flex().items_center().gap_1().flex_wrap();

            for &preset_color in BookmarkColor::ALL_PRESETS {
                let color_count = bookmarks.iter().filter(|b| b.color == preset_color).count();
                if color_count == 0 {
                    continue;
                }
                let is_color_hidden = self
                    .editor
                    .as_ref()
                    .map(|ed| ed.read(cx).bookmarks().is_color_hidden(preset_color))
                    .unwrap_or(false);
                let badge_hsla = preset_color.to_badge_hsla();

                let chip = Button::new(SharedString::from(format!("filter-bm-{}", preset_color.name())))
                    .ghost()
                    .with_size(Size::XSmall)
                    .tooltip(if is_color_hidden {
                        format!("Expand {} bookmarks (currently folded)", preset_color.name())
                    } else {
                        format!("Fold {} bookmarks", preset_color.name())
                    })
                    .on_click(cx.listener(move |this, _, _window, cx| {
                        if let Some(ed) = &this.editor {
                            ed.update(cx, |editor, cx| {
                                editor.bookmarks_mut().toggle_color(preset_color);
                                cx.notify();
                            });
                            cx.notify();
                        }
                    }))
                    .child(
                        h_flex()
                            .items_center()
                            .gap_1()
                            .opacity(if is_color_hidden { 0.45 } else { 1.0 })
                            .child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(badge_hsla))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(if is_color_hidden { theme.muted_foreground } else { theme.foreground })
                                    .child(color_count.to_string()),
                            ),
                    );

                chips_row = chips_row.child(chip);
            }

            let is_hide_unbookmarked = self.editor.as_ref().map(|ed| ed.read(cx).bookmarks().is_hide_unbookmarked()).unwrap_or(false);

            let filter_actions = h_flex()
                .items_center()
                .gap_0p5()
                .child({
                    let btn = Button::new("toggle-hide-unbookmarked")
                        .icon(IconName::Bookmark)
                        .with_size(Size::XSmall)
                        .tooltip(if is_hide_unbookmarked {
                            "Show all file data (currently showing bookmarks only)"
                        } else {
                            "Show only bookmarked regions (fold unbookmarked data)"
                        })
                        .on_click(cx.listener(|this, _, _window, cx| {
                            if let Some(ed) = &this.editor {
                                ed.update(cx, |editor, cx| {
                                    editor.bookmarks_mut().toggle_hide_unbookmarked();
                                    cx.notify();
                                });
                                cx.notify();
                            }
                        }));
                    if is_hide_unbookmarked { btn.primary() } else { btn.ghost() }
                })
                .child(
                    Button::new("expand-all-bm")
                        .ghost()
                        .icon(IconName::Eye)
                        .with_size(Size::XSmall)
                        .tooltip("Expand all bookmarks")
                        .on_click(cx.listener(|this, _, _window, cx| {
                            if let Some(ed) = &this.editor {
                                ed.update(cx, |editor, cx| {
                                    editor.bookmarks_mut().show_all();
                                    cx.notify();
                                });
                                cx.notify();
                            }
                        })),
                )
                .child(
                    Button::new("fold-all-bm")
                        .ghost()
                        .icon(IconName::EyeOff)
                        .with_size(Size::XSmall)
                        .tooltip("Fold all bookmarks")
                        .on_click(cx.listener(|this, _, _window, cx| {
                            if let Some(ed) = &this.editor {
                                ed.update(cx, |editor, cx| {
                                    editor.bookmarks_mut().hide_all();
                                    cx.notify();
                                });
                                cx.notify();
                            }
                        })),
                );

            let filter_row = h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .px_2()
                .py_1()
                .bg(theme.muted.opacity(0.15))
                .border_b_1()
                .border_color(theme.border.opacity(0.4))
                .child(chips_row)
                .child(filter_actions);

            Some(filter_row)
        };

        // Content body
        let body = if !has_editor {
            crate::ui::style::panel_empty_state(
                IconName::Bookmark,
                "No Active File",
                Some("Open a binary file to view and manage bookmarks"),
                None,
                &theme,
            )
            .into_any_element()
        } else if bookmarks.is_empty() {
            crate::ui::style::panel_empty_state(
                IconName::Bookmark,
                "No Bookmarks",
                Some("Select bytes in hex view and choose a color, or click the add icon above"),
                None,
                &theme,
            )
            .into_any_element()
        } else {
            let mut list = v_flex().flex_1().gap_1().p_1();

            for item in bookmarks {
                list = list.child(self.render_bookmark_item(&item, &theme, window, cx));
            }

            v_flex()
                .flex_1()
                .overflow_hidden()
                .child(div().id("bookmarks-scroll").flex_1().overflow_y_scroll().child(list))
                .into_any_element()
        };

        let mut container = crate::ui::style::panel_container(is_focused, &theme)
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::move_up))
            .on_action(cx.listener(Self::move_down))
            .on_action(cx.listener(Self::move_top))
            .on_action(cx.listener(Self::move_bottom))
            .on_action(cx.listener(Self::page_up))
            .on_action(cx.listener(Self::page_down))
            .on_action(cx.listener(Self::select_current))
            .on_action(cx.listener(Self::edit_comment))
            .on_action(cx.listener(Self::delete_selected))
            .child(header);

        if let Some(toolbar) = filter_toolbar {
            container = container.child(toolbar);
        }

        container.child(body)
    }
}

impl BookmarkPanel {
    fn render_bookmark_item(&self, item: &BookmarkItem, theme: &gpui_kit::component::Theme, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let item_id = item.id.clone();
        let is_selected = self.selected_id.as_deref() == Some(&item_id);
        let is_editing = self.editing_id.as_deref() == Some(&item_id);
        let show_color_picker = self.color_picker_id.as_deref() == Some(&item_id);

        let offset = item.offset;
        let size = item.size;
        let badge_color = item.color.to_badge_hsla();

        let item_id_edit = item_id.clone();
        let item_id_del = item_id.clone();
        let item_id_color = item_id.clone();
        let item_id_select = item_id.clone();

        let bg_color = if is_selected { theme.selection } else { theme.sidebar };

        let is_item_hidden = self.editor.as_ref().map(|ed| ed.read(cx).bookmarks().is_item_hidden(item)).unwrap_or(false);
        let item_id_vis = item_id.clone();

        let mut row_container = v_flex()
            .w_full()
            .rounded_md()
            .border_1()
            .border_color(if is_selected { theme.accent } else { theme.border.opacity(0.5) })
            .bg(bg_color)
            .opacity(if is_item_hidden { 0.6 } else { 1.0 })
            .p_2()
            .gap_1p5();

        // 1. Header row: Color Dot + Offset + Size + Action Buttons
        let font_family = cx.global::<Appearance>().font_family.clone();
        let header_row = h_flex()
            .w_full()
            .justify_between()
            .items_center()
            .child(
                h_flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .id(SharedString::from(format!("color-badge-{}", item_id)))
                            .w(px(12.0))
                            .h(px(12.0))
                            .rounded_full()
                            .bg(badge_color)
                            .cursor_pointer()
                            .on_click(cx.listener({
                                let item_id = item_id_color.clone();
                                move |this, _, _window, cx| {
                                    if this.color_picker_id.as_deref() == Some(&item_id) {
                                        this.color_picker_id = None;
                                    } else {
                                        this.color_picker_id = Some(item_id.clone());
                                    }
                                    cx.notify();
                                }
                            })),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("nav-link-{}", item_id)))
                            .cursor_pointer()
                            .on_click(cx.listener({
                                let item_id = item_id_select.clone();
                                move |this, _, window, cx| {
                                    this.focus_handle.focus(window, cx);
                                    this.selected_id = Some(item_id.clone());
                                    this.navigate_to_bookmark(offset, size, cx);
                                }
                            }))
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .font_family(font_family.clone())
                                            .text_xs()
                                            .font_semibold()
                                            .text_color(theme.foreground)
                                            .child({
                                                let address_map = self
                                                    .editor
                                                    .as_ref()
                                                    .and_then(|ed| ed.read(cx).document.read().ok().map(|d| d.address_map.clone()))
                                                    .unwrap_or_default();
                                                format!("0x{:08X}", address_map.offset_to_address(item.offset))
                                            }),
                                    )
                                    .child(
                                        div()
                                            .px_1()
                                            .py_0p5()
                                            .rounded_sm()
                                            .bg(theme.muted.opacity(0.5))
                                            .text_xs()
                                            .font_family(font_family.clone())
                                            .text_color(theme.muted_foreground)
                                            .child(format!("{} B", item.size)),
                                    ),
                            ),
                    ),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap_0p5()
                    .child(
                        Button::new(SharedString::from(format!("vis-{}", item_id)))
                            .ghost()
                            .icon(if is_item_hidden { IconName::EyeOff } else { IconName::Eye })
                            .with_size(Size::XSmall)
                            .tooltip(if is_item_hidden {
                                "Expand bookmark in hex view"
                            } else {
                                "Fold bookmark in hex view"
                            })
                            .on_click(cx.listener({
                                let item_id = item_id_vis.clone();
                                move |this, _, _window, cx| {
                                    if let Some(ed) = &this.editor {
                                        ed.update(cx, |editor, cx| {
                                            editor.bookmarks_mut().toggle_item_visibility(&item_id);
                                            cx.notify();
                                        });
                                        cx.notify();
                                    }
                                }
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("nav-{}", item_id)))
                            .ghost()
                            .icon(IconName::Binoculars)
                            .with_size(Size::XSmall)
                            .tooltip("Go to address")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.focus_handle.focus(window, cx);
                                this.navigate_to_bookmark(offset, size, cx);
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("edit-{}", item_id)))
                            .ghost()
                            .icon(IconName::PenLine)
                            .with_size(Size::XSmall)
                            .tooltip("Edit comment")
                            .on_click(cx.listener({
                                let item_id = item_id_edit.clone();
                                move |this, _, window, cx| {
                                    this.start_editing_comment(item_id.clone(), window, cx);
                                }
                            })),
                    )
                    .child(
                        Button::new(SharedString::from(format!("del-{}", item_id)))
                            .ghost()
                            .icon(IconName::BookmarkX)
                            .with_size(Size::XSmall)
                            .tooltip("Delete bookmark")
                            .on_click(cx.listener({
                                let item_id = item_id_del.clone();
                                move |this, _, _window, cx| {
                                    this.delete_bookmark(&item_id, cx);
                                }
                            })),
                    ),
            );

        row_container = row_container.child(header_row);

        // 2. Optional Color Picker row
        if show_color_picker {
            let mut picker_row = h_flex().items_center().gap_1().py_1().px_1().bg(theme.muted.opacity(0.3)).rounded_sm();
            for &preset in BookmarkColor::ALL_PRESETS {
                let p_badge = preset.to_badge_hsla();
                let item_id_for_preset = item_id.clone();
                let is_current = item.color == preset;

                picker_row = picker_row.child(
                    div()
                        .id(SharedString::from(format!("palette-{}-{}", item_id, preset.name())))
                        .w(px(14.0))
                        .h(px(14.0))
                        .rounded_full()
                        .bg(p_badge)
                        .cursor_pointer()
                        .border_1()
                        .border_color(if is_current { theme.foreground } else { hsla(0.0, 0.0, 0.0, 0.0) })
                        .hover(|s| s.opacity(0.8))
                        .on_click(cx.listener({
                            let item_id = item_id_for_preset.clone();
                            move |this, _, _window, cx| {
                                this.set_bookmark_color(&item_id, preset, cx);
                            }
                        })),
                );
            }
            row_container = row_container.child(picker_row);
        }

        // 3. Comment Display or Edit Mode
        if is_editing {
            let edit_box = v_flex()
                .key_context("CommentEdit")
                .on_action(cx.listener(|this, _: &CancelEdit, window, cx| {
                    this.cancel_editing_comment(window, cx);
                }))
                .on_action(cx.listener(|this, _: &SaveEdit, window, cx| {
                    this.save_editing_comment(window, cx);
                }))
                .gap_1p5()
                .p_1()
                .bg(theme.background)
                .rounded_sm()
                .child(Input::new(&self.comment_input).cleanable(true))
                .child(
                    h_flex()
                        .justify_end()
                        .gap_1()
                        .child(
                            Button::new("cancel-edit")
                                .ghost()
                                .with_size(Size::XSmall)
                                .label("Cancel")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.cancel_editing_comment(window, cx);
                                })),
                        )
                        .child(
                            Button::new("save-edit")
                                .primary()
                                .with_size(Size::XSmall)
                                .label("Save")
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.save_editing_comment(window, cx);
                                })),
                        ),
                );
            row_container = row_container.child(edit_box);
        } else {
            let comment_text = if item.comment.trim().is_empty() {
                div().text_xs().italic().text_color(theme.muted_foreground.opacity(0.7)).child("(No comment)")
            } else {
                div().text_xs().text_color(theme.foreground).child(item.comment.clone())
            };

            let comment_row = div()
                .id(SharedString::from(format!("comment-row-{}", item_id)))
                .w_full()
                .cursor_pointer()
                .on_click(cx.listener({
                    let item_id = item_id.clone();
                    move |this, _, window, cx| {
                        this.start_editing_comment(item_id.clone(), window, cx);
                    }
                }))
                .child(comment_text);

            row_container = row_container.child(comment_row);
        }

        row_container
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_bookmark_navigation_indices() {
        let count = 15;
        let move_up = |curr: Option<usize>| match curr {
            Some(idx) => idx.saturating_sub(1),
            None => 0,
        };
        assert_eq!(move_up(Some(5)), 4);
        assert_eq!(move_up(Some(0)), 0);

        let move_down = |curr: Option<usize>| match curr {
            Some(idx) => (idx + 1).min(count - 1),
            None => 0,
        };
        assert_eq!(move_down(Some(5)), 6);
        assert_eq!(move_down(Some(14)), 14);

        let page_up = |curr: usize| curr.saturating_sub(10);
        let page_down = |curr: usize| (curr + 10).min(count - 1);
        assert_eq!(page_up(12), 2);
        assert_eq!(page_up(4), 0);
        assert_eq!(page_down(2), 12);
        assert_eq!(page_down(10), 14);
    }
}

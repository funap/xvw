use std::ops::Range;
use std::path::{Path, PathBuf};

use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, StyledExt,
    button::{Button, ButtonVariants},
    dock::{Panel, PanelControl, PanelEvent},
    input::{self, Editor, EditorState, Input, InputState},
    text::{FrontmatterPlugin, MarkdownExtensions, RangeHighlight, RenderedText, SelectionFormat, TextView, TextViewState},
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::ui::icon::IconName;

const CONTEXT: &str = "ScratchpadView";
const SEARCH_CONTEXT: &str = "ScratchpadSearch";
pub const DEFAULT_SCRATCHPAD_CONTENT: &str = "# Scratchpad\n\nTake quick notes, draft structures, or record findings here.\n\n## Notes\n- \n";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ScratchpadMode {
    #[default]
    Split,
    Edit,
    Preview,
}

/// Pure helper for finding non-overlapping, case-insensitive substring matches.
///
/// Returns byte ranges referencing valid UTF-8 boundaries in `text`.
pub fn find_case_insensitive_matches(text: &str, query: &str) -> Vec<Range<usize>> {
    let query = query.trim();
    if query.is_empty() || text.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    let query_chars: Vec<char> = query_lower.chars().collect();
    let mut matches = Vec::new();
    let mut last_match_end = 0;

    for (start, _) in text.char_indices() {
        if start < last_match_end {
            continue;
        }

        let mut t_chars = text[start..].chars();
        let mut matched = true;
        let mut matched_bytes = 0;

        for &q_char in &query_chars {
            match t_chars.next() {
                Some(t_char) => {
                    let mut t_lower = t_char.to_lowercase();
                    if t_lower.next() != Some(q_char) || t_lower.next().is_some() {
                        matched = false;
                        break;
                    }
                    matched_bytes += t_char.len_utf8();
                }
                None => {
                    matched = false;
                    break;
                }
            }
        }

        if matched && matched_bytes > 0 {
            let end = start + matched_bytes;
            if text.is_char_boundary(start) && text.is_char_boundary(end) {
                matches.push(start..end);
                last_match_end = end;
            }
        }
    }

    matches
}

pub struct ScratchpadView {
    id: usize,
    file_path: PathBuf,
    title: String,
    cached_content: String,
    debounce_task: Option<Task<()>>,
    is_dirty: bool,
    focus_handle: FocusHandle,
    editor: Entity<EditorState>,
    text_view_state: Entity<TextViewState>,
    search_input: Entity<InputState>,
    is_search_open: bool,
    search_matches: Vec<Range<usize>>,
    current_match_index: usize,
    last_searched_text: Option<RenderedText>,
    mode: ScratchpadMode,
    _subscriptions: Vec<Subscription>,
}

impl ScratchpadView {
    #[allow(dead_code)]
    pub fn new(id: usize, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let file_path = crate::service::ScratchService::scratch_file_path(id).unwrap_or_else(|| PathBuf::from(format!("scratch_{id}.md")));
        let content = if file_path.exists() {
            crate::service::ScratchService::load_scratch(&file_path).unwrap_or_else(|_| DEFAULT_SCRATCHPAD_CONTENT.to_string())
        } else {
            DEFAULT_SCRATCHPAD_CONTENT.to_string()
        };
        Self::new_with_file(id, file_path, content, window, cx)
    }

    pub fn new_with_file(id: usize, file_path: PathBuf, content: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title = crate::service::ScratchService::extract_title(&content).unwrap_or_else(|| format!("Scratchpad {id}"));
        let cached_content = content.clone();
        let focus_handle = cx.focus_handle();
        let editor = cx.new(|cx| {
            let mut state = EditorState::new(window, cx).language("markdown");
            state.set_value(content.clone(), window, cx);
            state
        });
        let text_view_state = cx.new(|cx| TextViewState::markdown(&content, cx));
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search in note..."));

        let mut subscriptions = Vec::new();

        let sub_editor = cx.subscribe_in(&editor, window, |this, editor, event: &input::InputEvent, _window, cx| {
            if let input::InputEvent::Change = event {
                let content = editor.read(cx).value().to_string();
                this.cached_content = content.clone();
                this.title = crate::service::ScratchService::extract_title(&content).unwrap_or_else(|| format!("Scratchpad {}", this.id));
                this.is_dirty = true;
                this.text_view_state.update(cx, |tv, cx| {
                    tv.set_text(&content, cx);
                });
                if this.is_search_open {
                    this.update_search(cx);
                }

                // Debounced auto-save (500ms)
                this.debounce_task = None;
                let task = cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |this, cx| {
                            this.save_sync();
                            cx.notify();
                        });
                    }
                });
                this.debounce_task = Some(task);

                cx.notify();
            }
        });
        subscriptions.push(sub_editor);

        let sub_search = cx.subscribe_in(&search_input, window, |this, _, event: &input::InputEvent, _window, cx| match event {
            input::InputEvent::Change => {
                this.update_search(cx);
                cx.notify();
            }
            input::InputEvent::PressEnter { shift, .. } => {
                if *shift {
                    this.prev_match(cx);
                } else {
                    this.next_match(cx);
                }
                cx.notify();
            }
            _ => {}
        });
        subscriptions.push(sub_search);

        let sub_tv = cx.observe(&text_view_state, |this, tv, cx| {
            if this.is_search_open && this.mode == ScratchpadMode::Preview {
                let rendered = tv.read(cx).rendered_text();
                if this.last_searched_text.as_ref() != Some(&rendered) {
                    this.update_search(cx);
                }
            }
        });
        subscriptions.push(sub_tv);

        Self {
            id,
            file_path,
            title,
            cached_content,
            debounce_task: None,
            is_dirty: false,
            focus_handle,
            editor,
            text_view_state,
            search_input,
            is_search_open: false,
            search_matches: Vec::new(),
            current_match_index: 0,
            last_searched_text: None,
            mode: ScratchpadMode::Split,
            _subscriptions: subscriptions,
        }
    }

    #[allow(dead_code)]
    pub fn new_with_content(id: usize, content: String, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let file_path = crate::service::ScratchService::scratch_file_path(id).unwrap_or_else(|| PathBuf::from(format!("scratch_{id}.md")));
        Self::new_with_file(id, file_path, content, window, cx)
    }

    #[allow(dead_code)]
    pub fn id(&self) -> usize {
        self.id
    }

    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    pub fn title_str(&self) -> &str {
        &self.title
    }

    #[allow(dead_code)]
    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    pub fn save_sync(&mut self) {
        if !self.is_dirty {
            return;
        }
        if let Err(e) = crate::service::ScratchService::save_scratch_atomic(&self.file_path, &self.cached_content) {
            eprintln!("Failed to save scratchpad to {}: {}", self.file_path.display(), e);
        } else {
            self.is_dirty = false;
        }
    }

    pub fn mark_deleted(&mut self) {
        self.is_dirty = false;
        self.debounce_task = None;
    }

    #[allow(dead_code)]
    pub fn mode(&self) -> ScratchpadMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: ScratchpadMode, cx: &mut Context<Self>) {
        if self.mode != mode {
            self.mode = mode;
            if self.is_search_open {
                self.update_search(cx);
            }
            cx.notify();
        }
    }

    /// Inserts text at the editor's current cursor position or replaces active selection,
    /// syncing cached content, preview, and triggering debounced auto-save.
    pub fn insert_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == ScratchpadMode::Preview {
            self.mode = ScratchpadMode::Split;
        }

        self.editor.update(cx, |ed, cx| {
            ed.insert(text, window, cx);
            ed.focus_handle(cx).focus(window, cx);
        });
        let content = self.editor.read(cx).value().to_string();
        self.cached_content = content.clone();
        self.title = crate::service::ScratchService::extract_title(&content).unwrap_or_else(|| format!("Scratchpad {}", self.id));
        self.is_dirty = true;
        self.text_view_state.update(cx, |tv, cx| {
            tv.set_text(&content, cx);
        });
        if self.is_search_open {
            self.update_search(cx);
        }

        // Debounced auto-save (500ms)
        self.debounce_task = None;
        let task = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(std::time::Duration::from_millis(500)).await;
            if let Some(this) = this.upgrade() {
                this.update(cx, |this, cx| {
                    this.save_sync();
                    cx.notify();
                });
            }
        });
        self.debounce_task = Some(task);
        cx.notify();
    }

    pub fn content(&self, cx: &App) -> String {
        self.editor.read(cx).value().to_string()
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.mode == ScratchpadMode::Preview {
            self.focus_handle.focus(window, cx);
        } else {
            self.editor.read(cx).focus_handle(cx).focus(window, cx);
        }
    }

    pub fn toggle_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.is_search_open = !self.is_search_open;
        if self.is_search_open {
            self.search_input.update(cx, |input, cx| {
                input.focus(window, cx);
            });
            self.update_search(cx);
        } else {
            self.close_search(cx);
        }
        cx.notify();
    }

    pub fn close_search(&mut self, cx: &mut Context<Self>) {
        if self.is_search_open {
            self.is_search_open = false;
            self.search_matches.clear();
            self.current_match_index = 0;
            self.editor.update(cx, |ed, cx| {
                ed.close_search(cx);
                let cursor = ed.cursor();
                ed.set_selected_range(cursor..cursor, cx);
            });
            self.text_view_state.update(cx, |tv, cx| {
                tv.clear_range_highlights(cx);
            });
            cx.notify();
        }
    }

    pub fn next_match(&mut self, cx: &mut Context<Self>) {
        if self.mode != ScratchpadMode::Preview {
            self.editor.update(cx, |ed, cx| {
                if let Some(range) = ed.next_search_match(cx) {
                    ed.set_selected_range(range, cx);
                }
            });

            if self.mode == ScratchpadMode::Split {
                self.sync_preview_current_match(cx);
            }
        } else {
            if self.search_matches.is_empty() {
                return;
            }
            self.current_match_index = (self.current_match_index + 1) % self.search_matches.len();
            self.apply_preview_highlight_ranges(cx);
        }
        cx.notify();
    }

    pub fn prev_match(&mut self, cx: &mut Context<Self>) {
        if self.mode != ScratchpadMode::Preview {
            self.editor.update(cx, |ed, cx| {
                if let Some(range) = ed.previous_search_match(cx) {
                    ed.set_selected_range(range, cx);
                }
            });

            if self.mode == ScratchpadMode::Split {
                self.sync_preview_current_match(cx);
            }
        } else {
            if self.search_matches.is_empty() {
                return;
            }
            self.current_match_index = if self.current_match_index == 0 {
                self.search_matches.len() - 1
            } else {
                self.current_match_index - 1
            };
            self.apply_preview_highlight_ranges(cx);
        }
        cx.notify();
    }

    fn update_search(&mut self, cx: &mut Context<Self>) {
        let query = self.search_input.read(cx).value().trim().to_string();
        if query.is_empty() {
            self.search_matches.clear();
            self.current_match_index = 0;
            self.editor.update(cx, |ed, cx| {
                ed.close_search(cx);
            });
            self.text_view_state.update(cx, |tv, cx| {
                tv.clear_range_highlights(cx);
            });
            return;
        }

        if self.mode != ScratchpadMode::Preview {
            // Edit side is displayed: search on Editor
            self.editor.update(cx, |ed, cx| {
                ed.set_search_query(&query, true, cx);
                if let Some(first_range) = ed.next_search_match(cx) {
                    ed.set_selected_range(first_range, cx);
                }
            });

            // In Split mode, also update Preview highlights to keep both in sync
            if self.mode == ScratchpadMode::Split {
                self.update_preview_highlights(&query, cx);
            } else {
                self.text_view_state.update(cx, |tv, cx| {
                    tv.clear_range_highlights(cx);
                });
            }
        } else {
            // Preview mode: search on TextView
            self.update_preview_highlights(&query, cx);
        }
    }

    fn sync_preview_current_match(&mut self, cx: &mut Context<Self>) {
        let (current_idx, matched_ranges) = self.editor.read_with(cx, |ed, _| {
            let session = ed.search_session();
            (session.matcher.current_match_index(), session.matcher.matched_ranges())
        });
        if let Some(source_range) = matched_ranges.get(current_idx) {
            let rendered = self.text_view_state.read(cx).rendered_text();
            if let Some(preview_range) = rendered.range_for_source(source_range.clone()) {
                self.current_match_index = self.search_matches.iter().position(|r| r.start == preview_range.start).unwrap_or(0);
                self.apply_preview_highlight_ranges(cx);
            }
        }
    }

    fn update_preview_highlights(&mut self, query: &str, cx: &mut Context<Self>) {
        let rendered = self.text_view_state.read(cx).rendered_text();
        self.last_searched_text = Some(rendered.clone());
        self.search_matches = find_case_insensitive_matches(rendered.as_str(), query);

        if self.search_matches.is_empty() {
            self.current_match_index = 0;
            self.text_view_state.update(cx, |tv, cx| {
                tv.clear_range_highlights(cx);
            });
            return;
        }

        if self.current_match_index >= self.search_matches.len() {
            self.current_match_index = 0;
        }

        self.apply_preview_highlight_ranges(cx);
    }

    fn apply_preview_highlight_ranges(&mut self, cx: &mut Context<Self>) {
        if self.search_matches.is_empty() {
            self.text_view_state.update(cx, |tv, cx| {
                tv.clear_range_highlights(cx);
            });
            return;
        }

        let theme = cx.theme();
        let match_bg = theme.accent.opacity(0.35);
        let active_bg = theme.selection.opacity(0.85);

        let highlights: Vec<RangeHighlight> = self
            .search_matches
            .iter()
            .enumerate()
            .map(|(idx, range)| {
                let bg = if idx == self.current_match_index { active_bg } else { match_bg };
                RangeHighlight::new(range.clone(), bg)
            })
            .collect();

        let _ = self.text_view_state.update(cx, |tv, cx| {
            let res = tv.set_range_highlights(highlights, cx);
            if res.is_ok()
                && let Some(target_range) = self.search_matches.get(self.current_match_index)
            {
                let _ = tv.reveal_range(target_range.clone(), cx);
            }
            res
        });
    }

    fn render_text_view(&self) -> TextView {
        TextView::new(&self.text_view_state)
            .size_full()
            .selectable(true)
            .selection_format(SelectionFormat::Source)
            .scrollable(true)
            .markdown_extensions(MarkdownExtensions::default().frontmatter())
            .plugin(FrontmatterPlugin::new())
            .code_block_actions(|code_block, _window, _cx| {
                let code = code_block.code().to_string();
                Button::new("copy-code")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Copy)
                    .tooltip("Copy code")
                    .on_click(move |_event, _window, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
                    })
            })
            .table_actions(|table, _window, _cx| {
                let md = table.markdown.clone();
                div()
                    .flex()
                    .justify_end()
                    .pt_1()
                    .child(
                        Button::new("copy-table")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Copy)
                            .label("Copy Table")
                            .on_click(move |_event, _window, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(md.clone()));
                            }),
                    )
            })
            .on_link_click(|url, _event, window, cx| {
                if crate::core::offset_link::parse_offset_link(url.as_ref()).is_some() {
                    window.dispatch_action(Box::new(crate::actions::NavigateToOffsetLink { url: url.to_string() }), cx);
                } else {
                    cx.open_url(url.as_ref());
                }
            })
    }
}

impl Render for ScratchpadView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme();
        let mode = self.mode;

        let filename_badge = self
            .file_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|name| format!("({name})"))
            .unwrap_or_default();

        let toolbar = div()
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .px_3()
            .py_1p5()
            .border_b_1()
            .border_color(theme.border)
            .bg(theme.muted)
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .child(Icon::new(IconName::FileText).size_4().text_color(theme.muted_foreground))
                    .child(div().text_sm().font_semibold().text_color(theme.foreground).child(self.title.clone()))
                    .child(div().text_xs().text_color(theme.muted_foreground).child(filename_badge)),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("insert-offset-link")
                            .icon(IconName::ExternalLink)
                            .label("Link Offset")
                            .tooltip("Insert offset link from active binary editor")
                            .ghost()
                            .on_click(cx.listener(|_this, _, window, cx| {
                                window.dispatch_action(Box::new(crate::actions::InsertActiveOffsetLink), cx);
                            })),
                    )
                    .child(
                        Button::new("toggle-search")
                            .icon(IconName::Search)
                            .tooltip("Search in note")
                            .when(self.is_search_open, |btn| btn.primary())
                            .when(!self.is_search_open, |btn| btn.ghost())
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.toggle_search(window, cx);
                            })),
                    )
                    .child(
                        Button::new("mode-split")
                            .icon(IconName::Split)
                            .label("Split")
                            .when(mode == ScratchpadMode::Split, |btn| btn.primary())
                            .when(mode != ScratchpadMode::Split, |btn| btn.ghost())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_mode(ScratchpadMode::Split, cx);
                            })),
                    )
                    .child(
                        Button::new("mode-edit")
                            .icon(IconName::PenLine)
                            .label("Edit")
                            .when(mode == ScratchpadMode::Edit, |btn| btn.primary())
                            .when(mode != ScratchpadMode::Edit, |btn| btn.ghost())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_mode(ScratchpadMode::Edit, cx);
                            })),
                    )
                    .child(
                        Button::new("mode-preview")
                            .icon(IconName::Eye)
                            .label("Preview")
                            .when(mode == ScratchpadMode::Preview, |btn| btn.primary())
                            .when(mode != ScratchpadMode::Preview, |btn| btn.ghost())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.set_mode(ScratchpadMode::Preview, cx);
                            })),
                    ),
            );

        let search_bar = if self.is_search_open {
            let query = self.search_input.read(cx).value();
            let count_text = if query.trim().is_empty() {
                String::new()
            } else if self.mode != ScratchpadMode::Preview {
                self.editor.read(cx).search_session().matcher.label()
            } else if self.search_matches.is_empty() {
                "0/0".to_string()
            } else {
                format!("{}/{}", self.current_match_index + 1, self.search_matches.len())
            };

            Some(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap_2()
                    .px_3()
                    .py_1p5()
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(theme.muted)
                    .key_context(SEARCH_CONTEXT)
                    .child(
                        div()
                            .w(px(240.0))
                            .child(Input::new(&self.search_input).prefix(Icon::new(IconName::Search).size_3p5()).cleanable(true)),
                    )
                    .when(!count_text.is_empty(), |el| {
                        el.child(div().text_xs().text_color(theme.muted_foreground).child(count_text))
                    })
                    .child(
                        Button::new("search-prev")
                            .ghost()
                            .xsmall()
                            .icon(IconName::ChevronUp)
                            .tooltip("Previous match (Shift+Enter)")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.prev_match(cx);
                            })),
                    )
                    .child(
                        Button::new("search-next")
                            .ghost()
                            .xsmall()
                            .icon(IconName::ChevronDown)
                            .tooltip("Next match (Enter)")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.next_match(cx);
                            })),
                    )
                    .child(
                        Button::new("search-close")
                            .ghost()
                            .xsmall()
                            .icon(IconName::Close)
                            .tooltip("Close search (Esc)")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.close_search(cx);
                            })),
                    ),
            )
        } else {
            None
        };

        let content_view: AnyElement = match mode {
            ScratchpadMode::Split => div()
                .flex()
                .flex_row()
                .flex_1()
                .size_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(
                    div()
                        .flex_1()
                        .h_full()
                        .min_w_0()
                        .overflow_hidden()
                        .border_r_1()
                        .border_color(theme.border)
                        .child(Editor::new(&self.editor).size_full().bordered(false)),
                )
                .child(
                    div()
                        .id("scratchpad_split_preview")
                        .flex_1()
                        .h_full()
                        .min_w_0()
                        .overflow_hidden()
                        .p_4()
                        .child(self.render_text_view()),
                )
                .into_any_element(),
            ScratchpadMode::Edit => div()
                .flex_1()
                .size_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .child(Editor::new(&self.editor).size_full().bordered(false))
                .into_any_element(),
            ScratchpadMode::Preview => div()
                .id("scratchpad_preview")
                .flex_1()
                .size_full()
                .min_w_0()
                .min_h_0()
                .overflow_hidden()
                .p_4()
                .child(self.render_text_view())
                .into_any_element(),
        };

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(theme.background)
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &crate::actions::ToggleSearch, window, cx| {
                this.toggle_search(window, cx);
            }))
            .on_action(cx.listener(|this, _: &crate::actions::SearchNext, _, cx| {
                if this.is_search_open {
                    this.next_match(cx);
                }
            }))
            .on_action(cx.listener(|this, _: &crate::actions::SearchPrev, _, cx| {
                if this.is_search_open {
                    this.prev_match(cx);
                }
            }))
            .child(toolbar)
            .children(search_bar)
            .child(content_view)
    }
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-f", crate::actions::ToggleSearch, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-f", crate::actions::ToggleSearch, Some(CONTEXT)),
        KeyBinding::new("escape", crate::actions::ToggleSearch, Some(SEARCH_CONTEXT)),
    ]);
}

impl EventEmitter<PanelEvent> for ScratchpadView {}

impl Focusable for ScratchpadView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Drop for ScratchpadView {
    fn drop(&mut self) {
        self.save_sync();
    }
}

impl Panel for ScratchpadView {
    fn title(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.title.clone()
    }

    fn tab_name(&self, _: &App) -> Option<SharedString> {
        Some(self.title.clone().into())
    }

    fn zoom_control(&self, _: &App) -> Option<PanelControl> {
        None
    }
}

impl gpui_kit::base::dock::Panel for ScratchpadView {
    fn panel_name(&self) -> &'static str {
        "ScratchpadView"
    }

    fn closable(&self, _: &App) -> bool {
        true
    }

    fn zoomable(&self, _: &App) -> bool {
        false
    }

    fn visible(&self, _: &App) -> bool {
        true
    }

    fn set_active(&mut self, active: bool, window: &mut Window, cx: &mut Context<Self>) {
        if active {
            self.focus(window, cx);
        }
    }

    fn set_zoomed(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) {}
}

impl crate::ui::pane::WorkspaceTab for Entity<ScratchpadView> {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn title(&self, cx: &App) -> String {
        self.read(cx).title.clone()
    }

    fn is_dirty(&self, cx: &App) -> bool {
        self.read(cx).is_dirty
    }

    fn path(&self, cx: &App) -> Option<PathBuf> {
        Some(self.read(cx).file_path.clone())
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.read(cx).focus_handle.clone()
    }

    fn render(&self) -> AnyElement {
        self.clone().into_any_element()
    }

    fn create_split(&self, window: &mut Window, cx: &mut App) -> Option<crate::ui::pane::TabContent> {
        let (id, file_path, content) = {
            let view = self.read(cx);
            (view.id, view.file_path.clone(), view.content(cx))
        };
        let new_view = cx.new(|cx| ScratchpadView::new_with_file(id, file_path, content, window, cx));
        Some(crate::ui::pane::TabContent::new(new_view))
    }
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_SCRATCHPAD_CONTENT, ScratchpadMode, find_case_insensitive_matches};

    #[test]
    fn test_default_scratchpad_content() {
        assert!(DEFAULT_SCRATCHPAD_CONTENT.contains("# Scratchpad"));
        assert!(DEFAULT_SCRATCHPAD_CONTENT.contains("## Notes"));
    }

    #[test]
    fn test_markdown_syntax_highlighter() {
        use gpui_kit::component::highlighter::SyntaxHighlighter;
        use gpui_kit::component::input::Rope;
        let mut highlighter = SyntaxHighlighter::new("markdown");
        let rope = Rope::from("# Scratchpad\n\n## Notes\n- item\n");
        assert!(highlighter.update(None, &rope, None));
        assert!(highlighter.tree().is_some());
        let embedded = crate::theme::EmbeddedThemes::load_from_assets(&crate::assets::Assets);
        let default_dark = embedded.get("Default Dark").expect("Default Dark theme should be present");
        let theme = gpui_kit::component::highlighter::HighlightTheme {
            name: default_dark.name.to_string(),
            appearance: default_dark.mode,
            style: default_dark.highlight.clone().expect("Highlight style must exist"),
        };
        let styles = highlighter.styles(&(0..rope.len()), &theme);
        // Verify that heading text "Scratchpad" receives the heading title style
        let scratchpad_range = "# Scratchpad".find("Scratchpad").unwrap();
        assert!(
            styles
                .iter()
                .any(|(range, style)| { range.contains(&scratchpad_range) && style.color.is_some() }),
            "Heading text should have a highlight style color"
        );

        // Verify that "#" marker receives a punctuation style
        let hash_range = 0;
        assert!(
            styles.iter().any(|(range, style)| { range.contains(&hash_range) && style.color.is_some() }),
            "Heading marker '#' should have a highlight style color"
        );
    }

    #[test]
    fn test_scratchpad_mode_default_and_variants() {
        assert_eq!(ScratchpadMode::default(), ScratchpadMode::Split);
        assert_ne!(ScratchpadMode::Split, ScratchpadMode::Edit);
        assert_ne!(ScratchpadMode::Split, ScratchpadMode::Preview);
        assert_ne!(ScratchpadMode::Edit, ScratchpadMode::Preview);
    }

    #[test]
    fn test_find_case_insensitive_matches_ascii() {
        let text = "Hello world, HELLO universe, hello again";
        let matches = find_case_insensitive_matches(text, "hello");
        assert_eq!(matches.len(), 3);
        assert_eq!(&text[matches[0].clone()], "Hello");
        assert_eq!(&text[matches[1].clone()], "HELLO");
        assert_eq!(&text[matches[2].clone()], "hello");
    }

    #[test]
    fn test_find_case_insensitive_matches_empty_and_not_found() {
        assert!(find_case_insensitive_matches("", "hello").is_empty());
        assert!(find_case_insensitive_matches("Hello", "").is_empty());
        assert!(find_case_insensitive_matches("Hello", "   ").is_empty());
        assert!(find_case_insensitive_matches("Hello", "world").is_empty());
    }

    #[test]
    fn test_find_case_insensitive_matches_multibyte() {
        let text = "Markdownのメモ。大事なメモを残す。";
        let matches = find_case_insensitive_matches(text, "メモ");
        assert_eq!(matches.len(), 2);
        assert_eq!(&text[matches[0].clone()], "メモ");
        assert_eq!(&text[matches[1].clone()], "メモ");
    }

    #[test]
    fn test_find_case_insensitive_matches_non_overlapping() {
        let text = "aaaa";
        let matches = find_case_insensitive_matches(text, "aa");
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0], 0..2);
        assert_eq!(matches[1], 2..4);
    }

    #[test]
    fn test_scratchpad_title_fallback_and_extraction() {
        use crate::service::ScratchService;
        let content_with_title = "# Project Architecture\n\nNotes here";
        assert_eq!(ScratchService::extract_title(content_with_title), Some("Project Architecture".to_string()));

        let content_without_title = "Just plain text notes";
        assert_eq!(ScratchService::extract_title(content_without_title), None);
    }

    #[test]
    fn test_scratchpad_offset_link_integration() {
        use crate::core::offset_link::{OffsetLinkTarget, format_offset_markdown, parse_offset_link};

        // 1. Generate link markdown from an editor offset
        let md = format_offset_markdown(0x200, Some(32), 0x1000);
        assert_eq!(md, "[0x0200..0x021F (32 B)](xvw://goto/0x200?len=32)");

        // 2. Extract link URL target and verify it parses back to the expected range
        let url = "xvw://goto/0x200?len=32";
        let target = parse_offset_link(url).expect("link must parse");
        assert_eq!(target, OffsetLinkTarget::Range(0x200..0x220));
    }
}

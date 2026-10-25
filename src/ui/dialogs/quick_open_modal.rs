use std::path::{Path, PathBuf};
use std::time::SystemTime;

use autocorrect::ignorer::Ignorer;
use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable, Size, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::actions::{PaletteCancel, PaletteConfirm, PaletteSelectNext, PaletteSelectPrev};
use crate::core::format::FileFormat;
use crate::core::fuzzy_matcher::FuzzyMatcher;
use crate::service::scratch_service::ScratchEntry;
use crate::ui::icon::IconName;

const CONTEXT: &str = "QuickOpenModal";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", PaletteSelectNext, Some(CONTEXT)),
        KeyBinding::new("up", PaletteSelectPrev, Some(CONTEXT)),
        KeyBinding::new("enter", PaletteConfirm, Some(CONTEXT)),
        KeyBinding::new("escape", PaletteCancel, Some(CONTEXT)),
    ]);
}

const IGNORED_DIR_NAMES: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    "node_modules",
    "target",
    "vendor",
    "venv",
    ".venv",
    "env",
    ".env",
    "__pycache__",
    ".cache",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".turbo",
    ".cargo",
    ".rustup",
    "Library",
    ".Trash",
];

/// Maximum directory depth for workspace file collection.
const MAX_DIRECTORY_DEPTH: usize = 6;

/// Walks the workspace directory recursively up to `max_files`, ignoring VCS, common bulky directories,
/// and gitignored files. Returns a vector of `(absolute_path, relative_path, file_name)`.
pub fn collect_workspace_files(root: &Path, max_files: usize) -> Vec<(PathBuf, String, String)> {
    let mut results = Vec::new();
    let ignorer = Ignorer::new(&root.to_string_lossy());
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut subdirs = Vec::new();
        for entry in entries.flatten() {
            let file_type = match entry.file_type() {
                Ok(ft) => ft,
                Err(_) => continue,
            };
            let is_dir = file_type.is_dir();
            let is_file = file_type.is_file();
            if !is_dir && !is_file {
                continue;
            }

            let file_name_os = entry.file_name();
            let file_name_str = match file_name_os.to_str() {
                Some(s) => s,
                None => continue,
            };

            if is_dir {
                if IGNORED_DIR_NAMES.contains(&file_name_str) {
                    continue;
                }
            } else if is_file
                && (file_name_str == ".DS_Store"
                    || file_name_str.ends_with(".crdownload")
                    || file_name_str.ends_with(".part")
                    || file_name_str.ends_with(".download"))
            {
                continue;
            }

            let path = entry.path();
            let relative_path = path.strip_prefix(root).unwrap_or(&path);
            let rel_str = relative_path.to_string_lossy();

            if ignorer.is_ignored(&rel_str) {
                continue;
            }

            if is_dir {
                if depth < MAX_DIRECTORY_DEPTH {
                    subdirs.push((path, depth + 1));
                }
            } else {
                let relative_path_str = rel_str.into_owned();
                results.push((path, relative_path_str, file_name_str.to_string()));
                if results.len() >= max_files {
                    return results;
                }
            }
        }
        for sub in subdirs.into_iter().rev() {
            stack.push(sub);
        }
    }

    results
}

/// Input candidate for Quick Open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickOpenCandidate {
    /// A file from recent history (opened or imported)
    RecentFile {
        path: PathBuf,
        file_name: String,
        detail: String,
        format: Option<FileFormat>,
    },
    /// A file in the currently opened workspace folder in FILES
    WorkspaceFile { path: PathBuf, file_name: String, relative_path: String },
    /// A scratchpad note
    Note(ScratchEntry),
}

/// Filtered and scored item ready for display.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickOpenItem {
    RecentFile {
        path: PathBuf,
        file_name: String,
        detail: String,
        format: Option<FileFormat>,
        score: i32,
        name_matches: Vec<usize>,
        detail_matches: Vec<usize>,
    },
    WorkspaceFile {
        path: PathBuf,
        file_name: String,
        relative_path: String,
        score: i32,
        name_matches: Vec<usize>,
        path_matches: Vec<usize>,
    },
    Note {
        entry: ScratchEntry,
        score: i32,
        title_matches: Vec<usize>,
        filename_matches: Vec<usize>,
    },
    NewNote {
        query: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickOpenEvent {
    OpenFile { path: PathBuf, format: Option<FileFormat> },
    OpenNote(PathBuf),
    NewNote(Option<String>),
    Cancel,
}

/// Maximum number of candidate items to display in the modal at once to prevent layout stalls.
pub const MAX_FILTERED_RESULTS: usize = 100;

/// Pure helper that fuzzy matches candidates (recent files, workspace folder files, and scratch notes)
/// using funap/fuzzy-matcher, ranks candidates by match score descending, and appends a "Create new note" entry.
pub fn filter_items(candidates: &[QuickOpenCandidate], query: &str) -> Vec<QuickOpenItem> {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        let count = candidates.len().min(MAX_FILTERED_RESULTS);
        let mut items = Vec::with_capacity(count + 1);
        for candidate in candidates.iter().take(MAX_FILTERED_RESULTS) {
            match candidate {
                QuickOpenCandidate::RecentFile {
                    path,
                    file_name,
                    detail,
                    format,
                } => {
                    items.push(QuickOpenItem::RecentFile {
                        path: path.clone(),
                        file_name: file_name.clone(),
                        detail: detail.clone(),
                        format: *format,
                        score: 0,
                        name_matches: Vec::new(),
                        detail_matches: Vec::new(),
                    });
                }
                QuickOpenCandidate::WorkspaceFile {
                    path,
                    file_name,
                    relative_path,
                } => {
                    items.push(QuickOpenItem::WorkspaceFile {
                        path: path.clone(),
                        file_name: file_name.clone(),
                        relative_path: relative_path.clone(),
                        score: 0,
                        name_matches: Vec::new(),
                        path_matches: Vec::new(),
                    });
                }
                QuickOpenCandidate::Note(entry) => {
                    items.push(QuickOpenItem::Note {
                        entry: entry.clone(),
                        score: 0,
                        title_matches: Vec::new(),
                        filename_matches: Vec::new(),
                    });
                }
            }
        }
        items.push(QuickOpenItem::NewNote { query: String::new() });
        return items;
    }

    let mut matcher = FuzzyMatcher::new(trimmed);
    let mut scored_items: Vec<(i32, QuickOpenItem)> = Vec::with_capacity(candidates.len().min(MAX_FILTERED_RESULTS * 4));

    for candidate in candidates {
        match candidate {
            QuickOpenCandidate::RecentFile {
                path,
                file_name,
                detail,
                format,
            } => {
                let name_res = matcher.score_match_with_positions(file_name);
                let detail_res = matcher.score_match_with_positions(detail);

                if name_res.is_some() || detail_res.is_some() {
                    let (score, name_matches, detail_matches) = match (name_res, detail_res) {
                        (Some((s_n, p_n)), Some((s_d, p_d))) => (s_n.max(s_d) + 5, p_n, p_d),
                        (Some((s_n, p_n)), None) => (s_n, p_n, Vec::new()),
                        (None, Some((s_d, p_d))) => (s_d, Vec::new(), p_d),
                        (None, None) => unreachable!(),
                    };
                    let score = score + 2; // bonus for recent files
                    scored_items.push((
                        score,
                        QuickOpenItem::RecentFile {
                            path: path.clone(),
                            file_name: file_name.clone(),
                            detail: detail.clone(),
                            format: *format,
                            score,
                            name_matches,
                            detail_matches,
                        },
                    ));
                }
            }
            QuickOpenCandidate::WorkspaceFile {
                path,
                file_name,
                relative_path,
            } => {
                let name_res = matcher.score_match_with_positions(file_name);
                let path_res = matcher.score_match_with_positions(relative_path);

                if name_res.is_some() || path_res.is_some() {
                    let (score, name_matches, path_matches) = match (name_res, path_res) {
                        (Some((s_n, p_n)), Some((s_p, p_p))) => (s_n.max(s_p) + 5, p_n, p_p),
                        (Some((s_n, p_n)), None) => (s_n, p_n, Vec::new()),
                        (None, Some((s_p, p_p))) => (s_p, Vec::new(), p_p),
                        (None, None) => unreachable!(),
                    };
                    scored_items.push((
                        score,
                        QuickOpenItem::WorkspaceFile {
                            path: path.clone(),
                            file_name: file_name.clone(),
                            relative_path: relative_path.clone(),
                            score,
                            name_matches,
                            path_matches,
                        },
                    ));
                }
            }
            QuickOpenCandidate::Note(entry) => {
                let title_res = matcher.score_match_with_positions(&entry.title);
                let filename_res = matcher.score_match_with_positions(&entry.filename);

                if title_res.is_some() || filename_res.is_some() {
                    let (score, title_matches, filename_matches) = match (title_res, filename_res) {
                        (Some((s_t, p_t)), Some((s_f, p_f))) => (s_t.max(s_f) + 5, p_t, p_f),
                        (Some((s_t, p_t)), None) => (s_t, p_t, Vec::new()),
                        (None, Some((s_f, p_f))) => (s_f, Vec::new(), p_f),
                        (None, None) => unreachable!(),
                    };
                    scored_items.push((
                        score,
                        QuickOpenItem::Note {
                            entry: entry.clone(),
                            score,
                            title_matches,
                            filename_matches,
                        },
                    ));
                }
            }
        }
    }

    scored_items.sort_by_key(|(score, _)| std::cmp::Reverse(*score));

    let count = scored_items.len().min(MAX_FILTERED_RESULTS);
    let mut items = Vec::with_capacity(count + 1);
    for (_, item) in scored_items.into_iter().take(MAX_FILTERED_RESULTS) {
        items.push(item);
    }
    items.push(QuickOpenItem::NewNote { query: trimmed.to_string() });

    items
}

/// Convenience helper for filtering scratches directly (used in tests).
#[cfg(test)]
pub fn filter_items_from_scratches(entries: &[ScratchEntry], query: &str) -> Vec<QuickOpenItem> {
    let candidates: Vec<QuickOpenCandidate> = entries.iter().cloned().map(QuickOpenCandidate::Note).collect();
    filter_items(&candidates, query)
}

/// Formats a `SystemTime` as a human-readable relative time string.
pub fn format_relative_time(time: SystemTime) -> String {
    if let Ok(elapsed) = time.elapsed() {
        let secs = elapsed.as_secs();
        if secs < 60 {
            "just now".to_string()
        } else if secs < 3600 {
            let mins = secs / 60;
            format!("{mins}m ago")
        } else if secs < 86400 {
            let hours = secs / 3600;
            format!("{hours}h ago")
        } else {
            let days = secs / 86400;
            format!("{days}d ago")
        }
    } else {
        "recently".to_string()
    }
}
pub use super::common::{palette_list_container, palette_modal_container, render_highlighted_text, render_palette_empty_message};

pub struct QuickOpenModal {
    focus_handle: FocusHandle,
    query_input: Entity<InputState>,
    scroll_handle: ScrollHandle,
    all_candidates: Vec<QuickOpenCandidate>,
    filtered_items: Vec<QuickOpenItem>,
    selected_index: usize,
    is_loading: bool,
}

impl EventEmitter<QuickOpenEvent> for QuickOpenModal {}

impl QuickOpenModal {
    pub fn new(candidates: Vec<QuickOpenCandidate>, is_loading: bool, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let query_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search files, notes, or create new..."));

        let filtered_items = filter_items(&candidates, "");

        cx.subscribe(&query_input, |this, input, event: &input::InputEvent, cx| match event {
            input::InputEvent::Change => {
                let val = input.read(cx).value().to_string();
                this.update_query(&val, cx);
            }
            input::InputEvent::PressEnter { .. } => {
                this.confirm(cx);
            }
            _ => {}
        })
        .detach();

        Self {
            focus_handle,
            query_input,
            scroll_handle: ScrollHandle::new(),
            all_candidates: candidates,
            filtered_items,
            selected_index: 0,
            is_loading,
        }
    }

    /// Appends asynchronously discovered candidates, updating the filtered list seamlessly.
    pub fn append_candidates(&mut self, new_candidates: Vec<QuickOpenCandidate>, cx: &mut Context<Self>) {
        self.is_loading = false;
        if new_candidates.is_empty() {
            cx.notify();
            return;
        }
        self.all_candidates.extend(new_candidates);
        let query = self.query_input.read(cx).value().to_string();
        self.filtered_items = filter_items(&self.all_candidates, &query);
        if !self.filtered_items.is_empty() {
            self.selected_index = self.selected_index.min(self.filtered_items.len() - 1);
        } else {
            self.selected_index = 0;
        }
        cx.notify();
    }

    /// Updates the loading indicator state.
    #[allow(dead_code)]
    pub fn set_loading(&mut self, is_loading: bool, cx: &mut Context<Self>) {
        if self.is_loading != is_loading {
            self.is_loading = is_loading;
            cx.notify();
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.query_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
    }

    pub fn update_query(&mut self, query: &str, cx: &mut Context<Self>) {
        self.filtered_items = filter_items(&self.all_candidates, query);
        self.selected_index = 0;
        self.scroll_handle.scroll_to_item(0);
        cx.notify();
    }

    pub fn select_next(&mut self, cx: &mut Context<Self>) {
        if !self.filtered_items.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.filtered_items.len() - 1);
            self.scroll_handle.scroll_to_item(self.selected_index);
            cx.notify();
        }
    }

    pub fn select_prev(&mut self, cx: &mut Context<Self>) {
        if !self.filtered_items.is_empty() {
            self.selected_index = self.selected_index.saturating_sub(1);
            self.scroll_handle.scroll_to_item(self.selected_index);
            cx.notify();
        }
    }

    pub fn confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(item) = self.filtered_items.get(self.selected_index) {
            match item {
                QuickOpenItem::RecentFile { path, format, .. } => {
                    cx.emit(QuickOpenEvent::OpenFile {
                        path: path.clone(),
                        format: *format,
                    });
                }
                QuickOpenItem::WorkspaceFile { path, .. } => {
                    cx.emit(QuickOpenEvent::OpenFile {
                        path: path.clone(),
                        format: None,
                    });
                }
                QuickOpenItem::Note { entry, .. } => {
                    cx.emit(QuickOpenEvent::OpenNote(entry.path.clone()));
                }
                QuickOpenItem::NewNote { query } => {
                    let title = if query.trim().is_empty() { None } else { Some(query.trim().to_string()) };
                    cx.emit(QuickOpenEvent::NewNote(title));
                }
            }
        }
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(QuickOpenEvent::Cancel);
    }
}

impl Render for QuickOpenModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();

        let mut list_children = Vec::new();

        if self.filtered_items.is_empty() {
            list_children.push(render_palette_empty_message("No matching files or notes found", &theme));
        } else {
            for (idx, item) in self.filtered_items.iter().enumerate() {
                let is_selected = idx == self.selected_index;
                let element = match item {
                    QuickOpenItem::RecentFile {
                        file_name,
                        detail,
                        name_matches,
                        detail_matches,
                        ..
                    } => {
                        let name_elem = render_highlighted_text(file_name, name_matches, theme.foreground, theme.primary, true);
                        let detail_elem = if !detail.is_empty() {
                            let elem = if detail_matches.is_empty() {
                                div().text_xs().text_color(theme.muted_foreground).child(detail.clone()).into_any_element()
                            } else {
                                render_highlighted_text(detail, detail_matches, theme.muted_foreground, theme.primary, false)
                            };
                            Some(elem)
                        } else {
                            None
                        };

                        h_flex()
                            .id(SharedString::from(format!("palette-item-{idx}")))
                            .items_center()
                            .justify_between()
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .when(is_selected, |el| el.bg(theme.list_active))
                            .when(!is_selected, |el| el.hover(|s| s.bg(theme.list_hover)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_index = idx;
                                this.confirm(cx);
                            }))
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .overflow_hidden()
                                    .child(Icon::new(IconName::Binary).size(px(14.0)).text_color(theme.muted_foreground))
                                    .child(div().text_sm().child(name_elem))
                                    .when_some(detail_elem, |el, d| el.child(div().text_xs().text_color(theme.muted_foreground).child(d))),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(theme.secondary)
                                    .text_color(theme.muted_foreground)
                                    .child("Recent"),
                            )
                            .into_any_element()
                    }
                    QuickOpenItem::WorkspaceFile {
                        file_name,
                        relative_path,
                        name_matches,
                        path_matches,
                        ..
                    } => {
                        let name_elem = render_highlighted_text(file_name, name_matches, theme.foreground, theme.primary, true);
                        let path_elem = if relative_path != file_name {
                            let elem = if path_matches.is_empty() {
                                div()
                                    .text_xs()
                                    .text_color(theme.muted_foreground)
                                    .child(relative_path.clone())
                                    .into_any_element()
                            } else {
                                render_highlighted_text(relative_path, path_matches, theme.muted_foreground, theme.primary, false)
                            };
                            Some(elem)
                        } else {
                            None
                        };

                        h_flex()
                            .id(SharedString::from(format!("palette-item-{idx}")))
                            .items_center()
                            .justify_between()
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .when(is_selected, |el| el.bg(theme.list_active))
                            .when(!is_selected, |el| el.hover(|s| s.bg(theme.list_hover)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_index = idx;
                                this.confirm(cx);
                            }))
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .overflow_hidden()
                                    .child(Icon::new(IconName::File).size(px(14.0)).text_color(theme.muted_foreground))
                                    .child(div().text_sm().child(name_elem))
                                    .when_some(path_elem, |el, p| el.child(div().text_xs().text_color(theme.muted_foreground).child(p))),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(theme.secondary)
                                    .text_color(theme.muted_foreground)
                                    .child("File"),
                            )
                            .into_any_element()
                    }
                    QuickOpenItem::Note {
                        entry,
                        title_matches,
                        filename_matches,
                        ..
                    } => {
                        let rel_time = entry.modified.map(format_relative_time).unwrap_or_default();
                        let title_element = render_highlighted_text(&entry.title, title_matches, theme.foreground, theme.primary, true);

                        let filename_element = if filename_matches.is_empty() {
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("({})", entry.filename))
                                .into_any_element()
                        } else {
                            h_flex()
                                .items_center()
                                .text_xs()
                                .child(div().text_color(theme.muted_foreground).child("("))
                                .child(render_highlighted_text(
                                    &entry.filename,
                                    filename_matches,
                                    theme.muted_foreground,
                                    theme.primary,
                                    false,
                                ))
                                .child(div().text_color(theme.muted_foreground).child(")"))
                                .into_any_element()
                        };

                        let target_name_opt = entry
                            .target_file
                            .as_ref()
                            .and_then(|p| p.file_name())
                            .and_then(|n| n.to_str())
                            .map(|s| s.to_string());

                        h_flex()
                            .id(SharedString::from(format!("palette-item-{idx}")))
                            .items_center()
                            .justify_between()
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .when(is_selected, |el| el.bg(theme.list_active))
                            .when(!is_selected, |el| el.hover(|s| s.bg(theme.list_hover)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_index = idx;
                                this.confirm(cx);
                            }))
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .overflow_hidden()
                                    .child(Icon::new(IconName::FileText).size(px(14.0)).text_color(theme.muted_foreground))
                                    .child(div().text_sm().child(title_element))
                                    .child(filename_element)
                                    .when_some(target_name_opt, |el, target_name| {
                                        el.child(
                                            div()
                                                .px_1()
                                                .py_0p5()
                                                .rounded_md()
                                                .bg(theme.secondary)
                                                .text_xs()
                                                .text_color(theme.primary)
                                                .child(target_name),
                                        )
                                    }),
                            )
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .when(!rel_time.is_empty(), |el| {
                                        el.child(div().text_xs().text_color(theme.muted_foreground).child(rel_time))
                                    })
                                    .child(
                                        div()
                                            .text_xs()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .bg(theme.secondary)
                                            .text_color(theme.muted_foreground)
                                            .child("Note"),
                                    ),
                            )
                            .into_any_element()
                    }
                    QuickOpenItem::NewNote { query } => {
                        let label = if query.is_empty() {
                            "Create new scratchpad note".to_string()
                        } else {
                            format!("Create new scratchpad note \"{query}\"")
                        };

                        h_flex()
                            .id(SharedString::from(format!("palette-item-new-{idx}")))
                            .items_center()
                            .justify_between()
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .when(is_selected, |el| el.bg(theme.list_active))
                            .when(!is_selected, |el| el.hover(|s| s.bg(theme.list_hover)))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_index = idx;
                                this.confirm(cx);
                            }))
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(Icon::new(IconName::Plus).size(px(14.0)).text_color(theme.primary))
                                    .child(div().text_sm().font_weight(FontWeight::MEDIUM).text_color(theme.primary).child(label)),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .px_1p5()
                                    .py_0p5()
                                    .rounded_sm()
                                    .bg(theme.secondary)
                                    .text_color(theme.foreground)
                                    .child("New"),
                            )
                            .into_any_element()
                    }
                };
                list_children.push(element);
            }
        }

        palette_modal_container(&theme)
            .id("quick-open-modal")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_scroll_wheel(|_, _, cx| {
                cx.stop_propagation();
            })
            .on_action(cx.listener(|this, _: &PaletteSelectNext, _, cx| {
                this.select_next(cx);
            }))
            .on_action(cx.listener(|this, _: &PaletteSelectPrev, _, cx| {
                this.select_prev(cx);
            }))
            .on_action(cx.listener(|this, _: &PaletteConfirm, _, cx| {
                this.confirm(cx);
            }))
            .on_action(cx.listener(|this, _: &PaletteCancel, _, cx| {
                this.cancel(cx);
            }))
            .child(
                h_flex()
                    .items_center()
                    .p_3()
                    .border_b_1()
                    .border_color(theme.border)
                    .bg(theme.tab_bar)
                    .rounded_t_lg()
                    .child(
                        Input::new(&self.query_input)
                            .cleanable(true)
                            .prefix(Icon::new(IconName::Search).size(px(16.0)))
                            .with_size(Size::Small),
                    ),
            )
            .child(palette_list_container(&self.scroll_handle, "palette-items-scroll").children(list_children))
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1p5()
                    .border_t_1()
                    .border_color(theme.border)
                    .bg(theme.muted)
                    .rounded_b_lg()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(
                        h_flex()
                            .items_center()
                            .gap_3()
                            .child("↑↓ Navigate")
                            .child("↵ Select")
                            .child("Esc Dismiss")
                            .when(self.is_loading, |el| {
                                el.child(
                                    h_flex()
                                        .items_center()
                                        .gap_1()
                                        .text_color(theme.primary)
                                        .child(Icon::new(IconName::Loader).size(px(12.0)))
                                        .child("Scanning files..."),
                                )
                            }),
                    )
                    .child(format!("{} items", self.filtered_items.len())),
            )
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::{QuickOpenCandidate, QuickOpenItem, collect_workspace_files, filter_items, filter_items_from_scratches, format_relative_time};
    use crate::core::format::FileFormat;
    use crate::service::scratch_service::ScratchEntry;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    #[test]
    fn test_format_relative_time() {
        let now = SystemTime::now();
        assert_eq!(format_relative_time(now), "just now");

        let five_mins_ago = now - Duration::from_secs(300);
        assert_eq!(format_relative_time(five_mins_ago), "5m ago");

        let two_hours_ago = now - Duration::from_secs(7200);
        assert_eq!(format_relative_time(two_hours_ago), "2h ago");

        let two_days_ago = now - Duration::from_secs(172800);
        assert_eq!(format_relative_time(two_days_ago), "2d ago");
    }

    #[test]
    fn test_filter_items_empty_query() {
        let candidates = vec![
            QuickOpenCandidate::RecentFile {
                path: PathBuf::from("data.bin"),
                file_name: "data.bin".to_string(),
                detail: "testdata".to_string(),
                format: Some(FileFormat::Binary),
            },
            QuickOpenCandidate::WorkspaceFile {
                path: PathBuf::from("src/main.rs"),
                file_name: "main.rs".to_string(),
                relative_path: "src/main.rs".to_string(),
            },
            QuickOpenCandidate::Note(ScratchEntry {
                id: Some(1),
                filename: "scratch_1.md".to_string(),
                path: PathBuf::from("scratch_1.md"),
                title: "Rust Notes".to_string(),
                modified: None,
                target_file: None,
            }),
        ];

        let items = filter_items(&candidates, "");
        assert_eq!(items.len(), 4);
        match &items[0] {
            QuickOpenItem::RecentFile { file_name, .. } => assert_eq!(file_name, "data.bin"),
            _ => panic!("expected recent file"),
        }
        match &items[1] {
            QuickOpenItem::WorkspaceFile { file_name, .. } => assert_eq!(file_name, "main.rs"),
            _ => panic!("expected workspace file"),
        }
        match &items[2] {
            QuickOpenItem::Note { entry, .. } => assert_eq!(entry.title, "Rust Notes"),
            _ => panic!("expected note"),
        }
        assert_eq!(items[3], QuickOpenItem::NewNote { query: String::new() });
    }

    #[test]
    fn test_filter_items_fuzzy_matching_and_ranking() {
        let entries = vec![
            ScratchEntry {
                id: Some(1),
                filename: "scratch_1.md".to_string(),
                path: PathBuf::from("scratch_1.md"),
                title: "format_table".to_string(),
                modified: None,
                target_file: None,
            },
            ScratchEntry {
                id: Some(2),
                filename: "scratch_2.md".to_string(),
                path: PathBuf::from("scratch_2.md"),
                title: "file_management_tool".to_string(),
                modified: None,
                target_file: None,
            },
            ScratchEntry {
                id: Some(3),
                filename: "scratch_3.md".to_string(),
                path: PathBuf::from("scratch_3.md"),
                title: "FastMessageThread".to_string(),
                modified: None,
                target_file: None,
            },
        ];

        let items = filter_items_from_scratches(&entries, "fmt");
        assert_eq!(items.len(), 4);
        match &items[0] {
            QuickOpenItem::Note {
                entry, score, title_matches, ..
            } => {
                assert_eq!(entry.title, "file_management_tool");
                assert_eq!(*score, 22);
                assert!(!title_matches.is_empty());
            }
            _ => panic!("expected note"),
        }
        match &items[1] {
            QuickOpenItem::Note {
                entry, score, title_matches, ..
            } => {
                assert_eq!(entry.title, "FastMessageThread");
                assert_eq!(*score, 19);
                assert!(!title_matches.is_empty());
            }
            _ => panic!("expected note"),
        }
        match &items[2] {
            QuickOpenItem::Note {
                entry, score, title_matches, ..
            } => {
                assert_eq!(entry.title, "format_table");
                assert_eq!(*score, 18);
                assert!(!title_matches.is_empty());
            }
            _ => panic!("expected note"),
        }
        assert_eq!(items[3], QuickOpenItem::NewNote { query: "fmt".to_string() });
    }

    #[test]
    fn test_filter_items_matching_by_filename() {
        let candidates = vec![
            QuickOpenCandidate::WorkspaceFile {
                path: PathBuf::from("src/fuzzy_matcher.rs"),
                file_name: "fuzzy_matcher.rs".to_string(),
                relative_path: "src/fuzzy_matcher.rs".to_string(),
            },
            QuickOpenCandidate::RecentFile {
                path: PathBuf::from("testdata/sample.bin"),
                file_name: "sample.bin".to_string(),
                detail: "testdata".to_string(),
                format: None,
            },
        ];

        let items = filter_items(&candidates, "fuzzy");
        assert_eq!(items.len(), 2);
        match &items[0] {
            QuickOpenItem::WorkspaceFile { file_name, name_matches, .. } => {
                assert_eq!(file_name, "fuzzy_matcher.rs");
                assert!(!name_matches.is_empty());
            }
            _ => panic!("expected workspace file"),
        }
    }

    #[test]
    fn test_filter_items_matching_by_relative_path() {
        let candidates = vec![QuickOpenCandidate::WorkspaceFile {
            path: PathBuf::from("src/ui/dialogs/quick_open_modal.rs"),
            file_name: "quick_open_modal.rs".to_string(),
            relative_path: "src/ui/dialogs/quick_open_modal.rs".to_string(),
        }];

        let items = filter_items(&candidates, "dialogs");
        assert_eq!(items.len(), 2);
        match &items[0] {
            QuickOpenItem::WorkspaceFile {
                relative_path, path_matches, ..
            } => {
                assert_eq!(relative_path, "src/ui/dialogs/quick_open_modal.rs");
                assert!(!path_matches.is_empty());
            }
            _ => panic!("expected workspace file"),
        }
    }

    #[test]
    fn test_collect_workspace_files() {
        let temp_dir = std::env::temp_dir().join(format!("xvw_test_quick_open_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp_dir);
        std::fs::create_dir_all(temp_dir.join("sub")).unwrap();
        std::fs::create_dir_all(temp_dir.join(".git")).unwrap();

        std::fs::write(temp_dir.join("root.txt"), "hello").unwrap();
        std::fs::write(temp_dir.join("sub/child.rs"), "fn main() {}").unwrap();
        std::fs::write(temp_dir.join(".git/config"), "git config").unwrap();

        let files = collect_workspace_files(&temp_dir, 100);
        assert_eq!(files.len(), 2);
        let names: Vec<String> = files.into_iter().map(|(_, _, name)| name).collect();
        assert!(names.contains(&"root.txt".to_string()));
        assert!(names.contains(&"child.rs".to_string()));
        assert!(!names.contains(&"config".to_string()));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}

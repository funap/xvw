use std::path::PathBuf;
use std::time::SystemTime;

use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable, Size, h_flex, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::actions::{PaletteCancel, PaletteConfirm, PaletteSelectNext, PaletteSelectPrev};
use crate::core::fuzzy_matcher::FuzzyMatcher;
use crate::service::scratch_service::{ScratchEntry, ScratchService};
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchedNote {
    pub entry: ScratchEntry,
    pub score: i32,
    pub title_matches: Vec<usize>,
    pub filename_matches: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickOpenItem {
    Note(MatchedNote),
    NewNote { query: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickOpenEvent {
    OpenFile(PathBuf),
    NewNote(Option<String>),
    Cancel,
}

/// Pure helper that fuzzy matches scratches by title and filename using funap/fuzzy-matcher,
/// ranks candidates by match score descending, and appends a "Create new" entry.
pub fn filter_items(entries: &[ScratchEntry], query: &str) -> Vec<QuickOpenItem> {
    let trimmed = query.trim();

    let mut items = Vec::new();

    if trimmed.is_empty() {
        for entry in entries {
            items.push(QuickOpenItem::Note(MatchedNote {
                entry: entry.clone(),
                score: 0,
                title_matches: Vec::new(),
                filename_matches: Vec::new(),
            }));
        }
        items.push(QuickOpenItem::NewNote { query: String::new() });
        return items;
    }

    let mut matcher = FuzzyMatcher::new(trimmed);
    let mut scored_entries = Vec::new();

    for entry in entries {
        let title_res = matcher.score_match_with_positions(&entry.title);
        let filename_res = matcher.score_match_with_positions(&entry.filename);

        if title_res.is_some() || filename_res.is_some() {
            let (score, title_matches, filename_matches) = match (title_res, filename_res) {
                (Some((s_t, p_t)), Some((s_f, p_f))) => {
                    let score = s_t.max(s_f);
                    (score, p_t, p_f)
                }
                (Some((s_t, p_t)), None) => (s_t, p_t, Vec::new()),
                (None, Some((s_f, p_f))) => (s_f, Vec::new(), p_f),
                (None, None) => unreachable!(),
            };

            scored_entries.push(MatchedNote {
                entry: entry.clone(),
                score,
                title_matches,
                filename_matches,
            });
        }
    }

    // Sort matching entries by match score descending
    scored_entries.sort_by_key(|a| std::cmp::Reverse(a.score));

    for matched in scored_entries {
        items.push(QuickOpenItem::Note(matched));
    }
    items.push(QuickOpenItem::NewNote { query: trimmed.to_string() });

    items
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

/// Pure helper that renders text with fuzzy-matched characters highlighted in primary theme color.
pub fn render_highlighted_text(text: &str, matches: &[usize], text_color: Hsla, highlight_color: Hsla, is_medium_weight: bool) -> AnyElement {
    if matches.is_empty() {
        return div()
            .when(is_medium_weight, |el| el.font_weight(FontWeight::MEDIUM))
            .text_color(text_color)
            .child(text.to_string())
            .into_any_element();
    }

    let match_set: std::collections::HashSet<usize> = matches.iter().copied().collect();
    let mut spans: Vec<(String, bool)> = Vec::new();

    for (byte_offset, ch) in text.char_indices() {
        let is_matched = match_set.contains(&byte_offset);
        if let Some((last_str, last_matched)) = spans.last_mut()
            && *last_matched == is_matched
        {
            last_str.push(ch);
            continue;
        }
        spans.push((ch.to_string(), is_matched));
    }

    let children: Vec<AnyElement> = spans
        .into_iter()
        .map(|(chunk, is_matched)| {
            div()
                .when(!is_matched && is_medium_weight, |el| el.font_weight(FontWeight::MEDIUM))
                .when(is_matched, |el| el.font_weight(FontWeight::BOLD).text_color(highlight_color))
                .when(!is_matched, |el| el.text_color(text_color))
                .child(chunk)
                .into_any_element()
        })
        .collect();

    h_flex().items_center().children(children).into_any_element()
}

pub struct QuickOpenModal {
    focus_handle: FocusHandle,
    query_input: Entity<InputState>,
    all_entries: Vec<ScratchEntry>,
    filtered_items: Vec<QuickOpenItem>,
    selected_index: usize,
}

impl EventEmitter<QuickOpenEvent> for QuickOpenModal {}

impl QuickOpenModal {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let query_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search notes or create new..."));

        let all_entries = ScratchService::list_scratches();
        let filtered_items = filter_items(&all_entries, "");

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
            all_entries,
            filtered_items,
            selected_index: 0,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.query_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
    }

    pub fn update_query(&mut self, query: &str, cx: &mut Context<Self>) {
        self.filtered_items = filter_items(&self.all_entries, query);
        self.selected_index = 0;
        cx.notify();
    }

    pub fn select_next(&mut self, cx: &mut Context<Self>) {
        if !self.filtered_items.is_empty() {
            self.selected_index = (self.selected_index + 1).min(self.filtered_items.len() - 1);
            cx.notify();
        }
    }

    pub fn select_prev(&mut self, cx: &mut Context<Self>) {
        if !self.filtered_items.is_empty() {
            self.selected_index = self.selected_index.saturating_sub(1);
            cx.notify();
        }
    }

    pub fn confirm(&mut self, cx: &mut Context<Self>) {
        if let Some(item) = self.filtered_items.get(self.selected_index) {
            match item {
                QuickOpenItem::Note(matched) => {
                    cx.emit(QuickOpenEvent::OpenFile(matched.entry.path.clone()));
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
            list_children.push(
                div()
                    .p_4()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child("No matching notes found")
                    .into_any_element(),
            );
        } else {
            for (idx, item) in self.filtered_items.iter().enumerate() {
                let is_selected = idx == self.selected_index;
                let element = match item {
                    QuickOpenItem::Note(matched) => {
                        let rel_time = matched.entry.modified.map(format_relative_time).unwrap_or_default();
                        let title = matched.entry.title.clone();
                        let filename = matched.entry.filename.clone();

                        let title_element = render_highlighted_text(&title, &matched.title_matches, theme.foreground, theme.primary, true);

                        let filename_element = if matched.filename_matches.is_empty() {
                            div()
                                .text_xs()
                                .text_color(theme.muted_foreground)
                                .child(format!("({filename})"))
                                .into_any_element()
                        } else {
                            h_flex()
                                .items_center()
                                .text_xs()
                                .child(div().text_color(theme.muted_foreground).child("("))
                                .child(render_highlighted_text(
                                    &filename,
                                    &matched.filename_matches,
                                    theme.muted_foreground,
                                    theme.primary,
                                    false,
                                ))
                                .child(div().text_color(theme.muted_foreground).child(")"))
                                .into_any_element()
                        };

                        h_flex()
                            .id(SharedString::from(format!("palette-item-{idx}")))
                            .items_center()
                            .justify_between()
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .when(is_selected, |el| el.bg(theme.accent.opacity(0.2)))
                            .when(!is_selected, |el| el.hover(|s| s.bg(theme.accent.opacity(0.08))))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.selected_index = idx;
                                this.confirm(cx);
                            }))
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .child(Icon::new(IconName::FileText).size(px(14.0)).text_color(theme.muted_foreground))
                                    .child(div().text_sm().child(title_element))
                                    .child(filename_element),
                            )
                            .child(div().text_xs().text_color(theme.muted_foreground).child(rel_time))
                            .into_any_element()
                    }
                    QuickOpenItem::NewNote { query } => {
                        let label = if query.is_empty() {
                            "Create new scratchpad".to_string()
                        } else {
                            format!("Create new scratchpad \"{query}\"")
                        };

                        h_flex()
                            .id(SharedString::from(format!("palette-item-new-{idx}")))
                            .items_center()
                            .justify_between()
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .cursor_pointer()
                            .when(is_selected, |el| el.bg(theme.accent.opacity(0.2)))
                            .when(!is_selected, |el| el.hover(|s| s.bg(theme.accent.opacity(0.08))))
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
                                    .bg(theme.accent.opacity(0.3))
                                    .text_color(theme.foreground)
                                    .child("New"),
                            )
                            .into_any_element()
                    }
                };
                list_children.push(element);
            }
        }

        v_flex()
            .id("quick-open-modal")
            .key_context(CONTEXT)
            .track_focus(&self.focus_handle)
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
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
            .w(px(520.0))
            .bg(theme.background)
            .border_1()
            .border_color(theme.border)
            .rounded_lg()
            .shadow_xl()
            .overflow_hidden()
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
            .child(
                div()
                    .id("palette-items-scroll")
                    .max_h(px(320.0))
                    .overflow_y_scroll()
                    .p_1p5()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .children(list_children),
            )
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
                    .child(h_flex().gap_3().child("↑↓ Navigate").child("↵ Select").child("Esc Dismiss"))
                    .child(format!("{} items", self.filtered_items.len())),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::{QuickOpenItem, filter_items, format_relative_time};
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
        let entries = vec![
            ScratchEntry {
                id: Some(1),
                filename: "scratch_1.md".to_string(),
                path: PathBuf::from("scratch_1.md"),
                title: "Rust Notes".to_string(),
                modified: None,
            },
            ScratchEntry {
                id: Some(2),
                filename: "scratch_2.md".to_string(),
                path: PathBuf::from("scratch_2.md"),
                title: "Hex Analysis".to_string(),
                modified: None,
            },
        ];

        let items = filter_items(&entries, "");
        assert_eq!(items.len(), 3);
        match &items[0] {
            QuickOpenItem::Note(m) => assert_eq!(m.entry, entries[0]),
            _ => panic!("expected note"),
        }
        match &items[1] {
            QuickOpenItem::Note(m) => assert_eq!(m.entry, entries[1]),
            _ => panic!("expected note"),
        }
        assert_eq!(items[2], QuickOpenItem::NewNote { query: String::new() });
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
            },
            ScratchEntry {
                id: Some(2),
                filename: "scratch_2.md".to_string(),
                path: PathBuf::from("scratch_2.md"),
                title: "file_management_tool".to_string(),
                modified: None,
            },
            ScratchEntry {
                id: Some(3),
                filename: "scratch_3.md".to_string(),
                path: PathBuf::from("scratch_3.md"),
                title: "FastMessageThread".to_string(),
                modified: None,
            },
        ];

        // Searching "fmt" with funap/fuzzy-matcher scoring:
        // 1. file_management_tool (score 22: first letter + boundary matches)
        // 2. FastMessageThread (score 19: camel case boundaries)
        // 3. format_table (score 18)
        let items = filter_items(&entries, "fmt");
        assert_eq!(items.len(), 4);
        match &items[0] {
            QuickOpenItem::Note(m) => {
                assert_eq!(m.entry.title, "file_management_tool");
                assert_eq!(m.score, 22);
                assert!(!m.title_matches.is_empty());
            }
            _ => panic!("expected note"),
        }
        match &items[1] {
            QuickOpenItem::Note(m) => {
                assert_eq!(m.entry.title, "FastMessageThread");
                assert_eq!(m.score, 19);
                assert!(!m.title_matches.is_empty());
            }
            _ => panic!("expected note"),
        }
        match &items[2] {
            QuickOpenItem::Note(m) => {
                assert_eq!(m.entry.title, "format_table");
                assert_eq!(m.score, 18);
                assert!(!m.title_matches.is_empty());
            }
            _ => panic!("expected note"),
        }
        assert_eq!(items[3], QuickOpenItem::NewNote { query: "fmt".to_string() });
    }

    #[test]
    fn test_filter_items_matching_by_filename() {
        let entries = vec![ScratchEntry {
            id: Some(1),
            filename: "my_custom_scratch.md".to_string(),
            path: PathBuf::from("my_custom_scratch.md"),
            title: "Daily Notes".to_string(),
            modified: None,
        }];

        let items = filter_items(&entries, "custom");
        assert_eq!(items.len(), 2);
        match &items[0] {
            QuickOpenItem::Note(m) => {
                assert_eq!(m.entry.filename, "my_custom_scratch.md");
                assert!(!m.filename_matches.is_empty());
            }
            _ => panic!("expected note"),
        }
    }

    #[test]
    fn test_filter_items_no_match() {
        let entries = vec![ScratchEntry {
            id: Some(1),
            filename: "scratch_1.md".to_string(),
            path: PathBuf::from("scratch_1.md"),
            title: "Rust Notes".to_string(),
            modified: None,
        }];

        let items = filter_items(&entries, "nonexistent999");
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0],
            QuickOpenItem::NewNote {
                query: "nonexistent999".to_string()
            }
        );
    }
}

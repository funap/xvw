use std::sync::Arc;

use gpui_kit::component::command::{Command, CommandItem, CommandState};
use gpui_kit::component::{ActiveTheme as _, Disableable as _, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::core::fuzzy_matcher::FuzzyMatcher;
use crate::ui::dialogs::common::{
    PALETTE_LIST_HEIGHT, PALETTE_WIDTH, render_highlighted_text, render_palette_empty_message, render_palette_footer_hints, render_shortcut_badge,
};
use crate::ui::menus::{MenuEditorState, PaletteCommand};

/// Registers keybindings for the command palette modal context.
pub fn init(_cx: &mut App) {}

/// Filtered and scored item ready for display in the command palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommandPaletteItem {
    pub command_index: usize,
    pub category: String,
    pub label: String,
    pub title: String,
    pub shortcut: Option<String>,
    pub is_executable: bool,
    pub score: i32,
    pub title_matches: Vec<usize>,
}

#[derive(Clone)]
pub enum CommandPaletteEvent {
    Execute(Arc<dyn Fn() -> Box<dyn Action> + Send + Sync>),
    Cancel,
}

/// Pure helper that filters and ranks commands by query using fuzzy matching.
pub fn filter_commands(commands: &[PaletteCommand], query: &str, state: &MenuEditorState) -> Vec<CommandPaletteItem> {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        return commands
            .iter()
            .enumerate()
            .map(|(idx, cmd)| {
                let is_executable = cmd.is_executable(state);
                CommandPaletteItem {
                    command_index: idx,
                    category: cmd.category.to_string(),
                    label: cmd.label.to_string(),
                    title: cmd.title.to_string(),
                    shortcut: cmd.shortcut.as_ref().map(|s| s.to_string()),
                    is_executable,
                    score: if is_executable { 1 } else { 0 },
                    title_matches: Vec::new(),
                }
            })
            .collect();
    }

    let mut matcher = FuzzyMatcher::new(trimmed);
    let mut scored: Vec<(i32, usize, CommandPaletteItem)> = Vec::with_capacity(commands.len());

    for (idx, cmd) in commands.iter().enumerate() {
        let is_executable = cmd.is_executable(state);
        let title_str = cmd.title.as_ref();
        let label_str = cmd.label.as_ref();

        let title_match = matcher.score_match_with_positions(title_str);
        let label_match = matcher.score_match_with_positions(label_str);

        if let Some((score, title_matches)) = title_match {
            let bonus = if is_executable { 10 } else { -20 };
            // Extra bonus if the command label itself matched directly
            let label_bonus = if label_match.is_some() { 5 } else { 0 };
            let final_score = score + bonus + label_bonus;

            scored.push((
                final_score,
                idx,
                CommandPaletteItem {
                    command_index: idx,
                    category: cmd.category.to_string(),
                    label: cmd.label.to_string(),
                    title: cmd.title.to_string(),
                    shortcut: cmd.shortcut.as_ref().map(|s| s.to_string()),
                    is_executable,
                    score: final_score,
                    title_matches,
                },
            ));
        }
    }

    // Sort descending by score; on tie, preserve command declaration order
    scored.sort_by(|(s_a, idx_a, _), (s_b, idx_b, _)| s_b.cmp(s_a).then_with(|| idx_a.cmp(idx_b)));

    scored.into_iter().map(|(_, _, item)| item).collect()
}

pub struct CommandPaletteModal {
    pub command_state: Entity<CommandState>,
    all_commands: Vec<PaletteCommand>,
    filtered_items: Vec<CommandPaletteItem>,
    editor_state: MenuEditorState,
    query: String,
}

impl EventEmitter<CommandPaletteEvent> for CommandPaletteModal {}

impl CommandPaletteModal {
    pub fn new(commands: Vec<PaletteCommand>, editor_state: MenuEditorState, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let command_state = cx.new(|cx| CommandState::new(window, cx));
        let filtered_items = filter_commands(&commands, "", &editor_state);

        Self {
            command_state,
            all_commands: commands,
            filtered_items,
            editor_state,
            query: String::new(),
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.command_state.update(cx, |state, cx| {
            state.focus(window, cx);
        });
    }

    pub fn update_query(&mut self, query: &str, cx: &mut Context<Self>) {
        if self.query == query {
            return;
        }
        self.query = query.to_string();
        self.filtered_items = filter_commands(&self.all_commands, query, &self.editor_state);
        cx.notify();
    }

    pub fn confirm_index(&mut self, row_ix: usize, cx: &mut Context<Self>) {
        if let Some(item) = self.filtered_items.get(row_ix)
            && let Some(cmd) = self.all_commands.get(item.command_index)
            && item.is_executable
        {
            cx.emit(CommandPaletteEvent::Execute(cmd.action.clone()));
        }
    }

    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        cx.emit(CommandPaletteEvent::Cancel);
    }
}

impl Render for CommandPaletteModal {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = cx.entity().clone();
        let theme = cx.theme().clone();

        let items: Vec<CommandItem> = self
            .filtered_items
            .iter()
            .map(|item| {
                let is_executable = item.is_executable;
                let title = item.title.clone();
                let title_matches = item.title_matches.clone();
                let category = item.category.clone();
                let shortcut = item.shortcut.clone();
                let theme_clone = theme.clone();

                CommandItem::new()
                    .label(item.title.clone())
                    .disabled(!is_executable)
                    .child(move |_window, _cx| {
                        let title_color = if is_executable {
                            theme_clone.foreground
                        } else {
                            theme_clone.muted_foreground
                        };
                        let title_element = render_highlighted_text(
                            &title,
                            &title_matches,
                            title_color,
                            if is_executable { theme_clone.primary } else { theme_clone.muted_foreground },
                            true,
                        );

                        h_flex()
                            .items_center()
                            .justify_between()
                            .w_full()
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .overflow_hidden()
                                    .child(
                                        div()
                                            .text_xs()
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .bg(theme_clone.secondary)
                                            .text_color(theme_clone.primary)
                                            .child(category.clone()),
                                    )
                                    .child(div().text_sm().child(title_element)),
                            )
                            .child(
                                h_flex()
                                    .items_center()
                                    .gap_2()
                                    .when_some(shortcut.as_ref(), |el, sc| el.child(render_shortcut_badge(sc)))
                                    .when(!is_executable, |el| {
                                        el.child(div().text_xs().text_color(theme_clone.muted_foreground).child("unavailable"))
                                    }),
                            )
                    })
            })
            .collect();

        let count = self.filtered_items.len();
        let footer_theme = theme.clone();
        let view_for_query = view.clone();
        let view_for_confirm = view.clone();
        let view_for_cancel = view;

        div()
            .occlude()
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation();
            })
            .child(
                Command::new(&self.command_state)
                    .w(px(PALETTE_WIDTH))
                    .max_h(px(PALETTE_LIST_HEIGHT))
                    .searchable(true)
                    .filterable(false)
                    .placeholder("Type a command to run...")
                    .items(items)
                    .empty(move |_state, _window, cx| render_palette_empty_message("No matching commands found", cx.theme()))
                    .footer(move |_state, _window, _cx| {
                        h_flex()
                            .items_center()
                            .justify_between()
                            .w_full()
                            .text_xs()
                            .text_color(footer_theme.muted_foreground)
                            .child(render_palette_footer_hints())
                            .child(format!("{} commands", count))
                    })
                    .on_query(move |query, _window, cx| {
                        view_for_query.update(cx, |this, cx| {
                            this.update_query(query, cx);
                        });
                    })
                    .on_confirm(move |index_path, _window, cx| {
                        view_for_confirm.update(cx, |this, cx| {
                            this.confirm_index(index_path.row, cx);
                        });
                    })
                    .on_cancel(move |_window, cx| {
                        view_for_cancel.update(cx, |this, cx| {
                            this.cancel(cx);
                        });
                    }),
            )
    }
}

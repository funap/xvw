use std::sync::Arc;

use gpui_kit::component::input::{self, Input, InputState};
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable, Size, h_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::actions::{PaletteCancel, PaletteConfirm, PaletteSelectNext, PaletteSelectPrev};
use crate::core::fuzzy_matcher::FuzzyMatcher;
use crate::ui::dialogs::common::{palette_list_container, palette_modal_container, render_highlighted_text, render_palette_empty_message};
use crate::ui::icon::IconName;
use crate::ui::menus::{MenuEditorState, PaletteCommand};

const CONTEXT: &str = "CommandPaletteModal";

/// Registers keybindings for the command palette modal context.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", PaletteSelectNext, Some(CONTEXT)),
        KeyBinding::new("up", PaletteSelectPrev, Some(CONTEXT)),
        KeyBinding::new("enter", PaletteConfirm, Some(CONTEXT)),
        KeyBinding::new("escape", PaletteCancel, Some(CONTEXT)),
    ]);
}

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

/// Maximum number of candidate items to display at once to prevent layout stalls.
pub const MAX_PALETTE_RESULTS: usize = 80;

/// Pure helper that filters and ranks commands by query using fuzzy matching.
pub fn filter_commands(commands: &[PaletteCommand], query: &str, state: &MenuEditorState) -> Vec<CommandPaletteItem> {
    let trimmed = query.trim();

    if trimmed.is_empty() {
        return commands
            .iter()
            .enumerate()
            .take(MAX_PALETTE_RESULTS)
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

    scored.into_iter().take(MAX_PALETTE_RESULTS).map(|(_, _, item)| item).collect()
}

pub struct CommandPaletteModal {
    focus_handle: FocusHandle,
    query_input: Entity<InputState>,
    scroll_handle: ScrollHandle,
    all_commands: Vec<PaletteCommand>,
    filtered_items: Vec<CommandPaletteItem>,
    selected_index: usize,
    editor_state: MenuEditorState,
}

impl EventEmitter<CommandPaletteEvent> for CommandPaletteModal {}

impl CommandPaletteModal {
    pub fn new(commands: Vec<PaletteCommand>, editor_state: MenuEditorState, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        let query_input = cx.new(|cx| InputState::new(window, cx).placeholder("Type a command to run..."));

        let filtered_items = filter_commands(&commands, "", &editor_state);

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
            all_commands: commands,
            filtered_items,
            selected_index: 0,
            editor_state,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.query_input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
    }

    pub fn update_query(&mut self, query: &str, cx: &mut Context<Self>) {
        self.filtered_items = filter_commands(&self.all_commands, query, &self.editor_state);
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
        if let Some(item) = self.filtered_items.get(self.selected_index)
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
        let theme = cx.theme().clone();

        let mut list_children = Vec::new();

        if self.filtered_items.is_empty() {
            list_children.push(render_palette_empty_message("No matching commands found", &theme));
        } else {
            for (idx, item) in self.filtered_items.iter().enumerate() {
                let is_selected = idx == self.selected_index;
                let is_executable = item.is_executable;

                let title_color = if is_executable {
                    theme.foreground
                } else {
                    theme.muted_foreground.opacity(0.6)
                };

                let title_element = render_highlighted_text(
                    &item.title,
                    &item.title_matches,
                    title_color,
                    if is_executable { theme.primary } else { theme.muted_foreground },
                    true,
                );

                let shortcut_badge = item.shortcut.as_ref().map(|sc| {
                    div()
                        .px_1p5()
                        .py_0p5()
                        .rounded_sm()
                        .border_1()
                        .border_color(theme.border.opacity(0.6))
                        .bg(theme.secondary.opacity(0.3))
                        .text_xs()
                        .text_color(theme.muted_foreground)
                        .child(sc.clone())
                });

                let status_badge = if !is_executable {
                    Some(div().text_xs().text_color(theme.muted_foreground.opacity(0.5)).child("unavailable"))
                } else {
                    None
                };

                let element = h_flex()
                    .id(SharedString::from(format!("cmd-palette-item-{idx}")))
                    .items_center()
                    .justify_between()
                    .px_3()
                    .py_1p5()
                    .rounded_md()
                    .cursor_pointer()
                    .when(is_selected, |el| el.bg(theme.accent.opacity(0.2)))
                    .when(!is_selected && is_executable, |el| el.hover(|s| s.bg(theme.accent.opacity(0.08))))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.selected_index = idx;
                        this.confirm(cx);
                    }))
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
                                    .bg(theme.accent.opacity(0.12))
                                    .text_color(theme.primary)
                                    .child(item.category.clone()),
                            )
                            .child(div().text_sm().child(title_element)),
                    )
                    .child(
                        h_flex()
                            .items_center()
                            .gap_2()
                            .when_some(shortcut_badge, |el, sc| el.child(sc))
                            .when_some(status_badge, |el, st| el.child(st)),
                    );

                list_children.push(element.into_any_element());
            }
        }

        palette_modal_container(&theme)
            .id("command-palette-modal")
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
            .child(palette_list_container(&self.scroll_handle, "command-palette-list").children(list_children))
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
                    .child(h_flex().items_center().gap_3().child("↑↓ Navigate").child("↵ Select").child("Esc Dismiss"))
                    .child(format!("{} commands", self.filtered_items.len())),
            )
    }
}

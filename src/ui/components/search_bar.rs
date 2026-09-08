use crate::ui::icon::IconName;
use gpui_kit::component::{
    ActiveTheme, Icon,
    button::{Button, ButtonVariants},
    input::{self, Input, InputState},
};
use gpui_kit::prelude::*;
use gpui_kit::*;

use crate::core::search::SearchMode;

#[allow(dead_code)]
pub enum SearchBarEvent {
    IncrementalSearch(String, SearchMode),
    FullSearch(String, SearchMode),
    Next,
    Prev,
    Dismiss,
}

pub struct SearchBar {
    input: Entity<InputState>,
    mode: SearchMode,
    debounce_task: Option<Task<()>>,
}

impl EventEmitter<SearchBarEvent> for SearchBar {}

impl SearchBar {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(SearchMode::Hex.placeholder()));

        // Subscribe to input changes with debouncing
        cx.subscribe(&input, |this, input, event: &input::InputEvent, cx| {
            if let input::InputEvent::Change = event {
                let query = input.read(cx).value().to_string();
                let mode = this.mode;

                // Cancel previous debounce task
                this.debounce_task = None;

                // Start new debounce task (300ms)
                let task = cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(std::time::Duration::from_millis(300)).await;
                    if let Some(this) = this.upgrade() {
                        this.update(cx, |_this, cx| {
                            cx.emit(SearchBarEvent::IncrementalSearch(query, mode));
                            cx.notify();
                        });
                    }
                });
                this.debounce_task = Some(task);
            }
        })
        .detach();

        Self {
            input,
            mode: SearchMode::Hex,
            debounce_task: None,
        }
    }

    pub fn focus(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |input, cx| {
            input.focus(window, cx);
        });
    }

    pub fn query(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    pub fn mode(&self) -> SearchMode {
        self.mode
    }

    fn on_mode_change(&mut self, mode: SearchMode, window: &mut Window, cx: &mut Context<Self>) {
        self.mode = mode;
        self.input.update(cx, |input, cx| {
            input.set_placeholder(mode.placeholder(), window, cx);
        });
        let query = self.input.read(cx).value().to_string();
        cx.emit(SearchBarEvent::IncrementalSearch(query, self.mode));
        cx.notify();
    }
}

impl Render for SearchBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .items_center()
            .gap_2()
            .p_2()
            .bg(cx.theme().background)
            .border_b_1()
            .border_color(cx.theme().border)
            .key_context("SearchBar")
            .on_action(cx.listener(|_, _: &crate::actions::SearchNext, _, cx| {
                cx.emit(SearchBarEvent::Next);
            }))
            .on_action(cx.listener(|_, _: &crate::actions::SearchPrev, _, cx| {
                cx.emit(SearchBarEvent::Prev);
            }))
            .on_action(cx.listener(|_, _: &crate::actions::ToggleSearch, _, cx| {
                cx.emit(SearchBarEvent::Dismiss);
            }))
            .child(
                div()
                    .flex()
                    .child(
                        if self.mode == SearchMode::Hex {
                            Button::new("hex_mode").label("Hex").primary()
                        } else {
                            Button::new("hex_mode").label("Hex").ghost()
                        }
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.on_mode_change(SearchMode::Hex, window, cx);
                        })),
                    )
                    .child(
                        if self.mode == SearchMode::Text {
                            Button::new("text_mode").label("Text").primary()
                        } else {
                            Button::new("text_mode").label("Text").ghost()
                        }
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.on_mode_change(SearchMode::Text, window, cx);
                        })),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .child(Input::new(&self.input).prefix(Icon::new(IconName::Search).size_3p5()).cleanable(true)),
            )
            .child(Button::new("prev").ghost().icon(IconName::ChevronUp).on_click(cx.listener(|_, _, _, cx| {
                cx.emit(SearchBarEvent::Prev);
            })))
            .child(Button::new("next").ghost().icon(IconName::ChevronDown).on_click(cx.listener(|_, _, _, cx| {
                cx.emit(SearchBarEvent::Next);
            })))
            .child(Button::new("close").ghost().icon(IconName::Close).on_click(cx.listener(|_, _, _, cx| {
                cx.emit(SearchBarEvent::Dismiss);
            })))
    }
}

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", crate::actions::SearchNext, Some("SearchBar")),
        KeyBinding::new("shift-enter", crate::actions::SearchPrev, Some("SearchBar")),
        KeyBinding::new("escape", crate::actions::ToggleSearch, Some("SearchBar")),
    ]);
}

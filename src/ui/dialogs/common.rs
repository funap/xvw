use gpui_kit::component::{h_flex, theme::Theme, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

/// Standard width in pixels for top-level palettes (Command Palette, Quick Open).
pub const PALETTE_WIDTH: f32 = 580.0;

/// Standard fixed height in pixels for the scrollable results list in top-level palettes.
pub const PALETTE_LIST_HEIGHT: f32 = 320.0;

/// Creates a standardized top-level palette shell container (width, background, border, shadow, rounded corners).
pub fn palette_modal_container(theme: &Theme) -> Div {
    v_flex()
        .w(px(PALETTE_WIDTH))
        .bg(theme.background)
        .border_1()
        .border_color(theme.border)
        .rounded_lg()
        .shadow_xl()
        .overflow_hidden()
}

/// Creates a standardized palette results list container with fixed height and scrolling.
pub fn palette_list_container(scroll_handle: &ScrollHandle, id: impl Into<ElementId>) -> Stateful<Div> {
    v_flex()
        .id(id)
        .track_scroll(scroll_handle)
        .h(px(PALETTE_LIST_HEIGHT))
        .overflow_y_scroll()
        .p_1p5()
        .gap_0p5()
}

/// Creates a standardized empty message centered in the fixed-height palette list area.
pub fn render_palette_empty_message(message: &str, theme: &Theme) -> AnyElement {
    div()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .p_4()
        .text_xs()
        .text_color(theme.muted_foreground)
        .child(message.to_string())
        .into_any_element()
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

    let mut match_iter = matches.iter().copied().peekable();
    let mut spans: Vec<(String, bool)> = Vec::new();

    for (byte_offset, ch) in text.char_indices() {
        let is_matched = if let Some(&next_pos) = match_iter.peek() {
            if next_pos == byte_offset {
                match_iter.next();
                true
            } else {
                false
            }
        } else {
            false
        };
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

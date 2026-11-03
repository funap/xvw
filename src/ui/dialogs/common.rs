use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{h_flex, theme::Theme, v_flex};
use gpui_kit::prelude::*;
use gpui_kit::*;

/// Standard width in pixels for top-level palettes (Command Palette, Quick Open).
pub const PALETTE_WIDTH: f32 = 580.0;

/// Standard fixed height in pixels for the scrollable results list in top-level palettes.
pub const PALETTE_LIST_HEIGHT: f32 = 320.0;

/// Creates a standardized top-level palette shell container (width, background, border, shadow, rounded corners).
#[allow(dead_code)]
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
#[allow(dead_code)]
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

fn current_os() -> gpui_kit::OperatingSystem {
    #[cfg(target_os = "macos")]
    {
        gpui_kit::OperatingSystem::Mac
    }
    #[cfg(target_os = "windows")]
    {
        gpui_kit::OperatingSystem::Windows
    }
    #[cfg(target_os = "linux")]
    {
        gpui_kit::OperatingSystem::Linux
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        gpui_kit::OperatingSystem::Unknown
    }
}

/// Parses a display shortcut string (e.g. "⌘S", "Ctrl+S", "⇧⌘P", "Ctrl+Shift+P", "Enter", "Esc", "cmd-s") into a `Keystroke`.
pub fn parse_shortcut_to_keystroke(s: &str) -> Option<Keystroke> {
    if s.is_empty() {
        return None;
    }

    // Direct GPUI parse if formatted with hyphens like "cmd-s" or "ctrl-shift-p"
    if s.contains('-')
        && let Ok(stroke) = Keystroke::parse_with_os(s, current_os())
    {
        return Some(stroke);
    }

    let mut control = false;
    let mut alt = false;
    let mut shift = false;
    let mut platform = false;

    // Check for macOS modifier symbols: ⌃, ⌥, ⇧, ⌘
    let mut rest = s;
    while let Some(ch) = rest.chars().next() {
        match ch {
            '⌃' => {
                control = true;
                rest = &rest[ch.len_utf8()..];
            }
            '⌥' => {
                alt = true;
                rest = &rest[ch.len_utf8()..];
            }
            '⇧' => {
                shift = true;
                rest = &rest[ch.len_utf8()..];
            }
            '⌘' => {
                platform = true;
                rest = &rest[ch.len_utf8()..];
            }
            _ => break,
        }
    }

    if rest != s {
        let key = normalize_key_name(rest);
        return Some(Keystroke {
            modifiers: Modifiers {
                control,
                alt,
                shift,
                platform,
                function: false,
            },
            key,
            key_char: None,
        });
    }

    // Check for plus-separated modifiers: e.g. "Ctrl+Shift+P", "Alt+Ctrl+D"
    if s.contains('+') {
        let parts: Vec<&str> = s.split('+').collect();
        if let Some((&last, modifiers)) = parts.split_last() {
            for &m in modifiers {
                if m.eq_ignore_ascii_case("ctrl") || m.eq_ignore_ascii_case("control") {
                    control = true;
                } else if m.eq_ignore_ascii_case("alt") {
                    alt = true;
                } else if m.eq_ignore_ascii_case("shift") {
                    shift = true;
                } else if m.eq_ignore_ascii_case("win") || m.eq_ignore_ascii_case("cmd") {
                    platform = true;
                }
            }
            let key = normalize_key_name(last);
            return Some(Keystroke {
                modifiers: Modifiers {
                    control,
                    alt,
                    shift,
                    platform,
                    function: false,
                },
                key,
                key_char: None,
            });
        }
    }

    // Standalone key: "Enter", "Esc", "Insert", "F3", etc.
    let key = normalize_key_name(s);
    Some(Keystroke {
        modifiers: Modifiers::none(),
        key,
        key_char: None,
    })
}

fn normalize_key_name(key: &str) -> String {
    let lower = key.to_lowercase();
    match lower.as_str() {
        "esc" | "escape" | "⎋" => "escape".to_string(),
        "enter" | "return" | "↵" => "enter".to_string(),
        "tab" | "⇥" => "tab".to_string(),
        "backspace" | "⌫" => "backspace".to_string(),
        "delete" | "del" | "⌦" => "delete".to_string(),
        "insert" | "ins" => "insert".to_string(),
        "home" => "home".to_string(),
        "end" => "end".to_string(),
        "pageup" => "pageup".to_string(),
        "pagedown" => "pagedown".to_string(),
        "left" | "←" => "left".to_string(),
        "right" | "→" => "right".to_string(),
        "up" | "↑" => "up".to_string(),
        "down" | "↓" => "down".to_string(),
        _ => lower,
    }
}

/// Renders a keybinding shortcut badge using `gpui_kit::component::kbd::Kbd`.
pub fn render_shortcut_badge(shortcut: &str) -> AnyElement {
    if shortcut == "↑↓" {
        let up = Keystroke {
            modifiers: Modifiers::none(),
            key: "up".into(),
            key_char: None,
        };
        let down = Keystroke {
            modifiers: Modifiers::none(),
            key: "down".into(),
            key_char: None,
        };
        return h_flex().items_center().gap_0p5().child(Kbd::new(up)).child(Kbd::new(down)).into_any_element();
    }

    if let Some(stroke) = parse_shortcut_to_keystroke(shortcut) {
        Kbd::new(stroke).into_any_element()
    } else {
        div()
            .text_xs()
            .py_0p5()
            .px_1()
            .min_w_5()
            .text_center()
            .rounded_sm()
            .line_height(relative(1.))
            .flex_shrink_0()
            .child(shortcut.to_string())
            .into_any_element()
    }
}

/// Renders a keyboard hint item (key badge + description) for modal footers.
pub fn render_keyboard_hint(key: &str, label: &str) -> AnyElement {
    h_flex()
        .items_center()
        .gap_1p5()
        .child(render_shortcut_badge(key))
        .child(div().child(label.to_string()))
        .into_any_element()
}

/// Renders standard modal footer keyboard hints (↑↓ Navigate, ↵ Select, Esc Dismiss).
pub fn render_palette_footer_hints() -> AnyElement {
    h_flex()
        .items_center()
        .gap_3()
        .child(render_keyboard_hint("↑↓", "Navigate"))
        .child(render_keyboard_hint("enter", "Select"))
        .child(render_keyboard_hint("escape", "Dismiss"))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::parse_shortcut_to_keystroke;

    #[test]
    fn test_parse_shortcut_to_keystroke() {
        let stroke = parse_shortcut_to_keystroke("⌘S").unwrap();
        assert!(stroke.modifiers.platform);
        assert_eq!(stroke.key, "s");

        let stroke = parse_shortcut_to_keystroke("Ctrl+S").unwrap();
        assert!(stroke.modifiers.control);
        assert_eq!(stroke.key, "s");

        let stroke = parse_shortcut_to_keystroke("⇧⌘C").unwrap();
        assert!(stroke.modifiers.shift);
        assert!(stroke.modifiers.platform);
        assert_eq!(stroke.key, "c");

        let stroke = parse_shortcut_to_keystroke("Ctrl+Shift+C").unwrap();
        assert!(stroke.modifiers.control);
        assert!(stroke.modifiers.shift);
        assert_eq!(stroke.key, "c");

        let stroke = parse_shortcut_to_keystroke("⌥⌘D").unwrap();
        assert!(stroke.modifiers.alt);
        assert!(stroke.modifiers.platform);
        assert_eq!(stroke.key, "d");

        let stroke = parse_shortcut_to_keystroke("Alt+Ctrl+D").unwrap();
        assert!(stroke.modifiers.alt);
        assert!(stroke.modifiers.control);
        assert_eq!(stroke.key, "d");

        let stroke = parse_shortcut_to_keystroke("Esc").unwrap();
        assert_eq!(stroke.key, "escape");

        let stroke = parse_shortcut_to_keystroke("Enter").unwrap();
        assert_eq!(stroke.key, "enter");

        let stroke = parse_shortcut_to_keystroke("F3").unwrap();
        assert_eq!(stroke.key, "f3");

        let stroke = parse_shortcut_to_keystroke("⇧F3").unwrap();
        assert!(stroke.modifiers.shift);
        assert_eq!(stroke.key, "f3");

        let stroke = parse_shortcut_to_keystroke("Shift+F3").unwrap();
        assert!(stroke.modifiers.shift);
        assert_eq!(stroke.key, "f3");

        assert!(parse_shortcut_to_keystroke("").is_none());
    }
}

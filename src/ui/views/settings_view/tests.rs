use super::theme_selector::{ThemeListDelegate, ThemePreviewState, cycle_theme_index};
use crate::core::radix::HexCase;
use gpui_kit::SharedString;
use gpui_kit::component::IndexPath;
use gpui_kit::component::searchable_list::SearchableListDelegate;
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn test_hex_case_dropdown_options() {
    assert_eq!(HexCase::ALL.len(), 2);
    assert_eq!(HexCase::Lower.label(), "Lowercase (a-f)");
    assert_eq!(HexCase::Upper.label(), "Uppercase (A-F)");
    assert_eq!(HexCase::default(), HexCase::Lower);
}

#[test]
fn test_theme_preview_state_start_and_cancel_reverts_to_origin() {
    let mut state = ThemePreviewState::default();
    assert_eq!(state.origin(), None);

    state.start_preview(SharedString::from("Default Light"));
    assert_eq!(state.origin(), Some(&SharedString::from("Default Light")));

    let restored = state.cancel();
    assert_eq!(restored, Some(SharedString::from("Default Light")));
    assert_eq!(state.origin(), None);

    assert_eq!(state.cancel(), None);
}

#[test]
fn test_theme_preview_state_multiple_start_preview_preserves_initial_origin() {
    let mut state = ThemePreviewState::default();
    state.start_preview(SharedString::from("Default Light"));
    state.start_preview(SharedString::from("Ayu Dark"));
    state.start_preview(SharedString::from("Solarized Light"));

    assert_eq!(state.origin(), Some(&SharedString::from("Default Light")));

    let restored = state.cancel();
    assert_eq!(restored, Some(SharedString::from("Default Light")));
}

#[test]
fn test_theme_preview_state_confirm_prevents_revert_on_cancel() {
    let mut state = ThemePreviewState::default();
    state.start_preview(SharedString::from("Default Light"));

    state.confirm();
    assert_eq!(state.origin(), None);

    assert_eq!(state.cancel(), None);
}

#[test]
fn test_theme_preview_state_step_index_navigation() {
    let mut state = ThemePreviewState::default();
    assert_eq!(state.current_index(), None);

    // Zero items returns None
    assert_eq!(state.step_index(true, 0), None);
    assert_eq!(state.step_index(false, 0), None);

    // Step down from None starts at index 0
    assert_eq!(state.step_index(true, 3), Some(0));
    assert_eq!(state.current_index(), Some(0));

    // Step down advances index
    assert_eq!(state.step_index(true, 3), Some(1));
    assert_eq!(state.step_index(true, 3), Some(2));

    // Step down at the end wraps around to 0
    assert_eq!(state.step_index(true, 3), Some(0));

    // Step up at 0 wraps around to the last index
    assert_eq!(state.step_index(false, 3), Some(2));
    assert_eq!(state.step_index(false, 3), Some(1));
    assert_eq!(state.step_index(false, 3), Some(0));

    // Step up from None starts at the last item
    let mut state_up = ThemePreviewState::default();
    assert_eq!(state_up.step_index(false, 3), Some(2));
    assert_eq!(state_up.current_index(), Some(2));
}

#[test]
fn test_theme_preview_state_hover_sync_and_reset() {
    let mut state = ThemePreviewState::default();
    state.start_preview(SharedString::from("Default Light"));

    // Setting hover index updates current_index
    state.set_hover_index(2);
    assert_eq!(state.current_index(), Some(2));

    // Stepping continues from the hovered index
    assert_eq!(state.step_index(true, 5), Some(3));
    assert_eq!(state.step_index(false, 5), Some(2));

    // Cancel resets current_index
    state.cancel();
    assert_eq!(state.current_index(), None);

    // Confirm also resets current_index
    state.start_preview(SharedString::from("Default Light"));
    state.set_hover_index(1);
    state.confirm();
    assert_eq!(state.current_index(), None);
}

#[test]
fn test_theme_preview_state_scoped_borrow_allows_reborrow() {
    let cell = std::cell::RefCell::new(ThemePreviewState::default());
    let step_result = {
        let mut state = cell.borrow_mut();
        state.start_preview(SharedString::from("Default Light"));
        state.step_index(true, 5)
    };
    assert_eq!(step_result, Some(0));

    // Reborrowing succeeds because the previous borrow was scoped and dropped
    cell.borrow_mut().confirm();
    assert_eq!(cell.borrow().origin(), None);
}

#[test]
fn test_cycle_theme_index() {
    assert_eq!(cycle_theme_index(0, true, 0), 0);
    assert_eq!(cycle_theme_index(0, false, 0), 0);

    // Single item
    assert_eq!(cycle_theme_index(0, true, 1), 0);
    assert_eq!(cycle_theme_index(0, false, 1), 0);

    // Three items - step down
    assert_eq!(cycle_theme_index(0, true, 3), 1);
    assert_eq!(cycle_theme_index(1, true, 3), 2);
    assert_eq!(cycle_theme_index(2, true, 3), 0); // wrap to start

    // Three items - step up
    assert_eq!(cycle_theme_index(0, false, 3), 2); // wrap to end
    assert_eq!(cycle_theme_index(2, false, 3), 1);
    assert_eq!(cycle_theme_index(1, false, 3), 0);
}

#[test]
fn test_theme_preview_state_open_tracking() {
    let mut state = ThemePreviewState::default();
    assert!(!state.is_open());

    state.set_open(true);
    assert!(state.is_open());

    state.confirm();
    assert!(!state.is_open());

    state.set_open(true);
    assert!(state.is_open());

    state.cancel();
    assert!(!state.is_open());
}

#[test]
fn test_theme_list_delegate() {
    let themes = vec![SharedString::from("Light"), SharedString::from("Dark")];
    let state = Rc::new(RefCell::new(ThemePreviewState::default()));
    let delegate = ThemeListDelegate::new(themes, state);

    assert_eq!(delegate.items_count(0), 2);
    assert_eq!(delegate.item(IndexPath::new(0)), Some(&SharedString::from("Light")));
    assert_eq!(delegate.item(IndexPath::new(1)), Some(&SharedString::from("Dark")));
    assert_eq!(delegate.item(IndexPath::new(2)), None);

    assert_eq!(delegate.position(&SharedString::from("Light")), Some(IndexPath::new(0)));
    assert_eq!(delegate.position(&SharedString::from("Dark")), Some(IndexPath::new(1)));
    assert_eq!(delegate.position(&SharedString::from("Unknown")), None);
}

#[test]
fn test_theme_list_delegate_is_item_checked() {
    let themes = vec![SharedString::from("Light"), SharedString::from("Dark")];
    let state = Rc::new(RefCell::new(ThemePreviewState::default()));
    let delegate = ThemeListDelegate::new(themes, state.clone());

    let selection = vec![(IndexPath::new(0), SharedString::from("Light"))];
    assert!(delegate.check_item(&SharedString::from("Light"), &selection));
    assert!(!delegate.check_item(&SharedString::from("Dark"), &selection));

    state.borrow_mut().start_preview(SharedString::from("Dark"));
    assert!(!delegate.check_item(&SharedString::from("Light"), &selection));
    assert!(delegate.check_item(&SharedString::from("Dark"), &selection));
}

#[test]
fn test_settings_input_validators() {
    use super::{validate_bytes_per_row, validate_font_family, validate_font_size, validate_yaml_sha_threshold};

    assert_eq!(validate_font_family(" JetBrains Mono "), Ok("JetBrains Mono".to_string()));
    assert!(validate_font_family("   ").is_err());

    assert_eq!(validate_font_size("14"), Ok(14.0));
    assert!(validate_font_size("0").is_err());
    assert!(validate_font_size("999").is_err());
    assert!(validate_font_size("abc").is_err());

    assert_eq!(validate_bytes_per_row("16"), Ok(16));
    assert!(validate_bytes_per_row("0").is_err());
    assert!(validate_bytes_per_row("128").is_err());
    assert!(validate_bytes_per_row("-4").is_err());

    assert_eq!(validate_yaml_sha_threshold("0"), Ok(0));
    assert_eq!(validate_yaml_sha_threshold("32"), Ok(32));
    assert!(validate_yaml_sha_threshold("not-a-num").is_err());
}

#[test]
fn test_matches_settings_filter() {
    use super::matches_settings_filter;

    assert!(matches_settings_filter("", "Editor", "Font Size", "points"));
    assert!(matches_settings_filter("  font size  ", "Editor", "Font Size", "points"));
    assert!(matches_settings_filter("editor points", "Editor", "Font Size", "points"));
    assert!(!matches_settings_filter("radix", "Editor", "Font Size", "points"));
}

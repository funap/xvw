use crate::ui::dialogs::command_palette_modal::filter_commands;
use crate::ui::menus::{MenuDef, MenuEditorState, MenuItemDef, PaletteCommand};

fn make_test_commands() -> Vec<PaletteCommand> {
    let menus = vec![
        MenuDef {
            name: "File",
            items: vec![
                MenuItemDef::action("New File...", crate::actions::NewFile),
                MenuItemDef::action_with_condition("Save", crate::actions::Save, |s| s.has_doc && !s.is_read_only),
            ],
        },
        MenuDef {
            name: "Edit",
            items: vec![
                MenuItemDef::action_with_condition("Undo", crate::actions::Undo, |s| s.can_undo),
                MenuItemDef::submenu("Copy As", vec![MenuItemDef::action("as Hex Dump", crate::actions::CopyAsHexDump)]),
            ],
        },
    ];
    crate::ui::menus::collect_palette_commands(&menus)
}

#[test]
fn test_filter_commands_empty_query() {
    let commands = make_test_commands();
    let state = MenuEditorState::default();
    let items = filter_commands(&commands, "", &state);
    assert_eq!(items.len(), 4);
    assert_eq!(items[0].title, "File: New File...");
    assert_eq!(items[1].title, "File: Save");
    assert_eq!(items[2].title, "Edit: Undo");
    assert_eq!(items[3].title, "Edit: Copy As > as Hex Dump");
}

#[test]
fn test_filter_commands_matching() {
    let commands = make_test_commands();
    let state = MenuEditorState {
        has_doc: true,
        is_read_only: false,
        ..Default::default()
    };

    // Query "Save"
    let items = filter_commands(&commands, "Save", &state);
    assert!(!items.is_empty());
    assert_eq!(items[0].title, "File: Save");
    assert!(items[0].is_executable);
    assert!(!items[0].title_matches.is_empty());

    // Query "Hex"
    let items_hex = filter_commands(&commands, "Hex", &state);
    assert_eq!(items_hex.len(), 1);
    assert_eq!(items_hex[0].title, "Edit: Copy As > as Hex Dump");
}

#[test]
fn test_filter_commands_executable_priority() {
    let commands = make_test_commands();
    // State where Undo is disabled (can_undo = false)
    let state = MenuEditorState {
        can_undo: false,
        ..Default::default()
    };
    let items = filter_commands(&commands, "Undo", &state);
    assert_eq!(items.len(), 1);
    assert!(!items[0].is_executable);
}

#[test]
fn test_filter_real_panel_commands() {
    let menus = crate::ui::menus::application_menus();
    let commands = crate::ui::menus::collect_palette_commands(&menus);
    let state = MenuEditorState::default();

    // Searching "Inspector" finds the Inspector panel
    let inspector_results = filter_commands(&commands, "Inspector", &state);
    assert!(!inspector_results.is_empty());
    assert_eq!(inspector_results[0].title, "View: Panels > Inspector");

    // Searching "Search" finds the Search panel
    let search_results = filter_commands(&commands, "Panels Search", &state);
    assert!(!search_results.is_empty());
    assert_eq!(search_results[0].title, "View: Panels > Search");
}

#[test]
fn test_palette_dimensions_consistency() {
    use crate::ui::dialogs::common::{PALETTE_LIST_HEIGHT, PALETTE_WIDTH};

    // Both Command Palette and Quick Open share these exact dimensions
    assert_eq!(PALETTE_WIDTH, 580.0);
    assert_eq!(PALETTE_LIST_HEIGHT, 320.0);
}

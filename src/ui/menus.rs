use std::sync::Arc;

use crate::core::encoding::Encoding;
use crate::core::radix::{ByteGroupSize, ByteOrder, DisplayRadix};
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{Action, Context, Menu, MenuItem, OsAction, SharedString, Window};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct MenuEditorState {
    pub has_doc: bool,
    pub has_tab: bool,
    pub has_diff: bool,
    pub is_read_only: bool,
    pub is_insert_mode: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub has_selection: bool,
    pub can_copy: bool,
    pub can_close_others: bool,
    pub can_close_right: bool,
    pub has_saved: bool,
    pub has_scratch: bool,
    pub encoding: Option<Encoding>,
    pub radix: Option<DisplayRadix>,
    pub group_size: Option<ByteGroupSize>,
    pub byte_order: Option<ByteOrder>,
    pub inline_structure_view: bool,
    pub hide_unbookmarked: bool,
}

type StatePredicate = Arc<dyn Fn(&MenuEditorState) -> bool + Send + Sync>;

#[derive(Clone)]
pub enum MenuItemDef {
    Action {
        label: SharedString,
        action: Arc<dyn Fn() -> Box<dyn Action> + Send + Sync>,
        is_enabled: Option<fn(&MenuEditorState) -> bool>,
        is_checked: Option<StatePredicate>,
        os_action: Option<OsAction>,
    },
    Submenu {
        label: SharedString,
        items: Vec<MenuItemDef>,
    },
    Separator,
}

impl MenuItemDef {
    pub fn action<A: Action + Clone + 'static + Sync>(label: impl Into<SharedString>, action: A) -> Self {
        Self::Action {
            label: label.into(),
            action: Arc::new(move || Box::new(action.clone())),
            is_enabled: None,
            is_checked: None,
            os_action: None,
        }
    }

    pub fn action_with_condition<A: Action + Clone + 'static + Sync>(
        label: impl Into<SharedString>,
        action: A,
        is_enabled: fn(&MenuEditorState) -> bool,
    ) -> Self {
        Self::Action {
            label: label.into(),
            action: Arc::new(move || Box::new(action.clone())),
            is_enabled: Some(is_enabled),
            is_checked: None,
            os_action: None,
        }
    }

    pub fn action_with_condition_and_check<A, F>(label: impl Into<SharedString>, action: A, is_enabled: fn(&MenuEditorState) -> bool, is_checked: F) -> Self
    where
        A: Action + Clone + 'static + Sync,
        F: Fn(&MenuEditorState) -> bool + Send + Sync + 'static,
    {
        Self::Action {
            label: label.into(),
            action: Arc::new(move || Box::new(action.clone())),
            is_enabled: Some(is_enabled),
            is_checked: Some(Arc::new(is_checked)),
            os_action: None,
        }
    }

    pub fn action_with_condition_and_os_action<A: Action + Clone + 'static + Sync>(
        label: impl Into<SharedString>,
        action: A,
        is_enabled: fn(&MenuEditorState) -> bool,
        os_action: OsAction,
    ) -> Self {
        Self::Action {
            label: label.into(),
            action: Arc::new(move || Box::new(action.clone())),
            is_enabled: Some(is_enabled),
            is_checked: None,
            os_action: Some(os_action),
        }
    }

    pub fn submenu(label: impl Into<SharedString>, items: Vec<MenuItemDef>) -> Self {
        Self::Submenu { label: label.into(), items }
    }

    pub fn separator() -> Self {
        Self::Separator
    }

    pub fn to_gpui_menu_item(&self, state: &MenuEditorState) -> MenuItem {
        match self {
            MenuItemDef::Action {
                label,
                action,
                is_enabled,
                is_checked,
                os_action,
            } => MenuItem::Action {
                name: label.clone(),
                action: (action)(),
                os_action: *os_action,
                checked: is_checked.as_ref().is_some_and(|f| f(state)),
                disabled: is_enabled.is_some_and(|f| !f(state)),
            },
            MenuItemDef::Submenu { label, items } => MenuItem::submenu(Menu {
                name: label.clone(),
                items: items.iter().map(|item| item.to_gpui_menu_item(state)).collect(),
                disabled: false,
            }),
            MenuItemDef::Separator => MenuItem::separator(),
        }
    }

    pub fn apply_to_popup_menu(&self, menu: PopupMenu, state: &MenuEditorState, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
        match self {
            MenuItemDef::Action {
                label,
                action,
                is_enabled,
                is_checked,
                ..
            } => {
                let disabled = is_enabled.is_some_and(|f| !f(state));
                if let Some(checked_fn) = is_checked {
                    menu.menu_with_check_and_disabled(label.clone(), checked_fn(state), (action)(), disabled)
                } else {
                    menu.menu_with_disabled(label.clone(), (action)(), disabled)
                }
            }
            MenuItemDef::Submenu { label, items } => {
                let sub_items = items.clone();
                let state_copy = *state;
                menu.submenu(label.clone(), window, cx, move |sub_menu, window, cx| {
                    sub_items.iter().fold(sub_menu, |m, item| item.apply_to_popup_menu(m, &state_copy, window, cx))
                })
            }
            MenuItemDef::Separator => menu.separator(),
        }
    }
}

#[derive(Clone)]
pub struct MenuDef {
    pub name: &'static str,
    pub items: Vec<MenuItemDef>,
}

impl MenuDef {
    pub fn to_gpui_menu(&self, state: &MenuEditorState) -> Menu {
        Menu {
            name: self.name.into(),
            items: self.items.iter().map(|item| item.to_gpui_menu_item(state)).collect(),
            disabled: false,
        }
    }

    pub fn build_popup_menu(&self, mut menu: PopupMenu, state: &MenuEditorState, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
        for item in &self.items {
            menu = item.apply_to_popup_menu(menu, state, window, cx);
        }
        menu
    }
}

/// A flat command descriptor suitable for display and filtering in the Command Palette.
#[derive(Clone)]
pub struct PaletteCommand {
    pub category: SharedString,
    pub label: SharedString,
    pub title: SharedString,
    pub action: Arc<dyn Fn() -> Box<dyn Action> + Send + Sync>,
    pub is_enabled: Option<fn(&MenuEditorState) -> bool>,
    pub shortcut: Option<SharedString>,
}

impl PaletteCommand {
    pub fn is_executable(&self, state: &MenuEditorState) -> bool {
        self.is_enabled.map(|f| f(state)).unwrap_or(true)
    }
}

pub(crate) fn shortcut_for_label(category: &str, label: &str) -> Option<&'static str> {
    #[cfg(target_os = "macos")]
    {
        match (category, label) {
            ("File", "New File...") => Some("⌘N"),
            ("File", "Open File...") => Some("⌘O"),
            ("File", "Quick Open...") => Some("⌘P"),
            ("File", "Open Folder...") => Some("⇧⌘O"),
            ("File", "Save") => Some("⌘S"),
            ("File", "Save As...") => Some("⇧⌘S"),
            ("File", "Close Tab") => Some("⌘W"),
            ("File", "Settings...") => Some("⌘,"),
            ("File", "Quit") => Some("⌘Q"),
            ("Edit", "Undo") => Some("⌘Z"),
            ("Edit", "Redo") => Some("⇧⌘Z"),
            ("Edit", "Cut") => Some("⌘X"),
            ("Edit", "Copy") => Some("⌘C"),
            ("Edit", "Paste") => Some("⌘V"),
            ("Edit", "as Hex Dump") | ("Edit > Copy As", "as Hex Dump") => Some("⇧⌘C"),
            ("Edit", "Select All") => Some("⌘A"),
            ("Edit", "Find") => Some("⌘F"),
            ("Edit", "Find in File (Scan All)") => Some("⇧⌘F"),
            ("Edit", "Find Next") | ("Go", "Find Next") => Some("⌘G"),
            ("Edit", "Find Previous") | ("Go", "Find Previous") => Some("⇧⌘G"),
            ("Edit", "Toggle Insert Mode") => Some("⌥⌘I"),
            ("Edit", "Unfold at Cursor")
            | ("Edit", "Unfold Bookmark at Cursor")
            | ("Go", "Unfold Bookmark at Cursor")
            | ("Go > Bookmark Visibility", "Unfold Bookmark at Cursor")
            | ("Edit > Bookmark Visibility", "Unfold Bookmark at Cursor") => Some("⇧⌘]"),
            ("View", "Toggle Left Panel") => Some("⌘B"),
            ("View", "Toggle Right Panel (Scratchpad)") => Some("⌥⌘B"),
            ("View", "Focus Active Editor") => Some("Esc"),
            ("View", "Command Palette...") => Some("⇧⌘P"),
            ("View", "Search") => Some("⇧⌘F"),
            ("View", "Break Line")
            | ("View", "Add Line Break")
            | ("Edit", "Break Line")
            | ("Edit", "Add Line Break")
            | ("Edit > Custom Line Breaks", "Add Line Break")
            | ("View > Custom Line Breaks", "Break Line")
            | ("View > Custom Line Breaks", "Add Line Break") => Some("Enter"),
            ("View", "Join Lines")
            | ("View", "Join Line")
            | ("Edit", "Join Lines")
            | ("Edit", "Join Line")
            | ("Edit > Custom Line Breaks", "Join Line")
            | ("View > Custom Line Breaks", "Join Line")
            | ("View > Custom Line Breaks", "Join Lines") => Some("⌘J"),
            ("View", "Reset Custom Breaks")
            | ("View", "Clear All Line Breaks")
            | ("Edit", "Reset Custom Breaks")
            | ("Edit", "Clear All Line Breaks")
            | ("Edit > Custom Line Breaks", "Clear All Line Breaks")
            | ("View > Custom Line Breaks", "Reset Custom Breaks")
            | ("View > Custom Line Breaks", "Clear All Line Breaks") => Some("⇧⌘Backspace"),
            ("View", "Reset Breaks in Selection")
            | ("Edit", "Reset Breaks in Selection")
            | ("Edit > Custom Line Breaks", "Reset Breaks in Selection")
            | ("View > Custom Line Breaks", "Reset Breaks in Selection") => Some("⇧Enter"),
            ("Go", "Go to Address...") => Some("⌘L"),
            ("Go", "Go to Beginning") | ("Go", "Beginning of File") => Some("⌘Home"),
            ("Go", "Go to End") | ("Go", "End of File") => Some("⌘End"),
            ("Go", "Next Difference") | ("Analysis", "Next Difference") => Some("F3"),
            ("Go", "Previous Difference") | ("Analysis", "Previous Difference") => Some("⇧F3"),
            ("Analysis", "Load Definition...") => Some("⇧⌘K"),
            ("Analysis", "Toggle Inline Structure View") => Some("⇧⌘V"),
            ("Analysis", "Compare Open Files...") => Some("⌥⌘D"),
            ("Scratch", "Toggle Scratchpad Panel") => Some("⌥⌘B"),
            ("Scratch", "New Scratchpad") => Some("⇧⌘N"),
            ("Scratch", "Switch Note Palette...") => Some("⌘P"),
            ("Scratch", "Toggle View Mode") => Some("⌘E"),
            ("Scratch", "as Offset") => Some("⌥⌘O"),
            ("Scratch", "as Hex Bytes") => Some("⌥⌘H"),
            ("Scratch", "as Text / String") => Some("⌥⌘T"),
            ("Window", "Split Right") => Some("⌘\\"),
            ("Window", "Split Down") => Some("⇧⌘D"),
            ("Window", "Next Tab") => Some("⌃Tab"),
            ("Window", "Previous Tab") => Some("⌃⇧Tab"),
            _ => None,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        match (category, label) {
            ("File", "New File...") => Some("Ctrl+N"),
            ("File", "Open File...") => Some("Ctrl+O"),
            ("File", "Quick Open...") => Some("Ctrl+P"),
            ("File", "Open Folder...") => Some("Ctrl+Shift+O"),
            ("File", "Save") => Some("Ctrl+S"),
            ("File", "Save As...") => Some("Ctrl+Shift+S"),
            ("File", "Close Tab") => Some("Ctrl+W"),
            ("File", "Settings...") => Some("Ctrl+,"),
            ("File", "Quit") => Some("Ctrl+Q"),
            ("Edit", "Undo") => Some("Ctrl+Z"),
            ("Edit", "Redo") => Some("Ctrl+Shift+Z"),
            ("Edit", "Cut") => Some("Ctrl+X"),
            ("Edit", "Copy") => Some("Ctrl+C"),
            ("Edit", "Paste") => Some("Ctrl+V"),
            ("Edit", "as Hex Dump") | ("Edit > Copy As", "as Hex Dump") => Some("Ctrl+Shift+C"),
            ("Edit", "Select All") => Some("Ctrl+A"),
            ("Edit", "Find") => Some("Ctrl+F"),
            ("Edit", "Find in File (Scan All)") => Some("Ctrl+Shift+F"),
            ("Edit", "Find Next") | ("Go", "Find Next") => Some("F3"),
            ("Edit", "Find Previous") | ("Go", "Find Previous") => Some("Shift+F3"),
            ("Edit", "Toggle Insert Mode") => Some("Insert"),
            ("Edit", "Unfold at Cursor")
            | ("Edit", "Unfold Bookmark at Cursor")
            | ("Go", "Unfold Bookmark at Cursor")
            | ("Go > Bookmark Visibility", "Unfold Bookmark at Cursor")
            | ("Edit > Bookmark Visibility", "Unfold Bookmark at Cursor") => Some("Ctrl+Shift+]"),
            ("View", "Toggle Left Panel") => Some("Ctrl+B"),
            ("View", "Toggle Right Panel (Scratchpad)") => Some("Alt+Ctrl+B"),
            ("View", "Focus Active Editor") => Some("Esc"),
            ("View", "Command Palette...") => Some("Ctrl+Shift+P"),
            ("View", "Search") => Some("Ctrl+Shift+F"),
            ("View", "Break Line")
            | ("View", "Add Line Break")
            | ("Edit", "Break Line")
            | ("Edit", "Add Line Break")
            | ("Edit > Custom Line Breaks", "Add Line Break")
            | ("View > Custom Line Breaks", "Break Line")
            | ("View > Custom Line Breaks", "Add Line Break") => Some("Enter"),
            ("View", "Join Lines")
            | ("View", "Join Line")
            | ("Edit", "Join Lines")
            | ("Edit", "Join Line")
            | ("Edit > Custom Line Breaks", "Join Line")
            | ("View > Custom Line Breaks", "Join Line")
            | ("View > Custom Line Breaks", "Join Lines") => Some("Ctrl+J"),
            ("View", "Reset Custom Breaks")
            | ("View", "Clear All Line Breaks")
            | ("Edit", "Reset Custom Breaks")
            | ("Edit", "Clear All Line Breaks")
            | ("Edit > Custom Line Breaks", "Clear All Line Breaks")
            | ("View > Custom Line Breaks", "Reset Custom Breaks")
            | ("View > Custom Line Breaks", "Clear All Line Breaks") => Some("Ctrl+Shift+Backspace"),
            ("View", "Reset Breaks in Selection")
            | ("Edit", "Reset Breaks in Selection")
            | ("Edit > Custom Line Breaks", "Reset Breaks in Selection")
            | ("View > Custom Line Breaks", "Reset Breaks in Selection") => Some("Shift+Enter"),
            ("Go", "Go to Address...") => Some("Ctrl+L"),
            ("Go", "Go to Beginning") | ("Go", "Beginning of File") => Some("Ctrl+Home"),
            ("Go", "Go to End") | ("Go", "End of File") => Some("Ctrl+End"),
            ("Go", "Next Difference") | ("Analysis", "Next Difference") => Some("F3"),
            ("Go", "Previous Difference") | ("Analysis", "Previous Difference") => Some("Shift+F3"),
            ("Analysis", "Load Definition...") => Some("Ctrl+Shift+K"),
            ("Analysis", "Toggle Inline Structure View") => Some("Ctrl+Shift+V"),
            ("Analysis", "Compare Open Files...") => Some("Alt+Ctrl+D"),
            ("Scratch", "Toggle Scratchpad Panel") => Some("Alt+Ctrl+B"),
            ("Scratch", "New Scratchpad") => Some("Ctrl+Shift+N"),
            ("Scratch", "Switch Note Palette...") => Some("Ctrl+P"),
            ("Scratch", "Toggle View Mode") => Some("Ctrl+E"),
            ("Scratch", "as Offset") => Some("Alt+Ctrl+O"),
            ("Scratch", "as Hex Bytes") => Some("Alt+Ctrl+H"),
            ("Scratch", "as Text / String") => Some("Alt+Ctrl+T"),
            ("Window", "Split Right") => Some("Ctrl+\\"),
            ("Window", "Split Down") => Some("Ctrl+Shift+D"),
            ("Window", "Next Tab") => Some("Ctrl+Tab"),
            ("Window", "Previous Tab") => Some("Ctrl+Shift+Tab"),
            _ => None,
        }
    }
}

fn collect_items_into(category: &str, prefix_path: &str, items: &[MenuItemDef], out: &mut Vec<PaletteCommand>) {
    for item in items {
        match item {
            MenuItemDef::Action { label, action, is_enabled, .. } => {
                let full_title = if prefix_path.is_empty() {
                    format!("{category}: {label}")
                } else {
                    format!("{category}: {prefix_path} > {label}")
                };
                let shortcut = shortcut_for_label(category, label.as_ref()).map(SharedString::from);
                out.push(PaletteCommand {
                    category: SharedString::from(category.to_string()),
                    label: label.clone(),
                    title: SharedString::from(full_title),
                    action: Arc::clone(action),
                    is_enabled: *is_enabled,
                    shortcut,
                });
            }
            MenuItemDef::Submenu { label, items } => {
                let next_prefix = if prefix_path.is_empty() {
                    label.to_string()
                } else {
                    format!("{prefix_path} > {label}")
                };
                collect_items_into(category, &next_prefix, items, out);
            }
            MenuItemDef::Separator => {}
        }
    }
}

/// Recursively flattens application menu hierarchies into a list of searchable palette commands.
pub fn collect_palette_commands(menus: &[MenuDef]) -> Vec<PaletteCommand> {
    let mut commands = Vec::new();
    for menu in menus {
        collect_items_into(menu.name, "", &menu.items, &mut commands);
    }
    commands
}

pub fn application_menus() -> Vec<MenuDef> {
    vec![
        build_file_menu(),
        build_edit_menu(),
        build_view_menu(),
        build_go_menu(),
        build_analysis_menu(),
        build_scratch_menu(),
        build_window_menu(),
        build_help_menu(),
    ]
}

fn build_macos_app_menu() -> MenuDef {
    MenuDef {
        name: "xvw",
        items: vec![
            MenuItemDef::action("About xvw", crate::actions::OpenAbout),
            MenuItemDef::separator(),
            MenuItemDef::action("Settings...", crate::actions::OpenSettings),
            MenuItemDef::separator(),
            MenuItemDef::action("Quit xvw", crate::actions::Quit),
        ],
    }
}

pub(crate) fn build_native_menus(state: &MenuEditorState, include_macos_app_menu: bool) -> Vec<Menu> {
    let mut menus = Vec::new();
    if include_macos_app_menu {
        menus.push(build_macos_app_menu().to_gpui_menu(state));
    }
    menus.extend(application_menus().iter().map(|menu| menu.to_gpui_menu(state)));
    menus
}

/// Updates the system menu bar with the latest application menus and editor state.
pub fn update_application_menus_with_state(state: &MenuEditorState, cx: &gpui_kit::App) {
    cx.set_menus(build_native_menus(state, cfg!(target_os = "macos")));
}

/// Updates the system menu bar with default editor state (used during app startup and scratchpad list refreshes).
pub fn update_application_menus(cx: &gpui_kit::App) {
    update_application_menus_with_state(&MenuEditorState::default(), cx);
}

fn build_file_menu() -> MenuDef {
    MenuDef {
        name: "File",
        items: vec![
            MenuItemDef::action("New File...", crate::actions::NewFile),
            MenuItemDef::action("Open File...", crate::actions::OpenFileDialog),
            MenuItemDef::action("Quick Open...", crate::actions::QuickOpen),
            MenuItemDef::action("Open Folder...", crate::actions::OpenFolder),
            MenuItemDef::action("Close Folder", crate::actions::CloseFolder),
            MenuItemDef::separator(),
            MenuItemDef::action_with_condition("Save", crate::actions::Save, |s| !s.is_read_only && s.has_doc),
            MenuItemDef::action_with_condition("Save As...", crate::actions::SaveAs, |s| s.has_doc),
            MenuItemDef::separator(),
            MenuItemDef::submenu(
                "Import",
                vec![
                    MenuItemDef::action("Motorola S-Record / Intel HEX...", crate::actions::ImportHexOrMot),
                    MenuItemDef::action("Base64...", crate::actions::ImportBase64),
                ],
            ),
            MenuItemDef::submenu(
                "Export",
                vec![
                    MenuItemDef::action_with_condition("Base64...", crate::actions::ExportBase64, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Motorola S-Record...", crate::actions::ExportMotorolaSrec, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Intel HEX...", crate::actions::ExportIntelHex, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Raw Binary...", crate::actions::ExportRawBinary, |s| s.has_doc),
                ],
            ),
            MenuItemDef::separator(),
            MenuItemDef::action_with_condition("Close Tab", crate::actions::CloseActivePanel, |s| s.has_tab || s.has_doc || s.has_diff),
            MenuItemDef::submenu(
                "Close Other Tabs",
                vec![
                    MenuItemDef::action_with_condition("Close Others", crate::actions::CloseOtherTabs, |s| s.can_close_others),
                    MenuItemDef::action_with_condition("Close Tabs to Right", crate::actions::CloseTabsToRight, |s| s.can_close_right),
                    MenuItemDef::action_with_condition("Close Saved Tabs", crate::actions::CloseSavedTabs, |s| s.has_saved),
                    MenuItemDef::action_with_condition("Close All Tabs", crate::actions::CloseAllTabs, |s| s.has_tab || s.has_doc || s.has_diff),
                ],
            ),
            MenuItemDef::separator(),
            MenuItemDef::action_with_condition("Copy Path", crate::actions::CopyPath, |s| s.has_doc),
            MenuItemDef::action_with_condition("Copy File Name", crate::actions::CopyFileName, |s| s.has_doc),
            MenuItemDef::action_with_condition("Reveal in File Manager", crate::actions::RevealInExplorer, |s| s.has_doc),
            MenuItemDef::separator(),
            MenuItemDef::action("Settings...", crate::actions::OpenSettings),
            MenuItemDef::separator(),
            MenuItemDef::action("Quit", crate::actions::Quit),
        ],
    }
}

fn build_edit_menu() -> MenuDef {
    MenuDef {
        name: "Edit",
        items: vec![
            MenuItemDef::action_with_condition_and_os_action("Undo", crate::actions::Undo, |s| s.can_undo, OsAction::Undo),
            MenuItemDef::action_with_condition_and_os_action("Redo", crate::actions::Redo, |s| s.can_redo, OsAction::Redo),
            MenuItemDef::separator(),
            MenuItemDef::action_with_condition_and_os_action("Cut", crate::actions::Cut, |s| !s.is_read_only && s.has_doc && s.has_selection, OsAction::Cut),
            MenuItemDef::action_with_condition_and_os_action("Copy", crate::actions::Copy, |s| s.can_copy, OsAction::Copy),
            MenuItemDef::action_with_condition_and_os_action("Paste", crate::actions::Paste, |s| !s.is_read_only && s.has_doc, OsAction::Paste),
            MenuItemDef::action_with_condition("Fill Selection...", crate::actions::FillSelection, |s| {
                !s.is_read_only && s.has_doc && s.has_selection
            }),
            MenuItemDef::action_with_condition_and_check(
                "Toggle Insert Mode",
                crate::actions::ToggleInsertMode,
                |s| !s.is_read_only && s.has_doc,
                |s| s.is_insert_mode,
            ),
            MenuItemDef::action_with_condition_and_check(
                "Toggle Read-only",
                crate::actions::ToggleReadOnly,
                |s| s.has_doc,
                |s| s.has_doc && s.is_read_only,
            ),
            MenuItemDef::separator(),
            MenuItemDef::submenu(
                "Copy As",
                vec![
                    MenuItemDef::action_with_condition("as Hex Dump", crate::actions::CopyAsHexDump, |s| s.can_copy),
                    MenuItemDef::action_with_condition("as Hex with Spaces", crate::actions::CopyAsHexSpaces, |s| s.can_copy),
                    MenuItemDef::action_with_condition("as Hex Stream", crate::actions::CopyAsHexStream, |s| s.can_copy),
                    MenuItemDef::action_with_condition("as Printable Text", crate::actions::CopyAsPrintableText, |s| s.can_copy),
                    MenuItemDef::action_with_condition("as Escaped String", crate::actions::CopyAsEscapedString, |s| s.can_copy),
                    MenuItemDef::action_with_condition("as Base64", crate::actions::CopyAsBase64, |s| s.can_copy),
                    MenuItemDef::action_with_condition("as Binary", crate::actions::CopyAsBinary, |s| s.can_copy),
                    MenuItemDef::action_with_condition("as C++ Array", crate::actions::CopyAsCppArray, |s| s.can_copy),
                    MenuItemDef::action_with_condition("as Rust Array", crate::actions::CopyAsRustArray, |s| s.can_copy),
                    MenuItemDef::action_with_condition("as JSON Array", crate::actions::CopyAsJsonArray, |s| s.can_copy),
                ],
            ),
            MenuItemDef::action_with_condition_and_os_action("Select All", crate::actions::SelectAll, |s| s.has_doc, OsAction::SelectAll),
            MenuItemDef::separator(),
            MenuItemDef::action_with_condition("Find", crate::actions::ToggleSearch, |s| s.has_doc),
            MenuItemDef::action_with_condition("Find in File (Scan All)", crate::actions::ToggleSearchPanel, |s| s.has_doc),
            MenuItemDef::action_with_condition("Find Next", crate::actions::SearchNext, |s| s.has_doc),
            MenuItemDef::action_with_condition("Find Previous", crate::actions::SearchPrev, |s| s.has_doc),
            MenuItemDef::separator(),
            MenuItemDef::submenu(
                "Bookmark",
                vec![
                    MenuItemDef::action_with_condition("Red", crate::actions::BookmarkRed, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Orange", crate::actions::BookmarkOrange, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Yellow", crate::actions::BookmarkYellow, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Green", crate::actions::BookmarkGreen, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Cyan", crate::actions::BookmarkCyan, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Blue", crate::actions::BookmarkBlue, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Purple", crate::actions::BookmarkPurple, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Pink", crate::actions::BookmarkPink, |s| s.has_doc),
                    MenuItemDef::separator(),
                    MenuItemDef::action_with_condition("Clear Bookmark", crate::actions::ClearBookmark, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Clear All Bookmarks", crate::actions::ClearAllBookmarks, |s| s.has_doc),
                    MenuItemDef::separator(),
                    MenuItemDef::action_with_condition("Import Bookmarks...", crate::actions::ImportBookmarks, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Export Bookmarks...", crate::actions::ExportBookmarks, |s| s.has_doc),
                ],
            ),
            MenuItemDef::submenu(
                "Bookmark Visibility",
                vec![
                    MenuItemDef::action_with_condition("Show All Bookmarks", crate::actions::ShowAllBookmarks, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Hide All Bookmarks", crate::actions::HideAllBookmarks, |s| s.has_doc),
                    MenuItemDef::separator(),
                    MenuItemDef::action_with_condition_and_check(
                        "Show Only Bookmarked Regions",
                        crate::actions::ToggleHideUnbookmarked,
                        |s| s.has_doc,
                        |s| s.has_doc && s.hide_unbookmarked,
                    ),
                    MenuItemDef::separator(),
                    MenuItemDef::action_with_condition("Unfold at Cursor", crate::actions::UnfoldBookmarkAtCursor, |s| s.has_doc),
                    MenuItemDef::separator(),
                    MenuItemDef::submenu(
                        "Toggle by Color",
                        vec![
                            MenuItemDef::action_with_condition("Red", crate::actions::ToggleBookmarkRed, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Orange", crate::actions::ToggleBookmarkOrange, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Yellow", crate::actions::ToggleBookmarkYellow, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Green", crate::actions::ToggleBookmarkGreen, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Cyan", crate::actions::ToggleBookmarkCyan, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Blue", crate::actions::ToggleBookmarkBlue, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Purple", crate::actions::ToggleBookmarkPurple, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Pink", crate::actions::ToggleBookmarkPink, |s| s.has_doc),
                        ],
                    ),
                    MenuItemDef::submenu(
                        "Show Only Color",
                        vec![
                            MenuItemDef::action_with_condition("Only Red", crate::actions::ShowOnlyBookmarkRed, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Only Orange", crate::actions::ShowOnlyBookmarkOrange, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Only Yellow", crate::actions::ShowOnlyBookmarkYellow, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Only Green", crate::actions::ShowOnlyBookmarkGreen, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Only Cyan", crate::actions::ShowOnlyBookmarkCyan, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Only Blue", crate::actions::ShowOnlyBookmarkBlue, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Only Purple", crate::actions::ShowOnlyBookmarkPurple, |s| s.has_doc),
                            MenuItemDef::action_with_condition("Only Pink", crate::actions::ShowOnlyBookmarkPink, |s| s.has_doc),
                        ],
                    ),
                ],
            ),
        ],
    }
}

fn build_view_menu() -> MenuDef {
    let mut encoding_items = Vec::new();

    // Primary encodings (ASCII, UTF-8, UTF-16LE, UTF-16BE) directly at top level
    for &encoding in Encoding::primary_encodings() {
        encoding_items.push(MenuItemDef::action_with_condition_and_check(
            encoding.label(),
            crate::actions::SetEncoding { encoding },
            |s| s.has_doc || s.has_diff,
            move |s| s.encoding == Some(encoding),
        ));
    }

    encoding_items.push(MenuItemDef::separator());

    // Regional/legacy categories as submenus
    for (category, encodings) in Encoding::secondary_categories() {
        let cat_items = encodings
            .iter()
            .copied()
            .map(|encoding| {
                MenuItemDef::action_with_condition_and_check(
                    encoding.label(),
                    crate::actions::SetEncoding { encoding },
                    |s| s.has_doc || s.has_diff,
                    move |s| s.encoding == Some(encoding),
                )
            })
            .collect();
        encoding_items.push(MenuItemDef::submenu(category.label(), cat_items));
    }

    MenuDef {
        name: "View",
        items: vec![
            MenuItemDef::action("Command Palette...", crate::actions::CommandPalette),
            MenuItemDef::separator(),
            MenuItemDef::action("Toggle Left Panel", crate::actions::ToggleLeftPanel),
            MenuItemDef::action("Toggle Right Panel (Scratchpad)", crate::actions::ToggleRightPanel),
            MenuItemDef::action_with_condition("Focus Active Editor", crate::actions::FocusHexView, |s| s.has_doc || s.has_scratch),
            MenuItemDef::submenu(
                "Panels",
                vec![
                    MenuItemDef::action("Files", crate::actions::ShowFilesTab),
                    MenuItemDef::action("Search", crate::actions::ShowSearchTab),
                    MenuItemDef::action("Strings", crate::actions::ShowStringsTab),
                    MenuItemDef::action("Zlib", crate::actions::ShowZlibTab),
                    MenuItemDef::action("Structure", crate::actions::ShowStructureTab),
                    MenuItemDef::action("Inspector", crate::actions::ShowInspectorTab),
                    MenuItemDef::action("2D Visual Map", crate::actions::OpenVisualMap),
                    MenuItemDef::action("Checksum", crate::actions::ShowChecksumTab),
                    MenuItemDef::action("Bookmarks", crate::actions::ShowBookmarksTab),
                ],
            ),
            MenuItemDef::separator(),
            MenuItemDef::submenu(
                "Radix",
                vec![
                    MenuItemDef::action_with_condition_and_check(
                        "Hexadecimal (16)",
                        crate::actions::SetRadixHex,
                        |s| s.has_doc,
                        |s| s.radix == Some(DisplayRadix::Hexadecimal),
                    ),
                    MenuItemDef::action_with_condition_and_check(
                        "Decimal (10)",
                        crate::actions::SetRadixDec,
                        |s| s.has_doc,
                        |s| s.radix == Some(DisplayRadix::Decimal),
                    ),
                    MenuItemDef::action_with_condition_and_check(
                        "Octal (8)",
                        crate::actions::SetRadixOct,
                        |s| s.has_doc,
                        |s| s.radix == Some(DisplayRadix::Octal),
                    ),
                    MenuItemDef::action_with_condition_and_check(
                        "Binary (2)",
                        crate::actions::SetRadixBin,
                        |s| s.has_doc,
                        |s| s.radix == Some(DisplayRadix::Binary),
                    ),
                ],
            ),
            MenuItemDef::submenu(
                "Grouping",
                vec![
                    MenuItemDef::action_with_condition_and_check(
                        "1 Byte (8-bit)",
                        crate::actions::SetGroupSize1,
                        |s| s.has_doc,
                        |s| s.group_size == Some(ByteGroupSize::One),
                    ),
                    MenuItemDef::action_with_condition_and_check(
                        "2 Bytes (16-bit)",
                        crate::actions::SetGroupSize2,
                        |s| s.has_doc,
                        |s| s.group_size == Some(ByteGroupSize::Two),
                    ),
                    MenuItemDef::action_with_condition_and_check(
                        "4 Bytes (32-bit)",
                        crate::actions::SetGroupSize4,
                        |s| s.has_doc,
                        |s| s.group_size == Some(ByteGroupSize::Four),
                    ),
                    MenuItemDef::action_with_condition_and_check(
                        "8 Bytes (64-bit)",
                        crate::actions::SetGroupSize8,
                        |s| s.has_doc,
                        |s| s.group_size == Some(ByteGroupSize::Eight),
                    ),
                ],
            ),
            MenuItemDef::submenu(
                "Byte Order",
                vec![
                    MenuItemDef::action_with_condition_and_check(
                        "Little Endian",
                        crate::actions::SetByteOrderLittleEndian,
                        |s| s.has_doc,
                        |s| s.byte_order == Some(ByteOrder::LittleEndian),
                    ),
                    MenuItemDef::action_with_condition_and_check(
                        "Big Endian",
                        crate::actions::SetByteOrderBigEndian,
                        |s| s.has_doc,
                        |s| s.byte_order == Some(ByteOrder::BigEndian),
                    ),
                    MenuItemDef::separator(),
                    MenuItemDef::action_with_condition("Toggle Byte Order", crate::actions::ToggleByteOrder, |s| s.has_doc),
                ],
            ),
            MenuItemDef::submenu("Encoding", encoding_items),
            MenuItemDef::separator(),
            MenuItemDef::submenu(
                "Custom Line Breaks",
                vec![
                    MenuItemDef::action_with_condition("Break Line", crate::actions::AddCustomBreak, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Join Lines", crate::actions::JoinLine, |s| s.has_doc),
                    MenuItemDef::separator(),
                    MenuItemDef::action_with_condition("Remove Break Backward", crate::actions::RemoveCustomBreakBackward, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Remove Break Forward", crate::actions::RemoveCustomBreakForward, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Reset Breaks in Selection", crate::actions::RemoveCustomBreaksInRange, |s| s.has_doc),
                    MenuItemDef::separator(),
                    MenuItemDef::action_with_condition("Reset Custom Breaks", crate::actions::ClearAllCustomBreaks, |s| s.has_doc),
                ],
            ),
        ],
    }
}

fn build_go_menu() -> MenuDef {
    MenuDef {
        name: "Go",
        items: vec![
            MenuItemDef::action_with_condition("Go to Address...", crate::actions::ToggleGoToAddress, |s| s.has_doc),
            MenuItemDef::separator(),
            MenuItemDef::action_with_condition("Go to Beginning", crate::actions::GoToBeginning, |s| s.has_doc),
            MenuItemDef::action_with_condition("Go to End", crate::actions::GoToEnd, |s| s.has_doc),
            MenuItemDef::separator(),
            MenuItemDef::action_with_condition("Next Difference", crate::actions::NextDifference, |s| s.has_diff),
            MenuItemDef::action_with_condition("Previous Difference", crate::actions::PrevDifference, |s| s.has_diff),
        ],
    }
}

fn build_analysis_menu() -> MenuDef {
    MenuDef {
        name: "Analysis",
        items: vec![
            MenuItemDef::submenu(
                "Structure (Kaitai Struct)",
                vec![
                    MenuItemDef::action_with_condition("Load Definition...", crate::actions::LoadStructureDefinition, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Clear Definition", crate::actions::ClearStructureDefinition, |s| s.has_doc),
                    MenuItemDef::separator(),
                    MenuItemDef::action_with_condition_and_check(
                        "Toggle Inline Structure View",
                        crate::actions::ToggleInlineStructureView,
                        |s| s.has_doc,
                        |s| s.has_doc && s.inline_structure_view,
                    ),
                    MenuItemDef::separator(),
                    MenuItemDef::action_with_condition("Expand All", crate::actions::ExpandAllStructure, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Collapse All", crate::actions::CollapseAllStructure, |s| s.has_doc),
                ],
            ),
            MenuItemDef::separator(),
            MenuItemDef::action("2D Visual Map", crate::actions::OpenVisualMap),
            MenuItemDef::action("Checksum Calculation", crate::actions::ShowChecksumTab),
            MenuItemDef::separator(),
            MenuItemDef::submenu(
                "Compare / Diff",
                vec![
                    MenuItemDef::action("Compare Open Files...", crate::actions::CompareOpenFiles),
                    MenuItemDef::action("Compare Visible Split Panes", crate::actions::CompareVisiblePanes),
                    MenuItemDef::separator(),
                    MenuItemDef::action_with_condition("Swap Diff Files", crate::actions::SwapDiffFiles, |s| s.has_diff),
                    MenuItemDef::action_with_condition("Refresh Diff", crate::actions::RefreshDiff, |s| s.has_diff),
                    MenuItemDef::separator(),
                    MenuItemDef::action_with_condition("Next Difference", crate::actions::NextDifference, |s| s.has_diff),
                    MenuItemDef::action_with_condition("Previous Difference", crate::actions::PrevDifference, |s| s.has_diff),
                ],
            ),
        ],
    }
}

fn build_scratch_menu() -> MenuDef {
    let recent_scratches = crate::service::ScratchService::list_scratches();
    let mut recent_items = Vec::new();
    if recent_scratches.is_empty() {
        recent_items.push(MenuItemDef::action_with_condition("No Recent Scratches", crate::actions::NewScratchpad, |_| {
            false
        }));
    } else {
        for entry in recent_scratches.into_iter().take(15) {
            let label = if entry.title != entry.filename {
                format!("{} ({})", entry.title, entry.filename)
            } else {
                entry.title
            };
            recent_items.push(MenuItemDef::action(label, crate::actions::OpenScratchpadFile { path: entry.path }));
        }
    }

    MenuDef {
        name: "Scratch",
        items: vec![
            MenuItemDef::action("Toggle Scratchpad Panel", crate::actions::ToggleRightPanel),
            MenuItemDef::separator(),
            MenuItemDef::action("New Scratchpad", crate::actions::NewScratchpad),
            MenuItemDef::action("Open Scratchpad...", crate::actions::OpenScratchpadDialog),
            MenuItemDef::action_with_condition("Switch Note Palette...", crate::actions::QuickOpen, |s| s.has_scratch),
            MenuItemDef::submenu("Recent Scratches", recent_items),
            MenuItemDef::separator(),
            MenuItemDef::action_with_condition("Toggle View Mode", crate::actions::ToggleScratchpadMode, |s| s.has_scratch),
            MenuItemDef::separator(),
            MenuItemDef::action("Reveal in File Manager", crate::actions::RevealScratchesInExplorer),
            MenuItemDef::separator(),
            MenuItemDef::submenu(
                "Insert into Scratchpad",
                vec![
                    MenuItemDef::action_with_condition("as Offset", crate::actions::InsertActiveOffsetOnly, |s| s.has_scratch),
                    MenuItemDef::action_with_condition("as Hex Bytes", crate::actions::InsertActiveHexBytes, |s| s.has_scratch),
                    MenuItemDef::action_with_condition("as Text / String", crate::actions::InsertActiveText, |s| s.has_scratch),
                ],
            ),
            MenuItemDef::action_with_condition("Export Scratchpad As...", crate::actions::ExportScratchpadAs, |s| s.has_scratch),
            MenuItemDef::action_with_condition("Delete Current Scratchpad", crate::actions::DeleteCurrentScratchpad, |s| s.has_scratch),
        ],
    }
}

fn build_window_menu() -> MenuDef {
    MenuDef {
        name: "Window",
        items: vec![
            MenuItemDef::action("Split Right", crate::actions::SplitRight),
            MenuItemDef::action("Split Down", crate::actions::SplitDown),
            MenuItemDef::separator(),
            MenuItemDef::action("Next Tab", crate::actions::ActivateNextTab),
            MenuItemDef::action("Previous Tab", crate::actions::ActivatePreviousTab),
        ],
    }
}

fn build_help_menu() -> MenuDef {
    MenuDef {
        name: "Help",
        items: vec![MenuItemDef::action("About xvw", crate::actions::OpenAbout)],
    }
}

#[cfg(test)]
mod tests {
    use super::{MenuEditorState, MenuItemDef, application_menus};

    #[test]
    fn test_application_menus_structure() {
        let menus = application_menus();
        assert_eq!(menus.len(), 8);
        assert_eq!(menus[0].name, "File");
        assert_eq!(menus[1].name, "Edit");
        assert_eq!(menus[2].name, "View");
        assert_eq!(menus[3].name, "Go");
        assert_eq!(menus[4].name, "Analysis");
        assert_eq!(menus[5].name, "Scratch");
        assert_eq!(menus[6].name, "Window");
        assert_eq!(menus[7].name, "Help");
    }

    #[test]
    fn test_to_gpui_menu_conversion() {
        let menus = application_menus();
        let state = MenuEditorState::default();
        for menu in &menus {
            let gpui_menu = menu.to_gpui_menu(&state);
            assert_eq!(gpui_menu.name.as_ref(), menu.name);
            assert!(!gpui_menu.items.is_empty());
        }
    }

    #[test]
    fn test_scratch_menu_structure() {
        let menus = application_menus();
        let scratch_menu = menus.iter().find(|m| m.name == "Scratch").expect("Scratch menu found");
        let has_new = scratch_menu
            .items
            .iter()
            .any(|item| matches!(item, MenuItemDef::Action { label, .. } if label.as_ref() == "New Scratchpad"));
        let has_open = scratch_menu
            .items
            .iter()
            .any(|item| matches!(item, MenuItemDef::Action { label, .. } if label.as_ref() == "Open Scratchpad..."));
        let has_recent = scratch_menu
            .items
            .iter()
            .any(|item| matches!(item, MenuItemDef::Submenu { label, .. } if label.as_ref() == "Recent Scratches"));
        assert!(has_new);
        assert!(has_open);
        assert!(has_recent);

        let insert_submenu = scratch_menu
            .items
            .iter()
            .find_map(|item| match item {
                MenuItemDef::Submenu { label, items } if label.as_ref() == "Insert into Scratchpad" => Some(items),
                _ => None,
            })
            .expect("Insert into Scratchpad submenu found");
        assert_eq!(insert_submenu.len(), 3);
        let offset_item = insert_submenu
            .iter()
            .find(|item| matches!(item, MenuItemDef::Action { label, .. } if label.as_ref() == "as Offset"))
            .expect("as Offset item found");
        if let MenuItemDef::Action { is_enabled: Some(cond), .. } = offset_item {
            assert!(cond(&MenuEditorState {
                has_scratch: true,
                ..Default::default()
            }));
            assert!(!cond(&MenuEditorState {
                has_scratch: false,
                ..Default::default()
            }));
        } else {
            panic!("as Offset item must have condition");
        }

        let export_item = scratch_menu
            .items
            .iter()
            .find(|item| matches!(item, MenuItemDef::Action { label, .. } if label.as_ref() == "Export Scratchpad As..."))
            .expect("Export item found");
        if let MenuItemDef::Action { is_enabled: Some(cond), .. } = export_item {
            assert!(cond(&MenuEditorState {
                has_scratch: true,
                ..Default::default()
            }));
            assert!(!cond(&MenuEditorState {
                has_scratch: false,
                ..Default::default()
            }));
        } else {
            panic!("Export item must have condition");
        }

        let delete_item = scratch_menu
            .items
            .iter()
            .find(|item| matches!(item, MenuItemDef::Action { label, .. } if label.as_ref() == "Delete Current Scratchpad"))
            .expect("Delete item found");
        if let MenuItemDef::Action { is_enabled: Some(cond), .. } = delete_item {
            assert!(cond(&MenuEditorState {
                has_scratch: true,
                ..Default::default()
            }));
            assert!(!cond(&MenuEditorState {
                has_scratch: false,
                ..Default::default()
            }));
        } else {
            panic!("Delete item must have condition");
        }
    }

    #[test]
    fn test_view_encoding_menu_structure() {
        let menus = application_menus();
        let view_menu = menus.iter().find(|m| m.name == "View").expect("View menu found");
        let encoding_submenu = view_menu
            .items
            .iter()
            .find_map(|item| match item {
                MenuItemDef::Submenu { label, items } if label.as_ref() == "Encoding" => Some(items),
                _ => None,
            })
            .expect("Encoding submenu found");

        // The first 4 items must be top-level action items for ASCII & Unicode
        assert_eq!(encoding_submenu.len(), 4 + 1 + 5); // 4 primary + 1 separator + 5 categories
        let primary_labels: Vec<&str> = encoding_submenu[..4]
            .iter()
            .map(|item| match item {
                MenuItemDef::Action { label, .. } => label.as_ref(),
                _ => panic!("Expected action item"),
            })
            .collect();
        assert_eq!(primary_labels, vec!["ASCII", "UTF-8", "UTF-16 LE", "UTF-16 BE"]);

        // Followed by a separator
        assert!(matches!(encoding_submenu[4], MenuItemDef::Separator));

        // Followed by regional/legacy submenus
        let category_labels: Vec<&str> = encoding_submenu[5..]
            .iter()
            .map(|item| match item {
                MenuItemDef::Submenu { label, .. } => label.as_ref(),
                _ => panic!("Expected submenu"),
            })
            .collect();
        assert_eq!(
            category_labels,
            vec!["Japanese", "Chinese & Korean", "ISO-8859", "Windows Code Pages", "Legacy / DOS / Mac"]
        );
    }

    #[test]
    fn test_view_encoding_enabled_for_doc_or_diff() {
        let menus = application_menus();
        let view_menu = menus.iter().find(|m| m.name == "View").expect("View menu found");
        let encoding_submenu = view_menu
            .items
            .iter()
            .find_map(|item| match item {
                MenuItemDef::Submenu { label, items } if label.as_ref() == "Encoding" => Some(items),
                _ => None,
            })
            .expect("Encoding submenu found");

        let doc_only = MenuEditorState {
            has_doc: true,
            ..Default::default()
        };
        let diff_only = MenuEditorState {
            has_diff: true,
            ..Default::default()
        };
        let neither = MenuEditorState::default();

        for item in &encoding_submenu[..4] {
            let MenuItemDef::Action { is_enabled: Some(cond), .. } = item else {
                panic!("Expected conditional primary encoding action");
            };
            assert!(cond(&doc_only));
            assert!(cond(&diff_only));
            assert!(!cond(&neither));
        }

        for category in &encoding_submenu[5..] {
            let MenuItemDef::Submenu { items, .. } = category else {
                panic!("Expected encoding category submenu");
            };
            assert!(!items.is_empty());
            for item in items {
                let MenuItemDef::Action { is_enabled: Some(cond), .. } = item else {
                    panic!("Expected conditional secondary encoding action");
                };
                assert!(cond(&doc_only));
                assert!(cond(&diff_only));
                assert!(!cond(&neither));
            }
        }

        let commands = super::collect_palette_commands(&menus);
        let utf8_cmd = commands.iter().find(|c| c.title == "View: Encoding > UTF-8").expect("UTF-8 command in palette");
        let sjis_cmd = commands
            .iter()
            .find(|c| c.title == "View: Encoding > Japanese > Shift-JIS")
            .expect("Shift-JIS command in palette");
        assert!(utf8_cmd.is_executable(&diff_only));
        assert!(sjis_cmd.is_executable(&diff_only));
        assert!(!utf8_cmd.is_executable(&neither));
        assert!(!sjis_cmd.is_executable(&neither));
    }

    #[test]
    fn test_edit_menu_copy_conditions() {
        let menus = application_menus();
        let edit_menu = menus.iter().find(|m| m.name == "Edit").expect("Edit menu found");

        let copy_item = edit_menu
            .items
            .iter()
            .find(|item| matches!(item, MenuItemDef::Action { label, .. } if label.as_ref() == "Copy"))
            .expect("Copy item found");

        let is_enabled = match copy_item {
            MenuItemDef::Action { is_enabled: Some(f), .. } => *f,
            _ => panic!("Expected action with is_enabled"),
        };

        let state_can_copy_no_sel = MenuEditorState {
            can_copy: true,
            has_selection: false,
            ..Default::default()
        };
        assert!(is_enabled(&state_can_copy_no_sel));

        let state_cannot_copy = MenuEditorState {
            can_copy: false,
            has_selection: false,
            ..Default::default()
        };
        assert!(!is_enabled(&state_cannot_copy));

        let copy_as_submenu = edit_menu
            .items
            .iter()
            .find_map(|item| match item {
                MenuItemDef::Submenu { label, items } if label.as_ref() == "Copy As" => Some(items),
                _ => None,
            })
            .expect("Copy As submenu found");

        assert_eq!(copy_as_submenu.len(), 10);
        for item in copy_as_submenu {
            match item {
                MenuItemDef::Action { is_enabled: Some(f), .. } => {
                    assert!(f(&state_can_copy_no_sel));
                    assert!(!f(&state_cannot_copy));
                }
                _ => panic!("Expected action item with is_enabled in Copy As"),
            }
        }
    }

    #[test]
    fn test_scratch_menu_recent_label_formatting() {
        use crate::service::scratch_service::ScratchEntry;
        use std::path::PathBuf;

        let entry_same_title = ScratchEntry {
            id: Some(1),
            filename: "scratch_1.md".to_string(),
            path: PathBuf::from("scratch_1.md"),
            title: "Scratchpad".to_string(),
            modified: None,
            target_file: None,
        };
        let label1 = if entry_same_title.title != entry_same_title.filename {
            format!("{} ({})", entry_same_title.title, entry_same_title.filename)
        } else {
            entry_same_title.title
        };
        assert_eq!(label1, "Scratchpad (scratch_1.md)");

        let entry_custom = ScratchEntry {
            id: Some(2),
            filename: "scratch_2.md".to_string(),
            path: PathBuf::from("scratch_2.md"),
            title: "My Custom Notes".to_string(),
            modified: None,
            target_file: None,
        };
        let label2 = if entry_custom.title != entry_custom.filename {
            format!("{} ({})", entry_custom.title, entry_custom.filename)
        } else {
            entry_custom.title
        };
        assert_eq!(label2, "My Custom Notes (scratch_2.md)");
    }

    #[test]
    fn test_collect_palette_commands() {
        let menus = application_menus();
        let commands = super::collect_palette_commands(&menus);
        assert!(!commands.is_empty());

        // Check presence of key commands
        let save_cmd = commands.iter().find(|c| c.title == "File: Save");
        assert!(save_cmd.is_some());
        let save_cmd = save_cmd.unwrap();
        assert_eq!(save_cmd.category.as_ref(), "File");
        assert_eq!(save_cmd.label.as_ref(), "Save");
        #[cfg(target_os = "macos")]
        assert_eq!(save_cmd.shortcut.as_deref(), Some("⌘S"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(save_cmd.shortcut.as_deref(), Some("Ctrl+S"));

        // Check submenu flattening
        let copy_hex = commands.iter().find(|c| c.title == "Edit: Copy As > as Hex Dump");
        assert!(copy_hex.is_some());
        let copy_hex = copy_hex.unwrap();
        #[cfg(target_os = "macos")]
        assert_eq!(copy_hex.shortcut.as_deref(), Some("⇧⌘C"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(copy_hex.shortcut.as_deref(), Some("Ctrl+Shift+C"));

        let insert_mode = commands.iter().find(|c| c.title == "Edit: Toggle Insert Mode").expect("Toggle Insert Mode");
        #[cfg(target_os = "macos")]
        assert_eq!(insert_mode.shortcut.as_deref(), Some("⌥⌘I"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(insert_mode.shortcut.as_deref(), Some("Insert"));

        let find_next = commands.iter().find(|c| c.title == "Edit: Find Next").expect("Find Next");
        #[cfg(target_os = "macos")]
        assert_eq!(find_next.shortcut.as_deref(), Some("⌘G"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(find_next.shortcut.as_deref(), Some("F3"));

        let find_prev = commands.iter().find(|c| c.title == "Edit: Find Previous").expect("Find Previous");
        #[cfg(target_os = "macos")]
        assert_eq!(find_prev.shortcut.as_deref(), Some("⇧⌘G"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(find_prev.shortcut.as_deref(), Some("Shift+F3"));

        let unfold_bm = commands
            .iter()
            .find(|c| c.title == "Edit: Bookmark Visibility > Unfold at Cursor")
            .expect("Unfold at Cursor");
        #[cfg(target_os = "macos")]
        assert_eq!(unfold_bm.shortcut.as_deref(), Some("⇧⌘]"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(unfold_bm.shortcut.as_deref(), Some("Ctrl+Shift+]"));

        let break_line = commands
            .iter()
            .find(|c| c.title == "View: Custom Line Breaks > Break Line")
            .expect("Break Line");
        assert_eq!(break_line.shortcut.as_deref(), Some("Enter"));

        let join_lines = commands
            .iter()
            .find(|c| c.title == "View: Custom Line Breaks > Join Lines")
            .expect("Join Lines");
        #[cfg(target_os = "macos")]
        assert_eq!(join_lines.shortcut.as_deref(), Some("⌘J"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(join_lines.shortcut.as_deref(), Some("Ctrl+J"));

        let reset_breaks = commands
            .iter()
            .find(|c| c.title == "View: Custom Line Breaks > Reset Custom Breaks")
            .expect("Reset Custom Breaks");
        #[cfg(target_os = "macos")]
        assert_eq!(reset_breaks.shortcut.as_deref(), Some("⇧⌘Backspace"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(reset_breaks.shortcut.as_deref(), Some("Ctrl+Shift+Backspace"));

        let reset_range = commands
            .iter()
            .find(|c| c.title == "View: Custom Line Breaks > Reset Breaks in Selection")
            .expect("Reset Breaks in Selection");
        #[cfg(target_os = "macos")]
        assert_eq!(reset_range.shortcut.as_deref(), Some("⇧Enter"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(reset_range.shortcut.as_deref(), Some("Shift+Enter"));

        let go_addr = commands.iter().find(|c| c.title == "Go: Go to Address...").expect("Go to Address...");
        #[cfg(target_os = "macos")]
        assert_eq!(go_addr.shortcut.as_deref(), Some("⌘L"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(go_addr.shortcut.as_deref(), Some("Ctrl+L"));

        let go_begin = commands.iter().find(|c| c.title == "Go: Go to Beginning").expect("Go to Beginning");
        #[cfg(target_os = "macos")]
        assert_eq!(go_begin.shortcut.as_deref(), Some("⌘Home"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(go_begin.shortcut.as_deref(), Some("Ctrl+Home"));

        let go_end = commands.iter().find(|c| c.title == "Go: Go to End").expect("Go to End");
        #[cfg(target_os = "macos")]
        assert_eq!(go_end.shortcut.as_deref(), Some("⌘End"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(go_end.shortcut.as_deref(), Some("Ctrl+End"));

        let next_diff = commands.iter().find(|c| c.title == "Go: Next Difference").expect("Next Difference");
        assert_eq!(next_diff.shortcut.as_deref(), Some("F3"));

        let prev_diff = commands.iter().find(|c| c.title == "Go: Previous Difference").expect("Previous Difference");
        #[cfg(target_os = "macos")]
        assert_eq!(prev_diff.shortcut.as_deref(), Some("⇧F3"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(prev_diff.shortcut.as_deref(), Some("Shift+F3"));

        let diff_next = commands
            .iter()
            .find(|c| c.title == "Analysis: Compare / Diff > Next Difference")
            .expect("Analysis Next Difference");
        assert_eq!(diff_next.shortcut.as_deref(), Some("F3"));

        let diff_prev = commands
            .iter()
            .find(|c| c.title == "Analysis: Compare / Diff > Previous Difference")
            .expect("Analysis Previous Difference");
        #[cfg(target_os = "macos")]
        assert_eq!(diff_prev.shortcut.as_deref(), Some("⇧F3"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(diff_prev.shortcut.as_deref(), Some("Shift+F3"));

        // Check that all 9 left panels are available in commands
        let panel_commands = [
            "View: Panels > Files",
            "View: Panels > Search",
            "View: Panels > Strings",
            "View: Panels > Zlib",
            "View: Panels > Structure",
            "View: Panels > Inspector",
            "View: Panels > 2D Visual Map",
            "View: Panels > Checksum",
            "View: Panels > Bookmarks",
        ];
        for title in panel_commands {
            assert!(commands.iter().any(|c| c.title == title), "missing panel command: {title}");
        }

        // Check condition evaluation
        let state_ro = MenuEditorState {
            is_read_only: true,
            has_doc: true,
            ..Default::default()
        };
        assert!(!save_cmd.is_executable(&state_ro));

        let state_writable = MenuEditorState {
            is_read_only: false,
            has_doc: true,
            ..Default::default()
        };
        assert!(save_cmd.is_executable(&state_writable));
    }

    #[test]
    fn test_focus_active_editor_menu_item() {
        use super::collect_palette_commands;

        let menus = application_menus();
        let view_menu = menus.iter().find(|m| m.name == "View").expect("View menu found");
        let focus_item = view_menu
            .items
            .iter()
            .find(|item| matches!(item, MenuItemDef::Action { label, .. } if label.as_ref() == "Focus Active Editor"))
            .expect("Focus Active Editor item found");

        if let MenuItemDef::Action { is_enabled: Some(cond), .. } = focus_item {
            assert!(cond(&MenuEditorState {
                has_doc: true,
                ..Default::default()
            }));
            assert!(cond(&MenuEditorState {
                has_scratch: true,
                ..Default::default()
            }));
            assert!(!cond(&MenuEditorState {
                has_doc: false,
                has_scratch: false,
                ..Default::default()
            }));
        } else {
            panic!("Focus Active Editor item must have condition");
        }

        let commands = collect_palette_commands(&menus);
        let focus_cmd = commands
            .iter()
            .find(|c| c.title == "View: Focus Active Editor")
            .expect("Focus Active Editor in palette");
        assert_eq!(focus_cmd.shortcut.as_deref(), Some("Esc"));
    }

    #[test]
    fn test_shortcut_for_label_alignment() {
        use super::shortcut_for_label;

        // Next Difference and Previous Difference
        assert_eq!(shortcut_for_label("Go", "Next Difference"), Some("F3"));
        assert_eq!(shortcut_for_label("Analysis", "Next Difference"), Some("F3"));
        #[cfg(target_os = "macos")]
        {
            assert_eq!(shortcut_for_label("Go", "Previous Difference"), Some("⇧F3"));
            assert_eq!(shortcut_for_label("Analysis", "Previous Difference"), Some("⇧F3"));
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(shortcut_for_label("Go", "Previous Difference"), Some("Shift+F3"));
            assert_eq!(shortcut_for_label("Analysis", "Previous Difference"), Some("Shift+F3"));
        }

        // Find Next / Find Previous
        #[cfg(target_os = "macos")]
        {
            assert_eq!(shortcut_for_label("Edit", "Find Next"), Some("⌘G"));
            assert_eq!(shortcut_for_label("Go", "Find Next"), Some("⌘G"));
            assert_eq!(shortcut_for_label("Edit", "Find Previous"), Some("⇧⌘G"));
            assert_eq!(shortcut_for_label("Go", "Find Previous"), Some("⇧⌘G"));
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(shortcut_for_label("Edit", "Find Next"), Some("F3"));
            assert_eq!(shortcut_for_label("Go", "Find Next"), Some("F3"));
            assert_eq!(shortcut_for_label("Edit", "Find Previous"), Some("Shift+F3"));
            assert_eq!(shortcut_for_label("Go", "Find Previous"), Some("Shift+F3"));
        }

        // Toggle Insert Mode
        #[cfg(target_os = "macos")]
        assert_eq!(shortcut_for_label("Edit", "Toggle Insert Mode"), Some("⌥⌘I"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(shortcut_for_label("Edit", "Toggle Insert Mode"), Some("Insert"));

        // Save As, Settings, Load Definition, Scratchpad shortcuts
        #[cfg(target_os = "macos")]
        {
            assert_eq!(shortcut_for_label("File", "Save As..."), Some("⇧⌘S"));
            assert_eq!(shortcut_for_label("File", "Settings..."), Some("⌘,"));
            assert_eq!(shortcut_for_label("Analysis", "Load Definition..."), Some("⇧⌘K"));
            assert_eq!(shortcut_for_label("Scratch", "Toggle Scratchpad Panel"), Some("⌥⌘B"));
            assert_eq!(shortcut_for_label("Scratch", "Switch Note Palette..."), Some("⌘P"));
            assert_eq!(shortcut_for_label("Scratch", "Toggle View Mode"), Some("⌘E"));
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(shortcut_for_label("File", "Save As..."), Some("Ctrl+Shift+S"));
            assert_eq!(shortcut_for_label("File", "Settings..."), Some("Ctrl+,"));
            assert_eq!(shortcut_for_label("Analysis", "Load Definition..."), Some("Ctrl+Shift+K"));
            assert_eq!(shortcut_for_label("Scratch", "Toggle Scratchpad Panel"), Some("Alt+Ctrl+B"));
            assert_eq!(shortcut_for_label("Scratch", "Switch Note Palette..."), Some("Ctrl+P"));
            assert_eq!(shortcut_for_label("Scratch", "Toggle View Mode"), Some("Ctrl+E"));
        }

        // Copy as Hex Dump
        #[cfg(target_os = "macos")]
        {
            assert_eq!(shortcut_for_label("Edit", "as Hex Dump"), Some("⇧⌘C"));
            assert_eq!(shortcut_for_label("Edit > Copy As", "as Hex Dump"), Some("⇧⌘C"));
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(shortcut_for_label("Edit", "as Hex Dump"), Some("Ctrl+Shift+C"));
            assert_eq!(shortcut_for_label("Edit > Copy As", "as Hex Dump"), Some("Ctrl+Shift+C"));
        }

        // Beginning / End of File
        #[cfg(target_os = "macos")]
        {
            assert_eq!(shortcut_for_label("Go", "Beginning of File"), Some("⌘Home"));
            assert_eq!(shortcut_for_label("Go", "Go to Beginning"), Some("⌘Home"));
            assert_eq!(shortcut_for_label("Go", "End of File"), Some("⌘End"));
            assert_eq!(shortcut_for_label("Go", "Go to End"), Some("⌘End"));
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(shortcut_for_label("Go", "Beginning of File"), Some("Ctrl+Home"));
            assert_eq!(shortcut_for_label("Go", "Go to Beginning"), Some("Ctrl+Home"));
            assert_eq!(shortcut_for_label("Go", "End of File"), Some("Ctrl+End"));
            assert_eq!(shortcut_for_label("Go", "Go to End"), Some("Ctrl+End"));
        }

        // Unfold Bookmark at Cursor
        #[cfg(target_os = "macos")]
        {
            assert_eq!(shortcut_for_label("Edit", "Unfold at Cursor"), Some("⇧⌘]"));
            assert_eq!(shortcut_for_label("Edit", "Unfold Bookmark at Cursor"), Some("⇧⌘]"));
            assert_eq!(shortcut_for_label("Go > Bookmark Visibility", "Unfold Bookmark at Cursor"), Some("⇧⌘]"));
            assert_eq!(shortcut_for_label("Edit > Bookmark Visibility", "Unfold Bookmark at Cursor"), Some("⇧⌘]"));
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(shortcut_for_label("Edit", "Unfold at Cursor"), Some("Ctrl+Shift+]"));
            assert_eq!(shortcut_for_label("Edit", "Unfold Bookmark at Cursor"), Some("Ctrl+Shift+]"));
            assert_eq!(
                shortcut_for_label("Go > Bookmark Visibility", "Unfold Bookmark at Cursor"),
                Some("Ctrl+Shift+]")
            );
            assert_eq!(
                shortcut_for_label("Edit > Bookmark Visibility", "Unfold Bookmark at Cursor"),
                Some("Ctrl+Shift+]")
            );
        }

        // Custom Line Breaks
        assert_eq!(shortcut_for_label("View", "Break Line"), Some("Enter"));
        assert_eq!(shortcut_for_label("View", "Add Line Break"), Some("Enter"));
        assert_eq!(shortcut_for_label("Edit > Custom Line Breaks", "Add Line Break"), Some("Enter"));
        #[cfg(target_os = "macos")]
        {
            assert_eq!(shortcut_for_label("View", "Join Lines"), Some("⌘J"));
            assert_eq!(shortcut_for_label("View", "Join Line"), Some("⌘J"));
            assert_eq!(shortcut_for_label("Edit > Custom Line Breaks", "Join Line"), Some("⌘J"));
            assert_eq!(shortcut_for_label("View", "Reset Custom Breaks"), Some("⇧⌘Backspace"));
            assert_eq!(shortcut_for_label("View", "Clear All Line Breaks"), Some("⇧⌘Backspace"));
            assert_eq!(shortcut_for_label("Edit > Custom Line Breaks", "Clear All Line Breaks"), Some("⇧⌘Backspace"));
            assert_eq!(shortcut_for_label("View", "Reset Breaks in Selection"), Some("⇧Enter"));
        }
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(shortcut_for_label("View", "Join Lines"), Some("Ctrl+J"));
            assert_eq!(shortcut_for_label("View", "Join Line"), Some("Ctrl+J"));
            assert_eq!(shortcut_for_label("Edit > Custom Line Breaks", "Join Line"), Some("Ctrl+J"));
            assert_eq!(shortcut_for_label("View", "Reset Custom Breaks"), Some("Ctrl+Shift+Backspace"));
            assert_eq!(shortcut_for_label("View", "Clear All Line Breaks"), Some("Ctrl+Shift+Backspace"));
            assert_eq!(shortcut_for_label("View", "Reset Breaks in Selection"), Some("Shift+Enter"));
            assert_eq!(
                shortcut_for_label("Edit > Custom Line Breaks", "Clear All Line Breaks"),
                Some("Ctrl+Shift+Backspace")
            );
        }
    }

    #[test]
    fn test_settings_and_about_in_palette_and_menus() {
        let menus = application_menus();
        let commands = super::collect_palette_commands(&menus);
        let state = MenuEditorState::default();

        let settings_cmd = commands.iter().find(|c| c.title == "File: Settings...").expect("Settings command in palette");
        assert!(settings_cmd.is_executable(&state));
        #[cfg(target_os = "macos")]
        assert_eq!(settings_cmd.shortcut.as_deref(), Some("⌘,"));
        #[cfg(not(target_os = "macos"))]
        assert_eq!(settings_cmd.shortcut.as_deref(), Some("Ctrl+,"));

        let about_cmd = commands.iter().find(|c| c.title == "Help: About xvw").expect("About command in palette");
        assert!(about_cmd.is_executable(&state));
    }

    #[test]
    fn test_diff_and_close_tab_menu_conditions() {
        let menus = application_menus();
        let commands = super::collect_palette_commands(&menus);

        let empty = MenuEditorState::default();
        let doc_only = MenuEditorState {
            has_doc: true,
            has_tab: true,
            ..Default::default()
        };
        let diff_only = MenuEditorState {
            has_diff: true,
            has_tab: true,
            ..Default::default()
        };
        let settings_tab_only = MenuEditorState {
            has_tab: true,
            ..Default::default()
        };

        for title in [
            "Go: Next Difference",
            "Go: Previous Difference",
            "Analysis: Compare / Diff > Swap Diff Files",
            "Analysis: Compare / Diff > Refresh Diff",
            "Analysis: Compare / Diff > Next Difference",
            "Analysis: Compare / Diff > Previous Difference",
        ] {
            let cmd = commands.iter().find(|c| c.title == title).unwrap_or_else(|| panic!("missing command {title}"));
            assert!(cmd.is_executable(&diff_only), "{title} must be enabled for diff");
            assert!(!cmd.is_executable(&doc_only), "{title} must be disabled for non-diff doc");
            assert!(!cmd.is_executable(&empty), "{title} must be disabled when empty");
        }

        for title in ["File: Close Tab", "File: Close Other Tabs > Close All Tabs"] {
            let cmd = commands.iter().find(|c| c.title == title).unwrap_or_else(|| panic!("missing command {title}"));
            assert!(cmd.is_executable(&settings_tab_only), "{title} must be enabled for Settings/non-doc tab");
            assert!(cmd.is_executable(&diff_only), "{title} must be enabled for Diff tab");
            assert!(cmd.is_executable(&doc_only), "{title} must be enabled for Doc tab");
            assert!(!cmd.is_executable(&empty), "{title} must be disabled when no tabs open");
        }
    }

    #[test]
    fn test_native_menu_state_propagation_and_macos_app_menu() {
        use crate::core::encoding::Encoding;
        use crate::core::radix::{ByteGroupSize, ByteOrder, DisplayRadix};
        use gpui_kit::{MenuItem, OsAction};

        let state = MenuEditorState {
            has_doc: true,
            has_tab: true,
            is_read_only: false,
            is_insert_mode: true,
            can_undo: true,
            can_redo: false,
            can_copy: true,
            encoding: Some(Encoding::Utf8),
            radix: Some(DisplayRadix::Decimal),
            group_size: Some(ByteGroupSize::Four),
            byte_order: Some(ByteOrder::BigEndian),
            inline_structure_view: true,
            hide_unbookmarked: true,
            ..Default::default()
        };

        let mac_menus = super::build_native_menus(&state, true);
        assert_eq!(mac_menus[0].name.as_ref(), "xvw");
        assert_eq!(mac_menus[1].name.as_ref(), "File");

        let std_menus = super::build_native_menus(&state, false);
        assert_eq!(std_menus[0].name.as_ref(), "File");

        let edit_menu = std_menus.iter().find(|m| m.name.as_ref() == "Edit").expect("Edit menu");
        let os_action_name = |os_action: Option<OsAction>| -> Option<&'static str> {
            os_action.map(|action| match action {
                OsAction::Cut => "Cut",
                OsAction::Copy => "Copy",
                OsAction::Paste => "Paste",
                OsAction::SelectAll => "SelectAll",
                OsAction::Undo => "Undo",
                OsAction::Redo => "Redo",
            })
        };
        let find_action = |items: &[MenuItem], name: &str| -> (bool, bool, Option<&'static str>) {
            items
                .iter()
                .find_map(|item| match item {
                    MenuItem::Action {
                        name: item_name,
                        disabled,
                        checked,
                        os_action,
                        ..
                    } if item_name.as_ref() == name => Some((*disabled, *checked, os_action_name(*os_action))),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing menu item {name}"))
        };

        assert_eq!(find_action(&edit_menu.items, "Undo"), (false, false, Some("Undo")));
        assert_eq!(find_action(&edit_menu.items, "Redo"), (true, false, Some("Redo")));
        assert_eq!(find_action(&edit_menu.items, "Copy"), (false, false, Some("Copy")));
        assert_eq!(find_action(&edit_menu.items, "Toggle Insert Mode"), (false, true, None));
        assert_eq!(find_action(&edit_menu.items, "Toggle Read-only"), (false, false, None));

        let view_menu = std_menus.iter().find(|m| m.name.as_ref() == "View").expect("View menu");
        fn submenu_items<'a>(items: &'a [MenuItem], name: &str) -> &'a [MenuItem] {
            items
                .iter()
                .find_map(|item| match item {
                    MenuItem::Submenu(sub) if sub.name.as_ref() == name => Some(sub.items.as_slice()),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing submenu {name}"))
        }

        let radix_items = submenu_items(&view_menu.items, "Radix");
        assert_eq!(find_action(radix_items, "Hexadecimal (16)"), (false, false, None));
        assert_eq!(find_action(radix_items, "Decimal (10)"), (false, true, None));

        let group_items = submenu_items(&view_menu.items, "Grouping");
        assert_eq!(find_action(group_items, "4 Bytes (32-bit)"), (false, true, None));

        let order_items = submenu_items(&view_menu.items, "Byte Order");
        assert_eq!(find_action(order_items, "Big Endian"), (false, true, None));

        let enc_items = submenu_items(&view_menu.items, "Encoding");
        assert_eq!(find_action(enc_items, "UTF-8"), (false, true, None));
        assert_eq!(find_action(enc_items, "ASCII"), (false, false, None));
    }
}

use std::sync::Arc;

use gpui_kit::component::menu::PopupMenu;
use gpui_kit::{Action, Context, Menu, MenuItem, SharedString, Window};

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub struct MenuEditorState {
    pub has_doc: bool,
    pub is_read_only: bool,
    pub can_undo: bool,
    pub can_redo: bool,
    pub has_selection: bool,
    pub can_copy: bool,
    pub can_close_others: bool,
    pub can_close_right: bool,
    pub has_saved: bool,
    pub has_scratch: bool,
}

#[derive(Clone)]
pub enum MenuItemDef {
    Action {
        label: SharedString,
        action: Arc<dyn Fn() -> Box<dyn Action> + Send + Sync>,
        is_enabled: Option<fn(&MenuEditorState) -> bool>,
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
        }
    }

    pub fn submenu(label: impl Into<SharedString>, items: Vec<MenuItemDef>) -> Self {
        Self::Submenu { label: label.into(), items }
    }

    pub fn separator() -> Self {
        Self::Separator
    }

    pub fn to_gpui_menu_item(&self) -> MenuItem {
        match self {
            MenuItemDef::Action { label, action, .. } => MenuItem::Action {
                name: label.clone(),
                action: (action)(),
                os_action: None,
                checked: false,
                disabled: false,
            },
            MenuItemDef::Submenu { label, items } => MenuItem::submenu(Menu {
                name: label.clone(),
                items: items.iter().map(|item| item.to_gpui_menu_item()).collect(),
                disabled: false,
            }),
            MenuItemDef::Separator => MenuItem::separator(),
        }
    }

    pub fn apply_to_popup_menu(&self, menu: PopupMenu, state: &MenuEditorState, window: &mut Window, cx: &mut Context<PopupMenu>) -> PopupMenu {
        match self {
            MenuItemDef::Action { label, action, is_enabled } => {
                let disabled = is_enabled.is_some_and(|f| !f(state));
                menu.menu_with_disabled(label.clone(), (action)(), disabled)
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
    pub fn to_gpui_menu(&self) -> Menu {
        Menu {
            name: self.name.into(),
            items: self.items.iter().map(|item| item.to_gpui_menu_item()).collect(),
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

pub fn application_menus() -> Vec<MenuDef> {
    vec![
        build_file_menu(),
        build_edit_menu(),
        build_view_menu(),
        build_go_menu(),
        build_analysis_menu(),
        build_scratch_menu(),
        build_window_menu(),
    ]
}

/// Updates the system menu bar with the latest application menus.
pub fn update_application_menus(cx: &gpui_kit::App) {
    cx.set_menus(application_menus().iter().map(|menu| menu.to_gpui_menu()));
}

fn build_file_menu() -> MenuDef {
    MenuDef {
        name: "File",
        items: vec![
            MenuItemDef::action("New File...", crate::actions::NewFile),
            MenuItemDef::action("Open File...", crate::actions::OpenFileDialog),
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
            MenuItemDef::action_with_condition("Close Tab", crate::actions::CloseActivePanel, |s| s.has_doc),
            MenuItemDef::submenu(
                "Close Other Tabs",
                vec![
                    MenuItemDef::action_with_condition("Close Others", crate::actions::CloseOtherTabs, |s| s.can_close_others),
                    MenuItemDef::action_with_condition("Close Tabs to Right", crate::actions::CloseTabsToRight, |s| s.can_close_right),
                    MenuItemDef::action_with_condition("Close Saved Tabs", crate::actions::CloseSavedTabs, |s| s.has_saved),
                    MenuItemDef::action_with_condition("Close All Tabs", crate::actions::CloseAllTabs, |s| s.has_doc),
                ],
            ),
            MenuItemDef::separator(),
            MenuItemDef::action_with_condition("Copy Path", crate::actions::CopyPath, |s| s.has_doc),
            MenuItemDef::action_with_condition("Copy File Name", crate::actions::CopyFileName, |s| s.has_doc),
            MenuItemDef::action_with_condition("Reveal in File Manager", crate::actions::RevealInExplorer, |s| s.has_doc),
            MenuItemDef::separator(),
            MenuItemDef::action("Quit", crate::actions::Quit),
        ],
    }
}

fn build_edit_menu() -> MenuDef {
    MenuDef {
        name: "Edit",
        items: vec![
            MenuItemDef::action_with_condition("Undo", crate::actions::Undo, |s| s.can_undo),
            MenuItemDef::action_with_condition("Redo", crate::actions::Redo, |s| s.can_redo),
            MenuItemDef::separator(),
            MenuItemDef::action_with_condition("Cut", crate::actions::Cut, |s| !s.is_read_only && s.has_doc && s.has_selection),
            MenuItemDef::action_with_condition("Copy", crate::actions::Copy, |s| s.can_copy),
            MenuItemDef::action_with_condition("Paste", crate::actions::Paste, |s| !s.is_read_only && s.has_doc),
            MenuItemDef::action_with_condition("Fill Selection...", crate::actions::FillSelection, |s| {
                !s.is_read_only && s.has_doc && s.has_selection
            }),
            MenuItemDef::action_with_condition("Toggle Insert Mode", crate::actions::ToggleInsertMode, |s| !s.is_read_only && s.has_doc),
            MenuItemDef::action_with_condition("Toggle Read-only", crate::actions::ToggleReadOnly, |s| s.has_doc),
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
                    MenuItemDef::action_with_condition("as Offset Link", crate::actions::CopyAsOffsetLink, |s| s.can_copy),
                ],
            ),
            MenuItemDef::action_with_condition("Select All", crate::actions::SelectAll, |s| s.has_doc),
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
                    MenuItemDef::action_with_condition("Show Only Bookmarked Regions", crate::actions::ToggleHideUnbookmarked, |s| s.has_doc),
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
    for encoding in crate::core::encoding::Encoding::primary_encodings() {
        encoding_items.push(MenuItemDef::action_with_condition(
            encoding.label(),
            crate::actions::SetEncoding { encoding: *encoding },
            |s| s.has_doc,
        ));
    }

    encoding_items.push(MenuItemDef::separator());

    // Regional/legacy categories as submenus
    for (category, encodings) in crate::core::encoding::Encoding::secondary_categories() {
        let cat_items = encodings
            .iter()
            .copied()
            .map(|encoding| MenuItemDef::action_with_condition(encoding.label(), crate::actions::SetEncoding { encoding }, |s| s.has_doc))
            .collect();
        encoding_items.push(MenuItemDef::submenu(category.label(), cat_items));
    }

    MenuDef {
        name: "View",
        items: vec![
            MenuItemDef::action("Toggle Left Panel", crate::actions::ToggleLeftPanel),
            MenuItemDef::action("Toggle Right Panel (Scratchpad)", crate::actions::ToggleRightPanel),
            MenuItemDef::submenu(
                "Panels",
                vec![
                    MenuItemDef::action("Files", crate::actions::ShowFilesTab),
                    MenuItemDef::action("Strings", crate::actions::ShowStringsTab),
                    MenuItemDef::action("Structure", crate::actions::ShowStructureTab),
                    MenuItemDef::action("Bookmarks", crate::actions::ShowBookmarksTab),
                    MenuItemDef::action("Checksum", crate::actions::ShowChecksumTab),
                    MenuItemDef::action("2D Visual Map", crate::actions::OpenVisualMap),
                ],
            ),
            MenuItemDef::separator(),
            MenuItemDef::submenu(
                "Radix",
                vec![
                    MenuItemDef::action_with_condition("Hexadecimal (16)", crate::actions::SetRadixHex, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Decimal (10)", crate::actions::SetRadixDec, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Octal (8)", crate::actions::SetRadixOct, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Binary (2)", crate::actions::SetRadixBin, |s| s.has_doc),
                ],
            ),
            MenuItemDef::submenu(
                "Grouping",
                vec![
                    MenuItemDef::action_with_condition("1 Byte (8-bit)", crate::actions::SetGroupSize1, |s| s.has_doc),
                    MenuItemDef::action_with_condition("2 Bytes (16-bit)", crate::actions::SetGroupSize2, |s| s.has_doc),
                    MenuItemDef::action_with_condition("4 Bytes (32-bit)", crate::actions::SetGroupSize4, |s| s.has_doc),
                    MenuItemDef::action_with_condition("8 Bytes (64-bit)", crate::actions::SetGroupSize8, |s| s.has_doc),
                ],
            ),
            MenuItemDef::submenu(
                "Byte Order",
                vec![
                    MenuItemDef::action_with_condition("Little Endian", crate::actions::SetByteOrderLittleEndian, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Big Endian", crate::actions::SetByteOrderBigEndian, |s| s.has_doc),
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
            MenuItemDef::action_with_condition("Next Difference", crate::actions::NextDifference, |s| s.has_doc),
            MenuItemDef::action_with_condition("Previous Difference", crate::actions::PrevDifference, |s| s.has_doc),
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
                    MenuItemDef::action_with_condition("Toggle Inline Structure View", crate::actions::ToggleInlineStructureView, |s| s.has_doc),
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
                    MenuItemDef::action("Swap Diff Files", crate::actions::SwapDiffFiles),
                    MenuItemDef::action("Refresh Diff", crate::actions::RefreshDiff),
                    MenuItemDef::separator(),
                    MenuItemDef::action_with_condition("Next Difference", crate::actions::NextDifference, |s| s.has_doc),
                    MenuItemDef::action_with_condition("Previous Difference", crate::actions::PrevDifference, |s| s.has_doc),
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
            MenuItemDef::submenu("Recent Scratches", recent_items),
            MenuItemDef::separator(),
            MenuItemDef::action("Reveal in File Manager", crate::actions::RevealScratchesInExplorer),
            MenuItemDef::separator(),
            MenuItemDef::submenu(
                "Insert into Scratchpad",
                vec![
                    MenuItemDef::action_with_condition("as Offset", crate::actions::InsertActiveOffsetOnly, |s| s.has_scratch),
                    MenuItemDef::action_with_condition("as Hex Bytes", crate::actions::InsertActiveHexBytes, |s| s.has_scratch),
                    MenuItemDef::action_with_condition("as Text / String", crate::actions::InsertActiveText, |s| s.has_scratch),
                    MenuItemDef::action_with_condition("as Offset Link", crate::actions::InsertActiveOffsetLink, |s| s.has_scratch),
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

#[cfg(test)]
mod tests {
    use super::{MenuEditorState, MenuItemDef, application_menus};

    #[test]
    fn test_application_menus_structure() {
        let menus = application_menus();
        assert_eq!(menus.len(), 7);
        assert_eq!(menus[0].name, "File");
        assert_eq!(menus[1].name, "Edit");
        assert_eq!(menus[2].name, "View");
        assert_eq!(menus[3].name, "Go");
        assert_eq!(menus[4].name, "Analysis");
        assert_eq!(menus[5].name, "Scratch");
        assert_eq!(menus[6].name, "Window");
    }

    #[test]
    fn test_to_gpui_menu_conversion() {
        let menus = application_menus();
        for menu in &menus {
            let gpui_menu = menu.to_gpui_menu();
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
        assert_eq!(insert_submenu.len(), 4);
        let link_item = insert_submenu
            .iter()
            .find(|item| matches!(item, MenuItemDef::Action { label, .. } if label.as_ref() == "as Offset Link"))
            .expect("as Offset Link item found");
        if let MenuItemDef::Action { is_enabled: Some(cond), .. } = link_item {
            assert!(cond(&MenuEditorState {
                has_scratch: true,
                ..Default::default()
            }));
            assert!(!cond(&MenuEditorState {
                has_scratch: false,
                ..Default::default()
            }));
        } else {
            panic!("as Offset Link item must have condition");
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

        assert_eq!(copy_as_submenu.len(), 11);
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
        use std::path::PathBuf;

        let entry_same_title = crate::service::ScratchEntry {
            id: Some(1),
            filename: "scratch_1.md".to_string(),
            path: PathBuf::from("scratch_1.md"),
            title: "Scratchpad".to_string(),
            modified: None,
        };
        let label1 = if entry_same_title.title != entry_same_title.filename {
            format!("{} ({})", entry_same_title.title, entry_same_title.filename)
        } else {
            entry_same_title.title
        };
        assert_eq!(label1, "Scratchpad (scratch_1.md)");

        let entry_custom = crate::service::ScratchEntry {
            id: Some(2),
            filename: "scratch_2.md".to_string(),
            path: PathBuf::from("scratch_2.md"),
            title: "My Custom Notes".to_string(),
            modified: None,
        };
        let label2 = if entry_custom.title != entry_custom.filename {
            format!("{} ({})", entry_custom.title, entry_custom.filename)
        } else {
            entry_custom.title
        };
        assert_eq!(label2, "My Custom Notes (scratch_2.md)");
    }
}

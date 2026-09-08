use crate::core::format::FileFormat;
use gpui::Action;
use schemars::JsonSchema;
use serde::Deserialize;

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = app)]
#[serde(deny_unknown_fields)]
pub struct OpenFile {
    pub path: String,
    #[serde(default)]
    pub format: Option<FileFormat>,
}

impl OpenFile {
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            format: None,
        }
    }

    pub fn with_format(path: impl Into<String>, format: Option<FileFormat>) -> Self {
        Self { path: path.into(), format }
    }
}

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = app)]
#[serde(deny_unknown_fields)]
pub struct SetFileTreeFolder {
    pub path: String,
}

#[derive(Clone, PartialEq, Action)]
pub struct Rename;

#[derive(Clone, PartialEq, Action)]
pub struct SelectItem;

#[derive(Clone, PartialEq, Action)]
pub struct OpenFolder;

#[derive(Clone, PartialEq, Action)]
pub struct CloseFolder;

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = app)]
#[serde(deny_unknown_fields)]
pub struct LoadChildren {
    pub path: String,
}

#[derive(Clone, PartialEq, Action)]
pub struct ToggleSearch;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleSearchPanel;

#[derive(Clone, PartialEq, Action)]
pub struct SearchNext;

#[derive(Clone, PartialEq, Action)]
pub struct SearchPrev;

#[derive(Clone, PartialEq, Action)]
pub struct FocusHexView;

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = app)]
#[serde(deny_unknown_fields)]
pub struct OpenDiff {
    pub left_path: String,
    pub right_path: String,
}

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = app)]
#[serde(deny_unknown_fields)]
pub struct SelectForCompare {
    pub path: String,
}

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = app)]
#[serde(deny_unknown_fields)]
pub struct CompareWithActiveFile {
    pub path: String,
}

#[derive(Clone, PartialEq, Action)]
pub struct CompareVisiblePanes;

#[derive(Clone, PartialEq, Action)]
pub struct CompareOpenFiles;

#[derive(Clone, PartialEq, Action)]
pub struct SwapDiffFiles;

#[derive(Clone, PartialEq, Action)]
pub struct RefreshDiff;

#[derive(Clone, PartialEq, Action)]
pub struct NextDifference;

#[derive(Clone, PartialEq, Action)]
pub struct PrevDifference;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleSyncScroll;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleLeftPanel;

#[derive(Clone, PartialEq, Action)]
pub struct OpenSettings;

#[derive(Clone, PartialEq, Action)]
pub struct OpenAbout;

#[derive(Clone, PartialEq, Action)]
pub struct NewFile;

#[derive(Clone, PartialEq, Action)]
pub struct OpenFileDialog;

#[derive(Clone, PartialEq, Action)]
pub struct Quit;

#[derive(Clone, PartialEq, Action)]
pub struct SelectAll;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleGoToAddress;

#[derive(Clone, PartialEq, Action)]
pub struct GoToBeginning;

#[derive(Clone, PartialEq, Action)]
pub struct GoToEnd;

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = app)]
#[serde(deny_unknown_fields)]
pub struct SetEncoding {
    pub encoding: crate::core::encoding::Encoding,
}

#[derive(Clone, PartialEq, Action)]
pub struct SetEncodingAscii;

#[derive(Clone, PartialEq, Action)]
pub struct SetEncodingUtf8;

#[derive(Clone, PartialEq, Action)]
pub struct SetEncodingUtf16Le;

#[derive(Clone, PartialEq, Action)]
pub struct SetEncodingUtf16Be;

#[derive(Clone, PartialEq, Action)]
pub struct SetRadixHex;

#[derive(Clone, PartialEq, Action)]
pub struct SetRadixDec;

#[derive(Clone, PartialEq, Action)]
pub struct SetRadixOct;

#[derive(Clone, PartialEq, Action)]
pub struct SetRadixBin;

#[derive(Clone, PartialEq, Action)]
pub struct SetGroupSize1;

#[derive(Clone, PartialEq, Action)]
pub struct SetGroupSize2;

#[derive(Clone, PartialEq, Action)]
pub struct SetGroupSize4;

#[derive(Clone, PartialEq, Action)]
pub struct SetGroupSize8;

#[derive(Clone, PartialEq, Action)]
pub struct SetByteOrderLittleEndian;

#[derive(Clone, PartialEq, Action)]
pub struct SetByteOrderBigEndian;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleByteOrder;

#[derive(Clone, PartialEq, Action)]
pub struct ShowFilesTab;

#[derive(Clone, PartialEq, Action)]
pub struct ShowStringsTab;

#[derive(Clone, PartialEq, Action)]
pub struct ShowStructureTab;

#[derive(Clone, PartialEq, Action)]
pub struct ShowChecksumTab;

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = app)]
pub struct LoadStructureDefinition;

/// Loads a previously selected structure definition path.
#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = app)]
#[serde(deny_unknown_fields)]
pub struct LoadStructureDefinitionFromHistory {
    pub path: String,
}

/// Removes a structure definition path from the recent history.
#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = app)]
#[serde(deny_unknown_fields)]
pub struct RemoveStructureDefinitionFromHistory {
    pub path: String,
}

/// Removes a binary file path from the recent history.
#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = app)]
#[serde(deny_unknown_fields)]
pub struct RemoveFileFromHistory {
    pub path: String,
}

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = app)]
pub struct ClearStructureDefinition;

#[derive(Clone, PartialEq, Action)]
pub struct OpenVisualMap;

#[derive(Clone, PartialEq, Action)]
pub struct CloseActivePanel;

#[derive(Clone, PartialEq, Action)]
pub struct AddCustomBreak;

#[derive(Clone, PartialEq, Action)]
pub struct RemoveCustomBreakBackward;

#[derive(Clone, PartialEq, Action)]
pub struct RemoveCustomBreakForward;

#[derive(Clone, PartialEq, Action)]
pub struct JoinLine;

#[derive(Clone, PartialEq, Action)]
pub struct ClearAllCustomBreaks;

#[derive(Clone, PartialEq, Action)]
pub struct ShowAllBookmarks;

#[derive(Clone, PartialEq, Action)]
pub struct HideAllBookmarks;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleBookmarkRed;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleBookmarkOrange;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleBookmarkYellow;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleBookmarkGreen;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleBookmarkCyan;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleBookmarkBlue;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleBookmarkPurple;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleBookmarkPink;

#[derive(Clone, PartialEq, Action)]
pub struct ShowOnlyBookmarkRed;

#[derive(Clone, PartialEq, Action)]
pub struct ShowOnlyBookmarkOrange;

#[derive(Clone, PartialEq, Action)]
pub struct ShowOnlyBookmarkYellow;

#[derive(Clone, PartialEq, Action)]
pub struct ShowOnlyBookmarkGreen;

#[derive(Clone, PartialEq, Action)]
pub struct ShowOnlyBookmarkCyan;

#[derive(Clone, PartialEq, Action)]
pub struct ShowOnlyBookmarkBlue;

#[derive(Clone, PartialEq, Action)]
pub struct ShowOnlyBookmarkPurple;

#[derive(Clone, PartialEq, Action)]
pub struct ShowOnlyBookmarkPink;

#[derive(Clone, PartialEq, Action)]
pub struct UnfoldBookmarkAtCursor;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleHideUnbookmarked;

#[derive(Clone, PartialEq, Action)]
pub struct ActivateNextTab;

#[derive(Clone, PartialEq, Action)]
pub struct ActivatePreviousTab;

#[derive(Clone, PartialEq, Deserialize, JsonSchema, Action)]
#[action(namespace = app)]
pub struct ActivateTab {
    pub index: usize,
}

#[derive(Clone, PartialEq, Action)]
pub struct CloseOtherTabs;

#[derive(Clone, PartialEq, Action)]
pub struct CloseTabsToRight;

#[derive(Clone, PartialEq, Action)]
pub struct CloseSavedTabs;

#[derive(Clone, PartialEq, Action)]
pub struct CloseAllTabs;

#[derive(Clone, PartialEq, Action)]
pub struct CopyPath;

#[derive(Clone, PartialEq, Action)]
pub struct CopyFileName;

#[derive(Clone, PartialEq, Action)]
pub struct RevealInExplorer;

#[derive(Clone, PartialEq, Action)]
pub struct SplitRight;

#[derive(Clone, PartialEq, Action)]
pub struct SplitDown;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleInlineStructureView;

#[derive(Clone, PartialEq, Action)]
pub struct ExpandAllStructure;

#[derive(Clone, PartialEq, Action)]
pub struct CollapseAllStructure;

/// Toggles the Address column in the structure tree.
#[derive(Clone, PartialEq, Action)]
pub struct ToggleStructureAddressColumn;

/// Toggles the TYPE column in the structure tree.
#[derive(Clone, PartialEq, Action)]
pub struct ToggleStructureTypeColumn;

/// Toggles the SIZE column in the structure tree.
#[derive(Clone, PartialEq, Action)]
pub struct ToggleStructureSizeColumn;

/// Toggles the VALUE column in the structure tree.
#[derive(Clone, PartialEq, Action)]
pub struct ToggleStructureValueColumn;

/// Copies the current structure analysis in a human-readable text format.
#[derive(Clone, PartialEq, Action)]
pub struct CopyStructureResult;

/// Exports the current structure analysis as a YAML document.
#[derive(Clone, PartialEq, Action)]
pub struct ExportStructureYaml;

#[derive(Clone, PartialEq, Action)]
pub struct Copy;

#[derive(Clone, PartialEq, Action)]
pub struct Cut;

#[derive(Clone, PartialEq, Action)]
pub struct Paste;

#[derive(Clone, PartialEq, Action)]
pub struct FillSelection;

#[derive(Clone, PartialEq, Action)]
pub struct Undo;

#[derive(Clone, PartialEq, Action)]
pub struct Redo;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleInsertMode;

#[derive(Clone, PartialEq, Action)]
pub struct Save;

#[derive(Clone, PartialEq, Action)]
pub struct SaveAs;

#[derive(Clone, PartialEq, Action)]
pub struct ToggleReadOnly;

#[derive(Clone, PartialEq, Action)]
pub struct CopyAsHexDump;

#[derive(Clone, PartialEq, Action)]
pub struct CopyAsCppArray;

#[derive(Clone, PartialEq, Action)]
pub struct CopyAsHexStream;

#[derive(Clone, PartialEq, Action)]
pub struct CopyAsHexSpaces;

#[derive(Clone, PartialEq, Action)]
pub struct CopyAsPrintableText;

#[derive(Clone, PartialEq, Action)]
pub struct CopyAsBase64;

#[derive(Clone, PartialEq, Action)]
pub struct CopyAsEscapedString;

#[derive(Clone, PartialEq, Action)]
pub struct CopyAsBinary;

#[derive(Clone, PartialEq, Action)]
pub struct CopyAsRustArray;

#[derive(Clone, PartialEq, Action)]
pub struct CopyAsJsonArray;

#[derive(Clone, PartialEq, Action)]
pub struct BookmarkRed;

#[derive(Clone, PartialEq, Action)]
pub struct BookmarkOrange;

#[derive(Clone, PartialEq, Action)]
pub struct BookmarkYellow;

#[derive(Clone, PartialEq, Action)]
pub struct BookmarkGreen;

#[derive(Clone, PartialEq, Action)]
pub struct BookmarkCyan;

#[derive(Clone, PartialEq, Action)]
pub struct BookmarkBlue;

#[derive(Clone, PartialEq, Action)]
pub struct BookmarkPurple;

#[derive(Clone, PartialEq, Action)]
pub struct BookmarkPink;

#[derive(Clone, PartialEq, Action)]
pub struct ClearBookmark;

#[derive(Clone, PartialEq, Action)]
pub struct ClearAllBookmarks;

#[derive(Clone, PartialEq, Action)]
pub struct ShowBookmarksTab;

#[derive(Clone, PartialEq, Action)]
pub struct ExportBookmarks;

#[derive(Clone, PartialEq, Action)]
pub struct ImportBookmarks;

/// Imports a Motorola S-Record or Intel HEX file.
#[derive(Clone, PartialEq, Action)]
pub struct ImportHexOrMot;

/// Imports a Base64 encoded file.
#[derive(Clone, PartialEq, Action)]
pub struct ImportBase64;

/// Exports the current document as a Base64 file.
#[derive(Clone, PartialEq, Action)]
pub struct ExportBase64;

/// Exports the current document as a Motorola S-Record file.
#[derive(Clone, PartialEq, Action)]
pub struct ExportMotorolaSrec;

/// Exports the current document as an Intel HEX file.
#[derive(Clone, PartialEq, Action)]
pub struct ExportIntelHex;

/// Exports the current document as a raw binary file.
#[derive(Clone, PartialEq, Action)]
pub struct ExportRawBinary;

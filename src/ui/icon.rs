use gpui_kit::component::{Icon, IconNamed};
use gpui_kit::{AnyElement, App, IntoElement, RenderOnce, SharedString, Window};

/// Application icon set powered by official Lucide SVG icons.
#[derive(IntoElement, Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconName {
    // Files & Folders
    File,
    Files,
    FileCode,
    FileText,
    TextInitial,
    Folder,
    FolderOpen,
    FolderClosed,
    FolderSearch,

    // Binary & Structure
    Binary,
    Layers,
    Boxes,
    ListTree,
    Network,
    Braces,
    Code,

    // Inspector & Analysis
    SquareMousePointer,
    ScanEye,
    SearchCode,
    SearchText,
    Eye,
    EyeOff,

    // Visual Map
    Map,
    Grid2x2,
    Image,

    // Checksum & Math
    Hash,
    Calculator,
    ShieldCheck,
    ChartPie,

    // Highlights & Styling
    Highlighter,
    Bookmark,
    BookmarkPlus,
    BookmarkX,
    Binoculars,
    PenLine,
    PenOff,
    Palette,
    Sparkles,

    // Navigation & Common UI
    Search,
    Replace,
    Import,
    HardDriveDownload,
    Plus,
    Minus,
    Close,
    Check,
    Copy,
    ExternalLink,
    Delete,
    Eraser,
    Settings,
    Settings2,
    SlidersHorizontal,
    Info,
    TriangleAlert,
    CircleAlert,
    Loader,
    LoaderCircle,
    GitCompare,
    Split,
    Undo,
    Redo,
    ChevronUp,
    ChevronDown,
    ChevronLeft,
    ChevronRight,
    ChevronsUpDown,
    Maximize,
    Minimize,
    PanelLeft,
    PanelRight,
    PanelBottom,
    BookOpen,
}

impl IconNamed for IconName {
    fn path(self) -> SharedString {
        match self {
            Self::File => "icons/file.svg",
            Self::Files => "icons/files.svg",
            Self::FileCode => "icons/file-code.svg",
            Self::FileText => "icons/file-text.svg",
            Self::TextInitial => "icons/text-initial.svg",
            Self::Folder => "icons/folder.svg",
            Self::FolderOpen => "icons/folder-open.svg",
            Self::FolderClosed => "icons/folder-closed.svg",
            Self::FolderSearch => "icons/folder-search.svg",
            Self::Binary => "icons/binary.svg",
            Self::Layers => "icons/layers.svg",
            Self::Boxes => "icons/boxes.svg",
            Self::ListTree => "icons/list-tree.svg",
            Self::Network => "icons/network.svg",
            Self::Braces => "icons/braces.svg",
            Self::Code => "icons/code.svg",
            Self::SquareMousePointer => "icons/square-mouse-pointer.svg",
            Self::ScanEye => "icons/scan-eye.svg",
            Self::SearchCode => "icons/search-code.svg",
            Self::SearchText => "icons/search-text.svg",
            Self::Eye => "icons/eye.svg",
            Self::EyeOff => "icons/eye-off.svg",
            Self::Map => "icons/map.svg",
            Self::Grid2x2 => "icons/grid-2x2.svg",
            Self::Image => "icons/image.svg",
            Self::Hash => "icons/hash.svg",
            Self::Calculator => "icons/calculator.svg",
            Self::ShieldCheck => "icons/shield-check.svg",
            Self::ChartPie => "icons/chart-pie.svg",
            Self::Highlighter => "icons/highlighter.svg",
            Self::Bookmark => "icons/bookmark.svg",
            Self::BookmarkPlus => "icons/bookmark-plus.svg",
            Self::BookmarkX => "icons/bookmark-x.svg",
            Self::Binoculars => "icons/binoculars.svg",
            Self::PenLine => "icons/pen-line.svg",
            Self::PenOff => "icons/pen-off.svg",
            Self::Palette => "icons/palette.svg",
            Self::Sparkles => "icons/sparkles.svg",
            Self::Search => "icons/search.svg",
            Self::Replace => "icons/replace.svg",
            Self::Import => "icons/import.svg",
            Self::HardDriveDownload => "icons/hard-drive-download.svg",
            Self::Plus => "icons/plus.svg",
            Self::Minus => "icons/minus.svg",
            Self::Close => "icons/x.svg",
            Self::Check => "icons/check.svg",
            Self::Copy => "icons/copy.svg",
            Self::ExternalLink => "icons/external-link.svg",
            Self::Delete => "icons/trash-2.svg",
            Self::Eraser => "icons/eraser.svg",
            Self::Settings => "icons/settings.svg",
            Self::Settings2 => "icons/settings-2.svg",
            Self::SlidersHorizontal => "icons/sliders-horizontal.svg",
            Self::Info => "icons/info.svg",
            Self::TriangleAlert => "icons/triangle-alert.svg",
            Self::CircleAlert => "icons/circle-alert.svg",
            Self::Loader => "icons/loader.svg",
            Self::LoaderCircle => "icons/loader-circle.svg",
            Self::GitCompare => "icons/git-compare.svg",
            Self::Split => "icons/split.svg",
            Self::Undo => "icons/undo.svg",
            Self::Redo => "icons/redo.svg",
            Self::ChevronUp => "icons/chevron-up.svg",
            Self::ChevronDown => "icons/chevron-down.svg",
            Self::ChevronLeft => "icons/chevron-left.svg",
            Self::ChevronRight => "icons/chevron-right.svg",
            Self::ChevronsUpDown => "icons/chevrons-up-down.svg",
            Self::Maximize => "icons/maximize.svg",
            Self::Minimize => "icons/minimize.svg",
            Self::PanelLeft => "icons/panel-left.svg",
            Self::PanelRight => "icons/panel-right.svg",
            Self::PanelBottom => "icons/panel-bottom.svg",
            Self::BookOpen => "icons/book-open.svg",
        }
        .into()
    }
}

impl From<IconName> for AnyElement {
    fn from(val: IconName) -> Self {
        Icon::from(val).into_any_element()
    }
}

impl RenderOnce for IconName {
    fn render(self, _: &mut Window, _cx: &mut App) -> impl IntoElement {
        Icon::from(self)
    }
}

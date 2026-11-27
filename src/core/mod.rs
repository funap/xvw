// Buffer, document model & editing
pub mod buffer;
pub mod command;
pub mod dealloc;
pub mod document;
pub mod editor;
pub mod fill;
pub mod history;
pub mod new_file;
pub mod selection;

// Addressing, layout & formatting
pub mod address_map;
pub mod clipboard;
pub mod encoding;
pub mod format;
pub mod layout;
pub mod radix;

// Search, navigation & annotations
pub mod bookmark;
pub mod fuzzy_matcher;
pub mod goto;
pub mod offset_link;
pub mod recent_history;
pub mod scratch;
pub mod search;

// Binary analysis & structure parsing
pub mod checksum;
pub mod diff;
pub mod entropy;
pub mod inspector;
pub mod strings;
pub mod structure;
pub mod visual_map;
pub mod zlib;

// Visual theme & appearance
pub mod appearance;
pub mod color;

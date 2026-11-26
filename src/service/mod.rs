pub mod bookmark_service;
pub mod diff_service;
pub mod document_service;
pub mod scratch_service;
pub mod search_service;
pub mod structure_service;
pub mod zlib_service;

pub use bookmark_service::BookmarkService;
pub use diff_service::DiffService;
pub use document_service::DocumentService;
#[allow(unused_imports)]
pub use scratch_service::{ScratchEntry, ScratchService};
pub use search_service::SearchService;
pub use structure_service::StructureService;
pub use zlib_service::ZlibService;

/// Registers Tokio's blocking pool as the background deallocation dispatcher for `core::dealloc`.
pub fn init_background_dealloc() {
    crate::core::dealloc::register_background_dispatcher(|task| {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            handle.spawn_blocking(task);
        } else {
            task();
        }
    });
}

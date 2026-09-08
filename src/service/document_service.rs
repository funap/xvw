use crate::core::buffer::Buffer;
use crate::core::document::Document;
use crate::core::editor::Editor;
use gpui_kit::{App, Entity, EntityId, Task, WeakEntity};
use std::collections::HashMap;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// A service for managing file buffers, documents, and synchronization across open views.
/// It caches open files to avoid redundant reads and ensures thread-safe access.
#[allow(dead_code)]
#[derive(Clone)]
pub struct DocumentService {
    documents: Arc<RwLock<HashMap<PathBuf, Arc<RwLock<Document>>>>>,
    editors: Arc<RwLock<HashMap<PathBuf, Vec<WeakEntity<Editor>>>>>,
}

#[allow(dead_code)]
impl DocumentService {
    /// Creates a new, empty DocumentService.
    pub fn new() -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
            editors: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Registers an editor weak entity for a given document path to receive live change notifications.
    pub fn register_editor(&self, path: PathBuf, editor: WeakEntity<Editor>) {
        let path = path.canonicalize().unwrap_or(path);
        let mut editors = self.editors.write().expect("editors write lock");
        let list = editors.entry(path).or_default();
        list.retain(|w| w.upgrade().is_some());
        list.push(editor);
    }

    /// Releases one editor's ownership of a cached document.
    ///
    /// The cache remains available while another editor still references the
    /// same path. Once the last editor is released, the document cache entry
    /// is evicted so a future open starts with a fresh document state.
    pub fn release_editor(&self, path: &Path, editor_id: EntityId) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let should_evict = {
            let mut editors = self.editors.write().expect("editors write lock");
            let remove_entry = if let Some(list) = editors.get_mut(&path) {
                list.retain(|editor| editor.entity_id() != editor_id && editor.upgrade().is_some());
                list.is_empty()
            } else {
                true
            };
            if remove_entry {
                editors.remove(&path);
            }
            remove_entry
        };

        if should_evict {
            self.documents.write().expect("documents write lock").remove(&path);
        }
    }

    /// Notifies all active editors viewing the document at `path` to invalidate layout and repaint.
    pub fn notify_document_changed(&self, path: &Path, cx: &mut App) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let editors_to_notify: Vec<Entity<Editor>> = {
            let mut editors = self.editors.write().expect("editors write lock");
            if let Some(list) = editors.get_mut(&path) {
                list.retain(|w| w.upgrade().is_some());
                list.iter().filter_map(|w| w.upgrade()).collect()
            } else {
                Vec::new()
            }
        };

        for editor in editors_to_notify {
            editor.update(cx, |ed, cx| {
                ed.invalidate_line_map();
                cx.notify();
            });
        }
    }

    /// Opens a file asynchronously.
    /// If the file is already in the cache, it returns the cached document.
    /// Otherwise, it reads the file from disk, adds it to the cache, and returns it.
    /// This operation is thread-safe.
    pub async fn open_file(&self, path: PathBuf) -> anyhow::Result<Arc<RwLock<Document>>> {
        let path = path.canonicalize().unwrap_or(path);
        // First, check if the document is already in the cache with a read lock.
        if let Some(document) = self.documents.read().expect("documents read lock").get(&path) {
            return Ok(document.clone());
        }

        // If not in the cache, read the file using memory mapping without holding any lock.
        let path_clone = path.clone();
        let buffer = tokio::task::spawn_blocking(move || -> anyhow::Result<Buffer> {
            let file = std::fs::File::open(&path_clone)?;
            // SAFETY: Memory mapping the opened file is safe as long as the file is not
            // concurrently truncated or modified outside this process. Buffer encapsulates
            // read-only access to this memory mapping.
            let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };
            Ok(Buffer::from_mmap(mmap))
        })
        .await??;
        let mut doc = Document::new_read_only(path.clone(), buffer);
        doc.format = crate::core::format::FileFormat::Binary;
        let new_document = Arc::new(RwLock::new(doc));

        // Acquire a write lock to insert the new document into the cache.
        let mut documents = self.documents.write().expect("documents write lock");

        // Before inserting, check again if another thread has inserted it in the meantime.
        if let Some(document) = documents.get(&path) {
            return Ok(document.clone());
        }

        documents.insert(path, new_document.clone());
        Ok(new_document)
    }

    /// Closes a file by removing it from the document cache.
    pub fn close_file(&self, path: &std::path::Path) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let mut documents = self.documents.write().expect("documents write lock");
        documents.remove(&path);
    }

    /// Writes the current document snapshot to its path on a background
    /// executor. The document lock is released before any filesystem await.
    pub fn save_document(&self, document: Arc<RwLock<Document>>, cx: &App) -> Task<anyhow::Result<()>> {
        let (path, contents, address_map, format) = {
            let document = document.read().expect("document read lock");
            if document.is_read_only() {
                return cx.background_executor().spawn(async { Err(anyhow::anyhow!("document is read-only")) });
            }
            (
                document.path().to_path_buf(),
                document.buffer.data().to_vec(),
                document.address_map.clone(),
                document.format,
            )
        };
        cx.background_executor().spawn(async move {
            let bytes_to_write = match format {
                crate::core::format::FileFormat::MotorolaSrec | crate::core::format::FileFormat::HexOrMot => {
                    crate::core::hex_import::export_motorola_srec(&contents, &address_map).into_bytes()
                }
                crate::core::format::FileFormat::IntelHex => crate::core::hex_import::export_intel_hex(&contents, &address_map).into_bytes(),
                crate::core::format::FileFormat::Binary => crate::core::hex_import::export_raw_binary(&contents, &address_map, 0x00),
                crate::core::format::FileFormat::Base64 => crate::core::format::export_base64(&contents, &address_map).into_bytes(),
            };
            std::fs::write(path, bytes_to_write)?;
            Ok(())
        })
    }

    /// Writes a document snapshot to an explicit path on a background
    /// executor. This is used by Save As workflows.
    pub fn save_document_to_path(&self, document: Arc<RwLock<Document>>, path: PathBuf, cx: &App) -> Task<anyhow::Result<()>> {
        let (contents, address_map, format) = {
            let document = document.read().expect("document read lock");
            (document.buffer.data().to_vec(), document.address_map.clone(), document.format)
        };
        cx.background_executor().spawn(async move {
            let bytes_to_write = if crate::core::hex_import::is_mot_extension(&path) {
                crate::core::hex_import::export_motorola_srec(&contents, &address_map).into_bytes()
            } else if crate::core::hex_import::is_hex_extension(&path) {
                crate::core::hex_import::export_intel_hex(&contents, &address_map).into_bytes()
            } else if crate::core::format::is_base64_extension(&path) {
                crate::core::format::export_base64(&contents, &address_map).into_bytes()
            } else {
                match format {
                    crate::core::format::FileFormat::MotorolaSrec | crate::core::format::FileFormat::HexOrMot => {
                        crate::core::hex_import::export_motorola_srec(&contents, &address_map).into_bytes()
                    }
                    crate::core::format::FileFormat::IntelHex => crate::core::hex_import::export_intel_hex(&contents, &address_map).into_bytes(),
                    crate::core::format::FileFormat::Binary => crate::core::hex_import::export_raw_binary(&contents, &address_map, 0x00),
                    crate::core::format::FileFormat::Base64 => crate::core::format::export_base64(&contents, &address_map).into_bytes(),
                }
            };
            std::fs::write(path, bytes_to_write)?;
            Ok(())
        })
    }

    /// Searches for a query in the given buffer based on the search options.
    /// Returns a Task that executes the search in the background.
    pub fn search(&self, buffer: Arc<Buffer>, query: String, options: crate::core::search::SearchOptions, cx: &App) -> Task<Vec<usize>> {
        crate::service::search_service::SearchService.search(buffer, query, options, cx)
    }

    /// Searches for a query in the given buffer respecting memory segment boundaries.
    pub fn search_with_segments(
        &self,
        buffer: Arc<Buffer>,
        query: String,
        options: crate::core::search::SearchOptions,
        segments: Vec<std::ops::Range<usize>>,
        cx: &App,
    ) -> Task<Vec<usize>> {
        crate::service::search_service::SearchService.search_with_segments(buffer, query, options, segments, cx)
    }

    /// Performs a search and updates the provided Editor entity with the results.
    pub fn perform_search(
        &self,
        editor: Entity<crate::core::editor::Editor>,
        query: String,
        options: crate::core::search::SearchOptions,
        generation: usize,
        is_full: bool,
        cx: &App,
    ) -> Task<()> {
        crate::service::search_service::SearchService.perform_search(editor, query, options, generation, is_full, cx)
    }

    /// Performs an incremental search: immediate viewport search followed by background full search.
    pub fn incremental_search(
        &self,
        editor: Entity<Editor>,
        query: String,
        mode: crate::core::search::SearchMode,
        viewport_range: Range<usize>,
        cx: &App,
    ) -> (Task<()>, Task<()>) {
        crate::service::search_service::SearchService.incremental_search(editor, query, mode, viewport_range, cx)
    }

    pub fn compute_diff(&self, left: Arc<RwLock<Document>>, right: Arc<RwLock<Document>>, cx: &App) -> Task<crate::core::diff::DiffResult> {
        crate::service::diff_service::DiffService.compute_diff(left, right, cx)
    }
}

impl Default for DocumentService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestFile(PathBuf);

    impl TestFile {
        fn create(label: &str, contents: &[u8]) -> Self {
            let path = temporary_path(label);
            std::fs::write(&path, contents).expect("write test file");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TestFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn temporary_path(label: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after the Unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("xvw-document-service-{label}-{}-{nonce}.bin", std::process::id()))
    }

    #[test]
    fn test_document_service_register_empty() {
        let service = DocumentService::new();
        let path = PathBuf::from("test_doc.bin");
        assert!(service.editors.read().unwrap().get(&path).is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn open_file_caches_and_reopens_documents() {
        let file = TestFile::create("cache", &[0x10, 0x20, 0x30]);

        let service = DocumentService::new();
        let first = service.open_file(file.path().to_path_buf()).await.expect("open test file");
        assert_eq!(first.read().unwrap().buffer.data(), &[0x10, 0x20, 0x30]);
        assert!(first.read().unwrap().is_read_only());

        let second = service.open_file(file.path().to_path_buf()).await.expect("open cached file");
        assert!(Arc::ptr_eq(&first, &second), "opening a cached path must reuse its document");

        service.close_file(file.path());
        let reopened = service.open_file(file.path().to_path_buf()).await.expect("reopen test file");
        assert!(!Arc::ptr_eq(&first, &reopened), "closing a file must evict its cached document");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn open_file_returns_an_error_for_missing_path() {
        let path = temporary_path("missing");
        let service = DocumentService::new();

        let result = service.open_file(path).await;

        assert!(result.is_err());
    }

    #[test]
    fn test_save_document_with_address_map_to_bin_and_mot() {
        let data = vec![0x11, 0x22, 0x33, 0x44];
        let map = crate::core::address_map::AddressMap::from_segments(vec![
            crate::core::address_map::MemorySegment {
                buffer_offset: 0,
                address: 4,
                length: 2,
            },
            crate::core::address_map::MemorySegment {
                buffer_offset: 2,
                address: 8,
                length: 2,
            },
        ]);

        let doc = Document::new(PathBuf::from("test.mot"), crate::core::buffer::Buffer::new(data)).with_address_map(map);

        let bin_path = PathBuf::from("output.bin");
        let mot_path = PathBuf::from("output.mot");

        assert!(!crate::core::format::is_mot_extension(&bin_path));
        assert!(crate::core::format::is_mot_extension(&mot_path));

        let bin_bytes = crate::core::format::export_raw_binary(doc.buffer.data(), &doc.address_map, 0x00);
        assert_eq!(bin_bytes.len(), 10);
        assert_eq!(bin_bytes, vec![0, 0, 0, 0, 0x11, 0x22, 0, 0, 0x33, 0x44]);

        let mot_string = crate::core::format::export_motorola_srec(doc.buffer.data(), &doc.address_map);
        assert!(mot_string.contains("S1") || mot_string.contains("S2") || mot_string.contains("S3"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn open_file_opens_hex_as_raw_binary_without_auto_detection() {
        let hex_content = ":0400000001020304F2\n:00000001FF\n";
        let file = TestFile::create("sample.hex", hex_content.as_bytes());

        let service = DocumentService::new();
        let doc_arc = service.open_file(file.path().to_path_buf()).await.expect("open hex file");
        let doc = doc_arc.read().unwrap();

        // The buffer must contain the literal raw ASCII bytes of the file, not decoded segments.
        assert_eq!(doc.buffer.data(), hex_content.as_bytes());
        assert_eq!(doc.address_map, crate::core::address_map::AddressMap::default());
        assert_eq!(doc.format, crate::core::format::FileFormat::Binary);
    }

    #[test]
    fn test_export_document_as_base64() {
        let data = vec![0x48, 0x65, 0x6C, 0x6C, 0x6F]; // "Hello"
        let map = crate::core::address_map::AddressMap::default();
        let doc = Document::new(PathBuf::from("test.b64"), crate::core::buffer::Buffer::new(data)).with_format(crate::core::format::FileFormat::Base64);

        let b64_path = PathBuf::from("output.b64");
        assert!(crate::core::format::is_base64_extension(&b64_path));

        let b64_string = crate::core::format::export_base64(doc.buffer.data(), &map);
        assert_eq!(b64_string.trim(), "SGVsbG8=");
        let parsed = crate::core::format::parse_base64(&b64_string).expect("parse exported base64");
        assert_eq!(parsed, b"Hello");
    }
}

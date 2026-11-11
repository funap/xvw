use crate::core::address_map::AddressMap;
use crate::core::buffer::Buffer;
use crate::core::document::Document;
use crate::core::editor::Editor;
use crate::core::format::FileFormat;
use gpui_kit::{App, Entity, EntityId, Task, WeakEntity};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Determines the target file format for saving, preferring file extensions if applicable.
pub fn determine_export_format(path: &Path, fallback_format: FileFormat) -> FileFormat {
    FileFormat::from_path(path).unwrap_or(fallback_format)
}

/// Serializes raw buffer bytes and address map into the bytes corresponding to the target format.
pub fn export_document_bytes(contents: &[u8], address_map: &AddressMap, format: FileFormat) -> Vec<u8> {
    format.encode_bytes(contents, address_map)
}

/// Result of opening or importing a document through DocumentService.
pub struct OpenedDocument {
    pub document: Arc<RwLock<Document>>,
    pub detected_format: FileFormat,
    pub format_label: &'static str,
    pub address_gaps_count: usize,
}

/// A service for managing file buffers, documents, and synchronization across open views.
/// It caches open files to avoid redundant reads and ensures thread-safe access.
#[derive(Clone)]
pub struct DocumentService {
    documents: Arc<RwLock<HashMap<PathBuf, Arc<RwLock<Document>>>>>,
    editors: Arc<RwLock<HashMap<PathBuf, Vec<WeakEntity<Editor>>>>>,
}

impl std::fmt::Debug for DocumentService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DocumentService").finish_non_exhaustive()
    }
}

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

    /// Updates the document cache key when a document is renamed / saved-as.
    pub fn retarget_document_cache(&self, old_path: &Path, new_path: PathBuf) {
        let old_path = old_path.canonicalize().unwrap_or_else(|_| old_path.to_path_buf());
        let new_path = new_path.canonicalize().unwrap_or(new_path);
        if old_path == new_path {
            return;
        }

        let mut documents = self.documents.write().expect("documents write lock");
        if let Some(doc) = documents.remove(&old_path) {
            documents.insert(new_path.clone(), doc);
        }

        let mut editors = self.editors.write().expect("editors write lock");
        if let Some(list) = editors.remove(&old_path) {
            editors.insert(new_path, list);
        }
    }

    /// Finalizes saving a document by verifying that history state has not changed,
    /// marking it as saved, and notifying observers.
    pub fn finalize_save_state(&self, document: &Arc<RwLock<Document>>, expected_state_id: usize, cx: &mut App) -> bool {
        let should_mark = document.read().map(|doc| doc.history.state_id() == expected_state_id).unwrap_or(false);
        if should_mark {
            if let Ok(mut doc_write) = document.write() {
                doc_write.mark_as_saved();
            }
            let path = document.read().map(|doc| doc.path().to_path_buf()).unwrap_or_default();
            self.notify_document_changed(&path, cx);
        }
        should_mark
    }

    /// Finalizes retargeting a document path (Save As), updating cache keys,
    /// updating document path, marking as saved if state matches, and notifying observers.
    pub fn finalize_save_as_state(&self, document: &Arc<RwLock<Document>>, new_path: PathBuf, expected_state_id: usize, cx: &mut App) {
        let old_path = document.read().map(|doc| doc.path().to_path_buf()).unwrap_or_default();

        if let Ok(mut doc_write) = document.write() {
            doc_write.set_path(new_path.clone());
            if doc_write.history.state_id() == expected_state_id {
                doc_write.mark_as_saved();
            }
        }

        self.retarget_document_cache(&old_path, new_path.clone());
        self.notify_document_changed(&new_path, cx);
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
        let opened = self.open_or_import_file(path, None).await?;
        Ok(opened.document)
    }

    /// Opens or imports a file asynchronously according to optional format instructions.
    /// Caches the resulting document in `documents`.
    pub async fn open_or_import_file(&self, path: PathBuf, format_hint: Option<FileFormat>) -> anyhow::Result<OpenedDocument> {
        let path = path.canonicalize().unwrap_or(path);

        if let Some(document) = self.documents.read().expect("documents read lock").get(&path) {
            let doc_read = document.read().expect("document read lock");
            let detected_format = doc_read.format;
            let format_label = match detected_format {
                FileFormat::Base64 => "Base64",
                FileFormat::IntelHex => "Intel HEX",
                FileFormat::MotorolaSrec => "Motorola S-Record",
                FileFormat::HexOrMot => "Intel HEX or Motorola S-Record",
                FileFormat::Binary => "Binary",
            };
            let address_gaps_count = if doc_read.address_map.has_gaps() {
                doc_read.address_map.segments.len()
            } else {
                0
            };
            return Ok(OpenedDocument {
                document: document.clone(),
                detected_format,
                format_label,
                address_gaps_count,
            });
        }

        let is_import = format_hint.map(|f| f.is_import()).unwrap_or(false);
        let path_clone = path.clone();

        let (doc, detected_format, format_label, address_gaps_count) = if is_import {
            tokio::task::spawn_blocking(move || -> anyhow::Result<(Document, FileFormat, &'static str, usize)> {
                let content = std::fs::read_to_string(&path_clone)?;
                if format_hint == Some(FileFormat::Base64) {
                    let data = crate::core::format::parse_base64(&content).map_err(|e| anyhow::anyhow!("Failed to parse Base64 file: {e}"))?;
                    let buffer = Buffer::new(data);
                    let doc = Document::new_read_only(path_clone, buffer).with_format(FileFormat::Base64);
                    Ok((doc, FileFormat::Base64, "Base64", 0))
                } else {
                    let parse_result = match format_hint {
                        Some(FileFormat::IntelHex) => crate::core::hex_import::parse_intel_hex(&content),
                        Some(FileFormat::MotorolaSrec) => crate::core::hex_import::parse_motorola_srec(&content),
                        _ => crate::core::hex_import::parse_hex_or_mot(&content),
                    }
                    .map_err(|e| anyhow::anyhow!("Failed to parse hex/mot file: {e}"))?;

                    let format = FileFormat::from(parse_result.format);
                    let format_label = parse_result.format.label();
                    let gaps = if parse_result.address_map.has_gaps() {
                        parse_result.address_map.segments.len()
                    } else {
                        0
                    };
                    let buffer = Buffer::new(parse_result.data);
                    let doc = Document::new_read_only(path_clone, buffer)
                        .with_address_map(parse_result.address_map)
                        .with_format(format);
                    Ok((doc, format, format_label, gaps))
                }
            })
            .await??
        } else {
            tokio::task::spawn_blocking(move || -> anyhow::Result<(Document, FileFormat, &'static str, usize)> {
                let file = std::fs::File::open(&path_clone)?;
                let buffer = if file.metadata()?.len() == 0 {
                    Buffer::empty()
                } else {
                    // SAFETY: Memory mapping the opened file is safe as long as the file is not
                    // concurrently truncated or modified outside this process. Buffer encapsulates
                    // read-only access to this memory mapping.
                    let mmap = unsafe { memmap2::MmapOptions::new().map(&file)? };
                    Buffer::from_mmap(mmap)
                };
                let mut doc = Document::new_read_only(path_clone, buffer);
                doc.format = FileFormat::Binary;
                Ok((doc, FileFormat::Binary, "Binary", 0))
            })
            .await??
        };

        let new_document = Arc::new(RwLock::new(doc));

        let mut documents = self.documents.write().expect("documents write lock");
        if let Some(existing) = documents.get(&path) {
            return Ok(OpenedDocument {
                document: existing.clone(),
                detected_format,
                format_label,
                address_gaps_count,
            });
        }

        documents.insert(path, new_document.clone());
        Ok(OpenedDocument {
            document: new_document,
            detected_format,
            format_label,
            address_gaps_count,
        })
    }

    /// Closes a file by removing it from the document cache.
    #[allow(dead_code)]
    pub fn close_file(&self, path: &std::path::Path) {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let mut documents = self.documents.write().expect("documents write lock");
        documents.remove(&path);
    }

    /// Writes the current document snapshot to its path on a background
    /// executor. The document lock is released before any filesystem await.
    pub fn save_document(&self, document: Arc<RwLock<Document>>, cx: &App) -> Task<anyhow::Result<()>> {
        let (path, buffer, address_map, format) = {
            let document = document.read().expect("document read lock");
            if document.is_read_only() {
                return cx.background_executor().spawn(async { Err(anyhow::anyhow!("document is read-only")) });
            }
            (
                document.path().to_path_buf(),
                document.buffer.clone(),
                document.address_map.clone(),
                document.format,
            )
        };
        cx.background_executor().spawn(async move {
            let bytes_to_write = export_document_bytes(buffer.data(), &address_map, format);
            std::fs::write(path, bytes_to_write)?;
            Ok(())
        })
    }

    /// Writes a document snapshot to an explicit path on a background
    /// executor. This is used by Save As workflows.
    pub fn save_document_to_path(&self, document: Arc<RwLock<Document>>, path: PathBuf, cx: &App) -> Task<anyhow::Result<()>> {
        let (buffer, address_map, format) = {
            let document = document.read().expect("document read lock");
            let target_format = determine_export_format(&path, document.format);
            (document.buffer.clone(), document.address_map.clone(), target_format)
        };
        cx.background_executor().spawn(async move {
            let bytes_to_write = export_document_bytes(buffer.data(), &address_map, format);
            std::fs::write(path, bytes_to_write)?;
            Ok(())
        })
    }

    /// Exports raw bytes with custom formatting function to target path on background executor.
    pub fn export_document_to_path(
        &self,
        document: Arc<RwLock<Document>>,
        path: PathBuf,
        export_fn: fn(&[u8], &AddressMap) -> Vec<u8>,
        cx: &App,
    ) -> Task<anyhow::Result<()>> {
        let (contents, address_map) = {
            let doc = document.read().expect("document read lock");
            (doc.buffer.data().to_vec(), doc.address_map.clone())
        };
        cx.background_executor().spawn(async move {
            let bytes = export_fn(&contents, &address_map);
            std::fs::write(path, bytes)?;
            Ok(())
        })
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

    #[tokio::test(flavor = "current_thread")]
    async fn open_file_opens_empty_file_successfully() {
        let file = TestFile::create("empty", &[]);
        let service = DocumentService::new();
        let doc_arc = service.open_file(file.path().to_path_buf()).await.expect("open empty file");
        let doc = doc_arc.read().unwrap();
        assert!(doc.buffer.is_empty());
        assert_eq!(doc.buffer.len(), 0);
    }

    #[test]
    fn test_determine_export_format_extension_detection() {
        assert_eq!(determine_export_format(Path::new("file.mot"), FileFormat::Binary), FileFormat::MotorolaSrec);
        assert_eq!(determine_export_format(Path::new("file.hex"), FileFormat::Binary), FileFormat::IntelHex);
        assert_eq!(determine_export_format(Path::new("file.b64"), FileFormat::Binary), FileFormat::Base64);
        assert_eq!(determine_export_format(Path::new("file.bin"), FileFormat::Binary), FileFormat::Binary);
    }

    #[test]
    fn test_export_document_bytes_binary_and_base64() {
        let contents = b"Test binary data";
        let map = AddressMap::default();

        let raw = export_document_bytes(contents, &map, FileFormat::Binary);
        assert_eq!(raw, contents);

        let b64 = export_document_bytes(contents, &map, FileFormat::Base64);
        let b64_str = String::from_utf8(b64).expect("valid utf-8 base64");
        let decoded = crate::core::format::parse_base64(&b64_str).expect("decode base64");
        assert_eq!(decoded, contents);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn open_or_import_file_caches_imported_hex() {
        let hex_content = ":020000040000FA\n:0400000001020304F2\n:00000001FF\n";
        let file = TestFile::create("import_cache.hex", hex_content.as_bytes());

        let service = DocumentService::new();
        let opened = service
            .open_or_import_file(file.path().to_path_buf(), Some(FileFormat::IntelHex))
            .await
            .expect("import hex file");

        assert_eq!(opened.detected_format, FileFormat::IntelHex);
        assert_eq!(opened.document.read().unwrap().buffer.data(), &[1, 2, 3, 4]);

        let second = service
            .open_or_import_file(file.path().to_path_buf(), Some(FileFormat::IntelHex))
            .await
            .expect("re-open imported hex file");
        assert!(Arc::ptr_eq(&opened.document, &second.document));
    }

    #[test]
    fn test_retarget_document_cache_updates_keys() {
        let service = DocumentService::new();
        let old_path = PathBuf::from("/tmp/xvw_test_old.bin");
        let new_path = PathBuf::from("/tmp/xvw_test_new.bin");
        let doc = Arc::new(RwLock::new(Document::new(old_path.clone(), Buffer::new(vec![1, 2, 3]))));

        service.documents.write().unwrap().insert(old_path.clone(), doc.clone());
        assert!(service.documents.read().unwrap().contains_key(&old_path));

        service.retarget_document_cache(&old_path, new_path.clone());
        assert!(!service.documents.read().unwrap().contains_key(&old_path));
        assert!(service.documents.read().unwrap().contains_key(&new_path));
    }
}

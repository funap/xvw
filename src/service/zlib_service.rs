use crate::core::buffer::Buffer;
use crate::core::format::HexCase;
use crate::core::zlib::{ZlibError, ZlibScanResult, build_decompressed_document_path_with, decompress_zlib_at_cancellable, find_zlib_streams_segmented};
use gpui_kit::{App, Task};
use std::collections::HashSet;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// A service for asynchronously scanning and decompressing embedded zlib streams
/// and resolving collision-free virtual paths for newly created decompressed documents.
#[derive(Debug, Clone, Default)]
pub struct ZlibService;

impl ZlibService {
    /// Creates a new [`ZlibService`] instance.
    pub fn new() -> Self {
        Self
    }

    /// Scans `buffer` across `segments` on a background thread with cooperative cancellation.
    #[allow(dead_code)]
    pub fn scan_document(&self, buffer: Buffer, segments: Vec<Range<usize>>, max_results: usize, cancel: Arc<AtomicBool>, cx: &App) -> Task<ZlibScanResult> {
        self.scan_document_with(buffer, segments, max_results, cancel, |result| result, cx)
    }

    /// Scans `buffer` across `segments` on a background thread and transforms the [`ZlibScanResult`]
    /// on the same worker thread before returning to the caller.
    pub fn scan_document_with<T: Send + 'static>(
        &self,
        buffer: Buffer,
        segments: Vec<Range<usize>>,
        max_results: usize,
        cancel: Arc<AtomicBool>,
        transform: impl FnOnce(ZlibScanResult) -> T + Send + 'static,
        cx: &App,
    ) -> Task<T> {
        cx.background_executor().spawn(async move {
            let result = find_zlib_streams_segmented(buffer.data(), &segments, max_results, Some(&cancel));
            transform(result)
        })
    }

    /// Decompresses the zlib stream starting at `offset` on a background thread and returns a [`Buffer`].
    pub fn decompress_stream(
        &self,
        buffer: Buffer,
        offset: usize,
        compressed_len: usize,
        expected_decompressed_len: Option<usize>,
        cancel: Arc<AtomicBool>,
        cx: &App,
    ) -> Task<Result<Buffer, ZlibError>> {
        cx.background_executor().spawn(async move {
            let bytes = decompress_zlib_at_cancellable(buffer.data(), offset, Some(compressed_len), expected_decompressed_len, Some(&cancel))?;
            Ok(Buffer::new(bytes))
        })
    }

    /// Resolves a collision-free virtual path for a newly opened decompressed document.
    ///
    /// Normalizes open tab paths and the source parent directory once up front so candidate checks
    /// run in O(1) without repeated `canonicalize()` syscalls per candidate.
    pub fn resolve_unique_decompressed_path(&self, source_path: &Path, address: usize, hex_case: HexCase, open_paths: &[PathBuf]) -> PathBuf {
        resolve_unique_decompressed_path_with(source_path, address, hex_case, open_paths, |p| p.exists())
    }
}

fn normalize_path_with_parent(path: &Path) -> Option<PathBuf> {
    if let Ok(canonical) = path.canonicalize() {
        return Some(canonical);
    }
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty())?;
    let file_name = path.file_name()?;
    let canonical_parent = parent.canonicalize().ok()?;
    Some(canonical_parent.join(file_name))
}

fn resolve_unique_decompressed_path_with(
    source_path: &Path,
    address: usize,
    hex_case: HexCase,
    open_paths: &[PathBuf],
    exists_on_disk: impl Fn(&Path) -> bool,
) -> PathBuf {
    let mut taken: HashSet<PathBuf> = HashSet::with_capacity(open_paths.len() * 2);
    for open in open_paths {
        taken.insert(open.clone());
        if let Some(normalized) = normalize_path_with_parent(open) {
            taken.insert(normalized);
        }
    }

    let source_parent = source_path.parent().filter(|p| !p.as_os_str().is_empty());
    let canonical_parent = source_parent
        .and_then(|parent| parent.canonicalize().ok())
        .filter(|canon| Some(canon.as_path()) != source_parent);

    build_decompressed_document_path_with(source_path, address, hex_case, |candidate| {
        if taken.contains(candidate) {
            return true;
        }
        if let Some(canon_dir) = &canonical_parent
            && let Some(file_name) = candidate.file_name()
            && taken.contains(&canon_dir.join(file_name))
        {
            return true;
        }
        exists_on_disk(candidate)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zlib_service_new() {
        let service = ZlibService::new();
        let _cloned = service.clone();
        assert_eq!(format!("{service:?}"), "ZlibService");
    }

    #[test]
    fn test_resolve_unique_decompressed_path_avoids_open_and_disk_collisions() {
        let source = Path::new("/nonexistent-xvw-dir/firmware.bin");
        let first = resolve_unique_decompressed_path_with(source, 0x1040, HexCase::Lower, &[], |_| false);
        assert_eq!(first, PathBuf::from("/nonexistent-xvw-dir/firmware_zlib_0x1040.bin"));

        let open = vec![first.clone()];
        let second = resolve_unique_decompressed_path_with(source, 0x1040, HexCase::Lower, &open, |_| false);
        assert_eq!(second, PathBuf::from("/nonexistent-xvw-dir/firmware_zlib_0x1040_2.bin"));

        let third = resolve_unique_decompressed_path_with(source, 0x1040, HexCase::Lower, &open, |p| p == second.as_path());
        assert_eq!(third, PathBuf::from("/nonexistent-xvw-dir/firmware_zlib_0x1040_3.bin"));
    }
}

use crate::core::buffer::Buffer;
use crate::core::editor::Editor;
use crate::core::structure::types::ParseProgress;
use crate::core::structure::{KaitaiInterpreter, KaitaiStream, KsyDefinition, ParseResult, ParsedField};
use gpui_kit::component::Root;
use gpui_kit::{App, BackgroundExecutor, Entity};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Internal update queue item representing pending parsing fields and progress.
pub(crate) struct PendingParseUpdate {
    pub(crate) definition_id: String,
    pub(crate) fields: VecDeque<Arc<[ParsedField]>>,
    pub(crate) parsed_offset: usize,
    pub(crate) total_bytes: usize,
    pub(crate) is_done: bool,
    pub(crate) is_finalizing: bool,
    pub(crate) parse_result: Option<Arc<ParseResult>>,
}

/// A batch of fields extracted from the mailbox for incremental UI updates.
pub(crate) struct ParseUpdateBatch {
    pub(crate) definition_id: String,
    pub(crate) fields: Vec<Arc<[ParsedField]>>,
    pub(crate) parsed_offset: usize,
    pub(crate) total_bytes: usize,
    pub(crate) is_done: bool,
    pub(crate) is_finalizing: bool,
    pub(crate) parse_result: Option<Arc<ParseResult>>,
    pub(crate) has_more_fields: bool,
}

impl ParseUpdateBatch {
    pub(crate) fn discard_on_background(self, executor: &BackgroundExecutor) {
        executor
            .spawn(async move {
                drop(self);
            })
            .detach();
    }
}

/// Outcome of attempting to deliver a parse update batch to an editor view.
pub(crate) enum ParseUpdateDelivery {
    Applied { should_continue: bool, has_more_fields: bool },
    Stale(ParseUpdateBatch),
}

/// Thread-safe queue buffering parse updates from the background parser thread
/// and coalescing them for smooth foreground UI delivery.
pub(crate) struct ParseUpdateMailbox {
    pending: Mutex<Option<PendingParseUpdate>>,
    notify: tokio::sync::Notify,
    closed: AtomicBool,
}

impl ParseUpdateMailbox {
    pub(crate) fn new() -> Self {
        Self {
            pending: Mutex::new(None),
            notify: tokio::sync::Notify::new(),
            closed: AtomicBool::new(false),
        }
    }

    pub(crate) fn publish(&self, progress: ParseProgress) {
        if self.closed.load(Ordering::Acquire) {
            return;
        }

        let mut pending = self.pending.lock().expect("parse update mailbox lock");
        if self.closed.load(Ordering::Acquire) {
            return;
        }
        let update = pending.get_or_insert_with(|| PendingParseUpdate {
            definition_id: progress.definition_id.clone(),
            fields: VecDeque::new(),
            parsed_offset: progress.parsed_offset,
            total_bytes: progress.total_bytes,
            is_done: false,
            is_finalizing: false,
            parse_result: None,
        });

        update.definition_id = progress.definition_id;
        update.parsed_offset = progress.parsed_offset;
        update.total_bytes = progress.total_bytes;
        update.is_done = progress.is_done;
        update.is_finalizing = progress.is_finalizing;
        if !progress.fields.is_empty() {
            update.fields.push_back(progress.fields);
        }
        if progress.parse_result.is_some() {
            update.parse_result = progress.parse_result;
            // The final result contains every field. Discarding queued partial
            // chunks prevents stale intermediate work from delaying completion.
            update.fields.clear();
        }

        drop(pending);
        self.notify.notify_one();
    }

    pub(crate) fn close(&self) {
        self.closed.store(true, Ordering::Release);
        self.notify.notify_one();
    }

    pub(crate) fn close_and_discard(&self, executor: &BackgroundExecutor) {
        self.closed.store(true, Ordering::Release);
        let pending = self.pending.lock().expect("parse update mailbox lock").take();
        if let Some(pending) = pending {
            executor
                .spawn(async move {
                    drop(pending);
                })
                .detach();
        }
        self.notify.notify_one();
    }

    pub(crate) fn take_batch(&self, max_fields: usize) -> Option<ParseUpdateBatch> {
        let mut pending = self.pending.lock().expect("parse update mailbox lock");
        let update = pending.as_mut()?;

        if update.parse_result.is_some() {
            let update = pending.take().expect("pending parse update");
            return Some(ParseUpdateBatch {
                definition_id: update.definition_id,
                fields: Vec::new(),
                parsed_offset: update.parsed_offset,
                total_bytes: update.total_bytes,
                is_done: update.is_done,
                is_finalizing: update.is_finalizing,
                parse_result: update.parse_result,
                has_more_fields: false,
            });
        }

        let mut fields = Vec::new();
        let mut field_count = 0;
        while let Some(chunk) = update.fields.front()
            && (fields.is_empty() || field_count + chunk.len() <= max_fields)
        {
            let chunk = update.fields.pop_front().expect("parse field chunk");
            field_count += chunk.len();
            fields.push(chunk);
        }

        let has_more_fields = !update.fields.is_empty();
        let batch = ParseUpdateBatch {
            definition_id: update.definition_id.clone(),
            fields,
            parsed_offset: update.parsed_offset,
            total_bytes: update.total_bytes,
            is_done: update.is_done,
            is_finalizing: update.is_finalizing,
            parse_result: None,
            has_more_fields,
        };

        if !has_more_fields {
            pending.take();
        }
        Some(batch)
    }
}

/// Service managing asynchronous structure parsing, progress streaming,
/// and background object lifecycle management.
#[derive(Clone, Default)]
pub struct StructureService;

impl StructureService {
    /// Creates a new `StructureService`.
    pub fn new() -> Self {
        Self
    }

    /// Safely discards heavy AST/field data structures on a background thread pool.
    #[allow(dead_code)]
    pub fn discard_in_background<T: Send + 'static>(&self, value: T) {
        crate::core::dealloc::discard_in_background(value);
    }

    /// Spawns background structure parsing with incremental progress streaming.
    #[allow(dead_code)]
    pub fn parse_structure_async(
        &self,
        buffer: Buffer,
        definition: Arc<KsyDefinition>,
        cancel_token: Arc<AtomicBool>,
        on_progress: impl Fn(ParseProgress) + Send + 'static,
        on_complete: impl FnOnce() + Send + 'static,
    ) -> std::thread::JoinHandle<()> {
        std::thread::Builder::new()
            .name("kaitai-parser".into())
            .spawn(move || {
                let mut stream = KaitaiStream::new(buffer.data());
                let interpreter = KaitaiInterpreter::new((*definition).clone());
                let token_for_closure = cancel_token.clone();
                interpreter.parse_with_progress_cancellable(&mut stream, Some(&cancel_token), move |progress| {
                    if !token_for_closure.load(Ordering::Relaxed) {
                        on_progress(progress.clone());
                    }
                });
                on_complete();
            })
            .expect("Failed to spawn kaitai parser thread")
    }

    /// Initiates asynchronous structure parsing for the given editor entity,
    /// managing cancellation, incremental UI delivery batches, and error reporting.
    pub fn start_parse(&self, editor_entity: &Entity<Editor>, ksy: Arc<KsyDefinition>, cx: &mut App) {
        let cancel_token = Arc::new(AtomicBool::new(false));
        let cancel_token_clone = cancel_token.clone();

        let (doc_arc, doc_path, generation) = editor_entity.update(cx, |editor, cx| {
            let total = editor.document.read().expect("document read lock").buffer.len();
            let path = editor.document.read().ok().map(|d| d.path().to_path_buf());
            editor.structure.start_async_parse(total, cancel_token.clone());
            editor.set_ksy_definition(ksy.clone());
            editor.begin_partial_parse_result(ksy.meta.id.clone());
            editor.invalidate_line_map();
            cx.notify();
            (editor.document.clone(), path, editor.structure.generation)
        });

        if let Some(ref path) = doc_path {
            let service = crate::app_state::AppState::global(cx).document_service.clone();
            service.notify_document_changed(path, cx);
        }

        let mailbox = Arc::new(ParseUpdateMailbox::new());
        let producer_mailbox = mailbox.clone();

        let buffer = { if let Ok(doc) = doc_arc.read() { doc.buffer.clone() } else { Buffer::empty() } };

        let progress_mailbox = producer_mailbox.clone();
        self.parse_structure_async(
            buffer,
            ksy,
            cancel_token_clone,
            move |progress| {
                progress_mailbox.publish(progress);
            },
            move || {
                producer_mailbox.close();
            },
        );

        let editor_entity = editor_entity.clone();
        cx.spawn(async move |cx| {
            const MAX_FIELDS_PER_UPDATE: usize = 1024;

            loop {
                let notified = mailbox.notify.notified();
                let Some(batch) = mailbox.take_batch(MAX_FIELDS_PER_UPDATE) else {
                    if mailbox.closed.load(Ordering::Acquire) {
                        break;
                    }
                    notified.await;
                    continue;
                };

                let mut batch = Some(batch);
                let delivery = editor_entity.update(cx, |editor, cx| {
                    let batch = batch.take().expect("parse update batch must be present");
                    if editor.structure.generation != generation {
                        return ParseUpdateDelivery::Stale(batch);
                    }
                    if !editor.structure.is_parsing && !batch.is_done {
                        return ParseUpdateDelivery::Stale(batch);
                    }
                    let is_done = batch.is_done;
                    let has_more_fields = batch.has_more_fields;
                    editor.structure.progress_offset = batch.parsed_offset;
                    editor.structure.total_size = batch.total_bytes;
                    editor.structure.is_finalizing = batch.is_finalizing;
                    if let Some(res) = batch.parse_result {
                        editor.set_parse_result_arc(res);
                    } else if !batch.fields.is_empty() {
                        editor.append_parse_chunks(batch.definition_id, batch.fields, batch.parsed_offset, batch.total_bytes);
                    }
                    if is_done {
                        editor.structure.finish_parse();
                        editor.structure.cancel_token = None;

                        if let Some(ref res) = editor.parse_result()
                            && let Some(err) = res.errors.first()
                        {
                            let msg = format!("Structure parse error at offset 0x{:08X}: {}", err.offset, err.message);
                            if let Some(window) = cx.active_window()
                                && let Some(window) = window.downcast::<Root>()
                            {
                                let _ = window.update(cx, |root, window, cx| {
                                    let note = gpui_kit::component::notification::Notification::error(msg);
                                    root.notification.update(cx, |view, cx| view.push(note, window, cx));
                                    cx.notify();
                                });
                            }
                        }
                    }
                    cx.notify();
                    ParseUpdateDelivery::Applied {
                        should_continue: !is_done,
                        has_more_fields,
                    }
                });

                let (should_continue, has_more_fields) = match delivery {
                    ParseUpdateDelivery::Applied {
                        should_continue,
                        has_more_fields,
                    } => (should_continue, has_more_fields),
                    ParseUpdateDelivery::Stale(stale_batch) => {
                        let executor = cx.background_executor();
                        mailbox.close_and_discard(executor);
                        stale_batch.discard_on_background(executor);
                        break;
                    }
                };

                if !should_continue {
                    break;
                }

                if has_more_fields || should_continue {
                    cx.background_executor().timer(std::time::Duration::from_millis(16)).await;
                }
            }
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_update_mailbox_batching_and_coalescing() {
        let mailbox = ParseUpdateMailbox::new();

        // 1. Initial empty mailbox
        assert!(mailbox.take_batch(100).is_none());

        // 2. Publish progress with some fields
        let progress = ParseProgress {
            definition_id: "test_def".into(),
            fields: Arc::from(vec![].into_boxed_slice()),
            parsed_offset: 50,
            total_bytes: 100,
            is_done: false,
            is_finalizing: false,
            errors: Vec::new(),
            parse_result: None,
        };
        mailbox.publish(progress);

        let batch = mailbox.take_batch(100).expect("batch should be available");
        assert_eq!(batch.definition_id, "test_def");
        assert_eq!(batch.parsed_offset, 50);
        assert_eq!(batch.total_bytes, 100);
        assert!(!batch.is_done);

        // 3. Mailbox is drained
        assert!(mailbox.take_batch(100).is_none());

        // 4. Publish completion
        mailbox.publish(ParseProgress {
            definition_id: "test_def".into(),
            fields: Arc::from(vec![].into_boxed_slice()),
            parsed_offset: 100,
            total_bytes: 100,
            is_done: true,
            is_finalizing: false,
            errors: Vec::new(),
            parse_result: Some(Arc::new(ParseResult::empty("test_def".into()))),
        });

        let final_batch = mailbox.take_batch(100).expect("final batch should be available");
        assert!(final_batch.is_done);
        assert!(final_batch.parse_result.is_some());

        // 5. Close mailbox
        mailbox.close();
        assert!(mailbox.closed.load(Ordering::Acquire));
    }

    #[test]
    fn test_structure_service_discard() {
        let service = StructureService::new();
        let heavy_data = vec![0u8; 1024];
        service.discard_in_background(heavy_data);
    }
}

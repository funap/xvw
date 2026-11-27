use super::MAX_PROGRESS_FIELDS;
use crate::core::structure::types::{ParseProgress, ParsedField, StructureIndexBuilder};
use std::sync::Arc;

pub(super) struct ProgressTracker<'a> {
    pub(super) def_id: &'a str,
    pub(super) total_bytes: usize,
    pub(super) last_notify_time: std::time::Instant,
    pub(super) last_notify_offset: usize,
    pub(super) pending_fields: Vec<ParsedField>,
    pub(super) index_builder: Option<StructureIndexBuilder>,
    pub(super) on_progress: &'a mut dyn FnMut(&ParseProgress),
}

impl<'a> ProgressTracker<'a> {
    pub(super) fn record_root_field(&mut self, field: &ParsedField) {
        if let Some(index_builder) = self.index_builder.as_mut() {
            index_builder.add_field(field);
        }
        if self.index_builder.is_some() {
            self.pending_fields.push(field.clone());
        }
    }

    pub(super) fn maybe_notify(&mut self, current_offset: usize) {
        let now = std::time::Instant::now();
        let current_offset = current_offset.min(self.total_bytes);
        if now.duration_since(self.last_notify_time).as_millis() >= 50
            || current_offset.saturating_sub(self.last_notify_offset) >= 65536
            || self.pending_fields.len() >= MAX_PROGRESS_FIELDS
        {
            self.flush(current_offset, now);
        }
    }

    pub(super) fn flush_if_pending(&mut self, current_offset: usize) {
        if !self.pending_fields.is_empty() {
            let current_offset = current_offset.min(self.total_bytes);
            self.flush(current_offset, std::time::Instant::now());
        }
    }

    fn flush(&mut self, current_offset: usize, now: std::time::Instant) {
        self.last_notify_time = now;
        self.last_notify_offset = current_offset;
        (self.on_progress)(&ParseProgress {
            definition_id: self.def_id.to_string(),
            fields: into_field_chunk(std::mem::take(&mut self.pending_fields)),
            parsed_offset: current_offset,
            total_bytes: self.total_bytes,
            is_done: false,
            is_finalizing: false,
            errors: Vec::new(),
            parse_result: None,
        });
    }
}

#[derive(Clone, Copy)]
pub(super) struct ProgressOrigin {
    pub(super) abs_base: usize,
    pub(super) abs_limit: Option<usize>,
    pub(super) emit_root_fields: bool,
}

impl ProgressOrigin {
    pub(super) fn root(emit_root_fields: bool) -> Self {
        Self {
            abs_base: 0,
            abs_limit: None,
            emit_root_fields,
        }
    }

    pub(super) fn resolve_offset(self, stream_pos: u64) -> usize {
        let raw = self.abs_base.saturating_add(stream_pos as usize);
        self.abs_limit.map_or(raw, |lim| raw.min(lim))
    }
}

pub(super) enum ArrayRunningMinMax {
    Uninit,
    Strings { min: String, max: String },
    Ints { min: i64, max: i64 },
}

pub(super) fn into_field_chunk(fields: Vec<ParsedField>) -> Arc<[ParsedField]> {
    Arc::from(fields.into_boxed_slice())
}

pub(super) fn empty_field_chunk() -> Arc<[ParsedField]> {
    Arc::from(Vec::<ParsedField>::new().into_boxed_slice())
}

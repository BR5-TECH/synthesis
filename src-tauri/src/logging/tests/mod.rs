//! The test scenarios of `specifications/core/LGC-logging-console.md`.
//!
//! Every test drives the buffer directly, so no Tauri runtime is needed. This
//! module holds the recording sink and the input builders; the topic modules
//! hold the tests.

use super::*;
use std::sync::{Arc, Mutex as StdMutex};

/// A `LogSink` that records what it was handed, so the count and ordering of
/// delivered events are assertable without a Tauri runtime.
#[derive(Clone, Default)]
struct Recorder {
    events: Arc<StdMutex<Vec<BufferState>>>,
}

impl LogSink for Recorder {
    fn publish(&self, state: &BufferState) {
        self.events.lock().unwrap().push(*state);
    }
}

impl Recorder {
    fn len(&self) -> usize {
        self.events.lock().unwrap().len()
    }
    fn last(&self) -> BufferState {
        *self.events.lock().unwrap().last().unwrap()
    }
}

fn t0() -> Instant {
    Instant::now()
}

fn after(base: Instant, millis: u64) -> Instant {
    base + Duration::from_millis(millis)
}

fn input(level: LogLevel, domains: &[Domain], message: &str) -> LogInput {
    LogInput {
        ts: "2026-08-04T12:00:00.000Z".to_string(),
        level,
        domains: domains.to_vec(),
        message: message.to_string(),
        fields: Fields::new(),
    }
}

fn with_fields(mut i: LogInput, pairs: &[(&str, serde_json::Value)]) -> LogInput {
    for (k, v) in pairs {
        i.fields.insert((*k).to_string(), v.clone());
    }
    i
}

/// Append outside the coalescing window every time, so a test that is not
/// about coalescing never trips over it.
fn append(buffer: &LogBuffer, inputs: Vec<LogInput>) {
    buffer.append_at(inputs, Instant::now() + COALESCE_WINDOW * 100);
}

fn all(buffer: &LogBuffer) -> LogPage {
    buffer
        .query(&LogFilter::default(), None, BUFFER_CAPACITY)
        .unwrap()
}

mod payload_shape;
mod ordering;
mod eviction;
mod filters;
mod coalescing;
mod export;
mod paging;

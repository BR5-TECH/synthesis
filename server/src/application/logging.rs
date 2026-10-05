//! The diagnostic channel of the service.
//!
//! Specification: `specifications/server/SAS-server-application-service.md`
//! (the non-functional record of the route, the status, and the identifiers of
//! the records a request touched) and
//! `specifications/server/SRB-server-relay-boundary.md` SRB-FR-IZAB (the relay
//! record of the connection identifier, the `instance_id`, and the operation).
//!
//! One line of JSON for one record, written to standard output, which is where
//! `BMS-backend-microservice.md` BMS-FR-09 already writes the record of the
//! bound listener. The crate takes no logging dependency for this: a record is
//! a `serde_json` document, and the sink is one trait with two implementations.
//!
//! **This module redacts nothing.** It writes the fields it is given. Keeping a
//! token, a handle, a key, an email address, Draft content, and a Conversation
//! message body out of a record is the obligation of the caller, and every
//! caller in the crate builds a record from identifiers alone
//! (SAS-FR-PMRB, SRB-FR-IZAB).

use std::io::{self, Write};
use std::sync::{Arc, Mutex};

use serde_json::Value;

/// Where a record goes.
///
/// The sink is an outbound port like every other: the service holds the trait,
/// the process holds [`StdoutSink`], and a test holds [`CaptureSink`] and reads
/// back what the code under test emitted.
pub trait LogSink: Send + Sync {
    /// Writes one record.
    ///
    /// A sink never fails the operation that emitted the record: a diagnostic
    /// that cannot be written is dropped rather than raised.
    fn write(&self, record: Value);
}

/// The sink of the running process: one line of JSON on standard output.
#[derive(Debug, Default)]
pub struct StdoutSink;

impl LogSink for StdoutSink {
    fn write(&self, record: Value) {
        // `writeln!` rather than `println!`: a collector that closed the pipe
        // must not turn a served request into a panic. The same rule the
        // process records of `main.rs` follow.
        //
        // The write is blocking, and a request record is written from the task
        // that served the request. One line to a pipe is cheap, and a V1
        // instance serves tens of requests rather than thousands; a service for
        // a fleet would move the write to a channel.
        let mut stdout = io::stdout().lock();
        let _ = writeln!(stdout, "{record}");
        let _ = stdout.flush();
    }
}

/// The sink of a test: the records are held in memory and read back.
#[derive(Debug, Default)]
pub struct CaptureSink {
    records: Mutex<Vec<Value>>,
}

impl CaptureSink {
    /// A sink that holds no record.
    pub fn new() -> Self {
        CaptureSink::default()
    }

    /// Every record the sink has taken, in the order it took them.
    pub fn records(&self) -> Vec<Value> {
        self.records
            .lock()
            .expect("the capture sink is not poisoned")
            .clone()
    }

    /// Every record of one event, in the order the sink took them.
    pub fn records_of(&self, event: &str) -> Vec<Value> {
        self.records()
            .into_iter()
            .filter(|record| record["event"] == event)
            .collect()
    }

    /// The records as one text, which is what a leak test reads.
    pub fn text(&self) -> String {
        self.records()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Drops every record the sink holds.
    pub fn clear(&self) {
        self.records
            .lock()
            .expect("the capture sink is not poisoned")
            .clear();
    }
}

impl LogSink for CaptureSink {
    fn write(&self, record: Value) {
        self.records
            .lock()
            .expect("the capture sink is not poisoned")
            .push(record);
    }
}

/// The sink the running service holds.
pub fn stdout_sink() -> Arc<dyn LogSink> {
    Arc::new(StdoutSink)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // The capture sink reports the records in the order it took them, which is
    // what the request and relay tests read.
    #[test]
    fn the_capture_sink_holds_the_records_in_order() {
        let sink = CaptureSink::new();
        sink.write(json!({"event": "relay", "operation": "register_worker"}));
        sink.write(json!({"event": "request", "route": "/v1/users"}));

        let records = sink.records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0]["operation"], "register_worker");
        assert_eq!(records[1]["route"], "/v1/users");
        assert_eq!(sink.records_of("request").len(), 1);
    }

    // One record is one line, so a collector reads a record per line.
    #[test]
    fn a_record_serialises_to_one_line() {
        let record = json!({"event": "request", "route": "/v1/users", "status": 201});
        assert!(!record.to_string().contains('\n'));
    }
}

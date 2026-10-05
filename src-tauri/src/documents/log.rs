//! Where the collection's log records go.
//!
//! The pure core of this module has no Tauri handle, so it reports through this
//! small trait. The application implements it with the `AppHandle`, which puts
//! every record into the session log under the `backend` domain (DCL-FR-UPFP);
//! a test implements it with a recorder and reads the records back.
//!
//! A record names a document by its id and its format, and a source by its kind.
//! It never carries document bytes, text, a file name, a path, or a query
//! (DCL-FR-TAGV), because nothing downstream redacts a record.

use crate::logging::{Domain, Fields, LogLevel, BUFFER};

/// The sink the collection logs into.
pub trait DocLog {
    fn log(&self, level: LogLevel, message: &str, fields: Fields);
}

impl<R: tauri::Runtime> DocLog for tauri::AppHandle<R> {
    fn log(&self, level: LogLevel, message: &str, fields: Fields) {
        crate::logging::log(self, &BUFFER, level, &[Domain::Backend], message, fields);
    }
}

/// A sink that drops every record, for callers that have nothing to report to.
pub struct NullLog;

impl DocLog for NullLog {
    fn log(&self, _level: LogLevel, _message: &str, _fields: Fields) {}
}

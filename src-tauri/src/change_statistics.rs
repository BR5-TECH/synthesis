//! Change statistics (`CST-change-statistics.md`).
//!
//! The log of one OpenSpec change is `changes/<stream-id>/<change>/statistics.jsonl`
//! in the repository machine store (CST-FR-KGGT). Its lines have the envelope
//! of `DSS-draft-statistics-storage.md` with `streamId` and `change` in place of
//! `draftId` (CST-FR-WSQI). The fold is DSS's own fold: each line is read as the
//! draft event it mirrors, so every rule of DSS-FR-GAWO through DSS-FR-DZQF
//! applies unchanged (CST-FR-NCLF).

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::fs as fsa;
use crate::log_fields;
use crate::logging::{self, Domain, LogSink, BUFFER};
use crate::notes::new_note_id;
use crate::statistics::{
    fold::fold_log, time::parse_instant_ms, EventBody, Measure, Pending, TokenPair, SCHEMA_VERSION,
};

/// CST-FR-PSUF.
pub const CHANGE_STATISTICS_CHANGED: &str = "change-statistics-changed";

pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
pub const ERR_INVALID_CHANGE_NAME: &str = "invalid_change_name";
pub const ERR_INVALID_INTERVAL: &str = "invalid_interval";
pub const ERR_READ_FAILED: &str = "read_failed";

/// The wire spelling of a working interval (CST-FR-WSQI).
const WORKING_INTERVAL: &str = "working_interval";

/// CST-FR-PSUF: the payload of the event.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeStatisticsChanged {
    pub stream_id: String,
    pub change: String,
}

/// CST contract surface `ChangeStatistics`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeStatistics {
    pub stream_id: String,
    pub change: String,
    pub boundary_at: Option<String>,
    pub working_time_ms: Measure,
    pub ai_interactions: Measure,
    pub versions: Measure,
    pub accepted_proposals: Measure,
    pub rejected_proposals: Measure,
    pub conversation_tokens: TokenPair,
    pub unreadable_lines: u32,
}

/// CST-FR-KGGT: the log of one change, behind the escape gate. Both parts are
/// checked before they become a path.
pub fn log_path(
    store: &fsa::RootFs,
    stream_id: &str,
    change: &str,
) -> Result<std::path::PathBuf, String> {
    if !crate::comments::is_change_owner(stream_id, change) {
        return Err(ERR_INVALID_CHANGE_NAME.to_string());
    }
    fsa::resolve_under(
        store.path(),
        format!(
            "{}/{stream_id}/{change}/statistics.jsonl",
            crate::repository_store::CHANGES_DIRNAME
        ),
    )
    .map_err(|e| e.to_string())
}

/// One line in the change's envelope (CST-FR-WSQI).
fn line_of(stream_id: &str, change: &str, at: &str, body: &EventBody) -> Result<String, String> {
    let mut value = serde_json::to_value(body).map_err(|e| e.to_string())?;
    let map = value.as_object_mut().ok_or("event is not an object")?;
    if map.get("type").and_then(|t| t.as_str()) == Some("editing_interval") {
        map.insert("type".into(), serde_json::json!(WORKING_INTERVAL));
    }
    map.insert("v".into(), serde_json::json!(SCHEMA_VERSION));
    map.insert("eventId".into(), serde_json::json!(new_note_id()));
    map.insert("streamId".into(), serde_json::json!(stream_id));
    map.insert("change".into(), serde_json::json!(change));
    map.insert("at".into(), serde_json::json!(at));
    serde_json::to_string(&value).map_err(|e| e.to_string())
}

/// CST-FR-NCLF: a change line read as the draft line it mirrors, for DSS's fold.
/// A line that does not parse is passed through as it is, so the fold counts it
/// unreadable on its own terms.
fn as_draft_line(line: &str) -> String {
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(line) else {
        return line.to_string();
    };
    let Some(map) = value.as_object_mut() else {
        return line.to_string();
    };
    if map.get("type").and_then(|t| t.as_str()) == Some(WORKING_INTERVAL) {
        map.insert("type".into(), serde_json::json!("editing_interval"));
    }
    map.remove("streamId");
    map.remove("change");
    map.insert("draftId".into(), serde_json::json!("change"));
    serde_json::to_string(&value).unwrap_or_else(|_| line.to_string())
}

/// CST-FR-QJGO: the synchronous half of the recording path. It writes the
/// activation boundary with the first events, on the terms of DSS-FR-VCTQ.
pub fn append_change_events(
    store: &fsa::RootFs,
    stream_id: &str,
    change: &str,
    pending: &[Pending],
) -> Result<(), String> {
    if pending.is_empty() {
        return Ok(());
    }
    let path = log_path(store, stream_id, change)?;
    let mut lines = Vec::with_capacity(pending.len() + 1);
    if store.file_info(&path).is_err() {
        let boundary = pending.iter().map(|p| p.at.as_str()).min().unwrap_or_default().to_string();
        lines.push(line_of(stream_id, change, &boundary, &EventBody::TelemetryActivated)?);
    }
    for event in pending {
        lines.push(line_of(stream_id, change, &event.at, &event.body)?);
    }
    store.append_lines(&path, &lines).map_err(|e| e.to_string())
}

/// CST-FR-WPHC / CST-FR-PUAN: fold the change's log, with the version count
/// read beside it.
pub fn read_change_statistics_impl(
    store: &fsa::RootFs,
    stream_id: &str,
    change: &str,
    versions: Result<usize, String>,
) -> Result<ChangeStatistics, String> {
    let path = log_path(store, stream_id, change)?;
    let text = match store.read_text(&path) {
        Ok(text) => text,
        Err(fsa::FsError::NotFound { .. }) => String::new(),
        Err(_) => return Err(ERR_READ_FAILED.to_string()),
    };
    let mirrored: String = text.lines().map(as_draft_line).collect::<Vec<_>>().join("\n");
    let folded = fold_log("change", &mirrored);
    let totals = folded.totals;
    let versions = match versions {
        Ok(count) => Measure::of(count as u64, crate::statistics::Availability::Available),
        Err(_) => Measure::unavailable(),
    };
    Ok(ChangeStatistics {
        stream_id: stream_id.to_string(),
        change: change.to_string(),
        boundary_at: folded.boundary_at,
        working_time_ms: totals.editing_time_ms,
        ai_interactions: totals.ai_interactions,
        versions,
        accepted_proposals: totals.accepted_proposals,
        rejected_proposals: totals.rejected_proposals,
        conversation_tokens: totals.conversation_tokens,
        unreadable_lines: folded.unreadable_lines,
    })
}

/// CST-FR-QJGO / CST-FR-PSUF: record events for a change on the statistics
/// writer, and announce them once they landed. The caller never waits.
pub fn record_change_statistics_events<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
    pending: Vec<Pending>,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    if pending.is_empty() {
        return;
    }
    let Some(store) = app
        .try_state::<crate::project::ProjectState>()
        .and_then(|state| state.require_store().ok())
    else {
        return;
    };
    let sink = app.clone();
    let (stream_id, change) = (stream_id.to_string(), change.to_string());
    crate::statistics::enqueue(Box::new(move || {
        let types = pending.iter().map(|e| e.body.type_name()).collect::<Vec<_>>().join(",");
        match append_change_events(&store, &stream_id, &change, &pending) {
            Ok(()) => {
                logging::log_debug(
                    &sink,
                    &BUFFER,
                    &[Domain::Backend],
                    "change statistics events appended",
                    log_fields! {
                        "streamId" => &stream_id,
                        "change" => &change,
                        "events" => pending.len(),
                        "types" => &types,
                    },
                );
                let _ = sink.emit(
                    CHANGE_STATISTICS_CHANGED,
                    ChangeStatisticsChanged {
                        stream_id: stream_id.clone(),
                        change: change.clone(),
                    },
                );
            }
            Err(reason) => logging::log_warn(
                &sink,
                &BUFFER,
                &[Domain::Backend],
                "change statistics append dropped",
                log_fields! {
                    "streamId" => &stream_id,
                    "change" => &change,
                    "events" => pending.len(),
                    "types" => &types,
                    "reason" => reason,
                },
            ),
        }
    }));
}

/// [`record_change_statistics_events`] for one event.
pub fn record_change_statistics_event<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
    body: EventBody,
) where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    record_change_statistics_events(app, stream_id, change, vec![Pending::now(body)]);
}

fn require_store<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<fsa::RootFs, String> {
    let project = app
        .try_state::<crate::project::ProjectState>()
        .ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    project.require_store()
}

/// CST-FR-WPHC / CST-FR-BLIF: `"read change statistics"`.
#[tauri::command(async)]
pub fn read_change_statistics<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    change: String,
) -> Result<ChangeStatistics, String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    let store = require_store(&app)?;
    if !crate::openspec::change::valid_change_name(&change) {
        return Err(ERR_INVALID_CHANGE_NAME.to_string());
    }
    let versions = crate::change_history::count_change_versions(&stream_id, &change);
    read_change_statistics_impl(&store, &stream_id, &change, versions).inspect_err(|reason| {
        logging::log_warn(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "change statistics read refused",
            log_fields! { "streamId" => &stream_id, "change" => &change, "reason" => reason },
        )
    })
}

/// CST-FR-TSOU: `"record change working interval"`.
#[tauri::command]
pub fn record_change_working_interval<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    stream_id: String,
    change: String,
    started_at: String,
    ended_at: String,
) -> Result<(), String>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    require_store(&app)?;
    if !crate::comments::is_change_owner(&stream_id, &change) {
        return Err(ERR_INVALID_CHANGE_NAME.to_string());
    }
    let (Some(started), Some(ended)) = (parse_instant_ms(&started_at), parse_instant_ms(&ended_at))
    else {
        return Err(ERR_INVALID_INTERVAL.to_string());
    };
    if ended <= started {
        return Err(ERR_INVALID_INTERVAL.to_string());
    }
    record_change_statistics_event(
        &app,
        &stream_id,
        &change,
        EventBody::EditingInterval { interval_id: new_note_id(), started_at, ended_at },
    );
    Ok(())
}

#[cfg(test)]
mod tests;

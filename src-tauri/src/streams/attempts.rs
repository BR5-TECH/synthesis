//! The life of one reconciliation attempt's scratch material (GRB-FR-UHFE,
//! WKS-FR-LWEI, WKS-FR-QFTH).
//!
//! An attempt owns a checkout, a scratch branch and a registered worktree. All
//! three go when the attempt completes, whatever it decided, and what a killed
//! process stranded is reclaimed at the next listing.

use tauri::Manager;

use super::artifact::MergeArtifact;
use super::*;
use crate::log_fields;
use crate::logging::{self, Domain};

/// The artifact and its checkout go when the attempt completes, whatever it
/// decided.
pub(super) fn cleanup<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    fs: &fsa::FsAccess,
    stream: &WorkStream,
    artifact: &MergeArtifact,
) {
    let _ = stream;
    let Ok(root) = commands::project_root(app) else {
        return;
    };
    let Ok(main) = git::primary_repo(&root) else {
        return;
    };
    semantic::release(fs, &main, artifact);
}

/// GRB-FR-UHFE: reclaim what an update the application never finished left behind.
///
/// Every graceful exit of [`run`] reclaims its own attempt, but a process that
/// is killed while a turn is executing cannot. What it strands is a scratch
/// branch, a registered worktree and a whole checkout under the store, none of
/// which any later merge would ever look at again — so they are removed at the
/// first listing of the project's streams, on the terms `WKS-FR-LWEI` releases
/// a stream the application stopped while holding.
///
/// An attempt this process is running is left alone: it is the live one.
pub(super) fn sweep_stranded_attempts<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Ok(fs) = commands::store_fs(app) else {
        return;
    };
    let Ok(store) = commands::store(app) else {
        return;
    };
    let Ok(root) = commands::project_root(app) else {
        return;
    };
    let Ok(main) = git::primary_repo(&root) else {
        return;
    };
    let live: Vec<String> = app
        .try_state::<StreamState>()
        .map(|state| state.attempts_running())
        .unwrap_or_default();

    let attempts_root = store.root().join(artifact::ATTEMPTS_DIR);
    let stranded: Vec<String> = fs
        .list_dir(&attempts_root)
        .unwrap_or_default()
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| artifact::is_attempt_id(name) && !live.contains(name))
        .collect();

    for attempt_id in stranded {
        let artifact = MergeArtifact::new(&store.root(), &attempt_id);
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "work stream update reclaimed an attempt the application never finished",
            log_fields! { "attempt_id" => attempt_id.clone() },
        );
        semantic::release(&fs, &main, &artifact);
    }
}

/// What marks one attempt live, for as long as it runs.
pub(super) struct AttemptMark<'a> {
    pub(super) state: &'a StreamState,
    pub(super) attempt_id: String,
}

impl Drop for AttemptMark<'_> {
    fn drop(&mut self) {
        self.state.end_attempt(&self.attempt_id);
    }
}

/// WKS-FR-QFTH: recover the updates the application never finished.
///
/// An update lives on a thread and its record on disk, so a
/// process that stopped mid-update leaves a record claiming to be running with
/// nothing behind it. Neither branch moved — an update writes the stream branch
/// only after a turn has settled every path (GRB-FR-TXVL).
pub(super) fn sweep_interrupted_updates<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let (Ok(fs), Ok(store)) = (commands::store_fs(app), commands::store(app)) else {
        return;
    };
    let key = commands::project_key(app);
    let state = app.try_state::<StreamState>();
    let live = |stream_id: &str| {
        state
            .as_ref()
            .map(|state| state.has_update_job(stream_id) || state.is_updating(stream_id))
            .unwrap_or(false)
    };
    let _write = state.as_ref().map(|state| state.write_guard());
    for record in update_record::interrupted_records(&fs, &store, &key, live) {
        // Read again under the hold: an update that settled in between has
        // already written what it settled.
        if !update_record::read_update(&fs, &store, &record.stream_id)
            .map(|current| current.state.is_running())
            .unwrap_or(false)
        {
            continue;
        }
        if update_record::write_update(&fs, &store, &record).is_err() {
            continue;
        }
        logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[Domain::Backend],
            "work stream update was interrupted by a stopped application",
            log_fields! {
                "stream_id" => record.stream_id.clone(),
                "attempt_id" => record.attempt_id.clone(),
            },
        );
    }
}

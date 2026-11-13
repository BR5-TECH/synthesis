//! The changes a stream merge archives, read from the run records
//! (`OAR-openspec-archive.md` OAR-FR-KGDP, OAR-FR-SURB).
//!
//! A change is a candidate where a completed apply run of the stream names it,
//! and that run has no revert on the stream branch and no landing on the base
//! branch. The caller checks that the change folder stands in the working
//! copy. The read opens the records of the stream's completed runs alone,
//! found by the run order's index.

use super::*;

/// OAR-FR-KGDP: one completed apply run, as the archive step reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArchiveCandidate {
    pub change: String,
    pub run_id: String,
    /// When the run completed: RFC 3339 UTC.
    pub completed_at: String,
}

/// When `run` completed: the moment of its last transition to the `done`
/// stage, or its last write where its record holds none.
fn completed_at(run: &GraduationRun) -> String {
    run.observability
        .stage_history
        .iter()
        .rev()
        .find(|t| t.to == observability::GraduationVisualStage::Done)
        .map(|t| t.at.clone())
        .unwrap_or_else(|| run.updated_at.clone())
}

/// OAR-FR-KGDP, OAR-FR-SURB: the candidates of a stream, one for each change,
/// in the order in which the first such apply run of each change completed,
/// oldest first. Two runs of one moment keep the project's run order.
pub fn stream_archive_candidates<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    stream_branch: &str,
    base_branch: &str,
) -> Vec<ArchiveCandidate> {
    let (Ok(fs), Ok(base)) = (store::store_fs(app), store::store_base(app)) else {
        return Vec::new();
    };
    let index = store::read_queue_index(&fs, &base, &queue::project_key(app));
    let mut found: Vec<ArchiveCandidate> = Vec::new();
    for entry in index.runs.iter().filter(|entry| {
        entry.stream_id == stream_id
            && entry.state == GraduationRunState::Completed
            && !entry.merge
            && !entry.pr_summary
    }) {
        let Ok(run) = runs::load_run(app, &entry.run_id) else {
            logging::log_warn(
                app,
                &BUFFER,
                &[Domain::Backend],
                "openspec archive could not read a completed run",
                log_fields! { "run_id" => entry.run_id.clone() },
            );
            continue;
        };
        if !run.is_change_apply()
            || run.landing_on(base_branch).is_some()
            || run.reverts.iter().any(|revert| revert.branch == stream_branch)
        {
            continue;
        }
        let Some(data) = run.openspec.as_ref() else {
            continue;
        };
        found.push(ArchiveCandidate {
            change: data.change.clone(),
            run_id: run.id.clone(),
            completed_at: completed_at(&run),
        });
    }
    // The times are compared as instants, not as text: a stamp with a
    // fraction of a second sorts before the same second with none as text.
    // A time that does not read sorts last. A stable sort keeps the run order
    // for two runs of one moment.
    found.sort_by_key(|candidate| {
        crate::statistics::time::parse_instant_ms(&candidate.completed_at).unwrap_or(i64::MAX)
    });
    let mut seen = std::collections::HashSet::new();
    found.retain(|candidate| seen.insert(candidate.change.clone()));
    found
}

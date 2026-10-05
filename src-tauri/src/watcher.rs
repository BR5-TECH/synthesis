//! Filesystem watching + content/structural change routing
//! (ASC-FR-10/14/15, PST-FR-16).
//!
//! The live recursive watcher for the open project lives here, along with the
//! pure routing (`route_watch_change` / `external_divergence`) that decides, for
//! each changed path in a debounced batch, whether it is an external content
//! modification (-> `"artifact changed externally"`) or a structural change
//! (-> `"project tree changed"`). The pure halves are unit-tested without a
//! Tauri runtime.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use tauri::{Emitter, Manager};

use crate::artifacts::{ArtifactChangedPayload, ContentTracker};
use crate::changes::{self, ChangesUpdatedPayload};
use crate::scanning;

/// Holds the live recursive filesystem watcher for the open project
/// (ASC-FR-10). Storing the debouncer here keeps it alive; dropping it (on
/// replace or `stop`) tears the watcher down so no further
/// PST-FR-16 / EXC-FR-LKHZ: an artifact's on-disk content diverged externally.
pub const ARTIFACT_CHANGED_EXTERNALLY: &str = "artifact-changed-externally";
/// ASC-FR-10 / LIB-FR-10: the project tree changed structurally (debounced).
pub const PROJECT_TREE_CHANGED: &str = "project-tree-changed";

/// How many paths of one burst ASC-FR-21 itemises before summarising the rest.
///
/// The session buffer is a bounded ring of 20,000 that evicts oldest-first
/// regardless of level (LGC-FR-07), so an uncapped per-path loop lets one branch
/// checkout push a burst's worth of routine INFO over the WARN and ERROR records
/// explaining whatever the user is actually debugging. Anything over the cap is
/// still *accounted for* — a trailing record names how many were omitted — so
/// the log never quietly implies it listed everything.
const TREE_CHANGE_LOG_CAP: usize = 50;

/// `"project tree changed"` event fires (ASC-FR-14).
#[derive(Default)]
pub struct ProjectWatcher {
    inner: Mutex<Option<Debouncer<RecommendedWatcher>>>,
}

impl ProjectWatcher {
    /// Tear down any active watcher (ASC-FR-14). Idempotent.
    pub fn stop(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
    }

    #[cfg(test)]
    pub fn is_active(&self) -> bool {
        self.inner
            .lock()
            .map(|guard| guard.is_some())
            .unwrap_or(false)
    }

    pub fn set(&self, debouncer: Debouncer<RecommendedWatcher>) {
        if let Ok(mut guard) = self.inner.lock() {
            // Replacing drops the previous debouncer -> previous watcher torn
            // down (ASC-FR-14 on project switch).
            *guard = Some(debouncer);
        }
    }
}

/// How one changed path in a watcher batch routes (PST-FR-16 / ASC-FR-15). Pure
/// so the content-vs-structural split is fully unit-testable.
#[derive(Debug, PartialEq)]
pub enum WatchRoute {
    /// A tracked artifact whose on-disk content diverged from its baseline ->
    /// emit `"artifact changed externally"` with this checksum and adopt it.
    Content(String),
    /// A tracked artifact touched but unchanged (the Editor's own save) -> no
    /// event, and NOT structural (its tree shape did not change).
    Quiet,
    /// An untracked path, or a tracked file that was removed -> contributes to
    /// the structural `"project tree changed"` reload (ASC-FR-10).
    Structural,
}

/// PST-FR-16 decision (pure): given the checksum this module last served/wrote
/// for an artifact (`tracked`) and its freshly-computed on-disk checksum
/// (`current`, `None` if the file is gone), decide whether this is an EXTERNAL
/// content divergence worth emitting. `Some(new)` -> emit and update the
/// tracker; `None` -> stay quiet:
///   - untracked path (`tracked = None`): not a served artifact;
///   - file removed (`current = None`): a structural delete, not a divergence;
///   - `current == tracked`: unchanged, i.e. the Editor's own save (self-write).
pub fn external_divergence(tracked: Option<&str>, current: Option<&str>) -> Option<String> {
    match (tracked, current) {
        (Some(t), Some(c)) if t != c => Some(c.to_string()),
        _ => None,
    }
}

/// Is a `.synthesis/library.toml` event the echo of this application's own
/// write (pure)? `baseline` is what [`scanning::AttributionBaseline`] recorded
/// after the last write this application made; `current` is the file's
/// freshly-computed on-disk checksum, `None` if it is gone or unreadable.
///
/// Only an exact match of a recorded baseline against what is on disk counts as
/// a self-write. No baseline, an unreadable file, and a deleted file are all
/// EXTERNAL, and the asymmetry is deliberate: erring toward "external" costs one
/// redundant index pass, while erring toward "self-write" costs a classification
/// that stays stale for the life of the project (BMI-FR-17, BMI-FR-18).
pub fn attribution_is_self_write(baseline: Option<&str>, current: Option<&str>) -> bool {
    matches!((baseline, current), (Some(b), Some(c)) if b == c)
}

/// Take the attribution file out of a batch's changed paths, reporting whether
/// it was there (pure).
///
/// ASC-FR-09: `.synthesis/library.toml` rides the raw path channel (ASC-FR-20)
/// but is not a tree node, so it must leave the batch before the per-path
/// routing loop — otherwise `route_watch_change` would see an untracked path,
/// return `Structural`, and a *deleted* attribution file would reach
/// `removed_paths`, where TAB-FR-19 would close a tab for a path no tab ever
/// had. A named function rather than two lines inlined in the callback so a
/// test can call the same code the callback does.
pub fn split_attribution(changed: &mut Vec<String>) -> bool {
    let touched = changed.iter().any(|p| p == scanning::LIBRARY_TOML_REL);
    changed.retain(|p| p != scanning::LIBRARY_TOML_REL);
    touched
}

/// Decide how a single changed path routes, given the tracker's baseline for it
/// (`tracked`) and its freshly-computed on-disk checksum (`current`, `None` if
/// the file is gone). This is the routing the watcher callback composes over a
/// batch; isolating it keeps PST-FR-16 / PST-FR-17, PST-FR-31's "the two channels never
/// cross-fire" claim testable without a runtime.
pub fn route_watch_change(tracked: Option<&str>, current: Option<&str>) -> WatchRoute {
    match (tracked, current) {
        (Some(_), Some(_)) => match external_divergence(tracked, current) {
            Some(ck) => WatchRoute::Content(ck),
            None => WatchRoute::Quiet,
        },
        // Untracked path, or a tracked file that disappeared (a structural
        // delete) -> the tree-reload channel, never the content channel.
        _ => WatchRoute::Structural,
    }
}

/// The Git directory events are attributed to, and the extra path that must be
/// watched to see them (CHC-FR-16).
///
/// Both come back **canonicalised**, and that is the whole point of pairing them
/// in one function: notify reports event paths under the path it was handed, so
/// if the watched path were canonical while the path events are stripped against
/// were not — `/tmp` versus `/private/tmp` on macOS, or any repository reached
/// through a symlink — every strip would fail and the Git channel would go
/// silent while looking perfectly wired.
///
/// The second element is `None` for a standalone project, whose `.git` already
/// sits inside the recursive watch on the project root.
pub fn git_watch_targets(root: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let Ok(repo) = changes::open_repo(root) else {
        return (None, None);
    };
    let git_dir = changes::canonicalize_lenient(repo.path());
    let extra = changes::git_dir_to_watch(&git_dir, root);
    (Some(git_dir), extra)
}

/// Start (or replace) the recursive watcher on `root`, emitting a debounced
/// `"project tree changed"` event when the filesystem under the root changes
/// (ASC-FR-10). Best-effort: a watcher-init failure is logged and never blocks
/// opening the project. The pure change-summary half lives in
/// `scanning::removed_rel_paths` and `scanning::changed_rel_paths`, both
/// unit-tested there.
pub fn start_watching<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    watcher: &ProjectWatcher,
    root: &crate::fs::RootFs,
) {
    if !root.exists() {
        // Nothing to watch (e.g. a not-yet-cloned git URL handle); leave any
        // prior watcher torn down.
        watcher.stop();
        return;
    }
    let app_handle = app.clone();
    let root_owned = root.clone();
    // CHC-FR-16: the Changes panel watches the repository's Git directory too —
    // a commit or a checkout alters every diff while touching no working-tree
    // file.
    let (git_dir_owned, extra_git_watch) = git_watch_targets(root);
    let debouncer = new_debouncer(
        Duration::from_millis(300),
        move |result: DebounceEventResult| {
            // The debounced batch carries no event kinds (notify-mini collapses
            // create/modify/delete into `Any`), so we discriminate structural
            // from content changes via the per-artifact checksum tracker:
            //   - a change to a file the Editor has loaded/saved (tracked) and
            //     that still exists is a CONTENT modification -> the
            //     `"artifact changed externally"` channel (ASC-FR-15 /
            //     PST-FR-16); it deliberately does NOT reload the tree.
            //   - everything else (untracked file, a delete, a directory) is
            //     STRUCTURAL -> the debounced `"project tree changed"` event
            //     (ASC-FR-10). An untracked file's content edit also reloads the
            //     tree, which is correct: its content may have just become
            //     classifiable (content tiebreak, ASC-FR-04).
            // CHC-FR-16: the Changes panel's change set is sensitive to more
            // than the tree's shape — a content edit alters a diff without
            // altering the tree, and a commit alters every diff while touching
            // no working-tree file at all. So it gets its own count over both
            // sources, computed before the early return below (which exists for
            // the tree/content channels, whose sources are working-tree only).
            // The debouncer's window is what coalesces a burst into one event.
            let changes_count = changes::changes_change_count(
                &result,
                &root_owned,
                git_dir_owned.as_deref(),
            );
            if changes_count > 0 {
                let _ = app_handle.emit(
                    crate::changes::CHANGES_UPDATED,
                    ChangesUpdatedPayload {
                        change_count: changes_count,
                    },
                );
            }
            let mut changed = scanning::changed_rel_paths(&result, &root_owned);
            // ASC-FR-09: the attribution file is not a tree node, so it must
            // never become an `"artifact changed externally"` subject nor appear
            // among `structural_paths` / `removed_paths` — no tab is backed by
            // it, and TAB-FR-19 would be asked to close one for a path that
            // never had one. Splitting it out here rather than special-casing it
            // inside the routing loop is what keeps that structural.
            //
            // It is still a change of the first importance: every file's
            // resolved type derives from it (ASC-FR-05 / ASC-FR-06), so an
            // external write restates the whole classification — the indexes a
            // file belongs to (BMI-FR-17, and BMI-FR-18 for the pull that
            // carried it) and the types the Project panel renders (LIB-FR-09).
            let attribution_touched = split_attribution(&mut changed);
            let attribution_changed = attribution_touched && {
                // `try_state`, not `state`: this runs on the notify thread, where
                // a panic surfaces as a silently dead watcher rather than as a
                // failure anyone sees. An unmanaged baseline degrades to
                // "always external" — one redundant pass, never staleness.
                let state = app_handle.try_state::<scanning::AttributionBaseline>();
                let baseline = state.as_ref().and_then(|s| s.current());
                let current = root_owned.sha256_file(scanning::LIBRARY_TOML_REL).ok();
                let is_self = attribution_is_self_write(baseline.as_deref(), current.as_deref());
                // Adopt what we just observed, exactly as the content channel
                // adopts a checksum below: the baseline names what consumers
                // last SAW, not what this application last WROTE. A baseline
                // that only moved on our own writes would suppress a revert back
                // to bytes we once wrote — and a `git checkout` to the previous
                // branch is precisely that revert (BMI-FR-18 names checkout),
                // which would leave the classification stale for good.
                if let Some(s) = state.as_ref() {
                    s.adopt(current);
                }
                !is_self
            };
            if changed.is_empty() && !attribution_changed {
                return;
            }
            // ASC-FR-10 / ASC-FR-15 / ASC-FR-17: the mounted candidate list no
            // longer describes the tree, so the next search rebuilds it. A
            // search already running keeps the list it started from.
            //
            // Invalidated for *any* change, not only a structural one: a content
            // edit can change a file's classification through the frontmatter
            // tiebreak (ASC-FR-04), and a stale list would go on grouping that
            // file under the type it used to have (SCC-FR-08).
            app_handle
                .state::<scanning::CandidateStore>()
                .invalidate();
            // BMI-FR-15 / BMI-FR-16 / BMI-FR-17 / BMI-FR-18: every change the
            // watcher sees — a creation, a deletion, a content edit, a pull
            // that rewrote half the tree — reaches the BM25 indexes here and
            // nowhere else. The pass runs in the background and collapses a
            // burst into one (BMI-FR-21).
            //
            // ASC-FR-20 / DSL-FR-19: `changed` is the raw path channel, which
            // reports gitignored paths too, and a pass always re-enumerates the
            // four skill folders whatever scope it was asked for. That pairing
            // is what makes an edit to a gitignored `.claude/skills/*/SKILL.md`
            // — invisible to the tree, and so to every other consumer here —
            // still reach the skills index.
            crate::bm25_index::request_pass(
                &app_handle,
                crate::bm25_index::PassScope::ARTIFACTS,
            );
            let tracker = app_handle.state::<ContentTracker>();
            // PST-FR-35: whether what the Recently edited widget enumerates, or
            // how it orders, may have moved. Set from BOTH channels below — a
            // content modification to a path that resolves to an artifact, and
            // any structural change — because both alter what PST-FR-31 reads.
            let mut recently_edited_moved = false;
            // ASC-FR-22: the structural paths, so the emitted payload can name
            // the ones that are gone. Collected rather than counted because the
            // count alone tells a consumer that *something* changed and never
            // which of its open tabs no longer has a file behind it.
            let mut structural_paths: Vec<String> = Vec::new();
            for rel in &changed {
                let current = root_owned.sha256_file(rel).ok();
                let tracked = tracker.current(rel);
                let route = route_watch_change(tracked.as_deref(), current.as_deref());
                // PST-FR-35 carries **no self-write suppression**: a write this
                // application performed moves the file's modification time and
                // must therefore move the widget, which is the whole point of
                // ordering by that time. So the quiet route — a save of ours,
                // already recorded — counts here exactly as an external edit
                // does, where the path resolves to an artifact.
                //
                // Guarded on `!recently_edited_moved` first: the event is one
                // per batch, so once a path has earned it there is nothing to
                // learn from classifying the rest — and classification can read
                // a Markdown file's frontmatter (ASC-FR-04), which is not work
                // to repeat across a checkout's worth of paths on the notify
                // thread.
                if !recently_edited_moved
                    && matches!(route, WatchRoute::Content(_) | WatchRoute::Quiet)
                    && crate::artifacts::resolved_type(&root_owned, rel).is_some()
                {
                    recently_edited_moved = true;
                }
                match route {
                    WatchRoute::Content(new_ck) => {
                        // Adopt the new checksum so a duplicate batch for the
                        // same change does not re-fire; a further external edit
                        // (new checksum) still will (EXC-FR-UWYK).
                        tracker.record(rel, &new_ck);
                        let _ = app_handle.emit(
                            ARTIFACT_CHANGED_EXTERNALLY,
                            ArtifactChangedPayload {
                                artifact_id: rel.clone(),
                                checksum: new_ck,
                            },
                        );
                    }
                    // Tracked self-write: silent, and never structural.
                    WatchRoute::Quiet => {}
                    WatchRoute::Structural => structural_paths.push(rel.clone()),
                }
            }
            // PST-FR-35: a structural change alters which artifacts exist and
            // therefore what the widget enumerates, whatever their modification
            // times say.
            if recently_edited_moved || !structural_paths.is_empty() {
                crate::dashboard::note_recently_edited_changed(&app_handle);
            }
            if !structural_paths.is_empty() || attribution_changed {
                let removed_paths =
                    scanning::removed_rel_paths(&structural_paths, &root_owned);
                // The tree reload an attribution change causes has no path
                // records of its own — the file is not a node, so the loop below
                // never names it — which would leave a reader looking at a
                // refresh with nothing explaining it. Names the path only; the
                // stored assignments are user curation and no part of the file's
                // contents belongs in a record.
                if attribution_changed {
                    crate::logging::log_debug(
                        &app_handle,
                        &crate::logging::BUFFER,
                        &[crate::logging::Domain::Backend],
                        "artifact attribution changed externally",
                        crate::log_fields! {
                            "path" => scanning::LIBRARY_TOML_REL,
                        },
                    );
                }
                // ASC-FR-21: one INFO per structural change, not one per emitted
                // event. A burst coalesces into a single `"project tree changed"`
                // (ASC-FR-10), so logging per event would say only that the
                // Library refreshed; logging per path says what it refreshed
                // *for*, which is the question anyone reading the panel has.
                //
                // A path is safe in a record — it is not user content and carries
                // no credential — and nothing of the file's contents is read here,
                // let alone logged.
                for rel in structural_paths.iter().take(TREE_CHANGE_LOG_CAP) {
                    // Two values, not the four ASC-FR-21 names, because two is
                    // all the watcher honestly knows: `notify-mini` collapses
                    // create/modify/delete into `Any`, so a rename cannot be
                    // told from a delete-plus-create, and a path that still
                    // exists may have been created *or* had its content change
                    // in a way that re-classifies it. "changed" says the weaker
                    // true thing rather than the stronger false one.
                    let action = if removed_paths.contains(rel) {
                        "removed"
                    } else {
                        "changed"
                    };
                    // `DEBUG`: a tree change is detail that matters once you
                    // are already investigating one, and a single save or a
                    // branch checkout produces a burst of them. At `INFO` that
                    // burst would push the records explaining whatever the
                    // reader is actually after out of a bounded ring
                    // (LGC-FR-07).
                    crate::logging::log_debug(
                        &app_handle,
                        &crate::logging::BUFFER,
                        &[crate::logging::Domain::Backend],
                        "project tree changed",
                        crate::log_fields! {
                            "path" => rel,
                            "action" => action,
                        },
                    );
                }
                // The cap is stated, never silent. A branch checkout can rewrite
                // thousands of files in one burst, and the session buffer is a
                // bounded ring (LGC-FR-07) that evicts oldest-first regardless of
                // level — so an uncapped per-path loop would push a burst's worth
                // of routine INFO over the WARN and ERROR records explaining
                // whatever the user is actually debugging. Naming the remainder
                // keeps ASC-FR-21's account complete without paying that cost.
                if structural_paths.len() > TREE_CHANGE_LOG_CAP {
                    // Follows its subjects to `DEBUG`: a note about records
                    // the reader has filtered out is of no use above the level
                    // those records live at.
                    crate::logging::log_debug(
                        &app_handle,
                        &crate::logging::BUFFER,
                        &[crate::logging::Domain::Backend],
                        "project tree changed: further paths not itemised",
                        crate::log_fields! {
                            "itemised" => TREE_CHANGE_LOG_CAP,
                            "omitted" => structural_paths.len() - TREE_CHANGE_LOG_CAP,
                            "total" => structural_paths.len(),
                        },
                    );
                }
                // Best-effort emit; a dropped event is not fatal (the user can
                // still trigger a manual rescan).
                let _ = app_handle.emit(
                    PROJECT_TREE_CHANGED,
                    scanning::TreeChangedPayload {
                        // The attribution change counts as one: it is a change
                        // this burst genuinely carried, and a `0` here would be
                        // a count no consumer has ever been handed — the shape a
                        // future truthiness gate would silently drop, dropping
                        // precisely the reload this exists to deliver.
                        change_count: structural_paths.len()
                            + usize::from(attribution_changed),
                        // Structural only. The attribution file backs no tab, so
                        // it must never reach TAB-FR-19's close-removed pass.
                        removed_paths,
                    },
                );
            }
        },
    );
    let mut debouncer = match debouncer {
        Ok(d) => d,
        Err(e) => {
            eprintln!("synthesis: failed to init filesystem watcher: {e}");
            return;
        }
    };
    if let Err(e) = debouncer.watcher().watch(root, RecursiveMode::Recursive) {
        eprintln!("synthesis: failed to watch project root {root:?}: {e}");
        return;
    }
    // CHC-FR-16, co-located mode: the host repository's Git directory sits
    // outside the project root. Best-effort — a project whose Git directory
    // cannot be watched still gets working-tree events and the manual refresh.
    if let Some(gd) = &extra_git_watch {
        if let Err(e) = debouncer.watcher().watch(gd, RecursiveMode::Recursive) {
            eprintln!("synthesis: failed to watch git directory {gd:?}: {e}");
        }
    }
    watcher.set(debouncer);
}

#[cfg(test)]
mod tests;

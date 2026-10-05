//! The drafts-root watch (DRS-FR-42).
//!
//! `.synthesis/drafts/` is pruned from the project scan (ASC-FR-09) and from the
//! raw path channel of the project watcher (ASC-FR-20), so nothing else in the
//! application can observe a write to a draft's prompt. This module mounts its
//! own recursive watch on the active worktree's drafts root and emits
//! `"draft prompt changed"` when — and only when — the **content** of a draft's
//! one prompt file changes on disk, whoever wrote it.
//!
//! The distinction that costs the most care is between a prompt that was
//! *written* and a prompt that merely *moved*: renaming a draft renames its
//! prompt (DRS-FR-25) and moving a draft between folders moves the directory
//! holding it, and both reach the watcher as paths under `files/` while changing
//! no byte of the prompt. Both are answered by the same fact — a rename
//! preserves a file's modification time — so this module remembers the activity
//! instant it last saw per draft and emits only where the new one differs
//! ([`PromptActivity`]). That keeps DRS-FR-42's exclusion list ("a rename, a
//! move, a status change, a write under `assets/` …") a consequence of one rule
//! rather than a list of special cases to keep in step with the spec.
//!
//! The pure halves — [`prompt_target`] and [`PromptActivity`] — are unit-tested
//! here without a Tauri runtime.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use serde::Serialize;
use tauri::Emitter;

/// DRS-FR-42: the content of one draft's prompt file changed on disk.
///
/// Kebab-case for the reason every other channel here is: Tauri accepts only
/// alphanumerics, `-`, `/`, `:` and `_` in an event name, and rejects a name
/// with spaces on both sides silently.
pub const DRAFT_PROMPT_CHANGED: &str = "draft-prompt-changed";

/// How long a burst of writes to one prompt is collapsed over. The same window
/// the project watcher uses, for the same reason: an editor's save is several
/// filesystem events and the author made one change.
const DEBOUNCE: Duration = Duration::from_millis(300);

/// Payload of `"draft prompt changed"` (DRS-FR-42).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftPromptChangedPayload {
    pub draft_id: String,
    /// The prompt file's new modification time, RFC 3339 UTC (DRS-FR-41).
    pub prompt_activity_at: String,
}

/// The last prompt-activity instant this watch saw for each draft.
///
/// The whole of what tells a written prompt from a moved one: a write moves the
/// instant, a rename or a move does not. Seeded when the watch mounts, so the
/// first batch after a project opens reports what changed rather than every
/// draft the worktree holds.
#[derive(Default)]
pub struct PromptActivity {
    seen: HashMap<String, String>,
}

impl PromptActivity {
    /// Record `at` for `draft_id` and answer whether it is news.
    ///
    /// A draft this map has not seen is news — a prompt that has just come into
    /// existence was written, whatever else was true a moment ago.
    fn observe(&mut self, draft_id: &str, at: &str) -> bool {
        match self.seen.insert(draft_id.to_string(), at.to_string()) {
            Some(previous) => previous != at,
            None => true,
        }
    }

    /// Seed `draft_id` without reporting it, for the mount pass.
    fn seed(&mut self, draft_id: &str, at: &str) {
        self.seen.insert(draft_id.to_string(), at.to_string());
    }
}

/// Holds the live watch on the drafts root, and the activity map that decides
/// what it reports.
///
/// Storing the debouncer here keeps it alive; dropping it (on replace or
/// [`DraftsWatcher::stop`]) tears the watch down, which is what makes a closed
/// project and an outgoing worktree emit nothing (DRS-FR-42).
#[derive(Default)]
pub struct DraftsWatcher {
    inner: Mutex<Option<Debouncer<RecommendedWatcher>>>,
    activity: std::sync::Arc<Mutex<PromptActivity>>,
}

impl DraftsWatcher {
    /// Tear down any active watch (DRS-FR-42). Idempotent.
    ///
    /// The activity map is cleared with it: an instant recorded for the outgoing
    /// worktree describes a different file, and carrying it over would let a
    /// coincidence suppress the incoming root's first genuine change.
    pub fn stop(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = None;
        }
        if let Ok(mut guard) = self.activity.lock() {
            guard.seen.clear();
        }
    }

    fn set(&self, debouncer: Debouncer<RecommendedWatcher>) {
        if let Ok(mut guard) = self.inner.lock() {
            *guard = Some(debouncer);
        }
    }

    /// Arm the watch with a debouncer built by hand, so a teardown assertion in
    /// another module's tests is not vacuous against an already-idle watch.
    #[cfg(test)]
    pub fn set_for_test(&self, debouncer: Debouncer<RecommendedWatcher>) {
        self.set(debouncer);
    }

    #[cfg(test)]
    pub fn is_active(&self) -> bool {
        self.inner.lock().map(|g| g.is_some()).unwrap_or(false)
    }
}

/// The draft id and prompt basename a drafts-root-relative path names, or `None`
/// where the path is not a file directly inside some draft's `files/`.
///
/// Pure over the path alone — it opens nothing — because it runs on the notify
/// thread for every path of every batch, and because the shape of a draft's
/// storage (DRS-FR-01) is exactly the kind of rule worth pinning in a test.
/// The draft's `files/` holds one file at its root and no folder (DRS-FR-11), so
/// a path with anything between `files/` and the basename names material this
/// module has no business reporting.
pub fn prompt_target(rel: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty()).collect();
    let n = parts.len();
    // `<...folders>/<draft-id>/files/<name>` — three components at least, and
    // the last but one must be `files`.
    if n < 3 || parts[n - 2] != crate::drafts::FILES_DIR {
        return None;
    }
    Some((parts[n - 3].to_string(), parts[n - 1].to_string()))
}

/// Start (or replace) the watch on the active worktree's drafts root
/// (DRS-FR-42).
///
/// Best-effort throughout: a watch that cannot be mounted is reported and never
/// blocks the project from opening — the Drafts panel and the Dashboard still
/// refresh on every command that changes a draft, they simply stop hearing about
/// an external editor.
pub fn start_watching<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    watcher: &DraftsWatcher,
    root: &crate::fs::RootFs,
) {
    let drafts_root = root.path().join(".synthesis").join("drafts");
    // `file_info`, not a `Path` predicate: every disk read in this crate goes
    // through `FsAccess`, so a drafts root outside the allowlist is refused here
    // rather than watched.
    let is_dir = root
        .file_info(&drafts_root)
        .is_ok_and(|info| info.kind == crate::fs::EntryKind::Dir);
    if !is_dir {
        // A handle whose project has not been scaffolded yet (a not-yet-cloned
        // git URL, say). Leave any prior watch torn down rather than watching a
        // path that does not exist.
        watcher.stop();
        return;
    }
    watcher.stop();
    // DRS-FR-42: seed the activity map from what is on disk *before* the watch
    // is armed, so the first write after a project opens is reported as one
    // change rather than the whole worktree being reported as new.
    if let Ok(mut guard) = watcher.activity.lock() {
        for draft in crate::drafts::list_drafts_impl(root).drafts {
            if let Some(at) = draft.prompt_activity_at.as_deref() {
                guard.seed(&draft.id, at);
            }
        }
    }

    let app_handle = app.clone();
    let root_owned = root.clone();
    let drafts_root_owned = drafts_root.clone();
    let activity = std::sync::Arc::clone(&watcher.activity);
    let debouncer = new_debouncer(DEBOUNCE, move |result: DebounceEventResult| {
        let Ok(events) = &result else { return };
        // The batch is collapsed to one entry per draft before anything is
        // read: a save is several filesystem events for the same file, and the
        // stat below is what this watch costs (DRS non-functional).
        let mut candidates: Vec<(String, std::path::PathBuf)> = Vec::new();
        for event in events {
            let Ok(rel) = event.path.strip_prefix(&drafts_root_owned) else {
                continue;
            };
            let parts: Vec<String> = rel
                .components()
                .filter_map(|c| match c {
                    std::path::Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect();
            let Some((draft_id, _)) = prompt_target(&parts.join("/")) else {
                continue;
            };
            if candidates.iter().any(|(id, _)| *id == draft_id) {
                continue;
            }
            // The draft's own directory is everything above `files/<name>`, and
            // the event already names it: re-deriving it by walking the drafts
            // root would make a burst of writes to one prompt cost a walk per
            // batch (DRS non-functional).
            let dir = parts[..parts.len() - 2]
                .iter()
                .fold(drafts_root_owned.clone(), |acc, part| acc.join(part));
            candidates.push((draft_id, dir));
        }
        for (draft_id, dir) in candidates {
            // DRS-FR-41: two small reads and one stat. A draft made inconsistent
            // by the very write that was reported has no single prompt to stat
            // and is passed over — `"drafts changed"` still carries it to the
            // panel (DRS-FR-22).
            let Some(at) = crate::drafts::prompt_activity_in(&root_owned, &dir, &draft_id) else {
                // Handled, and therefore invisible unless it is said out loud:
                // the draft was deleted between the write and this read, or the
                // write is what made it inconsistent (DRS-FR-15). Either way
                // there is no prompt to report activity for, and
                // `"drafts changed"` still carries the draft to the panel.
                crate::logging::log_debug(
                    &app_handle,
                    &crate::logging::BUFFER,
                    &[crate::logging::Domain::Backend],
                    "draft prompt change skipped: no single prompt to stat",
                    crate::log_fields! { "draft" => draft_id.clone() },
                );
                continue;
            };
            let news = activity
                .lock()
                .map(|mut guard| guard.observe(&draft_id, &at))
                .unwrap_or(false);
            if !news {
                // A rename or a move: the path under `files/` changed and the
                // prompt's bytes did not (DRS-FR-42). `"drafts changed"` reports
                // those on its own terms (DRS-FR-22).
                continue;
            }
            crate::logging::log_debug(
                &app_handle,
                &crate::logging::BUFFER,
                &[crate::logging::Domain::Backend],
                "draft prompt changed on disk",
                crate::log_fields! {
                    "draft" => draft_id.clone(),
                    "promptActivityAt" => at.clone(),
                },
            );
            let _ = app_handle.emit(
                DRAFT_PROMPT_CHANGED,
                DraftPromptChangedPayload {
                    draft_id: draft_id.clone(),
                    prompt_activity_at: at,
                },
            );
            // PST-FR-35: what moves the Dashboard's Active workstreams ordering.
            crate::dashboard::note_active_drafts_changed(&app_handle);
        }
    });
    let mut debouncer = match debouncer {
        Ok(d) => d,
        Err(e) => {
            crate::logging::log_warn(
                app,
                &crate::logging::BUFFER,
                &[crate::logging::Domain::Backend],
                "drafts watch could not be started",
                crate::log_fields! { "reason" => e.to_string() },
            );
            return;
        }
    };
    if let Err(e) = debouncer
        .watcher()
        .watch(&drafts_root, RecursiveMode::Recursive)
    {
        crate::logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "drafts root could not be watched",
            crate::log_fields! { "reason" => e.to_string() },
        );
        return;
    }
    watcher.set(debouncer);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_target_names_the_draft_and_its_prompt() {
        // DRS-FR-01 / DRS-FR-11: a prompt sits directly inside the draft's
        // `files/`, wherever in the folder hierarchy the draft itself is filed.
        assert_eq!(
            prompt_target("d-1/files/spec.md"),
            Some(("d-1".to_string(), "spec.md".to_string())),
            "a draft at the drafts root"
        );
        assert_eq!(
            prompt_target("UI/Components/d-1/files/spec.md"),
            Some(("d-1".to_string(), "spec.md".to_string())),
            "a draft filed several folders deep"
        );
    }

    #[test]
    fn prompt_target_ignores_everything_that_is_not_a_prompt() {
        // DRS-FR-42: a write to the record, to the sibling folders, or to the
        // conversation log is not prompt content and must reach nothing here.
        for rel in [
            "d-1/draft.toml",
            "d-1/assets/image.png",
            "d-1/comments/thread.jsonl",
            "d-1/proposals/p-1/candidate.md",
            "d-1/history/h-1.md",
            "d-1/conversation.jsonl",
            "d-1/files",
            "UI",
            "",
        ] {
            assert_eq!(prompt_target(rel), None, "{rel} is not a prompt write");
        }
        // DRS-FR-11 forbids a folder under `files/`, so a nested path names
        // material this module has no business reporting as a prompt.
        assert_eq!(
            prompt_target("d-1/files/nested/spec.md"),
            None,
            "a nested path is not the draft's one prompt"
        );
    }

    #[test]
    fn activity_reports_a_write_and_stays_quiet_for_a_move() {
        // DRS-FR-42: the instant is what separates a prompt that was written
        // from one that was renamed or moved — a rename preserves it.
        let mut activity = PromptActivity::default();
        assert!(
            activity.observe("d-1", "2026-01-01T00:00:00.000Z"),
            "a prompt this watch has not seen has just been written"
        );
        assert!(
            !activity.observe("d-1", "2026-01-01T00:00:00.000Z"),
            "the same instant again is the same bytes: a rename or a move"
        );
        assert!(
            activity.observe("d-1", "2026-01-01T00:05:00.000Z"),
            "a new instant is a write"
        );
    }

    #[test]
    fn seeding_the_mount_pass_reports_nothing() {
        // DRS-FR-42: mounting the watch must not report every draft the
        // worktree holds as newly written.
        let mut activity = PromptActivity::default();
        activity.seed("d-1", "2026-01-01T00:00:00.000Z");
        assert!(
            !activity.observe("d-1", "2026-01-01T00:00:00.000Z"),
            "a seeded draft is quiet until its prompt actually moves"
        );
    }

    // ------------------------------------------------------------------
    // DRS-FR-42 — the watch itself, end to end
    // ------------------------------------------------------------------

    /// Every `"draft prompt changed"` this app handle sees, as `(id, instant)`.
    fn collect_prompt_events(
        app: &tauri::App<tauri::test::MockRuntime>,
    ) -> std::sync::Arc<Mutex<Vec<(String, String)>>> {
        use tauri::Listener;
        let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
        let s = std::sync::Arc::clone(&seen);
        app.listen(DRAFT_PROMPT_CHANGED, move |event| {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(event.payload()) {
                let id = v.get("draftId").and_then(|x| x.as_str()).unwrap_or("");
                let at = v.get("promptActivityAt").and_then(|x| x.as_str()).unwrap_or("");
                s.lock().unwrap().push((id.to_string(), at.to_string()));
            }
        });
        seen
    }

    /// Poll a condition over a bounded window rather than sleeping a fixed one.
    fn wait_until(mut done: impl FnMut() -> bool) -> bool {
        for _ in 0..100 {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        false
    }

    fn ids_seen(seen: &std::sync::Arc<Mutex<Vec<(String, String)>>>) -> Vec<String> {
        seen.lock().unwrap().iter().map(|(id, _)| id.clone()).collect()
    }

    /// DRS-FR-42: a prompt write emits, and a rename — which moves the prompt's
    /// path and not its bytes — emits nothing.
    ///
    /// Every absence here is proved against a **positive barrier** rather than a
    /// sleep. A fixed sleep asserting "no event arrived" passes green for every
    /// reason the event could fail to arrive: a watch that never mounted, a
    /// debouncer that failed to init, an FSEvents cold start slower than the
    /// guess. Writing a second draft's prompt in the same window forces the
    /// pipeline to prove it is alive — that event MUST arrive, and the rename's
    /// silence is only meaningful beside it.
    #[test]
    fn a_prompt_write_is_reported_and_a_rename_is_not() {
        use tauri::Manager;
        let dir = tempfile::TempDir::new().unwrap();
        let root_path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&root_path);
        std::fs::create_dir_all(root_path.join(".synthesis/drafts")).unwrap();

        let one = crate::drafts::create_draft_impl(&root, Some("first"), None).unwrap();
        let two = crate::drafts::create_draft_impl(&root, Some("second"), None).unwrap();

        let app = tauri::test::mock_app();
        app.manage(DraftsWatcher::default());
        let seen = collect_prompt_events(&app);
        let watcher = app.state::<DraftsWatcher>();
        start_watching(&app.handle().clone(), &watcher, &root);
        assert!(
            watcher.is_active(),
            "precondition: a dead watch would make every assertion below vacuous"
        );

        // A prompt written through the ordinary save path.
        crate::drafts::save_draft_file_impl(&root, &one.draft.id, &one.file, "# rewritten\n")
            .unwrap();
        assert!(
            wait_until(|| ids_seen(&seen).contains(&one.draft.id)),
            "a prompt write is reported, carrying that draft's id (DRS-FR-42)"
        );
        let reported_at = seen
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(id, _)| *id == one.draft.id)
            .map(|(_, at)| at.clone())
            .unwrap();
        assert_eq!(
            Some(reported_at),
            crate::drafts::draft_prompt_activity(&root, &one.draft.id).unwrap(),
            "and the file's new modification time"
        );

        // A rename moves the prompt's path and not its bytes; the barrier beside
        // it is the second draft's own prompt write, which must be reported.
        let before = ids_seen(&seen);
        crate::drafts::rename_draft_impl(&root, &one.draft.id, "renamed").unwrap();
        crate::drafts::save_draft_file_impl(&root, &two.draft.id, &two.file, "# barrier\n")
            .unwrap();
        assert!(
            wait_until(|| ids_seen(&seen).contains(&two.draft.id)),
            "the barrier write must arrive, proving the watch is still alive"
        );
        std::thread::sleep(Duration::from_millis(400));
        let after = ids_seen(&seen);
        assert_eq!(
            after.iter().filter(|id| **id == one.draft.id).count(),
            before.iter().filter(|id| **id == one.draft.id).count(),
            "a rename reports nothing: the path under `files/` changed and the \
             prompt's bytes did not (DRS-FR-42)"
        );
    }

    /// DRS-FR-42: a write beside the prompt reports nothing, and a torn-down
    /// watch reports nothing at all.
    #[test]
    fn sibling_writes_and_a_closed_project_report_nothing() {
        use tauri::Manager;
        let dir = tempfile::TempDir::new().unwrap();
        let root_path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&root_path);
        std::fs::create_dir_all(root_path.join(".synthesis/drafts")).unwrap();

        let one = crate::drafts::create_draft_impl(&root, Some("first"), None).unwrap();
        let two = crate::drafts::create_draft_impl(&root, Some("second"), None).unwrap();
        let one_dir = root_path.join(".synthesis/drafts").join(&one.draft.id);

        let app = tauri::test::mock_app();
        app.manage(DraftsWatcher::default());
        let seen = collect_prompt_events(&app);
        let watcher = app.state::<DraftsWatcher>();
        start_watching(&app.handle().clone(), &watcher, &root);
        assert!(watcher.is_active(), "precondition");

        // Everything a draft holds beside its one prompt, written at once — and
        // the barrier that has to arrive regardless.
        std::fs::write(one_dir.join("draft.toml.bak"), "by hand\n").unwrap();
        std::fs::write(one_dir.join("assets/img.png"), b"bytes").unwrap();
        std::fs::write(one_dir.join("proposals/p1.md"), "candidate\n").unwrap();
        std::fs::write(one_dir.join("history/h1.snapshot"), "version\n").unwrap();
        std::fs::write(one_dir.join("conversation.jsonl"), "line\n").unwrap();
        crate::drafts::save_draft_file_impl(&root, &two.draft.id, &two.file, "# barrier\n")
            .unwrap();

        assert!(
            wait_until(|| ids_seen(&seen).contains(&two.draft.id)),
            "the barrier write must arrive, proving the watch is alive"
        );
        std::thread::sleep(Duration::from_millis(400));
        assert!(
            !ids_seen(&seen).contains(&one.draft.id),
            "a write under `assets/`, `comments/`, `proposals/`, `history/`, or \
             the conversation log is not prompt content (DRS-FR-42): {:?}",
            seen.lock().unwrap()
        );

        // DRS-FR-42: the watch goes down with the project, so a further external
        // write to a prompt emits nothing.
        watcher.stop();
        assert!(!watcher.is_active());
        let quiet = ids_seen(&seen).len();
        crate::drafts::save_draft_file_impl(&root, &one.draft.id, &one.file, "# after close\n")
            .unwrap();
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(
            ids_seen(&seen).len(),
            quiet,
            "a closed project emits nothing"
        );
    }

    #[test]
    fn drafts_of_one_worktree_do_not_suppress_another_worktrees() {
        // DRS-FR-42: the map is cleared with the watch, so an id that means one
        // file in worktree A cannot silence the same id in worktree B.
        let watcher = DraftsWatcher::default();
        if let Ok(mut guard) = watcher.activity.lock() {
            guard.seed("d-1", "2026-01-01T00:00:00.000Z");
        }
        watcher.stop();
        let news = watcher
            .activity
            .lock()
            .map(|mut g| g.observe("d-1", "2026-01-01T00:00:00.000Z"))
            .unwrap();
        assert!(news, "a cleared map reports the incoming worktree's prompt");
    }
}

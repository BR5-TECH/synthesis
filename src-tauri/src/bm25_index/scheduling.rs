//! Pass scopes and pass scheduling (BMI-FR-12, BMI-FR-15 to BMI-FR-21).

use tauri::Manager;

use super::{Bm25Indexer, IndexId, COALESCE_WINDOW};

/// Which parts of the index set a pass reconciles.
///
/// A draft save has no bearing on the artifact indexes, a tree change none on
/// the drafts index, and a note write none on either, so a trigger names the
/// part it dirtied and a pass reads only that. Nothing dirties more than one: a
/// graduation writes into the project through its own targeted operation and
/// leaves the draft where it is, so the published files arrive on the watcher's
/// report like any other project write (BMI-FR-20).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct PassScope {
    pub artifacts: bool,
    pub drafts: bool,
    /// BMI-FR-29: the `notes` index, dirtied by the channel of NTC-FR-24 and by
    /// nothing else. Notes travel that channel rather than the project watcher,
    /// because `.synthesis/notes/` is outside what the scan surfaces
    /// (ASC-FR-09).
    pub notes: bool,
    /// BMI-FR-MWNQ: the `documents` index, dirtied by the channel of
    /// `DCL-documents-collection.md` DCL-FR-QGLH. Selected documents lie
    /// outside the project, so no project watcher reports them.
    pub documents: bool,
}

impl PassScope {
    pub const ALL: PassScope = PassScope {
        artifacts: true,
        drafts: true,
        notes: true,
        documents: true,
    };
    pub const ARTIFACTS: PassScope = PassScope {
        artifacts: true,
        drafts: false,
        notes: false,
        documents: false,
    };
    pub const DRAFTS: PassScope = PassScope {
        artifacts: false,
        drafts: true,
        notes: false,
        documents: false,
    };
    pub const NOTES: PassScope = PassScope {
        artifacts: false,
        drafts: false,
        notes: true,
        documents: false,
    };
    pub const DOCUMENTS: PassScope = PassScope {
        artifacts: false,
        drafts: false,
        notes: false,
        documents: true,
    };

    pub(super) fn merge(self, other: PassScope) -> PassScope {
        PassScope {
            artifacts: self.artifacts || other.artifacts,
            drafts: self.drafts || other.drafts,
            notes: self.notes || other.notes,
            documents: self.documents || other.documents,
        }
    }

    pub(super) fn is_empty(self) -> bool {
        !self.artifacts && !self.drafts && !self.notes && !self.documents
    }

    pub(super) fn covers(self, index: IndexId) -> bool {
        match index {
            // DSL-FR-19: every pass re-enumerates all four skill folders in
            // full whatever triggered it, so the skills index is always in
            // scope. That is what makes the registry converge on what is on
            // disk rather than on the sequence of events that reached it — and
            // it is why no trigger has to know it touched a skill.
            IndexId::Skills => true,
            IndexId::Drafts => self.drafts,
            IndexId::Notes => self.notes,
            IndexId::Documents => self.documents,
            _ => self.artifacts,
        }
    }
}

/// BMI-FR-12 / BMI-FR-21: ask for a pass covering `scope`.
///
/// Returns immediately. A burst of requests collapses into one pass: the first
/// starts a thread that waits out the coalescing window before reading the
/// accumulated scope, and a request that lands while a pass is running is
/// served by one more pass after it rather than by a thread of its own.
pub fn request_pass<R: tauri::Runtime>(app: &tauri::AppHandle<R>, scope: PassScope) {
    // `try_state` rather than `state`: a mock app in another module's test
    // manages only the stores that test needs, and a panic here would land on a
    // background thread where it takes down the process rather than the test.
    let Some(indexer) = app.try_state::<Bm25Indexer>() else {
        return;
    };
    let mut schedule = indexer.schedule();
    if schedule.root.is_none() {
        // Nothing is mounted; a pass would have no root to reconcile against.
        return;
    }
    schedule.pending = schedule.pending.merge(scope);
    if schedule.scheduled {
        return;
    }
    schedule.scheduled = true;
    drop(schedule);

    let app = app.clone();
    std::thread::spawn(move || {
        let Some(indexer) = app.try_state::<Bm25Indexer>() else {
            return;
        };
        loop {
            std::thread::sleep(COALESCE_WINDOW);
            let (root, scope) = {
                let mut schedule = indexer.schedule();
                let scope = std::mem::take(&mut schedule.pending);
                match (&schedule.root, scope.is_empty()) {
                    (Some(root), false) => (root.clone(), scope),
                    _ => {
                        schedule.scheduled = false;
                        return;
                    }
                }
            };
            let generation = indexer.generation();
            indexer.run_pass(&app, &root, scope, generation);
            let mut schedule = indexer.schedule();
            if schedule.pending.is_empty() || schedule.root.is_none() {
                schedule.scheduled = false;
                return;
            }
        }
    });
}

/// DRS-FR-28 / BMI-FR-19: a draft's file set or a draft file's contents
/// changed.
///
/// Always the drafts half and only that half. A graduation is not an exception
/// (BMI-FR-20): it writes into the project through its own targeted operation
/// and leaves the draft where it is, so the published files reach their artifact
/// index through the watcher's report of those paths like any other project
/// write, and this channel carries nothing about the project tree at all.
pub fn note_draft_change<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    change: &crate::drafts::DraftChange,
) {
    let scope = scope_for_draft_change(change);
    if let Some(indexer) = app.try_state::<Bm25Indexer>() {
        indexer.record_draft_change(change);
    }
    request_pass(app, scope);
}

/// Which halves of the index set a draft change dirties: the drafts half, and
/// never the artifact half (BMI-FR-20).
pub fn scope_for_draft_change(_change: &crate::drafts::DraftChange) -> PassScope {
    PassScope::DRAFTS
}

/// NTC-FR-24 / BMI-FR-29: a note was created, had its `body` rewritten, or was
/// deleted.
///
/// Always the notes part and only that part. A change that leaves the body alone
/// never reaches here at all — moving a note between entities or to and from
/// project scope, following it through a correlated rename, leaving it
/// unresolved when a rename could not be followed, and setting or clearing its
/// reminder each surface nothing, none of them touching what is indexed.
///
/// Notes travel this channel rather than the project watcher because
/// `.synthesis/notes/` is outside what the scan surfaces (ASC-FR-09), so no
/// watcher reports a note file.
pub fn note_notes_change<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    change: &crate::notes::NoteChange,
) {
    let scope = scope_for_note_change(change);
    if let Some(indexer) = app.try_state::<Bm25Indexer>() {
        indexer.record_note_change(change);
    }
    request_pass(app, scope);
}

/// Which part of the index set a note change dirties: the notes part, and never
/// the artifact or drafts parts (BMI-FR-29).
///
/// Pure and separate from [`note_notes_change`] for the reason
/// [`scope_for_draft_change`] is: `request_pass` returns before doing anything
/// when nothing is mounted, so the scope a channel chooses has no observable of
/// its own and would otherwise be assertable only by inspecting the code that
/// chooses it.
pub fn scope_for_note_change(_change: &crate::notes::NoteChange) -> PassScope {
    PassScope::NOTES
}

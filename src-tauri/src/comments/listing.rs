//! The list operations: every discussion of an owner or of the project.

use super::*;

/// CMS-FR-23 / CMS-FR-31: every thread in the artifact's log as it stands on disk
/// right now. No thread is withheld — which of them the rail renders where is its
/// decision (CMT-FR-17, CMT-FR-19).
pub fn list_fragment_discussions_in(root: &crate::fs::RootFs, artifact_id: &str) -> Vec<Discussion> {
    fold_events(artifact_id, read_events(root, artifact_id))
}

/// One discussion of a project-wide listing, together with what the filesystem
/// currently says about its owner (CMS-FR-32).
///
/// Mirrors `notes::NoteListItem`: the record as stored, plus the resolution the
/// panel needs and cannot work out for itself. `owner_unavailable` is what
/// `CMP-comments-panel.md` CMP-FR-12 renders as a last-known path that opens
/// nothing: the artifact file is gone, the draft is gone, or the note is gone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscussionListItem {
    pub discussion: Discussion,
    pub owner_unavailable: bool,
}

/// The path recorded when a discussion was opened, from the first opening event
/// in a log.
///
/// Informational in the log and *stale after a rename*, because the log is
/// append-only and `follow_rename` moves the file rather than rewriting its lines.
/// It is the starting point for `resolve_log_artifact`, never the answer on its own.
///
/// `owner_is_draft` selects whose paths count. A draft's log names a draft file
/// through its fragment; in the committed folder only an artifact owner counts, so
/// a draft's log misplaced there contributes nothing rather than leaking into the
/// project-wide read (CMS-FR-33).
pub(super) fn recorded_artifact_path(events: &[Event], owner_is_draft: bool) -> Option<&str> {
    events.iter().find_map(|e| match &e.body {
        EventBody::ThreadOpened { artifact_path, .. } => Some(artifact_path.as_str()),
        EventBody::DiscussionOpened {
            fragment_target: Some(fragment),
            ..
        } if owner_is_draft == matches!(fragment.owner, DiscussionTarget::Draft { .. }) => {
            Some(fragment.path.as_str())
        }
        EventBody::DiscussionOpened {
            target: Some(DiscussionTarget::Artifact { artifact_id }),
            ..
        } if !owner_is_draft => Some(artifact_id.as_str()),
        _ => None,
    })
}

/// CMS-FR-33: which artifact a log belongs to, given the log's filename stem and
/// the path its own events recorded.
///
/// The *filename* is what binds a log to an artifact (CMS-FR-24) — `follow_rename`
/// keeps it in sync — so the recorded path is authoritative only while it still
/// hashes to the stem. When it does not, the artifact has been renamed since the
/// thread was opened and the recorded path names a file that no longer exists;
/// the current path is recovered from `index`, which inverts `log_id` by
/// enumerating the project's own paths rather than by inverting the hash (which
/// cannot be done).
///
/// Falling back to the recorded path when the index holds no match is what keeps
/// the threads of a deleted artifact — and of one moved by a change this module
/// could not correlate — readable under their last-known path (CMS-FR-25).
pub(super) fn resolve_log_artifact<'a>(
    stem: &str,
    recorded: &'a str,
    index: &'a HashMap<String, String>,
) -> &'a str {
    if log_id(recorded) == stem {
        return recorded;
    }
    index.get(stem).map(|s| s.as_str()).unwrap_or(recorded)
}

/// CMS-FR-34: whether resolving these logs needs the project enumerated.
///
/// True exactly when some log's recorded path no longer hashes to its own
/// filename, which is the only thing a followed rename produces (CMS-FR-24).
/// Building the index walks the project, so a project where nothing has been
/// renamed — the overwhelmingly common case — never pays for it and reads no
/// directory outside the comments folder.
pub(super) fn needs_path_index(logs: &[(String, Vec<Event>)]) -> bool {
    logs.iter().any(|(stem, events)| {
        recorded_artifact_path(events, false).is_some_and(|rec| log_id(rec) != *stem)
    })
}

/// CMS-FR-32: every discussion of the artifact scope, fragment and whole-target.
///
/// Ordered most-recently-active first, which is the order
/// `CMP-comments-panel.md` CMP-FR-09 renders within each of its groups.
///
/// CMS-FR-35: a log that cannot be read is skipped rather than failing the call —
/// one damaged file never costs the reviewer every other conversation in the
/// project, on the same terms a damaged *line* never costs the rest of a log
/// (CMS-FR-08).
pub fn list_all_artifact_discussions_in(
    root: &crate::fs::RootFs,
    worktree: &crate::fs::RootFs,
) -> Vec<DiscussionListItem> {
    let dir = comments_dir(root);
    let Ok(entries) = root.list_dir(&dir) else {
        // No comments folder at all is simply no threads.
        return Vec::new();
    };

    // (stem, events) for every readable log, deferred so the reverse index below
    // is built at most once and only when some log actually needs it.
    let mut logs: Vec<(String, Vec<Event>)> = Vec::new();
    for entry in entries {
        let path = dir.join(&entry.name);
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(text) = root.read_text(&path) else {
            continue; // CMS-FR-35
        };
        logs.push((stem.to_string(), parse_events(&text)));
    }

    let index: HashMap<String, String> = if needs_path_index(&logs) {
        scanning::file_rel_paths(worktree)
            .into_iter()
            .map(|rel| (log_id(&rel), rel))
            .collect()
    } else {
        HashMap::new()
    };

    let mut out: Vec<DiscussionListItem> = Vec::new();
    for (stem, events) in &logs {
        // CMS-FR-33: a log holding no readable opening event names no artifact,
        // so its threads cannot be listed, opened, or attributed to a file.
        let Some(recorded) = recorded_artifact_path(events, false) else {
            continue;
        };
        // CMS-FR-33: the reserved marker is stripped before the derivation is
        // matched, both of a file's logs naming it through one derivation.
        let (derived_stem, is_discussion) = match stem.strip_suffix(DISCUSSION_LOG_MARKER) {
            Some(base) => (base, true),
            None => (stem.as_str(), false),
        };
        let artifact_id = resolve_log_artifact(derived_stem, recorded, &index);
        // Through `file_info`, not `Path::is_file`: the latter follows a
        // symlink, so a link at the id's path would report as a resolved
        // artifact while every guarded read of it is refused (FSA-FR-17).
        let unresolved = !fsa::resolve_under(worktree.path(), artifact_id)
            .ok()
            .and_then(|p| worktree.file_info(p).ok())
            .is_some_and(|i| i.kind == fsa::EntryKind::File);
        // CMS-FR-40: the scope decides what reaches the panel and not the kind, so
        // an artifact's discussions are listed beside the remarks pinned inside it.
        let threads = if is_discussion {
            fold_events_for(FoldTarget::ArtifactDiscussion { artifact_id }, events.clone())
        } else {
            fold_events(artifact_id, events.clone())
        };
        for thread in threads {
            out.push(DiscussionListItem {
                discussion: thread,
                owner_unavailable: unresolved,
            });
        }
    }

    // `updated_at` is fixed-width UTC, so a string comparison is a chronological
    // one. The id breaks a tie so the order is stable across calls rather than
    // following directory order.
    sort_items(&mut out);
    out
}

/// Most recently active first, the id breaking a tie so the order is stable.
pub(super) fn sort_items(items: &mut [DiscussionListItem]) {
    items.sort_by(|a, b| {
        b.discussion
            .updated_at
            .cmp(&a.discussion.updated_at)
            .then_with(|| a.discussion.id.cmp(&b.discussion.id))
    });
}

/// Where a `draft`-scoped log lives (CMS-FR-37): `drafts/<draft-id>/comments/`
/// inside the repository machine store.
///
/// Addressed by the draft's **stable id** alone (`DRS-draft-storage.md`
/// DRS-FR-02), exactly as a statistics log is (`DSS-draft-statistics-storage.md`
/// DSS-FR-KQVN), so renaming a draft, filing it under a folder, and moving it
/// between folders each leave its whole review where it is and nothing is moved
/// with the draft.
///
/// The id is checked before it becomes a path rather than trusted from the
/// caller: a `draft_id` of `..` composes a path that normalises to somewhere
/// still inside the store, so the escape gate alone would admit it, and one of
/// the operations below is a recursive delete.
pub(super) fn draft_comments_dir(root: &crate::fs::RootFs, draft_id: &str) -> Result<PathBuf, String> {
    if !is_storage_id(draft_id) {
        return Err(ERR_DRAFT_NOT_FOUND.to_string());
    }
    fsa::resolve_under(
        root,
        format!("{DRAFT_COMMENTS_ROOT}/{draft_id}/{DRAFT_COMMENTS_SUBDIR}"),
    )
    .map_err(|e| e.to_string())
}

/// CMS-FR-39: the whole of one draft's conversation storage, which
/// `delete_draft_comments` removes in one recursive step.
pub(super) fn draft_storage_dir(root: &crate::fs::RootFs, draft_id: &str) -> Result<PathBuf, String> {
    if !is_storage_id(draft_id) {
        return Err(ERR_DRAFT_NOT_FOUND.to_string());
    }
    fsa::resolve_under(root, format!("{DRAFT_COMMENTS_ROOT}/{draft_id}"))
        .map_err(|e| e.to_string())
}

/// The threads of one draft file, folded from its `draft`-scoped log
/// (CMS-FR-36 / CMS-FR-38). The filename derivation is the same one-way
/// derivation an artifact's uses, applied to the **draft-relative** path.
pub fn list_draft_fragment_discussions_in(root: &crate::fs::RootFs, draft_id: &str, file_rel: &str) -> Vec<Discussion> {
    let Ok(dir) = draft_comments_dir(root, draft_id) else {
        // The draft is gone, or was never there: no threads rather than an
        // enumeration of somewhere else (DRS-FR-36).
        return Vec::new();
    };
    let Ok(path) = fsa::resolve_under(dir, format!("{}.jsonl", log_id(file_rel))) else {
        return Vec::new();
    };
    let Ok(text) = root.read_text(&path) else {
        return Vec::new();
    };
    fold_events_for(
        FoldTarget::DraftFile { draft_id, file_rel },
        parse_events(&text),
    )
}

/// CMS-FR-55: where an artifact's reserved discussion log lives — beside the
/// anchored log its passages' threads append to, under the same committed folder,
/// told apart by the reserved marker alone.
pub(super) fn artifact_discussion_log_rel(artifact_id: &str) -> String {
    format!(
        "{COMMENTS_REL}/{}{DISCUSSION_LOG_MARKER}.jsonl",
        log_id(artifact_id)
    )
}

/// CMS-FR-55 / CMS-FR-58: every discussion about one artifact, folded from its
/// reserved log and ordered by `created_at` ascending.
pub fn fold_artifact_discussion(root: &crate::fs::RootFs, artifact_id: &str) -> Vec<Discussion> {
    let Ok(path) = fsa::resolve_under(root, artifact_discussion_log_rel(artifact_id)) else {
        return Vec::new();
    };
    let Ok(text) = root.read_text(&path) else {
        // No log yet is simply no discussions; the first append creates it.
        return Vec::new();
    };
    fold_events_for(
        FoldTarget::ArtifactDiscussion { artifact_id },
        parse_events(&text),
    )
}

/// CMS-FR-37: the folder holding one note's conversation — its log and its
/// stored attachments together.
///
/// A folder per note rather than a log per note in a shared directory, because
/// that is what makes CMS-FR-64's removal a single recursive delete that cannot
/// take a neighbour's blobs with it. `<note-id>` is the note's own opaque id
/// rather than a derivation of anything (NTC-FR-02), so a note moved between
/// entities keeps the folder it started with and its conversation follows it
/// without a rename.
pub(super) fn note_comments_rel(note_id: &str) -> String {
    format!("{COMMENTS_REL}/{NOTES_SUBDIR}/{note_id}")
}

/// Whether this is a note id at all, checked here rather than trusted from the
/// caller.
///
/// The FSA escape gate is **not** sufficient on its own for this shape: a
/// `note_id` of `../..` composes a path that normalises to somewhere still
/// *inside* the project root, so the gate passes it and the folder named is one
/// this module has no business touching — and one of the operations below is a
/// recursive delete. Both public entry points happen to validate first (the
/// notes store rejects the id before either is reached), but a primitive that is
/// only safe by virtue of its callers is one refactor from not being.
///
/// The rule is the notes store's own (`NTC-FR-02` ids are opaque and
/// alphanumeric), restated as a single-segment check rather than imported, so
/// this module refuses a traversal whatever the notes store later admits.
pub(super) fn is_note_id(note_id: &str) -> bool {
    is_storage_id(note_id)
}

/// Whether an opaque id may become one path segment beneath the store.
///
/// A single segment of alphanumerics, `-` and `_`, checked here rather than
/// trusted from the caller. It binds a note id (`NTC-notes-storage.md`
/// NTC-FR-02) and a draft id (`DRS-draft-storage.md` DRS-FR-02) alike: both
/// reach this module as caller-supplied text, and the escape gate is not
/// sufficient on its own, a `..` segment composing a path that normalises to
/// somewhere still inside the store.
pub(super) fn is_storage_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The absolute path of a note's conversation folder, behind the FSA-FR-10
/// escape gate.
///
/// The gate is load-bearing here rather than belt-and-braces: unlike a
/// `<log-id>`, a note id reaches this from the frontend as caller-supplied text,
/// so a `..` segment would otherwise name any directory the project holds.
pub(super) fn note_comments_dir(root: &crate::fs::RootFs, note_id: &str) -> Result<PathBuf, String> {
    if !is_note_id(note_id) {
        return Err(ERR_NOTE_NOT_FOUND.to_string());
    }
    fsa::resolve_under(root, note_comments_rel(note_id)).map_err(|e| e.to_string())
}

/// CMS-FR-55: the absolute path of a note's one reserved discussion log.
pub(super) fn note_discussion_log_path(root: &crate::fs::RootFs, note_id: &str) -> Result<PathBuf, String> {
    fsa::resolve_under(note_comments_dir(root, note_id)?, DISCUSSION_LOG_NAME)
        .map_err(|e| e.to_string())
}

/// CMS-FR-62: the one discussion a note carries, or none.
///
/// A `Vec` rather than an `Option` so it folds and lists on exactly the terms
/// every other discussion log does; CMS-FR-62 is what keeps it at most one
/// entry, and a hand-edited log holding two is read as the first alone by
/// [`note_discussion_of`].
pub fn fold_note_discussion(root: &crate::fs::RootFs, note_id: &str) -> Vec<Discussion> {
    let Ok(path) = note_discussion_log_path(root, note_id) else {
        return Vec::new();
    };
    let Ok(text) = root.read_text(&path) else {
        // No log yet is simply no discussion; the first append creates it.
        return Vec::new();
    };
    fold_events_for(FoldTarget::NoteDiscussion { note_id }, parse_events(&text))
}

/// CMS-FR-62: the note's one discussion, which is the association itself.
///
/// The first thread the log folds to, so a log that a hand edit or a mismerge
/// left holding two resolves to one conversation rather than to an ambiguity.
pub fn note_discussion_of(root: &crate::fs::RootFs, note_id: &str) -> Option<Discussion> {
    fold_note_discussion(root, note_id).into_iter().next()
}

/// CMS-FR-55: the absolute path of a draft's one reserved discussion log,
/// inside that draft's own comments folder wherever it is filed (CMS-FR-37).
pub(super) fn discussion_log_path(root: &crate::fs::RootFs, draft_id: &str) -> Result<PathBuf, String> {
    fsa::resolve_under(draft_comments_dir(root, draft_id)?, DISCUSSION_LOG_NAME)
        .map_err(|e| e.to_string())
}

/// CMS-FR-55 / CMS-FR-58: every discussion of one draft, folded from its reserved
/// log and ordered by `created_at` ascending — the order they were begun in.
///
/// The read half `list_discussion_threads` funnels through, and the counterpart of
/// [`fold`](list_fragment_discussions_in) for the discussion log.
pub fn fold_discussion(root: &crate::fs::RootFs, draft_id: &str) -> Vec<Discussion> {
    let Ok(path) = discussion_log_path(root, draft_id) else {
        return Vec::new();
    };
    let Ok(text) = root.read_text(&path) else {
        // No log yet is simply no discussions; the first append creates it.
        return Vec::new();
    };
    fold_events_for(FoldTarget::DraftDiscussion { draft_id }, parse_events(&text))
}

/// Every fragment discussion of every file of one draft.
///
/// A draft's log directory is small and holds one draft's logs alone, so this
/// walks it rather than deriving a filename per file: the derivation is one-way
/// (CMS-FR-02), and the caller here may hold a discussion id rather than a path.
pub fn list_all_draft_fragment_discussions_in(root: &crate::fs::RootFs, draft_id: &str) -> Vec<Discussion> {
    let Ok(dir) = draft_comments_dir(root, draft_id) else {
        return Vec::new();
    };
    let Ok(entries) = root.list_dir(&dir) else {
        // No comments folder at all is simply no discussions.
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in entries {
        let path = dir.join(&entry.name);
        if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
            continue;
        }
        // CMS-FR-58: the reserved whole-draft log is reached through
        // `list_discussions` and through nothing else, so it is not one of the
        // per-file logs this enumeration is walking.
        if path.file_name().and_then(|n| n.to_str()) == Some(DISCUSSION_LOG_NAME) {
            continue;
        }
        let Ok(text) = root.read_text(&path) else {
            continue; // CMS-FR-35: one unreadable log never costs the rest.
        };
        let events = parse_events(&text);
        // CMS-FR-33: a log holding no readable opening event names no file.
        let Some(recorded) = recorded_artifact_path(&events, true).map(str::to_string) else {
            continue;
        };
        out.extend(fold_events_for(
            FoldTarget::DraftFile {
                draft_id,
                file_rel: &recorded,
            },
            events.clone(),
        ));
    }
    out
}

/// CMS-FR-58: fragment discussions first, ordered by `fragment_target.start` then
/// `created_at`; whole-target ones after them, ordered by `created_at`.
fn order_owner_discussions(mut items: Vec<Discussion>) -> Vec<Discussion> {
    items.sort_by(|a, b| {
        match (&a.fragment_target, &b.fragment_target) {
            (Some(x), Some(y)) => x.start.cmp(&y.start),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
        .then_with(|| a.created_at.cmp(&b.created_at))
        .then_with(|| a.id.cmp(&b.id))
    });
    items
}

/// CMS-FR-58: every discussion one owner holds, fragment and whole-target.
///
/// A note holds at most one. Nothing is withheld: which surface renders which
/// discussion is the surface's decision.
pub fn list_discussions_in(root: &crate::fs::RootFs, target: &DiscussionTarget) -> Vec<Discussion> {
    let mut items = match target {
        DiscussionTarget::Artifact { artifact_id } => {
            let mut v = list_fragment_discussions_in(root, artifact_id);
            v.extend(fold_artifact_discussion(root, artifact_id));
            v
        }
        DiscussionTarget::Draft { draft_id } => {
            let mut v = list_all_draft_fragment_discussions_in(root, draft_id);
            v.extend(fold_discussion(root, draft_id));
            v
        }
        DiscussionTarget::Note { note_id } => fold_note_discussion(root, note_id),
    };
    items = order_owner_discussions(items);
    items
}

/// The draft ids the store holds conversation storage for.
fn stored_draft_ids(root: &crate::fs::RootFs) -> Vec<String> {
    let Ok(dir) = fsa::resolve_under(root, DRAFT_COMMENTS_ROOT) else {
        return Vec::new();
    };
    let Ok(entries) = root.list_dir(&dir) else {
        return Vec::new();
    };
    let mut ids: Vec<String> = entries
        .into_iter()
        .filter(|e| is_storage_id(&e.name))
        .map(|e| e.name)
        .collect();
    ids.sort();
    ids
}

/// CMS-FR-32: every discussion of every scope the store holds — artifact
/// (fragment and whole-target), draft and note — each with its owner target.
///
/// Ordered most-recently-active first, id breaking a tie. `owner_unavailable` is
/// true where the owner no longer resolves: an artifact path with no file, a draft
/// with no record, a note the project does not hold.
pub fn list_all_discussions_in(
    root: &crate::fs::RootFs,
    worktree: &crate::fs::RootFs,
) -> Vec<DiscussionListItem> {
    let mut out = list_all_artifact_discussions_in(root, worktree);
    for draft_id in stored_draft_ids(root) {
        let owner_unavailable = crate::drafts::read_draft_record(worktree, &draft_id).is_err();
        let target = DiscussionTarget::Draft {
            draft_id: draft_id.clone(),
        };
        for discussion in list_discussions_in(root, &target) {
            out.push(DiscussionListItem {
                discussion,
                owner_unavailable,
            });
        }
    }
    let notes_dir = fsa::resolve_under(root, format!("{COMMENTS_REL}/{NOTES_SUBDIR}")).ok();
    if let Some(entries) = notes_dir.and_then(|dir| root.list_dir(&dir).ok()) {
        let existing = crate::notes::note_ids(worktree);
        for entry in entries {
            if entry.name.starts_with(DISCARDED_PREFIX) {
                continue;
            }
            if let Some(discussion) = note_discussion_of(root, &entry.name) {
                out.push(DiscussionListItem {
                    discussion,
                    owner_unavailable: !existing.contains(&entry.name),
                });
            }
        }
    }
    sort_items(&mut out);
    out
}

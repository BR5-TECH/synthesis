//! Reading the files a pass considers (BMI-FR-03, BMI-FR-04, BMI-FR-09, BMI-FR-26, BMI-FR-28).

use std::collections::BTreeMap;
use std::path::Path;

use tauri::Manager;

use crate::logging::{log_warn, Domain, BUFFER};
use crate::scanning::{ArtifactType, Candidate, CandidateStore};

use super::{documents, FileRef, IndexId, IndexSet, PassScope, SourceFile, MAX_FILE_BYTES};

/// Read every file the pass should consider.
///
/// BMI-FR-03: the artifact half is the scan's candidate list (ASC-FR-17)
/// filtered to the files carrying a resolved type — this module walks no tree
/// of its own and applies no ignore rule of its own, so it inherits ASC-FR-09's
/// exclusions rather than restating them.
///
/// BMI-FR-04: the draft half is one file per draft of the active worktree —
/// that draft's prompt, a draft holding exactly one file (DRS-FR-11) — whatever
/// the draft's status.
///
/// DSL-FR-19: the skills half is enumerated in full on every pass whatever the
/// scope, because its folders may be gitignored and so invisible to the scan
/// that drives the other two halves.
pub(super) fn collect_sources<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    root: &crate::fs::RootFs,
    scope: PassScope,
) -> (
    Vec<SourceFile>,
    Vec<crate::skills::SkillDescriptor>,
    BTreeMap<String, &'static str>,
) {
    let mut sources = Vec::new();
    if scope.artifacts {
        let candidates = app.state::<CandidateStore>().candidates(root);
        for candidate in candidates.iter() {
            let Some(artifact_type) = candidate.artifact_type else {
                continue;
            };
            sources.push(read_artifact(root, candidate, artifact_type));
        }
    }
    if scope.drafts {
        // BMI-FR-19: one draft is one prompt (DRS-FR-11), so the drafts index
        // holds one document per draft rather than one per file. An inconsistent
        // draft contributes none: nothing under it is the prompt, and choosing a
        // file to index would be choosing one on the author's behalf
        // (DRS-FR-15).
        for summary in crate::drafts::list_drafts_impl(root).drafts {
            let Ok(prompt) = crate::drafts::require_prompt(root, &summary.id) else {
                continue;
            };
            sources.push(read_draft_file(root, &summary.id, &prompt));
        }
    }
    if scope.notes {
        // BMI-FR-28: one document per persisted note, and that document is the
        // note's `body`. The bodies arrive already read (BMI-FR-26) — this
        // module performs no read for them, `.synthesis/notes/` being outside
        // what the scan surfaces and the store being the only thing that
        // decides what belongs here (NTC-FR-24).
        sources.extend(collect_notes(root));
    }
    if scope.documents {
        // BMI-FR-WBKZ: the documents arrive already read, on the channel of
        // `DCL-documents-collection.md`; no read is performed here (BMI-FR-26).
        sources.extend(documents::collect_documents(app));
    }

    // DSL-FR-13: the skills index's documents are the descriptors themselves,
    // already read — this module performs no read for them (BMI-FR-26).
    let (skills, excluded) = crate::skills::enumerate(root);
    for skill in &skills {
        sources.push(SourceFile {
            index: IndexId::Skills,
            file: FileRef::artifact(skill.path.clone()),
            text: Ok(skill.document()),
            plain_text: false,
        });
    }
    let exclusions: BTreeMap<String, &'static str> = excluded
        .into_iter()
        .map(|e| (e.path, e.reason))
        .collect();
    (sources, skills, exclusions)
}

/// DSL-FR-24: emit a `WARN` for each skill exclusion that is new, or whose
/// reason changed, since the previous pass.
///
/// An author who wrote a skill and cannot find it has nowhere else to look, so
/// the record has to exist; but it only needs to exist once per time the
/// exclusion actually arises. The path and the reason only — a `SKILL.md` is
/// user content, its reason is a fixed phrase this application chose, and no
/// part of the file itself belongs in a log record.
pub(super) fn log_skill_exclusions<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    previous: &IndexSet,
    exclusions: &BTreeMap<String, &'static str>,
) {
    for (path, reason) in exclusions {
        if previous.skill_exclusion(path) == Some(*reason) {
            continue;
        }
        log_warn(
            app,
            &BUFFER,
            &[Domain::Backend],
            "skill excluded from the index",
            crate::log_fields! {
                "path" => path.clone(),
                "reason" => *reason,
            },
        );
    }
}

pub(super) fn read_artifact(root: &crate::fs::RootFs, candidate: &Candidate, artifact_type: ArtifactType) -> SourceFile {
    let index = IndexId::for_artifact_type(artifact_type);
    let file = FileRef::artifact(candidate.path.clone());
    // BMI-FR-26: resolved through the `FSA-filesystem-access.md` gate rather
    // than joined onto the root, so FSA-FR-10's path-escape rejection binds
    // this read too. A candidate path comes from the scan and should already be
    // inside the root; a plain join would read whatever it pointed at if that
    // ever stopped being true.
    let text = match crate::fs::resolve_under(root, &candidate.path) {
        Ok(abs) => read_bounded(root, &abs),
        Err(e) => Err(format!("refused: {e}")),
    };
    SourceFile {
        index,
        file,
        text,
        plain_text: false,
    }
}

/// BMI-FR-28: the `notes` index's documents — one per persisted note, each the
/// note's whole `body`.
///
/// Two kinds of note contribute no document rather than a truncated or a partial
/// one, and each is a skip with a `WARN` on the terms of BMI-FR-09 rather than a
/// failure of the pass: one the store could not read (NTC-FR-14), and one whose
/// stored body nonetheless exceeds the 1 KiB the store bounds a note at
/// (NTC-FR-23) — which a file hand-edited or merged outside the application can
/// still be. An author whose note cannot be found has nowhere else to look, so
/// the record has to exist.
pub(super) fn collect_notes(root: &crate::fs::RootFs) -> Vec<SourceFile> {
    let (documents, skipped) = crate::notes::note_documents(root);
    let mut sources: Vec<SourceFile> = documents
        .into_iter()
        .map(|document| SourceFile {
            index: IndexId::Notes,
            file: FileRef::note(document.note_id),
            text: if document.body.len() > crate::notes::MAX_NOTE_BODY_BYTES {
                Err(format!(
                    "exceeds the {}-byte note ceiling",
                    crate::notes::MAX_NOTE_BODY_BYTES
                ))
            } else {
                Ok(document.body)
            },
            plain_text: false,
        })
        .collect();
    sources.extend(skipped.into_iter().map(|skip| SourceFile {
        index: IndexId::Notes,
        file: FileRef::note(skip.note_id),
        text: Err(skip.reason.to_string()),
        plain_text: false,
    }));
    sources
}

pub(super) fn read_draft_file(root: &crate::fs::RootFs, draft_id: &str, path: &str) -> SourceFile {
    let file = FileRef::draft(draft_id, path);
    let text = match crate::drafts::draft_file_abs_path(root, draft_id, path) {
        Ok(abs) => read_bounded(root, &abs),
        Err(e) => Err(e),
    };
    SourceFile {
        index: IndexId::Drafts,
        file,
        text,
        plain_text: false,
    }
}

/// BMI-FR-09: a file above the size ceiling, or one that does not decode as
/// UTF-8, contributes no chunks. Both come back as a skip reason rather than as
/// a failure, so the pass carries on and completes.
///
/// The reason names the shape of what was refused — a byte count, a decode
/// failure — never the file's contents, which are user content and belong in no
/// log record.
pub(super) fn read_bounded(root: &crate::fs::RootFs, abs: &Path) -> Result<String, String> {
    // The link itself is inspected rather than its target, and a symlink is
    // never read. `resolve_under` enforces confinement only *syntactically* —
    // it rejects a path that climbs out with `..` — but a symlink sitting
    // inside the root can point anywhere on the machine, and `metadata`/`read`
    // follow it. Indexing through one would put another checkout's bytes, or
    // `/etc`'s, into a chunk attributed to a path under this project, and
    // `search` returns a chunk's text verbatim. This is the same refusal
    // `SCC-search.md`'s content read makes on the identical candidate list.
    let link = match root.file_info(abs) {
        Ok(link) => link,
        Err(e) => return Err(format!("unreadable: {e}")),
    };
    if link.kind == crate::fs::EntryKind::Symlink {
        return Err("refusing to index through a symlink".to_string());
    }
    if link.kind != crate::fs::EntryKind::File {
        return Err("not a regular file".to_string());
    }
    if link.size > MAX_FILE_BYTES {
        return Err(format!("exceeds the {MAX_FILE_BYTES}-byte ceiling"));
    }
    root.read_text(abs).map_err(|e| match e {
        crate::fs::FsError::Utf8 { .. } => "not valid UTF-8 text".to_string(),
        other => format!("unreadable: {other}"),
    })
}

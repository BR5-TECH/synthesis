//! The plan references of a Map turn (AGC-FR-UOPJ, AGC-FR-EFCL;
//! `MPR-map-plan-proposals.md` MPR-FR-HRUK, MPR-FR-CHRV, MPR-FR-SRBV,
//! MPR-FR-IIWW, MPR-FR-EHCG, MPR-FR-VGCX, MPR-FR-AGTT).
//!
//! The references of the delivery plan (the HLD and the other documents the
//! plan answers to) are read from disk at each dispatch and put in one
//! `plan_references` section after `map_context`. No text is kept from one turn
//! to the next, so an edit on disk reaches the next turn. Each reference
//! carries a change mark against the fingerprint record of the discussion, and
//! a turn that delivers replaces that record.

use super::*;
use crate::delivery::source::{resolve_local, Resolved};
use crate::map_plan::{FingerprintRecord, ReferenceFingerprint};

/// MPR-FR-SRBV: what one reference contributes as content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Content {
    /// The UTF-8 text of the file.
    Text(String),
    /// The file is not UTF-8 text.
    NotText,
    /// The file could not be read, with the code of the typed error.
    Unreadable(&'static str),
    /// The id does not resolve to a file of the project.
    Unresolved,
}

/// MPR-FR-SRBV: one plan reference as the turn read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ReadReference {
    pub id: String,
    pub display_name: String,
    pub artifact_type: Option<crate::scanning::ArtifactType>,
    pub content: Content,
    pub fingerprint: Option<String>,
}

/// MPR-FR-EHCG: the change mark of one reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mark {
    First,
    Added,
    Changed,
    Unchanged,
}

impl Mark {
    fn as_str(self) -> &'static str {
        match self {
            Mark::First => "first",
            Mark::Added => "added",
            Mark::Changed => "changed",
            Mark::Unchanged => "unchanged",
        }
    }
}

/// MPR-FR-EHCG: the mark of `reference` against `record`. Two references with
/// no fingerprint are equal. Pure.
pub(super) fn mark_of(record: Option<&FingerprintRecord>, reference: &ReadReference) -> Mark {
    let Some(record) = record else { return Mark::First };
    match record.references.iter().find(|r| r.id == reference.id) {
        None => Mark::Added,
        Some(stored) if stored.fingerprint == reference.fingerprint => Mark::Unchanged,
        Some(_) => Mark::Changed,
    }
}

/// MPR-FR-EHCG: the ids that `record` holds and the plan no longer holds, in
/// the order of the record. Pure.
pub(super) fn removed_ids(
    record: Option<&FingerprintRecord>,
    references: &[ReadReference],
) -> Vec<String> {
    record
        .map(|record| {
            record
                .references
                .iter()
                .filter(|stored| !references.iter().any(|r| r.id == stored.id))
                .map(|stored| stored.id.clone())
                .collect()
        })
        .unwrap_or_default()
}

/// MPR-FR-HRUK: the reference ids of the delivery plan, in the order the plan
/// holds them. A `file` source gives the references of the Flow file, and a
/// `github` source the references of the overlay. The source `none`, and a
/// source that cannot be resolved, give none.
pub(super) fn plan_reference_ids<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
) -> Vec<String> {
    let refused = |what: &str, code: &str| {
        logging::log_warn(
            app,
            &logging::BUFFER,
            &[Domain::Ai, Domain::Backend],
            "map plan references not read",
            log_fields! { "what" => what, "code" => code },
        );
    };
    let resolved = match resolve_local(roots.worktree) {
        Ok((_, resolved)) => resolved,
        Err(e) => {
            refused("delivery source", &e.code);
            return Vec::new();
        }
    };
    match resolved {
        Resolved::File { flow_id } => match flow_reference_ids(roots.worktree, &flow_id) {
            Ok(ids) => ids,
            Err(code) => {
                refused("delivery flow", code);
                Vec::new()
            }
        },
        Resolved::Github { project_node_id } => {
            match crate::delivery::machine_state::read_delivery_overlay(
                app,
                crate::delivery::github::buffer_of(app),
                roots.store,
                &project_node_id,
            ) {
                Ok(overlay) => overlay.references,
                Err(e) => {
                    refused("delivery overlay", &e.code);
                    Vec::new()
                }
            }
        }
        Resolved::None { .. } => Vec::new(),
    }
}

/// MPR-FR-HRUK: the `references` of the Flow file `flow_id`. The read is
/// lenient: only the `references` field is read, so a Flow that breaks another
/// rule still gives its references, and an entry that is not a string is
/// dropped. A Flow with no `references` array gives none. A file that cannot
/// be read, or that is not JSON, is the error code of the refusal.
pub(super) fn flow_reference_ids(
    worktree: &crate::fs::RootFs,
    flow_id: &str,
) -> Result<Vec<String>, &'static str> {
    let path = crate::fs::resolve_under(worktree.path(), flow_id).map_err(|_| "flow_path")?;
    let body = worktree.read_text(path).map_err(|e| read_error_code(&e))?;
    let value = serde_json::from_str::<serde_json::Value>(&body).map_err(|_| "flow_not_json")?;
    Ok(value
        .get("references")
        .and_then(|r| r.as_array())
        .map(|ids| ids.iter().filter_map(|id| id.as_str().map(str::to_string)).collect())
        .unwrap_or_default())
}

/// The code of a failed read, for the input and the log. It holds no path.
fn read_error_code(error: &crate::fs::FsError) -> &'static str {
    match error {
        crate::fs::FsError::NotFound { .. } => "not_found",
        crate::fs::FsError::Io(_) => "io",
        _ => "unreadable",
    }
}

/// MPR-FR-CHRV, MPR-FR-SRBV: read the reference `id` from the file in the
/// worktree, as it is on disk now.
pub(super) fn read_reference(worktree: &crate::fs::RootFs, id: &str) -> ReadReference {
    let file_name = id.rsplit('/').next().unwrap_or(id).to_string();
    let resolved = crate::fs::resolve_under(worktree.path(), id).ok().filter(|path| {
        worktree.file_info(path).is_ok_and(|i| i.kind == crate::fs::EntryKind::File)
    });
    let Some(path) = resolved else {
        return ReadReference {
            id: id.to_string(),
            display_name: file_name,
            artifact_type: None,
            content: Content::Unresolved,
            fingerprint: None,
        };
    };
    let artifact_type = crate::artifacts::resolved_type(worktree, id);
    let display_name = crate::scanning::file_display_name(worktree, id, artifact_type);
    let (content, fingerprint) = match worktree.read_bytes(&path) {
        Ok(bytes) => {
            let fingerprint = crate::map_plan::fingerprint_of(&bytes);
            match String::from_utf8(bytes) {
                Ok(text) => (Content::Text(text), Some(fingerprint)),
                Err(_) => (Content::NotText, Some(fingerprint)),
            }
        }
        Err(e) => (Content::Unreadable(read_error_code(&e)), None),
    };
    ReadReference { id: id.to_string(), display_name, artifact_type, content, fingerprint }
}

/// MPR-FR-AGTT: the counts of one assembly. They hold no text, name, or path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Counts {
    pub references: usize,
    pub first: usize,
    pub added: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub removed: usize,
    pub unresolved: usize,
    pub not_text: usize,
    pub unreadable: usize,
    pub truncated: bool,
}

/// One line that names the size of a text (AGC-FR-OYVH).
fn size_words(text: &str) -> String {
    let size = DocumentSize::of(text);
    format!(
        "{} lines, {} characters, about {} tokens (an estimate)",
        size.lines, size.characters, size.estimated_tokens
    )
}

/// MPR-FR-HRUK to MPR-FR-EHCG: the `plan_references` section of `references`,
/// with the marks against `record` and the texts bound together by
/// `bound_chars` (MPR-FR-IIWW). Pure.
pub(super) fn render_section(
    references: &[ReadReference],
    record: Option<&FingerprintRecord>,
    bound_chars: usize,
) -> (InputSection, Counts) {
    let removed = removed_ids(record, references);
    let mut counts =
        Counts { references: references.len(), removed: removed.len(), ..Counts::default() };
    let mut body = String::new();
    if references.is_empty() {
        body.push_str("The delivery plan has no plan reference.\n");
    } else {
        body.push_str(&format!(
            "The delivery plan has {} plan references. Each text below is the file as it is on disk at this turn.\n",
            references.len()
        ));
    }
    let mut remaining = bound_chars;
    for (index, reference) in references.iter().enumerate() {
        let mark = mark_of(record, reference);
        match mark {
            Mark::First => counts.first += 1,
            Mark::Added => counts.added += 1,
            Mark::Changed => counts.changed += 1,
            Mark::Unchanged => counts.unchanged += 1,
        }
        body.push_str(&format!("\n### Reference {}: \"{}\"\n", index + 1, reference.display_name));
        body.push_str(&format!("path: {}\n", reference.id));
        let type_word = reference.artifact_type.map(|t| t.as_str()).unwrap_or("none");
        body.push_str(&format!("type: {type_word}\n"));
        body.push_str(&format!("change: {}\n", mark.as_str()));
        match &reference.content {
            Content::Unresolved => {
                counts.unresolved += 1;
                body.push_str(
                    "unresolved: no file of the project has this path. There is no text.\n",
                );
            }
            Content::NotText => {
                counts.not_text += 1;
                if reference.artifact_type == Some(crate::scanning::ArtifactType::Doc) {
                    body.push_str("The file is not text. Read it with `get_document`.\n");
                } else {
                    body.push_str("The file is not text. There is no text.\n");
                }
            }
            Content::Unreadable(code) => {
                counts.unreadable += 1;
                body.push_str(&format!(
                    "The file could not be read (error {code}). There is no text.\n"
                ));
            }
            Content::Text(text) => {
                let length = text.chars().count();
                if length <= remaining {
                    remaining -= length;
                    body.push_str("--- text ---\n");
                    body.push_str(text);
                    if !text.ends_with('\n') {
                        body.push('\n');
                    }
                    body.push_str("--- end of text ---\n");
                } else if remaining > 0 {
                    counts.truncated = true;
                    let part: String = text.chars().take(remaining).collect();
                    body.push_str(&format!(
                        "The text is cut after {remaining} characters. The whole file has {}. Read the rest with `read_file`.\n",
                        size_words(text)
                    ));
                    body.push_str("--- text (first part) ---\n");
                    body.push_str(&part);
                    body.push_str("\n--- the text is cut here ---\n");
                    remaining = 0;
                } else {
                    counts.truncated = true;
                    body.push_str(&format!(
                        "The text is left out because the size limit is reached. The file has {}. Read it with `read_file`.\n",
                        size_words(text)
                    ));
                }
            }
        }
    }
    if !removed.is_empty() {
        body.push_str("\nReferences that the plan no longer holds since the last turn:\n");
        for id in &removed {
            body.push_str(&format!("- path: {id}, change: removed\n"));
        }
    }
    if counts.truncated {
        body.push_str("\n[This material is partial: some texts are cut or left out.]\n");
    }
    let mut attributes = vec![("count".to_string(), references.len().to_string())];
    if counts.truncated {
        attributes.push(("truncated".to_string(), "true".to_string()));
    }
    let section = InputSection {
        tag: TAG_PLAN_REFERENCES.to_string(),
        attributes,
        parts: vec![InputPart::Text(body.clone())],
        body,
        truncated: counts.truncated,
    };
    (section, counts)
}

/// The fingerprints that a turn's input was assembled with, for the record of
/// AGC-FR-EFCL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AssembledFingerprints {
    pub discussion_id: String,
    pub references: Vec<ReferenceFingerprint>,
}

/// AGC-FR-UOPJ, MPR-FR-HRUK: put the `plan_references` section after
/// `map_context` in `sections`, and return the fingerprints it was assembled
/// with. Where `sections` holds no `map_context`, it adds nothing.
pub(super) fn extend_plan_references<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    budget: &DocumentBudget,
    discussion_id: &str,
    sections: &mut Vec<InputSection>,
) -> Option<AssembledFingerprints> {
    let at = sections.iter().position(|s| s.tag == TAG_MAP_CONTEXT)?;
    let references: Vec<ReadReference> = plan_reference_ids(app, roots)
        .iter()
        .map(|id| read_reference(roots.worktree, id))
        .collect();
    let record = crate::map_plan::read_fingerprints(roots.store, discussion_id);
    let (section, counts) = render_section(&references, record.as_ref(), budget.bound_chars());
    sections.insert(at + 1, section);
    logging::log_debug(
        app,
        &logging::BUFFER,
        &[Domain::Ai],
        "map plan references for a turn",
        log_fields! {
            "discussionId" => discussion_id,
            "references" => counts.references,
            "first" => counts.first,
            "added" => counts.added,
            "changed" => counts.changed,
            "unchanged" => counts.unchanged,
            "removed" => counts.removed,
            "unresolved" => counts.unresolved,
            "notText" => counts.not_text,
            "unreadable" => counts.unreadable,
            "truncated" => counts.truncated,
        },
    );
    Some(AssembledFingerprints {
        discussion_id: discussion_id.to_string(),
        references: references
            .into_iter()
            .map(|r| ReferenceFingerprint { id: r.id, fingerprint: r.fingerprint })
            .collect(),
    })
}

/// AGC-FR-EFCL, MPR-FR-VGCX: whether a turn that terminated in `state` with
/// the group `role` replaces the fingerprint record. Only a turn whose
/// contribution reaches the discussion does: a turn outside a group, a Meta
/// turn, or a Synthesis turn, that delivered or asked. Pure.
pub(super) fn records_fingerprints(role: Option<GroupRole>, state: AgentTurnState) -> bool {
    !matches!(role, Some(GroupRole::Advisor | GroupRole::Participant))
        && matches!(state, AgentTurnState::Delivered | AgentTurnState::AwaitingReply)
}

/// AGC-FR-EFCL, MPR-FR-VGCX: replace the fingerprint record of the discussion
/// with the fingerprints of a turn that terminated `delivered` or
/// `awaiting_reply`. A failed write logs its typed error and changes nothing
/// else.
pub(super) fn record_fingerprints<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static logging::LogBuffer,
    store: &crate::fs::RootFs,
    assembled: &AssembledFingerprints,
) {
    let record = FingerprintRecord {
        discussion_id: assembled.discussion_id.clone(),
        references: assembled.references.clone(),
    };
    match crate::map_plan::write_fingerprints(store, &record) {
        Ok(()) => logging::log_debug(
            app,
            buffer,
            &[Domain::Ai, Domain::Backend],
            "map plan reference fingerprints recorded",
            log_fields! {
                "discussionId" => &assembled.discussion_id,
                "references" => assembled.references.len(),
            },
        ),
        Err(e) => logging::log_warn(
            app,
            buffer,
            &[Domain::Ai, Domain::Backend],
            "map plan reference fingerprints not recorded",
            log_fields! { "discussionId" => &assembled.discussion_id, "code" => &e.code },
        ),
    }
}

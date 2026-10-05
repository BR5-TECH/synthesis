//! The Tauri commands of the notes contract surface.

use super::*;

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_notes_for_entity(
    entity_id: String,
    project: State<'_, ProjectState>,
) -> Result<Vec<NoteListItem>, String> {
    let root = project.require_root()?;
    let store = project.require_store()?;
    Ok(list_notes_for_entity_in(&root, &store, &entity_id))
}

#[tauri::command]
pub fn list_project_notes(project: State<'_, ProjectState>) -> Result<Vec<NoteListItem>, String> {
    let root = project.require_root()?;
    let store = project.require_store()?;
    Ok(list_project_notes_in(&root, &store))
}

#[tauri::command]
pub fn list_all_notes(project: State<'_, ProjectState>) -> Result<Vec<NoteListItem>, String> {
    let root = project.require_root()?;
    let store = project.require_store()?;
    Ok(list_all_notes_in(&root, &store))
}

/// What a record about a note may say.
///
/// The note's **id** and the *shape* of its body — a byte count — and nothing
/// else. A note's body is what an author wrote to themselves about their own
/// project, and nothing downstream redacts anything, so no part of it reaches a
/// record here any more than it reaches one in
/// `../tools/NST-note-search-tool.md`'s tool (NST-FR-20). The byte count is what
/// makes a `note_body_too_large` refusal followable — it says how far over the
/// bound the author was — and it cannot leak, being a number.
fn note_info<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    message: &str,
    fields: crate::logging::Fields,
) {
    crate::logging::log_info(
        app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        message,
        fields,
    );
}

/// The same, for an operation that did not land.
fn note_warn<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    message: &str,
    fields: crate::logging::Fields,
) {
    crate::logging::log_warn(
        app,
        &crate::logging::BUFFER,
        &[crate::logging::Domain::Backend],
        message,
        fields,
    );
}

/// NTC-FR-04, and NTC-FR-24: a creation is a change to the set of notes, so it
/// surfaces on the internal channel the `notes` index consumes (BMI-FR-29).
///
/// Both outcomes are reported. The refusal especially: `note_body_too_large`
/// (NTC-FR-23) is the one an author hits in ordinary use, and a handled error
/// that reports nothing is invisible in the one place anybody will look.
#[tauri::command]
pub fn create_note<R: tauri::Runtime>(
    scope: NoteScope,
    body: String,
    reminder: Option<String>,
    revision: Option<String>,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
) -> Result<Note, String> {
    let root = project.require_root()?;
    let body_bytes = body.len();
    match create_note_in(&root, scope, body, reminder, revision, &now_rfc3339()) {
        Ok(note) => {
            note_info(
                &app,
                "note created",
                crate::log_fields! { "noteId" => note.id.clone(), "bodyBytes" => body_bytes },
            );
            announce_note_change(&app, NoteChange::to(&note.id));
            Ok(note)
        }
        Err(err) => {
            note_warn(
                &app,
                "note could not be created",
                crate::log_fields! { "bodyBytes" => body_bytes, "reason" => err.clone() },
            );
            Err(err)
        }
    }
}

/// NTC-FR-05, and NTC-FR-24: a **body** rewrite surfaces on the internal
/// channel; an update that changed only the reminder or only the scope does not,
/// neither of them changing what is indexed (BMI-FR-29).
#[tauri::command]
pub fn update_note<R: tauri::Runtime>(
    id: String,
    fields: NoteFields,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
) -> Result<Note, String> {
    let root = project.require_root()?;
    // The shape of what was submitted, not what it said: whether a body was
    // named at all is what decides whether NTC-FR-23 applies, and its length is
    // what makes a refusal followable.
    let body_bytes = fields.body.as_ref().map(String::len);
    match update_note_at(&root, &id, fields, &now_rfc3339()) {
        Ok((note, body_changed)) => {
            note_info(
                &app,
                "note updated",
                crate::log_fields! {
                    "noteId" => note.id.clone(),
                    "bodyBytes" => body_bytes,
                    // What decides whether the `notes` index has work to do
                    // (BMI-FR-29), and so the field to read when a search is not
                    // finding what an author just wrote.
                    "bodyChanged" => body_changed,
                },
            );
            if body_changed {
                announce_note_change(&app, NoteChange::to(&note.id));
            }
            Ok(note)
        }
        Err(err) => {
            note_warn(
                &app,
                "note could not be updated",
                crate::log_fields! {
                    "noteId" => id.clone(),
                    "bodyBytes" => body_bytes,
                    "reason" => err.clone(),
                },
            );
            Err(err)
        }
    }
}

/// NTC-FR-21: one transaction removing the note, its discussion, and that
/// discussion's whole history — and, once it has committed, whatever the running
/// application was still holding for that conversation.
///
/// The turn sweep runs **after** the transaction rather than before, and only on
/// its success: a cleanup that failed leaves the note and its conversation
/// whole, and cancelling an agent mid-answer in a conversation that is still
/// there would cost the author work for nothing (AGC-FR-30).
#[tauri::command]
pub fn delete_note<R: tauri::Runtime>(
    id: String,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
    turns: State<'_, crate::agent_conversations::TurnRegistry>,
    progress: State<'_, crate::progress::ProgressRegistry>,
) -> Result<(), String> {
    let root = project.require_root()?;
    let store = project.require_store()?;
    delete_note_in(&root, &store, &id).inspect_err(|err| {
        crate::logging::log_warn(
            &app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "note deletion failed",
            crate::log_fields! { "noteId" => id.clone(), "error" => err.clone() },
        );
    })?;
    crate::agent_conversations::cancel_turns_for_note(&app, &turns, &progress, &id);
    // NTC-FR-24: a deletion is a change to the set of notes, so the `notes`
    // index loses its document on the next pass (BMI-FR-29). Announced after the
    // transaction committed, never before: a cleanup that failed left the note
    // where it was.
    announce_note_change(&app, NoteChange::to(&id));
    Ok(())
}

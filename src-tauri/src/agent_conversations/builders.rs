//! The per-origin-kind context builders (AGC-FR-05).
//!
//! What a conversation carries differs by the surface it happens in, so a new
//! surface adds an origin kind and a builder here and changes no command
//! signature.

use super::*;

/// The input of a turn and the size of the document it holds (AGC-FR-05,
/// AGC-FR-OYVH). `document` is `None` on a note target and where the document
/// could not be read.
#[derive(Default)]
pub struct BuiltInput {
    pub sections: Vec<InputSection>,
    pub document: Option<DocumentSize>,
}

/// [`build_input_within`] with no known window, which bounds the `artifact`
/// section by the fallback of AGC-FR-XHHT.
pub fn build_input(
    roots: Roots<'_>,
    origin: &ConversationOrigin,
    trigger_comment_id: &str,
) -> Vec<InputSection> {
    build_input_within(roots, origin, trigger_comment_id, &DocumentBudget::unknown()).sections
}

/// AGC-FR-05: the whole of the per-kind dispatch. Adding a conversational
/// surface adds a variant to [`ConversationOrigin`] and an arm here, and changes
/// no command signature, no payload shape, and no requirement of the call.
///
/// AGC-FR-12: a section whose material cannot be read is omitted and the rest
/// are still assembled — one unreadable file never costs the agent the rest of
/// its input. An origin whose discussion cannot be located at all yields an empty
/// input, which the caller turns into `context_unavailable`.
///
/// AGC-FR-GMSF, AGC-FR-XHHT: `budget` decides the bound of the `artifact`
/// section, and every other section keeps the bound of AGC-FR-11.
pub fn build_input_within(
    roots: Roots<'_>,
    origin: &ConversationOrigin,
    trigger_comment_id: &str,
    budget: &DocumentBudget,
) -> BuiltInput {
    let Some(discussion) = resolve_origin(roots, origin) else {
        return BuiltInput::default();
    };
    let mut sections = Vec::new();
    let document = match &discussion.target {
        DiscussionTarget::Note { note_id } => {
            sections.extend(note_section(roots, note_id));
            None
        }
        target => {
            let material = material_of(roots, target, discussion.fragment_target.as_ref(), budget);
            sections.extend(material.section);
            material.document
        }
    };
    sections.push(discussion_history_section(roots, &discussion, trigger_comment_id));
    sections.push(current_comment_section(roots, &discussion, trigger_comment_id));
    BuiltInput { sections, document }
}

/// The discussion an origin names, read from the store through the owner the
/// origin carries.
///
/// `None` where the owner holds no such discussion, which is an origin that names
/// no conversation this build can answer in (AGC-FR-12).
pub fn resolve_origin(roots: Roots<'_>, origin: &ConversationOrigin) -> Option<Discussion> {
    comments::locate_discussion_of(roots.store, &origin.target, origin.discussion_id())
}

/// The origin as the discussion it names holds it: the owner target and the
/// fragment target are read from the store, so a caller cannot misstate either.
/// An origin that resolves to nothing is returned as it came, and fails later as
/// a context that cannot be built (AGC-FR-12).
pub fn normalise_origin(roots: Roots<'_>, origin: ConversationOrigin) -> ConversationOrigin {
    match resolve_origin(roots, &origin) {
        Some(discussion) => ConversationOrigin::of(&discussion),
        None => origin,
    }
}

/// AGC-FR-07: `note_context` holds the note's **current** body, with its scope
/// carried as attributes beside it.
///
/// The entity a note is filed against is named as *scope* and never as material:
/// a note attached to a specification is a remark about something the author
/// noticed, and the file it is filed against is where it is filed rather than
/// what is being discussed — an agent handed that file as the subject would
/// answer a question nobody asked. Where the author does want the file in the
/// conversation they say so in it, and the agent reads it with `read_file` like
/// any other material it decides it needs.
///
/// The body is read at dispatch rather than remembered from when the discussion
/// was opened (AGC-FR-13), so a note edited after its conversation began is
/// discussed as it now reads.
pub(super) fn note_section(roots: Roots<'_>, note_id: &str) -> Option<InputSection> {
    // AGC-FR-12: a note the project no longer holds is omitted and the history
    // is still assembled, exactly as an unreadable file is.
    let note = crate::notes::load_note(roots.worktree, note_id)?;
    let mut attributes = vec![("note_id".to_string(), note_id.to_string())];
    match &note.scope {
        crate::notes::NoteScope::Entity { entity_id, entity_path } => {
            attributes.push(("scope_kind".to_string(), "entity".to_string()));
            // Resolved where the entity still exists; the last-known path
            // where it does not, which is what the panel renders for it too
            // (NTS-FR-23).
            if crate::notes::entity_resolves(roots.worktree, entity_id) {
                attributes.push(("entity_name".to_string(), crate::project::basename(entity_id)));
                attributes.push(("entity_path".to_string(), entity_id.clone()));
            } else {
                let last_known =
                    if entity_path.is_empty() { entity_id.clone() } else { entity_path.clone() };
                attributes.push(("last_known_entity_path".to_string(), last_known));
            }
        }
        crate::notes::NoteScope::Project => {
            attributes.push(("scope_kind".to_string(), "project".to_string()));
        }
    }
    if let Some(revision) = &note.revision {
        attributes.push(("revision".to_string(), revision.clone()));
    }
    Some(section(TAG_NOTE_CONTEXT, attributes, note.body))
}

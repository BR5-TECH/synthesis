//! The per-origin-kind context builders (AGC-FR-05).
//!
//! What a conversation carries differs by the surface it happens in, so a new
//! surface adds an origin kind and a builder here and changes no command
//! signature.

use super::*;

/// AGC-FR-05: the whole of the per-kind dispatch. Adding a conversational
/// surface adds a variant to [`ConversationOrigin`] and an arm here, and changes
/// no command signature, no payload shape, and no requirement of the call.
///
/// AGC-FR-12: a section whose material cannot be read is omitted and the rest
/// are still assembled — one unreadable file never costs the agent the rest of
/// its input. An origin whose discussion cannot be located at all yields an empty
/// vector, which the caller turns into `context_unavailable`.
pub fn build_input(
    roots: Roots<'_>,
    origin: &ConversationOrigin,
    trigger_comment_id: &str,
) -> Vec<InputSection> {
    let Some(discussion) = resolve_origin(roots, origin) else {
        return Vec::new();
    };
    match (&discussion.target, &discussion.fragment_target) {
        (DiscussionTarget::Artifact { artifact_id }, Some(_)) => {
            build_artifact_input(roots, artifact_id, &discussion, trigger_comment_id)
        }
        (DiscussionTarget::Artifact { artifact_id }, None) => {
            build_artifact_discussion_input(roots, artifact_id, &discussion, trigger_comment_id)
        }
        (DiscussionTarget::Draft { draft_id }, Some(fragment)) => {
            build_draft_input(roots, draft_id, &fragment.path, &discussion, trigger_comment_id)
        }
        (DiscussionTarget::Draft { draft_id }, None) => {
            build_discussion_input(roots, draft_id, &discussion, trigger_comment_id)
        }
        (DiscussionTarget::Note { note_id }, _) => {
            build_note_discussion_input(roots, note_id, &discussion, trigger_comment_id)
        }
    }
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
fn build_note_discussion_input(
    roots: Roots<'_>,
    note_id: &str,
    thread: &Discussion,
    trigger_comment_id: &str,
) -> Vec<InputSection> {
    let mut sections = Vec::new();
    // AGC-FR-12: a note the project no longer holds is omitted and the history
    // is still assembled, exactly as an unreadable file is.
    if let Some(note) = crate::notes::load_note(roots.worktree, note_id) {
        let mut attributes = vec![("note_id".to_string(), note_id.to_string())];
        match &note.scope {
            crate::notes::NoteScope::Entity {
                entity_id,
                entity_path,
            } => {
                attributes.push(("scope_kind".to_string(), "entity".to_string()));
                // Resolved where the entity still exists; the last-known path
                // where it does not, which is what the panel renders for it too
                // (NTS-FR-23).
                if crate::notes::entity_resolves(roots.worktree, entity_id) {
                    attributes.push((
                        "entity_name".to_string(),
                        crate::project::basename(entity_id),
                    ));
                    attributes.push(("entity_path".to_string(), entity_id.clone()));
                } else {
                    let last_known = if entity_path.is_empty() {
                        entity_id.clone()
                    } else {
                        entity_path.clone()
                    };
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
        sections.push(section(TAG_NOTE_CONTEXT, attributes, note.body));
    }
    sections.push(discussion_history_section(roots, thread, trigger_comment_id));
    sections.push(current_comment_section(roots, thread, trigger_comment_id));
    sections
}

/// AGC-FR-07: `artifact` holds the **whole source of the file the discussion is
/// about**, together with its project-relative path.
///
/// The Markdown of a Markdown artifact, the JSON document of a Flow, and the text
/// of a file carrying no artifact type alike — each read as the bytes on disk
/// decode rather than as any surface renders them, because a discussion is about
/// the file as it is and an agent asked what to do with a workflow is better
/// served by the graph's own document than by a description of it.
fn build_artifact_discussion_input(
    roots: Roots<'_>,
    artifact_id: &str,
    thread: &Discussion,
    trigger_comment_id: &str,
) -> Vec<InputSection> {
    let mut sections = Vec::new();
    // AGC-FR-12: a file that cannot be read is omitted and the remaining sections
    // are still assembled, so one unreadable file never costs the agent the rest.
    if let Ok(path) = crate::fs::resolve_under(roots.worktree, artifact_id) {
        if let Ok(text) = roots.worktree.read_text(&path) {
            let mut attributes = vec![("path".to_string(), artifact_id.to_string())];
            // AGC-FR-07: the type the Library shows for this same path.
            attributes.extend(type_attribute(crate::artifacts::resolved_type(
                roots.worktree,
                artifact_id,
            )));
            sections.push(section(TAG_ARTIFACT, attributes, text));
        }
    }
    sections.push(discussion_history_section(roots, thread, trigger_comment_id));
    sections.push(current_comment_section(roots, thread, trigger_comment_id));
    sections
}

/// AGC-FR-07: `artifact` holds the Markdown source of the artifact the thread is
/// anchored in. An agent asked about a passage sees the passage, where it sits,
/// and the discussion around it.
fn build_artifact_input(
    roots: Roots<'_>,
    artifact_id: &str,
    thread: &Discussion,
    trigger_comment_id: &str,
) -> Vec<InputSection> {
    let mut sections = Vec::new();
    if let Ok(path) = crate::fs::resolve_under(roots.worktree, artifact_id) {
        if let Ok(text) = roots.worktree.read_text(&path) {
            let mut attributes = vec![("path".to_string(), artifact_id.to_string())];
            // AGC-FR-07: the type the Library shows for this same path.
            attributes.extend(type_attribute(crate::artifacts::resolved_type(
                roots.worktree,
                artifact_id,
            )));
            sections.push(section(TAG_ARTIFACT, attributes, text));
        }
    }
    sections.push(discussion_history_section(roots, thread, trigger_comment_id));
    sections.push(current_comment_section(roots, thread, trigger_comment_id));
    sections
}

/// AGC-FR-07: `artifact` holds the single draft file the thread is anchored in,
/// carrying the draft's name and the file's draft-relative path.
///
/// The anchored file rather than the whole draft, because the file the thread is
/// anchored in is the material under discussion and a draft's other files are
/// addressed by threads of their own.
///
/// The two builders differ in what they read and in nothing else: the sections,
/// their order, and their tags are the same, so an agent answers a draft thread
/// and an artifact thread by the same instruction (CVL-FR-03).
fn build_draft_input(
    roots: Roots<'_>,
    draft_id: &str,
    file_rel: &str,
    thread: &Discussion,
    trigger_comment_id: &str,
) -> Vec<InputSection> {
    let mut sections = Vec::new();
    // AGC-FR-12: a file that does not decode as UTF-8 is omitted and the rest of
    // the input is still assembled.
    if let Ok(contents) = crate::drafts::load_draft_file_impl(roots.worktree, draft_id, file_rel) {
        let mut attributes = Vec::new();
        if let Ok(record) = crate::drafts::read_draft_record(roots.worktree, draft_id) {
            attributes.push(("draft".to_string(), record.name.clone()));
        }
        attributes.push(("path".to_string(), file_rel.to_string()));
        // AGC-FR-07: the type this file will carry once the draft graduates.
        attributes.extend(type_attribute(draft_file_type(roots.worktree, draft_id, file_rel)));
        // AGC-FR-35: on a `draft_comment` origin the `artifact` section holds the
        // prompt's text interleaved with the images the prompt embeds, each in
        // the position its Markdown reference occupies.
        sections.push(section_of_parts(
            TAG_ARTIFACT,
            attributes,
            prompt_parts(roots.worktree, draft_id, &contents.body, "", ""),
        ));
    }
    sections.push(discussion_history_section(roots, thread, trigger_comment_id));
    sections.push(current_comment_section(roots, thread, trigger_comment_id));
    sections
}

/// AGC-FR-07: `artifact` holds **every file the draft holds**, in the draft's own
/// tree order, each named by its draft-relative path and carrying the draft's
/// name.
///
/// The whole draft rather than one file of it, because a discussion is about the
/// draft as a whole and an agent asked what to do with it cannot answer from one
/// file. The sections, their order, and their tags are the same as every other
/// builder's — only what is read differs.
fn build_discussion_input(
    roots: Roots<'_>,
    draft_id: &str,
    thread: &Discussion,
    trigger_comment_id: &str,
) -> Vec<InputSection> {
    let mut sections = Vec::new();

    let mut attributes = Vec::new();
    let mut prompt_path = None;
    if let Ok(record) = crate::drafts::read_draft_record(roots.worktree, draft_id) {
        attributes.push(("draft".to_string(), record.name.clone()));
        // AGC-FR-07: the type the draft's one prompt would classify as, which is
        // the artifact the draft is becoming. A draft is one prompt (DRS-FR-11),
        // so one type describes it rather than one per file.
        if let Some(prompt) = record.prompt_path.as_deref() {
            attributes.extend(type_attribute(draft_file_type(roots.worktree, draft_id, prompt)));
            prompt_path = Some(prompt.to_string());
        }
    }
    // AGC-FR-07: the material under discussion is the draft's one prompt.
    let mut parts: Vec<InputPart> = Vec::new();
    if let Some(path) = prompt_path.as_deref() {
        // AGC-FR-12: a prompt that cannot be read — one that does not decode as
        // UTF-8, or one belonging to an inconsistent draft — is left out, and the
        // turn proceeds on its history alone.
        if let Ok(contents) = crate::drafts::load_draft_file_impl(roots.worktree, draft_id, path) {
            // Named by its own draft-relative path, so an agent asked to change
            // it can say which file it means in a `propose_draft_changes` call.
            // AGC-FR-35: the prompt's text interleaved with the images it
            // embeds, each in the position its Markdown reference occupies, so a
            // diagram means what the sentence before it says it means.
            parts = prompt_parts(
                roots.worktree,
                draft_id,
                &contents.body,
                &format!("--- {path} ---\n"),
                "\n\n",
            );
        }
    }
    // Empty exactly where the prompt could not be read at all — no prompt path
    // on the record, or a file that does not decode — which is the one case
    // AGC-FR-12 leaves the section out for. A prompt that is merely empty still
    // makes a section, as it always did: an empty prompt is a thing an author
    // may well want to discuss.
    if !parts.is_empty() {
        sections.push(section_of_parts(TAG_ARTIFACT, attributes, parts));
    }

    sections.push(discussion_history_section(roots, thread, trigger_comment_id));
    sections.push(current_comment_section(roots, thread, trigger_comment_id));
    sections
}

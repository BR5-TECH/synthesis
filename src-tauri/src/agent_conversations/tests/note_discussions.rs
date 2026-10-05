//! Discussions held against a note.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// Note discussions (AGC-FR-06, AGC-FR-07, CVL-FR-03, CVL-FR-06, CVL-FR-08, AGC-FR-30)
// ---------------------------------------------------------------------------

/// The value of one attribute of a section, or a failure naming what was there.
fn attr_of(section: &InputSection, key: &str) -> Option<String> {
    section
        .attributes
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
}


fn entity_scope(entity_id: &str) -> crate::notes::NoteScope {
    crate::notes::NoteScope::Entity {
        entity_id: entity_id.into(),
        entity_path: entity_id.into(),
    }
}

/// AGC-FR-05, AGC-FR-06, AGC-FR-07: the note builder emits exactly `note_context`,
/// `discussion_history`, `current_comment` — and no `artifact` section, so the
/// file the note is filed against is never offered as the subject.
#[test]
fn the_note_builder_gathers_the_note_the_history_and_the_current_comment() {
    let h = Harness::new(vec![]);
    let root = h.root();
    std::fs::create_dir_all(root.path().join("specifications/ui")).unwrap();
    std::fs::write(
        root.path().join("specifications/ui/EDT-editor.md"),
        "UNIQUE-ARTIFACT-TEXT that must not reach the request",
    )
    .unwrap();

    let (_, origin) = seed_note_discussion(
        &h,
        entity_scope("specifications/ui/EDT-editor.md"),
        "the rail's floor is wrong on a narrow tab",
        "@arch is this real?",
    );
    let thread_id = origin.discussion_id().to_string();
    let note_id = origin.note_id().expect("a note origin").to_string();
    // Two earlier comments, then the trigger.
    let target = crate::comments::ThreadRef::note_discussion(&note_id);
    for (body, at) in [
        ("worth checking", "2026-01-01T00:02:00Z"),
        ("I think so", "2026-01-01T00:03:00Z"),
    ] {
        crate::comments::add_comment_to(
            &root,
            target,
            &thread_id,
            body.into(),
            Vec::new(),
            Vec::new(),
            &human("ada"),
            at,
        )
        .unwrap();
    }
    let trigger = crate::comments::add_comment_to(
        &root,
        target,
        &thread_id,
        "@arch so what should I do?".into(),
        Vec::new(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:04:00Z",
    )
    .unwrap()
    .comments
    .last()
    .unwrap()
    .id
    .clone();

    let sections = build_input(Roots::same(&root), &origin, &trigger);

    assert_eq!(
        tags_of(&sections),
        NOTE_INPUT_TAGS.map(str::to_string).to_vec(),
        "exactly the three note tags, in order",
    );
    assert!(
        !sections.iter().any(|s| s.tag == TAG_ARTIFACT),
        "a note turn carries no artifact section",
    );

    let note_ctx = section_of(&sections, TAG_NOTE_CONTEXT);
    assert_eq!(note_ctx.body, "the rail's floor is wrong on a narrow tab");
    assert_eq!(attr_of(note_ctx, "note_id"), Some(note_id));
    assert_eq!(attr_of(note_ctx, "scope_kind"), Some("entity".into()));
    assert_eq!(attr_of(note_ctx, "entity_name"), Some("EDT-editor.md".into()));
    assert_eq!(
        attr_of(note_ctx, "entity_path"),
        Some("specifications/ui/EDT-editor.md".into()),
    );
    assert_eq!(attr_of(note_ctx, "last_known_entity_path"), None);

    // The attached file's *text* reaches nothing.
    let whole: String = sections.iter().map(|s| s.body.clone()).collect();
    assert!(
        !whole.contains("UNIQUE-ARTIFACT-TEXT"),
        "the attached artifact's source is not in the request",
    );

    let history = section_of(&sections, TAG_DISCUSSION_HISTORY);
    assert!(history.body.contains("@arch is this real?"));
    assert!(history.body.contains("worth checking"));
    assert!(history.body.contains("I think so"));
    assert!(
        !history.body.contains("so what should I do?"),
        "the trigger is not in the history",
    );
    assert!(section_of(&sections, TAG_CURRENT_COMMENT)
        .body
        .contains("@arch so what should I do?"));
}

/// AGC-FR-07, AGC-FR-12: the scope attributes across the three shapes a note takes.
#[test]
fn the_note_context_names_the_scope_without_making_it_the_subject() {
    let h = Harness::new(vec![]);
    let root = h.root();

    // Project-wide: neither entity attribute.
    let (_, project_origin) =
        seed_note_discussion(&h, crate::notes::NoteScope::Project, "a project note", "opening");
    let sections = build_input(Roots::same(&root), &project_origin, "no-trigger");
    let ctx = section_of(&sections, TAG_NOTE_CONTEXT);
    assert_eq!(attr_of(ctx, "scope_kind"), Some("project".into()));
    assert_eq!(attr_of(ctx, "entity_name"), None);
    assert_eq!(attr_of(ctx, "entity_path"), None);
    assert_eq!(attr_of(ctx, "last_known_entity_path"), None);

    // Entity-scoped but unresolved: the last-known path, and no resolved pair.
    let (_, gone_origin) =
        seed_note_discussion(&h, entity_scope("specs/gone.md"), "about a deleted file", "opening");
    let sections = build_input(Roots::same(&root), &gone_origin, "no-trigger");
    let ctx = section_of(&sections, TAG_NOTE_CONTEXT);
    assert_eq!(attr_of(ctx, "scope_kind"), Some("entity".into()));
    assert_eq!(
        attr_of(ctx, "last_known_entity_path"),
        Some("specs/gone.md".into()),
    );
    assert_eq!(attr_of(ctx, "entity_name"), None);
    assert_eq!(attr_of(ctx, "entity_path"), None);
    assert!(
        has_material(&sections),
        "an unresolved entity does not stop the turn",
    );

    // AGC-FR-07, AGC-FR-12: an empty note body emits the section carrying an empty body
    // rather than omitting it — the note is the material, and a note the author
    // has not written in yet is still what the conversation is about.
    let (_, empty_origin) =
        seed_note_discussion(&h, crate::notes::NoteScope::Project, "", "opening");
    let sections = build_input(Roots::same(&root), &empty_origin, "no-trigger");
    assert_eq!(
        tags_of(&sections),
        NOTE_INPUT_TAGS.map(str::to_string).to_vec(),
        "the section is emitted, not omitted",
    );
    assert_eq!(section_of(&sections, TAG_NOTE_CONTEXT).body, "");
    assert!(has_material(&sections));

    // A revision, when the note carries one.
    let note = crate::notes::create_note_in(
        &root,
        crate::notes::NoteScope::Project,
        "against a past version".into(),
        None,
        Some("abc123".into()),
        "2026-01-01T00:00:00Z",
    )
    .unwrap();
    let (thread, _) = crate::comments::get_or_create_note_discussion_in(
        &root,
        &root,
        &note.id,
        "opening".into(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:01:00Z",
    )
    .unwrap();
    let sections = build_input(
        Roots::same(&root),
        &ConversationOrigin::of(&thread),
        "no-trigger",
    );
    assert_eq!(
        attr_of(section_of(&sections, TAG_NOTE_CONTEXT), "revision"),
        Some("abc123".into()),
    );
}

/// AGC-FR-07, AGC-FR-13: the body is read at dispatch, so an edited note is discussed as
/// it now reads while the conversation stays the one it always was.
#[test]
fn a_note_edited_after_its_discussion_began_is_discussed_as_it_now_reads() {
    let h = Harness::new(vec![]);
    let root = h.root();
    let (note_id, origin) = seed_note_discussion(
        &h,
        crate::notes::NoteScope::Project,
        "check the FR numbering",
        "opening",
    );
    let thread_id = origin.discussion_id().to_string();

    crate::notes::update_note_in(
        &root,
        &note_id,
        crate::notes::NoteFields {
            body: Some("check the FR numbering and the TS coverage".into()),
            ..Default::default()
        },
        "2026-01-02T00:00:00Z",
    )
    .unwrap();

    let sections = build_input(Roots::same(&root), &origin, "no-trigger");
    assert_eq!(
        section_of(&sections, TAG_NOTE_CONTEXT).body,
        "check the FR numbering and the TS coverage",
    );
    assert!(
        section_of(&sections, TAG_DISCUSSION_HISTORY).body.contains("opening"),
        "every comment posted before the edit is still there",
    );
    assert_eq!(
        crate::comments::note_discussion_of(&root, &note_id).unwrap().id,
        thread_id,
        "the conversation is the one it always was",
    );
}

/// CVL-FR-03, CVL-FR-04, CVL-FR-08: the prompt is `discuss-note.md`, it differs from the other three,
/// and it offers no rewrite.
#[test]
fn a_note_turn_carries_the_note_prompt_and_cannot_propose() {
    let compiled = |kind| compile_prompt(kind, "Argue about structure.", "");
    let note = compiled(OriginKind::NoteDiscussion);

    assert_eq!(note, compile_prompt(OriginKind::NoteDiscussion, "Argue about structure.", ""));
    assert!(note.contains("Argue about structure."));
    for other in [
        OriginKind::ArtifactDiscussion,
        OriginKind::DraftDiscussion,
        OriginKind::ArtifactComment,
    ] {
        assert_ne!(note, compiled(other), "each origin kind has its own prompt");
    }
    assert_eq!(prompt_template(OriginKind::NoteDiscussion), DISCUSS_NOTE_PROMPT_TEMPLATE);

    // CVL-FR-08: a note is not a draft, so no seventh tool and nothing in the
    // instruction telling the agent it may offer a rewrite.
    assert!(!ConversationOrigin::stub_note("t", "n")
    .is_draft());
    let lowered = DISCUSS_NOTE_PROMPT_TEMPLATE.to_lowercase();
    assert!(!lowered.contains("propose"), "the note prompt offers no rewrite");
}

/// CVL-FR-06: prompt/builder tag parity, per origin kind and in order.
#[test]
fn every_prompt_names_exactly_the_tags_its_builder_emits() {
    for kind in [
        OriginKind::ArtifactComment,
        OriginKind::ArtifactDiscussion,
        OriginKind::DraftComment,
        OriginKind::DraftDiscussion,
        OriginKind::NoteDiscussion,
    ] {
        let prompt = prompt_template(kind);
        let expected = input_tags(kind);
        // The tags as the prompt names them, in the order it names them in.
        // Matched on the angle-bracketed form rather than on the bare word, so a
        // prompt that happens to use "artifact" in its prose is not read as
        // naming a section — the tag is `<artifact>` and the parity is about
        // tags.
        let mut named: Vec<&str> = Vec::new();
        let mut cursor = 0usize;
        while cursor < prompt.len() {
            let next = [TAG_ARTIFACT, TAG_NOTE_CONTEXT, TAG_DISCUSSION_HISTORY, TAG_CURRENT_COMMENT]
                .into_iter()
                .filter_map(|tag| {
                    prompt[cursor..]
                        .find(&format!("<{tag}>"))
                        .map(|at| (cursor + at, tag))
                })
                .min();
            match next {
                Some((at, tag)) => {
                    if !named.contains(&tag) {
                        named.push(tag);
                    }
                    cursor = at + tag.len() + 2;
                }
                None => break,
            }
        }
        assert_eq!(
            named,
            expected.to_vec(),
            "{} names exactly the tags its builder emits, in the same order",
            kind.as_str(),
        );
    }
    assert_eq!(input_tags(OriginKind::NoteDiscussion), NOTE_INPUT_TAGS);
    assert_eq!(input_tags(OriginKind::ArtifactDiscussion), INPUT_TAGS);
}

/// AGC-FR-30, AGC-FR-20, AGC-FR-18 (context half): a note that has been deleted leaves the builder
/// with nothing, which the caller turns into `context_unavailable`.
#[test]
fn a_deleted_note_leaves_a_turn_no_material() {
    let h = Harness::new(vec![]);
    let root = h.root();
    let (note_id, origin) = seed_note_discussion(
        &h,
        crate::notes::NoteScope::Project,
        "about to go",
        "opening",
    );
    assert!(has_material(&build_input(Roots::same(&root), &origin, "no-trigger")));

    crate::notes::delete_note_in(&root, &root, &note_id).unwrap();

    let sections = build_input(Roots::same(&root), &origin, "no-trigger");
    assert!(sections.is_empty(), "the conversation went with the note");
    assert!(!has_material(&sections));
}


/// CVL-FR-30 / CVL-FR-03, CVL-FR-04 / CVL-FR-08: a note turn end to end.
///
/// The five new arms the note origin added — the tool set, `deliver`, the lock
/// gate, the prompt, and the builder — are only reachable by actually dispatching
/// one, which the builder-level tests above do not do.
#[test]
fn a_note_turn_is_offered_the_nine_and_delivers_into_the_note_s_conversation() {
    let h = Harness::new(vec![Ok("Split it in two.".into())]);
    h.mount();
    h.create_agent("arch", "");

    let (note_id, origin) = seed_note_discussion(
        &h,
        crate::notes::NoteScope::Project,
        "the rail's floor is wrong",
        "@arch is this real?",
    );
    let thread_id = origin.discussion_id().to_string();
    let trigger = crate::comments::note_discussion_of(&h.root(), &note_id)
        .unwrap()
        .comments[0]
        .id
        .clone();

    let turn = h.dispatch("arch", origin.clone(), &trigger).expect("dispatch");
    let settled = wait_for_terminal(&h, &turn.id);
    assert_eq!(settled.state, AgentTurnState::Delivered, "{settled:?}");

    // CVL-FR-08: the nine and `ask_discussion_questions`, both proposal tools
    // absent — observed on the request the seam was actually given rather than
    // inferred. A note is neither a draft nor a file of the project, so there is
    // nothing about it to rewrite; it is a discussion, so its agent may put
    // several questions at once (ADQ-FR-LFDX).
    let (request, _) = h.seam.requests().into_iter().next().expect("one request");
    let mut expected: Vec<&str> = EXPECTED_TOOLS.to_vec();
    expected.push(DISCUSSION_ONLY_TOOL);
    assert_eq!(
        request
            .tools
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>(),
        expected,
    );
    assert!(
        !request.tools.iter().any(|t| t.name == DRAFT_ONLY_TOOL),
        "a note is not a draft, so there is nothing to propose against",
    );
    assert!(
        !request.tools.iter().any(|t| t.name == ARTIFACT_ONLY_TOOL),
        "nor is it an artifact",
    );
    // CVL-FR-30, CVL-FR-08 / NST-FR-01: the definitions are byte-identical to the ones an
    // artifact turn is offered, so "the same nine everywhere" spans all five
    // origin kinds rather than the four the sweeping test above compares.
    let from_an_artifact_turn = {
        let other = Harness::new(vec![Ok("ok".into())]);
        other.mount();
        other.create_agent("arch", "");
        let thread = other.seed_artifact_thread("spec.md", "Some artifact source here.");
        let t = other
            .dispatch(
                "arch",
                ConversationOrigin::of(&thread),
                &thread.comments[0].id,
            )
            .expect("dispatch");
        wait_for_terminal(&other, &t.id);
        other
            .seam
            .requests()
            .into_iter()
            .next()
            .expect("one request")
            .0
            .tools
    };
    assert_eq!(
        &request.tools[..EXPECTED_TOOLS.len()],
        &from_an_artifact_turn[..EXPECTED_TOOLS.len()],
        "a note turn is offered the same nine, byte for byte",
    );
    // CVL-FR-03: under `discuss-note.md`.
    assert_eq!(request.instructions, compile_prompt(OriginKind::NoteDiscussion, "", ""));
    // AGC-FR-06: and the note's own tags.
    assert_eq!(tags_of(&request.input), NOTE_INPUT_TAGS.map(str::to_string).to_vec());

    // AGC-FR-16: the answer lands in the note's conversation as an agent's
    // comment, on exactly the terms it lands in any other.
    let folded = crate::comments::note_discussion_of(&h.root(), &note_id).unwrap();
    assert_eq!(folded.id, thread_id);
    assert_eq!(folded.comments.len(), 2, "the opening and the answer");
    let answer = folded.comments.last().unwrap();
    assert_eq!(answer.body, "Split it in two.");
    assert!(
        matches!(&answer.author, Participant::Agent { handle, .. } if handle == "arch"),
        "{:?}",
        answer.author,
    );
}

/// AGC-FR-19 / CMS-FR-17: a locked note discussion refuses a dispatch before it
/// registers a turn, exactly as every other conversation does.
#[test]
fn a_locked_note_discussion_refuses_a_dispatch() {
    let h = Harness::new(vec![Ok("Answered.".into())]);
    h.mount();
    h.create_agent("arch", "");

    let (note_id, origin) = seed_note_discussion(
        &h,
        crate::notes::NoteScope::Project,
        "a note",
        "@arch thoughts?",
    );
    let trigger = crate::comments::note_discussion_of(&h.root(), &note_id)
        .unwrap()
        .comments[0]
        .id
        .clone();
    crate::comments::set_lock_to(
        &h.root(),
        crate::comments::ThreadRef::note_discussion(&note_id),
        origin.discussion_id(),
        true,
        &human("ada"),
        "2026-01-01T00:05:00Z",
    )
    .unwrap();

    let err = h.dispatch("arch", origin, &trigger).unwrap_err();
    assert_eq!(err, FAIL_THREAD_LOCKED);
    assert!(h.seam.requests().is_empty(), "no model call was made");
    assert_eq!(
        crate::comments::note_discussion_of(&h.root(), &note_id)
            .unwrap()
            .comments
            .len(),
        1,
        "the conversation gained no line",
    );
}

/// AGC-FR-30: a dispatch for a deleted note's conversation is refused **before
/// it is registered**, so no turn record is created for it by any route.
#[test]
fn a_dispatch_for_a_deleted_note_is_refused_before_registration() {
    let h = Harness::new(vec![Ok("Answered.".into())]);
    h.mount();
    h.create_agent("arch", "");

    let (note_id, origin) = seed_note_discussion(
        &h,
        crate::notes::NoteScope::Project,
        "about to go",
        "@arch thoughts?",
    );
    let trigger = crate::comments::note_discussion_of(&h.root(), &note_id)
        .unwrap()
        .comments[0]
        .id
        .clone();

    crate::notes::delete_note_in(&h.root(), &h.root(), &note_id).unwrap();

    let err = h.dispatch("arch", origin, &trigger).unwrap_err();
    assert_eq!(err, FAIL_CONTEXT_UNAVAILABLE);
    assert!(h.seam.requests().is_empty(), "no model call was made");
    // No turn was registered, so none is outstanding and none was published.
    assert!(
        h.turns().in_flight(None).is_empty(),
        "no turn record exists for a deleted note",
    );
}


/// AGC-FR-12 for a note: the `note_context` section is omitted when the note
/// cannot be read, and the remaining sections are still assembled.
///
/// The only reachable way to exercise the guard around the note read: a note
/// file that is malformed is skipped by the notes store (NTC-FR-14) while its
/// conversation, which lives elsewhere, survives.
#[test]
fn an_unreadable_note_omits_its_section_and_the_turn_proceeds() {
    let h = Harness::new(vec![]);
    let root = h.root();
    let (note_id, origin) = seed_note_discussion(
        &h,
        crate::notes::NoteScope::Project,
        "readable for now",
        "opening",
    );
    assert!(has_material(&build_input(Roots::same(&root), &origin, "no-trigger")));

    // Malform the note's own file, leaving its conversation untouched.
    std::fs::write(
        root.path().join(format!(".synthesis/notes/{note_id}.toml")),
        "this is not a note",
    )
    .unwrap();

    let sections = build_input(Roots::same(&root), &origin, "no-trigger");
    assert_eq!(
        tags_of(&sections),
        vec![
            TAG_DISCUSSION_HISTORY.to_string(),
            TAG_CURRENT_COMMENT.to_string(),
        ],
        "the note section is omitted and the rest are still assembled",
    );
    assert!(
        has_material(&sections),
        "the turn proceeds on its history alone",
    );
}

/// AGC-FR-06: a thread id that is not this note's discussion yields nothing,
/// which the caller turns into `context_unavailable`.
#[test]
fn a_note_origin_naming_another_thread_yields_no_material() {
    let h = Harness::new(vec![]);
    let root = h.root();
    let (note_id, _) = seed_note_discussion(
        &h,
        crate::notes::NoteScope::Project,
        "a note",
        "opening",
    );
    let sections = build_input(
        Roots::same(&root),
        &ConversationOrigin::stub_note("not-this-note's-thread", note_id),
        "no-trigger",
    );
    assert!(sections.is_empty());
    assert!(!has_material(&sections));
}

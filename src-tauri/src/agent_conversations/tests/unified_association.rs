//! The association of a turn with its discussion, under the unified model.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

use crate::comments::{self, DiscussionTarget, FragmentTarget};

// ---------------------------------------------------------------------------
// One discussion id, whatever the discussion is about (AGC-FR-01, AGC-FR-05,
// CMS-FR-54, CMS-FR-83)
// ---------------------------------------------------------------------------

/// A legacy `thread_opened` log for an artifact, written the way an older build
/// wrote it, and the discussion the fold reads out of it.
fn seed_legacy_artifact_thread(h: &Harness, artifact_id: &str, source: &str) -> Discussion {
    std::fs::write(h.root().path().join(artifact_id), source).expect("artifact");
    let log = h
        .root()
        .join(format!("{}/{}.jsonl", crate::repository_store::COMMENTS_DIRNAME, comments::log_id(artifact_id)));
    std::fs::create_dir_all(log.parent().expect("a parent")).expect("comments dir");
    let thread_id = "legacy-artifact-thread";
    let opened = serde_json::json!({
        "v": 1, "eventId": "e1", "threadId": thread_id, "at": "2026-01-01T00:00:00Z",
        "by": { "kind": "human", "login": "ada" },
        "type": "thread_opened",
        "artifactPath": artifact_id,
        "anchor": { "start": 0, "end": 4, "quote": "Some" },
    });
    let commented = serde_json::json!({
        "v": 1, "eventId": "e2", "threadId": thread_id, "at": "2026-01-01T00:00:01Z",
        "by": { "kind": "human", "login": "ada" },
        "type": "comment_added",
        "commentId": "c1", "body": "@arch what do you think?", "quotes": [],
    });
    std::fs::write(&log, format!("{opened}\n{commented}\n")).expect("log");
    comments::list_fragment_discussions_in(&h.root(), artifact_id)
        .into_iter()
        .next()
        .expect("legacy artifact thread")
}

/// Every kind of discussion one owner can carry, as `(label, discussion)`.
fn every_kind_of_discussion(h: &Harness) -> Vec<(&'static str, Discussion)> {
    let root = h.root();
    let human = human("ada");
    let mut out = Vec::new();

    out.push((
        "artifact, fragment",
        h.seed_artifact_thread("fragment.md", "Some artifact source here."),
    ));

    std::fs::write(root.path().join("whole.md"), "Whole file source.").expect("artifact");
    out.push((
        "artifact, whole target",
        comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Artifact {
                artifact_id: "whole.md".into(),
            },
            None,
            "@arch is this ready?".into(),
            Vec::new(),
            &human,
            "2026-01-01T00:00:00Z",
        )
        .expect("whole-target artifact discussion"),
    ));

    out.push((
        "artifact, legacy fragment",
        seed_legacy_artifact_thread(h, "legacy.md", "Some legacy source."),
    ));

    let (draft_id, whole_draft) = seed_discussion(h, &["@arch what now?"]);
    out.push(("draft, whole target", whole_draft));

    let created = crate::drafts::create_draft_at_root(&root, Some("fragment-draft")).expect("draft");
    crate::drafts::save_draft_file_impl(&root, &created.draft.id, &created.file, "Hello prompt.")
        .expect("write");
    out.push((
        "draft, fragment",
        comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Draft {
                draft_id: created.draft.id.clone(),
            },
            Some(FragmentTarget::in_draft(&created.draft.id, &created.file, 0, 5, "Hello")),
            "@arch this opening?".into(),
            Vec::new(),
            &human,
            "2026-01-01T00:00:00Z",
        )
        .expect("draft fragment discussion"),
    ));

    out.push((
        "draft, legacy fragment",
        seed_draft_thread(h, &draft_id, &format!("{draft_id}.md")),
    ));

    let (_, note_origin) = seed_note_discussion(
        h,
        crate::notes::NoteScope::Project,
        "a note body",
        "@arch is this real?",
    );
    out.push((
        "note",
        comments::read_discussion_by_id(&root, &root, note_origin.discussion_id())
            .expect("note discussion"),
    ));
    out
}

/// AGC-FR-01, AGC-FR-05, CMS-FR-54, CMS-FR-83: a turn names the **one** discussion
/// id, and its answer lands in that discussion and in no other, for a fragment and
/// a whole-target discussion alike and for one a legacy log opened.
#[test]
fn a_turn_attaches_to_the_one_discussion_id_for_every_kind_of_discussion() {
    let h = Harness::new(vec![
        Ok("one".into()),
        Ok("two".into()),
        Ok("three".into()),
        Ok("four".into()),
        Ok("five".into()),
        Ok("six".into()),
        Ok("seven".into()),
    ]);
    h.create_agent("arch", "Argue about structure.");
    let discussions = every_kind_of_discussion(&h);
    let root = h.root();

    for (label, discussion) in &discussions {
        let origin = ConversationOrigin::of(discussion);
        assert_eq!(origin.discussion_id(), discussion.id, "{label}: the origin names the id");
        assert_eq!(origin.target, discussion.target, "{label}: the owner is the discussion's");
        assert_eq!(
            origin.fragment_target.is_some(),
            discussion.fragment_target.is_some(),
            "{label}: a fragment is present exactly when the discussion has one",
        );

        let turn = h
            .dispatch("arch", origin, &discussion.comments[0].id)
            .unwrap_or_else(|e| panic!("{label}: dispatch refused: {e}"));
        assert_eq!(turn.origin.discussion_id(), discussion.id, "{label}: the turn names the id");
        let terminal = wait_for_terminal(&h, &turn.id);
        assert_eq!(terminal.origin.discussion_id(), discussion.id, "{label}");
        assert_eq!(terminal.state, AgentTurnState::Delivered, "{label}: {terminal:?}");
    }
    h.settle();

    for (label, before) in &discussions {
        let after = comments::read_discussion_by_id(&root, &root, &before.id)
            .unwrap_or_else(|| panic!("{label}: the discussion is still readable by its id"));
        assert_eq!(after.id, before.id, "{label}");
        assert_eq!(after.target, before.target, "{label}");
        assert_eq!(
            after.fragment_target, before.fragment_target,
            "{label}: an answer moves no fragment",
        );
        assert_eq!(after.comments.len(), before.comments.len() + 1, "{label}: one answer");
        assert!(
            matches!(after.comments.last().unwrap().author, Participant::Agent { .. }),
            "{label}: the answer is the agent's",
        );
        // The owner holds that one discussion once: the answer made no second.
        let held = comments::list_discussions_in(&root, &before.target);
        assert_eq!(
            held.iter().filter(|d| d.id == before.id).count(),
            1,
            "{label}: one record under the owner",
        );
    }
}

/// AGC-FR-YQMD, DSS-FR-XBGA: the `conversation_turn` event of a draft turn carries
/// the discussion id and whether it was fragment-targeted, for a fragment, a
/// whole-target and a legacy-origin discussion alike.
#[test]
fn a_draft_turns_statistics_event_carries_the_one_discussion_id() {
    let h = Harness::new(vec![Ok("one".into()), Ok("two".into()), Ok("three".into())]);
    h.create_agent("arch", "Argue about structure.");
    h.app
        .state::<ProjectState>()
        .set_root(h.root.path().to_path_buf());
    let discussions = every_kind_of_discussion(&h);

    let mut expected: Vec<(String, String, bool)> = Vec::new();
    for (label, discussion) in &discussions {
        let Some(draft_id) = discussion.draft_id() else {
            continue;
        };
        let turn = h
            .dispatch(
                "arch",
                ConversationOrigin::of(discussion),
                &discussion.comments[0].id,
            )
            .unwrap_or_else(|e| panic!("{label}: dispatch refused: {e}"));
        wait_for_terminal(&h, &turn.id);
        expected.push((draft_id.to_string(), discussion.id.clone(), discussion.is_fragment_targeted()));
    }
    h.settle();
    crate::statistics::wait_for_writer();
    assert_eq!(expected.len(), 3, "a whole-target, a fragment and a legacy-origin draft discussion");

    for (draft_id, discussion_id, fragment_targeted) in expected {
        let path = crate::statistics::log_path(&h.root(), &draft_id).expect("path");
        let text = h.root().read_text(&path).expect("log");
        let recorded: Vec<(String, bool)> = text
            .lines()
            .filter_map(|line| serde_json::from_str::<crate::statistics::StatisticsEvent>(line).ok())
            .filter_map(|event| match event.body {
                crate::statistics::EventBody::ConversationTurn {
                    discussion_id,
                    fragment_targeted,
                    ..
                } => Some((discussion_id, fragment_targeted)),
                _ => None,
            })
            .filter(|(recorded_id, _)| *recorded_id == discussion_id)
            .collect();
        assert_eq!(
            recorded,
            vec![(discussion_id.clone(), fragment_targeted)],
            "one conversation_turn line names discussion {discussion_id} of draft {draft_id}",
        );
    }
}

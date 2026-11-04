//! The transcript of a translated voice comment (CMS-FR-UYVK).

use super::*;

fn draft_thread() -> (tempfile::TempDir, String, Discussion) {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Voice");
    let thread = open_discussion(&root, &draft_id, "first", "2026-11-04T10:00:00Z");
    (dir, draft_id, thread)
}

// CMS-FR-UYVK: a comment keeps its transcript, the event carries it, and the
// body is the translation alone.
#[test]
fn a_comment_keeps_its_spoken_original() {
    let (dir, draft_id, thread) = draft_thread();
    let root = crate::fs::RootFs::for_root(dir.path());
    let (folded, _) = add_comment_reporting_id(
        &root,
        ThreadRef::discussion(&draft_id),
        &thread.id,
        "Look at the second part.".into(),
        Vec::new(),
        Vec::new(),
        Some("Посмотри вторую часть.".into()),
        Stamp { by: &human("raver119"), at: "2026-11-04T10:01:00Z" },
    )
    .unwrap();
    let last = folded.comments.last().unwrap();
    assert_eq!(last.body, "Look at the second part.");
    assert_eq!(last.spoken_original.as_deref(), Some("Посмотри вторую часть."));
    let lines = discussion_log_lines(&root, &draft_id);
    assert!(lines.last().unwrap().contains("\"spokenOriginal\""), "{}", lines.last().unwrap());
    // The opening comment had none, and its line holds no such key.
    assert!(folded.comments[0].spoken_original.is_none());
    assert!(!lines.iter().take(2).any(|l| l.contains("spokenOriginal")));
}

// CMS-FR-UYVK: an empty transcript is not written, and a line written without
// the field folds to none.
#[test]
fn an_empty_transcript_is_not_written() {
    let (dir, draft_id, thread) = draft_thread();
    let root = crate::fs::RootFs::for_root(dir.path());
    add_comment_reporting_id(
        &root,
        ThreadRef::discussion(&draft_id),
        &thread.id,
        "typed".into(),
        Vec::new(),
        Vec::new(),
        Some("   ".into()),
        Stamp { by: &human("raver119"), at: "2026-11-04T10:01:00Z" },
    )
    .unwrap();
    let lines = discussion_log_lines(&root, &draft_id);
    assert!(!lines.last().unwrap().contains("spokenOriginal"));
    let event: Event = serde_json::from_str(lines.last().unwrap()).unwrap();
    match event.body {
        EventBody::CommentAdded { spoken_original, .. } => assert_eq!(spoken_original, None),
        other => panic!("not a comment: {other:?}"),
    }
}

// CMS-FR-UYVK: the opening comment of a new discussion can carry a transcript.
#[test]
fn an_opening_comment_can_carry_a_spoken_original() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let draft_id = make_draft(&root, "Voice");
    let thread = open_discussion_spoken_in(
        &root,
        &root,
        &DiscussionTarget::Draft { draft_id: draft_id.clone() },
        None,
        Opening {
            body: "Hello.".into(),
            attachments: Vec::new(),
            spoken_original: Some("Hola.".into()),
        },
        Stamp { by: &human("raver119"), at: "2026-11-04T10:00:00Z" },
    )
    .unwrap();
    assert_eq!(thread.comments[0].spoken_original.as_deref(), Some("Hola."));
    assert_eq!(thread.comments[0].body, "Hello.");
}

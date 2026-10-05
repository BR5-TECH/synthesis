//! Agent participants and the title snapshot (CMS-FR-26, CMS-FR-65).

use super::*;

// -- CMS-FR-26 (agents) -------------------------------------------------

#[test]
fn an_agent_posts_through_the_same_writer_and_folds_the_same_way() {
    // CMS-FR-26: the whole reason the participant is a union.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "can you tighten this?",
        "2026-01-01T00:00:00Z",
    );

    append_as(
        root,
        "specs/a.md",
        &agent("claude_code", "claude"),
        "2026-01-02T00:00:00Z",
        vec![(
            t.id.clone(),
            EventBody::CommentAdded {
                comment_id: "agent-1".into(),
                body: "Shortened to two lines.".into(),
                quotes: vec![],
                attachments: Vec::new(),
            },
        )],
    )
    .unwrap();

    let threads = list_fragment_discussions_in(root, "specs/a.md");
    assert_eq!(threads[0].comments.len(), 2);
    assert!(matches!(
        threads[0].comments[0].author,
        Participant::Human { .. }
    ));
    match &threads[0].comments[1].author {
        Participant::Agent { agent_id, handle, .. } => {
            assert_eq!(agent_id, "claude_code");
            assert_eq!(handle, "claude");
        }
        other => panic!("expected an agent author, got {other:?}"),
    }
    assert_eq!(threads[0].updated_at, "2026-01-02T00:00:00Z");
}

// -- CMS-FR-65, CMS-FR-04, CMS-FR-09 / CMS-FR-07: the title snapshot ---------------------------

/// An agent participant carrying a title snapshot, as every append path this
/// build has stamps one (CMS-FR-65).
fn titled_agent(id: &str, handle: &str, title: &str) -> Participant {
    Participant::Agent {
        agent_id: id.into(),
        handle: handle.into(),
        model: Some("m".into()),
        title: Some(title.into()),
    }
}

fn add_as(root: &crate::fs::RootFs, thread: &str, by: &Participant, id: &str, at: &str) {
    append_as(
        root,
        "specs/a.md",
        by,
        at,
        vec![(
            thread.to_string(),
            EventBody::CommentAdded {
                comment_id: id.into(),
                body: "…".into(),
                quotes: vec![],
                attachments: Vec::new(),
            },
        )],
    )
    .unwrap();
}

#[test]
fn a_title_snapshot_is_what_the_line_recorded_rather_than_what_the_agent_is_now() {
    // CMS-FR-65, CMS-FR-04, CMS-FR-09. The point of the
    // snapshot: an agent retitled after the fact leaves what it already
    // wrote reading as it was written, because nothing rewrites a line and
    // nothing resolves an agent record while folding.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "two specs or one?",
        "2026-01-01T00:00:00Z",
    );

    add_as(
        root,
        &t.id,
        &titled_agent("a1", "arch", "Developer"),
        "c1",
        "2026-01-02T00:00:00Z",
    );
    // The same agent, later, under a different title.
    add_as(
        root,
        &t.id,
        &titled_agent("a1", "arch", "Architect"),
        "c2",
        "2026-01-03T00:00:00Z",
    );

    let title_of = |c: &Comment| match &c.author {
        Participant::Agent { title, .. } => title.clone(),
        other => panic!("expected an agent author, got {other:?}"),
    };
    let before = std::fs::read_to_string(log_path(dir.path(), "specs/a.md").unwrap()).unwrap();
    let threads = list_fragment_discussions_in(root, "specs/a.md");
    let comments = &threads[0].comments;
    assert_eq!(comments.len(), 3);
    assert_eq!(title_of(&comments[1]).as_deref(), Some("Developer"));
    assert_eq!(title_of(&comments[2]).as_deref(), Some("Architect"));

    // Folding read the log and nothing else — the earlier line is still on
    // disk byte-for-byte, so nothing "caught it up" to the newer title.
    let after = std::fs::read_to_string(log_path(dir.path(), "specs/a.md").unwrap()).unwrap();
    assert_eq!(before, after);
    assert!(after.contains("\"title\":\"Developer\""));

    // CMS-FR-65, CMS-FR-04, CMS-FR-09's last clause — the agent going entirely takes nothing with
    // it — needs no arrangement here: `list_fragment_discussions_in` takes a filesystem
    // root and a path, so there is no registry in scope for the fold to
    // consult and no state a test could set up to make it. The signature is
    // the proof, and the two titles above already came out of the log.
}

#[test]
fn an_older_line_folds_to_no_title_and_an_empty_one_folds_to_empty() {
    // CMS-FR-04 / CMS-FR-65, CMS-FR-07. Absence is a state this module
    // preserves rather than papers over: a line written before the field
    // existed carries none, and folding it must not invent one.
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(
        root,
        "specs/a.md",
        anchor(0, 3, "abc"),
        "two specs or one?",
        "2026-01-01T00:00:00Z",
    );

    // The older shape: an agent participant with no `title` key at all.
    add_as(
        root,
        &t.id,
        &agent("a0", "scribe"),
        "c0",
        "2026-01-02T00:00:00Z",
    );
    // An agent that carries no title today: the empty string, recorded.
    add_as(
        root,
        &t.id,
        &titled_agent("a1", "arch", ""),
        "c1",
        "2026-01-03T00:00:00Z",
    );

    let title_of = |c: &Comment| match &c.author {
        Participant::Agent { title, .. } => title.clone(),
        other => panic!("expected an agent author, got {other:?}"),
    };
    let comments = list_fragment_discussions_in(root, "specs/a.md")
        .into_iter()
        .next()
        .unwrap()
        .comments;
    // The distinction survives the round trip in both directions.
    assert_eq!(title_of(&comments[1]), None, "an older line carries none");
    assert_eq!(
        title_of(&comments[2]),
        Some(String::new()),
        "an empty snapshot is empty rather than absent",
    );

    // And absence is absence on disk too: `skip_serializing_if` keeps the
    // key out of the line rather than writing a null a reader would have to
    // interpret.
    let text = std::fs::read_to_string(log_path(dir.path(), "specs/a.md").unwrap()).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    let older = lines.iter().find(|l| l.contains("\"c0\"")).unwrap();
    let empty = lines.iter().find(|l| l.contains("\"c1\"")).unwrap();
    assert!(!older.contains("title"), "the older line gained no key");
    assert!(empty.contains("\"title\":\"\""));
}

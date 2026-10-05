//! NST-FR-17, NST-FR-18, NST-FR-19: freshness, passes, and read-only posture.

use super::*;

// ---------------------------------------------------------------------------
// NST-FR-17, NST-FR-18, NST-FR-19: freshness, passes, and read-only posture
// ---------------------------------------------------------------------------

/// NST-FR-17: the call never blocks on an index pass.
#[test]
fn a_call_made_before_the_first_build_finishes_returns_immediately() {
    // `block_on` polls exactly once and panics on `Pending`, so a call that
    // waited on a pass would fail here rather than hang (TLC-FR-16).
    let fixture = NoteFixture::new();
    fixture.note("a note nobody has indexed yet");
    // Deliberately no `reindex()`: the note is on disk and the index does not
    // know about it, which is the state a call during the first build sees.
    let output = block_on(fixture.tool().call(NoteSearchArgs {
        query: "indexed".into(),
        limit: None,
    }))
    .expect("it answers from what has been indexed so far");
    assert!(output.notes.is_empty());

    fixture.reindex();
    assert_eq!(fixture.search("indexed", None).len(), 1);
}

/// NST-FR-18: a change reaches this tool on the next pass, in every direction.
#[test]
fn a_note_change_reaches_this_tool_on_the_next_pass() {
    let fixture = NoteFixture::new();
    let rewritten = fixture.note("alpha the original wording");
    let removed = fixture.note("bravo the doomed note");
    fixture.reindex();
    assert_eq!(fixture.search("alpha", Some(20)).len(), 1);

    crate::notes::update_note_in(
        &fixture.root(),
        &rewritten,
        crate::notes::NoteFields {
            body: Some("delta the replacement wording".to_string()),
            ..Default::default()
        },
        "2026-02-02T00:00:00Z",
    )
    .expect("the rewrite lands");
    let created = fixture.note("echo the newcomer");
    crate::notes::delete_note_in(&fixture.root(), &fixture.root(), &removed).expect("the deletion lands");
    fixture.reindex();

    assert!(
        fixture.search("alpha", Some(20)).is_empty(),
        "the text the rewrite replaced stops being matchable",
    );
    let delta = fixture.search("delta", Some(20));
    assert_eq!(ids(&delta), vec![rewritten]);
    assert_eq!(
        delta[0].body, "delta the replacement wording",
        "and carries no text of the version it replaced",
    );
    assert_eq!(ids(&fixture.search("echo", Some(20))), vec![created]);
    assert!(fixture.search("bravo", Some(20)).is_empty());
}

/// NST-FR-18: the notes of a newly activated worktree replace the outgoing
/// one's.
#[test]
fn changing_the_active_worktree_replaces_what_is_searchable() {
    let a = NoteFixture::new();
    a.note("golf belongs to worktree A");
    a.reindex();
    assert_eq!(a.search("golf", None).len(), 1);

    let b = tempfile::TempDir::new().unwrap();
    let b_root = crate::changes::canonicalize_lenient(b.path());
    crate::notes::create_note_in(
        &crate::fs::RootFs::for_root(&b_root),
        NoteScope::Project,
        "hotel belongs to worktree B".to_string(),
        None,
        None,
        "2026-01-01T00:00:00Z",
    )
    .expect("the note writes");

    let indexer = a.fixture.app.state::<crate::bm25_index::Bm25Indexer>();
    indexer.mount(&crate::fs::RootFs::for_root(&b_root));
    drop(indexer);
    // The session's reach follows the worktree it governs (FSA-FR-29).
    let session = agent_session(&a.fixture.app, &b_root, "note-search-b");
    let tool = NoteSearchTool::new(a.fixture.handle(), session);

    assert!(
        block_on(tool.call(NoteSearchArgs {
            query: "golf".into(),
            limit: None,
        }))
        .expect("the call succeeds")
        .notes
        .is_empty(),
        "no note of A is returned once B is active",
    );
}

/// NST-FR-19: the tool is read-only.
#[test]
fn searching_leaves_every_note_and_the_worktree_exactly_as_they_were() {
    let fixture = NoteFixture::new();
    let discussed = fixture.note("the teardown of the overlay is unhandled");
    fixture.note("another note about the palette");
    crate::comments::get_or_create_note_discussion_in(
        &fixture.root(),
        &fixture.root(),
        &discussed,
        "what did you mean here?".into(),
        Vec::new(),
        &crate::comments::Participant::Human {
            login: "raver119".into(),
            display_name: None,
            email: None,
        },
        "2026-01-01T00:00:00Z",
    )
    .expect("a discussion opens");
    fixture.reindex();

    let before = tree_fingerprint(&fixture.root().to_path_buf());
    assert!(before.len() >= 3, "precondition: the fixture has content");

    for (query, limit) in [
        ("teardown", None),
        ("palette", Some(1)),
        ("overlay", Some(20)),
        ("nothing at all", Some(7)),
    ] {
        let _ = fixture.search(query, limit);
    }
    // A refusal changes nothing either.
    let _ = fixture.refusal("", None);

    assert_eq!(
        tree_fingerprint(&fixture.root().to_path_buf()),
        before,
        "every note file, every discussion, and everything else is byte-identical",
    );
    // And no note was resolved or marked: `updated_at` is untouched.
    assert_eq!(
        crate::notes::note_record(&fixture.root(), &discussed)
            .unwrap()
            .updated_at,
        "2026-01-01T00:00:00Z",
    );
}

/// Every file under `root`, with its bytes hashed.
fn tree_fingerprint(root: &std::path::Path) -> Vec<(String, String)> {
    fn walk(dir: &std::path::Path, base: &std::path::Path, out: &mut Vec<(String, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else if let Ok(bytes) = std::fs::read(&path) {
                let rel = path.strip_prefix(base).unwrap_or(&path);
                out.push((
                    rel.to_string_lossy().into_owned(),
                    crate::fs::sha256_bytes(&bytes),
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

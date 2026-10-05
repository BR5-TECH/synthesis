//! Which operations surface on the change channel, and what reaches the Logs
//! panel.
//!
//! One part of `mod.rs`, which holds the helpers these use.

use super::*;

/// NTC-FR-11, NTC-FR-24, NTC-FR-14: which operations surface on the channel and which do not.
///
/// Asserted against the **commands**, because that is where the handle to
/// report through exists — an impl has none, and a test over the impls alone
/// would leave the silence untested by testing nothing that could have
/// spoken.
#[test]
fn only_a_creation_a_body_rewrite_and_a_deletion_surface_on_the_channel() {
    use tauri::Manager;
    let dir = temp_root();
    std::fs::create_dir_all(dir.path().join("specifications")).unwrap();
    std::fs::write(dir.path().join("specifications/a.md"), "# A\n").unwrap();
    let app = crate::tools::tests::mock_app();
    app.manage(ProjectState::default());
    app.manage(crate::agent_conversations::TurnRegistry::default());
    app.state::<ProjectState>().set_root(dir.path().to_path_buf());
    let indexer = || app.state::<crate::bm25_index::Bm25Indexer>();

    let note = create_note(
        entity("specifications/a.md"),
        "the first body".to_string(),
        None,
        None,
        app.handle().clone(),
        app.state::<ProjectState>(),
    )
    .expect("the note is created");
    assert_eq!(
        indexer().take_note_changes(),
        vec![NoteChange::to(&note.id)],
        "a creation surfaces, naming the note",
    );

    // A body rewrite surfaces.
    update_note(
        note.id.clone(),
        NoteFields {
            body: Some("the second body".to_string()),
            ..Default::default()
        },
        app.handle().clone(),
        app.state::<ProjectState>(),
    )
    .expect("the rewrite lands");
    assert_eq!(indexer().take_note_changes(), vec![NoteChange::to(&note.id)]);

    // Setting and clearing the reminder, and moving the scope, do not — none
    // of them changes what is indexed.
    for fields in [
        NoteFields {
            reminder: Some(Some("2027-01-01T00:00:00Z".to_string())),
            ..Default::default()
        },
        NoteFields {
            reminder: Some(None),
            ..Default::default()
        },
        NoteFields {
            scope: Some(NoteScope::Project),
            ..Default::default()
        },
        // And an update whose `body` is what the note already holds is no
        // rewrite at all (NTC-FR-06).
        NoteFields {
            body: Some("the second body".to_string()),
            ..Default::default()
        },
    ] {
        update_note(
            note.id.clone(),
            fields,
            app.handle().clone(),
            app.state::<ProjectState>(),
        )
        .expect("the update lands");
        assert_eq!(
            indexer().take_note_changes(),
            Vec::new(),
            "nothing that leaves the body alone surfaces",
        );
    }

    // A rename this module follows surfaces nothing either (NTC-FR-11).
    update_note_in(
        &crate::fs::RootFs::for_root(dir.path()),
        &note.id,
        NoteFields {
            scope: Some(entity("specifications/a.md")),
            ..Default::default()
        },
        "2026-03-03T00:00:00Z",
    )
    .unwrap();
    indexer().take_note_changes();
    std::fs::rename(
        dir.path().join("specifications/a.md"),
        dir.path().join("specifications/b.md"),
    )
    .unwrap();
    follow_rename(
        &crate::fs::RootFs::for_root(dir.path()),
        "specifications/a.md",
        "specifications/b.md",
    );
    assert_eq!(indexer().take_note_changes(), Vec::new());

    // NTC-FR-24, NTC-FR-23, NTC-FR-05: a write the bound refused surfaces nothing — there is no
    // change to the set of notes and none to any body, so the `notes` index
    // has no work to do. Asserted against the **command**, which is the only
    // thing that could have spoken.
    update_note(
        note.id.clone(),
        NoteFields {
            body: Some("a".repeat(MAX_NOTE_BODY_BYTES + 1)),
            ..Default::default()
        },
        app.handle().clone(),
        app.state::<ProjectState>(),
    )
    .expect_err("over the bound");
    assert_eq!(
        indexer().take_note_changes(),
        Vec::new(),
        "a refused update surfaces nothing",
    );
    create_note(
        NoteScope::Project,
        "a".repeat(MAX_NOTE_BODY_BYTES + 1),
        None,
        None,
        app.handle().clone(),
        app.state::<ProjectState>(),
    )
    .expect_err("over the bound");
    assert_eq!(
        indexer().take_note_changes(),
        Vec::new(),
        "a refused creation surfaces nothing",
    );
    // An update naming a note that does not exist likewise.
    update_note(
        "no-such-note".to_string(),
        NoteFields {
            body: Some("anything".to_string()),
            ..Default::default()
        },
        app.handle().clone(),
        app.state::<ProjectState>(),
    )
    .expect_err("no such note");
    assert_eq!(indexer().take_note_changes(), Vec::new());

    // A deletion surfaces.
    delete_note(
        note.id.clone(),
        app.handle().clone(),
        app.state::<ProjectState>(),
        app.state::<crate::agent_conversations::TurnRegistry>(),
        app.state::<crate::progress::ProgressRegistry>(),
    )
    .expect("the deletion lands");
    assert_eq!(indexer().take_note_changes(), vec![NoteChange::to(&note.id)]);
}

/// LGC-FR-01: a note write and a note refusal both reach the Logs panel,
/// and neither carries a word of what the author wrote.
///
/// The refusal matters most: `note_body_too_large` is the one an author hits
/// in ordinary use, and a handled error that reports nothing is invisible in
/// the one place anybody will look.
#[test]
fn a_note_write_and_a_note_refusal_are_both_reported_without_the_body() {
    use tauri::Manager;
    let dir = temp_root();
    let app = crate::tools::tests::mock_app();
    app.manage(ProjectState::default());
    app.state::<ProjectState>().set_root(dir.path().to_path_buf());

    // A token no other test in the process writes, so the scan below cannot
    // be satisfied — or defeated — by somebody else's record.
    let secret = "the overlay leaks a zz-note-log-probe-4c1e of memory";
    let note = create_note(
        NoteScope::Project,
        secret.to_string(),
        None,
        None,
        app.handle().clone(),
        app.state::<ProjectState>(),
    )
    .expect("the note is created");
    update_note(
        note.id.clone(),
        NoteFields {
            body: Some("a".repeat(MAX_NOTE_BODY_BYTES + 1)),
            ..Default::default()
        },
        app.handle().clone(),
        app.state::<ProjectState>(),
    )
    .expect_err("over the bound");

    let records = crate::logging::BUFFER
        .query(
            &crate::logging::LogFilter {
                min_level: crate::logging::LogLevel::Debug,
                domains: vec![crate::logging::Domain::Backend],
                ..crate::logging::LogFilter::default()
            },
            None,
            1000,
        )
        .expect("the filter compiles")
        .records;

    let created = records
        .iter()
        .find(|r| r.message == "note created" && r.fields["noteId"] == note.id.as_str())
        .expect("a creation is reported");
    assert_eq!(created.level, crate::logging::LogLevel::Info);
    assert_eq!(
        created.fields["bodyBytes"],
        serde_json::json!(secret.len()),
        "the shape of the body, never the body",
    );

    let refused = records
        .iter()
        .find(|r| r.message == "note could not be updated" && r.fields["noteId"] == note.id.as_str())
        .expect("a refusal is reported");
    assert_eq!(refused.level, crate::logging::LogLevel::Warn);
    assert_eq!(
        refused.fields["bodyBytes"],
        serde_json::json!(MAX_NOTE_BODY_BYTES + 1),
        "how far over the bound the author was is what makes it followable",
    );
    assert!(refused.fields["reason"]
        .as_str()
        .expect("a reason")
        .starts_with(ERR_NOTE_BODY_TOO_LARGE));

    // Nothing downstream redacts anything, so the emit site is the only
    // place this can be enforced: no part of a note's text is in a record.
    //
    // Scanned over the whole buffer rather than over this test's two
    // records: the claim is that the body reached NO record anywhere, and a
    // scan of the two records this test already read would be satisfied by
    // the very fields it just asserted on. `BUFFER` is process-global and
    // `cargo test` runs in parallel, which is exactly why the token above is
    // one nothing else in the process writes.
    let all = crate::logging::BUFFER
        .query(
            &crate::logging::LogFilter {
                min_level: crate::logging::LogLevel::Debug,
                ..crate::logging::LogFilter::default()
            },
            None,
            crate::logging::BUFFER_CAPACITY,
        )
        .expect("the filter compiles")
        .records;
    let rendered = serde_json::to_string(&all).expect("serializes");
    assert!(
        !rendered.contains("zz-note-log-probe-4c1e"),
        "a note's body must never reach a log record",
    );
    assert!(!rendered.contains(secret));
}

/// NTC-FR-11, NTC-FR-24, NTC-FR-14: a consumer's absence delays no note operation.
#[test]
fn a_note_command_answers_the_same_with_nothing_consuming_the_channel() {
    use tauri::Manager;
    let dir = temp_root();
    // Deliberately no `Bm25Indexer` under management: `note_notes_change`
    // must find nothing and return, rather than panicking or waiting.
    let app = tauri::test::mock_app();
    app.manage(ProjectState::default());
    app.state::<ProjectState>().set_root(dir.path().to_path_buf());

    let note = create_note(
        NoteScope::Project,
        "unheard".to_string(),
        None,
        None,
        app.handle().clone(),
        app.state::<ProjectState>(),
    )
    .expect("the note is created regardless");
    assert_eq!(note.body, "unheard");
    assert_eq!(
        update_note(
            note.id.clone(),
            NoteFields {
                body: Some("still unheard".to_string()),
                ..Default::default()
            },
            app.handle().clone(),
            app.state::<ProjectState>(),
        )
        .expect("the update lands regardless")
        .body,
        "still unheard",
    );
}

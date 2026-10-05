//! The attempt record and the store it lives in (DRS-FR-EJBM, DRS-FR-PSCH,
//! DRS-FR-VDQR, DRS-FR-JOEV, DRS-FR-XNLP, DRS-FR-WBTA, DRS-FR-ULKN,
//! GHP-FR-RUYT, GHP-FR-FQIZ, GHP-FR-XOBH, GHP-FR-ZFPI, GHP-FR-NAXT,
//! GHP-FR-EBSA).

use super::*;

/// DRS-FR-PSCH / DRS-FR-ULKN: absent until the first attempt, and a draft that
/// predates publication reads as an empty history rather than as an error.
#[test]
fn the_store_is_absent_until_the_first_attempt() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    let path = crate::drafts::draft_publication_path(&root, &id).unwrap();
    assert!(!path.exists());
    let empty = store::read_store(&root, &id).unwrap();
    assert!(empty.publication.is_empty());
    assert_eq!(empty.attempt, None);

    flow::open_attempt(&root, &id, &origin(), "pub-1".into()).unwrap();
    assert!(path.exists());
}

/// GHP-FR-RUYT / DRS-FR-EJBM: the attempt record carries the draft's resolved
/// remote, repository, marker, and state, and it survives a reload — which is
/// what makes it recoverable after a restart (GHP-FR-ZFPI).
#[test]
fn the_attempt_record_is_written_before_any_request_and_survives_a_reload() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    flow::open_attempt(&root, &id, &origin(), "pub-abc".into()).unwrap();

    // Read through a second RootFs, which is what a relaunch amounts to.
    let reloaded = store::read_store(&crate::fs::RootFs::for_root(dir.path()), &id).unwrap();
    let attempt = reloaded.attempt.unwrap();
    assert_eq!(attempt.marker, "pub-abc");
    assert_eq!(attempt.remote_name, "origin");
    assert_eq!(attempt.remote_url, "github.com/acme/widgets");
    assert_eq!(attempt.repository_owner, "acme");
    assert_eq!(attempt.repository_name, "widgets");
    assert_eq!(attempt.state, AttemptState::Open);
}

/// DRS-FR-VDQR / GHP-FR-EBSA: the three transitions, and cancelling the
/// recovery choice returns the attempt to `open` with no history entry.
#[test]
fn an_attempt_moves_between_its_two_non_terminal_states() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    flow::open_attempt(&root, &id, &origin(), "pub-1".into()).unwrap();
    flow::set_attempt_state(&root, &id, AttemptState::AwaitingChoice).unwrap();
    assert_eq!(store::read_store(&root, &id).unwrap().attempt.unwrap().state, AttemptState::AwaitingChoice);
    flow::set_attempt_state(&root, &id, AttemptState::Open).unwrap();
    let store = store::read_store(&root, &id).unwrap();
    assert_eq!(store.attempt.unwrap().state, AttemptState::Open);
    assert!(store.publication.is_empty());
}

/// DRS-FR-JOEV / GHP-FR-NAXT: an abandoned attempt is cleared with nothing
/// appended, and a state change against no attempt is the typed refusal.
#[test]
fn abandoning_an_attempt_clears_it_and_appends_nothing() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    flow::open_attempt(&root, &id, &origin(), "pub-1".into()).unwrap();
    flow::clear_attempt(&root, &id).unwrap();
    let store = store::read_store(&root, &id).unwrap();
    assert_eq!(store.attempt, None);
    assert!(store.publication.is_empty());
    assert_eq!(flow::clear_attempt(&root, &id), Err(ERR_NO_ATTEMPT.to_string()));
    assert_eq!(
        flow::set_attempt_state(&root, &id, AttemptState::Open),
        Err(ERR_NO_ATTEMPT.to_string())
    );
}

/// DRS-FR-JOEV: completing an attempt appends the record and clears the
/// attempt in one write. DRS-FR-XNLP: the newest record is the current one and
/// no earlier record is disturbed.
#[test]
fn completing_an_attempt_appends_one_record_and_clears_it() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    for (index, number) in [11u64, 12, 13].iter().enumerate() {
        let attempt =
            flow::open_attempt(&root, &id, &origin(), format!("pub-{index}")).unwrap();
        let issue = IssueRef {
            number: *number,
            url: format!("https://github.com/acme/widgets/issues/{number}"),
            title: "widget".into(),
            body: "body".into(),
            ..Default::default()
        };
        store::complete_attempt(&root, &id, flow::record_of(&attempt, &issue)).unwrap();
    }
    let store = store::read_store(&root, &id).unwrap();
    assert_eq!(store.attempt, None);
    // Stored oldest first.
    assert_eq!(
        store.publication.iter().map(|r| r.issue_number).collect::<Vec<_>>(),
        vec![11, 12, 13]
    );
    assert_eq!(store::current_of(&store).unwrap().issue_number, 13);
    // Served newest first.
    assert_eq!(
        store::history_newest_first(&store).iter().map(|r| r.issue_number).collect::<Vec<_>>(),
        vec![13, 12, 11]
    );
    // Every record keeps its own marker and provider.
    assert_eq!(store.publication[0].marker, "pub-0");
    assert!(store.publication.iter().all(|r| r.provider == "github"));
}

/// DRS-FR-EJBM: a store that no longer parses is a typed error against that
/// draft alone, and nothing repairs or rewrites it.
#[test]
fn an_unparseable_store_is_reported_rather_than_repaired() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    let path = crate::drafts::draft_publication_path(&root, &id).unwrap();
    std::fs::write(&path, "<<<<<<< HEAD\nnot toml\n").unwrap();
    assert_eq!(store::read_store(&root, &id), Err(ERR_STORE_WRITE_FAILED.to_string()));
    assert!(std::fs::read_to_string(&path).unwrap().contains("<<<<<<<"));
}

// DRS-FR-WBTA / DRS-FR-ZIVL / GHP-FR-RUYT: the store is private draft
// storage, so no draft-event commit names it.
#[test]
fn the_publication_store_is_named_by_no_event_commit() {
    let id = "0197c2a1b3f-0001-deadbeef";
    assert!(!crate::drafts::is_draft_event_rel(&format!("{id}/publication.toml"), id, false));
    assert!(!crate::drafts::is_draft_event_rel(&format!("UI/{id}/publication.toml"), id, false));
    // A file the author put under the drafts root of that name is theirs.
    assert!(!crate::drafts::is_draft_event_rel("Research/publication.toml", id, false));
}

/// GHP-FR-FQIZ: the marker is opaque, unique per attempt, and written on its
/// own line in the fixed format. GHP-FR-XOBH: the body is the whole prompt
/// followed by that line and nothing else, and the title is the draft's name.
#[test]
fn the_issue_body_is_the_prompt_plus_one_marker_line() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "Widget window");
    save_prompt(&root, &id, "# Widget\n\nDo the thing.\n");

    let first = flow::new_marker();
    let second = flow::new_marker();
    assert_ne!(first, second);
    assert!(first.starts_with("pub-"));

    let content = flow::issue_content(&root, &id, &first).unwrap();
    assert_eq!(content.title, "Widget window");
    assert_eq!(content.body, format!("# Widget\n\nDo the thing.\n\n<!-- synthesis-publication-marker: {first} -->"));
    // The marker never reaches the prompt itself.
    let prompt = crate::drafts::read_draft_prompt(&root, &id).unwrap();
    assert!(!prompt.content.contains(&first));
}

/// GHP-FR-MJTB: only a `github.com` issue address may be handed to the OS.
///
/// The publication store is committed content, so a record reaches this machine
/// from whoever else works on the project: being named by a record is not on
/// its own evidence that a URL is safe to open.
#[test]
fn only_a_github_issue_address_may_be_opened() {
    for safe in [
        "https://github.com/acme/widgets/issues/1",
        "https://GitHub.com/acme/widgets/issues/4181",
    ] {
        assert!(flow::is_openable_issue_url(safe), "{safe}");
    }
    for unsafe_url in [
        "file:///etc/passwd",
        "http://github.com/acme/widgets/issues/1",
        "https://github.com.evil.test/acme/widgets/issues/1",
        "https://user@github.com/acme/widgets/issues/1",
        "https://github.com/acme/widgets/pulls/1",
        "javascript:alert(1)",
        "https://github.com",
    ] {
        assert!(!flow::is_openable_issue_url(unsafe_url), "{unsafe_url}");
    }
}

/// DRS-FR-JOEV / GHP-FR-KZAP: a completion never clears an attempt that is not
/// its own, so two publications racing for one draft cannot erase each other.
#[test]
fn a_completion_refuses_to_clear_somebody_elses_attempt() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    let mine = flow::open_attempt(&root, &id, &origin(), "pub-mine".into()).unwrap();

    // The attempt on disk is replaced while this one is in flight.
    flow::clear_attempt(&root, &id).unwrap();
    flow::open_attempt(&root, &id, &origin(), "pub-theirs".into()).unwrap();

    let issue = IssueRef {
        number: 5,
        url: "https://github.com/acme/widgets/issues/5".into(),
        title: "widget".into(),
        body: "body".into(),
        ..Default::default()
    };
    assert_eq!(
        store::complete_attempt(&root, &id, flow::record_of(&mine, &issue)),
        Err(ERR_ATTEMPT_IN_PROGRESS.to_string())
    );
    let store = store::read_store(&root, &id).unwrap();
    assert!(store.publication.is_empty());
    assert_eq!(store.attempt.unwrap().marker, "pub-theirs");
}


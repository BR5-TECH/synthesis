//! Tests for the entries a draft history holds: what creates one, what does not, and how they list.

use super::*;

// ---------------------------------------------------------------------------
// DHS-FR-07, DHS-FR-08, DHS-FR-02, DHS-FR-03, DHS-FR-04 — the Original entry, where the draft is (DHS-FR-01, FR-02, FR-03,
// FR-04, FR-07)
// ---------------------------------------------------------------------------

#[test]
fn creating_a_draft_settles_no_version_at_all() {
    // DHS-FR-01, DHS-FR-02, DHS-FR-03, DHS-FR-04 (DHS-FR-07, DHS-FR-08, DHS-FR-10): a draft nobody has proposed a
    // change to holds no entry. Its live prompt IS its `Original`, and an empty
    // snapshot taken beside an empty prompt would be a version of nothing —
    // recorded before there was anything to record, and stale from the first
    // word the author typed.
    let f = Fixture::new();

    assert_eq!(
        f.history_files(),
        Vec::<String>::new(),
        "creating a draft wrote into its history",
    );
    let list = f.list();
    assert!(list.entries.is_empty());
    assert_eq!(list.live.path, FILE);
    assert_eq!(list.live.byte_len, 0, "a draft is created holding an empty prompt");
    // Nothing it could have moved on from, so the rail renders it plainly
    // (NAW-FR-37) rather than as modified since a version that is not there.
    assert!(list.live.matches_latest);
}

#[test]
fn the_first_acceptance_settles_the_prompt_it_supersedes_as_original() {
    // DHS-FR-08, DHS-FR-01, DHS-FR-02, DHS-FR-03 / DHS-FR-09 (DHS-FR-04, DHS-FR-05, DHS-FR-07): the `Original` is
    // the prompt as it stood before the first accepted change — written by that
    // acceptance, over the very bytes it is about to supersede.
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);

    let outcome = f.accept_new(PROPOSED);

    let list = f.list();
    assert_eq!(list.entries.len(), 2, "the Original and the version accepted");
    let original = &list.entries[0];
    assert_eq!(original.seq, 1);
    assert_eq!(original.source, DraftHistorySource::Original);
    assert_eq!(original.path, FILE);
    assert_eq!(
        load_impl(&f.root, &original.id).expect("loaded").content,
        ORIGINAL_TEXT,
        "the Original holds the prompt the acceptance superseded",
    );
    assert_eq!(
        outcome.original.as_ref().expect("reported").id,
        original.id,
        "the acceptance reports the Original it settled",
    );
    assert_eq!(list.entries[1].seq, 2);
    assert_eq!(list.entries[1].id, outcome.entry.expect("an entry").id);
    assert!(list.live.matches_latest);

    // DHS-FR-03 / DHS-FR-04: each id opaque, derived from neither the draft, nor
    // the path, nor the content, and neither reserved name.
    for entry in &list.entries {
        assert!(is_valid_entry_id(&entry.id));
        assert_ne!(entry.id, "journal");
        assert!(!entry.id.contains(&f.draft_id));
        assert!(!entry.id.contains("spec"));
    }
    assert_ne!(list.entries[0].id, list.entries[1].id);
    let names = f.history_files();
    assert_eq!(names.len(), 4, "two manifests and two payloads: {names:?}");
    assert!(!names.contains(&JOURNAL_FILE.to_string()));
    assert!(!names.contains(&JOURNAL_PRIOR.to_string()));

    // DHS-FR-06: and every acceptance after the first settles one entry, the
    // `Original` already standing and never written a second time.
    f.accept_new("a third version\n");
    let list = f.list();
    assert_eq!(list.entries.len(), 3);
    assert_eq!(
        list.entries
            .iter()
            .filter(|e| e.source == DraftHistorySource::Original)
            .count(),
        1,
        "exactly one Original, for the draft's whole life",
    );
    assert_eq!(list.entries[0].id, original.id);
}

#[test]
fn a_filed_drafts_history_lands_inside_that_draft_wherever_it_is_filed() {
    // DHS-FR-01 / DRS-FR-36: the folder is obtained from the drafts module
    // rather than composed from the draft's id, so filing the draft moves its
    // history with it and leaves nothing at the path an id alone would name.
    let f = Fixture::in_folder("UI/Components");
    f.accept_new("rewritten\n");

    let filed = f
        .root
        .join(".synthesis/drafts/UI/Components")
        .join(&f.draft_id)
        .join("history");
    assert_eq!(f.hist(), filed);
    assert_eq!(f.history_files().len(), 4);
    assert!(
        !f.root.join(".synthesis/drafts").join(&f.draft_id).exists(),
        "no directory named for the draft's id sits at the drafts root",
    );
}

// ---------------------------------------------------------------------------
// DHS-FR-08, DHS-FR-10, DHS-FR-22, DHS-FR-07, DHS-FR-09, DHS-FR-08 — what does and does not create an entry
// (DHS-FR-08, DHS-FR-10, DHS-FR-22)
// ---------------------------------------------------------------------------

#[test]
fn the_authors_own_typing_adds_no_version() {
    let f = Fixture::new();
    let seen = f.watch();

    for body in ["a", "ab", "abc"] {
        f.write_prompt(body);
    }
    let list = f.list();
    assert!(list.entries.is_empty(), "typing settles nothing");
    assert!(
        list.live.matches_latest,
        "and there is no version for it to have moved on from",
    );
    assert!(seen.lock().unwrap_or_else(|e| e.into_inner()).is_empty());

    // Once a change has been accepted there IS a version to have moved on from,
    // and the author's typing is what moves the live prompt off it — creating
    // nothing either way (DHS-FR-08, DHS-FR-10).
    f.accept_new("the accepted text\n");
    let settled = f.list().entries.len();
    assert_eq!(settled, 2);
    f.write_prompt("the accepted text\nand a sentence more\n");
    let list = f.list();
    assert_eq!(list.entries.len(), settled);
    assert!(!list.live.matches_latest, "the live prompt has moved on");

    // …and editing back to the newest version's bytes makes them match again.
    f.write_prompt("the accepted text\n");
    let list = f.list();
    assert_eq!(list.entries.len(), settled);
    assert!(list.live.matches_latest);
}

#[test]
fn renaming_archiving_and_moving_a_draft_add_no_version() {
    let f = Fixture::new();
    // Against a draft that HOLDS versions: a rail that gained nothing because
    // there was nothing to gain would assert nothing about any of these.
    f.accept_new("rewritten\n");
    let seen = f.watch();

    drafts::rename_draft_impl(&f.root, &f.draft_id, "overview").expect("rename");
    drafts::set_draft_status_impl(&f.root, &f.draft_id, drafts::DraftStatus::Archived)
        .expect("archive");
    drafts::set_draft_status_impl(&f.root, &f.draft_id, drafts::DraftStatus::Active)
        .expect("restore");
    drafts::create_drafts_folder_impl(&f.root, "", "UI").expect("folder");
    drafts::move_draft_to_folder_impl(&f.root, &f.draft_id, "UI").expect("move");

    assert_eq!(f.list().entries.len(), 2);
    assert!(seen.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
}

#[test]
fn a_proposal_recorded_edited_and_declined_adds_no_version() {
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let seen = f.watch();

    let proposal = f.propose(PROPOSED);
    let baseline = dcp::load_content_impl(&f.root, &proposal.id).expect("candidate");
    dcp::save_candidate_impl(&f.root, &proposal.id, "# Spec\n\nMine.\n", &baseline.checksum)
        .expect("candidate save");
    dcp::decline_for_test(&f.handle(), &f.root, &f.root, &proposal.id, &human()).expect("declined");

    assert!(f.list().entries.is_empty());
    assert!(seen.lock().unwrap_or_else(|e| e.into_inner()).is_empty());

    // And the acceptance that follows creates exactly one version of its own,
    // naming the proposal and the proposing agent, beside the `Original` it
    // superseded (DHS-FR-07).
    let outcome = f.accept_new(PROPOSED);
    let entry = outcome.entry.expect("an entry");
    assert_eq!(entry.seq, 2);
    assert_eq!(
        load_impl(&f.root, &outcome.original.expect("an Original").id)
            .expect("loaded")
            .content,
        ORIGINAL_TEXT,
    );
    match entry.source {
        DraftHistorySource::ProposalAccepted { proposal_id, agent, .. } => {
            assert_ne!(proposal_id, proposal.id, "the accepted one, not the declined");
            assert_eq!(agent, self::agent());
        }
        other => panic!("wrong source: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// DRS-FR-12 — the snapshot holds the bytes the save path persisted (DHS-FR-09)
// ---------------------------------------------------------------------------

#[test]
fn a_snapshot_holds_the_normalised_bytes_rather_than_the_candidates_own() {
    let f = Fixture::new();
    crate::project_settings::save_project_config_to(
        &f.root,
        crate::project_settings::ProjectConfig {
            line_endings: crate::project_settings::LineEndings::Crlf,
            draft_template: None,
                    ..Default::default()
        },
    )
    .expect("convention");
    // LF endings, trailing whitespace on two lines, and no final newline.
    let candidate = "# Spec  \n\nA line   \nand the last with no newline";

    let outcome = f.accept_new(candidate);
    let entry = outcome.entry.expect("an entry");

    let on_disk = std::fs::read(
        drafts::draft_dir(&f.root, &f.draft_id)
            .expect("dir")
            .join("files")
            .join(FILE),
    )
    .expect("prompt bytes");
    assert_eq!(
        on_disk,
        b"# Spec  \r\n\r\nA line   \r\nand the last with no newline",
        "CRLF throughout, with the trailing whitespace and the missing final \
         newline intact",
    );
    let payload = std::fs::read(snapshot_path(&f.hist(), &entry.id)).expect("payload");
    assert_eq!(payload, on_disk, "byte-for-byte the bytes that landed");
    assert_ne!(payload, candidate.as_bytes(), "not the candidate's own bytes");
    assert_eq!(entry.byte_len, payload.len() as u64);
}

// ---------------------------------------------------------------------------
// DHS-FR-10, DHS-FR-11 — listing and loading (DHS-FR-05, FR-10, FR-11)
// ---------------------------------------------------------------------------

#[test]
fn entries_list_oldest_first_and_survive_a_relaunch() {
    let f = Fixture::new();
    f.accept_new("first rewrite\n");
    f.accept_new("second rewrite\n");

    let list = f.list();
    let seqs: Vec<u32> = list.entries.iter().map(|e| e.seq).collect();
    assert_eq!(seqs, vec![1, 2, 3], "ascending seq, oldest first");
    let ids: std::collections::BTreeSet<&str> =
        list.entries.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids.len(), 3, "each entry carries an id no other carries");
    assert_eq!(list.live.path, FILE);
    assert_eq!(list.live.sha256, list.entries[2].sha256);
    assert!(list.live.matches_latest);

    // A relaunch is a fresh read of the same folder — nothing is held in memory
    // between calls, so a second list over the same root is the identical answer.
    let again = list_impl(&fs::RootFs::for_root(f.root.path()), &f.draft_id).expect("relist");
    assert_eq!(again, list);
}

#[test]
fn loading_an_entry_verifies_it_and_a_corrupt_payload_is_typed() {
    let f = Fixture::new();
    f.accept_new("first rewrite\n");
    f.accept_new("second rewrite\n");
    let list = f.list();

    for entry in &list.entries {
        let loaded = load_impl(&f.root, &entry.id).expect("loaded");
        assert_eq!(loaded.sha256, entry.sha256);
    }
    assert_eq!(load_impl(&f.root, &list.entries[1].id).expect("mid").content, "first rewrite\n");

    // Truncated on disk: a mismatch is `snapshot_corrupt` naming that entry, no
    // text is returned in its place, and the other two still load.
    let victim = &list.entries[1];
    std::fs::write(snapshot_path(&f.hist(), &victim.id), b"tampered").expect("truncate");
    assert_eq!(
        load_impl(&f.root, &victim.id).unwrap_err(),
        ERR_SNAPSHOT_CORRUPT,
    );
    assert_eq!(f.prompt(), "second rewrite\n", "the prompt is untouched");
    assert!(load_impl(&f.root, &list.entries[0].id).is_ok());
    assert!(load_impl(&f.root, &list.entries[2].id).is_ok());
}

#[test]
fn a_list_reads_no_snapshot_payload() {
    // DHS-FR-10: a draft carrying a long history costs a list one directory walk
    // however large its prompt has grown. Observed by making every payload
    // unreadable — a list that touched one would fail or lie.
    let f = Fixture::new();
    f.accept_new("first rewrite\n");
    let list = f.list();
    for entry in &list.entries {
        std::fs::remove_file(snapshot_path(&f.hist(), &entry.id)).expect("remove payload");
    }

    let again = f.list();
    assert_eq!(again.entries.len(), 2, "the manifests still list");
    assert_eq!(
        load_impl(&f.root, &again.entries[0].id).unwrap_err(),
        ERR_SNAPSHOT_CORRUPT,
        "and loading one is where the absence is reported",
    );
}

// ---------------------------------------------------------------------------
// DHS-FR-16 — an acceptance that changes nothing (DHS-FR-16, DHS-FR-22)
// ---------------------------------------------------------------------------

#[test]
fn an_acceptance_that_changes_no_byte_creates_no_entry() {
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    f.accept_new(PROPOSED);
    let before = f.list();
    let seen = f.watch();

    // A candidate that normalises to exactly what the prompt now holds. It has
    // to reach the transaction directly: `record_proposal` refuses a no-change
    // candidate outright (DCP-FR-07), which is the layer above this one.
    let proposal = f.propose("something else entirely\n");
    let outcome = apply_acceptance(
        &f.handle(),
        &f.root,
        &f.root,
        Acceptance {
            hunk_id: None,
            resolves: true,
            proposal: &proposal,
            candidate: PROPOSED,
            comment_body: "Accepted.".into(),
            comment_by: &human(),
        },
    )
    .expect("accepted");

    assert!(outcome.entry.is_none());
    assert_eq!(outcome.proposal.state, dcp::ProposalState::Accepted);
    assert_eq!(f.list().entries, before.entries, "history gained nothing");
    assert!(f.list().live.matches_latest);
    assert!(
        seen.lock().unwrap_or_else(|e| e.into_inner()).is_empty(),
        "no history event for an acceptance that created no version",
    );
    assert!(
        f.discussion()
            .comments
            .last()
            .expect("last")
            .body
            .contains("Accepted"),
        "the decision comment was still appended",
    );
}

#[test]
fn a_first_acceptance_that_changes_no_byte_settles_no_original_either() {
    // DHS-FR-16 with DHS-FR-07: an acceptance that supersedes nothing settles
    // nothing — not the version, and not an `Original` for a prompt that is
    // still exactly what it was. Without this the draft would gain a lone
    // `Original` duplicating a live prompt nothing had changed.
    let f = Fixture::new();
    f.write_prompt(ORIGINAL_TEXT);
    let proposal = f.propose("something else entirely\n");
    let seen = f.watch();

    let outcome = apply_acceptance(
        &f.handle(),
        &f.root,
        &f.root,
        Acceptance {
            hunk_id: None,
            resolves: true,
            proposal: &proposal,
            candidate: ORIGINAL_TEXT,
            comment_body: "Accepted.".into(),
            comment_by: &human(),
        },
    )
    .expect("accepted");

    assert!(outcome.entry.is_none());
    assert!(outcome.original.is_none());
    assert_eq!(
        f.history_files(),
        Vec::<String>::new(),
        "history/ gained a file for an acceptance that changed nothing",
    );
    assert!(f.list().entries.is_empty());
    assert!(f.list().live.matches_latest);
    assert!(seen.lock().unwrap_or_else(|e| e.into_inner()).is_empty());
    assert_eq!(outcome.proposal.state, dcp::ProposalState::Accepted);
    assert!(
        f.discussion()
            .comments
            .last()
            .expect("last")
            .body
            .contains("Accepted"),
        "the decision comment was still appended",
    );
}


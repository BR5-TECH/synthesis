//! Tests for a legacy whole-candidate proposal (PCP-FR-LGXQ).

use super::*;

/// The record an older build wrote: no ledger, no counts, and a `.content`
/// file holding the whole candidate. Each call mints its own id, because the
/// acceptance slot is shared by every test in the process.
fn stage_legacy(f: &Fixture, candidate: &str) -> String {
    let id = new_note_id();
    let dir = f.dir();
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::write(dir.join(format!("{id}.content")), candidate).expect("content");
    let record = format!(
        "id = \"{id}\"\nartifactId = \"{PROMPT}\"\npath = \"{PROMPT}\"\n\
         rationale = \"Older.\"\nthreadId = \"{}\"\ncommentId = \"c-old\"\n\
         state = \"pending\"\noriginChecksum = \"{}\"\ncandidateEdited = false\n\
         commentOwed = false\ncreatedAt = \"2026-01-01T00:00:00Z\"\n\n\
         [agent]\nkind = \"agent\"\nagentId = \"agent-1\"\nhandle = \"arch\"\n",
        f.thread_id,
        fs::sha256_bytes(candidate.as_bytes()),
    );
    std::fs::write(dir.join(format!("{id}.toml")), record).expect("record");
    id
}

// PCP-FR-LGXQ, PCP-FR-HKLD: a legacy record is served with a one-row ledger
// built from its state, and is marked legacy.
#[test]
fn a_legacy_record_is_served_with_one_ledger_row() {
    let f = Fixture::new();
    let id = stage_legacy(&f, PROPOSED);
    let listed = f.list();
    assert_eq!(listed.len(), 1, "the legacy proposal is not lost");
    let record = &listed[0];
    assert!(record.legacy);
    assert_eq!(record.state, PromptProposalState::Pending);
    assert_eq!(record.hunk_count, 1);
    assert_eq!(record.ledger.len(), 1);
    assert_eq!(record.ledger[0].id, LEGACY_HUNK_ID);
    assert_eq!(record.ledger[0].state, HunkState::Pending);
    assert_eq!(record.counts.pending, 1);
    assert!(pending_for(&f.root, PROMPT).is_some(), "it holds the artifact's one slot");
}

// PCP-FR-LGXQ, PCP-FR-LDHK: its hunks are one `replace` over the whole file as
// it stands, with the candidate as the proposed text, and it takes no edit.
#[test]
fn a_legacy_proposal_reads_as_one_hunk_over_the_whole_file_and_takes_no_edit() {
    let f = Fixture::new();
    let id = stage_legacy(&f, PROPOSED);
    let loaded = load_hunks_impl(&f.root, &id).expect("hunks");
    assert!(loaded.legacy);
    assert_eq!(loaded.hunks.len(), 1);
    assert_eq!(loaded.hunks[0].id, LEGACY_HUNK_ID);
    assert_eq!(loaded.hunks[0].before.as_deref(), Some(ORIGINAL));
    assert_eq!(loaded.hunks[0].after.as_deref(), Some(PROPOSED));
    assert_eq!(loaded.resolutions[0], HunkPlacement::Resolved { start: 0, end: ORIGINAL.len() });

    assert_eq!(
        edit_hunk_impl(&f.root, &id, LEGACY_HUNK_ID, "mine", &loaded.checksum).unwrap_err(),
        ERR_WRITE_FAILED,
    );
    assert_eq!(
        std::fs::read_to_string(f.dir().join(format!("{id}.content"))).unwrap(),
        PROPOSED,
        "the candidate is never rewritten",
    );
    assert!(!f.dir().join(format!("{id}.hunks")).exists());
}

// PCP-FR-LGXQ, PCP-FR-ZQAH: accepting it replaces the whole file with the
// candidate, whatever the file holds now, and the candidate stays as it was.
#[test]
fn a_legacy_acceptance_replaces_the_whole_file() {
    let f = Fixture::new();
    let id = stage_legacy(&f, PROPOSED);
    write_file(&f.root, PROMPT, "# Review\n\nSomething else entirely.\n");
    let outcome = f.accept_hunk(&id, LEGACY_HUNK_ID).expect("accepted");
    assert!(outcome.resolved);
    assert_eq!(outcome.proposal.state, PromptProposalState::Accepted);
    assert_eq!(f.artifact_body(), PROPOSED);
    assert_eq!(
        std::fs::read_to_string(f.dir().join(format!("{id}.content"))).unwrap(),
        PROPOSED,
    );
    let read = read_proposal(&f.root, &id).expect("record");
    assert!(read.legacy);
    assert_eq!(read.ledger[0].state, HunkState::Accepted);
}

// PCP-FR-LGXQ, PCP-FR-RJHN: it is rejected on the ordinary terms, writing
// nothing into the file.
#[test]
fn a_legacy_proposal_is_rejected_on_the_ordinary_terms() {
    let f = Fixture::new();
    let id = stage_legacy(&f, PROPOSED);
    let outcome = f.reject_hunk(&id, LEGACY_HUNK_ID).expect("rejected");
    assert!(outcome.resolved);
    assert_eq!(outcome.proposal.state, PromptProposalState::Rejected);
    assert_eq!(f.artifact_body(), ORIGINAL);
    assert!(f.dir().join(format!("{id}.content")).is_file());
}

// PCP-FR-LGXQ, PCP-FR-14: a journal an older build left, naming no hunk, rolls
// the legacy hunk forward.
#[test]
fn an_older_journal_settles_the_legacy_hunk() {
    let f = Fixture::new();
    let id = stage_legacy(&f, PROPOSED);
    let dir = f.dir();
    let prior = std::fs::read(f.root.path().join(PROMPT)).expect("prior");
    std::fs::write(dir.join(format!("{id}.prior")), &prior).expect("prior");
    let journal = format!(
        "operationId = \"op\"\nproposalId = \"{id}\"\nartifactId = \"{PROMPT}\"\n\
         path = \"{PROMPT}\"\npriorSha256 = \"{}\"\ncandidateSha256 = \"{}\"\n\
         commentId = \"c-dec\"\neventId = \"e-dec\"\nthreadId = \"{}\"\n\
         commentBody = \"Accepted the proposed change.\"\ncreatedAt = \"2026-01-01T00:00:00Z\"\n\n\
         [commentBy]\nkind = \"human\"\nlogin = \"raver119\"\n",
        fs::sha256_bytes(&prior),
        fs::sha256_bytes(PROPOSED.as_bytes()),
        f.thread_id,
    );
    std::fs::write(dir.join(format!("{id}.journal")), journal).expect("journal");
    write_file(&f.root, PROMPT, PROPOSED);

    let listed = f.list();
    assert_eq!(listed[0].state, PromptProposalState::Accepted);
    assert_eq!(listed[0].ledger[0].state, HunkState::Accepted);
    assert!(!listed[0].comment_owed);
    let decision = f.discussion().comments.last().cloned().expect("decision");
    assert_eq!(decision.id, "c-dec");
    assert!(!dir.join(format!("{id}.journal")).exists());
}

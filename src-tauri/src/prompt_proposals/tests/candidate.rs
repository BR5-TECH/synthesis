//! Tests for the candidate of a prompt change proposal (PCP-FR-22 … PCP-FR-24).

use super::*;


// ---------------------------------------------------------------------------
// The candidate (PCP-FR-22 … PCP-FR-24)
// ---------------------------------------------------------------------------

#[test]
fn a_candidate_save_touches_the_candidate_and_nothing_else() {
    // PCP-FR-22, PCP-FR-23, PCP-FR-11.
    let f = Fixture::new();
    let proposal = f.pending();
    let events = f.watch();

    let loaded = load_content_impl(&f.root, &proposal.id).expect("loaded");
    let mine = "# Review\n\nMy own version.\n";
    let saved = save_candidate_impl(&f.root, &proposal.id, mine, &loaded.checksum).expect("saved");

    assert_eq!(
        std::fs::read_to_string(f.dir().join(format!("{}.content", proposal.id))).unwrap(),
        mine,
    );
    assert_eq!(saved.checksum, fs::sha256_bytes(mine.as_bytes()));
    let record = read_proposal(&f.root, &proposal.id).expect("record");
    assert!(record.candidate_edited);
    assert_eq!(record.state, PromptProposalState::Pending);
    assert_eq!(f.artifact_body(), ORIGINAL);
    assert!(events.lock().unwrap().is_empty(), "and nothing was announced");

    // …and putting the agent's own bytes back clears the flag.
    save_candidate_impl(&f.root, &proposal.id, PROPOSED, &saved.checksum).expect("saved");
    assert!(!read_proposal(&f.root, &proposal.id).unwrap().candidate_edited);
}

#[test]
fn an_edited_candidate_is_what_an_acceptance_lands() {
    // PCP-FR-13, PCP-FR-15, PCP-FR-22.
    let f = Fixture::new();
    let proposal = f.pending();
    let loaded = load_content_impl(&f.root, &proposal.id).expect("loaded");
    let mine = "# Review\n\nMy own version.\n";
    save_candidate_impl(&f.root, &proposal.id, mine, &loaded.checksum).expect("saved");

    let outcome = f.accept(&proposal.id).expect("accepted");
    assert_eq!(f.artifact_body(), mine);
    assert!(outcome.proposal.candidate_edited);
}

#[test]
fn a_candidate_save_is_refused_once_the_proposal_is_decided() {
    // PCP-FR-23.
    let f = Fixture::new();
    let accepted = f.pending();
    let loaded = load_content_impl(&f.root, &accepted.id).expect("loaded");
    f.accept(&accepted.id).expect("accepted");
    let events = f.watch();
    assert_eq!(
        save_candidate_impl(&f.root, &accepted.id, "nope\n", &loaded.checksum).unwrap_err(),
        ERR_ALREADY_DECIDED,
    );
    assert_eq!(
        load_content_impl(&f.root, &accepted.id).unwrap().content,
        PROPOSED,
    );
    assert!(events.lock().unwrap().is_empty());
}

#[test]
fn a_stale_candidate_save_writes_nothing() {
    // PCP-FR-24.
    let f = Fixture::new();
    let proposal = f.pending();
    let first = load_content_impl(&f.root, &proposal.id).expect("loaded");
    let second = first.clone();

    save_candidate_impl(&f.root, &proposal.id, "first\n", &first.checksum).expect("the first lands");
    assert_eq!(
        save_candidate_impl(&f.root, &proposal.id, "second\n", &second.checksum).unwrap_err(),
        ERR_CANDIDATE_STALE,
    );
    assert_eq!(
        load_content_impl(&f.root, &proposal.id).unwrap().content,
        "first\n",
    );

    // …and re-reading and saving with the fresh checksum lands.
    let fresh = load_content_impl(&f.root, &proposal.id).expect("loaded");
    save_candidate_impl(&f.root, &proposal.id, "second\n", &fresh.checksum).expect("lands");
    assert_eq!(
        load_content_impl(&f.root, &proposal.id).unwrap().content,
        "second\n",
    );
}

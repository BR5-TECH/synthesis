//! Tests for the author's authority over a decision by tool
//! (`AGC-agent-conversations.md` AGC-FR-TFEN, AGC-FR-DTCL, AGC-FR-QZVW;
//! `DDC-decide-draft-changes-tool.md` DDC-FR-PZBH, DDC-FR-ELAE, DDC-FR-ZYOL,
//! DDC-FR-NKAB, DDC-FR-VONX, DDC-FR-CMRE, DDC-FR-VYLJ).

use std::sync::{Arc, Barrier};

use tauri::Manager;
use tempfile::TempDir;

use super::authority_for_test::{end_speech, is_spent, register_turn, steer_chunk, steer_comment};
use super::*;
use crate::comments::{DiscussionTarget, ThreadRef};

fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

fn agent() -> Participant {
    Participant::Agent {
        agent_id: "agent-1".into(),
        handle: "arch".into(),
        model: Some("m".into()),
        title: None,
    }
}

/// A draft with one discussion, in an app that manages a turn registry.
struct Fixture {
    app: tauri::App<tauri::test::MockRuntime>,
    _dir: TempDir,
    root: crate::fs::RootFs,
    draft_id: String,
    thread_id: String,
    /// The opening comment: "@arch accept the second change, please".
    opening: String,
}

impl Fixture {
    fn new() -> Fixture {
        let dir = TempDir::new().unwrap();
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&path);
        let draft_id = crate::drafts::create_draft_at_root(&root, Some("d")).unwrap().draft.id;
        let thread = comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Draft { draft_id: draft_id.clone() },
            None,
            "@arch Accept the second change, please!".into(),
            Vec::new(),
            comments::Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .unwrap();
        let app = tauri::test::mock_app();
        app.manage(TurnRegistry::default());
        let opening = thread.comments[0].id.clone();
        Fixture { app, _dir: dir, root, draft_id, thread_id: thread.id, opening }
    }

    fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle().clone()
    }

    fn origin(&self) -> ConversationOrigin {
        ConversationOrigin::stub_draft(self.thread_id.clone(), self.draft_id.clone(), false)
    }

    /// Append one comment by `by`, and answer its id.
    fn append(&self, by: &Participant, body: &str) -> String {
        let id = crate::notes::new_note_id();
        comments::add_comment_with_id(
            &self.root,
            ThreadRef::discussion(&self.draft_id),
            &self.thread_id,
            id.clone(),
            body.to_string(),
            by,
            "2026-01-01T00:00:01Z",
        )
        .unwrap();
        id
    }

    fn authorize(
        &self,
        turn_id: &str,
        instruction: &str,
    ) -> Result<AuthorityGrant<tauri::test::MockRuntime>, AuthorityRefusal> {
        authorize_decision(
            &self.handle(),
            Roots::same(&self.root),
            &self.origin(),
            turn_id,
            instruction,
        )
    }

    /// A turn triggered by the opening comment, noted as an author comment.
    fn authored_turn(&self) -> String {
        note_author_comment(&self.handle(), &self.opening);
        register_turn(&self.handle(), self.origin(), Some(&self.opening), None)
    }
}

// DDC-FR-NKAB
#[test]
fn an_instruction_matches_as_whole_words_ignoring_case_spaces_and_punctuation() {
    let body = "@arch Accept   the second change, please!";
    assert!(instruction_matches("accept the second change", body));
    assert!(instruction_matches("ACCEPT the SECOND change.", body));
    assert!(instruction_matches("“Accept the second change”", body));
    assert!(instruction_matches("change please", body));
    // Whole words: a part of a word is no match.
    assert!(!instruction_matches("cept the", body));
    assert!(!instruction_matches("accept", "This is unacceptable."));
    // The words keep their order.
    assert!(!instruction_matches("second the", body));
    // Words that are not in the comment.
    assert!(!instruction_matches("reject the second change", body));
    // An instruction with no letter and no digit matches nothing.
    assert!(!instruction_matches("!!!", body));
}

// DDC-FR-NKAB
#[test]
fn normalising_lower_cases_and_joins_runs_of_other_characters_with_one_space() {
    assert_eq!(normalise_instruction_text("  Hey, ARCH — accept it!  "), "hey arch accept it");
    assert_eq!(normalise_instruction_text("Ünïcode Wörds"), "ünïcode wörds");
    assert_eq!(normalise_instruction_text("--"), "");
}

// DDC-FR-PZBH
#[test]
fn an_instruction_is_well_formed_only_with_a_letter_or_digit_and_up_to_500_characters() {
    assert!(instruction_is_well_formed("accept it"));
    assert!(instruction_is_well_formed("2"));
    assert!(!instruction_is_well_formed(""));
    assert!(!instruction_is_well_formed("   "));
    assert!(!instruction_is_well_formed("?!"));
    assert!(instruction_is_well_formed(&"a".repeat(MAX_INSTRUCTION_CHARS)));
    assert!(!instruction_is_well_formed(&"a".repeat(MAX_INSTRUCTION_CHARS + 1)));
    // Characters are counted, not bytes.
    assert!(instruction_is_well_formed(&"ü".repeat(MAX_INSTRUCTION_CHARS)));
}

// AGC-FR-DTCL
#[test]
fn the_newest_received_comment_is_the_last_steering_comment_and_a_chunk_is_none() {
    let f = Fixture::new();
    let turns = f.app.state::<TurnRegistry>();
    let turn = register_turn(&f.handle(), f.origin(), Some("c-1"), None);
    assert_eq!(turns.received_by(&turn), Some((Some("c-1".to_string()), false)));
    steer_comment(&f.handle(), &turn, "c-2");
    steer_chunk(&f.handle(), &turn);
    assert_eq!(turns.received_by(&turn), Some((Some("c-2".to_string()), false)));
    assert_eq!(turns.received_by("turn-unknown"), None);
}

// AGC-FR-DTCL / DDC-FR-ZYOL
#[test]
fn a_speech_turn_has_no_received_comment_until_its_speech_ends_with_one() {
    let f = Fixture::new();
    let turns = f.app.state::<TurnRegistry>();
    let turn = register_turn(&f.handle(), f.origin(), None, Some("speech-1"));
    assert_eq!(turns.received_by(&turn), Some((None, true)));
    assert_eq!(f.authorize(&turn, "accept").err(), Some(AuthorityRefusal::SpeechNotPosted));

    let spoken = f.append(&human(), "Accept that draft.");
    note_author_comment(&f.handle(), &spoken);
    end_speech(&f.handle(), &turn, &spoken);
    assert_eq!(turns.received_by(&turn), Some((Some(spoken.clone()), false)));
    let grant = f.authorize(&turn, "accept that draft").expect("authorized");
    assert_eq!(grant.comment_id(), spoken);
}

// DDC-FR-ZYOL / DDC-FR-VYLJ: a typed turn that takes a speech while it runs
// waits for the speech, even though its trigger is an author comment.
#[test]
fn a_turn_bound_to_an_open_speech_is_refused_before_the_authority_check() {
    let f = Fixture::new();
    note_author_comment(&f.handle(), &f.opening);
    let turn = register_turn(&f.handle(), f.origin(), Some(&f.opening), Some("speech-2"));
    assert_eq!(f.authorize(&turn, "zzz").err(), Some(AuthorityRefusal::SpeechNotPosted));
}

// AGC-FR-TFEN / DDC-FR-ELAE
#[test]
fn only_a_noted_human_comment_authorizes_a_decision() {
    let f = Fixture::new();
    // The opening comment is not noted: the app wrote it, as it writes a
    // decision comment in the author's name.
    let turn = register_turn(&f.handle(), f.origin(), Some(&f.opening), None);
    assert_eq!(
        f.authorize(&turn, "accept the second change").err(),
        Some(AuthorityRefusal::NotAuthorized)
    );
    // Noted, it authorizes.
    note_author_comment(&f.handle(), &f.opening);
    assert!(f.authorize(&turn, "accept the second change").is_ok());
}

// DDC-FR-ELAE: the newest received comment decides, and an agent comment is
// never an author comment, also when its id is noted.
#[test]
fn an_agent_comment_received_last_takes_the_authority_away() {
    let f = Fixture::new();
    let turn = f.authored_turn();
    let by_agent = f.append(&agent(), "Accept the second change, please!");
    note_author_comment(&f.handle(), &by_agent);
    steer_comment(&f.handle(), &turn, &by_agent);
    assert_eq!(
        f.authorize(&turn, "accept the second change").err(),
        Some(AuthorityRefusal::NotAuthorized)
    );
}

// DDC-FR-ELAE: the newest received comment decides, so a later decision
// comment that the app wrote in the author's name takes the authority away.
#[test]
fn a_decision_comment_received_last_takes_the_authority_away() {
    let f = Fixture::new();
    let turn = f.authored_turn();
    let record = f.append(&human(), "Accepted the proposed draft “A”, part 1 of 2.");
    steer_comment(&f.handle(), &turn, &record);
    assert_eq!(f.authorize(&turn, "accepted").err(), Some(AuthorityRefusal::NotAuthorized));
}

// DDC-FR-ELAE: no registry, an unknown turn, and a turn with no comment.
#[test]
fn a_turn_with_no_received_comment_is_not_authorized() {
    let f = Fixture::new();
    assert_eq!(f.authorize("turn-unknown", "accept").err(), Some(AuthorityRefusal::NotAuthorized));
    let turn = register_turn(&f.handle(), f.origin(), None, None);
    assert_eq!(f.authorize(&turn, "accept").err(), Some(AuthorityRefusal::NotAuthorized));

    let bare = tauri::test::mock_app();
    let refused = authorize_decision(
        &bare.handle().clone(),
        Roots::same(&f.root),
        &f.origin(),
        &turn,
        "accept",
    );
    assert_eq!(refused.err(), Some(AuthorityRefusal::NotAuthorized));
}

// DDC-FR-NKAB / DDC-FR-VYLJ
#[test]
fn an_instruction_not_in_the_authorizing_comment_is_refused_and_spends_nothing() {
    let f = Fixture::new();
    let turn = f.authored_turn();
    assert_eq!(
        f.authorize(&turn, "reject the second change").err(),
        Some(AuthorityRefusal::InstructionNotFound)
    );
    assert!(!is_spent(&f.handle(), &f.opening));
    assert!(f.authorize(&turn, "accept the second change").is_ok());
}

// AGC-FR-QZVW / DDC-FR-VONX
#[test]
fn a_committed_grant_spends_the_comment_for_every_turn() {
    let f = Fixture::new();
    let first = f.authored_turn();
    f.authorize(&first, "accept the second change").expect("authorized").commit();
    assert!(is_spent(&f.handle(), &f.opening));
    // The same turn, and a second turn on the same comment, are refused.
    assert_eq!(
        f.authorize(&first, "accept the second change").err(),
        Some(AuthorityRefusal::InstructionUsed)
    );
    let second = register_turn(&f.handle(), f.origin(), Some(&f.opening), None);
    assert_eq!(
        f.authorize(&second, "accept the second change").err(),
        Some(AuthorityRefusal::InstructionUsed)
    );
}

// AGC-FR-QZVW / DDC-FR-VONX
#[test]
fn a_grant_dropped_without_a_commit_gives_the_comment_back() {
    let f = Fixture::new();
    let turn = f.authored_turn();
    let grant = f.authorize(&turn, "accept the second change").expect("authorized");
    assert!(is_spent(&f.handle(), &f.opening), "reserved while the call runs");
    drop(grant);
    assert!(!is_spent(&f.handle(), &f.opening));
    assert!(f.authorize(&turn, "accept the second change").is_ok());
}

// DDC-FR-CMRE / AGC-FR-QZVW: whatever the order the threads run in, exactly
// one of two concurrent calls on one comment is granted. Each thread hands its
// grant back, so no grant drops before both ran, and a panic in one thread
// fails the join rather than blocking the other.
#[test]
fn two_concurrent_calls_on_one_comment_never_both_hold_it() {
    let f = Fixture::new();
    let first = f.authored_turn();
    let second = register_turn(&f.handle(), f.origin(), Some(&f.opening), None);
    let barrier = Arc::new(Barrier::new(2));
    let threads: Vec<_> = [first, second]
        .into_iter()
        .map(|turn| {
            let handle = f.handle();
            let root = f.root.clone();
            let origin = f.origin();
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                authorize_decision(
                    &handle,
                    Roots::same(&root),
                    &origin,
                    &turn,
                    "accept the second change",
                )
            })
        })
        .collect();
    let results: Vec<_> = threads.into_iter().map(|thread| thread.join().unwrap()).collect();
    assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
    assert!(results.iter().any(|r| r.as_ref().err() == Some(&AuthorityRefusal::InstructionUsed)));
}

// DDC-FR-ELAE / DDC-FR-NKAB: only the body of the authorizing comment counts.
// A quote of another comment is not the author's instruction.
#[test]
fn the_words_of_a_quoted_comment_do_not_authorize() {
    let f = Fixture::new();
    let quoted = f.append(&agent(), "Shall I accept the second change?");
    let (thread, asking) = comments::add_comment_reporting_id(
        &f.root,
        ThreadRef::discussion(&f.draft_id),
        &f.thread_id,
        "What about this?".into(),
        vec![comments::CommentQuote {
            comment_id: quoted,
            excerpt: "accept the second change".into(),
        }],
        Vec::new(),
        None,
        comments::Stamp { by: &human(), at: "2026-01-01T00:00:03Z" },
    )
    .unwrap();
    assert!(thread.comments.iter().any(|c| c.id == asking));
    note_author_comment(&f.handle(), &asking);
    let turn = register_turn(&f.handle(), f.origin(), Some(&asking), None);
    assert_eq!(
        f.authorize(&turn, "accept the second change").err(),
        Some(AuthorityRefusal::InstructionNotFound)
    );
}

// DDC-FR-VONX / DDC-FR-CMRE: while one call holds the comment, a second call
// is refused. When the first call decided nothing, the comment comes back.
#[test]
fn a_held_comment_refuses_a_second_call_until_it_is_given_back() {
    let f = Fixture::new();
    let turn = f.authored_turn();
    let held = f.authorize(&turn, "accept the second change").expect("authorized");
    assert_eq!(
        f.authorize(&turn, "accept the second change").err(),
        Some(AuthorityRefusal::InstructionUsed)
    );
    drop(held);
    assert!(f.authorize(&turn, "accept the second change").is_ok());
}

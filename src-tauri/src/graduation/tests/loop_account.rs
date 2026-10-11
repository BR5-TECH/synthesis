//! The account a turn carries to the next turn of the loop
//! (`../ai/GRL-graduation-loop.md`).

use super::*;

/// GRL-FR-DNKA: a work turn that reported **success** carries its account to
/// the review on exactly the terms a failed one does.
///
/// This is the case the requirement exists for. A turn that wrote real work and
/// knows it left some of what was asked undone reports success — the only
/// outcome that does not misdescribe what it wrote — and says the rest in its
/// summary. Carried only from a failed turn, that account is withheld from the
/// one turn whose delivery decision it settles.
#[test]
fn a_successful_work_turn_carries_its_own_account_to_the_review() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state, and wire it to the panel.");

    let said = "I added the empty state. I did not wire it to the panel.";
    let dispatch = ScriptedDispatch::new(vec![
        Turn::work_saying(said).writing("src/panel.ts", "half\n"),
        Turn::ready(),
    ]);
    let run = fx.drive(&run, dispatch.clone());

    let review = dispatch.of_part("review");
    assert_eq!(review.len(), 1);
    assert_eq!(
        review[0].instruction_field("agent_account").as_deref(),
        Some(said),
        "GRL-FR-DNKA: the review is given what the work turn said about itself",
    );
    assert_eq!(run.state, GraduationRunState::Completed);
}

/// GRL-FR-DNKA: the account reaches the review that judges the work, and no
/// other turn.
///
/// A work turn is never given it. `work.md` names no such field, so an account
/// reaching one would be an input nothing explains — and a turn reading its
/// predecessor's account as if it described the tree in front of it is the
/// confusion this field exists to prevent.
#[test]
fn the_account_reaches_the_review_and_no_work_turn() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work_saying("I wrote half of it.").writing("src/panel.ts", "half\n"),
        Turn::revise(ReviewSeverity::Critical, "Only half is written."),
        Turn::work_saying("Now I wrote the rest.").writing("src/panel.ts", "whole\n"),
        Turn::ready(),
    ]);
    fx.drive(&run, dispatch.clone());

    // Both reviews read the account of the work turn immediately before them.
    let review = dispatch.of_part("review");
    assert_eq!(
        review[0].instruction_field("agent_account").as_deref(),
        Some("I wrote half of it."),
    );
    assert_eq!(
        review[1].instruction_field("agent_account").as_deref(),
        Some("Now I wrote the rest."),
    );

    // And no work turn is given one — the second included, although an account
    // stood in the checkpoint when its input was composed.
    let work = dispatch.of_part("work");
    assert_eq!(work.len(), 2);
    for turn in &work {
        assert!(
            turn.instruction_field("agent_account").is_none(),
            "a work turn was given an account",
        );
    }
}

/// GRL-FR-DNKA: a turn that said nothing about itself carries no account.
///
/// An empty account is none at all, so a review's input gains no blank field
/// from an agent that reported nothing.
#[test]
fn a_work_turn_that_said_nothing_carries_no_account() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work_saying("   ").writing("src/panel.ts", "whole\n"),
        Turn::ready(),
    ]);
    fx.drive(&run, dispatch.clone());

    let review = dispatch.of_part("review");
    assert_eq!(review.len(), 1);
    assert!(review[0].instruction_field("agent_account").is_none());
}

/// GRL-FR-DNKA: a turn that says nothing **clears** the account the turn before
/// it left, rather than letting it stand.
///
/// The assignment is unconditional for exactly this reason. Written as
/// `if let Some(account) = .. { checkpoint.agent_account = Some(account) }` — a
/// plausible tidying — pass 1's words would reach pass 2's review as though they
/// described the tree in front of it, which is the confusion this field exists
/// to prevent.
#[test]
fn a_turn_that_says_nothing_clears_the_account_before_it() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work_saying("I left the panel unwired.").writing("src/panel.ts", "half\n"),
        Turn::revise(ReviewSeverity::Critical, "Only half is written."),
        // The second turn finishes the work and says nothing about it.
        Turn::work_saying("   ").writing("src/panel.ts", "whole\n"),
        Turn::ready(),
    ]);
    fx.drive(&run, dispatch.clone());

    let review = dispatch.of_part("review");
    assert_eq!(
        review[0].instruction_field("agent_account").as_deref(),
        Some("I left the panel unwired."),
    );
    assert!(
        review[1].instruction_field("agent_account").is_none(),
        "pass 2's review inherited pass 1's account: {:?}",
        review[1].instruction_field("agent_account"),
    );
}

/// GRL-FR-DNKA: a failure whose message says nothing falls back to the turn's
/// summary, which is then the only account there is.
///
/// The fallback turns on the message being **empty** rather than on the failure
/// being absent: an envelope carrying `failure` with a blank message and a
/// useful summary would otherwise reach the review with no account at all.
#[test]
fn a_failure_with_no_message_carries_the_turns_summary() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::answering(Answer::ReportedFailureWithoutMessage(
            "I could not finish; the panel is half written.".to_string(),
        ))
        .writing("src/panel.ts", "half\n"),
        Turn::ready(),
    ]);
    fx.drive(&run, dispatch.clone());

    let review = dispatch.of_part("review");
    assert_eq!(
        review[0].instruction_field("agent_account").as_deref(),
        Some("I could not finish; the panel is half written."),
    );
}

/// GRL-FR-NVBZ: the account is a **claim** the review checks, never evidence
/// about the filesystem.
///
/// The loop reads the envelope for the outcome and the account, and reads what
/// changed from the working copy. What this adds over the two tests that prove
/// each half on its own is the **combination**: one turn whose account is
/// carried whole *and* whose change set is unmoved by what that account claims.
/// A loop that read the envelope for the change set would pass both halves
/// separately and fail here.
#[test]
fn an_account_claiming_more_than_the_turn_wrote_changes_no_change_set() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let run = fx.enqueue(&stream, "Add the empty state to the panel.");

    let dispatch = ScriptedDispatch::new(vec![
        Turn::work_saying("I wrote src/panel.ts, src/rail.ts and src/tree.ts.")
            .writing("src/panel.ts", "whole\n"),
        Turn::ready(),
    ]);
    fx.drive(&run, dispatch.clone());

    let review = dispatch.of_part("review");
    // The account is carried whole...
    assert_eq!(
        review[0].instruction_field("agent_account").as_deref(),
        Some("I wrote src/panel.ts, src/rail.ts and src/tree.ts."),
    );
    // ...and the change set is what the working copy holds, which is one path.
    let changed = review[0].path_list("changed_paths");
    assert_eq!(changed, vec!["src/panel.ts".to_string()]);
}

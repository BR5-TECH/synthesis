//! A Map turn that posts a plan card and goes on: later calls update the card
//! with no comment, and the answer is a closing comment or is discarded
//! (`CVL-conversation-loop.md` CVL-FR-KYBE, CVL-FR-NUOY, CVL-FR-WJKS,
//! CVL-FR-YNJS; `PPT-propose-plan-changes-tool.md` PPT-FR-PRPX, PPT-FR-ZMBA,
//! PPT-FR-IRFE; `AGC-agent-conversations.md` AGC-FR-ZPOI).
//!
//! One part of `../tests/mod.rs`, which holds the harness.

use super::*;
use crate::comments::{Attachment, Opening, ProposalUpdateCounts, Stamp};
use crate::delivery::github::tests::fake::{
    issue, link, milestone, FakeConnector, FakeTracker, OpenRepositories, PROJECT, SECRET,
};

const PLAN_TOOL: &str = crate::tools::propose_plan_changes::NAME;

/// [`plan_project`] with a Map thread that addresses `arch`.
fn plan_harness(replies: Vec<Result<ScriptedReply, &'static str>>) -> (Harness, Discussion) {
    let h = plan_project(replies);
    let thread = crate::comments::open_map_discussion_in(
        &h.root(),
        None,
        Opening {
            body: "@arch plan the payments work".into(),
            attachments: Vec::new(),
            spoken_original: None,
        },
        Stamp { by: &human("ada"), at: "2026-03-01T10:00:00Z" },
    )
    .expect("a Map thread")
    .discussion;
    (h, thread)
}

/// A harness whose open project has a GitHub Project plan with a `valid`
/// configuration, so every turn is attached the plan tools (CVL-FR-RWQB), and
/// the agent `arch`.
pub(super) fn plan_project(replies: Vec<Result<ScriptedReply, &'static str>>) -> Harness {
    let h = Harness::scripted(replies);
    let root = h.root().path().to_path_buf();
    let repo = git2::Repository::init(&root).expect("a repository");
    repo.remote("origin", "https://github.com/acme/widgets.git").expect("a remote");
    let fake = Arc::new(FakeTracker::new());
    fake.lock().milestones.push(milestone(1, Some("2026-12-01"), "open"));
    fake.board_issue(issue(1, Some("Epic")), Some("Todo"));
    let mut feature = issue(2, Some("Feature"));
    feature.parent = Some(link(1, "open"));
    fake.board_issue(feature, Some("Todo"));
    h.app.manage(crate::graduation::GraduationState::rooted_at(root.join(".graduation")));
    h.app.manage(crate::github_publication::GithubIssuesSeam(Arc::new(OpenRepositories)));
    h.app.manage(crate::github_polling::GithubPollingState::with_buffer(&TEST_BUFFER));
    h.app.manage(crate::github_polling::TestSecret(SECRET.into()));
    h.app.manage(crate::delivery::github::adapter::TrackerSeam(Arc::new(FakeConnector(fake))));
    h.app.manage(crate::delivery::github::GithubDeliveryState::with_buffer(&TEST_BUFFER));
    h.app.manage(crate::artifacts::ContentTracker::default());
    h.mount();
    let access = h.app.state::<crate::fs::FsAccessState>().get().expect("an instance");
    let project = h.app.state::<ProjectState>();
    project.set_root_with_access(root.clone(), Some(access));
    project.set_anchor(PROJECT_KEY.into());
    project.set_store(root.clone());
    let settings = crate::project_settings::GithubPollingSettings {
        project_node_id: Some(PROJECT.into()),
        interval_minutes: None,
        ready_status: None,
        in_progress_status: None,
        blocked_status: None,
    };
    crate::project_settings::save_github_polling_settings_to(&h.root(), &settings).unwrap();
    crate::delivery::github::load_plan_impl(&h.app.handle().clone(), true).expect("a plan");
    h.create_agent("arch", "");
    h
}

fn run_turn(h: &Harness, thread: &Discussion) -> AgentTurn {
    let turn = h
        .dispatch("arch", ConversationOrigin::of(thread), &thread.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(h, &turn.id)
}

fn comments_of(h: &Harness, thread: &Discussion) -> Vec<crate::comments::Comment> {
    crate::comments::fold_map_discussions(&h.root())
        .into_iter()
        .find(|d| d.id == thread.id)
        .expect("thread")
        .comments
}

fn proposals(h: &Harness, thread: &Discussion) -> Vec<crate::map_plan::MapPlanProposal> {
    crate::map_plan::proposals_in(&h.root(), &thread.id)
}

fn draft(key: &str) -> serde_json::Value {
    serde_json::json!({ "key": key, "op": "new_draft", "name": key, "body": "Build it." })
}

fn rows_call(keys: &[&str]) -> serde_json::Value {
    let rows: Vec<serde_json::Value> = keys.iter().map(|k| draft(k)).collect();
    serde_json::json!({ "rationale": said("Two drafts for the payments work."), "rows": rows })
}

fn add_call(key: &str) -> serde_json::Value {
    serde_json::json!({ "updates": [{ "action": "add", "key": key, "row": draft(key) }] })
}

// CVL-FR-KYBE, CVL-FR-NUOY, PPT-FR-IRFE, PPT-FR-PRPX, AGC-FR-ZPOI: the turn
// posts its card, reads an issue, adds a row with no comment, and ends on its
// answer, which is a closing comment with the counts of the turn.
#[test]
fn a_turn_posts_its_card_updates_it_and_closes_with_a_comment() {
    let (h, thread) = plan_harness(vec![
        Ok(ScriptedReply::calls(PLAN_TOOL, rows_call(&["a", "b"]))),
        Ok(ScriptedReply::calls(
            crate::tools::plan_issue_read::READ_NAME,
            serde_json::json!({ "number": 2 }),
        )),
        Ok(ScriptedReply::calls(PLAN_TOOL, add_call("c"))),
        Ok(ScriptedReply::answer(said("I added a third draft after I read issue 2."))),
    ]);
    let turn = run_turn(&h, &thread);
    assert_eq!(turn.state, AgentTurnState::AwaitingReply, "{:?}", turn.failure);
    assert_eq!(h.seam.requests().len(), 4);
    let all = proposals(&h, &thread);
    assert_eq!(all.len(), 1, "one card");
    let keys: Vec<&str> = all[0].rows.iter().map(|r| r.key.as_str()).collect();
    assert_eq!(keys, ["a", "b", "c"]);
    let comments = comments_of(&h, &thread);
    assert_eq!(comments.len(), 3, "the opening, the card, and the closing comment");
    assert!(comments[2].body.contains("I added a third draft"));
    assert!(comments[2].attachments.contains(&Attachment::MapPlanProposal {
        proposal_id: all[0].id.clone(),
        update: Some(ProposalUpdateCounts { changed: 0, added: 1, withdrawn: 0 }),
    }));
}

// CVL-FR-KYBE, CVL-FR-NUOY: where no later call changed the card, the answer is
// discarded. It may be empty or carry no spoken part, and it is not asked for
// again.
#[test]
fn an_unchanged_card_discards_the_answer() {
    for answer in ["", "Done."] {
        let (h, thread) = plan_harness(vec![
            Ok(ScriptedReply::calls(PLAN_TOOL, rows_call(&["a"]))),
            Ok(ScriptedReply::answer(answer)),
        ]);
        let turn = run_turn(&h, &thread);
        assert_eq!(turn.state, AgentTurnState::AwaitingReply, "{answer:?}: {:?}", turn.failure);
        assert_eq!(h.seam.requests().len(), 2, "{answer:?}: the answer is not asked for again");
        assert_eq!(comments_of(&h, &thread).len(), 2, "{answer:?}: the card is the one comment");
    }
}

// PPT-FR-PRPX, PPT-FR-ZMBA: a reply that asks for a new card and an update of it
// dispatches both, in order.
#[test]
fn two_calls_of_one_reply_dispatch_in_order() {
    let reply =
        ScriptedReply::calls(PLAN_TOOL, rows_call(&["a"])).and_calls(PLAN_TOOL, add_call("b"));
    let (h, thread) =
        plan_harness(vec![Ok(reply), Ok(ScriptedReply::answer(said("I offered two drafts.")))]);
    let turn = run_turn(&h, &thread);
    assert_eq!(turn.state, AgentTurnState::AwaitingReply, "{:?}", turn.failure);
    let keys: Vec<String> = proposals(&h, &thread)[0].rows.iter().map(|r| r.key.clone()).collect();
    assert_eq!(keys, ["a", "b"]);
    assert_eq!(comments_of(&h, &thread).len(), 3);
}

// PPT-FR-ZMBA: a second call with `rows` in the turn is refused, and the loop
// goes on.
#[test]
fn a_second_card_of_the_turn_is_refused() {
    let (h, thread) = plan_harness(vec![
        Ok(ScriptedReply::calls(PLAN_TOOL, rows_call(&["a"]))),
        Ok(ScriptedReply::calls(PLAN_TOOL, rows_call(&["b"]))),
        Ok(ScriptedReply::answer("")),
    ]);
    let turn = run_turn(&h, &thread);
    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    let results = tool_results(&h.seam.exchanges()[2]);
    assert!(
        results.iter().any(|r| r.contains(crate::tools::propose_plan_changes::ONE_CARD_A_TURN)),
        "{results:?}"
    );
    assert_eq!(proposals(&h, &thread).len(), 1);
}

// CVL-FR-WJKS: after the card, a question to the author is refused, nothing is
// posted, and the loop goes on.
#[test]
fn a_question_after_the_card_is_refused() {
    let (h, thread) = plan_harness(vec![
        Ok(ScriptedReply::calls(PLAN_TOOL, rows_call(&["a"]))),
        Ok(asks("Which milestone?")),
        Ok(ScriptedReply::answer("")),
    ]);
    let turn = run_turn(&h, &thread);
    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    let results = tool_results(&h.seam.exchanges()[2]);
    assert!(
        results
            .iter()
            .any(|r| r.contains(crate::agent_conversations::plan_card::CARD_POSTED_ASK_IN_REPLY)),
        "{results:?}"
    );
    assert_eq!(comments_of(&h, &thread).len(), 2, "no question is posted");
}

// CVL-FR-YNJS, CVL-FR-FSAY: a Map turn with the plan tools has 12 rounds, and
// the last of them is told so.
#[test]
fn a_planning_turn_has_twelve_rounds() {
    let list = || {
        Ok(ScriptedReply::calls(crate::tools::plan_issue_read::LIST_NAME, serde_json::json!({})))
    };
    let mut replies: Vec<Result<ScriptedReply, &'static str>> = (0..11).map(|_| list()).collect();
    replies.push(Ok(ScriptedReply::answer(said("The plan is in order."))));
    let (h, thread) = plan_harness(replies);
    let turn = run_turn(&h, &thread);
    assert_eq!(turn.state, AgentTurnState::Delivered, "{:?}", turn.failure);
    let exchanges = h.seam.exchanges();
    assert_eq!(exchanges.len(), 12);
    let carries_last_line = |exchange: &[rig::completion::Message]| {
        format!("{exchange:?}").contains(crate::agent_conversations::tool_wiring::LAST_CALL_LINE)
    };
    assert!(carries_last_line(&exchanges[11]));
    assert!(!carries_last_line(&exchanges[7]), "the eighth call is not the last");
}

// CVL-FR-WJKS: after the card, a question set and an export request are
// refused too, and nothing is recorded.
#[test]
fn every_ending_tool_after_the_card_is_refused() {
    for tool in
        [crate::tools::ask_discussion_questions::NAME, crate::tools::request_document_export::NAME]
    {
        let (h, thread) = plan_harness(vec![
            Ok(ScriptedReply::calls(PLAN_TOOL, rows_call(&["a"]))),
            Ok(ScriptedReply::calls(tool, serde_json::json!({}))),
            Ok(ScriptedReply::answer("")),
        ]);
        let turn = run_turn(&h, &thread);
        assert_eq!(turn.state, AgentTurnState::AwaitingReply, "{tool}");
        let results = tool_results(&h.seam.exchanges()[2]);
        assert!(
            results.iter().any(
                |r| r.contains(crate::agent_conversations::plan_card::CARD_POSTED_ASK_IN_REPLY)
            ),
            "{tool}: {results:?}"
        );
        assert_eq!(comments_of(&h, &thread).len(), 2, "{tool}: nothing else is posted");
    }
}

// CVL-FR-SUIZ: a turn that fails after a later call changed its card leaves
// the change on the card, and logs it with the counts and no row text.
#[test]
fn a_failed_turn_keeps_the_changes_of_its_card() {
    let (h, thread) = plan_harness(vec![
        Ok(ScriptedReply::calls(PLAN_TOOL, rows_call(&["a"]))),
        Ok(ScriptedReply::calls(PLAN_TOOL, add_call("zebra"))),
        Err("rejected"),
        Err("rejected"),
        Err("rejected"),
    ]);
    let turn = run_turn(&h, &thread);
    assert_eq!(turn.state, AgentTurnState::Failed);
    let keys: Vec<String> = proposals(&h, &thread)[0].rows.iter().map(|r| r.key.clone()).collect();
    assert_eq!(keys, ["a", "zebra"]);
    let record = wait_for_record(&turn.id, "agent turn ended after it changed its plan card");
    assert_eq!(field(&record, "added"), "1");
    assert!(!serde_json::to_string(&record).unwrap().contains("zebra"));
}

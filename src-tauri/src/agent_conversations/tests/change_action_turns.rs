//! A turn that asks the author to apply or rework a change
//! (`RCA-request-change-action-tool.md`, `CVL-conversation-loop.md`
//! CVL-FR-15, CVL-FR-XHKB, CVL-FR-HNGB).
//!
//! One part of `../tests/mod.rs`, which holds the harness.

use super::*;

const STREAM: &str = "cat-stream";
const CHANGE: &str = "add-auth";

fn requests(action: &str, instruction: Option<&str>) -> serde_json::Value {
    let mut call = serde_json::json!({ "action": action, "rationale": said("It is ready.") });
    if let Some(instruction) = instruction {
        call["instruction"] = serde_json::json!(instruction);
    }
    call
}

/// A harness with a stream whose working copy holds the change, and a
/// whole-change discussion that addresses `arch`.
fn change_turn(
    replies: Vec<Result<ScriptedReply, &'static str>>,
) -> (Harness, tempfile::TempDir, crate::comments::Discussion) {
    let h = Harness::scripted(replies);
    h.app.state::<crate::project::ProjectState>().set_root(h.root.path().to_path_buf());
    let wc = tempfile::TempDir::new().expect("tempdir");
    let base = wc.path().join(format!("openspec/changes/{CHANGE}"));
    std::fs::create_dir_all(&base).expect("dirs");
    std::fs::write(base.join("proposal.md"), "## Why\n\nAuth.\n").expect("proposal");
    crate::change_proposals::set_test_stream(
        STREAM,
        "Auth stream",
        crate::fs::RootFs::for_root(wc.path()),
    );
    let thread = crate::comments::open_discussion_unchecked(
        &h.root(),
        &DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() },
        None,
        "@arch is it ready to build?".into(),
        Vec::new(),
        &Participant::Human { login: "raver119".into(), display_name: None, email: None },
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");
    h.mount();
    h.create_agent("arch", "");
    (h, wc, thread)
}

fn folded(h: &Harness, id: &str) -> crate::comments::Discussion {
    let target = DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() };
    crate::comments::locate_discussion_of(&h.root(), &target, id).expect("thread")
}

// RCA-FR-IKUP / RCA-FR-ABXT / CVL-FR-15 / CVL-FR-XHKB: a recorded request
// ends the turn on the call, `awaiting_reply`; a second call in the same reply
// and the reply's prose are not carried out, and no run starts.
#[test]
fn a_recorded_request_ends_the_turn_and_a_second_is_not_carried_out() {
    let reply =
        ScriptedReply::calls(crate::tools::request_change_action::NAME, requests("apply", None))
            .and_calls(
                crate::tools::request_change_action::NAME,
                requests("rework", Some("Revise the tasks.")),
            )
            .with_text("I would also rework it");
    let (h, _wc, thread) = change_turn(vec![Ok(reply), Ok(ScriptedReply::answer("unreachable"))]);
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    let turn = wait_for_terminal(&h, &turn.id);

    assert_eq!(turn.state, AgentTurnState::AwaitingReply);
    assert_eq!(h.seam.requests().len(), 1, "the turn ended on its first reply");
    let thread = folded(&h, &thread.id);
    assert_eq!(thread.comments.len(), 2, "one contribution");
    assert!(matches!(
        thread.comments[1].attachments.as_slice(),
        [crate::comments::Attachment::ChangeActionRequest { action, .. }] if action == "apply"
    ));
    assert!(!thread.comments.iter().any(|c| c.body.contains("also rework")));
    let all = crate::change_action_requests::list_requests_impl(&h.root(), STREAM, CHANGE);
    assert_eq!(all.len(), 1, "one turn records at most one request");
    assert_eq!(all[0].action, crate::change_action_requests::ChangeAction::Apply);
    assert!(all[0].run_id.is_none(), "no run starts");
}

// RCA-FR-ABXT / RCA-FR-IKUP / CVL-FR-15: where the first call refused, the
// second call in the reply is not dispatched and gets the one at a time
// refusal, and the loop goes on.
#[test]
fn a_second_call_after_a_refused_one_gets_the_one_at_a_time_refusal() {
    let reply =
        ScriptedReply::calls(crate::tools::request_change_action::NAME, requests("archive", None))
            .and_calls(crate::tools::request_change_action::NAME, requests("apply", None));
    let (h, _wc, thread) =
        change_turn(vec![Ok(reply), Ok(ScriptedReply::answer("I could not ask."))]);
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    let turn = wait_for_terminal(&h, &turn.id);

    assert_eq!(turn.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.requests().len(), 2, "the loop went on after the refusal");
    let results = tool_results(&h.seam.exchanges()[1]);
    assert!(
        results.iter().any(|r| r.contains(crate::tools::request_change_action::ONE_AT_A_TIME)),
        "{results:?}"
    );
    assert!(
        crate::change_action_requests::list_requests_impl(&h.root(), STREAM, CHANGE).is_empty(),
        "nothing was recorded"
    );
}

// CVL-FR-15: `request_change_action` ends the turn, and it is one of the
// tools an advisor turn does not carry.
#[test]
fn the_request_tool_ends_the_turn_and_advisors_lack_it() {
    assert!(crate::agent_conversations::turn_body::ends_turn(
        crate::tools::request_change_action::NAME
    ));
    assert!(crate::agent_conversations::groups::ADVISOR_WITHHELD_TOOLS
        .contains(&crate::tools::request_change_action::NAME));
}

// RCA-FR-KZIJ / CVL-FR-XHKB: a turn whose target is not a change, a draft
// turn and an artifact turn, does not carry `request_change_action`.
#[test]
fn a_draft_turn_and_an_artifact_turn_lack_the_request_tool() {
    for kind in ["draft", "artifact"] {
        let h = Harness::new(vec![Ok("ok".into())]);
        h.mount();
        h.create_agent("arch", "");
        let thread = if kind == "draft" {
            super::support::seed_discussion(&h, &["@arch what now?"]).1
        } else {
            h.seed_artifact_thread("spec.md", "Some artifact source here.")
        };
        let turn = h
            .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
            .expect("dispatch");
        wait_for_terminal(&h, &turn.id);
        let request = h.seam.requests().into_iter().next().expect("one request").0;
        let tools: Vec<String> = request.tools.iter().map(|t| t.name.to_string()).collect();
        assert!(!tools.is_empty(), "{kind}: the turn has tools");
        assert!(
            !tools.iter().any(|t| t == crate::tools::request_change_action::NAME),
            "{kind}: {tools:?}"
        );
    }
}

// CVL-FR-HNGB / AGC-FR-UFAY / RCA-FR-KZIJ: an advisor turn on a change
// discussion does not carry `request_change_action`, which the Meta turn of
// the same change has. An advisor that calls it gets the refusal of an
// unknown tool, and nothing is recorded.
#[test]
fn an_advisor_that_calls_the_request_tool_gets_the_unknown_tool_refusal() {
    use super::agent_groups::{
        meta_asks_advisors, persona_of, switch, text_reply, tool_reply, RoutedSeam, ADVISOR_MARK,
    };
    const NAME: &str = crate::tools::request_change_action::NAME;
    let seam = RoutedSeam::replying(|r, exchange| {
        if r.instructions.contains(ADVISOR_MARK) {
            if exchange <= 1 {
                Ok(tool_reply(NAME, requests("apply", None)))
            } else {
                Ok(text_reply(said("Important points:\n- a point\nKey questions:\n- a question")))
            }
        } else {
            Ok(meta_asks_advisors(r, exchange, "Is it ready?", text_reply(said("Meta answer."))))
        }
    });
    let h = Harness::with_patient_seam(Box::new(seam.clone()));
    h.app.state::<crate::project::ProjectState>().set_root(h.root.path().to_path_buf());
    let wc = tempfile::TempDir::new().expect("tempdir");
    let base = wc.path().join(format!("openspec/changes/{CHANGE}"));
    std::fs::create_dir_all(&base).expect("dirs");
    std::fs::write(base.join("proposal.md"), "## Why\n\nAuth.\n").expect("proposal");
    crate::change_proposals::set_test_stream(
        STREAM,
        "Auth stream",
        crate::fs::RootFs::for_root(wc.path()),
    );
    let thread = crate::comments::open_discussion_unchecked(
        &h.root(),
        &DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() },
        None,
        "@arch is it ready to build?".into(),
        Vec::new(),
        &Participant::Human { login: "raver119".into(), display_name: None, email: None },
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");
    h.mount();
    h.create_agent("arch", "ARCH-PERSONA");
    h.create_agent("sec", "SEC-PERSONA");
    switch(&h, true, 60);
    dispatch_group_impl(
        &h.app.handle().clone(),
        Roots::same(&h.root()),
        PROJECT_KEY,
        &["arch".to_string()],
        ConversationOrigin::of(&thread),
        &thread.comments[0].id,
    )
    .expect("dispatch");
    h.settle();

    let requests = seam.requests();
    let has_tool = |r: &AgentRequest| r.tools.iter().any(|t| t.name.as_str() == NAME);
    let advisor: Vec<&AgentRequest> =
        requests.iter().filter(|r| r.instructions.contains(ADVISOR_MARK)).collect();
    assert_eq!(advisor.len(), 2, "the advisor called the tool, then answered");
    assert!(advisor.iter().all(|r| !has_tool(r)), "the advisor does not carry it");
    let meta = requests
        .iter()
        .find(|r| persona_of(r) == "ARCH" && !r.instructions.contains(ADVISOR_MARK))
        .expect("the Meta turn");
    assert!(has_tool(meta), "the Meta turn of the change carries it");
    let unknown = "No tool by that name is available to you.";
    assert!(seam.exchanges().iter().any(|e| e.contains(unknown)), "the unknown-tool refusal");
    assert!(
        crate::change_action_requests::list_requests_impl(&h.root(), STREAM, CHANGE).is_empty(),
        "nothing was recorded"
    );
    drop(wc);
}

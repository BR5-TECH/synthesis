//! A turn on an OpenSpec change (AGC-FR-05, AGC-FR-NBZN, AGC-FR-HETC,
//! AGC-FR-OYJX, AGC-FR-TKXH, AGC-FR-WPAP, CVL-FR-FGGI, CVL-FR-XUJX,
//! CVL-FR-EZKH, CVL-FR-15, CVL-FR-06).

use super::*;

const STREAM: &str = "ct-stream";
const CHANGE: &str = "add-auth";

/// A stream working copy with one change of three files, one of them not
/// text, registered for the turn builders.
fn working_copy(stream: &str) -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let base = dir.path().join(format!("openspec/changes/{CHANGE}"));
    std::fs::create_dir_all(base.join("specs/auth")).expect("dirs");
    std::fs::write(base.join("proposal.md"), "## Why\n\nUsers need to log in.\n")
        .expect("proposal");
    std::fs::write(base.join("tasks.md"), "## 1. Login\n\n- [ ] 1.1 Add the form\n")
        .expect("tasks");
    std::fs::write(base.join("specs/auth/spec.md"), "## ADDED Requirements\n").expect("spec");
    std::fs::write(base.join("logo.bin"), [0xff_u8, 0xfe, 0x00, 0x81]).expect("bin");
    crate::change_proposals::set_test_stream(
        stream,
        "Auth stream",
        crate::fs::RootFs::for_root(dir.path()),
    );
    dir
}

fn target(stream: &str) -> DiscussionTarget {
    DiscussionTarget::Change { stream_id: stream.into(), change: CHANGE.into() }
}

fn open(h: &Harness, stream: &str, fragment: Option<FragmentTarget>) -> Discussion {
    crate::comments::open_discussion_unchecked(
        &h.root(),
        &target(stream),
        fragment,
        "@arch what is missing?".into(),
        Vec::new(),
        &Participant::Human { login: "raver119".into(), display_name: None, email: None },
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion")
}

fn first_request(h: &Harness, thread: &Discussion) -> AgentRequest {
    h.mount();
    h.create_agent("arch", "");
    let turn = h
        .dispatch("arch", ConversationOrigin::of(thread), &thread.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(h, &turn.id);
    h.seam.requests().into_iter().next().expect("one request").0
}

// AGC-FR-05 / AGC-FR-NBZN / AGC-FR-HETC: the `change` builder fills
// `openspec_change` with the change, the stream name, and every file of the
// change in path order; a file that is not text is named with no content.
#[test]
fn the_change_builder_holds_every_file_of_the_change() {
    let h = Harness::new(vec![]);
    let _wc = working_copy(STREAM);
    let thread = open(&h, STREAM, None);
    let input = build_input(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &thread.comments[0].id,
    );
    let material = &input[0];
    assert_eq!(material.tag, "openspec_change");
    assert!(material.attributes.contains(&("change".to_string(), CHANGE.to_string())));
    assert!(material.attributes.contains(&("stream".to_string(), "Auth stream".to_string())));
    let order: Vec<usize> = [
        "--- logo.bin ---",
        "--- proposal.md ---",
        "--- specs/auth/spec.md ---",
        "--- tasks.md ---",
    ]
    .iter()
    .map(|header| material.body.find(header).unwrap_or_else(|| panic!("{header} missing")))
    .collect();
    assert!(order.windows(2).all(|w| w[0] < w[1]), "the files are in path order");
    assert!(material.body.contains("Users need to log in."));
    assert!(material.body.contains("[This file is not text. Its content is not included.]"));
    let tags: Vec<&str> = input.iter().map(|s| s.tag.as_str()).collect();
    assert_eq!(tags, crate::agent_conversations::CHANGE_INPUT_TAGS.to_vec());
}

// AGC-FR-OYJX: a change whose stream is not live, or whose folder is gone, has
// no context.
#[test]
fn a_change_with_no_stream_or_no_folder_has_no_context() {
    let h = Harness::new(vec![]);
    let gone = open(&h, "ct-no-stream", None);
    assert!(build_input(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&gone),
        &gone.comments[0].id
    )
    .is_empty());
    let wc = working_copy("ct-empty");
    std::fs::remove_dir_all(wc.path().join("openspec")).expect("remove");
    let empty = open(&h, "ct-empty", None);
    assert!(build_input(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&empty),
        &empty.comments[0].id
    )
    .is_empty());
}

// AGC-FR-TKXH: the document language comes from `proposal.md`, and for a
// fragment discussion from the file of the fragment.
#[test]
fn the_document_language_reads_the_proposal_or_the_fragment_file() {
    let h = Harness::new(vec![]);
    let _wc = working_copy("ct-lang");
    let whole = crate::agent_conversations::document_language::target_text(
        Roots::same(&h.root()),
        &target("ct-lang"),
        None,
    );
    assert_eq!(whole.as_deref(), Some("## Why\n\nUsers need to log in.\n"));
    let fragment = FragmentTarget {
        owner: target("ct-lang"),
        path: "tasks.md".into(),
        start: 0,
        end: 4,
        quote: "## 1".into(),
    };
    let part = crate::agent_conversations::document_language::target_text(
        Roots::same(&h.root()),
        &target("ct-lang"),
        Some(&fragment),
    );
    assert!(part.expect("text").starts_with("## 1. Login"));
}

// AGC-FR-05 / CVL-FR-FGGI / CVL-FR-EZKH / CVL-FR-06: a whole-change turn is
// answered under `discuss-change.md`, carries the change tools and
// `ask_discussion_questions`, no other proposal tool, and its prompt names the
// tags its builder emits.
#[test]
fn a_whole_change_turn_uses_the_change_prompt_and_tools() {
    let h = Harness::new(vec![Ok("ok".into())]);
    let _wc = working_copy("ct-whole");
    let thread = open(&h, "ct-whole", None);
    assert_eq!(ConversationOrigin::of(&thread).kind(), OriginKind::ChangeDiscussion);
    assert_eq!(OriginKind::ChangeDiscussion.as_str(), "change_discussion");
    assert_eq!(prompt_template(OriginKind::ChangeDiscussion), DISCUSS_CHANGE_PROMPT_TEMPLATE);
    let request = first_request(&h, &thread);
    assert!(request.instructions.contains("one OpenSpec change"));
    let tools: Vec<String> = request.tools.iter().map(|t| t.name.to_string()).collect();
    for name in [
        crate::tools::propose_change_edits::NAME,
        crate::tools::decide_change_edits::NAME,
        crate::tools::ask_discussion_questions::NAME,
        crate::tools::ask_user_comment::NAME,
        crate::tools::request_document_export::NAME,
    ] {
        assert!(tools.iter().any(|t| t == name), "{name} missing from {tools:?}");
    }
    for name in [
        crate::tools::propose_prompt_changes::NAME,
        crate::tools::propose_draft_changes::NAME,
        crate::tools::decide_prompt_changes::NAME,
        crate::tools::decide_draft_changes::NAME,
        crate::tools::propose_new_draft::NAME,
    ] {
        assert!(!tools.iter().any(|t| t == name), "{name} offered on a change turn");
    }
    let described = super::request_shape::tags_described_by(DISCUSS_CHANGE_PROMPT_TEMPLATE);
    let emitted: std::collections::BTreeSet<String> =
        request.input.iter().map(|s| s.tag.clone()).collect();
    assert_eq!(described, emitted);
}

// CVL-FR-XUJX / CVL-FR-EZKH: a fragment of a change is answered under
// `comment.md`, and it carries no `ask_discussion_questions`.
#[test]
fn a_change_fragment_turn_uses_the_comment_prompt() {
    let h = Harness::new(vec![Ok("ok".into())]);
    let _wc = working_copy("ct-frag");
    let fragment = FragmentTarget {
        owner: target("ct-frag"),
        path: "tasks.md".into(),
        start: 0,
        end: 4,
        quote: "## 1".into(),
    };
    let thread = open(&h, "ct-frag", Some(fragment));
    assert_eq!(ConversationOrigin::of(&thread).kind(), OriginKind::ChangeComment);
    assert_eq!(prompt_template(OriginKind::ChangeComment), COMMENT_PROMPT_TEMPLATE);
    let request = first_request(&h, &thread);
    let tools: Vec<String> = request.tools.iter().map(|t| t.name.to_string()).collect();
    assert!(tools.iter().any(|t| t == crate::tools::propose_change_edits::NAME));
    assert!(!tools.iter().any(|t| t == crate::tools::ask_discussion_questions::NAME));
    assert!(COMMENT_PROMPT_TEMPLATE.contains("<openspec_change>"));
}

// CVL-FR-15 / CVL-FR-HNGB: `propose_change_edits` ends the turn, and an
// advisor carries neither change tool.
#[test]
fn the_change_proposal_tool_ends_the_turn_and_advisors_lack_both() {
    assert!(crate::agent_conversations::turn_body::ends_turn(
        crate::tools::propose_change_edits::NAME
    ));
    assert!(!crate::agent_conversations::turn_body::ends_turn(
        crate::tools::decide_change_edits::NAME
    ));
    for name in [crate::tools::propose_change_edits::NAME, crate::tools::decide_change_edits::NAME]
    {
        assert!(crate::agent_conversations::groups::ADVISOR_WITHHELD_TOOLS.contains(&name));
    }
}

// AGC-FR-WPAP: the synthesis turn of a group on a change is told to propose
// with `propose_change_edits`, and that tool is the one a change turn has.
#[test]
fn a_group_on_a_change_proposes_with_the_change_tool() {
    assert_eq!(
        crate::agent_conversations::groups::proposal_tool_of(&target("ct-group")),
        crate::tools::propose_change_edits::NAME
    );
    assert_eq!(
        crate::agent_conversations::groups::proposal_tool_of(&DiscussionTarget::Artifact {
            artifact_id: "a.md".into()
        }),
        crate::tools::propose_prompt_changes::NAME
    );
    let source = include_str!("../groups.rs");
    assert!(
        source.contains("|| name == crate::tools::propose_change_edits::NAME"),
        "a call of `propose_change_edits` counts as the turn's proposal"
    );
}

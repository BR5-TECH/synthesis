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

// AGC-FR-05 / CVL-FR-FGGI / CVL-FR-EZKH / CVL-FR-PEFM / CVL-FR-06: a whole-change turn is
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
        crate::tools::propose_new_change::NAME,
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
    // CVL-FR-PEFM: a fragment turn on a change has the split tool too.
    assert!(tools.iter().any(|t| t == crate::tools::propose_new_change::NAME));
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

// AGC-FR-35 / AGC-FR-WMIY / AGC-FR-DLZE: the change section carries an image
// that a file references inside the change folder as image content after its
// reference; a reference out of the folder, a URL, and a missing file are
// metadata alone.
#[test]
fn the_change_section_carries_the_images_its_files_reference() {
    let h = Harness::new(vec![]);
    let wc = working_copy("ct-img");
    let base = wc.path().join(format!("openspec/changes/{CHANGE}"));
    std::fs::create_dir_all(base.join("images")).expect("dir");
    std::fs::write(base.join("images/flow.png"), [0x89, b'P', b'N', b'G']).expect("png");
    std::fs::write(wc.path().join("outside.png"), [0x89, b'P']).expect("outside");
    std::fs::write(
        base.join("design.md"),
        "Flow:\n\n![flow](images/flow.png)\n\n![out](../../../outside.png) ![web](https://x.test/a.png) ![gone](images/none.png)\n",
    )
    .expect("design");
    let thread = open(&h, "ct-img", None);
    let input = build_input(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &thread.comments[0].id,
    );
    let parts = &input[0].parts;
    let images: Vec<&InputImage> = parts
        .iter()
        .filter_map(|p| match p {
            InputPart::Image(image) => Some(image),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 1, "one image resolves inside the change folder");
    assert_eq!(images[0].media_type, "image/png");
    assert!(
        images[0].context.contains("design.md") && images[0].context.contains("images/flow.png")
    );
    let text: String = parts
        .iter()
        .filter_map(|p| match p {
            InputPart::Text(t) => Some(t.as_str()),
            _ => None,
        })
        .collect();
    let flow_ref = text.find("![flow](images/flow.png)").expect("reference kept");
    assert!(text.contains("[image: design.md: ![out](../../../outside.png), not resolved]"));
    assert!(text.contains("![web](https://x.test/a.png), not resolved]"));
    assert!(text.contains("![gone](images/none.png), not resolved]"));
    assert!(flow_ref < text.find("[image: design.md: ![out]").unwrap());
}

// AGC-FR-DLZE: a file left out under the bound contributes no image.
#[test]
fn a_file_too_large_contributes_no_image() {
    let h = Harness::new(vec![]);
    let wc = working_copy("ct-big");
    let base = wc.path().join(format!("openspec/changes/{CHANGE}"));
    std::fs::write(base.join("flow.png"), [0x89, b'P', b'N', b'G']).expect("png");
    std::fs::write(base.join("design.md"), format!("![flow](flow.png)\n{}", "x".repeat(400)))
        .expect("design");
    let thread = open(&h, "ct-big", None);
    let tiny = DocumentBudget { window: 100, percent: 1 };
    let input = build_input_within(
        Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &thread.comments[0].id,
        &tiny,
    );
    assert!(!input.sections[0].parts.iter().any(|p| matches!(p, InputPart::Image(_))));
}

// AGC-FR-JNOS / AGC-FR-LTLT: the history of a change turn ends with the split,
// and the proposal's comment names the proposed change and its state.
#[test]
fn the_history_of_a_change_turn_ends_with_the_split() {
    let h = Harness::new(vec![]);
    let wc = working_copy("ct-split");
    let thread = open(&h, "ct-split", None);
    let sites = crate::change_proposals::FixedSites {
        sites: vec![("ct-split".into(), crate::fs::RootFs::for_root(wc.path()))],
        busy: std::sync::Mutex::new(None),
    };
    let record = crate::new_change_proposals::record_new_change_proposal(
        &h.app.handle().clone(),
        &h.root(),
        &sites,
        crate::new_change_proposals::NewChange {
            stream_id: "ct-split",
            change: CHANGE,
            name: "part-one",
            files: &[crate::new_change_proposals::ProposedFile {
                path: "proposal.md".into(),
                text: "## Why\n\nPart one only.\n".into(),
            }],
            position: 1,
            total: 2,
            rationale: "Split. <spoken>Split.</spoken>",
            agent: &Participant::Agent {
                agent_id: "a".into(),
                handle: "arch".into(),
                model: None,
                title: None,
            },
            thread_id: &thread.id,
        },
    )
    .expect("recorded");
    // The author answers after the proposal, which puts the proposal's comment
    // in the history of the turn the answer triggers.
    let answer = crate::comments::add_comment_with_id(
        &h.root(),
        open_again(&h, "ct-split", &thread.id).log_ref(),
        &thread.id,
        "answer-1".into(),
        "@arch why part one?".into(),
        &Participant::Human { login: "raver119".into(), display_name: None, email: None },
        "2999-01-01T00:00:00Z",
    )
    .expect("answer");
    let input = build_input(Roots::same(&h.root()), &ConversationOrigin::of(&answer), "answer-1");
    let history = input.iter().find(|s| s.tag == TAG_DISCUSSION_HISTORY).expect("history");
    assert!(history
        .body
        .contains(&format!("The split {} of this change, not confirmed", record.set_id)));
    assert!(history.body.contains("Part one only."), "the pending files are shown");
    assert!(history
        .body
        .contains(&format!("[proposed a new OpenSpec change: {} — 1 of 2 — part-one", record.id)));
}

fn open_again(h: &Harness, stream: &str, id: &str) -> Discussion {
    crate::comments::locate_discussion_of(&h.root(), &target(stream), id).expect("thread")
}

// AGC-FR-YFLJ / CST-FR-VJNQ: a change turn records its `conversation_turn`
// into the change's statistics log and into no draft log.
#[test]
fn a_change_turn_records_into_the_change_statistics() {
    let h = Harness::new(vec![Ok("ok <spoken>ok</spoken>".into())]);
    h.app.state::<crate::project::ProjectState>().set_root(h.root.path().to_path_buf());
    let _wc = working_copy("ct-stats");
    let thread = open(&h, "ct-stats", None);
    first_request(&h, &thread);
    h.settle();
    crate::statistics::wait_for_writer();
    let log = h.root.path().join(format!("changes/ct-stats/{CHANGE}/statistics.jsonl"));
    let text = std::fs::read_to_string(log).expect("change log");
    assert!(text.contains("\"type\":\"conversation_turn\""));
    assert!(text.contains(&thread.id));
    assert!(!h.root.path().join("statistics").exists(), "no draft log");
}

// CVL-FR-BOFI: the change prompt says how a split is made.
#[test]
fn the_change_prompt_explains_the_split() {
    assert!(DISCUSS_CHANGE_PROMPT_TEMPLATE.contains("## Splitting the change"));
    assert!(DISCUSS_CHANGE_PROMPT_TEMPLATE.contains("one new change at a time"));
}

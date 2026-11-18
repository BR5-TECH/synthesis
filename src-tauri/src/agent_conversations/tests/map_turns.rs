//! A turn on a Map thread (AGC-FR-05, AGC-FR-UOPJ, CVL-FR-LCSN, CVL-FR-06,
//! CVL-FR-15, CVL-FR-HNGB; `MPR-map-plan-proposals.md` MPR-FR-PMLX,
//! MPR-FR-NXDX, MPR-FR-ZCAM).

use std::sync::atomic::AtomicBool;

use super::*;
use crate::agent_conversations::map_context::{focus_of, map_context_section, Focus};
use crate::comments::{
    Attachment, AttachmentInput, MapLens, MapNodeKind, MapNodeRef, Opening, Stamp, ThreadRef,
};
use crate::map_plan::{
    CreateIssueFields, MapPlanRow, ModifyIssueFields, NewDraftFields, ParentRef, ProposalComment,
    RowChange, RowFields, RowState,
};
use crate::tools::tests::plan_fixture::PlanFixture;

const PLAN_TOOLS: [&str; 4] = [
    crate::tools::plan_issue_read::LIST_NAME,
    crate::tools::plan_issue_read::SEARCH_NAME,
    crate::tools::plan_issue_read::READ_NAME,
    crate::tools::propose_plan_changes::NAME,
];

fn agent(id: &str, handle: &str) -> Participant {
    Participant::Agent {
        agent_id: id.into(),
        handle: handle.into(),
        model: None,
        title: Some(String::new()),
    }
}

fn node(kind: MapNodeKind, id: &str) -> MapNodeRef {
    MapNodeRef { lens: MapLens::Delivery, kind, id: id.into(), label: format!("Label of {id}") }
}

fn at(second: u32) -> String {
    format!("2026-03-01T10:00:{second:02}Z")
}

/// Open the Map thread of `map_node` as `ada`, with the opening comment `body`.
fn open_map(root: &crate::fs::RootFs, map_node: Option<MapNodeRef>, body: &str) -> Discussion {
    crate::comments::open_map_discussion_in(
        root,
        map_node,
        Opening { body: body.into(), attachments: Vec::new(), spoken_original: None },
        Stamp { by: &human("ada"), at: &at(0) },
    )
    .expect("a Map thread")
    .discussion
}

/// A human comment of the Map thread, naming `focus` where it is given.
fn human_comment(
    root: &crate::fs::RootFs,
    thread: &str,
    body: &str,
    focus: Option<MapNodeRef>,
    second: u32,
) -> String {
    let attachments: Vec<AttachmentInput> =
        focus.into_iter().map(|node| AttachmentInput::MapNode { node }).collect();
    crate::comments::add_comment_reporting_id(
        root,
        ThreadRef::map_discussion(),
        thread,
        body.into(),
        Vec::new(),
        attachments,
        None,
        Stamp { by: &human("ada"), at: &at(second) },
    )
    .expect("a comment")
    .1
}

fn thread_of(root: &crate::fs::RootFs, id: &str) -> Discussion {
    crate::comments::fold_map_discussions(root).into_iter().find(|d| d.id == id).expect("thread")
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

// AGC-FR-05, AGC-FR-UOPJ, CVL-FR-LCSN, CVL-FR-06, MPR-FR-ZCAM: a Map turn is
// answered under `discuss-map.md`, its prompt names the tags its builder emits,
// it carries the gathering tools, `request_document_export`, and
// `ask_discussion_questions`, no proposal or decision tool, and no plan tool
// where the delivery source is not a GitHub Project.
#[test]
fn a_map_turn_uses_the_map_prompt_and_the_gathering_tools() {
    let h = Harness::new(vec![Ok("ok".into())]);
    let thread = open_map(&h.root(), None, "@arch what is missing?");
    assert_eq!(ConversationOrigin::of(&thread).kind(), OriginKind::MapDiscussion);
    assert_eq!(OriginKind::MapDiscussion.as_str(), "map_discussion");
    assert_eq!(prompt_template(OriginKind::MapDiscussion), DISCUSS_MAP_PROMPT_TEMPLATE);
    let request = first_request(&h, &thread);
    assert!(request.instructions.contains("discussion about the **Map**"));
    let tools: Vec<String> = request.tools.iter().map(|t| t.name.to_string()).collect();
    for name in [
        crate::tools::ask_discussion_questions::NAME,
        crate::tools::ask_user_comment::NAME,
        crate::tools::request_document_export::NAME,
        crate::tools::file_read::NAME,
        crate::tools::spec_search::NAME,
    ] {
        assert!(tools.iter().any(|t| t == name), "{name} missing from {tools:?}");
    }
    for name in [
        crate::tools::propose_prompt_changes::NAME,
        crate::tools::propose_draft_changes::NAME,
        crate::tools::propose_change_edits::NAME,
        crate::tools::propose_new_draft::NAME,
        crate::tools::decide_prompt_changes::NAME,
        crate::tools::decide_draft_changes::NAME,
        crate::tools::decide_change_edits::NAME,
    ]
    .into_iter()
    .chain(PLAN_TOOLS)
    {
        assert!(!tools.iter().any(|t| t == name), "{name} offered on a Map turn: {tools:?}");
    }
    let emitted: Vec<String> = request.input.iter().map(|s| s.tag.clone()).collect();
    assert_eq!(emitted, MAP_INPUT_TAGS.map(str::to_string).to_vec());
    let described = super::request_shape::tags_described_by(DISCUSS_MAP_PROMPT_TEMPLATE);
    assert_eq!(described, emitted.into_iter().collect());
}

// MPR-FR-PMLX: the focus of a turn is the node reference of the latest human
// comment up to the trigger that carries one, else the node reference of the
// thread, else the whole Map. A comment of an agent never sets it.
#[test]
fn the_focus_is_the_latest_node_of_a_human_comment_then_the_thread_then_the_map() {
    let h = Harness::new(vec![]);
    let root = h.root();
    let whole = open_map(&root, None, "About the map");
    let (a, b, c) = (
        node(MapNodeKind::Item, "a"),
        node(MapNodeKind::Milestone, "b"),
        node(MapNodeKind::Spec, "c"),
    );
    assert_eq!(focus_of(&whole, &whole.comments[0].id), Focus::WholeMap);

    let about_a = open_map(&root, Some(a.clone()), "About a");
    assert_eq!(focus_of(&about_a, &about_a.comments[0].id), Focus::Node(&a));

    let second = human_comment(&root, &about_a.id, "Now b", Some(b.clone()), 1);
    crate::comments::append_agent_comment_with(
        &root,
        ThreadRef::map_discussion(),
        &about_a.id,
        crate::notes::new_note_id(),
        "An agent names c".into(),
        vec![Attachment::MapNode { node: c.clone() }],
        Stamp { by: &agent("a1", "arch"), at: &at(2) },
    )
    .unwrap();
    let third = human_comment(&root, &about_a.id, "No node here", None, 3);
    let thread = thread_of(&root, &about_a.id);
    assert_eq!(focus_of(&thread, &thread.comments[0].id), Focus::Node(&a), "up to the trigger");
    assert_eq!(focus_of(&thread, &second), Focus::Node(&b));
    assert_eq!(focus_of(&thread, &third), Focus::Node(&b), "the latest comment that carries one");
    assert_eq!(focus_of(&thread, "unknown trigger"), Focus::Node(&b));

    let section = map_context_section(Roots::same(&root), &thread, &second);
    let attribute =
        |key: &str| section.attributes.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    assert_eq!(attribute("focus").as_deref(), Some("node"));
    assert_eq!(attribute("focus_lens").as_deref(), Some("delivery"));
    assert_eq!(attribute("focus_kind").as_deref(), Some("milestone"));
    assert_eq!(attribute("focus_id").as_deref(), Some("b"));
    assert!(section.body.starts_with("Focus: the delivery milestone"));
    let whole_section = map_context_section(Roots::same(&root), &whole, &whole.comments[0].id);
    assert!(whole_section.attributes.contains(&("focus".into(), "map".into())));
    assert!(whole_section.body.starts_with("Focus: the whole Map."));
}

fn rows() -> Vec<MapPlanRow> {
    let row = |key: &str, fields: RowFields| MapPlanRow {
        key: key.into(),
        fields,
        state: RowState::Waiting,
        blocked_by: None,
        error: None,
        result: None,
    };
    let create = |title: &str, parent: Option<ParentRef>| {
        RowFields::CreateIssue(CreateIssueFields {
            title: title.into(),
            body: None,
            issue_type: None,
            milestone: None,
            parent,
            labels: Vec::new(),
        })
    };
    vec![
        row("a", create("Relay auth", None)),
        row(
            "b",
            RowFields::NewDraft(NewDraftFields { name: "Finish relay".into(), body: "x".into() }),
        ),
        row(
            "c",
            RowFields::ModifyIssue(ModifyIssueFields {
                number: 2,
                title: "Issue 2".into(),
                base_updated_at: "2000-01-01T00:00:00Z".into(),
                changes: vec![RowChange {
                    field: "title".into(),
                    before: serde_json::json!("Issue 2"),
                    after: serde_json::json!("Renamed"),
                    added: None,
                    removed: None,
                    reason: None,
                }],
            }),
        ),
        row("d", create("Relay tests", Some(ParentRef::Row("a".into())))),
    ]
}

// MPR-FR-NXDX, AGC-FR-UOPJ: the input of a Map turn holds the focus and each row
// of the proposals of the discussion with its state, so the agent knows what the
// author decided. The section is built from the store and makes no request.
#[test]
fn the_input_holds_each_row_with_its_state() {
    let f = PlanFixture::new();
    let roots = Roots { worktree: &f.h.root(), store: &f.store() };
    let origin = f.origin();
    let empty = map_context_section(roots, &thread_of(roots.store, &f.discussion_id), "");
    assert!(empty.body.contains("Plan proposals of this discussion:\nnone\n"));

    let comment = ProposalComment {
        body: "Rows. <spoken>Rows.</spoken>".into(),
        author: agent("a1", "arch"),
        spoken_original: None,
    };
    let proposal =
        crate::map_plan::record_map_plan_proposal(&f.app(), &f.discussion_id, rows(), comment)
            .unwrap();
    crate::map_plan::reject_row(&f.app(), &f.discussion_id, &proposal.id, "a").unwrap();
    crate::map_plan::accept_row(&f.app(), &f.discussion_id, &proposal.id, "b").unwrap();
    crate::map_plan::accept_row(&f.app(), &f.discussion_id, &proposal.id, "c").unwrap();

    f.h.fake.lock().calls.clear();
    let input =
        build_input(roots, &origin, &thread_of(roots.store, &f.discussion_id).comments[0].id);
    // The pure builder gives no `plan_references`: the dispatch path adds it,
    // because the read of the delivery source needs the application handle.
    assert_eq!(
        input.iter().map(|s| s.tag.as_str()).collect::<Vec<_>>(),
        vec![TAG_MAP_CONTEXT, TAG_DISCUSSION_HISTORY, TAG_CURRENT_COMMENT]
    );
    let body = &input[0].body;
    assert!(body.contains("Focus: the whole Map."), "{body}");
    for line in [
        "- proposal 1 row \"a\": new issue \"Relay auth\": rejected",
        "- proposal 1 row \"b\": new draft \"Finish relay\": accepted",
        "- proposal 1 row \"c\": change to issue 2 \"Issue 2\": failed (error changed_on_tracker)",
        "- proposal 1 row \"d\": new issue \"Relay tests\": waiting (blocked: row \"a\" is rejected)",
    ] {
        assert!(body.contains(line), "{line} missing from {body}");
    }
    assert!(f.h.fake.calls().is_empty(), "the builder makes no request: {:?}", f.h.fake.calls());
}

// MPR-FR-NXDX, AGC-FR-UOPJ, GDP-FR-LKXA: the short summary of the plan, the
// milestones with their issue counts and the Types that map to no plan kind, is
// added from the read cache of the GitHub plan with no request. Where the
// source is not a GitHub Project, nothing is added.
#[test]
fn the_plan_summary_comes_from_the_read_cache_and_makes_no_request() {
    let f = PlanFixture::new();
    f.h.fake.board_issue(crate::delivery::github::tests::fake::issue(7, Some("Idea")), None);
    f.refresh();
    let roots = Roots { worktree: &f.h.root(), store: &f.store() };
    let mut sections =
        build_input(roots, &f.origin(), &thread_of(roots.store, &f.discussion_id).comments[0].id);
    let before = sections[0].body.clone();
    f.h.fake.lock().calls.clear();
    crate::agent_conversations::map_context::extend_map_material(
        &f.app(),
        roots,
        &DocumentBudget::default_window(),
        &mut sections,
    );
    assert!(f.h.fake.calls().is_empty(), "{:?}", f.h.fake.calls());
    let body = &sections[0].body;
    assert!(body.starts_with(&before));
    assert!(body.contains("Delivery plan summary (from the last read of GitHub"), "{body}");
    assert!(body.contains("Milestones: 1"), "{body}");
    assert!(body.contains("\"Milestone 1\":"), "{body}");
    assert!(body.contains("Types with no plan kind (not in the plan): Idea (1 issues)"), "{body}");

    f.h.select_project(None);
    let mut plain =
        build_input(roots, &f.origin(), &thread_of(roots.store, &f.discussion_id).comments[0].id);
    let plain_body = plain[0].body.clone();
    crate::agent_conversations::map_context::extend_map_material(
        &f.app(),
        roots,
        &DocumentBudget::default_window(),
        &mut plain,
    );
    assert_eq!(plain[0].body, plain_body);
}

fn plan_tool_names(f: &PlanFixture, origin: &ConversationOrigin) -> Vec<String> {
    let roots = Roots { worktree: &f.h.root(), store: &f.store() };
    let proposed = Arc::new(AtomicBool::new(false));
    let author = agent("a1", "arch");
    let mut names: Vec<String> = crate::agent_conversations::tool_wiring::plan_tools_for_turn(
        &f.app(),
        roots,
        origin,
        &author,
        &proposed,
    )
    .iter()
    .map(|t| t.name().to_string())
    .collect();
    names.sort();
    names
}

// MPR-FR-ZCAM, CVL-FR-LCSN, PIR-FR-HRXN, PPT-FR-DDSX: the plan tools are attached
// to a turn of a Map discussion only while the delivery source is a GitHub
// Project with a valid configuration, checked at the start of the turn. Other
// discussions never get them.
#[test]
fn the_plan_tools_need_a_map_turn_and_a_valid_github_source() {
    let f = PlanFixture::new();
    let mut expected: Vec<String> = PLAN_TOOLS.map(str::to_string).to_vec();
    expected.sort();
    assert_eq!(plan_tool_names(&f, &f.origin()), expected);

    for other in [
        ConversationOrigin::stub_artifact("t", "spec.md", false),
        ConversationOrigin::stub_draft("t", "d", false),
        ConversationOrigin::stub_note("t", "n"),
    ] {
        assert!(plan_tool_names(&f, &other).is_empty(), "{:?}", other.kind());
    }
    assert!(plan_tools_wanted(&f.origin(), true));
    assert!(!plan_tools_wanted(&f.origin(), false));
    assert!(!plan_tools_wanted(&ConversationOrigin::stub_note("t", "n"), true));

    // A configuration that is not valid.
    f.h.fake.lock().status_field = None;
    let _ = crate::delivery::github::load_plan_impl(&f.app(), true);
    assert!(!crate::agent_conversations::tool_wiring::map_plan_tools_enabled(&f.app()));
    assert!(plan_tool_names(&f, &f.origin()).is_empty());
}

// MPR-FR-ZCAM: a source that is a Flow file, and a project with no GitHub
// Project selected, attach no plan tool.
#[test]
fn another_source_attaches_no_plan_tool() {
    let f = PlanFixture::new();
    f.h.write("plans/a.flow", "");
    f.h.save_delivery_settings(&crate::delivery::settings::DeliverySettings {
        source: crate::delivery::settings::DeliverySourceChoice::File {
            flow_id: "plans/a.flow".into(),
        },
        ..crate::delivery::settings::DeliverySettings::default()
    });
    assert!(!crate::agent_conversations::tool_wiring::map_plan_tools_enabled(&f.app()));
    assert!(plan_tool_names(&f, &f.origin()).is_empty());

    let g = PlanFixture::new();
    g.h.select_project(None);
    assert!(!crate::agent_conversations::tool_wiring::map_plan_tools_enabled(&g.app()));
    assert!(plan_tool_names(&g, &g.origin()).is_empty());
}

// CVL-FR-15, PPT-FR-PRPX, CVL-FR-HNGB: `propose_plan_changes` ends the turn on a
// proposal, the three read tools end nothing, and an advisor carries none of the
// plan tools that end a turn.
#[test]
fn the_plan_proposal_ends_the_turn_and_advisors_lack_it() {
    assert!(crate::agent_conversations::turn_body::ends_turn(
        crate::tools::propose_plan_changes::NAME
    ));
    for read in [
        crate::tools::plan_issue_read::LIST_NAME,
        crate::tools::plan_issue_read::SEARCH_NAME,
        crate::tools::plan_issue_read::READ_NAME,
    ] {
        assert!(!crate::agent_conversations::turn_body::ends_turn(read));
    }
    assert!(crate::agent_conversations::groups::ADVISOR_WITHHELD_TOOLS
        .contains(&crate::tools::propose_plan_changes::NAME));
}

// MPR-FR-NWEZ, MPR-FR-QIJF, CTA-FR-YWSU: with no seam of a test, a re-prepare
// request dispatches one real turn of the agent that proposed. It is a turn of
// the Map discussion, and the Synthesis comment of the request is its trigger
// and the current comment of its input.
#[test]
fn a_reprepare_request_dispatches_a_map_turn_on_its_comment() {
    let h = Harness::new(vec![Ok("I read the issues again.".into())]);
    h.mount();
    let root = h.root().path().to_path_buf();
    let access = h.app.state::<crate::fs::FsAccessState>().get().expect("an instance");
    let project = h.app.state::<ProjectState>();
    project.set_root_with_access(root.clone(), Some(access));
    project.set_anchor(PROJECT_KEY.into());
    project.set_store(root.clone());
    h.create_agent("arch", "");
    let thread = open_map(&h.root(), None, "Plan the next step");
    let app = h.app.handle().clone();
    let draft = MapPlanRow {
        key: "d".into(),
        fields: RowFields::NewDraft(NewDraftFields { name: "Relay".into(), body: "x".into() }),
        state: RowState::Waiting,
        blocked_by: None,
        error: None,
        result: None,
    };
    let comment = ProposalComment {
        body: "Rows. <spoken>Rows.</spoken>".into(),
        author: agent("a1", "arch"),
        spoken_original: None,
    };
    let proposal =
        crate::map_plan::record_map_plan_proposal(&app, &thread.id, vec![draft], comment).unwrap();
    // DRS-FR-39: a project settings file that cannot be read fails the draft.
    let settings = root.join(".synthesis/project.toml");
    std::fs::create_dir_all(settings.parent().unwrap()).unwrap();
    std::fs::write(&settings, "this is = not [ toml").unwrap();
    let decided = crate::map_plan::accept_row(&app, &thread.id, &proposal.id, "d").unwrap();
    std::fs::remove_file(&settings).unwrap();
    assert_eq!(decided.rows[0].state, RowState::Failed);
    assert!(h.turns().all_turn_ids().is_empty(), "a failed draft asks nothing on its own");

    let asked = crate::map_plan::request_reprepare(&app, &thread.id, &proposal.id).unwrap();
    assert!(asked.reprepare_requested_at.is_some());
    // The turn can append its reply at any time from now, so the comment is
    // found by its author, not by its place in the thread.
    let synthesis: Vec<_> = thread_of(&h.root(), &thread.id)
        .comments
        .into_iter()
        .filter(|c| c.author == Participant::synthesis())
        .collect();
    assert_eq!(synthesis.len(), 1, "one Synthesis comment is appended");
    let request_comment = synthesis[0].clone();
    // The wait is on the turn threads, not on time. An emit that Tauri queued
    // is delivered by the next emit, so the flush makes the terminal event seen.
    h.settle();
    crate::delivery::github::tests::wait_until("the terminal event", || {
        let _ = tauri::Emitter::emit(&h.app, FLUSH_QUEUED_EMITS, ());
        !h.terminal.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
    });
    let ended = h.terminal.lock().unwrap_or_else(|e| e.into_inner()).clone();
    assert_eq!(ended.len(), 1, "one turn is dispatched");
    let turn = ended[0].clone();
    assert_eq!(turn.nickname, "arch");
    assert_eq!(turn.origin.kind(), OriginKind::MapDiscussion);
    assert_eq!(turn.origin.discussion_id(), thread.id);
    assert_eq!(turn.trigger_comment_id.as_deref(), Some(request_comment.id.as_str()));
    let (request, _) = h.seam.requests().into_iter().next().expect("one request");
    let current =
        request.input.iter().find(|s| s.tag == TAG_CURRENT_COMMENT).expect("a current comment");
    assert!(current.body.contains(&request_comment.body), "{}", current.body);
}

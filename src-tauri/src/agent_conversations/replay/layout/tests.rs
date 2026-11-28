//! The selection and the layout of a replay (`ERP-exchange-replay.md`).

use super::*;
use crate::agent_conversations::replay::rounds::derived_call_id;
use crate::comments::{DiscussionTarget, Participant, Stamp, ThreadRef};
use rig::completion::Message;
use std::collections::BTreeSet;

const ARTIFACT: &str = "a.md";
const MATERIAL: &str = "MATERIAL-TEXT";

struct Fixture {
    _dir: tempfile::TempDir,
    root: crate::fs::RootFs,
    thread_id: String,
    minute: std::cell::Cell<u32>,
}

fn agent(id: &str) -> Participant {
    Participant::Agent { agent_id: id.into(), handle: id.into(), model: None, title: None }
}

fn human() -> Participant {
    Participant::Human { login: "ada".into(), display_name: None, email: None }
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&path);
        std::fs::write(path.join(ARTIFACT), MATERIAL).expect("artifact");
        std::fs::write(path.join("notes.md"), "NOTES").expect("notes");
        let thread = crate::comments::open_discussion_in(
            &root,
            &root,
            &DiscussionTarget::Artifact { artifact_id: ARTIFACT.into() },
            None,
            "T1 @arch first?".into(),
            Vec::new(),
            Stamp { by: &human(), at: "2026-01-01T00:00:00Z" },
        )
        .expect("discussion");
        Fixture { _dir: dir, root, thread_id: thread.id, minute: std::cell::Cell::new(1) }
    }

    fn roots(&self) -> Roots<'_> {
        Roots::same(&self.root)
    }

    /// Add a comment by `by`, and return its id.
    fn add(&self, by: &Participant, body: &str) -> String {
        let minute = self.minute.get();
        self.minute.set(minute + 1);
        crate::comments::add_comment_to(
            &self.root,
            ThreadRef::artifact_discussion(ARTIFACT),
            &self.thread_id,
            body.into(),
            Vec::new(),
            Vec::new(),
            Stamp { by, at: &format!("2026-01-01T{:02}:{:02}:00Z", minute / 60, minute % 60) },
        )
        .expect("comment")
        .comments
        .last()
        .expect("the comment")
        .id
        .clone()
    }

    fn thread(&self) -> comments::Discussion {
        crate::comments::locate_discussion_of(
            &self.root,
            &DiscussionTarget::Artifact { artifact_id: ARTIFACT.into() },
            &self.thread_id,
        )
        .expect("thread")
    }

    fn first_id(&self) -> String {
        self.thread().comments[0].id.clone()
    }

    /// The input a turn for `trigger` is built with.
    fn input(&self, trigger: &str) -> Vec<InputSection> {
        build_input(self.roots(), &ConversationOrigin::of(&self.thread()), trigger)
    }
}

fn read_round(call_id: &str, text: &str) -> Vec<Message> {
    let call = rig::completion::message::ToolCall::from_wire(
        call_id,
        rig::completion::message::ToolFunction::new(
            rig::completion::message::ToolName::new("read_file").expect("name"),
            serde_json::json!({ "path": "notes.md" }),
        ),
    );
    vec![
        Message::Assistant(rig::completion::message::AssistantMessage::new(vec![
            rig::completion::message::AssistantContent::ToolCall(call.clone()),
        ])),
        Message::tool_result(call.id.clone(), call.function.name.clone(), text),
    ]
}

fn entry(trigger: &str, answer: Option<&str>, rounds: Vec<Message>) -> ReplayEntry {
    ReplayEntry {
        seq: 0,
        trigger_comment_id: trigger.into(),
        answer_comment_id: answer.map(str::to_string),
        provider: "anthropic".into(),
        model_id: "m".into(),
        tool_names: BTreeSet::new(),
        rounds,
        state: Vec::new(),
        stamps: Vec::new(),
        last_used: 0,
    }
}

fn inputs<'a>(
    f: &'a Fixture,
    thread: &'a comments::Discussion,
    trigger: &'a str,
) -> ReplayInputs<'a> {
    ReplayInputs {
        roots: f.roots(),
        thread,
        trigger_comment_id: trigger,
        key: ReplayKey { discussion_id: thread.id.clone(), agent_id: "arch".into() },
        offered: ["read_file".to_string()].into_iter().collect(),
        provider: "anthropic",
        model_id: "m",
        window: 1_000_000,
        history_bound: 40_000,
        instruction_tokens: 100,
        update_memory: true,
    }
}

fn memory_key(thread: &comments::Discussion) -> ReplayKey {
    ReplayKey { discussion_id: thread.id.clone(), agent_id: "arch".into() }
}

fn tags(sections: &[InputSection]) -> Vec<&str> {
    sections.iter().map(|s| s.tag.as_str()).collect()
}

fn section<'a>(sections: &'a [InputSection], tag: &str) -> &'a InputSection {
    sections.iter().find(|s| s.tag == tag).expect("section")
}

/// T1 by the human, A1 by arch, T2 by the human.
fn one_answered(f: &Fixture) -> (String, String, String) {
    let t1 = f.first_id();
    let a1 = f.add(&agent("arch"), "A1-ANSWER");
    let t2 = f.add(&human(), "T2 @arch and now?");
    (t1, a1, t2)
}

// ERP-FR-SBBE: with no entry, the plan is empty and the input is unchanged.
#[test]
fn no_entry_leaves_the_input() {
    let f = Fixture::new();
    let (_, _, t2) = one_answered(&f);
    let thread = f.thread();
    let mut input = f.input(&t2);
    let before = input.clone();
    let plan = plan_replay(&ReplayMemory::default(), &inputs(&f, &thread, &t2), &mut input);
    assert!(plan.blocks.is_empty());
    assert_eq!(input, before);
}

// ERP-FR-OPRO, ERP-FR-ISTB, ERP-FR-LKUP, ERP-FR-FMMT, ERP-FR-BBXA: one entry
// gives one block with the material, the history before T1, T1 and its
// rounds, and the answer; the input holds T2 and no A1.
#[test]
fn one_entry_gives_one_block_and_a_short_input() {
    let f = Fixture::new();
    let (t1, a1, t2) = one_answered(&f);
    let thread = f.thread();
    let memory = ReplayMemory::default();
    memory.capture(memory_key(&thread), entry(&t1, Some(&a1), read_round("c1", "NOTES")));
    let mut input = f.input(&t2);
    let plan = plan_replay(&memory, &inputs(&f, &thread, &t2), &mut input);
    assert_eq!(plan.blocks.len(), 1);
    let block = &plan.blocks[0];
    assert_eq!(tags(&block.opening), [TAG_ARTIFACT, TAG_DISCUSSION_HISTORY, TAG_CURRENT_COMMENT]);
    assert_eq!(plan.head, 1);
    assert!(section(&block.opening, TAG_ARTIFACT).body.contains(MATERIAL));
    assert!(section(&block.opening, TAG_CURRENT_COMMENT).body.contains("first?"));
    assert!(!section(&block.opening, TAG_DISCUSSION_HISTORY).body.contains("A1"));
    assert_eq!(block.answer.as_deref(), Some("A1-ANSWER"));
    assert_eq!(block.rounds.len(), 2);
    assert_eq!(tags(&input), [TAG_DISCUSSION_HISTORY, TAG_CURRENT_COMMENT]);
    let history = &section(&input, TAG_DISCUSSION_HISTORY).body;
    assert!(!history.contains("A1-ANSWER") && !history.contains("first?"), "{history}");
    assert!(section(&input, TAG_CURRENT_COMMENT).body.contains("and now?"));
    assert!(plan.tokens > 0);
}

// ERP-FR-IVGL: the opening of an entry is the bytes of the input that its
// turn was built with, where nothing changed.
#[test]
fn the_first_opening_is_the_input_of_its_turn() {
    let f = Fixture::new();
    let t1 = f.first_id();
    let turn_input = f.input(&t1);
    let a1 = f.add(&agent("arch"), "A1-ANSWER");
    let t2 = f.add(&human(), "T2 @arch and now?");
    let thread = f.thread();
    let memory = ReplayMemory::default();
    memory.capture(memory_key(&thread), entry(&t1, Some(&a1), Vec::new()));
    let mut input = f.input(&t2);
    let plan = plan_replay(&memory, &inputs(&f, &thread, &t2), &mut input);
    assert_eq!(render_sections(&plan.blocks[0].opening), render_sections(&turn_input));
}

// ERP-FR-CMMA, ERP-FR-VMRY: two entries are in trigger order; the second
// opening holds no material and only the comments between the triggers,
// without the first answer.
#[test]
fn two_entries_are_in_trigger_order() {
    let f = Fixture::new();
    let t1 = f.first_id();
    let a1 = f.add(&agent("arch"), "A1-ANSWER");
    f.add(&human(), "BETWEEN note");
    let t2 = f.add(&human(), "T2 @arch and now?");
    let a2 = f.add(&agent("arch"), "A2-ANSWER");
    let t3 = f.add(&human(), "T3 @arch last?");
    let thread = f.thread();
    let memory = ReplayMemory::default();
    memory.capture(memory_key(&thread), entry(&t2, Some(&a2), read_round("c2", "TWO")));
    memory.capture(memory_key(&thread), entry(&t1, Some(&a1), read_round("c1", "ONE")));
    let mut input = f.input(&t3);
    let plan = plan_replay(&memory, &inputs(&f, &thread, &t3), &mut input);
    assert_eq!(plan.blocks.len(), 2);
    assert_eq!(plan.blocks[0].answer.as_deref(), Some("A1-ANSWER"));
    let second = &plan.blocks[1];
    assert_eq!(tags(&second.opening), [TAG_DISCUSSION_HISTORY, TAG_CURRENT_COMMENT]);
    let history = &section(&second.opening, TAG_DISCUSSION_HISTORY).body;
    assert!(history.contains("BETWEEN note") && !history.contains("A1-ANSWER"), "{history}");
    assert!(section(&second.opening, TAG_CURRENT_COMMENT).body.contains("and now?"));
    assert_eq!(second.answer.as_deref(), Some("A2-ANSWER"));
}

// ERP-FR-CMMA, ERP-FR-BXWA: an entry of another agent, and an entry whose
// answer comes after the current trigger, are not selected; the second stays
// in memory.
#[test]
fn only_the_agents_earlier_entries_are_selected() {
    let f = Fixture::new();
    let t1 = f.first_id();
    let t2 = f.add(&human(), "T2 @arch and now?");
    let late = f.add(&agent("arch"), "LATE-ANSWER");
    let thread = f.thread();
    let memory = ReplayMemory::default();
    memory.capture(memory_key(&thread), entry(&t1, Some(&late), Vec::new()));
    let other = ReplayKey { agent_id: "eve".into(), ..memory_key(&thread) };
    memory.capture(other, entry(&t1, None, Vec::new()));
    let mut input = f.input(&t2);
    let plan = plan_replay(&memory, &inputs(&f, &thread, &t2), &mut input);
    assert!(plan.blocks.is_empty());
    assert_eq!(memory.entries_of(&memory_key(&thread)).len(), 1, "kept");
}

// ERP-FR-BBXA: an entry whose answer is no longer in the discussion is
// dropped, and with `update_memory` false it stays.
#[test]
fn an_entry_whose_answer_is_gone_is_dropped() {
    let f = Fixture::new();
    let (t1, _, t2) = one_answered(&f);
    let thread = f.thread();
    let memory = ReplayMemory::default();
    memory.capture(memory_key(&thread), entry(&t1, Some("gone"), Vec::new()));
    let mut input = f.input(&t2);
    let dry = ReplayInputs { update_memory: false, ..inputs(&f, &thread, &t2) };
    assert!(plan_replay(&memory, &dry, &mut input).blocks.is_empty());
    assert_eq!(memory.entries_of(&memory_key(&thread)).len(), 1, "a dry run changes nothing");
    let mut input = f.input(&t2);
    assert!(plan_replay(&memory, &inputs(&f, &thread, &t2), &mut input).blocks.is_empty());
    assert!(memory.entries_of(&memory_key(&thread)).is_empty(), "dropped");
}

// ERP-FR-FEAD, ERP-FR-FMMT: an entry with no answer comment has no answer
// message, and its question comment stays in the history that follows it.
#[test]
fn a_contribution_entry_keeps_its_comment_in_the_history() {
    let f = Fixture::new();
    let t1 = f.first_id();
    f.add(&agent("arch"), "QUESTION-COMMENT");
    let t2 = f.add(&human(), "T2 the answer");
    let thread = f.thread();
    let memory = ReplayMemory::default();
    memory.capture(memory_key(&thread), entry(&t1, None, read_round("c1", "NOTES")));
    let mut input = f.input(&t2);
    let plan = plan_replay(&memory, &inputs(&f, &thread, &t2), &mut input);
    assert_eq!(plan.blocks.len(), 1);
    assert!(plan.blocks[0].answer.is_none());
    assert!(section(&input, TAG_DISCUSSION_HISTORY).body.contains("QUESTION-COMMENT"));
}

// ERP-FR-RVYX: a result whose file changed becomes the outdated line, which
// is written into the entry; a dry run counts it and writes nothing.
#[test]
fn a_changed_stamp_gives_the_outdated_line() {
    let f = Fixture::new();
    let (t1, a1, t2) = one_answered(&f);
    let thread = f.thread();
    let memory = ReplayMemory::default();
    let mut captured = entry(&t1, Some(&a1), read_round("c1", "NOTES"));
    let call = super::super::sources::source_of(&match &captured.rounds[0] {
        Message::Assistant(a) => match &a.content[0] {
            rig::completion::message::AssistantContent::ToolCall(c) => c.clone(),
            _ => panic!("call"),
        },
        _ => panic!("assistant"),
    });
    let source = call.expect("a source");
    captured.stamps.push(super::super::sources::SourceStamp {
        call_id: "c1".into(),
        sha256: source.digest(f.roots()).expect("digest"),
        source,
    });
    memory.capture(memory_key(&thread), captured);
    std::fs::write(f.root.path().join("notes.md"), "CHANGED").expect("edit");
    let mut input = f.input(&t2);
    let dry = ReplayInputs { update_memory: false, ..inputs(&f, &thread, &t2) };
    let plan = plan_replay(&memory, &dry, &mut input);
    assert_eq!(plan.outdated, 1);
    assert_eq!(memory.entries_of(&memory_key(&thread))[0].stamps.len(), 1, "unchanged");
    let mut input = f.input(&t2);
    let plan = plan_replay(&memory, &inputs(&f, &thread, &t2), &mut input);
    assert_eq!(plan.outdated, 1);
    let text = compaction::message_text(&plan.blocks[0].rounds[1]);
    assert!(text.contains("[Outdated: notes.md changed"), "{text}");
    let stored = &memory.entries_of(&memory_key(&thread))[0];
    assert!(stored.stamps.is_empty(), "the stamp is removed");
    assert!(compaction::message_text(&stored.rounds[1]).contains("[Outdated:"));
}

// ERP-FR-UUAD: the room is the lesser of 15% of the window and 60% of the
// window less the estimate.
#[test]
fn the_room_follows_the_window_and_the_estimate() {
    assert_eq!(replay_room(100_000, 0), 15_000);
    assert_eq!(replay_room(100_000, 50_000), 10_000);
    assert_eq!(replay_room(100_000, 70_000), 0);
    assert_eq!(replay_room(0, 0), 0);
}

// ERP-FR-GVHU, ERP-FR-GBOK: above the room, the results of the oldest entry
// are omitted first; where that is not enough, the oldest entries are
// demoted and their answers stand in the history.
#[test]
fn entries_above_the_room_are_omitted_then_demoted() {
    let f = Fixture::new();
    let (t1, a1, t2) = one_answered(&f);
    let a2 = f.add(&agent("arch"), "A2-ANSWER");
    let t3 = f.add(&human(), "T3 @arch last?");
    let thread = f.thread();
    let memory = ReplayMemory::default();
    let big = "x".repeat(40_000);
    let medium = "m".repeat(16_000);
    memory.capture(memory_key(&thread), entry(&t1, Some(&a1), read_round("c1", &big)));
    memory.capture(memory_key(&thread), entry(&t2, Some(&a2), read_round("c2", &medium)));
    // A window whose room is 7,500 tokens: the two results, 14,000 tokens,
    // fit three quarters of it once the oldest alone is omitted.
    let small = ReplayInputs { window: 50_000, ..inputs(&f, &thread, &t3) };
    let mut input = f.input(&t3);
    let plan = plan_replay(&memory, &small, &mut input);
    assert_eq!(plan.blocks.len(), 2);
    assert_eq!(plan.omitted, 1, "the oldest result is omitted");
    assert!(compaction::message_text(&plan.blocks[0].rounds[1]).contains(RESULT_OMITTED));
    assert!(
        compaction::message_text(&plan.blocks[1].rounds[1]).contains(&medium),
        "the newest is whole"
    );
    // Again: nothing more to omit, the same blocks.
    let mut again = f.input(&t3);
    let second = plan_replay(&memory, &small, &mut again);
    assert_eq!(second.omitted, 0);
    assert_eq!(second.blocks, plan.blocks);
    // A room of nothing demotes every entry; the input keeps its material.
    let none = ReplayInputs { window: 100, ..inputs(&f, &thread, &t3) };
    let mut input = f.input(&t3);
    let plan = plan_replay(&memory, &none, &mut input);
    assert!(plan.blocks.is_empty());
    assert_eq!(plan.demoted, 2);
    assert!(memory.entries_of(&memory_key(&thread)).is_empty());
    assert_eq!(input[0].tag, TAG_ARTIFACT);
    assert!(section(&input, TAG_DISCUSSION_HISTORY).body.contains("A1-ANSWER"));
}

const RESULT_OMITTED: &str = "[Result omitted to save room.";

// ERP-FR-GAXX: a `custom` turn and an id that two entries hold get derived
// ids; another turn keeps the stored ids.
#[test]
fn custom_turns_and_repeated_ids_get_derived_ids() {
    let f = Fixture::new();
    let (t1, a1, t2) = one_answered(&f);
    let a2 = f.add(&agent("arch"), "A2-ANSWER");
    let t3 = f.add(&human(), "T3 @arch last?");
    let thread = f.thread();
    let ids = |plan: &ReplayPlan| -> Vec<Vec<String>> {
        plan.blocks.iter().map(|b| call_ids(&b.rounds)).collect()
    };
    let memory = ReplayMemory::default();
    memory.capture(memory_key(&thread), entry(&t1, Some(&a1), read_round("toolu_1", "ONE")));
    memory.capture(memory_key(&thread), entry(&t2, Some(&a2), read_round("toolu_2", "TWO")));
    let mut input = f.input(&t3);
    let stored = plan_replay(&memory, &inputs(&f, &thread, &t3), &mut input);
    assert_eq!(ids(&stored), [vec!["toolu_1".to_string()], vec!["toolu_2".to_string()]]);
    let custom = ReplayInputs { provider: "custom", ..inputs(&f, &thread, &t3) };
    let mut entries = memory.entries_of(&memory_key(&thread));
    entries.iter_mut().for_each(|e| e.provider = "custom".into());
    entries.iter().for_each(|e| memory.replace(&memory_key(&thread), e));
    let mut input = f.input(&t3);
    let renamed = plan_replay(&memory, &custom, &mut input);
    let seqs: Vec<u64> = memory.entries_of(&memory_key(&thread)).iter().map(|e| e.seq).collect();
    assert_eq!(
        ids(&renamed),
        seqs.iter().map(|seq| vec![derived_call_id(*seq, 0)]).collect::<Vec<_>>()
    );

    let repeated = ReplayMemory::default();
    repeated.capture(memory_key(&thread), entry(&t1, Some(&a1), read_round("call_0", "ONE")));
    repeated.capture(memory_key(&thread), entry(&t2, Some(&a2), read_round("call_0", "TWO")));
    let mut input = f.input(&t3);
    let plan = plan_replay(&repeated, &inputs(&f, &thread, &t3), &mut input);
    let seqs: Vec<u64> = repeated.entries_of(&memory_key(&thread)).iter().map(|e| e.seq).collect();
    assert_eq!(
        ids(&plan),
        seqs.iter().map(|seq| vec![derived_call_id(*seq, 0)]).collect::<Vec<_>>(),
        "both entries are renamed"
    );
}

// ERP-FR-QJEM: where the newest comment alone does not fit the bound, the
// history stays as the builder made it and no entry is selected.
#[test]
fn a_newest_comment_above_the_bound_selects_nothing() {
    let f = Fixture::new();
    let t1 = f.first_id();
    let a1 = f.add(&agent("arch"), "A1-ANSWER");
    f.add(&human(), &"y".repeat(2_000));
    let t2 = f.add(&human(), "T2 @arch and now?");
    let thread = f.thread();
    let memory = ReplayMemory::default();
    memory.capture(memory_key(&thread), entry(&t1, Some(&a1), Vec::new()));
    let tight = ReplayInputs { history_bound: 500, ..inputs(&f, &thread, &t2) };
    let mut input = f.input(&t2);
    let before = input.clone();
    let plan = plan_replay(&memory, &tight, &mut input);
    assert!(plan.blocks.is_empty());
    assert_eq!(input, before);
}

// ERP-FR-IICX: over the bound, the start keeps the rest within three quarters
// of the bound, the memory keeps it, and a later turn keeps the same start
// while the rest fits.
#[test]
fn the_history_start_is_kept_while_the_rest_fits() {
    let f = Fixture::new();
    for i in 0..8 {
        f.add(&human(), &format!("C{i} {}", "z".repeat(90)));
    }
    let t2 = f.add(&human(), "T2 @arch now?");
    let thread = f.thread();
    let memory = ReplayMemory::default();
    let bounded = ReplayInputs { history_bound: 800, ..inputs(&f, &thread, &t2) };
    let mut input = f.input(&t2);
    plan_replay(&memory, &bounded, &mut input);
    let start = memory.history_start(&memory_key(&thread)).expect("a start");
    let history = section(&input, TAG_DISCUSSION_HISTORY);
    assert!(history.truncated);
    assert!(history.body.starts_with("[Earlier comments omitted:"), "{}", history.body);
    assert!(history.body.chars().count() <= 600, "three quarters of the bound");

    let t3 = f.add(&human(), "T3 ok");
    let thread = f.thread();
    let later = ReplayInputs { history_bound: 800, ..inputs(&f, &thread, &t3) };
    let mut input = f.input(&t3);
    plan_replay(&memory, &later, &mut input);
    assert_eq!(memory.history_start(&memory_key(&thread)), Some(start), "the same start");
}

// ERP-FR-FEAD: the answer comment is the newest comment of that agent.
#[test]
fn the_answer_comment_is_the_newest_of_the_agent() {
    let f = Fixture::new();
    f.add(&agent("arch"), "older");
    let newest = f.add(&agent("arch"), "newer");
    let eve = f.add(&agent("eve"), "other agent");
    f.add(&human(), "a human");
    let thread = f.thread();
    assert_eq!(super::super::capture::answer_comment_of(&thread, "arch"), Some(newest));
    assert_eq!(super::super::capture::answer_comment_of(&thread, "eve"), Some(eve));
    assert_eq!(super::super::capture::answer_comment_of(&thread, "bob"), None);
}

/// A round of one call of `name` and its result.
fn call_round(call_id: &str, name: &str) -> Vec<Message> {
    let call = rig::completion::message::ToolCall::from_wire(
        call_id,
        rig::completion::message::ToolFunction::new(
            rig::completion::message::ToolName::new(name).expect("name"),
            serde_json::json!({ "rationale": "why" }),
        ),
    );
    vec![
        Message::Assistant(rig::completion::message::AssistantMessage::new(vec![
            rig::completion::message::AssistantContent::ToolCall(call.clone()),
        ])),
        Message::tool_result(call.id.clone(), call.function.name.clone(), "RECORDED"),
    ]
}

/// The names of the calls of the replayed rounds of `plan`.
fn replayed_calls(plan: &ReplayPlan) -> Vec<String> {
    plan.blocks
        .iter()
        .flat_map(|b| b.rounds.iter())
        .flat_map(|m| match m {
            Message::Assistant(a) => a
                .content
                .iter()
                .filter_map(|c| match c {
                    rig::completion::message::AssistantContent::ToolCall(call) => {
                        Some(call.function.name.to_string())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>(),
            _ => Vec::new(),
        })
        .collect()
}

/// The plan of a turn on a discussion of `target` in `root`, where arch
/// answered T1 with one stored call of `propose_change_edits`, and both
/// `propose_change_edits` and `propose_openspec_change_edits` are offered.
fn plan_on(root: &crate::fs::RootFs, target: &DiscussionTarget) -> ReplayPlan {
    let thread = crate::comments::open_discussion_unchecked(
        root,
        target,
        None,
        "T1 @arch first?".into(),
        Vec::new(),
        &human(),
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");
    let add = |by: &Participant, body: &str, at: &str| -> comments::Discussion {
        crate::comments::add_comment_to(
            root,
            thread.log_ref(),
            &thread.id,
            body.into(),
            Vec::new(),
            Vec::new(),
            Stamp { by, at },
        )
        .expect("comment")
    };
    add(&agent("arch"), "A1-ANSWER", "2026-01-01T00:01:00Z");
    let thread = add(&human(), "T2 @arch and now?", "2026-01-01T00:02:00Z");
    let (t1, a1, t2) = (
        thread.comments[0].id.clone(),
        thread.comments[1].id.clone(),
        thread.comments[2].id.clone(),
    );
    let memory = ReplayMemory::default();
    memory.capture(
        memory_key(&thread),
        entry(&t1, Some(&a1), call_round("c1", crate::tools::propose_draft_changes::NAME)),
    );
    let offered =
        [crate::tools::propose_draft_changes::NAME, crate::tools::propose_change_edits::NAME]
            .into_iter()
            .map(str::to_string)
            .collect();
    let inputs = ReplayInputs {
        roots: Roots::same(root),
        thread: &thread,
        trigger_comment_id: &t2,
        key: memory_key(&thread),
        offered,
        provider: "anthropic",
        model_id: "m",
        window: 1_000_000,
        history_bound: 40_000,
        instruction_tokens: 100,
        update_memory: true,
    };
    let mut input = build_input(Roots::same(root), &ConversationOrigin::of(&thread), &t2);
    plan_replay(&memory, &inputs, &mut input)
}

// PVC-FR-LGYT, PVC-FR-PXOE: on an OpenSpec change discussion, a stored call of
// `propose_change_edits` is an old call of the OpenSpec tool, so the replay
// gives it the current name. On a change of the Changes panel, the same entry
// is a call of the change tool and keeps its name.
#[test]
fn the_target_kind_tells_the_current_name_of_a_replayed_call() {
    assert_eq!(crate::tools::propose_draft_changes::NAME, "propose_change_edits");
    assert_eq!(crate::tools::propose_change_edits::NAME, "propose_openspec_change_edits");

    let dir = tempfile::TempDir::new().expect("tempdir");
    let path = crate::changes::canonicalize_lenient(dir.path());
    let base = path.join("openspec/changes/add-auth");
    std::fs::create_dir_all(&base).expect("dirs");
    std::fs::write(base.join("proposal.md"), "## Why\n\nUsers need to log in.\n")
        .expect("proposal");
    let root = crate::fs::RootFs::for_root(&path);
    crate::change_proposals::set_test_stream("layout-stream", "Auth stream", root.clone());
    let on_change = plan_on(
        &root,
        &DiscussionTarget::Change { stream_id: "layout-stream".into(), change: "add-auth".into() },
    );
    assert_eq!(on_change.blocks.len(), 1, "the entry is replayed");
    assert_eq!(replayed_calls(&on_change), vec!["propose_openspec_change_edits"]);

    let created =
        crate::drafts::create_draft_at_root(&root, Some("layout-draft")).expect("a draft");
    let on_draft = plan_on(&root, &DiscussionTarget::Draft { draft_id: created.draft.id });
    assert_eq!(on_draft.blocks.len(), 1, "the entry is replayed");
    assert_eq!(replayed_calls(&on_draft), vec!["propose_change_edits"]);
}

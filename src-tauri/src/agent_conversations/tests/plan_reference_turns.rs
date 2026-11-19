//! The plan references of a Map turn, end to end through a dispatch
//! (AGC-FR-UOPJ, AGC-FR-EFCL; `MPR-map-plan-proposals.md` MPR-FR-NXDX,
//! MPR-FR-HRUK, MPR-FR-CHRV, MPR-FR-EHCG, MPR-FR-VGCX, MPR-FR-AGTT).
//!
//! Each test waits for the terminal state of a turn before it changes a file or
//! dispatches the next turn. The record is written before the terminal event,
//! so the order of the steps does not depend on time.

use super::*;
use crate::comments::{Opening, Stamp, ThreadRef};
use crate::delivery::settings::{DeliverySettings, DeliverySourceChoice};
use crate::map_plan::read_fingerprints;

fn write(h: &Harness, rel: &str, body: &str) {
    let path = h.root.path().join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// A Flow file at `plans/a.flow` with `references`, selected as the delivery.
fn file_source(h: &Harness, references: &[&str]) {
    let refs = serde_json::to_string(references).unwrap();
    write(
        h,
        "plans/a.flow",
        &format!(r#"{{"version":3,"references":{refs},"items":[],"edges":[]}}"#),
    );
    crate::project_settings::save_delivery_settings_to(
        &h.root(),
        &DeliverySettings {
            source: DeliverySourceChoice::File { flow_id: "plans/a.flow".into() },
            ..DeliverySettings::default()
        },
    )
    .unwrap();
}

fn open_map(h: &Harness) -> Discussion {
    h.mount();
    h.create_agent("arch", "");
    crate::comments::open_map_discussion_in(
        &h.root(),
        None,
        Opening { body: "@arch plan it".into(), attachments: Vec::new(), spoken_original: None },
        Stamp { by: &human("ada"), at: "2026-03-01T10:00:00Z" },
    )
    .unwrap()
    .discussion
}

/// Add a human comment to the Map thread and run one turn of `arch` on it.
fn turn(h: &Harness, thread: &Discussion, second: u32) -> AgentTurn {
    let id = crate::comments::add_comment_reporting_id(
        &h.root(),
        ThreadRef::map_discussion(),
        &thread.id,
        "@arch again".into(),
        Vec::new(),
        Vec::new(),
        None,
        Stamp { by: &human("ada"), at: &format!("2026-03-01T10:01:{second:02}Z") },
    )
    .unwrap()
    .1;
    let turn = h.dispatch("arch", ConversationOrigin::of(thread), &id).unwrap();
    wait_for_terminal(h, &turn.id)
}

fn first_turn(h: &Harness, thread: &Discussion) -> AgentTurn {
    let turn = h.dispatch("arch", ConversationOrigin::of(thread), &thread.comments[0].id).unwrap();
    wait_for_terminal(h, &turn.id)
}

/// The `plan_references` section of the request at `index`.
fn references_of(h: &Harness, index: usize) -> InputSection {
    let request = h.seam.requests().into_iter().nth(index).expect("a request").0;
    request.input.into_iter().find(|s| s.tag == TAG_PLAN_REFERENCES).expect("plan_references")
}

/// MPR-FR-EHCG: the change marks of request `index`, which are in its
/// `discussion_state` section.
fn marks_of(h: &Harness, index: usize) -> String {
    let request = h.seam.requests().into_iter().nth(index).expect("a request").0;
    request
        .input
        .into_iter()
        .find(|s| s.tag == TAG_DISCUSSION_STATE)
        .map(|s| s.body)
        .unwrap_or_default()
}

fn recorded(h: &Harness, thread: &Discussion) -> Vec<(String, Option<String>)> {
    read_fingerprints(&h.root(), &thread.id)
        .expect("a record")
        .references
        .into_iter()
        .map(|r| (r.id, r.fingerprint))
        .collect()
}

fn fp(text: &str) -> Option<String> {
    Some(crate::map_plan::fingerprint_of(text.as_bytes()))
}

// MPR-FR-HRUK, MPR-FR-SRBV, AGC-FR-UOPJ, MPR-FR-VGCX: a Map turn over a Flow
// source carries `plan_references` after `map_context`, with each reference
// of the Flow in order and its text, marked `first`. The turn that delivered
// records the fingerprint of each.
#[test]
fn a_turn_carries_the_references_of_the_flow_and_records_them() {
    let h = Harness::new(vec![Ok("Answer. <spoken>Answer.</spoken>".into())]);
    file_source(&h, &["docs/hld.md", "docs/api.md"]);
    write(&h, "docs/hld.md", "# HLD\nPayments use cards.\n");
    write(&h, "docs/api.md", "API v1");
    let thread = open_map(&h);
    assert!(read_fingerprints(&h.root(), &thread.id).is_none());
    assert_eq!(first_turn(&h, &thread).state, AgentTurnState::Delivered);

    let request = h.seam.requests().into_iter().next().unwrap().0;
    let tags: Vec<&str> = request.input.iter().map(|s| s.tag.as_str()).collect();
    assert_eq!(tags, MAP_INPUT_TAGS.to_vec());
    let body = references_of(&h, 0).body;
    let hld = body.find("path: docs/hld.md").expect(&body);
    let api = body.find("path: docs/api.md").expect(&body);
    assert!(hld < api, "{body}");
    assert!(body.contains("Payments use cards."), "{body}");
    assert!(body.contains("API v1"), "{body}");
    let marks = marks_of(&h, 0);
    assert_eq!(marks.matches("change: first").count(), 2, "{marks}");
    assert!(!body.contains("change: first"), "the marks are state: {body}");
    assert_eq!(
        recorded(&h, &thread),
        vec![
            ("docs/hld.md".into(), fp("# HLD\nPayments use cards.\n")),
            ("docs/api.md".into(), fp("API v1"))
        ]
    );
}

// MPR-FR-CHRV, MPR-FR-EHCG, MPR-FR-VGCX: an edit on disk between two turns
// reaches the next turn. The edited reference is `changed` and carries the new
// text and not the old one; the other is `unchanged`.
#[test]
fn an_edit_on_disk_reaches_the_next_turn_marked_changed() {
    let h = Harness::new(vec![
        Ok("One. <spoken>One.</spoken>".into()),
        Ok("Two. <spoken>Two.</spoken>".into()),
    ]);
    file_source(&h, &["docs/hld.md", "docs/api.md"]);
    write(&h, "docs/hld.md", "Old design.");
    write(&h, "docs/api.md", "API v1");
    let thread = open_map(&h);
    first_turn(&h, &thread);
    write(&h, "docs/hld.md", "New design.");
    assert_eq!(turn(&h, &thread, 1).state, AgentTurnState::Delivered);

    let body = references_of(&h, 1).body;
    assert!(body.contains("New design."), "{body}");
    assert!(!body.contains("Old design."), "{body}");
    let marks = marks_of(&h, 1);
    assert!(marks.contains("- path: docs/hld.md, change: changed"), "{marks}");
    assert!(marks.contains("- path: docs/api.md, change: unchanged"), "{marks}");
    assert_eq!(recorded(&h, &thread)[0].1, fp("New design."));
}

// MPR-FR-VGCX, AGC-FR-EFCL: a turn that fails changes no record, so the next
// turn still compares with the last turn that delivered.
#[test]
fn a_failed_turn_keeps_the_record() {
    let mut replies = vec![Ok("One. <spoken>One.</spoken>".into())];
    replies.extend(std::iter::repeat_n(Err(FAIL_UNREACHABLE), MAX_ATTEMPTS));
    replies.push(Ok("Three. <spoken>Three.</spoken>".into()));
    let h = Harness::new(replies);
    file_source(&h, &["docs/hld.md"]);
    write(&h, "docs/hld.md", "v1");
    let thread = open_map(&h);
    first_turn(&h, &thread);
    write(&h, "docs/hld.md", "v2");
    assert_eq!(turn(&h, &thread, 1).state, AgentTurnState::Failed);
    assert_eq!(recorded(&h, &thread), vec![("docs/hld.md".into(), fp("v1"))]);

    assert_eq!(turn(&h, &thread, 2).state, AgentTurnState::Delivered);
    let last = h.seam.requests().len() - 1;
    let body = references_of(&h, last).body;
    assert!(marks_of(&h, last).contains("change: changed"), "{body}");
    assert!(body.contains("v2"), "{body}");
    assert_eq!(recorded(&h, &thread), vec![("docs/hld.md".into(), fp("v2"))]);
}

// MPR-FR-HRUK, MPR-FR-EHCG: with no delivery source the section says in one
// line that there is no plan reference. The turn still records an empty set,
// so a reference attached later is `added`, and one dropped later is
// `removed`.
#[test]
fn references_added_and_removed_between_turns_are_marked() {
    let h = Harness::new(vec![
        Ok("One. <spoken>One.</spoken>".into()),
        Ok("Two. <spoken>Two.</spoken>".into()),
        Ok("Three. <spoken>Three.</spoken>".into()),
    ]);
    write(&h, "docs/hld.md", "Design.");
    write(&h, "docs/api.md", "API.");
    let thread = open_map(&h);
    first_turn(&h, &thread);
    assert_eq!(references_of(&h, 0).body, "The delivery plan has no plan reference.\n");
    assert_eq!(recorded(&h, &thread), Vec::new());

    file_source(&h, &["docs/hld.md"]);
    assert_eq!(turn(&h, &thread, 1).state, AgentTurnState::Delivered);
    assert!(marks_of(&h, 1).contains("change: added"));

    file_source(&h, &["docs/api.md"]);
    assert_eq!(turn(&h, &thread, 2).state, AgentTurnState::Delivered);
    let body = references_of(&h, 2).body;
    let marks = marks_of(&h, 2);
    assert!(marks.contains("- path: docs/api.md, change: added"), "{marks}");
    assert!(marks.contains("- path: docs/hld.md, change: removed"), "{marks}");
    assert!(!body.contains("Design."), "{body}");
}

// MPR-FR-AGTT: the assembly logs its counts, and no record carries the text,
// the name, or the path of a reference.
#[test]
fn the_assembly_logs_counts_and_no_content() {
    let h = Harness::new(vec![Ok("One. <spoken>One.</spoken>".into())]);
    file_source(&h, &["docs/secret-hld.md", "docs/missing.md"]);
    write(&h, "docs/secret-hld.md", "Very private design text.");
    let thread = open_map(&h);
    first_turn(&h, &thread);

    let records: Vec<_> = crate::logging::BUFFER
        .query(&LogFilter::default(), None, logging::BUFFER_CAPACITY)
        .expect("the buffer answers a default filter")
        .records
        .into_iter()
        .filter(|r| r.fields.get("discussionId") == Some(&serde_json::json!(thread.id)))
        .filter(|r| r.message == "map plan references for a turn")
        .collect();
    assert_eq!(records.len(), 1, "{records:?}");
    let field = |name: &str| records[0].fields.get(name).cloned();
    assert_eq!(field("references"), Some(serde_json::json!(2)));
    assert_eq!(field("first"), Some(serde_json::json!(2)));
    assert_eq!(field("unresolved"), Some(serde_json::json!(1)));
    assert_eq!(field("truncated"), Some(serde_json::json!(false)));
    let all = format!("{records:?}");
    for leak in ["secret-hld", "missing.md", "Very private"] {
        assert!(!all.contains(leak), "{leak} in {all}");
    }
}

// MPR-FR-HRUK: a GitHub source gives the references of the overlay, read from
// the store, and their text from the worktree.
#[test]
fn a_github_source_gives_the_references_of_the_overlay() {
    let f = crate::tools::tests::plan_fixture::PlanFixture::new();
    let roots = f.roots();
    let (_, resolved) = crate::delivery::source::resolve_local(&roots.worktree).unwrap();
    let crate::delivery::source::Resolved::Github { project_node_id } = resolved else {
        panic!("a github source")
    };
    let overlay = crate::delivery::machine_state::DeliveryOverlay {
        references: vec!["docs/hld.md".into()],
        ..Default::default()
    };
    crate::delivery::machine_state::write_delivery_overlay(
        &f.app(),
        &crate::logging::BUFFER,
        &roots.store,
        &project_node_id,
        &overlay,
    )
    .unwrap();
    let path = roots.worktree.path().join("docs/hld.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "GitHub plan design.").unwrap();

    let mut sections = vec![crate::agent_conversations::sections::section(
        TAG_MAP_CONTEXT,
        Vec::new(),
        "Focus: the whole Map.".into(),
    )];
    let assembled = crate::agent_conversations::plan_references::extend_plan_references(
        &f.app(),
        roots.borrowed(),
        &DocumentBudget::default_window(),
        &f.discussion_id,
        &mut sections,
    )
    .expect("a Map input");
    assert_eq!(sections[1].tag, TAG_PLAN_REFERENCES);
    assert!(sections[1].body.contains("path: docs/hld.md"), "{}", sections[1].body);
    assert!(sections[1].body.contains("GitHub plan design."), "{}", sections[1].body);
    assert_eq!(assembled.references.len(), 1);
    assert_eq!(assembled.references[0].fingerprint, fp("GitHub plan design."));
}

fn records_of(thread: &Discussion, message: &str) -> Vec<LogRecord> {
    let all: Vec<LogRecord> = [&crate::logging::BUFFER, &TEST_BUFFER]
        .into_iter()
        .flat_map(|buffer| {
            buffer
                .query(&LogFilter::default(), None, logging::BUFFER_CAPACITY)
                .expect("the buffer answers a default filter")
                .records
        })
        .collect();
    all.into_iter()
        .filter(|r| r.fields.get("discussionId") == Some(&serde_json::json!(thread.id)))
        .filter(|r| r.message == message)
        .collect()
}

// MPR-FR-AGTT, MPR-FR-EHCG: the counts of a turn name each mark, and the
// files that are not text, and no record of the feature carries a path, a
// name, or a text.
#[test]
fn the_counts_name_each_mark_and_no_record_carries_content() {
    let h = Harness::new(vec![
        Ok("One. <spoken>One.</spoken>".into()),
        Ok("Two. <spoken>Two.</spoken>".into()),
    ]);
    file_source(&h, &["docs/keep.md", "docs/edit.md", "docs/drop.md"]);
    write(&h, "docs/keep.md", "Kept text.");
    write(&h, "docs/edit.md", "Old text.");
    write(&h, "docs/drop.md", "Dropped text.");
    std::fs::write(h.root.path().join("docs/blob.bin"), [0xff_u8, 0xfe]).unwrap();
    let thread = open_map(&h);
    first_turn(&h, &thread);
    write(&h, "docs/edit.md", "New text.");
    file_source(&h, &["docs/keep.md", "docs/edit.md", "docs/blob.bin"]);
    assert_eq!(turn(&h, &thread, 1).state, AgentTurnState::Delivered);

    let counts = records_of(&thread, "map plan references for a turn");
    assert_eq!(counts.len(), 2, "{counts:?}");
    let second = &counts[1].fields;
    for (name, value) in [
        ("references", 3),
        ("first", 0),
        ("added", 1),
        ("changed", 1),
        ("unchanged", 1),
        ("removed", 1),
        ("unresolved", 0),
        ("notText", 1),
        ("unreadable", 0),
    ] {
        assert_eq!(second.get(name), Some(&serde_json::json!(value)), "{name}: {second:?}");
    }
    let mut all = counts.clone();
    all.extend(records_of(&thread, "map plan reference fingerprints recorded"));
    let all = format!("{all:?}");
    for leak in ["keep.md", "edit.md", "drop.md", "blob.bin", "Kept text", "New text"] {
        assert!(!all.contains(leak), "{leak} in {all}");
    }
}

// MPR-FR-VGCX: a record that cannot be written logs its typed error, the turn
// still delivers, and the next turn compares with the older record (here: none).
#[test]
fn a_record_that_cannot_be_written_does_not_change_the_turn() {
    let h = Harness::new(vec![
        Ok("One. <spoken>One.</spoken>".into()),
        Ok("Two. <spoken>Two.</spoken>".into()),
    ]);
    file_source(&h, &["docs/hld.md"]);
    write(&h, "docs/hld.md", "Design.");
    let thread = open_map(&h);
    // A plain file where the folder of the records must be.
    let folder = crate::comments::map_fingerprints_dir(&h.root()).unwrap();
    std::fs::write(&folder, "not a folder").unwrap();
    assert_eq!(first_turn(&h, &thread).state, AgentTurnState::Delivered);
    let warns = records_of(&thread, "map plan reference fingerprints not recorded");
    assert_eq!(warns.len(), 1, "{warns:?}");
    assert!(warns[0].fields.contains_key("code"), "{warns:?}");
    assert!(!format!("{warns:?}").contains("hld"), "{warns:?}");

    assert_eq!(turn(&h, &thread, 1).state, AgentTurnState::Delivered);
    assert!(marks_of(&h, 1).contains("change: first"));
}

// MPR-FR-CHRV, MPR-FR-VGCX, AGC-FR-13: a turn in flight keeps the text it was
// assembled with, and records the fingerprint of that text. An edit made while
// it runs reaches the next turn as `changed`.
#[test]
fn a_turn_in_flight_keeps_its_text_and_records_it() {
    use super::speech_turns::gated;
    let (h, gate) =
        gated(vec![Ok(ScriptedReply::answer("One.")), Ok(ScriptedReply::answer("Two."))]);
    file_source(&h, &["docs/hld.md"]);
    write(&h, "docs/hld.md", "Before.");
    let thread = crate::comments::open_map_discussion_in(
        &h.root(),
        None,
        Opening { body: "@arch plan it".into(), attachments: Vec::new(), spoken_original: None },
        Stamp { by: &human("ada"), at: "2026-03-01T10:00:00Z" },
    )
    .unwrap()
    .discussion;
    let running =
        h.dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id).unwrap();
    gate.wait_entered(1);
    write(&h, "docs/hld.md", "After.");
    gate.release();
    assert_eq!(wait_for_terminal(&h, &running.id).state, AgentTurnState::Delivered);
    let body = references_of(&h, 0).body;
    assert!(body.contains("Before."), "{body}");
    assert!(!body.contains("After."), "{body}");
    assert_eq!(recorded(&h, &thread), vec![("docs/hld.md".into(), fp("Before."))]);

    gate.release();
    assert_eq!(turn(&h, &thread, 1).state, AgentTurnState::Delivered);
    let body = references_of(&h, 1).body;
    assert!(body.contains("After."), "{body}");
    assert!(marks_of(&h, 1).contains("change: changed"), "{body}");
}

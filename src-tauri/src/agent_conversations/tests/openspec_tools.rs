//! The OpenSpec read tools in a conversational turn (CVL-FR-DPWP,
//! CVL-FR-TPHR, CVL-FR-08, LOS-FR-OCCY, ROS-FR-MHZE, LOC-FR-DHDA).
//!
//! Each test observes the request the seam was given, so what is asserted is
//! what a model is offered.

use super::*;
use crate::openspec::tests::{env, FakeProbe, FakeRunner, CLI};

const OPENSPEC_TOOLS: [&str; 3] = [
    crate::tools::openspec_list_specs::NAME,
    crate::tools::openspec_read_spec::NAME,
    crate::tools::openspec_list_changes::NAME,
];

/// The tool names of the first request of one artifact turn in a harness
/// whose OpenSpec seam finds a CLI when `cli` is true.
fn offered(setup: impl FnOnce(&Harness), cli: bool) -> Vec<String> {
    let h = Harness::new(vec![Ok("ok".into())]);
    setup(&h);
    let probe = if cli { FakeProbe::default().with_executable(CLI) } else { FakeProbe::default() };
    let runner = FakeRunner::default().json("--version", "1.17.0\n");
    let seam = crate::openspec::OpenSpec::new(Arc::new(runner), Arc::new(probe), env());
    // The kept check a project open leaves behind (OSM-FR-ZNVZ), so the turn
    // finds the CLI state without starting a process.
    seam.cli_state(&crate::openspec::tests::Silent, crate::openspec::tests::buffer());
    h.app.manage(seam);
    h.mount();
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(&h, &turn.id);
    h.seam
        .requests()
        .into_iter()
        .next()
        .expect("one request")
        .0
        .tools
        .into_iter()
        .map(|t| t.name.as_str().to_string())
        .collect()
}

fn write(h: &Harness, rel: &str, body: &str) {
    let path = h.root.path().join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

// CVL-FR-DPWP, LOS-FR-OCCY, ROS-FR-MHZE, LOC-FR-DHDA: an active mode with an
// available CLI adds the three tools to the turn.
#[test]
fn an_openspec_project_with_a_cli_offers_the_three_read_tools() {
    let tools = offered(|h| write(h, "openspec/config.yaml", "schema: spec-driven\n"), true);
    for name in OPENSPEC_TOOLS {
        assert!(tools.iter().any(|t| t == name), "{name} missing from {tools:?}");
    }
    // CVL-FR-08: the tools every turn gets are still there.
    for name in EXPECTED_TOOLS {
        assert!(tools.iter().any(|t| t == name), "{name} missing from {tools:?}");
    }
}

// CVL-FR-DPWP, OSM-FR-RMUD: a project that does not use OpenSpec is offered
// exactly the set it is offered with no OpenSpec seam at all.
#[test]
fn a_project_without_openspec_is_offered_the_same_set_as_before() {
    let with_seam = offered(|_| {}, true);
    let without_seam = {
        let h = Harness::new(vec![Ok("ok".into())]);
        h.mount();
        h.create_agent("arch", "");
        let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
        let turn = h
            .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
            .expect("dispatch");
        wait_for_terminal(&h, &turn.id);
        h.seam
            .requests()
            .into_iter()
            .next()
            .unwrap()
            .0
            .tools
            .into_iter()
            .map(|t| t.name.as_str().to_string())
            .collect::<Vec<_>>()
    };
    assert_eq!(with_seam, without_seam);
    assert!(!with_seam.iter().any(|t| OPENSPEC_TOOLS.contains(&t.as_str())));
}

// CVL-FR-DPWP, OSM-FR-UILN: the setting `off` and a missing CLI each keep the
// three tools away.
#[test]
fn the_setting_off_or_a_missing_cli_offers_none_of_the_three() {
    let off = offered(
        |h| {
            write(h, "openspec/config.yaml", "schema: spec-driven\n");
            write(h, ".synthesis/project.toml", "openspecMode = \"off\"\n");
        },
        true,
    );
    let no_cli = offered(|h| write(h, "openspec/config.yaml", "schema: spec-driven\n"), false);
    for tools in [off, no_cli] {
        assert!(!tools.iter().any(|t| OPENSPEC_TOOLS.contains(&t.as_str())), "{tools:?}");
    }
}

// CVL-FR-TPHR: the three tools end no turn and are not withheld from an
// advisor.
#[test]
fn the_three_read_tools_end_no_turn_and_an_advisor_keeps_them() {
    for name in OPENSPEC_TOOLS {
        assert!(!crate::agent_conversations::turn_body::ends_turn(name), "{name} ends a turn");
        assert!(!crate::agent_conversations::groups::ADVISOR_WITHHELD_TOOLS.contains(&name));
    }
}

/// The tool names of the first request of one turn on the origin `seed`
/// makes, in an OpenSpec project whose CLI check is kept.
fn offered_on(seed: impl FnOnce(&Harness) -> (ConversationOrigin, String)) -> Vec<String> {
    let h = Harness::new(vec![Ok("ok".into())]);
    write(&h, "openspec/config.yaml", "schema: spec-driven\n");
    let runner = FakeRunner::default().json("--version", "1.17.0\n");
    let seam = crate::openspec::OpenSpec::new(
        Arc::new(runner),
        Arc::new(FakeProbe::default().with_executable(CLI)),
        env(),
    );
    seam.cli_state(&crate::openspec::tests::Silent, crate::openspec::tests::buffer());
    h.app.manage(seam);
    h.mount();
    h.create_agent("arch", "");
    let (origin, trigger) = seed(&h);
    let turn = h.dispatch("arch", origin, &trigger).expect("dispatch");
    wait_for_terminal(&h, &turn.id);
    h.seam
        .requests()
        .into_iter()
        .next()
        .expect("one request")
        .0
        .tools
        .into_iter()
        .map(|t| t.name.as_str().to_string())
        .collect()
}

// CVL-FR-DPWP: a turn of any target gets the three tools, a draft discussion
// and a note discussion as well as an artifact one.
#[test]
fn a_draft_and_a_note_turn_are_offered_the_three_read_tools() {
    let draft = offered_on(|h| {
        let created =
            crate::drafts::create_draft_at_root(&h.root(), Some("Review me")).expect("draft");
        crate::drafts::save_draft_file_impl(
            &h.root(),
            &created.draft.id,
            &created.file,
            "Hello there.",
        )
        .expect("write the draft");
        let thread = seed_draft_thread(h, &created.draft.id, &created.file);
        (ConversationOrigin::of(&thread), thread.comments[0].id.clone())
    });
    let note = offered_on(|h| {
        let (note_id, origin) = seed_note_discussion(
            h,
            crate::notes::NoteScope::Project,
            "a note",
            "@arch is this real?",
        );
        let trigger = crate::comments::note_discussion_of(&h.root(), &note_id).unwrap().comments[0]
            .id
            .clone();
        (origin, trigger)
    });
    for tools in [draft, note] {
        for name in OPENSPEC_TOOLS {
            assert!(tools.iter().any(|t| t == name), "{name} missing from {tools:?}");
        }
    }
}

// CVL-FR-DPWP: a CLI with no kept check gives a turn none of the three, and
// the turn starts no CLI process of its own.
#[test]
fn a_turn_with_no_kept_check_is_offered_none_of_the_three() {
    let h = Harness::new(vec![Ok("ok".into())]);
    write(&h, "openspec/config.yaml", "schema: spec-driven\n");
    let runner = Arc::new(FakeRunner::default().json("--version", "1.17.0\n"));
    h.app.manage(crate::openspec::OpenSpec::new(
        runner.clone(),
        Arc::new(FakeProbe::default().with_executable(CLI)),
        env(),
    ));
    h.mount();
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let turn = h
        .dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id)
        .expect("dispatch");
    wait_for_terminal(&h, &turn.id);
    let tools: Vec<String> = h
        .seam
        .requests()
        .into_iter()
        .next()
        .unwrap()
        .0
        .tools
        .into_iter()
        .map(|t| t.name.as_str().to_string())
        .collect();
    assert!(!tools.iter().any(|t| OPENSPEC_TOOLS.contains(&t.as_str())), "{tools:?}");
}

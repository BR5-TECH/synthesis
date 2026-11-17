//! The definition of the tool and the contract of its output (PDM-FR-ZXQE,
//! PDM-FR-MBHW, PDM-FR-LJGN, PDM-FR-JLYU, PDM-FR-VQKI).

use super::*;

fn keys(value: &serde_json::Value) -> Vec<String> {
    let mut keys: Vec<String> = value.as_object().expect("an object").keys().cloned().collect();
    keys.sort();
    keys
}

// PDM-FR-ZXQE: the tool is a portable tool of this name, with no Tauri command
// and no event of its own.
#[test]
fn it_is_a_portable_tool_with_no_command_or_event_of_its_own() {
    let f = Fixture::new(Subject::Draft);
    let definition = rig::tool::tool_definition(&f.tool());
    assert_eq!(definition.name, "propose_diagram_changes");
    assert_eq!(definition.name, NAME);
    assert_eq!(
        <ProposeDiagramChangesTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        "propose_diagram_changes",
    );
    const LIB: &str = include_str!("../../../lib.rs");
    assert!(!LIB.contains(NAME), "no Tauri command and no event belongs to this tool");
    const MODULE: &str = include_str!("../../propose_diagram_changes.rs");
    assert!(!MODULE.contains("#[tauri::command]"));
    assert!(!MODULE.contains("app.emit"));
}

// PDM-FR-MBHW: the description is the fixed text, and the schema has the four
// arguments with their own descriptions. `path` and `rationale` are required,
// because a call carries `diagrams` or `updates`.
#[test]
fn the_definition_is_the_fixed_text_with_four_arguments() {
    let f = Fixture::new(Subject::Draft);
    let tool = f.tool();
    assert_eq!(tool.description(), DESCRIPTION);

    let schema = tool.parameters();
    assert_eq!(schema["type"], "object");
    assert_eq!(keys(&schema["properties"]), ["diagrams", "path", "rationale", "updates"]);
    assert_eq!(schema["required"], serde_json::json!(["path", "rationale"]));
    for (name, description) in [
        ("path", PATH_DESCRIPTION),
        ("rationale", RATIONALE_DESCRIPTION),
        ("diagrams", DIAGRAMS_DESCRIPTION),
        ("updates", UPDATES_DESCRIPTION),
    ] {
        assert_eq!(schema["properties"][name]["description"], description, "{name}");
    }

    // TLC-FR-06: each field of a diagram change carries its own description.
    let item = &schema["properties"]["diagrams"]["items"];
    assert_eq!(
        keys(&item["properties"]),
        ["after", "after_text", "before", "lead", "note", "trail"],
    );
    for (name, property) in item["properties"].as_object().expect("properties") {
        assert!(
            property["description"].as_str().is_some_and(|d| !d.is_empty()),
            "{name} carries its own description",
        );
    }
    let entry = &schema["properties"]["updates"]["items"];
    assert_eq!(
        entry["properties"]["action"]["enum"],
        serde_json::json!(["change", "add", "withdraw"]),
    );
}

// PDM-FR-MBHW: neither the description nor the schema varies by origin, by
// conversation, by agent, or by project.
#[test]
fn the_definition_is_byte_identical_wherever_it_appears() {
    let draft = Fixture::new(Subject::Draft);
    let artifact = Fixture::new(Subject::Artifact);
    let one = rig::tool::tool_definition(&draft.tool());
    let two = rig::tool::tool_definition(&artifact.tool());
    let three = rig::tool::tool_definition(&artifact.tool_for(
        ConversationOrigin::stub_artifact("other-thread", "other.md", true),
        other_agent(),
    ));
    for other in [&two, &three] {
        assert_eq!(one.name, other.name);
        assert_eq!(one.description, other.description);
        assert_eq!(one.parameters, other.parameters);
    }
}

// PDM-FR-LJGN: no argument names a draft, an artifact, a conversation, a
// thread, a participant, or an agent.
#[test]
fn no_argument_names_a_conversation_an_agent_or_a_participant() {
    let schema = parameters();
    let mut names = keys(&schema["properties"]);
    for part in ["diagrams", "updates"] {
        names.extend(keys(&schema["properties"][part]["items"]["properties"]));
    }
    for forbidden in [
        "draft",
        "draft_id",
        "artifact",
        "artifact_id",
        "thread",
        "thread_id",
        "conversation",
        "participant",
        "agent",
    ] {
        assert!(!names.iter().any(|name| name == forbidden), "no argument is named {forbidden}");
    }
}

// PDM-FR-MBHW, CVL-FR-04: the model reads that a diagram change never goes in a
// text change, that the source is given without fence markers, and that the
// rationale carries a spoken part.
#[test]
fn the_description_states_the_rules_the_model_must_keep() {
    assert!(DESCRIPTION.contains("A change to a diagram never goes in a text change"));
    assert!(DESCRIPTION.contains("leave out the fence markers"));
    assert!(DESCRIPTION.contains("The new source must be valid Mermaid."));
    assert!(DESCRIPTION.contains("one `<spoken>…</spoken>` tag in `rationale`"));
    assert!(DESCRIPTION.contains("A rationale with no tag is refused."));
    assert!(DESCRIPTION.contains("`updates`"));
    assert!(RATIONALE_DESCRIPTION.contains("`<spoken>…</spoken>`"));
    // The first sentence tells this tool from the two text tools.
    assert!(DESCRIPTION.starts_with("Propose changes to Mermaid diagrams"));
}

// PDM-FR-JLYU: the output is the fact of the proposal, with no id and no part of
// the source, and a successful call sets the flag that ends the turn.
#[test]
fn the_output_is_the_data_object_and_nothing_composed() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        let output = f.propose(vec![change(SRC_A, SRC_A2)]).expect("recorded");
        assert_eq!(output, ProposeDiagramChangesOutput { proposed: true });
        let json = serde_json::to_value(&output).expect("serialises");
        assert_eq!(json, serde_json::json!({ "proposed": true }));
        assert!(f.proposed.load(Ordering::SeqCst), "the loop is told a proposal went out");
    }
}

// PDM-FR-JLYU: the call resolves without being polled twice, because it waits
// for no decision, holds no timer, and opens no watcher. `block_on` panics on a
// future that is not ready.
#[test]
fn the_call_waits_for_nothing() {
    let f = Fixture::new(Subject::Artifact);
    let tool = f.tool();
    let future = tool.call(f.args(vec![change(SRC_A, SRC_A2)]));
    assert!(block_on(future).is_ok());
}

// PDM-FR-VQKI: the parse of a source and the record are local. The module and
// its check reach no network client.
#[test]
fn the_tool_reaches_no_network() {
    for source in [include_str!("../../propose_diagram_changes.rs"), include_str!("../validate.rs")]
    {
        for word in ["reqwest", "hyper", "TcpStream", "http://", "https://"] {
            assert!(!source.contains(word), "the tool reaches no network: {word}");
        }
    }
}

// PDM-FR-OHSB: the tool holds no state between calls. Two calls through two
// instances of one turn are the same as two through one.
#[test]
fn the_tool_holds_no_state_between_calls() {
    let f = Fixture::new(Subject::Draft);
    let refusal = f.propose(vec![change("flowchart TD\n  Z --> Q", SRC_A2)]).unwrap_err();
    assert!(matches!(refusal, ToolRefusal::InvalidDiagramAt { .. }));
    // The refusal left nothing behind: the same tool takes a good call at once.
    let tool = f.tool();
    assert!(block_on(tool.call(f.args(vec![change(SRC_A, SRC_A2)]))).is_ok());
    assert_eq!(f.proposals().len(), 1);
}

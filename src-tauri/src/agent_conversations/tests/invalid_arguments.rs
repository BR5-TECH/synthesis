//! Arguments the model sent as anything but a JSON object
//! (`TLC-tool-conventions.md` TLC-FR-SPRR, TLC-FR-14).
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

/// Run one turn whose model asks for `read_file` with `arguments`, and give
/// back the result the model was handed and the refusal recorded for it.
fn refused_call(arguments: serde_json::Value) -> (String, LogRecord) {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls("read_file", arguments)),
        Ok(ScriptedReply::answer("Corrected myself.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let results = tool_results(&h.seam.exchanges()[1]);
    assert_eq!(results.len(), 1);
    let record = wait_for_record(&terminal.id, "tool call refused");
    assert_eq!(require(&record, "tool"), "read_file");
    (results[0].clone(), record)
}

// TLC-FR-SPRR, TLC-FR-14: a cut-off object is refused before the tool runs,
// also when the framework recovered the field the tool needs from it, and the
// record names the shape the model sent rather than the fields recovered.
#[test]
fn a_cut_off_object_is_refused_before_the_tool_runs() {
    // The framework does recover the field the tool needs from this text, so
    // the refusal below is the rule and not a parse that found nothing.
    let recovered = rig::completion::message::ToolFunction::parse(
        rig::completion::message::ToolName::new("read_file").expect("a tool name"),
        "{\"path\": \"README.md\"",
    );
    assert_eq!(recovered.arguments["path"], "README.md");
    assert!(recovered.invalid_arguments.is_some());

    let (result, record) = refused_call(serde_json::json!("{\"path\": \"README.md\""));
    assert!(
        result.contains("not in the shape it expects"),
        "the model is told its arguments were wrong: {result}",
    );
    assert_eq!(
        record.fields["reason"],
        serde_json::json!(crate::tools::ARGUMENTS_UNDECODABLE),
    );
    assert_eq!(record.fields["argShape"], serde_json::json!("string"));
    assert!(!format!("{record:?}").contains("README"), "no value is recorded");
}

// TLC-FR-SPRR, TLC-FR-14: an array is refused before the tool runs, and the
// record names its shape.
#[test]
fn an_array_is_refused_before_the_tool_runs() {
    let (result, record) = refused_call(serde_json::json!(["README.md", 3]));
    assert!(result.contains("not in the shape it expects"), "{result}");
    assert_eq!(
        record.fields["reason"],
        serde_json::json!(crate::tools::ARGUMENTS_UNDECODABLE),
    );
    assert_eq!(record.fields["argShape"], serde_json::json!("[string](2)"));
}

// TLC-FR-SPRR: a number is refused before the tool runs.
#[test]
fn a_number_is_refused_before_the_tool_runs() {
    let (result, record) = refused_call(serde_json::json!(42));
    assert!(result.contains("not in the shape it expects"), "{result}");
    assert_eq!(record.fields["argShape"], serde_json::json!("number"));
}

// TLC-FR-SPRR: arguments sent as `null` are an empty object, which the tool
// itself reads — here, as an object without the field it requires.
#[test]
fn null_arguments_are_an_empty_object() {
    let (result, record) = refused_call(serde_json::Value::Null);
    assert!(result.contains("not in the shape it expects"), "{result}");
    assert_eq!(record.fields["argShape"], serde_json::json!("{}"));
}

// TLC-FR-SPRR: a string holding a whole object is that object, so the tool
// runs on it.
#[test]
fn a_string_holding_a_whole_object_is_that_object() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(
            "read_file",
            serde_json::json!("{\"path\": \"no-such-file.md\"}"),
        )),
        Ok(ScriptedReply::answer("Done.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    let results = tool_results(&h.seam.exchanges()[1]);
    assert_eq!(results.len(), 1);
    assert!(
        !results[0].contains("not in the shape it expects"),
        "the tool ran on the object: {}",
        results[0],
    );
}

// TLC-FR-SPRR: a cut-off call of a tool that ends the turn is refused before
// it runs, so the turn asks the author nothing on half a question.
#[test]
fn a_cut_off_question_asks_nothing() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls(
            crate::tools::ask_user_comment::NAME,
            serde_json::json!("{\"question\": \"Do you mean the intro"),
        )),
        Ok(ScriptedReply::answer("Corrected myself.")),
    ]);
    h.mount();
    h.create_agent("arch", "");
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered, "no question was asked");
    let results = tool_results(&h.seam.exchanges()[1]);
    assert_eq!(results.len(), 1);
    assert!(results[0].contains("not in the shape it expects"), "{}", results[0]);
    let record = wait_for_record(&terminal.id, "tool call refused");
    assert_eq!(require(&record, "tool"), crate::tools::ask_user_comment::NAME);
    assert_eq!(
        record.fields["reason"],
        serde_json::json!(crate::tools::ARGUMENTS_UNDECODABLE),
    );
}

// TLC-FR-SPRR: arguments sent as `null` or as blank text are an empty object,
// so a tool that needs no argument runs on them and nothing is refused.
#[test]
fn null_or_blank_arguments_let_a_tool_without_arguments_run() {
    for arguments in [serde_json::Value::Null, serde_json::json!(""), serde_json::json!("   ")] {
        let h = Harness::scripted(vec![
            Ok(ScriptedReply::calls(crate::tools::skill_list::NAME, arguments.clone())),
            Ok(ScriptedReply::answer("Done.")),
        ]);
        h.mount();
        h.create_agent("arch", "");
        let (terminal, _) = run_one(&h, "arch");
        assert_eq!(terminal.state, AgentTurnState::Delivered);
        let results = tool_results(&h.seam.exchanges()[1]);
        assert_eq!(results.len(), 1);
        assert!(
            !results[0].contains("not in the shape it expects"),
            "{arguments:?}: the tool ran: {}",
            results[0],
        );
        let refused = records_where("turnId", &terminal.id)
            .into_iter()
            .filter(|record| record.message == "tool call refused")
            .count();
        assert_eq!(refused, 0, "{arguments:?}: nothing was refused");
    }
}

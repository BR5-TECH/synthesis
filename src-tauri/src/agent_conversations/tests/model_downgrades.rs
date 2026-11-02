//! The served model a turn checks against the requested one (CVL-FR-XBMT,
//! AGC-FR-BXOS, AGC-FR-UVKF, AGC-FR-PNHM).
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;
use crate::model_downgrades::MODEL_DOWNGRADE_DETECTED;

/// Every `"model downgrade detected"` payload the app emits from now on.
fn capture(h: &Harness) -> Arc<Mutex<Vec<serde_json::Value>>> {
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    h.app.listen(MODEL_DOWNGRADE_DETECTED, move |event| {
        if let Ok(value) = serde_json::from_str(event.payload()) {
            sink.lock().unwrap_or_else(|e| e.into_inner()).push(value);
        }
    });
    seen
}

fn pairs(seen: &Mutex<Vec<serde_json::Value>>) -> Vec<(String, String)> {
    seen.lock()
        .unwrap()
        .iter()
        .map(|v| {
            (
                v["requestedModel"].as_str().unwrap_or_default().to_string(),
                v["servedModel"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect()
}

// CVL-FR-XBMT, AGC-FR-BXOS, AGC-FR-UVKF: a turn of three rounds emits each
// requested→served pair once, and a round served by the requested model emits
// nothing.
#[test]
fn a_turn_emits_each_pair_once_across_its_rounds() {
    let read = || ScriptedReply::calls("read_file", serde_json::json!({ "path": "spec.md" }));
    let h = Harness::scripted(vec![
        Ok(read().served_model("m-older")),
        Ok(read().served_model("m-older")),
        Ok(read().served_model("m")),
        Ok(ScriptedReply::answer("Done.").served_model("m-other")),
    ]);
    h.mount();
    h.create_agent("arch", "Argue about structure.");
    let seen = capture(&h);
    let (terminal, thread) = run_one(&h, "arch");
    assert_eq!(
        terminal.state,
        AgentTurnState::Delivered,
        "the check changes nothing about the turn"
    );

    let requested = h.seam.requests()[0].1.model_id.clone().expect("the endpoint names a model");
    assert_eq!(
        pairs(&seen),
        vec![(requested.clone(), "m-older".to_string()), (requested, "m-other".to_string())]
    );

    // AGC-FR-PNHM: the payload names the source, the provider, the project,
    // the worktree, and the conversation, and nothing of what was said.
    let first = seen.lock().unwrap()[0].clone();
    assert_eq!(first["source"], "agent");
    assert_eq!(first["provider"], "openrouter");
    assert_eq!(first["projectKey"], PROJECT_KEY);
    assert_eq!(first["worktree"], h.root().path().to_string_lossy().as_ref());
    assert_eq!(first["origin"]["discussionId"], thread.id.as_str());
    assert!(first["runId"].is_null());
    let text = first.to_string();
    assert!(!text.contains("Done."), "{text}");
    assert!(!text.contains("Argue about structure."), "{text}");
}

// AGC-FR-BXOS: a response that names no model, or the requested one, emits
// nothing.
#[test]
fn no_served_model_or_the_same_model_emits_nothing() {
    let h = Harness::scripted(vec![
        Ok(ScriptedReply::calls("read_file", serde_json::json!({ "path": "spec.md" }))),
        Ok(ScriptedReply::answer("Done.").served_model("m")),
    ]);
    h.mount();
    h.create_agent("arch", "Argue about structure.");
    let seen = capture(&h);
    let (terminal, _) = run_one(&h, "arch");
    assert_eq!(terminal.state, AgentTurnState::Delivered);
    assert_eq!(h.seam.requests()[0].1.model_id.as_deref(), Some("m"));
    assert!(pairs(&seen).is_empty());
}

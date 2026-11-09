//! "write voice progress update" (AGC-FR-YMFC, AGC-FR-PGSJ, AGC-FR-TUNO).
//!
//! The seam answers the call, so no test reaches a network, and no test waits on
//! time.

use super::agent_groups::*;
use super::*;

fn entry(tool: &str, advisor: Option<&str>) -> ActivityEntry {
    ActivityEntry { tool: tool.to_string(), advisor: advisor.map(str::to_string) }
}

fn harness(reply: &'static str) -> (Harness, Arc<RoutedSeam>) {
    let seam = RoutedSeam::new(move |_| Ok(reply.to_string()));
    let h = Harness::with_patient_seam(Box::new(seam.clone()));
    h.mount();
    (h, seam)
}

fn update(h: &Harness, activity: &[ActivityEntry]) -> Result<ProgressUpdate, String> {
    progress_update_impl(&h.app.handle().clone(), PROJECT_KEY, activity, "English")
}

// AGC-FR-PGSJ: the answer is the first line, trimmed, without a pair of quotation
// marks, and cut to its first 5 words.
#[test]
fn the_answer_is_cut_to_five_words() {
    assert_eq!(
        clean_update("  Checking existing specifications.  \nA second line."),
        Some("Checking existing specifications.".to_string())
    );
    assert_eq!(
        clean_update("\"One two three four five six seven\""),
        Some("One two three four five".to_string())
    );
    assert_eq!(
        clean_update("\n\nReading the draft now.\n"),
        Some("Reading the draft now.".to_string())
    );
    assert_eq!(clean_update("   \n  "), None);
    assert_eq!(clean_update(""), None);
}

// AGC-FR-YMFC: the call has no tools, and the input is the activity and the
// language alone. An advisor call names the advisor. No key input goes in.
#[test]
fn the_input_is_the_activity_alone() {
    let activity = [entry("search_specifications", None), entry("search_drafts", Some("sec"))];
    let request = progress_request(&activity, "French");
    assert!(request.tools.is_empty());
    assert!(request.native_tools.is_empty());
    let tags: Vec<&str> = request.input.iter().map(|s| s.tag.as_str()).collect();
    assert_eq!(tags, ["language", "activity"]);
    assert_eq!(request.input[0].body, "French");
    assert_eq!(request.input[1].body, "- search_specifications\n- search_drafts (advisor @sec)");
    assert!(request.instructions.contains("at most 5 words"));
    assert!(request.instructions.contains("Do not ask a question"));
}

// AGC-FR-YMFC: an empty activity is a valid input.
#[test]
fn an_empty_activity_is_a_valid_input() {
    let request = progress_request(&[], "English");
    assert_eq!(request.input[1].body, "");
}

// AGC-FR-YMFC, AGC-FR-PGSJ, AAP-FR-EDBQ: one call with no tool through the seam,
// and the answer is the phrase cut to 5 words.
#[test]
fn the_call_answers_a_phrase_of_up_to_five_words() {
    let (h, seam) = harness("Checking the existing specifications for you now.");
    let answer = update(&h, &[entry("search_specifications", None)]).expect("an update");
    assert_eq!(answer.text, "Checking the existing specifications for");
    assert_eq!(answer.text.split_whitespace().count(), 5);
    let requests = seam.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].tools.is_empty());
}

// AGC-FR-PGSJ: an empty reply and a failed call answer `progress_update_failed`.
#[test]
fn an_empty_reply_and_a_failed_call_are_a_failure() {
    let (h, _) = harness("   ");
    assert_eq!(update(&h, &[]).unwrap_err(), ERR_PROGRESS_UPDATE_FAILED);
    let seam = RoutedSeam::new(|_| Err("unreachable"));
    let failing = Harness::with_patient_seam(Box::new(seam));
    failing.mount();
    assert_eq!(update(&failing, &[]).unwrap_err(), ERR_PROGRESS_UPDATE_FAILED);
}

// AGC-FR-PGSJ: a project with no provider answers `provider_unconfigured`, and the
// seam is not called.
#[test]
fn a_missing_provider_is_a_typed_refusal() {
    let (h, seam) = harness("Working on it.");
    let (_, active) = h.store().load_ai_api_registry().expect("registry");
    h.store().save_ai_api_registry(Vec::new(), active).expect("registry");
    assert_eq!(update(&h, &[]).unwrap_err(), ai_api::ERR_PROVIDER_UNCONFIGURED);
    assert!(seam.requests().is_empty());
}

// AGC-FR-TUNO: the record holds the provider, the model, counts, and the duration,
// and it holds no text of the answer, no tool name, and no advisor name.
#[test]
fn the_record_holds_counts_and_no_text() {
    let (h, _) = harness("Checking existing specifications.");
    update(&h, &[entry("search_specifications", Some("secret-advisor"))]).expect("an update");
    let record = find(&all_records(), "voice progress update written");
    assert_eq!(require(&record, "activityEntries"), "1");
    assert_eq!(require(&record, "words"), "3");
    require(&record, "provider");
    require(&record, "model");
    require(&record, "durationMs");
    let text = format!("{:?}", record.fields);
    for forbidden in ["Checking", "specifications", "search_specifications", "secret-advisor"] {
        assert!(!text.contains(forbidden), "the record holds {forbidden}: {text}");
    }
}

// AGC-FR-TUNO: a failure holds a typed code and no text.
#[test]
fn a_failure_record_holds_a_code_and_no_text() {
    let (h, _) = harness("");
    let _ = update(&h, &[entry("search_drafts", Some("hidden-advisor"))]);
    let record = find(&all_records(), "voice progress update failed");
    assert_eq!(require(&record, "code"), "empty_reply");
    assert!(!format!("{:?}", record.fields).contains("hidden-advisor"));
}

//! The spoken language of the author: the prompt rule, the translation of a
//! voice transcript, and the transcript a turn never reads (CVL-FR-FQMM,
//! AGC-FR-QEOF, AGC-FR-SBGB, AGC-FR-JRXG).

use super::*;
use crate::global_settings::AppPreferences;

fn speak_language(h: &Harness, value: &str) {
    h.store()
        .save_app_preferences(AppPreferences {
            voice_spoken_language: value.to_string(),
            ..Default::default()
        })
        .expect("preferences");
}

// CVL-FR-04, CVL-FR-FQMM: the rule of a spoken language goes into its
// placeholder in every prompt, and no language gives nothing.
#[test]
fn the_spoken_language_rule_fills_its_placeholder_in_every_prompt() {
    let rule = "The author speaks Russian. Write the <spoken> part in Russian. Keep the comment \
                body and every document change in English.";
    for kind in [
        OriginKind::ArtifactComment,
        OriginKind::DraftComment,
        OriginKind::ArtifactDiscussion,
        OriginKind::DraftDiscussion,
        OriginKind::NoteDiscussion,
    ] {
        let with = compile_prompt_spoken(kind, "Argue.", "Developer", rule);
        assert!(with.contains(rule), "{kind:?} lacks the rule");
        assert!(!with.contains(SPOKEN_LANGUAGE_PLACEHOLDER));
        let without = compile_prompt(kind, "Argue.", "Developer");
        assert!(!without.contains("The author speaks"));
        assert!(!without.contains(SPOKEN_LANGUAGE_PLACEHOLDER));
    }
}

// CVL-FR-04: a value that holds another placeholder is text, and expands to
// nothing further.
#[test]
fn a_spoken_rule_cannot_reach_another_placeholder() {
    let compiled = compile_prompt_spoken(
        OriginKind::ArtifactComment,
        "Argue.",
        "",
        "{{ agent-instructions }} {{ agent-title }}",
    );
    assert!(compiled.contains("{{ agent-instructions }} {{ agent-title }}"));
    assert_eq!(compiled.matches("Argue.").count(), 1);
}

// CVL-FR-FQMM, GSS-FR-DNAW, VOP-FR-BYZC: the rule names the English name of a
// code of the table, a free name as it stands, and nothing for no language.
#[test]
fn the_rule_comes_from_the_preference() {
    let h = Harness::new(vec![]);
    assert_eq!(spoken_language_rule(&h.store()), "");
    speak_language(&h, "ru");
    assert_eq!(
        spoken_language_rule(&h.store()),
        "The author speaks Russian. Write the <spoken> part in Russian. Keep the comment body \
         and every document change in English."
    );
    speak_language(&h, "Basque");
    assert!(spoken_language_rule(&h.store()).starts_with("The author speaks Basque."));
    speak_language(&h, "<script>");
    assert_eq!(spoken_language_rule(&h.store()), "");
}

// CVL-FR-FQMM: a turn dispatched while a spoken language is set carries the
// rule in its prompt.
#[test]
fn a_turn_carries_the_rule_of_the_spoken_language() {
    let h = Harness::new(vec![Ok(said("Fine."))]);
    h.create_agent("arch", "");
    speak_language(&h, "de");
    let thread = h.seed_artifact_thread("spec.md", "Some source.");
    h.dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id).expect("dispatch");
    h.settle();
    let (request, _) = &h.seam.requests()[0];
    assert!(request.instructions.contains("The author speaks German."), "{}", request.instructions);
}

// AGC-FR-JRXG: the transcript of a translated comment never reaches the input
// of a turn.
#[test]
fn a_spoken_original_never_reaches_a_turn() {
    let h = Harness::new(vec![Ok(said("Fine."))]);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some source.");
    let root = h.root();
    let (_, comment_id) = crate::comments::add_comment_reporting_id(
        &root,
        crate::comments::ThreadRef::artifact(&thread_artifact(&thread)),
        &thread.id,
        "@arch look at the second part".into(),
        Vec::new(),
        Vec::new(),
        Some("смотри вторую часть ZEBRAQUARTZ".into()),
        crate::comments::Stamp { by: &human("ada"), at: "2026-01-01T00:09:00Z" },
    )
    .expect("comment");
    h.dispatch("arch", ConversationOrigin::of(&thread), &comment_id).expect("dispatch");
    h.settle();
    let (request, _) = &h.seam.requests()[0];
    let input = format!("{:?}", request.input);
    assert!(input.contains("look at the second part"));
    assert!(!input.contains("ZEBRAQUARTZ"), "the original reached the turn");
}

fn thread_artifact(thread: &Discussion) -> String {
    match &thread.target {
        DiscussionTarget::Artifact { artifact_id } => artifact_id.clone(),
        other => panic!("not an artifact: {other:?}"),
    }
}

// AGC-FR-QEOF, AGC-FR-SBGB: a translation is one call with no tools, under the
// fixed prompt, with the language, the names, and the transcript. Its record
// holds counts and no text.
#[test]
fn a_translation_is_one_call_with_no_tools() {
    let h =
        Harness::scripted(vec![Ok(ScriptedReply::unspoken("  @arch look at the second part  "))]);
    h.mount();
    speak_language(&h, "ru");
    let handle = h.app.handle().clone();
    let translated =
        translate_impl(&handle, PROJECT_KEY, "@arch смотри вторую часть", &["arch".to_string()])
            .expect("translated");
    assert_eq!(translated.text, "@arch look at the second part");
    let (request, call) = &h.seam.requests()[0];
    // AGC-FR-QEOF: the provider of the project, with its selected or first model.
    let provider = crate::ai_api::resolve_agent_provider(&h.store(), PROJECT_KEY).unwrap();
    assert_eq!(call.provider, provider);
    assert_eq!(
        call.model_id,
        crate::ai_api::default_model_of(&h.store(), &provider),
        "the model of the call"
    );
    assert_eq!(request.input[2].body, "@arch смотри вторую часть");
    assert!(request.tools.is_empty() && request.native_tools.is_empty());
    assert!(request.instructions.contains("You translate one spoken message into English."));
    let tags: Vec<&str> = request.input.iter().map(|s| s.tag.as_str()).collect();
    assert_eq!(tags, vec!["language", "names", "transcript"]);
    assert_eq!(request.input[0].body, "Russian");
    assert_eq!(request.input[1].body, "arch");
    let record = find(&all_records(), "voice transcript translated");
    assert_eq!(field(&record, "transcriptChars"), "25");
    assert_eq!(field(&record, "translationChars"), "29");
    assert_eq!(field(&record, "provider"), provider);
    assert!(!field(&record, "durationMs").is_empty());
    let text = format!("{:?}", record.fields);
    assert!(!text.contains("смотри") && !text.contains("second part"), "{text}");
}

// AGC-FR-QEOF: no spoken language, an empty reply, and a failed call are typed.
#[test]
fn a_translation_fails_with_a_typed_code() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::unspoken("   ")), Err(FAIL_UNREACHABLE)]);
    h.mount();
    let handle = h.app.handle().clone();
    assert_eq!(
        translate_impl(&handle, PROJECT_KEY, "hola", &[]),
        Err(ERR_NO_SPOKEN_LANGUAGE.to_string())
    );
    speak_language(&h, "es");
    assert_eq!(
        translate_impl(&handle, PROJECT_KEY, "hola", &[]),
        Err(ERR_TRANSLATION_FAILED.to_string())
    );
    assert_eq!(
        translate_impl(&handle, PROJECT_KEY, "hola", &[]),
        Err(ERR_TRANSLATION_FAILED.to_string())
    );
    let failed = find(&all_records(), "voice transcript translation failed");
    assert!(!field(&failed, "code").is_empty(), "the failure names a typed code");
}

//! The document language and the spoken language: the prompt rules, the
//! translation of a voice transcript, and the transcript a turn never reads
//! (CVL-FR-FQMM, AGC-FR-LDTX, AGC-FR-QEOF, AGC-FR-WSKP, AGC-FR-SBGB,
//! AGC-FR-JRXG).

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

const POLISH: &str = "Dokument opisuje, jak aplikacja zapisuje komentarze autora i jak agent \
                      odpowiada na nie w tym samym wątku rozmowy.";
const SPANISH: &str = "El documento describe cómo la aplicación guarda los comentarios del autor \
                       y cómo el agente responde a ellos en la misma conversación.";
const ENGLISH_SPEECH: &str = "Please look at the second part of the document and tell me what \
                              you think about the order of the steps.";

// CVL-FR-04, CVL-FR-FQMM: the language rules go into their placeholder in
// every prompt.
#[test]
fn the_language_rules_fill_their_placeholder_in_every_prompt() {
    let rule = "Write the comment body and every document change in Polish.";
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

// CVL-FR-FQMM, GSS-FR-DNAW, VOP-FR-BYZC: the document rule is always there.
// The spoken rule names the English name of a code of the table, a free name
// as it stands, and nothing for no language.
#[test]
fn the_rules_come_from_the_document_and_the_preference() {
    let h = Harness::new(vec![]);
    assert_eq!(
        language_rules(&h.store(), "Polish"),
        "Write the comment body and every document change in Polish."
    );
    speak_language(&h, "ru");
    assert_eq!(
        language_rules(&h.store(), "English"),
        "Write the comment body and every document change in English.\nThe author speaks \
         Russian. Write the <spoken> part in Russian."
    );
    speak_language(&h, "Basque");
    assert!(language_rules(&h.store(), "English")
        .ends_with("The author speaks Basque. Write the <spoken> part in Basque."));
    speak_language(&h, "<script>");
    assert!(!language_rules(&h.store(), "English").contains("The author speaks"));
}

// AGC-FR-LDTX: the detector names a reliable language, and a short, code-only,
// or empty text has none, so it is English.
#[test]
fn the_document_language_is_detected_from_prose() {
    use document_language::{detect_language, language_of};
    assert_eq!(detect_language(POLISH), Some("Polish"));
    assert_eq!(detect_language(SPANISH), Some("Spanish"));
    assert_eq!(detect_language(ENGLISH_SPEECH), Some("English"));
    assert_eq!(language_of(Some("Tak.")).name, "English", "too few letters");
    assert_eq!(language_of(Some("")).name, "English");
    assert_eq!(language_of(None).name, "English", "a text that cannot be read");
    // Code and URLs are not prose: a Polish text keeps its language beside
    // a long English code block.
    let mixed = format!(
        "{POLISH}\n\n```rust\nfn main() {{ println!(\"read the file and write the result\"); \
         let value = compute_the_total_of_every_item_in_the_list(); }}\n```\n\
         `the inline code is english too` https://example.com/the/english/path/of/the/page\n"
    );
    assert_eq!(detect_language(&mixed), Some("Polish"));
    let code_only = "```\nlet the_value = read_the_file_and_return_the_text(path);\n```";
    assert_eq!(language_of(Some(code_only)).name, "English", "fenced code is not read");
}

// AGC-FR-LDTX: the text of each target kind decides its document language.
#[test]
fn each_target_kind_has_the_language_of_its_text() {
    let h = Harness::new(vec![]);
    let root = h.root();
    let roots = Roots::same(&root);
    let thread = h.seed_artifact_thread("spec.md", SPANISH);
    assert_eq!(document_language::document_language(roots, &thread.target, None).name, "Spanish");
    let (_, discussion) = seed_discussion(&h, &["@arch what now?"]);
    let DiscussionTarget::Draft { draft_id } = &discussion.target else { unreachable!() };
    let record = crate::drafts::read_draft_record(&root, draft_id).unwrap();
    let prompt = record.prompt_path.clone().unwrap();
    assert_eq!(
        document_language::document_language(roots, &discussion.target, None).name,
        "English",
        "a short prompt"
    );
    crate::drafts::save_draft_file_impl(&root, draft_id, &prompt, POLISH).unwrap();
    assert_eq!(
        document_language::document_language(roots, &discussion.target, None).name,
        "Polish"
    );
    let (_, origin) = seed_note_discussion(&h, crate::notes::NoteScope::Project, SPANISH, "hm");
    assert_eq!(document_language::document_language(roots, &origin.target, None).name, "Spanish");
    let missing = DiscussionTarget::Artifact { artifact_id: "missing.md".into() };
    assert_eq!(document_language::document_language(roots, &missing, None).name, "English");
}

// CVL-FR-FQMM, AGC-FR-LDTX: a turn on a Polish document is told to write in
// Polish, also with no spoken language, and a spoken language adds its rule.
#[test]
fn a_turn_carries_the_rules_of_its_document() {
    let h = Harness::new(vec![Ok(said("Fine.")), Ok(said("Fine."))]);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", POLISH);
    h.dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id).expect("dispatch");
    h.settle();
    let (request, _) = &h.seam.requests()[0];
    assert!(
        request
            .instructions
            .contains("Write the comment body and every document change in Polish."),
        "{}",
        request.instructions
    );
    assert!(!request.instructions.contains("The author speaks"));
    speak_language(&h, "de");
    h.dispatch("arch", ConversationOrigin::of(&thread), &thread.comments[0].id).expect("dispatch");
    h.settle();
    let (request, _) = &h.seam.requests()[1];
    assert!(request.instructions.contains("in Polish.\nThe author speaks German."));
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

fn translate(h: &Harness, text: &str, target: &DiscussionTarget) -> Result<Translation, String> {
    let handle = h.app.handle().clone();
    let root = h.root();
    translate_impl(&handle, Roots::same(&root), PROJECT_KEY, text, &["arch".to_string()], target)
}

// AGC-FR-QEOF, AGC-FR-SBGB: a translation is one call with no tools, under the
// fixed prompt, with the spoken language, the document language, the names,
// and the transcript. Its record holds counts and no text.
#[test]
fn a_translation_is_one_call_with_no_tools() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::unspoken("  @arch spójrz na drugą część  "))]);
    h.mount();
    speak_language(&h, "ru");
    let thread = h.seed_artifact_thread("spec.md", POLISH);
    let translated =
        translate(&h, "@arch смотри вторую часть", &thread.target).expect("translated");
    assert_eq!(translated.text, "@arch spójrz na drugą część");
    let (request, call) = &h.seam.requests()[0];
    // AGC-FR-QEOF: the provider of the project, with its selected or first model.
    let provider = crate::ai_api::resolve_agent_provider(&h.store(), PROJECT_KEY).unwrap();
    assert_eq!(call.provider, provider);
    assert_eq!(
        call.model_id,
        crate::ai_api::default_model_of(&h.store(), &provider),
        "the model of the call"
    );
    assert!(request.tools.is_empty() && request.native_tools.is_empty());
    assert!(request.instructions.contains("into the language of the document"));
    let tags: Vec<&str> = request.input.iter().map(|s| s.tag.as_str()).collect();
    assert_eq!(tags, vec!["language", "target_language", "names", "transcript"]);
    assert_eq!(request.input[0].body, "Russian");
    assert_eq!(request.input[1].body, "Polish");
    assert_eq!(request.input[2].body, "arch");
    assert_eq!(request.input[3].body, "@arch смотри вторую часть");
    // Tests run in parallel and share the buffer, so the record is the one of
    // this transcript.
    let record = newest(
        "voice transcript translated",
        &[("transcriptChars", "25"), ("documentLanguage", "Polish")],
    );
    assert_eq!(field(&record, "translationChars"), "27");
    assert_eq!(field(&record, "provider"), provider);
    assert_eq!(field(&record, "documentLanguage"), "Polish");
    assert_eq!(field(&record, "called"), "true");
    assert!(!field(&record, "durationMs").is_empty());
    let text = format!("{:?}", record.fields);
    assert!(!text.contains("смотри") && !text.contains("część"), "{text}");
}

// AGC-FR-QEOF: with no spoken language, a transcript that is not reliably in
// the document language is translated, and the prompt says the language is
// not known.
#[test]
fn a_translation_with_no_spoken_language_is_made() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::unspoken("Spójrz na drugą część."))]);
    h.mount();
    let thread = h.seed_artifact_thread("spec.md", POLISH);
    let translated = translate(&h, ENGLISH_SPEECH, &thread.target).expect("translated");
    assert_eq!(translated.text, "Spójrz na drugą część.");
    let (request, _) = &h.seam.requests()[0];
    assert_eq!(request.input[0].body, "Not known");
    assert_eq!(request.input[1].body, "Polish");
}

// AGC-FR-WSKP, AGC-FR-SBGB: a transcript already in the document language
// makes no call: by the spoken language, or by detection with none set.
#[test]
fn a_transcript_in_the_document_language_makes_no_call() {
    let h = Harness::scripted(vec![]);
    h.mount();
    let english = h.seed_artifact_thread("en.md", ENGLISH_SPEECH);
    // No spoken language: the transcript is detected as English.
    assert_eq!(translate(&h, ENGLISH_SPEECH, &english.target).unwrap().text, ENGLISH_SPEECH);
    // The spoken language is the document language: no detection is needed.
    let polish = h.seed_artifact_thread("pl.md", POLISH);
    speak_language(&h, "pl");
    assert_eq!(translate(&h, "tak", &polish.target).unwrap().text, "tak");
    assert!(h.seam.requests().is_empty(), "no model call");
    let record = newest(
        "voice transcript already in the document language",
        &[("documentLanguage", "Polish"), ("transcriptChars", "3")],
    );
    assert_eq!(field(&record, "called"), "false");
    assert!(!format!("{:?}", record.fields).contains("tak"));
}

// AGC-FR-WSKP: a spoken language that is not the document language is
// translated, also when the transcript reads as the document language.
#[test]
fn a_spoken_language_unlike_the_document_is_translated() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::unspoken("Look at it."))]);
    h.mount();
    speak_language(&h, "es");
    let english = h.seed_artifact_thread("en.md", ENGLISH_SPEECH);
    assert_eq!(translate(&h, "míralo", &english.target).unwrap().text, "Look at it.");
    assert_eq!(h.seam.requests().len(), 1);
}

// AGC-FR-QEOF: an empty reply and a failed call are typed.
#[test]
fn a_translation_fails_with_a_typed_code() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::unspoken("   ")), Err(FAIL_UNREACHABLE)]);
    h.mount();
    speak_language(&h, "es");
    let thread = h.seed_artifact_thread("spec.md", ENGLISH_SPEECH);
    assert_eq!(translate(&h, "hola", &thread.target), Err(ERR_TRANSLATION_FAILED.to_string()));
    assert_eq!(translate(&h, "hola", &thread.target), Err(ERR_TRANSLATION_FAILED.to_string()));
    let failed = newest(
        "voice transcript translation failed",
        &[("code", "empty_reply"), ("documentLanguage", "English")],
    );
    assert_eq!(field(&failed, "called"), "true");
}

/// The newest record of `message` whose fields hold each of `wanted`.
fn newest(message: &str, wanted: &[(&str, &str)]) -> LogRecord {
    all_records()
        .into_iter()
        .rev()
        .find(|r| r.message == message && wanted.iter().all(|(k, v)| field(r, k) == *v))
        .unwrap_or_else(|| panic!("no record reads {message:?} with {wanted:?}"))
}

// AGC-FR-WSKP: with no spoken language, a transcript too short to detect is
// not known to be in the document language, so the call is made.
#[test]
fn a_short_transcript_with_no_spoken_language_is_translated() {
    let h = Harness::scripted(vec![Ok(ScriptedReply::unspoken("OK."))]);
    h.mount();
    let english = h.seed_artifact_thread("en.md", ENGLISH_SPEECH);
    assert_eq!(translate(&h, "OK.", &english.target).unwrap().text, "OK.");
    assert_eq!(h.seam.requests().len(), 1);
}

// AGC-FR-QEOF, AGC-FR-SBGB: with no provider, a transcript that needs a
// translation is `provider_unconfigured`, and the record says no call was
// made. A transcript already in the language needs no provider.
#[test]
fn a_translation_with_no_provider_is_typed_and_makes_no_call() {
    let h = Harness::scripted(vec![]);
    h.mount();
    h.store().save_ai_api_registry(Vec::new(), None).unwrap();
    speak_language(&h, "es");
    let polish = h.seed_artifact_thread("pl.md", POLISH);
    assert_eq!(
        translate(&h, "hola", &polish.target),
        Err(crate::ai_api::ERR_PROVIDER_UNCONFIGURED.to_string())
    );
    let failed = newest(
        "voice transcript translation failed",
        &[("code", crate::ai_api::ERR_PROVIDER_UNCONFIGURED), ("documentLanguage", "Polish")],
    );
    assert_eq!(field(&failed, "called"), "false");
    speak_language(&h, "pl");
    assert_eq!(translate(&h, "tak", &polish.target).unwrap().text, "tak");
    assert!(h.seam.requests().is_empty());
}

// CVL-FR-FQMM, AGC-FR-LDTX: the estimate takes the document language of its
// target, as the turn does, and records it.
#[test]
fn the_estimate_takes_the_document_language() {
    let h = Harness::new(vec![]);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("estimate-pl.md", POLISH);
    let _ = estimate_impl(
        &h.app.handle().clone(),
        Roots::same(&h.root()),
        PROJECT_KEY,
        thread.target.clone(),
        Some(&thread.id),
        None,
    );
    let record =
        newest("document language chosen", &[("use", "estimate"), ("documentLanguage", "Polish")]);
    assert_eq!(field(&record, "basis"), "detected");
    assert_eq!(field(&record, "targetKind"), "artifact");
}

// AGC-FR-LDTX: the bound of 40 letters, a `~~~` fence, a `www.` word, and
// the first 20 000 characters alone.
#[test]
fn the_detector_reads_the_bounded_prose_alone() {
    use document_language::{language_of, Basis};
    let letters = |n: usize| "Dokument ".repeat(n / 8 + 1).chars().take(n).collect::<String>();
    assert_eq!(language_of(Some(&letters(39))).basis, Basis::TooShort);
    assert_ne!(language_of(Some(&letters(48))).basis, Basis::TooShort);
    let tilde = format!("~~~\n{ENGLISH_SPEECH} {ENGLISH_SPEECH}\n~~~\n{POLISH}");
    assert_eq!(language_of(Some(&tilde)).name, "Polish");
    let www = format!("{POLISH} www.the-english-words-of-a-long-address.example");
    assert_eq!(language_of(Some(&www)).name, "Polish");
    let english = ENGLISH_SPEECH.repeat(20_000 / ENGLISH_SPEECH.len() + 1);
    let late = format!("{english}\n{}", POLISH.repeat(50));
    assert_eq!(language_of(Some(&late)).name, "English", "the Polish is past the bound");
    let chinese =
        "这份文档说明应用程序如何保存作者的评论，以及代理如何在同一个对话中回答这些评论。";
    assert_eq!(language_of(Some(&chinese.repeat(2))).name, "Chinese");
    assert_eq!(language_of(None).basis, Basis::Unreadable);
}

// AGC-FR-WSKP, VOP-FR-IRZC: each name of the language table is a name that the
// detector gives, so a spoken language from the table can match a document.
#[test]
fn each_table_language_is_a_name_the_detector_gives() {
    let detected: Vec<&str> = whatlang::Lang::all()
        .iter()
        .map(|lang| match lang {
            whatlang::Lang::Cmn => "Chinese",
            other => other.eng_name(),
        })
        .collect();
    for (_, name) in crate::voice_ai::languages::LANGUAGES {
        assert!(detected.contains(name), "{name} is no name of the detector");
    }
}

// AGC-FR-LDTX: a note that the project no longer holds has no text, so its
// document language is English.
#[test]
fn a_missing_note_is_english() {
    let h = Harness::new(vec![]);
    let root = h.root();
    let missing = DiscussionTarget::Note { note_id: "gone".into() };
    let found = document_language::document_language(Roots::same(&root), &missing, None);
    assert_eq!(found.name, "English");
    assert_eq!(found.basis, document_language::Basis::Unreadable);
}

// AGC-FR-LDTX: a large artifact is read to its bound alone, and a character
// that the bound cuts does not make it a text that cannot be read.
#[test]
fn a_large_artifact_is_read_to_its_bound() {
    let h = Harness::new(vec![]);
    let mut body = String::new();
    while body.len() < 120_000 {
        body.push_str(POLISH);
        body.push('\n');
    }
    // Move the last two-byte character at or before byte 79 999 to start at
    // 79 999, so the bound of 80 000 bytes cuts it.
    let start = body
        .char_indices()
        .rev()
        .find(|(i, c)| *i <= 79_999 && c.len_utf8() == 2)
        .map(|(i, _)| i)
        .expect("a two-byte character");
    let text = format!("{}{body}", " ".repeat(79_999 - start));
    let thread = h.seed_artifact_thread("large-pl.md", &text);
    let root = h.root();
    let found = document_language::document_language(Roots::same(&root), &thread.target, None);
    assert_eq!(found.name, "Polish");
    assert_eq!(found.basis, document_language::Basis::Detected);
}

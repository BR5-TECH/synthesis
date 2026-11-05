//! "translate voice transcript": the transcript of a voice comment in the
//! document language of its target (AGC-FR-QEOF, AGC-FR-WSKP, AGC-FR-SBGB).

use super::*;
use crate::voice_ai::languages::{spoken_language, SpokenLanguage};
use tauri::Manager;

/// AGC-FR-QEOF: the fixed prompt of the call.
pub const TRANSLATE_PROMPT_TEMPLATE: &str =
    include_str!("../../../resources/prompts/translate-transcript.md");

/// AGC-FR-QEOF: the limit of the call.
const TRANSLATE_LIMIT: Duration = Duration::from_secs(30);

pub const ERR_TRANSLATION_FAILED: &str = "translation_failed";

/// GSS-FR-DNAW: the spoken language that the app preferences hold, if any.
fn preferred_spoken_language(store: &GlobalSettingsStore) -> Option<SpokenLanguage> {
    store
        .load_app_preferences()
        .ok()
        .and_then(|prefs| spoken_language(&prefs.voice_spoken_language))
}

/// CVL-FR-FQMM: the first line of the language rules, for the document
/// language `document`.
pub fn document_rule(document: &str) -> String {
    format!("Write the comment body and every document change in {document}.")
}

/// CVL-FR-FQMM: the language rules of a turn whose target has the document
/// language `document` (AGC-FR-LDTX).
pub fn language_rules(store: &GlobalSettingsStore, document: &str) -> String {
    let body = document_rule(document);
    match preferred_spoken_language(store) {
        Some(language) => {
            let name = language.name;
            format!("{body}\nThe author speaks {name}. Write the <spoken> part in {name}.")
        }
        None => body,
    }
}

/// AGC-FR-WSKP: the transcript needs no model call, because it is already in
/// the document language.
pub(crate) fn already_in(spoken: Option<&SpokenLanguage>, text: &str, document: &str) -> bool {
    match spoken {
        Some(language) => language.name.eq_ignore_ascii_case(document),
        None => document_language::detect_language(text) == Some(document),
    }
}

/// What the command answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Translation {
    pub text: String,
}

fn section(tag: &str, body: String) -> InputSection {
    InputSection {
        tag: tag.to_string(),
        attributes: Vec::new(),
        body,
        truncated: false,
        parts: Vec::new(),
    }
}

/// AGC-FR-QEOF: the request of one translation.
pub fn translation_request(
    spoken: &str,
    target: &str,
    nicknames: &[String],
    text: &str,
) -> AgentRequest {
    AgentRequest {
        instructions: crate::prompts::instruction_text(TRANSLATE_PROMPT_TEMPLATE),
        input: vec![
            section("language", spoken.to_string()),
            section("target_language", target.to_string()),
            section("names", nicknames.join(", ")),
            section("transcript", text.to_string()),
        ],
        ..Default::default()
    }
}

/// AGC-FR-QEOF, AGC-FR-WSKP: the text in the document language of `target`.
/// One model call with no tools, on the provider of the project and its
/// selected model, unless the text is already in that language.
pub fn translate_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    project_key: &str,
    text: &str,
    nicknames: &[String],
    target: &DiscussionTarget,
) -> Result<Translation, String> {
    let store = app.state::<GlobalSettingsStore>();
    let ai = app.state::<AiApiIntegrations>();
    let turns = app.state::<TurnRegistry>();
    let found = document_language::document_language(roots, target, None);
    document_language::log_document_language(app, turns.buffer(), "translation", target, found);
    let document = found.name;
    let spoken = preferred_spoken_language(&store);
    if already_in(spoken.as_ref(), text, document) {
        // AGC-FR-SBGB: counts and the language, never the text.
        logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "voice transcript already in the document language",
            log_fields! {
                "documentLanguage" => document,
                "called" => false,
                "transcriptChars" => text.chars().count(),
            },
        );
        return Ok(Translation { text: text.to_string() });
    }
    let provider = ai_api::resolve_agent_provider(&store, project_key).map_err(|_| {
        let code = ai_api::ERR_PROVIDER_UNCONFIGURED;
        log_translation_failure(app, &turns, ("", ""), document, false, code);
        ai_api::ERR_PROVIDER_UNCONFIGURED.to_string()
    })?;
    let model = ai_api::default_model_of(&store, &provider).unwrap_or_default();
    let endpoint =
        ai_api::resolve_ai_api_endpoint(&store, &ai, &provider, &model, None).map_err(|e| {
            log_translation_failure(app, &turns, (&provider, &model), document, false, &e);
            ai_api::ERR_PROVIDER_UNCONFIGURED.to_string()
        })?;
    let spoken_name = spoken.as_ref().map_or("Not known", |language| language.name.as_str());
    let request = translation_request(spoken_name, document, nicknames, text);
    let started = std::time::Instant::now();
    let outcome = turns.seam.complete(&request, &[], &endpoint, TRANSLATE_LIMIT);
    let reply = match outcome {
        Ok(reply) if !reply.refused && !reply.text.trim().is_empty() => {
            reply.text.trim().to_string()
        }
        Ok(_) => {
            log_translation_failure(
                app,
                &turns,
                (&provider, &model),
                document,
                true,
                "empty_reply",
            );
            return Err(ERR_TRANSLATION_FAILED.to_string());
        }
        Err(failure) => {
            log_translation_failure(
                app,
                &turns,
                (&provider, &model),
                document,
                true,
                failure.class,
            );
            return Err(ERR_TRANSLATION_FAILED.to_string());
        }
    };
    // AGC-FR-SBGB: counts and the duration, never the text.
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Remote],
        "voice transcript translated",
        log_fields! {
            "provider" => &provider,
            "model" => &model,
            "documentLanguage" => document,
            "called" => true,
            "transcriptChars" => text.chars().count(),
            "translationChars" => reply.chars().count(),
            "durationMs" => started.elapsed().as_millis() as u64,
        },
    );
    Ok(Translation { text: reply })
}

/// AGC-FR-SBGB: a typed reason, and never the text.
fn log_translation_failure<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    (provider, model): (&str, &str),
    document: &str,
    called: bool,
    code: &str,
) {
    logging::log_warn(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Remote],
        "voice transcript translation failed",
        log_fields! {
            "provider" => provider,
            "model" => model,
            "documentLanguage" => document,
            "called" => called,
            "code" => code,
        },
    );
}

/// AGC-FR-QEOF.
#[tauri::command]
pub async fn translate_voice_transcript<R: tauri::Runtime>(
    text: String,
    nicknames: Vec<String>,
    target: DiscussionTarget,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
) -> Result<Translation, String> {
    let key = project.anchor().ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    let store = project.require_store()?;
    tauri::async_runtime::spawn_blocking(move || {
        let roots = Roots { worktree: &root, store: &store };
        translate_impl(&app, roots, &key, &text, &nicknames, &target)
    })
    .await
    .map_err(|_| ERR_TRANSLATION_FAILED.to_string())?
}

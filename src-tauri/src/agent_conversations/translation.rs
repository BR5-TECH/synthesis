//! "translate voice transcript": the transcript of a voice comment in English
//! (AGC-FR-QEOF, AGC-FR-SBGB).

use super::*;
use crate::voice_ai::languages::spoken_language;
use tauri::Manager;

/// AGC-FR-QEOF: the fixed prompt of the call.
pub const TRANSLATE_PROMPT_TEMPLATE: &str =
    include_str!("../../../resources/prompts/translate-transcript.md");

/// AGC-FR-QEOF: the limit of the call.
const TRANSLATE_LIMIT: Duration = Duration::from_secs(30);

pub const ERR_NO_SPOKEN_LANGUAGE: &str = "no_spoken_language";
pub const ERR_TRANSLATION_FAILED: &str = "translation_failed";

/// CVL-FR-FQMM: the rule of the spoken language that the app preferences
/// hold, or `""` where they hold none.
pub fn spoken_language_rule(store: &GlobalSettingsStore) -> String {
    let language = store
        .load_app_preferences()
        .ok()
        .and_then(|prefs| spoken_language(&prefs.voice_spoken_language));
    match language {
        Some(language) => {
            let name = language.name;
            format!(
                "The author speaks {name}. Write the <spoken> part in {name}. Keep the comment \
                 body and every document change in English."
            )
        }
        None => String::new(),
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
pub fn translation_request(language: &str, nicknames: &[String], text: &str) -> AgentRequest {
    AgentRequest {
        instructions: crate::prompts::instruction_text(TRANSLATE_PROMPT_TEMPLATE),
        input: vec![
            section("language", language.to_string()),
            section("names", nicknames.join(", ")),
            section("transcript", text.to_string()),
        ],
        ..Default::default()
    }
}

/// AGC-FR-QEOF: one model call with no tools, on the provider of the project
/// and its selected model.
pub fn translate_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project_key: &str,
    text: &str,
    nicknames: &[String],
) -> Result<Translation, String> {
    let store = app.state::<GlobalSettingsStore>();
    let ai = app.state::<AiApiIntegrations>();
    let turns = app.state::<TurnRegistry>();
    let prefs = store.load_app_preferences().map_err(|_| ERR_NO_SPOKEN_LANGUAGE.to_string())?;
    let language = spoken_language(&prefs.voice_spoken_language).ok_or(ERR_NO_SPOKEN_LANGUAGE)?;
    let provider = ai_api::resolve_agent_provider(&store, project_key)
        .map_err(|_| ai_api::ERR_PROVIDER_UNCONFIGURED.to_string())?;
    let model = ai_api::default_model_of(&store, &provider).unwrap_or_default();
    let endpoint =
        ai_api::resolve_ai_api_endpoint(&store, &ai, &provider, &model, None).map_err(|e| {
            log_translation_failure(app, &turns, &provider, &model, &e);
            ai_api::ERR_PROVIDER_UNCONFIGURED.to_string()
        })?;
    let request = translation_request(&language.name, nicknames, text);
    let started = std::time::Instant::now();
    let outcome = turns.seam.complete(&request, &[], &endpoint, TRANSLATE_LIMIT);
    let reply = match outcome {
        Ok(reply) if !reply.refused && !reply.text.trim().is_empty() => {
            reply.text.trim().to_string()
        }
        Ok(_) => {
            log_translation_failure(app, &turns, &provider, &model, "empty_reply");
            return Err(ERR_TRANSLATION_FAILED.to_string());
        }
        Err(failure) => {
            log_translation_failure(app, &turns, &provider, &model, failure.class);
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
    provider: &str,
    model: &str,
    code: &str,
) {
    logging::log_warn(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Remote],
        "voice transcript translation failed",
        log_fields! { "provider" => provider, "model" => model, "code" => code },
    );
}

/// AGC-FR-QEOF.
#[tauri::command]
pub async fn translate_voice_transcript<R: tauri::Runtime>(
    text: String,
    nicknames: Vec<String>,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
) -> Result<Translation, String> {
    let key = project.anchor().ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    tauri::async_runtime::spawn_blocking(move || translate_impl(&app, &key, &text, &nicknames))
        .await
        .map_err(|_| ERR_TRANSLATION_FAILED.to_string())?
}

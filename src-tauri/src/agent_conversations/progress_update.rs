//! "write voice progress update": the short phrase that voice mode speaks while
//! a long turn runs (AGC-FR-YMFC, AGC-FR-PGSJ, AGC-FR-TUNO).

use super::*;
use tauri::Manager;

/// AGC-FR-YMFC: the fixed prompt of the call.
pub const PROGRESS_PROMPT: &str =
    include_str!("../../../resources/prompts/voice-progress-update.md");

/// AGC-FR-YMFC: the limit of the call.
const PROGRESS_LIMIT: Duration = Duration::from_secs(20);

/// AGC-FR-PGSJ: the most words of an update.
pub const PROGRESS_WORD_LIMIT: usize = 5;

/// AGC-FR-PGSJ: the typed refusal of a call that gave no usable phrase.
pub const ERR_PROGRESS_UPDATE_FAILED: &str = "progress_update_failed";

/// The most activity entries that one call reads.
const ACTIVITY_LIMIT: usize = 20;

/// AGC-FR-YMFC: one active tool call of the running turns of an utterance.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct ActivityEntry {
    pub tool: String,
    #[serde(default)]
    pub advisor: Option<String>,
}

/// What the command answers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ProgressUpdate {
    pub text: String,
}

/// A tool name or a nickname reaches the prompt as one plain word.
fn plain(value: &str) -> String {
    value.chars().filter(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.')).take(64).collect()
}

/// AGC-FR-YMFC: the request of one update. The input is the activity alone.
pub fn progress_request(activity: &[ActivityEntry], spoken_language: &str) -> AgentRequest {
    let lines: Vec<String> = activity
        .iter()
        .take(ACTIVITY_LIMIT)
        .map(|entry| match entry.advisor.as_deref().map(plain).filter(|a| !a.is_empty()) {
            Some(advisor) => format!("- {} (advisor @{advisor})", plain(&entry.tool)),
            None => format!("- {}", plain(&entry.tool)),
        })
        .collect();
    let section = |tag: &str, body: String| InputSection {
        tag: tag.to_string(),
        attributes: Vec::new(),
        body,
        truncated: false,
        parts: Vec::new(),
    };
    AgentRequest {
        instructions: crate::prompts::instruction_text(PROGRESS_PROMPT),
        input: vec![
            section("language", spoken_language.trim().to_string()),
            section("activity", lines.join("\n")),
        ],
        ..Default::default()
    }
}

/// AGC-FR-PGSJ: the first line of the reply, trimmed, with a pair of quotation
/// marks removed, and cut to its first words. `None` for a reply with no text.
pub fn clean_update(reply: &str) -> Option<String> {
    let line = reply.lines().map(str::trim).find(|line| !line.is_empty())?;
    let unquoted = line
        .strip_prefix(['"', '\u{201c}', '\u{ab}', '\''])
        .and_then(|rest| rest.strip_suffix(['"', '\u{201d}', '\u{bb}', '\'']))
        .unwrap_or(line)
        .trim();
    let words: Vec<&str> = unquoted.split_whitespace().take(PROGRESS_WORD_LIMIT).collect();
    (!words.is_empty()).then(|| words.join(" "))
}

/// AGC-FR-TUNO: a typed reason, and never a text.
fn log_failure<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    (provider, model): (&str, &str),
    entries: usize,
    code: &str,
) {
    logging::log_warn(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Remote],
        "voice progress update failed",
        log_fields! {
            "provider" => provider,
            "model" => model,
            "activityEntries" => entries,
            "code" => code,
        },
    );
}

/// AGC-FR-YMFC, AGC-FR-PGSJ: one model call with no tools, on the provider of
/// the project and the model stored for the AI API (AAP-FR-EDBQ).
pub fn progress_update_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    project_key: &str,
    activity: &[ActivityEntry],
    spoken_language: &str,
) -> Result<ProgressUpdate, String> {
    let store = app.state::<GlobalSettingsStore>();
    let ai = app.state::<AiApiIntegrations>();
    let turns = app.state::<TurnRegistry>();
    let entries = activity.len().min(ACTIVITY_LIMIT);
    let endpoint = ai_api::resolve_ai_api_call(&store, &ai, project_key).map_err(|_| {
        log_failure(app, &turns, ("", ""), entries, ai_api::ERR_PROVIDER_UNCONFIGURED);
        ai_api::ERR_PROVIDER_UNCONFIGURED.to_string()
    })?;
    let model = endpoint.model_id.clone().unwrap_or_default();
    let request = progress_request(activity, spoken_language);
    let started = std::time::Instant::now();
    let reply = match turns.seam.complete(&request, &[], &endpoint, PROGRESS_LIMIT) {
        Ok(reply) if !reply.refused => reply.text,
        Ok(_) => {
            log_failure(app, &turns, (&endpoint.provider, &model), entries, "refused");
            return Err(ERR_PROGRESS_UPDATE_FAILED.to_string());
        }
        Err(failure) => {
            log_failure(app, &turns, (&endpoint.provider, &model), entries, failure.class);
            return Err(ERR_PROGRESS_UPDATE_FAILED.to_string());
        }
    };
    let Some(text) = clean_update(&reply) else {
        log_failure(app, &turns, (&endpoint.provider, &model), entries, "empty_reply");
        return Err(ERR_PROGRESS_UPDATE_FAILED.to_string());
    };
    // AGC-FR-TUNO: counts and the duration, never the text.
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Remote],
        "voice progress update written",
        log_fields! {
            "provider" => &endpoint.provider,
            "model" => &model,
            "activityEntries" => entries,
            "words" => text.split_whitespace().count(),
            "durationMs" => started.elapsed().as_millis() as u64,
        },
    );
    Ok(ProgressUpdate { text })
}

/// AGC-FR-YMFC.
#[tauri::command]
pub async fn write_voice_progress_update<R: tauri::Runtime>(
    activity: Vec<ActivityEntry>,
    spoken_language: String,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
) -> Result<ProgressUpdate, String> {
    let key = project.anchor().ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        progress_update_impl(&app, &key, &activity, &spoken_language)
    })
    .await
    .map_err(|_| ERR_PROGRESS_UPDATE_FAILED.to_string())?
}

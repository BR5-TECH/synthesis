//! The Codex model probe (`AIC-agentic-integrations.md` AIC-FR-TBTH).
//!
//! `codex debug models` prints the installation's own catalog as one JSON
//! document, `{"models":[...]}` (`../infra/CDX-codex-cli-protocol.md`
//! CDX-FR-BQNL). The document is large — each model carries its instruction
//! templates — so the shapes below name only the fields this module reads,
//! and serde skips the rest.

use super::*;

/// The part of the probe document this module reads.
#[derive(Deserialize)]
struct CodexCatalog {
    #[serde(default)]
    models: Vec<CodexModel>,
}

/// One model entry. Every field has a default, so an entry that lacks one is
/// read rather than refused: the probe is enrichment (AIC-FR-08).
#[derive(Deserialize, Default)]
#[serde(default)]
struct CodexModel {
    slug: String,
    display_name: Option<String>,
    visibility: Option<String>,
    default_reasoning_level: Option<String>,
    supported_reasoning_levels: Vec<CodexLevel>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct CodexLevel {
    effort: String,
}

/// A model id or an effort id this module may carry onward. The executor puts
/// both into a quoted TOML override, so an id with a quote, a backslash, or a
/// space never reaches it.
fn plausible_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 100
        && value.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_'))
}

/// AIC-FR-TBTH: the offered models of a `codex debug models` answer.
///
/// Keeps only models whose `visibility` is `list`, keeps the first entry of a
/// repeated slug, and drops any effort id outside the six levels of
/// `../infra/CDX-codex-cli-protocol.md` CDX-FR-SDOV. A model whose ladder is
/// then empty declares no ladder. A
/// document that does not parse yields no model, which the caller reads as a
/// failed probe and answers with the bundled catalog.
pub fn parse_codex_models(output: &CliOutput) -> Vec<ModelOption> {
    let Ok(catalog) = serde_json::from_str::<CodexCatalog>(output.stdout.trim()) else {
        return Vec::new();
    };
    let mut models: Vec<ModelOption> = Vec::new();
    for entry in catalog.models {
        if entry.visibility.as_deref() != Some("list") || !plausible_id(&entry.slug) {
            continue;
        }
        if models.iter().any(|m| m.id == entry.slug) {
            continue;
        }
        let mut efforts: Vec<String> = Vec::new();
        for level in entry.supported_reasoning_levels {
            // CDX-FR-SDOV: only the six levels the CLI accepts are carried
            // onward. A level outside them would be offered, stored, and then
            // dropped at launch, so it is not offered at all.
            let known = EFFORTS_CODEX.iter().any(|(id, _)| *id == level.effort);
            if known && !efforts.contains(&level.effort) {
                efforts.push(level.effort);
            }
        }
        let label = entry
            .display_name
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| entry.slug.clone());
        let mut option = ModelOption::new(&entry.slug, &label);
        if !efforts.is_empty() {
            // A default the ladder does not hold is dropped rather than kept,
            // because `ModelReasoning` promises it is a member of the ladder.
            let default_effort = entry.default_reasoning_level.filter(|d| efforts.contains(d));
            option.reasoning = Some(ModelReasoning {
                mandatory: true,
                default_enabled: None,
                supported_efforts: Some(efforts),
                default_effort,
            });
        }
        models.push(option);
    }
    models
}

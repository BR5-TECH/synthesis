//! The models the Global Indexing section offers (EMB-FR-MIDG, EMB-FR-XTLI,
//! EMB-FR-MHHY).

use serde::Serialize;

/// EMB-FR-XTLI: BGE-small-en-v1.5.
pub const DEFAULT_MODEL_ID: &str = "BGESmallENV15";

/// EMB-FR-MIDG: the recommended models, in the order the picker shows them,
/// with their display names.
const RECOMMENDED: [(&str, &str); 4] = [
    ("BGESmallENV15", "BGE-small-en-v1.5"),
    ("AllMiniLML6V2", "all-MiniLM-L6-v2"),
    ("EmbeddingGemma300M", "EmbeddingGemma 300M"),
    ("BGEM3", "BGE-M3"),
];

/// The models that only the Candle backend of FastEmbed runs: id, display
/// name, HuggingFace repository, and vector length.
const CANDLE_MODELS: [(&str, &str, &str, usize, CandleKind); 4] = [
    (
        "Qwen3Embedding06B",
        "Qwen3-Embedding-0.6B",
        "Qwen/Qwen3-Embedding-0.6B",
        1024,
        CandleKind::Qwen3,
    ),
    ("Qwen3Embedding4B", "Qwen3-Embedding-4B", "Qwen/Qwen3-Embedding-4B", 2560, CandleKind::Qwen3),
    ("Qwen3Embedding8B", "Qwen3-Embedding-8B", "Qwen/Qwen3-Embedding-8B", 4096, CandleKind::Qwen3),
    (
        "NomicEmbedTextV2MoE",
        "nomic-embed-text-v2-moe",
        "nomic-ai/nomic-embed-text-v2-moe",
        768,
        CandleKind::NomicV2Moe,
    ),
];

/// The tokenizer files `fastembed` reads for an ONNX model.
const TOKENIZER_FILES: [&str; 4] =
    ["tokenizer.json", "config.json", "special_tokens_map.json", "tokenizer_config.json"];

/// EMB-FR-MHHY: the backend that runs a model.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    Candle,
    Onnx,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CandleKind {
    Qwen3,
    NomicV2Moe,
}

/// One entry of `list_embedding_models` (EMB contract surface).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingModelEntry {
    pub id: String,
    pub name: String,
    pub dimension: usize,
    pub backend: Backend,
    pub recommended: bool,
    pub default: bool,
}

/// How a model is fetched and loaded.
#[derive(Clone, Debug)]
pub enum ModelSource {
    /// An ONNX model of `fastembed::TextEmbedding`, with the files its cache
    /// must hold.
    Onnx { model: fastembed::EmbeddingModel, repo: String, files: Vec<String> },
    /// A Candle model, loaded from its repository.
    Candle { kind: CandleKind, repo: String },
}

/// A model of the catalogue, with what loading it needs.
#[derive(Clone, Debug)]
pub struct ModelSpec {
    pub entry: EmbeddingModelEntry,
    pub source: ModelSource,
}

/// EMB-FR-MIDG: every model of this build, the recommended four first in their
/// fixed order, the others sorted by name.
pub fn list() -> Vec<EmbeddingModelEntry> {
    all_specs().into_iter().map(|spec| spec.entry).collect()
}

/// The model with `id`, or `None` for an id this build does not know.
pub fn find(id: &str) -> Option<ModelSpec> {
    all_specs().into_iter().find(|spec| spec.entry.id == id)
}

fn all_specs() -> Vec<ModelSpec> {
    let mut onnx: Vec<ModelSpec> = fastembed::TextEmbedding::list_supported_models()
        .into_iter()
        .map(|info| {
            let id = format!("{:?}", info.model);
            let recommended = RECOMMENDED.iter().find(|(rid, _)| *rid == id);
            let mut files = vec![info.model_file.clone()];
            files.extend(info.additional_files.iter().cloned());
            files.extend(TOKENIZER_FILES.iter().map(|f| f.to_string()));
            ModelSpec {
                entry: EmbeddingModelEntry {
                    name: recommended
                        .map(|(_, name)| name.to_string())
                        .unwrap_or_else(|| display_name(&info.model_code, &id)),
                    default: id == DEFAULT_MODEL_ID,
                    recommended: recommended.is_some(),
                    id,
                    dimension: info.dim,
                    backend: Backend::Onnx,
                },
                source: ModelSource::Onnx {
                    model: info.model.clone(),
                    repo: info.model_code.clone(),
                    files,
                },
            }
        })
        .collect();
    let candle = CANDLE_MODELS.iter().map(|(id, name, repo, dimension, kind)| ModelSpec {
        entry: EmbeddingModelEntry {
            id: id.to_string(),
            name: name.to_string(),
            dimension: *dimension,
            backend: Backend::Candle,
            recommended: false,
            default: false,
        },
        source: ModelSource::Candle { kind: *kind, repo: repo.to_string() },
    });
    onnx.extend(candle);

    let rank = |spec: &ModelSpec| {
        RECOMMENDED.iter().position(|(id, _)| *id == spec.entry.id).unwrap_or(RECOMMENDED.len())
    };
    onnx.sort_by(|a, b| {
        rank(a)
            .cmp(&rank(b))
            .then_with(|| a.entry.name.to_lowercase().cmp(&b.entry.name.to_lowercase()))
            .then_with(|| a.entry.id.cmp(&b.entry.id))
    });
    onnx
}

/// A readable name for a model the picker does not name itself: the
/// repository name, marked when the variant is quantized.
fn display_name(model_code: &str, id: &str) -> String {
    let base = model_code.rsplit('/').next().unwrap_or(model_code);
    let base = base.trim_end_matches("-onnx-Q").trim_end_matches("-onnx").trim_end_matches("-ONNX");
    if id.ends_with('Q') || id.ends_with("Q4") {
        format!("{base} (quantized{})", if id.ends_with("Q4") { ", 4-bit" } else { "" })
    } else {
        base.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // EMB-FR-MIDG, EMB-FR-XTLI: the recommended four come first, in order,
    // marked, and the default is the first of them.
    #[test]
    fn recommended_models_come_first_and_the_default_is_bge_small() {
        let models = list();
        let first: Vec<&str> = models.iter().take(4).map(|m| m.name.as_str()).collect();
        assert_eq!(
            first,
            ["BGE-small-en-v1.5", "all-MiniLM-L6-v2", "EmbeddingGemma 300M", "BGE-M3"]
        );
        assert!(models.iter().take(4).all(|m| m.recommended));
        assert!(models.iter().skip(4).all(|m| !m.recommended));
        let defaults: Vec<&str> =
            models.iter().filter(|m| m.default).map(|m| m.id.as_str()).collect();
        assert_eq!(defaults, [DEFAULT_MODEL_ID]);
        assert_eq!(models[0].id, DEFAULT_MODEL_ID);
        assert_eq!(models[0].dimension, 384);
    }

    // EMB-FR-MIDG: the others follow sorted by name, and every id is unique.
    #[test]
    fn other_models_are_sorted_by_name_and_ids_are_unique() {
        let models = list();
        let rest: Vec<String> = models.iter().skip(4).map(|m| m.name.to_lowercase()).collect();
        let mut sorted = rest.clone();
        sorted.sort();
        assert_eq!(rest, sorted);
        let mut ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), models.len());
    }

    // EMB-FR-MHHY: each entry names its backend; the recommended four run on
    // ONNX Runtime and the Qwen3 and Nomic v2 MoE models on Candle.
    #[test]
    fn each_model_names_its_backend() {
        let models = list();
        assert!(models.iter().take(4).all(|m| m.backend == Backend::Onnx));
        let candle: Vec<&str> =
            models.iter().filter(|m| m.backend == Backend::Candle).map(|m| m.id.as_str()).collect();
        assert_eq!(candle.len(), 4);
        assert!(candle.contains(&"NomicEmbedTextV2MoE"));
        assert!(find("unknown-model").is_none());
        assert!(matches!(find("BGEM3").unwrap().source, ModelSource::Onnx { .. }));
    }
}

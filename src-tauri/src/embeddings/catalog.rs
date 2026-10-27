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

/// EMB-FR-QQER: the text a model expects before a query and before a
/// passage, as its model card defines it. Empty for a model with no prefix.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Prefixes {
    pub query: &'static str,
    pub passage: &'static str,
}

impl Prefixes {
    /// The text a query is embedded as.
    pub fn query(&self, query: &str) -> String {
        format!("{}{query}", self.query)
    }

    /// The texts a batch of chunks is embedded as.
    pub fn passages(&self, texts: &[String]) -> Vec<String> {
        if self.passage.is_empty() {
            return texts.to_vec();
        }
        texts.iter().map(|text| format!("{}{text}", self.passage)).collect()
    }
}

/// The BGE English instruction for a retrieval query, which mxbai and
/// Snowflake Arctic use too.
const BGE_QUERY: &str = "Represent this sentence for searching relevant passages: ";

/// EMB-FR-QQER: the prefixes of the model with `id`. A model that is not
/// named here uses none. A change here is a change of the store schema
/// version (`store::SCHEMA_VERSION`).
pub fn prefixes_for(id: &str) -> Prefixes {
    let starts = |prefix: &str| id.starts_with(prefix);
    let bge = ["BGESmallENV15", "BGEBaseENV15", "BGELargeENV15", "MxbaiEmbedLargeV1"];
    if bge.iter().any(|name| starts(name)) || starts("SnowflakeArcticEmbed") {
        Prefixes { query: BGE_QUERY, passage: "" }
    } else if starts("EmbeddingGemma300M") {
        Prefixes { query: "task: search result | query: ", passage: "title: none | text: " }
    } else if starts("MultilingualE5") {
        Prefixes { query: "query: ", passage: "passage: " }
    } else if starts("NomicEmbedText") {
        Prefixes { query: "search_query: ", passage: "search_document: " }
    } else if starts("Qwen3Embedding") {
        Prefixes {
            query: "Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery:",
            passage: "",
        }
    } else {
        Prefixes::default()
    }
}

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
    pub prefixes: Prefixes,
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
            let prefixes = prefixes_for(&id);
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
                prefixes,
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
        prefixes: prefixes_for(id),
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

    // EMB-FR-QQER: the prefixes of each model family, from its model card; a
    // model with none, or an unknown id, gets none.
    #[test]
    fn each_model_family_has_its_prefixes() {
        let bge = Prefixes { query: BGE_QUERY, passage: "" };
        for id in ["BGESmallENV15", "BGESmallENV15Q", "BGEBaseENV15", "MxbaiEmbedLargeV1"] {
            assert_eq!(prefixes_for(id), bge, "{id}");
        }
        assert_eq!(prefixes_for("SnowflakeArcticEmbedM"), bge);
        assert_eq!(
            prefixes_for("EmbeddingGemma300M"),
            Prefixes { query: "task: search result | query: ", passage: "title: none | text: " }
        );
        assert_eq!(prefixes_for("MultilingualE5Small").passage, "passage: ");
        assert_eq!(prefixes_for("NomicEmbedTextV15").query, "search_query: ");
        assert_eq!(prefixes_for("NomicEmbedTextV2MoE").passage, "search_document: ");
        assert!(prefixes_for("Qwen3Embedding06B").query.starts_with("Instruct: "));
        for id in ["BGEM3", "AllMiniLML6V2", "unknown-model"] {
            assert_eq!(prefixes_for(id), Prefixes::default(), "{id}");
        }
    }

    // EMB-FR-QQER: a prefix goes before the text; no passage prefix leaves the
    // texts as they are; every catalogue entry carries the prefixes of its id.
    #[test]
    fn prefixes_go_before_the_text_and_every_entry_carries_them() {
        let gemma = prefixes_for("EmbeddingGemma300M");
        assert_eq!(gemma.query("rate limits"), "task: search result | query: rate limits");
        assert_eq!(gemma.passages(&["a".to_string()]), ["title: none | text: a"]);
        let bge = prefixes_for("BGESmallENV15");
        let texts = vec!["# Title\n\nBody.".to_string()];
        assert_eq!(bge.passages(&texts), texts);
        for spec in all_specs() {
            assert_eq!(spec.prefixes, prefixes_for(&spec.entry.id), "{}", spec.entry.id);
        }
    }

    // EMB-FR-QQER, EMB-FR-UJLS: the prefixes are part of the store schema. A
    // change of this digest without a new `SCHEMA_VERSION` is a defect: the
    // stored vectors would mix two prefix sets.
    #[test]
    fn a_change_of_the_prefixes_needs_a_new_schema_version() {
        let mut text = String::new();
        // Only the models with a prefix: a new model with none changes nothing.
        for spec in all_specs().into_iter().filter(|spec| spec.prefixes != Prefixes::default()) {
            text.push_str(&format!(
                "{}|{}|{}\n",
                spec.entry.id, spec.prefixes.query, spec.prefixes.passage
            ));
        }
        let digest = crate::fs::sha256_bytes(text.as_bytes());
        assert_eq!(
            (super::super::store::SCHEMA_VERSION, digest.as_str()),
            ("2", PREFIX_DIGEST),
            "update SCHEMA_VERSION and this digest together"
        );
    }

    // EMB-FR-YGXX: an aarch64 Linux build enables `fp16`, which the half-precision
    // `asm!` under Candle needs. No other target and no `build.rustflags` gets
    // it, so no other build starts to need a CPU feature its target does not
    // promise. That Cargo applies the flag is shown only by a real aarch64
    // Linux build from `src-tauri/`; no suite here compiles that target.
    #[test]
    fn only_the_aarch64_linux_build_enables_fp16() {
        const TRIPLE: &str = "aarch64-unknown-linux-gnu";
        let config: toml::Value = toml::from_str(include_str!("../../.cargo/config.toml")).unwrap();
        assert_eq!(
            config["target"][TRIPLE]["rustflags"],
            toml::Value::Array(vec!["-C".into(), "target-feature=+fp16".into()])
        );
        let names_fp16 = |table: &toml::Value| {
            table.get("rustflags").is_some_and(|flags| flags.to_string().contains("fp16"))
        };
        for (target, table) in config["target"].as_table().unwrap() {
            assert!(target == TRIPLE || !names_fp16(table), "{target} also enables fp16");
        }
        assert!(
            !config.get("build").is_some_and(names_fp16),
            "build.rustflags enables fp16 everywhere"
        );
    }

    /// The digest of the prefixes that schema version 2 was built with.
    const PREFIX_DIGEST: &str = "2a4069fefb421ace914edf6bea331baaa3e1376c063dc8e7f8709167b2642759";
}

//! Model download, cache, and inference (EMB-FR-NNOK, EMB-FR-LYKW,
//! EMB-FR-MHHY, EMB-FR-OJRK).

use std::path::PathBuf;

use super::catalog::{CandleKind, ModelSource, ModelSpec};

/// EMB-FR-OJRK: the input limit, in tokens, every model is loaded with. A
/// longer chunk is truncated by the tokenizer; the chunk itself is unchanged.
pub const MAX_TOKENS: usize = 512;

/// Turns texts into vectors.
pub trait Embedder: Send {
    fn embed(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
}

/// EMB-FR-LYKW: `text` with the query string of every URL in it removed, so
/// a signed download link in an error does not reach the status or the log.
pub fn without_url_queries(text: &str) -> String {
    text.split(' ')
        .map(|word| match (word.contains("://"), word.find('?')) {
            (true, Some(at)) => &word[..at],
            _ => word,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Download progress in bytes (EMB-FR-LYKW).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DownloadProgress {
    pub completed: u64,
    pub total: Option<u64>,
}

/// Makes a model ready: downloads what the cache does not hold, then loads it.
pub trait EmbedderFactory: Send + Sync {
    /// Whether the cache holds every file of the model, so that no network
    /// access is needed to load it (EMB-FR-NNOK).
    fn is_cached(&self, spec: &ModelSpec) -> bool;

    fn download(
        &self,
        spec: &ModelSpec,
        progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<(), String>;

    fn load(&self, spec: &ModelSpec) -> Result<Box<dyn Embedder>, String>;
}

/// The production factory: HuggingFace downloads into `cache_dir`, FastEmbed
/// inference.
pub struct FastEmbedFactory {
    /// EMB-FR-KUFM: `app_data_dir()/models/`, or the reason it is not
    /// available. Without it, nothing downloads and nothing loads.
    pub cache_dir: Result<PathBuf, String>,
}

impl FastEmbedFactory {
    fn cache_dir(&self) -> Result<PathBuf, String> {
        self.cache_dir.clone()
    }

    fn api(&self) -> Result<hf_hub::api::sync::Api, String> {
        hf_hub::api::sync::ApiBuilder::new()
            .with_cache_dir(self.cache_dir()?)
            .with_progress(false)
            .with_token(std::env::var("HF_TOKEN").ok().filter(|t| !t.trim().is_empty()))
            .build()
            .map_err(|e| format!("hub client: {e}"))
    }

    fn cache_repo(&self, repo: &str) -> Result<hf_hub::CacheRepo, String> {
        Ok(hf_hub::Cache::new(self.cache_dir()?).model(repo.to_string()))
    }

    /// The files a model needs. The weights of a Candle model are one file or
    /// the shards its index names.
    fn files(
        &self,
        spec: &ModelSpec,
        api: Option<&hf_hub::api::sync::Api>,
    ) -> Result<(String, Vec<String>), String> {
        match &spec.source {
            ModelSource::Onnx { repo, files, .. } => Ok((repo.clone(), files.clone())),
            ModelSource::Candle { repo, .. } => {
                let mut files = vec!["config.json".to_string(), "tokenizer.json".to_string()];
                files.extend(self.weight_files(repo, api)?);
                Ok((repo.clone(), files))
            }
        }
    }

    fn weight_files(
        &self,
        repo: &str,
        api: Option<&hf_hub::api::sync::Api>,
    ) -> Result<Vec<String>, String> {
        const SINGLE: &str = "model.safetensors";
        const INDEX: &str = "model.safetensors.index.json";
        let cache = self.cache_repo(repo)?;
        if cache.get(SINGLE).is_some() {
            return Ok(vec![SINGLE.to_string()]);
        }
        let index_path = match cache.get(INDEX) {
            Some(path) => Some(path),
            None => match api {
                Some(api) => {
                    let remote = api.model(repo.to_string());
                    if api.metadata(&remote.url(SINGLE)).is_ok() {
                        return Ok(vec![SINGLE.to_string()]);
                    }
                    Some(remote.get(INDEX).map_err(|e| format!("weights index: {e}"))?)
                }
                None => None,
            },
        };
        let Some(index_path) = index_path else {
            return Ok(vec![SINGLE.to_string()]);
        };
        // FSA-FR-19 exception, deliberate: the index lives in the model cache
        // that `hf-hub` owns under `app_data_dir()/models/`.
        let bytes = std::fs::read(index_path).map_err(|e| format!("weights index: {e}"))?;
        let index: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|e| format!("weights index: {e}"))?;
        let mut shards: Vec<String> = index["weight_map"]
            .as_object()
            .map(|map| map.values().filter_map(|v| v.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        shards.sort_unstable();
        shards.dedup();
        shards.insert(0, INDEX.to_string());
        Ok(shards)
    }
}

/// Counts the bytes `hf-hub` reports for one file into the running total.
struct Counter<'a> {
    base: u64,
    total: Option<u64>,
    done: &'a std::cell::Cell<u64>,
    report: &'a mut dyn FnMut(DownloadProgress),
}

impl hf_hub::api::Progress for Counter<'_> {
    fn init(&mut self, _size: usize, _filename: &str) {}

    fn update(&mut self, size: usize) {
        self.done.set(self.done.get() + size as u64);
        (self.report)(DownloadProgress {
            completed: self.base + self.done.get(),
            total: self.total,
        });
    }

    fn finish(&mut self) {}
}

impl EmbedderFactory for FastEmbedFactory {
    fn is_cached(&self, spec: &ModelSpec) -> bool {
        let Ok((repo, files)) = self.files(spec, None) else {
            return false;
        };
        let Ok(cache) = self.cache_repo(&repo) else {
            return false;
        };
        files.iter().all(|file| cache.get(file).is_some())
    }

    fn download(
        &self,
        spec: &ModelSpec,
        progress: &mut dyn FnMut(DownloadProgress),
    ) -> Result<(), String> {
        let api = self.api()?;
        let (repo, files) = self.files(spec, Some(&api))?;
        let cache = self.cache_repo(&repo)?;
        let remote = api.model(repo.clone());
        let missing: Vec<&String> = files.iter().filter(|file| cache.get(file).is_none()).collect();
        // The total is known before the first byte: one ranged request per file.
        let mut total = 0u64;
        for file in &missing {
            let metadata = api.metadata(&remote.url(file)).map_err(|e| format!("{file}: {e}"))?;
            total += metadata.size() as u64;
        }
        let mut completed = 0u64;
        progress(DownloadProgress { completed, total: Some(total) });
        for file in missing {
            let done = std::cell::Cell::new(0u64);
            let counter = Counter {
                base: completed,
                total: Some(total),
                done: &done,
                report: &mut *progress,
            };
            // `hf-hub` writes into a temporary file and renames it into the
            // cache only when it is whole, so a failed download leaves no file
            // that a later load could read as cached (EMB-FR-LYKW).
            remote.download_with_progress(file, counter).map_err(|e| format!("{file}: {e}"))?;
            completed += done.get();
        }
        progress(DownloadProgress { completed: total.max(completed), total: Some(total) });
        Ok(())
    }

    fn load(&self, spec: &ModelSpec) -> Result<Box<dyn Embedder>, String> {
        // EMB-FR-KUFM: with no cache folder, no backend may fall back to a
        // folder of its own and download there.
        let cache_dir = self.cache_dir()?;
        match &spec.source {
            ModelSource::Onnx { model, .. } => {
                // `ort` allows one environment per process, and the voice
                // detector needs it too.
                let options = fastembed::TextInitOptions::new(model.clone())
                    .with_environment(crate::onnx_runtime::shared_environment()?)
                    .with_cache_dir(cache_dir)
                    .with_max_length(MAX_TOKENS)
                    .with_show_download_progress(false);
                let model =
                    fastembed::TextEmbedding::try_new(options).map_err(|e| format!("load: {e}"))?;
                Ok(Box::new(OnnxEmbedder(model)))
            }
            ModelSource::Candle { kind, repo } => {
                let device = candle_core::Device::Cpu;
                let dtype = candle_core::DType::F32;
                let model: Box<dyn Embedder> = match kind {
                    CandleKind::Qwen3 => Box::new(Qwen3Embedder(
                        fastembed::Qwen3TextEmbedding::from_hf(repo, &device, dtype, MAX_TOKENS)
                            .map_err(|e| format!("load: {e}"))?,
                    )),
                    CandleKind::NomicV2Moe => Box::new(NomicEmbedder(
                        fastembed::NomicV2MoeTextEmbedding::from_hf(
                            repo, &device, dtype, MAX_TOKENS,
                        )
                        .map_err(|e| format!("load: {e}"))?,
                    )),
                };
                Ok(model)
            }
        }
    }
}

struct OnnxEmbedder(fastembed::TextEmbedding);

impl Embedder for OnnxEmbedder {
    fn embed(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.0.embed(texts, Some(texts.len().max(1))).map_err(|e| format!("embed: {e}"))
    }
}

struct Qwen3Embedder(fastembed::Qwen3TextEmbedding);

impl Embedder for Qwen3Embedder {
    fn embed(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.0.embed(texts).map_err(|e| format!("embed: {e}"))
    }
}

struct NomicEmbedder(fastembed::NomicV2MoeTextEmbedding);

impl Embedder for NomicEmbedder {
    fn embed(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        self.0.embed(texts).map_err(|e| format!("embed: {e}"))
    }
}

#[cfg(test)]
pub(crate) mod fake {
    //! A deterministic embedder for tests: no download, no network, no model.

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    /// Vectors from the words of a text, so texts that share words are near.
    pub fn vector_of(text: &str) -> Vec<f32> {
        let mut v = [0.0f32; 16];
        for word in text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()) {
            let h = crate::fs::sha256_bytes(word.to_lowercase().as_bytes());
            let slot = usize::from_str_radix(&h[..2], 16).unwrap() % v.len();
            v[slot] += 1.0;
        }
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-6);
        v.iter().map(|x| x / norm).collect()
    }

    /// What a [`FakeFactory`] has done, shared with the test.
    #[derive(Default)]
    pub struct Log {
        pub embedded: AtomicUsize,
        pub downloads: AtomicUsize,
        pub texts: Mutex<Vec<String>>,
        /// When set, every embedding call fails.
        pub fail_embed: std::sync::atomic::AtomicBool,
        /// The reason a failing call gives, `"embed failed"` when unset.
        pub fail_reason: Mutex<Option<String>>,
        /// Runs once, inside the next embedding call — what a test does while
        /// a job is running, with no sleep.
        pub on_embed: Mutex<Option<Box<dyn FnOnce() + Send>>>,
        /// The size of each embedding call, in order.
        pub batches: Mutex<Vec<usize>>,
        /// Runs once, inside the next download.
        pub on_download: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    }

    pub struct FakeEmbedder {
        pub log: Arc<Log>,
        pub fail: bool,
    }

    impl Embedder for FakeEmbedder {
        fn embed(&mut self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            let hook = self.log.on_embed.lock().unwrap().take();
            if let Some(hook) = hook {
                hook();
            }
            if self.fail || self.log.fail_embed.load(Ordering::SeqCst) {
                let reason = self.log.fail_reason.lock().unwrap().clone();
                return Err(reason.unwrap_or_else(|| "embed failed".to_string()));
            }
            self.log.batches.lock().unwrap().push(texts.len());
            self.log.embedded.fetch_add(texts.len(), Ordering::SeqCst);
            self.log.texts.lock().unwrap().extend(texts.iter().cloned());
            Ok(texts.iter().map(|t| vector_of(t)).collect())
        }
    }

    pub struct FakeFactory {
        pub log: Arc<Log>,
        pub fail_download: Mutex<bool>,
        pub cached: Mutex<bool>,
    }

    impl FakeFactory {
        pub fn new() -> (FakeFactory, Arc<Log>) {
            let log = Arc::new(Log::default());
            (
                FakeFactory {
                    log: log.clone(),
                    fail_download: Mutex::new(false),
                    cached: Mutex::new(false),
                },
                log,
            )
        }
    }

    impl EmbedderFactory for FakeFactory {
        fn is_cached(&self, _spec: &ModelSpec) -> bool {
            *self.cached.lock().unwrap()
        }

        fn download(
            &self,
            _spec: &ModelSpec,
            progress: &mut dyn FnMut(DownloadProgress),
        ) -> Result<(), String> {
            self.log.downloads.fetch_add(1, Ordering::SeqCst);
            let hook = self.log.on_download.lock().unwrap().take();
            if let Some(hook) = hook {
                hook();
            }
            if *self.fail_download.lock().unwrap() {
                return Err("network unreachable".to_string());
            }
            progress(DownloadProgress { completed: 0, total: Some(10) });
            progress(DownloadProgress { completed: 10, total: Some(10) });
            *self.cached.lock().unwrap() = true;
            Ok(())
        }

        fn load(&self, _spec: &ModelSpec) -> Result<Box<dyn Embedder>, String> {
            Ok(Box::new(FakeEmbedder { log: self.log.clone(), fail: false }))
        }
    }
}

#[cfg(test)]
mod no_cache_folder {
    use super::*;

    // EMB-FR-LYKW: the query string of a URL in a failure reason is removed;
    // the rest of the reason stays.
    #[test]
    fn a_failure_reason_loses_the_query_strings_of_its_urls() {
        let reason =
            "model.onnx: https://cdn.example/x/model.onnx?X-Amz-Signature=abc&e=1: status 403";
        assert_eq!(
            without_url_queries(reason),
            "model.onnx: https://cdn.example/x/model.onnx status 403"
        );
        assert_eq!(without_url_queries("network unreachable? retry"), "network unreachable? retry");
    }

    fn factory() -> FastEmbedFactory {
        FastEmbedFactory { cache_dir: Err("the model cache folder is not available".to_string()) }
    }

    // EMB-FR-KUFM: with no `app_data_dir()`, no model is cached, no download
    // starts, and no backend loads a model from a folder of its own.
    #[test]
    fn without_a_cache_folder_nothing_downloads_and_nothing_loads() {
        let onnx = super::super::catalog::find("BGESmallENV15").unwrap();
        let candle = super::super::catalog::find("NomicEmbedTextV2MoE").unwrap();
        let factory = factory();
        assert!(!factory.is_cached(&onnx));
        assert!(!factory.is_cached(&candle));
        let mut reported = false;
        let error = factory.download(&onnx, &mut |_| reported = true).unwrap_err();
        assert!(error.contains("not available"), "{error}");
        assert!(!reported, "no progress for a download that did not start");
        assert!(factory.load(&onnx).err().unwrap().contains("not available"));
        assert!(factory.load(&candle).err().unwrap().contains("not available"));
    }
}

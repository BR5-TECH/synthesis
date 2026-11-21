//! The dev container config of a project: where it is (DVC-FR-BGVU), what it
//! says about the agent CLI (DVC-FR-SEXK), the Feature the application adds to
//! it (DVC-FR-ABQZ, DVC-FR-JVKA), the starter file (DVC-FR-BCDZ), its hash
//! (DVC-FR-DELL), and the config the application generates from it for a start
//! (DVC-FR-QGUT).
//!
//! Every function here is pure or reads through `FsAccess`, so the rules are
//! tested without the dev container CLI or Docker on the machine.

use std::path::{Component, Path, PathBuf};

use jsonc_parser::cst::{CstInputValue, CstRootNode};
use jsonc_parser::ParseOptions;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// DVC-FR-BGVU: the folder that holds the configs.
const DEVCONTAINER_DIR: &str = ".devcontainer";
/// DVC-FR-BGVU: the file name of a config.
const CONFIG_FILE: &str = "devcontainer.json";
/// DVC-FR-BGVU: the config at the project root.
const ROOT_CONFIG: &str = ".devcontainer.json";

/// DVC-FR-SEXK: the Feature that installs Claude Code.
pub const CLAUDE_CODE_FEATURE: &str = "ghcr.io/anthropics/devcontainer-features/claude-code";
/// DVC-FR-ABQZ: the Feature the Claude Code Feature installs after.
pub const NODE_FEATURE: &str = "ghcr.io/devcontainers/features/node";
/// DVC-FR-ABQZ: the major version the application adds a Feature at.
const FEATURE_MAJOR: &str = "1";
/// DVC-FR-BCDZ: the base image of the starter config.
pub const STARTER_IMAGE: &str = "mcr.microsoft.com/devcontainers/base:bookworm";

/// DVC-FR-WZSY: the vendors whose CLI a dev container can run.
pub const RUNNABLE_VENDORS: &[&str] = &["claude_code", "codex"];

/// DVC-FR-SEXK: the known Feature of `vendor`, where it has one.
pub fn known_feature(vendor: &str) -> Option<&'static str> {
    match vendor {
        "claude_code" => Some(CLAUDE_CODE_FEATURE),
        _ => None,
    }
}

/// DVC-FR-EKAO: the executable of `vendor`'s CLI.
pub fn cli_name(vendor: &str) -> Option<&'static str> {
    match vendor {
        "claude_code" => Some("claude"),
        "codex" => Some("codex"),
        _ => None,
    }
}

/// DVC-FR-SEXK: a key of `features` names `reference` with any version tag or
/// with none.
pub fn feature_matches(key: &str, reference: &str) -> bool {
    match key.strip_prefix(reference) {
        Some("") => true,
        Some(rest) => rest.starts_with(':') || rest.starts_with('@'),
        None => false,
    }
}

// ---------------------------------------------------------------------------
// Where the configs are (DVC-FR-BGVU, DVC-FR-WXYK)
// ---------------------------------------------------------------------------

/// DVC-FR-BGVU: the project-relative paths of the configs in `root`, in the
/// order the requirement states. Nothing deeper than one folder under
/// `.devcontainer` is read, and `FsAccess` follows no link out of the project.
pub fn list_configs(root: &crate::fs::RootFs) -> Vec<String> {
    let mut found = Vec::new();
    let nested = format!("{DEVCONTAINER_DIR}/{CONFIG_FILE}");
    if is_file(root, &nested) {
        found.push(nested);
    }
    if is_file(root, ROOT_CONFIG) {
        found.push(ROOT_CONFIG.to_string());
    }
    let mut folders: Vec<String> = root
        .list_dir(root.path().join(DEVCONTAINER_DIR))
        .unwrap_or_default()
        .into_iter()
        .filter(|entry| entry.kind == crate::fs::EntryKind::Dir)
        .map(|entry| entry.name)
        .collect();
    folders.sort();
    for folder in folders {
        let path = format!("{DEVCONTAINER_DIR}/{folder}/{CONFIG_FILE}");
        if is_file(root, &path) {
            found.push(path);
        }
    }
    found
}

fn is_file(root: &crate::fs::RootFs, relative: &str) -> bool {
    root.file_info(root.path().join(relative))
        .is_ok_and(|info| info.kind == crate::fs::EntryKind::File)
}

/// DVC-FR-WXYK: why a stored config path is refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigPathProblem {
    Absolute,
    LeavesProject,
    NotAFile,
}

/// DVC-FR-WXYK: the host path of a stored config path, or why it is refused.
pub fn validate_config_path(
    root: &crate::fs::RootFs,
    relative: &str,
) -> Result<PathBuf, ConfigPathProblem> {
    let path = Path::new(relative);
    if path.is_absolute() || relative.starts_with('/') || relative.starts_with('\\') {
        return Err(ConfigPathProblem::Absolute);
    }
    if path.components().any(|c| matches!(c, Component::ParentDir | Component::Prefix(_))) {
        return Err(ConfigPathProblem::LeavesProject);
    }
    match root.file_info(root.path().join(path)) {
        Ok(info) if info.kind == crate::fs::EntryKind::File => Ok(root.path().join(path)),
        // FsAccess refuses a path that resolves out of the project, also one a
        // link takes out of it.
        Err(
            crate::fs::FsError::EscapesAllowedRoots { .. }
            | crate::fs::FsError::PathEscape { .. }
            | crate::fs::FsError::SymlinkRefused { .. },
        ) => Err(ConfigPathProblem::LeavesProject),
        _ => Err(ConfigPathProblem::NotAFile),
    }
}

// ---------------------------------------------------------------------------
// Reading a config (DVC-FR-JVKA, DVC-FR-HINW)
// ---------------------------------------------------------------------------

/// DVC-FR-JVKA: a config that is not JSON with comments, or whose top level is
/// not an object.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConfigInvalid {
    pub message: String,
    pub line: Option<usize>,
}

/// DVC-FR-JVKA: the config text as an object.
pub fn parse_config(text: &str) -> Result<Map<String, Value>, ConfigInvalid> {
    let value: Value =
        jsonc_parser::parse_to_serde_value(text, &ParseOptions::default()).map_err(|e| {
            ConfigInvalid { message: e.kind().to_string(), line: Some(e.line_display()) }
        })?;
    match value {
        Value::Object(map) => Ok(map),
        _ => Err(ConfigInvalid {
            message: "the top level of the config is not an object".to_string(),
            line: None,
        }),
    }
}

/// DVC-FR-SEXK: the keys of the `features` object of a config.
pub fn feature_keys(config: &Map<String, Value>) -> Vec<String> {
    config
        .get("features")
        .and_then(Value::as_object)
        .map(|features| features.keys().cloned().collect())
        .unwrap_or_default()
}

/// DVC-FR-WZSY: the config holds the known Feature of `vendor`.
pub fn holds_known_feature(config: &Map<String, Value>, vendor: &str) -> bool {
    let Some(reference) = known_feature(vendor) else {
        return false;
    };
    feature_keys(config).iter().any(|key| feature_matches(key, reference))
}

/// DVC-FR-JXEA: the config starts compose services.
pub fn is_compose(config: &Map<String, Value>) -> bool {
    config.contains_key("dockerComposeFile")
}

// ---------------------------------------------------------------------------
// Adding the Feature (DVC-FR-ABQZ, DVC-FR-JVKA)
// ---------------------------------------------------------------------------

/// DVC-FR-JVKA: why the Feature is not added.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AddFeatureError {
    NoKnownFeature,
    Invalid(ConfigInvalid),
}

/// DVC-FR-ABQZ: `text` with the known Feature of `vendor` added, and the Node
/// Feature where the config has none. `Ok(None)` where the config holds the
/// Feature already, so the file is left as it is.
pub fn add_agent_feature(text: &str, vendor: &str) -> Result<Option<String>, AddFeatureError> {
    let reference = known_feature(vendor).ok_or(AddFeatureError::NoKnownFeature)?;
    let parsed = parse_config(text).map_err(AddFeatureError::Invalid)?;
    // DVC-FR-JVKA: a `features` that is not an object is not replaced.
    if parsed.get("features").is_some_and(|features| !features.is_object()) {
        return Err(AddFeatureError::Invalid(ConfigInvalid {
            message: "`features` is not an object".to_string(),
            line: None,
        }));
    }
    if holds_known_feature(&parsed, vendor) {
        return Ok(None);
    }
    let has_node = feature_keys(&parsed).iter().any(|key| feature_matches(key, NODE_FEATURE));

    let root = CstRootNode::parse(text, &ParseOptions::default()).map_err(|e| {
        AddFeatureError::Invalid(ConfigInvalid {
            message: e.kind().to_string(),
            line: Some(e.line_display()),
        })
    })?;
    let object = root.object_value_or_set();
    let features = object.object_value_or_set("features");
    if !has_node {
        features.append(&versioned(NODE_FEATURE), CstInputValue::Object(Vec::new()));
    }
    features.append(&versioned(reference), CstInputValue::Object(Vec::new()));
    Ok(Some(root.to_string()))
}

fn versioned(reference: &str) -> String {
    format!("{reference}:{FEATURE_MAJOR}")
}

/// DVC-FR-BCDZ: the starter config.
pub fn starter_config(name: &str) -> String {
    let value = serde_json::json!({
        "name": name,
        "image": STARTER_IMAGE,
        "features": {
            versioned(NODE_FEATURE): {},
            versioned(CLAUDE_CODE_FEATURE): {},
        },
    });
    let mut text = serde_json::to_string_pretty(&value).unwrap_or_default();
    text.push('\n');
    text
}

// ---------------------------------------------------------------------------
// The hash of a config (DVC-FR-PFKJ, DVC-FR-DELL)
// ---------------------------------------------------------------------------

/// DVC-FR-DELL: the hash of the config file and of every file it names by a
/// relative path for its build: the Dockerfile and the compose files. A file
/// that cannot be read adds its path alone, so its later appearance is a
/// change too.
pub fn config_hash(root: &crate::fs::RootFs, config_path: &Path) -> Result<String, String> {
    let text = root.read_text(config_path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    if let Ok(config) = parse_config(&text) {
        let folder = config_path.parent().unwrap_or(root.path());
        for named in named_files(&config) {
            hasher.update(named.as_bytes());
            if let Ok(bytes) = root.read_bytes(folder.join(&named)) {
                hasher.update(&bytes);
            }
        }
    }
    Ok(hex(&hasher.finalize()))
}

/// The files a config names by a relative path for its build.
fn named_files(config: &Map<String, Value>) -> Vec<String> {
    let mut named = Vec::new();
    if let Some(build) = config.get("build").and_then(Value::as_object) {
        if let Some(dockerfile) = build.get("dockerfile").and_then(Value::as_str) {
            named.push(dockerfile.to_string());
        }
    }
    if let Some(dockerfile) = config.get("dockerFile").and_then(Value::as_str) {
        named.push(dockerfile.to_string());
    }
    match config.get("dockerComposeFile") {
        Some(Value::String(file)) => named.push(file.clone()),
        Some(Value::Array(files)) => {
            named.extend(files.iter().filter_map(Value::as_str).map(str::to_string))
        }
        _ => {}
    }
    named.retain(|file| !Path::new(file).is_absolute());
    named
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------------------
// The generated config (DVC-FR-QGUT, DVC-FR-IFBK)
// ---------------------------------------------------------------------------

/// DVC-FR-IFBK: one mount the launch adds to the container.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct LaunchMount {
    pub source: String,
    pub target: String,
    pub read_only: bool,
}

impl LaunchMount {
    /// The `docker run --mount` form that `mounts` of a config takes.
    fn render(&self) -> String {
        let mut text = format!("type=bind,source={},target={}", self.source, self.target);
        if self.read_only {
            text.push_str(",readonly");
        }
        text
    }
}

/// DVC-FR-QGUT: the user's config with the worktree at its host path and the
/// launch's mounts added. `remote_env` holds the variables the CLI sets for
/// every `devcontainer exec`, such as `CLAUDE_CONFIG_DIR` (DVC-FR-IFBK).
pub fn generated_config(
    config: &Map<String, Value>,
    worktree: &str,
    mounts: &[LaunchMount],
    remote_env: &[(String, String)],
) -> Value {
    let mut generated = config.clone();
    let workspace = format!("type=bind,source={worktree},target={worktree}");
    let mut all_mounts: Vec<Value> = match config.get("mounts") {
        Some(Value::Array(existing)) => existing.clone(),
        _ => Vec::new(),
    };
    // A compose config takes no `workspaceMount`: its service mounts what the
    // compose file says, so the worktree is one more mount at its host path.
    if is_compose(config) {
        all_mounts.push(Value::String(workspace));
    } else {
        generated.insert("workspaceMount".to_string(), Value::String(workspace));
    }
    generated.insert("workspaceFolder".to_string(), Value::String(worktree.to_string()));
    all_mounts.extend(mounts.iter().map(|mount| Value::String(mount.render())));
    generated.insert("mounts".to_string(), Value::Array(all_mounts));
    if !remote_env.is_empty() {
        let mut env = match config.get("remoteEnv") {
            Some(Value::Object(existing)) => existing.clone(),
            _ => Map::new(),
        };
        for (name, value) in remote_env {
            env.insert(name.clone(), Value::String(value.clone()));
        }
        generated.insert("remoteEnv".to_string(), Value::Object(env));
    }
    Value::Object(generated)
}

#[cfg(test)]
#[path = "tests/config.rs"]
mod tests;

//! The agent environment of a project (DVC-FR-EVZD, PSS-FR-VNPU).
//!
//! Project-local: whether the agents of a project run in its dev container or in
//! the vendor images is a preference of each developer, so the value lives in
//! its own section of `.synthesis/local.toml` and is never committed. The save
//! carries every other section of the file through unchanged.

use serde::{Deserialize, Serialize};

use super::{load_local, local_store_lock, write_local};

/// The key the agent environment lives under in `.synthesis/local.toml`.
const AGENT_ENVIRONMENT_KEY: &str = "agent_environment";
/// The key the probe results of the last dev container build live under
/// (DVC-FR-EKAO, DVC-FR-PFKJ).
const DEVCONTAINER_PROBE_KEY: &str = "devcontainer_probe";

/// DVC-FR-EVZD: where the agentic CLI work of the project runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentEnvironmentSource {
    /// One Docker image for each vendor (PSS-FR-22).
    #[default]
    VendorImages,
    /// The dev container a `devcontainer.json` of the project defines.
    Devcontainer,
}

/// DVC-FR-EVZD: the stored agent environment.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AgentEnvironment {
    pub source: AgentEnvironmentSource,
    /// A project-relative path to a `devcontainer.json` file.
    pub devcontainer_config: Option<String>,
}

impl AgentEnvironment {
    /// The config path where the source is a dev container and a path is set.
    pub fn devcontainer_config(&self) -> Option<&str> {
        match self.source {
            AgentEnvironmentSource::Devcontainer => {
                self.devcontainer_config.as_deref().filter(|path| !path.is_empty())
            }
            AgentEnvironmentSource::VendorImages => None,
        }
    }
}

/// DVC-FR-EKAO: what one probe of a built image found for one vendor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeFinding {
    Found,
    Missing,
}

/// DVC-FR-EKAO / DVC-FR-PFKJ: the probe results of the last build, and the hash
/// of the config file at that build.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DevcontainerProbe {
    pub config: String,
    pub config_hash: String,
    pub claude_code: Option<ProbeFinding>,
    pub codex: Option<ProbeFinding>,
}

impl DevcontainerProbe {
    /// The finding for `vendor`, where the probe holds one.
    pub fn finding(&self, vendor: &str) -> Option<ProbeFinding> {
        match vendor {
            "claude_code" => self.claude_code,
            "codex" => self.codex,
            _ => None,
        }
    }
}

/// DVC-FR-EVZD: the stored agent environment, or the default where the section
/// is absent or damaged. One damaged value costs only itself.
pub fn load_agent_environment_from(root: &crate::fs::RootFs) -> AgentEnvironment {
    let Some(section) = load_local(root).get(AGENT_ENVIRONMENT_KEY).cloned() else {
        return AgentEnvironment::default();
    };
    let field = |key: &str| section.get(key).cloned();
    AgentEnvironment {
        source: field("source")
            .and_then(|v| v.try_into::<AgentEnvironmentSource>().ok())
            .unwrap_or_default(),
        devcontainer_config: field("devcontainer_config")
            .and_then(|v| v.try_into::<String>().ok())
            .filter(|path| !path.is_empty()),
    }
}

/// DVC-FR-EVZD: write the agent environment whole, and leave every other
/// section of `local.toml` as it was. The caller validates the record first
/// (DVC-FR-WXYK).
pub fn write_agent_environment_to(
    root: &crate::fs::RootFs,
    environment: &AgentEnvironment,
) -> Result<(), String> {
    let mut section = toml::Table::new();
    let source = toml::Value::try_from(environment.source)
        .map_err(|e| format!("failed to encode the agent environment: {e}"))?;
    section.insert("source".to_string(), source);
    if let Some(path) = &environment.devcontainer_config {
        section.insert("devcontainer_config".to_string(), toml::Value::String(path.clone()));
    }
    let _guard = local_store_lock();
    let mut config = load_local(root);
    config.insert(AGENT_ENVIRONMENT_KEY.to_string(), toml::Value::Table(section));
    write_local(root, &config)
}

/// DVC-FR-EKAO: the probe results of the last build, or none.
pub fn load_devcontainer_probe_from(root: &crate::fs::RootFs) -> Option<DevcontainerProbe> {
    load_local(root).get(DEVCONTAINER_PROBE_KEY).cloned().and_then(|v| v.try_into().ok())
}

/// DVC-FR-EKAO: store the probe results of a build that succeeded.
pub fn write_devcontainer_probe_to(
    root: &crate::fs::RootFs,
    probe: &DevcontainerProbe,
) -> Result<(), String> {
    let value = toml::Value::try_from(probe)
        .map_err(|e| format!("failed to encode the dev container probe: {e}"))?;
    let _guard = local_store_lock();
    let mut config = load_local(root);
    config.insert(DEVCONTAINER_PROBE_KEY.to_string(), value);
    write_local(root, &config)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> (tempfile::TempDir, crate::fs::RootFs) {
        let dir = tempfile::tempdir().unwrap();
        let root = crate::fs::RootFs::for_root(dir.path());
        (dir, root)
    }

    // DVC-FR-EVZD: a project that stores nothing has the vendor images.
    #[test]
    fn nothing_stored_reads_as_vendor_images() {
        let (_dir, root) = root();
        let environment = load_agent_environment_from(&root);
        assert_eq!(environment, AgentEnvironment::default());
        assert_eq!(environment.source, AgentEnvironmentSource::VendorImages);
        assert_eq!(environment.devcontainer_config(), None);
    }

    // DVC-FR-EVZD / PSS-FR-VNPU: the value goes to `local.toml` and never to
    // `project.toml`, and the other sections of `local.toml` stay.
    #[test]
    fn the_value_is_project_local_and_keeps_other_sections() {
        let (dir, root) = root();
        let local = dir.path().join(".synthesis/local.toml");
        std::fs::create_dir_all(local.parent().unwrap()).unwrap();
        std::fs::write(&local, "[docker_in_docker]\nenabled = true\n").unwrap();

        let environment = AgentEnvironment {
            source: AgentEnvironmentSource::Devcontainer,
            devcontainer_config: Some(".devcontainer/devcontainer.json".to_string()),
        };
        write_agent_environment_to(&root, &environment).unwrap();

        assert_eq!(load_agent_environment_from(&root), environment);
        let text = std::fs::read_to_string(&local).unwrap();
        assert!(text.contains("[docker_in_docker]"), "{text}");
        assert!(text.contains("[agent_environment]"), "{text}");
        assert!(!dir.path().join(".synthesis/project.toml").exists());
    }

    // DVC-FR-EVZD: a damaged value reads as its default and costs no other one.
    #[test]
    fn a_damaged_source_reads_as_the_default() {
        let (dir, root) = root();
        let local = dir.path().join(".synthesis/local.toml");
        std::fs::create_dir_all(local.parent().unwrap()).unwrap();
        std::fs::write(
            &local,
            "[agent_environment]\nsource = 7\ndevcontainer_config = \"a.json\"\n",
        )
        .unwrap();
        let environment = load_agent_environment_from(&root);
        assert_eq!(environment.source, AgentEnvironmentSource::VendorImages);
        assert_eq!(environment.devcontainer_config.as_deref(), Some("a.json"));
        assert_eq!(environment.devcontainer_config(), None);
    }

    // DVC-FR-EKAO: the probe results round-trip beside the agent environment.
    #[test]
    fn probe_results_round_trip() {
        let (_dir, root) = root();
        assert_eq!(load_devcontainer_probe_from(&root), None);
        let probe = DevcontainerProbe {
            config: ".devcontainer.json".to_string(),
            config_hash: "abc".to_string(),
            claude_code: Some(ProbeFinding::Found),
            codex: Some(ProbeFinding::Missing),
        };
        write_devcontainer_probe_to(&root, &probe).unwrap();
        let read = load_devcontainer_probe_from(&root).unwrap();
        assert_eq!(read, probe);
        assert_eq!(read.finding("codex"), Some(ProbeFinding::Missing));
        assert_eq!(read.finding("opencode"), None);
    }
}

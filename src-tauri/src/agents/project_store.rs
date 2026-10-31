//! The Project agent files — `specifications/core/PSS-project-settings-storage.md`
//! PSS-FR-AGFL through PSS-FR-AGWT.
//!
//! A Project agent is one file, `<worktree>/.synthesis/agents/<id>.toml`, so a
//! create, an update, and a delete each touch exactly one file and two
//! contributors who change different agents cause no merge conflict. The files
//! are committed. A file holds persona fields only (PSS-FR-AGKY): no id (the
//! file name is the id), no timestamp, no provider, no URL, and no key.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::{validate_nickname, Agent, AgentDraft};
use crate::ai_api::ReasoningChoice;
use crate::fs::{EntryKind, FsError, RootFs};

/// The folder of the Project agent files, inside `.synthesis/`.
const AGENTS_DIR: &str = "agents";
/// The extension of a Project agent file.
const EXTENSION: &str = "toml";

/// The persona fields of one Project agent file (PSS-FR-AGKY).
///
/// `#[serde(default)]` so a hand-edited file missing a key still loads. A key
/// that is not declared here is ignored on read and is gone on the next write
/// of the file, which is how a stray `provider` or `apiKey` never survives.
///
/// Field order is load-bearing for TOML: every scalar is declared before
/// `reasoning`, which serialises as a table.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(default, rename_all = "camelCase")]
struct AgentFile {
    nickname: String,
    title: String,
    model_id: String,
    instructions: String,
    voice: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reasoning: Option<ReasoningChoice>,
}

/// What reading the folder found: the agents that loaded, and the names of the
/// files that were skipped (PSS-FR-AGRD).
#[derive(Debug, Default)]
pub struct ProjectAgentFiles {
    pub agents: Vec<Agent>,
    pub skipped: Vec<String>,
}

/// `<worktree>/.synthesis/agents`.
fn agents_dir(root: &Path) -> PathBuf {
    root.join(".synthesis").join(AGENTS_DIR)
}

fn agent_path(root: &Path, id: &str) -> PathBuf {
    agents_dir(root).join(format!("{id}.{EXTENSION}"))
}

/// An id is a bare filename-safe token. It is checked before it is joined into
/// a path, so an id received from the frontend cannot name a file elsewhere.
pub fn is_valid_id(id: &str) -> bool {
    crate::notes::is_valid_id(id)
}

fn agent_of(id: &str, file: AgentFile) -> Agent {
    Agent {
        id: id.to_string(),
        nickname: file.nickname,
        title: file.title,
        model_id: file.model_id,
        instructions: file.instructions,
        voice: file.voice,
        // A Project agent carries no timestamp (PSS-FR-AGKY).
        created_at: String::new(),
        updated_at: String::new(),
        reasoning: file.reasoning,
    }
}

/// PSS-FR-AGRD: every Project agent of the worktree, in the order of the file
/// names. An absent folder is an empty list. A file that cannot be parsed,
/// whose name is not a valid id, or whose nickname fails the nickname rule is
/// skipped and reported in `skipped`; it never stops the others from loading.
pub fn list(root: &RootFs) -> Result<ProjectAgentFiles, String> {
    let dir = agents_dir(root);
    let entries = match root.list_dir(&dir) {
        Ok(entries) => entries,
        Err(FsError::NotFound { .. }) | Err(FsError::NotADirectory { .. }) => {
            return Ok(ProjectAgentFiles::default())
        }
        Err(e) => return Err(format!("failed to list the project agents: {e}")),
    };
    let mut out = ProjectAgentFiles::default();
    for entry in entries {
        let Some(id) = entry.name.strip_suffix(&format!(".{EXTENSION}")).map(str::to_string) else {
            continue;
        };
        if entry.kind != EntryKind::File {
            continue;
        }
        if !is_valid_id(&id) {
            out.skipped.push(entry.name);
            continue;
        }
        match root.read_toml::<AgentFile>(agent_path(root, &id)) {
            Ok(file) if validate_nickname(&file.nickname).is_ok() => {
                out.agents.push(agent_of(&id, file));
            }
            _ => out.skipped.push(entry.name),
        }
    }
    Ok(out)
}

/// PSS-FR-AGFL: write the one file of one agent atomically. The folder is
/// created by the write itself (FSA-FR-05).
pub fn write(root: &RootFs, id: &str, draft: &AgentDraft, title: &str) -> Result<(), String> {
    if !is_valid_id(id) {
        return Err(super::ERR_AGENT_NOT_FOUND.into());
    }
    let file = AgentFile {
        nickname: draft.nickname.clone(),
        title: title.to_string(),
        model_id: draft.model_id.clone(),
        instructions: draft.instructions.clone(),
        voice: draft.voice.trim().to_string(),
        reasoning: draft.reasoning.clone(),
    };
    root.write_toml_atomic(agent_path(root, id), &file)
        .map_err(|e| format!("failed to write the project agent: {e}"))
}

/// Whether the worktree holds a file for this id.
pub fn exists(root: &RootFs, id: &str) -> bool {
    is_valid_id(id) && root.file_info(agent_path(root, id)).is_ok()
}

/// PSS-FR-AGFL: remove the one file of one agent. Idempotent: a file that is
/// already gone is not an error.
pub fn remove(root: &RootFs, id: &str) -> Result<(), String> {
    if !is_valid_id(id) {
        return Ok(());
    }
    match root.delete_under(agents_dir(root), format!("{id}.{EXTENSION}"), false) {
        Ok(()) | Err(FsError::NotFound { .. }) => Ok(()),
        Err(e) => Err(format!("failed to delete the project agent: {e}")),
    }
}

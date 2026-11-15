//! The remote a draft publication reuses, stored in the project-local file.

use serde::{Deserialize, Serialize};

use super::{load_local, save_local, PUBLICATION_REMOTE_KEY};

/// PSS-FR-TQFB: the remote a draft publication reuses — a configured Git
/// remote's name together with its canonicalized URL.
///
/// Both halves are stored because a name alone is not an identity: a remote
/// renamed or re-pointed since the choice was made is a different destination,
/// and the reader compares both before it trusts the choice
/// (`GHP-github-publication.md` GHP-FR-HVQG).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PublicationRemoteSelection {
    pub name: String,
    pub url: String,
}

/// PSS-FR-TQFB / PSS-FR-NKRE: the stored choice, or `None`.
///
/// A **hint and never an authority**: nothing here checks that the remote still
/// exists or that the token may reach it. A malformed or half-written value
/// reads as `None` rather than as an error, on the terms PSS-FR-10 repairs any
/// project-local value.
pub fn load_publication_remote_selection_from(
    root: &crate::fs::RootFs,
) -> Option<PublicationRemoteSelection> {
    let section = load_local(root).get(PUBLICATION_REMOTE_KEY).cloned()?;
    let selection = section.try_into::<PublicationRemoteSelection>().ok()?;
    if selection.name.is_empty() || selection.url.is_empty() {
        return None;
    }
    Some(selection)
}

/// PSS-FR-TQFB: persist the choice, or clear it with `None`, leaving every
/// other section of `local.toml` exactly as it was.
pub fn save_publication_remote_selection_to(
    root: &crate::fs::RootFs,
    selection: Option<PublicationRemoteSelection>,
) -> Result<(), String> {
    let mut config = load_local(root);
    match selection {
        Some(selection) => {
            let value = toml::Value::try_from(selection)
                .map_err(|e| format!("failed to encode publication remote: {e}"))?;
            config.insert(PUBLICATION_REMOTE_KEY.to_string(), value);
        }
        None => {
            config.remove(PUBLICATION_REMOTE_KEY);
        }
    }
    save_local(root, &config)
}

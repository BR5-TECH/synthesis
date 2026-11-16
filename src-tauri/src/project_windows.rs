//! The list of open project windows (`PWN-project-windows.md` PWN-FR-EKKX).
//!
//! One project is one project tab and one OS window. This module holds the
//! ordered list of those windows and the rules that act on it: open (a project
//! that is already open is not opened twice), activate, close with the choice of
//! the next active tab, and the needs-attention flag. It has no window handle
//! and no Tauri type, so every rule is tested without a runtime. The window
//! calls that follow from a rule belong to the command layer that uses it.
//!
//! The list is in the order of opening. It is never written to disk: a restart
//! starts at the Project picker with an empty list.

mod host;
#[cfg(target_os = "macos")]
mod macos_tabs;
pub use host::*;

use serde::Serialize;

/// The dot that starts the title of a macOS native tab which needs attention.
pub const ATTENTION_DOT: char = '●';

/// One open project window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWindowEntry {
    /// The project identity anchor.
    pub project_id: String,
    /// The full project name. Never shortened.
    pub name: String,
    /// The label of the OS window that holds the project.
    pub window_label: String,
    /// The project needs the user and its tab is not the active one.
    pub needs_attention: bool,
}

/// What the list publishes to every project window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWindowsSnapshot {
    pub windows: Vec<ProjectWindowEntry>,
    /// The project id of the active tab, if a project window is open.
    pub active_project_id: Option<String>,
}

/// The result of asking the list to open a project.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OpenOutcome {
    /// The project was added. The caller creates its window.
    Added,
    /// The project is open already. The caller activates this window and
    /// creates none.
    AlreadyOpen { window_label: String },
}

/// The result of closing a project window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CloseOutcome {
    /// The window label of the tab that is active after the close, if any.
    /// `None` means the last tab closed and the Project picker shows.
    pub active_window_label: Option<String>,
    /// The closed tab was the active one, so another tab was chosen.
    pub active_changed: bool,
}

/// The ordered list of open project windows and which one is active.
#[derive(Debug, Default)]
pub struct ProjectWindowList {
    entries: Vec<ProjectWindowEntry>,
    active: Option<String>,
}

impl ProjectWindowList {
    /// The entries in the order of opening.
    pub fn entries(&self) -> &[ProjectWindowEntry] {
        &self.entries
    }

    /// The number of open project windows.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// No project window is open.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The project id of the active tab.
    pub fn active_project_id(&self) -> Option<&str> {
        self.active.as_deref()
    }

    /// The entry of one project.
    pub fn find(&self, project_id: &str) -> Option<&ProjectWindowEntry> {
        self.entries.iter().find(|e| e.project_id == project_id)
    }

    /// The entry that one window holds.
    pub fn find_by_label(&self, window_label: &str) -> Option<&ProjectWindowEntry> {
        self.entries.iter().find(|e| e.window_label == window_label)
    }

    /// What every window receives.
    pub fn snapshot(&self) -> ProjectWindowsSnapshot {
        ProjectWindowsSnapshot {
            windows: self.entries.clone(),
            active_project_id: self.active.clone(),
        }
    }

    /// Add a project at the end of the list and make it the active tab. A
    /// project that is open already is not added: its tab becomes the active
    /// one (PWN-FR-RORD).
    pub fn open(&mut self, entry: ProjectWindowEntry) -> OpenOutcome {
        if let Some(existing) = self.find(&entry.project_id) {
            let window_label = existing.window_label.clone();
            self.activate(&entry.project_id);
            return OpenOutcome::AlreadyOpen { window_label };
        }
        let project_id = entry.project_id.clone();
        self.entries.push(ProjectWindowEntry { needs_attention: false, ..entry });
        self.active = Some(project_id);
        OpenOutcome::Added
    }

    /// Replace the project of one window, as "This tab" does. The tab keeps its
    /// place in the order. Returns `false` when the window is not in the list,
    /// or when another window holds the new project already.
    pub fn replace_project(&mut self, window_label: &str, project_id: &str, name: &str) -> bool {
        if self.entries.iter().any(|e| e.project_id == project_id && e.window_label != window_label)
        {
            return false;
        }
        let Some(entry) = self.entries.iter_mut().find(|e| e.window_label == window_label) else {
            return false;
        };
        let was_active = self.active.as_deref() == Some(entry.project_id.as_str());
        entry.project_id = project_id.to_string();
        entry.name = name.to_string();
        entry.needs_attention = false;
        if was_active {
            self.active = Some(project_id.to_string());
        }
        true
    }

    /// Make one tab the active one and clear its needs-attention flag (NTF-FR-14).
    /// Returns
    /// the window label to focus, or `None` for an unknown project.
    pub fn activate(&mut self, project_id: &str) -> Option<String> {
        let entry = self.entries.iter_mut().find(|e| e.project_id == project_id)?;
        entry.needs_attention = false;
        let label = entry.window_label.clone();
        self.active = Some(project_id.to_string());
        Some(label)
    }

    /// Set or clear the needs-attention flag of one tab (PWN-FR-CFSW). A flag
    /// that has the value already changes nothing. Returns whether it changed.
    pub fn set_attention(&mut self, project_id: &str, needs_attention: bool) -> bool {
        let Some(entry) = self.entries.iter_mut().find(|e| e.project_id == project_id) else {
            return false;
        };
        if entry.needs_attention == needs_attention {
            return false;
        }
        entry.needs_attention = needs_attention;
        true
    }

    /// Remove one project. Closing a tab that is not active keeps the active
    /// tab. Closing the active tab chooses the nearest remaining tab: the one
    /// that followed it, or the one before it when it was last.
    pub fn close(&mut self, project_id: &str) -> Option<CloseOutcome> {
        let index = self.entries.iter().position(|e| e.project_id == project_id)?;
        let was_active = self.active.as_deref() == Some(project_id);
        self.entries.remove(index);
        if !was_active {
            let active_window_label =
                self.active.as_deref().and_then(|id| self.find(id)).map(|e| e.window_label.clone());
            return Some(CloseOutcome { active_window_label, active_changed: false });
        }
        let next_id = self
            .entries
            .get(index)
            .or_else(|| index.checked_sub(1).and_then(|i| self.entries.get(i)))
            .map(|e| e.project_id.clone());
        self.active = None;
        let active_window_label = next_id.and_then(|id| self.activate(&id));
        Some(CloseOutcome { active_window_label, active_changed: true })
    }

    /// The project id that follows (`step` = 1) or precedes (`step` = -1) the
    /// active tab. The order wraps around. `None` with fewer than two tabs.
    pub fn neighbour(&self, step: i32) -> Option<&str> {
        if self.entries.len() < 2 {
            return None;
        }
        let current = self.active.as_deref()?;
        let index = self.entries.iter().position(|e| e.project_id == current)?;
        let count = self.entries.len() as i32;
        let target = (index as i32 + step).rem_euclid(count) as usize;
        self.entries.get(target).map(|e| e.project_id.as_str())
    }
}

/// PWN-FR-DSLC: the title of a project window: the full project name, with a dot
/// and one space before it when the project needs attention.
pub fn project_window_title(name: &str, needs_attention: bool) -> String {
    if needs_attention {
        format!("{ATTENTION_DOT} {name}")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests;

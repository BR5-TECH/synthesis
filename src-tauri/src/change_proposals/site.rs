//! Where a change lives: its stream, the stream's working copy, and the folder
//! of the change in it (CEP-FR-WJKY, CEP-FR-ESQR, CEP-FR-LYSS).
//!
//! The operations of this module take a [`SiteResolver`] rather than an
//! application handle for this, so a test gives a working copy of its own and
//! the commands give the stream registry and the run order.

use super::*;

/// The folder of the changes, relative to a working copy (OSM-FR-MFVN).
pub const CHANGES_REL: &str = "openspec/changes";

/// One change on one stream, with the stream's working copy.
#[derive(Clone)]
pub struct ChangeSite {
    pub stream_id: String,
    pub change: String,
    pub working_copy: fs::RootFs,
}

impl ChangeSite {
    /// The change folder, relative to the working copy.
    pub fn change_rel(&self) -> String {
        format!("{CHANGES_REL}/{}", self.change)
    }

    /// CEP-FR-WJKY: a hunk path as a path relative to the working copy, or
    /// `path_outside_change`.
    pub fn file_rel(&self, path: &str) -> Result<String, String> {
        if !comments::is_change_relative_path(path) {
            return Err(ERR_PATH_OUTSIDE_CHANGE.to_string());
        }
        Ok(format!("{}/{path}", self.change_rel()))
    }

    /// The absolute path of a file of the change, behind the escape gate.
    pub fn file_path(&self, path: &str) -> Result<PathBuf, String> {
        let rel = self.file_rel(path)?;
        fs::resolve_under(self.working_copy.path(), rel)
            .map_err(|_| ERR_PATH_OUTSIDE_CHANGE.to_string())
    }

    /// CEP-FR-LYSS: whether the change folder is a folder of the working copy.
    pub fn change_exists(&self) -> bool {
        fs::resolve_under(self.working_copy.path(), self.change_rel())
            .ok()
            .and_then(|path| self.working_copy.file_info(path).ok())
            .is_some_and(|info| info.kind == fs::EntryKind::Dir)
    }

    /// The text of a file of the change, or `None` where it is not a file.
    pub fn read_file(&self, path: &str) -> Option<String> {
        let at = self.file_path(path).ok()?;
        let info = self.working_copy.file_info(&at).ok()?;
        if info.kind != fs::EntryKind::File {
            return None;
        }
        self.working_copy.read_text(&at).ok()
    }

    /// Whether anything stands at the path of a file of the change.
    pub fn exists(&self, path: &str) -> bool {
        self.file_path(path).ok().is_some_and(|at| self.working_copy.file_info(at).is_ok())
    }

    /// PST-FR-22: the text as the project's write path persists it.
    pub fn normalise(&self, text: &str) -> String {
        crate::project_settings::line_endings_for(&self.working_copy).normalize(text)
    }
}

/// How an operation finds a change and learns whether its stream is busy.
pub trait SiteResolver {
    /// CEP-FR-LYSS: the site of a change, or `unknown_stream` /
    /// `change_not_found`.
    fn site(&self, stream_id: &str, change: &str) -> Result<ChangeSite, String>;

    /// CEP-FR-ESQR: the id of a run of the stream that is `working` or
    /// `reviewing`, if one stands.
    fn busy_run(&self, stream_id: &str) -> Option<String>;
}

/// The resolver of the running application: the stream registry and the run
/// order index.
pub struct AppSites<'a, R: tauri::Runtime> {
    pub app: &'a tauri::AppHandle<R>,
}

impl<R: tauri::Runtime> SiteResolver for AppSites<'_, R> {
    fn site(&self, stream_id: &str, change: &str) -> Result<ChangeSite, String> {
        if !comments::is_change_owner(stream_id, change) {
            return Err(ERR_CHANGE_NOT_FOUND.to_string());
        }
        // A test gives its streams to the lookup, not to the application.
        #[cfg(test)]
        if let Ok((_, site)) = change_site_now(stream_id, change) {
            return Ok(site);
        }
        let location =
            crate::openspec::change::ChangeLocation::Stream { stream_id: stream_id.to_string() };
        let working_copy =
            crate::openspec::change::working_copy(self.app, &location).map_err(|e| match e {
                crate::openspec::OpenSpecReadError::UnknownStream => ERR_UNKNOWN_STREAM.to_string(),
                _ => ERR_NO_PROJECT_OPEN.to_string(),
            })?;
        let site = ChangeSite {
            stream_id: stream_id.to_string(),
            change: change.to_string(),
            working_copy,
        };
        if !site.change_exists() {
            return Err(ERR_CHANGE_NOT_FOUND.to_string());
        }
        Ok(site)
    }

    fn busy_run(&self, stream_id: &str) -> Option<String> {
        crate::graduation::agent_work_run_on_stream(self.app, stream_id)
    }
}

/// A resolver over fixed working copies, for a test.
#[cfg(test)]
pub struct FixedSites {
    pub sites: Vec<(String, fs::RootFs)>,
    pub busy: std::sync::Mutex<Option<String>>,
}

#[cfg(test)]
impl SiteResolver for FixedSites {
    fn site(&self, stream_id: &str, change: &str) -> Result<ChangeSite, String> {
        let (_, working_copy) = self
            .sites
            .iter()
            .find(|(id, _)| id == stream_id)
            .ok_or_else(|| ERR_UNKNOWN_STREAM.to_string())?;
        let site = ChangeSite {
            stream_id: stream_id.to_string(),
            change: change.to_string(),
            working_copy: working_copy.clone(),
        };
        if !site.change_exists() {
            return Err(ERR_CHANGE_NOT_FOUND.to_string());
        }
        Ok(site)
    }

    fn busy_run(&self, _stream_id: &str) -> Option<String> {
        self.busy.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }
}

/// CMS-FR-FACZ / VOM-FR-NTKT: whether a change owner resolves now, for a
/// caller that holds an application handle. `Ok(())`, or `unknown_stream` /
/// `change_not_found`.
pub fn require_change_owner<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    stream_id: &str,
    change: &str,
) -> Result<ChangeSite, String> {
    AppSites { app }.site(stream_id, change)
}

// ---------------------------------------------------------------------------
// The stream lookup of a caller with no application handle
// ---------------------------------------------------------------------------

/// A stream's name and working copy, by stream id.
type StreamLookup = Box<dyn Fn(&str) -> Option<(String, fs::RootFs)> + Send + Sync>;

static STREAM_LOOKUP: std::sync::OnceLock<StreamLookup> = std::sync::OnceLock::new();

/// The streams a test gives the lookup. A process-wide list, because a turn
/// builds its input on its own thread; nextest runs each test in its own
/// process, and a test uses a stream id of its own.
#[cfg(test)]
static TEST_STREAMS: std::sync::Mutex<Vec<(String, String, fs::RootFs)>> =
    std::sync::Mutex::new(Vec::new());

/// Give the turn builders (AGC-FR-NBZN) a way to find a stream's working copy.
/// Installed once, at application setup.
pub fn install_stream_lookup<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    let _ = STREAM_LOOKUP.set(Box::new(move |stream_id: &str| {
        let stream = crate::streams::stream_by_id(&app, stream_id)?;
        let location =
            crate::openspec::change::ChangeLocation::Stream { stream_id: stream_id.to_string() };
        let copy = crate::openspec::change::working_copy(&app, &location).ok()?;
        Some((stream.name, copy))
    }));
}

/// A stream for the lookup of the current test thread.
#[cfg(test)]
pub fn set_test_stream(stream_id: &str, name: &str, copy: fs::RootFs) {
    let mut s = TEST_STREAMS.lock().unwrap_or_else(|e| e.into_inner());
    s.retain(|(id, _, _)| id != stream_id);
    s.push((stream_id.to_string(), name.to_string(), copy));
}

/// The test streams are cleared for the next test.
#[cfg(test)]
pub fn clear_test_streams() {
    TEST_STREAMS.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

/// A live stream's name and working copy, or `None`.
pub fn stream_working_copy(stream_id: &str) -> Option<(String, fs::RootFs)> {
    #[cfg(test)]
    {
        let found = TEST_STREAMS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .find(|(id, _, _)| id == stream_id)
            .map(|(_, n, c)| (n.clone(), c.clone()));
        if found.is_some() {
            return found;
        }
    }
    STREAM_LOOKUP.get().and_then(|lookup| lookup(stream_id))
}

/// AGC-FR-OYJX / CEP-FR-LYSS: the site of a change, found through the stream
/// lookup, with the stream's name.
pub fn change_site_now(stream_id: &str, change: &str) -> Result<(String, ChangeSite), String> {
    if !comments::is_change_owner(stream_id, change) {
        return Err(ERR_CHANGE_NOT_FOUND.to_string());
    }
    let (name, working_copy) =
        stream_working_copy(stream_id).ok_or_else(|| ERR_UNKNOWN_STREAM.to_string())?;
    let site =
        ChangeSite { stream_id: stream_id.to_string(), change: change.to_string(), working_copy };
    if !site.change_exists() {
        return Err(ERR_CHANGE_NOT_FOUND.to_string());
    }
    Ok((name, site))
}

impl ChangeSite {
    /// AGC-FR-NBZN: every file of the change, as change-relative paths in path
    /// order. A folder that cannot be listed adds nothing.
    pub fn files(&self) -> Vec<String> {
        let mut out = Vec::new();
        let Ok(root) = fs::resolve_under(self.working_copy.path(), self.change_rel()) else {
            return out;
        };
        let mut stack: Vec<(PathBuf, String)> = vec![(root, String::new())];
        while let Some((dir, prefix)) = stack.pop() {
            let Ok(entries) = self.working_copy.list_dir(&dir) else {
                continue;
            };
            for entry in entries {
                let rel = if prefix.is_empty() {
                    entry.name.clone()
                } else {
                    format!("{prefix}/{}", entry.name)
                };
                match self.working_copy.file_info(dir.join(&entry.name)).map(|i| i.kind) {
                    Ok(fs::EntryKind::Dir) => stack.push((dir.join(&entry.name), rel)),
                    Ok(fs::EntryKind::File) => out.push(rel),
                    _ => {}
                }
            }
        }
        out.sort();
        out
    }
}

//! TRS-FR-OMYR: where tmux is, and whether it is usable.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use super::runner::{run_bounded, RealClients, RealTmux};
use super::{Tmux, TmuxSource, SOCKET};

/// TRS-FR-OMYR: the oldest usable version.
pub const MIN_VERSION: (u32, u32) = (3, 2);

/// TRS-FR-OMYR: the version that `tmux -V` reports, as major and minor.
/// `tmux 3.2a`, `tmux next-3.4`, and `tmux 3.4` all parse.
pub fn parse_version(text: &str) -> Option<(u32, u32)> {
    let word = text.trim().strip_prefix("tmux ")?.trim();
    let word = word.strip_prefix("next-").unwrap_or(word);
    let mut parts = word.splitn(2, '.');
    let major: u32 = parts.next()?.parse().ok()?;
    let minor: String = parts.next()?.chars().take_while(|c| c.is_ascii_digit()).collect();
    Some((major, minor.parse().ok()?))
}

/// TRS-FR-OMYR: whether the output of `tmux -V` names a usable version. A
/// build from the main branch (`tmux master`) is newer than every release.
pub fn version_usable(text: &str) -> bool {
    if text.trim() == "tmux master" {
        return true;
    }
    parse_version(text).is_some_and(|version| version >= MIN_VERSION)
}

/// The text between the markers of [`login_path_script`].
const PATH_MARKER: &str = "__SYNTHESIS_PATH__";

/// The script a login shell runs to print its `PATH` between markers, so that
/// any text that the login files print is ignored.
pub fn login_path_script() -> String {
    format!("printf '\\n{PATH_MARKER}%s{PATH_MARKER}\\n' \"$PATH\"")
}

/// The `PATH` from the output of [`login_path_script`].
pub fn parse_login_path(output: &str) -> Option<String> {
    let start = output.find(PATH_MARKER)? + PATH_MARKER.len();
    let rest = &output[start..];
    let end = rest.find(PATH_MARKER)?;
    Some(rest[..end].to_string()).filter(|p| !p.is_empty())
}

/// TRS-FR-OMYR: the directories to search, the login shell's `PATH` first.
pub fn search_directories(
    login_path: Option<&str>,
    inherited_path: Option<&str>,
    home: Option<&Path>,
) -> Vec<PathBuf> {
    let joined = [login_path, inherited_path].into_iter().flatten().collect::<Vec<_>>().join(":");
    let mut dirs = crate::agentic::detection_directories(Some(&joined), home);
    let mut seen = std::collections::HashSet::new();
    dirs.retain(|d| seen.insert(d.clone()));
    dirs
}

/// The time a login shell gets to print its `PATH`.
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// The login shell's `PATH`, read once for each application process.
fn login_path() -> Option<&'static str> {
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE
        .get_or_init(|| {
            let shell_var = std::env::var("SHELL").ok();
            let shell = crate::terminals::host::login_shell(shell_var.as_deref(), |p| {
                crate::fs::probe_entry_is_dir(Path::new(p)) == Some(false)
            });
            let mut command = std::process::Command::new(shell);
            command.args(["-l", "-c", &login_path_script()]);
            run_bounded(command, LOGIN_SHELL_TIMEOUT).ok().and_then(|out| parse_login_path(&out))
        })
        .as_deref()
}

/// TRS-FR-OMYR: the tmux executable, or `None` where none is found.
pub fn find_tmux() -> Option<String> {
    let inherited = std::env::var("PATH").ok();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let dirs = search_directories(login_path(), inherited.as_deref(), home.as_deref());
    crate::agentic::detect_in(&dirs, &["tmux"], &crate::agentic::RealFileProbe)
}

/// The tmux of this machine, checked at each call (TRS-FR-OMYR).
pub struct HostTmux;

impl TmuxSource for HostTmux {
    fn probe(&self) -> Result<Arc<Tmux>, &'static str> {
        let program = find_tmux().ok_or("tmux_not_found")?;
        let mut version = std::process::Command::new(&program);
        version.arg("-V");
        let text = run_bounded(version, super::runner::CALL_TIMEOUT)
            .map_err(|_| "tmux_version_unreadable")?;
        if !version_usable(&text) {
            return Err("tmux_too_old");
        }
        Ok(Arc::new(Tmux {
            runner: Arc::new(RealTmux { program: program.clone(), socket: SOCKET.into() }),
            clients: Arc::new(RealClients { program, socket: SOCKET.into() }),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TRS-FR-OMYR: 3.2 and later are usable; 3.1 and text that is not a
    // version are not.
    #[test]
    fn reads_the_version() {
        assert_eq!(parse_version("tmux 3.2a\n"), Some((3, 2)));
        assert_eq!(parse_version("tmux next-3.4"), Some((3, 4)));
        assert_eq!(parse_version("tmux 3.10"), Some((3, 10)));
        assert!(version_usable("tmux 3.2"));
        assert!(version_usable("tmux 3.2a"));
        assert!(version_usable("tmux 4.0"));
        assert!(version_usable("tmux master"));
        assert!(!version_usable("tmux 3.1c"));
        assert!(!version_usable("tmux 2.9"));
        assert!(!version_usable("screen 4.0"));
        assert!(!version_usable(""));
    }

    // TRS-FR-OMYR: the login shell's PATH comes from between the markers, so
    // text that the login files print is ignored.
    #[test]
    fn reads_the_login_path() {
        let out = format!("welcome!\n\n{PATH_MARKER}/opt/x/bin:/usr/bin{PATH_MARKER}\n");
        assert_eq!(parse_login_path(&out).as_deref(), Some("/opt/x/bin:/usr/bin"));
        assert_eq!(parse_login_path("no markers"), None);
        assert_eq!(parse_login_path(&format!("{PATH_MARKER}{PATH_MARKER}")), None);
    }

    // TRS-FR-OMYR: the login PATH comes first, then the inherited PATH, then
    // the conventional directories, each once.
    #[test]
    fn searches_the_login_path_first() {
        let dirs = search_directories(Some("/login/bin:/usr/bin"), Some("/usr/bin:/app/bin"), None);
        let names: Vec<String> = dirs.iter().map(|d| d.to_string_lossy().into_owned()).collect();
        assert_eq!(names[0], "/login/bin");
        assert!(names.contains(&"/app/bin".to_string()));
        assert!(names.contains(&"/opt/homebrew/bin".to_string()));
        assert_eq!(names.iter().filter(|n| *n == "/usr/bin").count(), 1);
    }

    // TRS-FR-OMYR: a real login shell prints its PATH between the markers.
    #[test]
    fn a_real_login_shell_prints_its_path() {
        let mut command = std::process::Command::new("/bin/sh");
        command.args(["-c", &login_path_script()]);
        let out = run_bounded(command, LOGIN_SHELL_TIMEOUT).unwrap();
        assert!(parse_login_path(&out).is_some(), "{out}");
    }
}

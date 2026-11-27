//! The host kinds: a Host shell (TRS-FR-QPBL) and an Agent CLI on the host
//! (TRS-FR-ASKO).

use super::model::{CliVendor, TerminalError, TerminalKind};
use super::process::{terminal_env, HostCommand};

/// TRS-FR-QPBL: the login shell, from `SHELL` where it names an executable,
/// else the platform's default shell where it exists, else `/bin/sh`.
pub fn login_shell(shell_var: Option<&str>, is_executable: impl Fn(&str) -> bool) -> String {
    if let Some(shell) = shell_var.map(str::trim).filter(|s| !s.is_empty()) {
        if is_executable(shell) {
            return shell.to_string();
        }
    }
    let platform_default = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/bash" };
    if is_executable(platform_default) {
        return platform_default.to_string();
    }
    "/bin/sh".to_string()
}

/// TRS-FR-QPBL: the command of a Host shell in `cwd`.
pub fn host_shell_command(shell: &str, cwd: &str) -> HostCommand {
    HostCommand {
        program: shell.to_string(),
        args: vec!["-l".to_string(), "-i".to_string()],
        cwd: cwd.to_string(),
        env: terminal_env(),
        env_remove: Vec::new(),
    }
}

/// TRS-FR-ZZZN: the absolute working directory of a start folder, given
/// relative to `checkout` with `/` separators. A folder that escapes the
/// checkout, goes through a symbolic link, is absent, or is not a directory is
/// `invalid_start_folder`.
pub fn resolve_start_folder(checkout: &str, folder: &str) -> Result<String, TerminalError> {
    let invalid = || TerminalError::new("invalid_start_folder");
    let root = std::path::Path::new(checkout);
    crate::fs::resolve_inside(root, folder).map_err(|_| invalid())?;
    let absolute = folder
        .split('/')
        .filter(|part| !part.is_empty())
        .fold(root.to_path_buf(), |path, part| path.join(part));
    if crate::fs::probe_entry_is_dir(&absolute) != Some(true) {
        return Err(invalid());
    }
    Ok(absolute.to_string_lossy().into_owned())
}

/// TRS-FR-QPBL, TRS-FR-ZZZN: the working directory of a new session: the
/// checkout, or the start folder inside it for a Host shell.
pub fn working_dir(
    kind: TerminalKind,
    checkout: &str,
    start_folder: Option<&str>,
) -> Result<String, TerminalError> {
    match start_folder.filter(|folder| !folder.is_empty()) {
        None => Ok(checkout.to_string()),
        Some(_) if kind != TerminalKind::HostShell => {
            Err(TerminalError::new("start_folder_unsupported"))
        }
        Some(folder) => resolve_start_folder(checkout, folder),
    }
}

/// TRS-FR-ASKO, TRS-FR-QDGW: the verified executable with no argument, so the
/// CLI starts a fresh interactive session with the user's own login.
pub fn host_agent_command(_vendor: CliVendor, executable: &str, cwd: &str) -> HostCommand {
    HostCommand {
        program: executable.to_string(),
        args: Vec::new(),
        cwd: cwd.to_string(),
        env: terminal_env(),
        env_remove: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // TRS-FR-QPBL: SHELL wins where it names an executable.
    #[test]
    fn shell_variable_wins() {
        assert_eq!(login_shell(Some("/usr/bin/fish"), |_| true), "/usr/bin/fish");
    }

    // TRS-FR-QPBL: a SHELL that is not executable falls to the platform default.
    #[test]
    fn falls_to_the_platform_default() {
        let expected = if cfg!(target_os = "macos") { "/bin/zsh" } else { "/bin/bash" };
        assert_eq!(login_shell(Some("/nope"), |p| p == expected), expected);
        assert_eq!(login_shell(None, |p| p == expected), expected);
    }

    // TRS-FR-QPBL: with nothing else, the shell is /bin/sh.
    #[test]
    fn falls_to_bin_sh() {
        assert_eq!(login_shell(Some(""), |_| false), "/bin/sh");
    }

    // TRS-FR-QPBL, TRS-FR-KMWO: an interactive login shell in the target's directory.
    #[test]
    fn the_host_shell_is_an_interactive_login_shell() {
        let command = host_shell_command("/bin/zsh", "/repo/wt");
        assert_eq!(command.args, vec!["-l", "-i"]);
        assert_eq!(command.cwd, "/repo/wt");
        assert!(command.env.iter().any(|(k, v)| k == "TERM" && v == "xterm-256color"));
        assert!(command.env.iter().any(|(k, v)| k == "COLORTERM" && v == "truecolor"));
        assert_eq!(command.env.len(), 2);
    }

    // TRS-FR-ASKO, TRS-FR-QDGW: an agent CLI runs with no argument.
    #[test]
    fn a_host_agent_cli_runs_with_no_argument() {
        let command = host_agent_command(CliVendor::Codex, "/usr/local/bin/codex", "/repo");
        assert_eq!(command.program, "/usr/local/bin/codex");
        assert!(command.args.is_empty());
    }

    // TRS-FR-ZZZN: a folder inside the checkout is the working directory.
    #[test]
    fn a_start_folder_inside_the_checkout_resolves() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("docs/specs")).unwrap();
        let checkout = tmp.path().to_string_lossy().into_owned();
        let resolved = resolve_start_folder(&checkout, "docs/specs").unwrap();
        assert_eq!(std::path::PathBuf::from(resolved), tmp.path().join("docs").join("specs"));
    }

    // TRS-FR-ZZZN: an escape, an absolute path, a file, and an absent folder
    // are each `invalid_start_folder`.
    #[test]
    fn a_start_folder_outside_or_not_a_folder_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("docs")).unwrap();
        std::fs::write(tmp.path().join("docs/a.md"), b"a").unwrap();
        let checkout = tmp.path().to_string_lossy().into_owned();
        for folder in ["../", "docs/../../x", "/etc", "docs/a.md", "missing"] {
            let err = resolve_start_folder(&checkout, folder).unwrap_err();
            assert_eq!(err.code, "invalid_start_folder", "{folder}");
        }
    }

    // TRS-FR-ZZZN: a folder reached through a symbolic link is refused, also
    // when the folder behind the link is real, inside or outside the checkout.
    #[cfg(unix)]
    #[test]
    fn a_start_folder_through_a_symlink_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("docs/sub")).unwrap();
        std::os::unix::fs::symlink(tmp.path().join("docs"), tmp.path().join("link")).unwrap();
        std::os::unix::fs::symlink(outside.path(), tmp.path().join("out")).unwrap();
        let checkout = tmp.path().to_string_lossy().into_owned();
        assert!(resolve_start_folder(&checkout, "docs/sub").is_ok());
        for folder in ["link", "link/sub", "out"] {
            let err = resolve_start_folder(&checkout, folder).unwrap_err();
            assert_eq!(err.code, "invalid_start_folder", "{folder}");
        }
    }

    // TRS-FR-ZZZN: `.` and a trailing separator resolve inside the checkout.
    #[test]
    fn a_start_folder_with_a_dot_or_a_trailing_separator_resolves() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("docs")).unwrap();
        let checkout = tmp.path().to_string_lossy().into_owned();
        assert_eq!(
            std::path::PathBuf::from(resolve_start_folder(&checkout, "docs/").unwrap()),
            tmp.path().join("docs")
        );
        assert!(std::path::PathBuf::from(resolve_start_folder(&checkout, ".").unwrap())
            .starts_with(tmp.path()));
    }

    // TRS-FR-QPBL, TRS-FR-ZZZN: no start folder, or an empty one, is the
    // checkout; a start folder of a Host shell is its folder; any other kind
    // with one is refused.
    #[test]
    fn the_working_dir_of_a_new_session() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join("docs")).unwrap();
        let checkout = tmp.path().to_string_lossy().into_owned();
        assert_eq!(working_dir(TerminalKind::HostShell, &checkout, None).unwrap(), checkout);
        assert_eq!(
            working_dir(TerminalKind::ContainerShell, &checkout, Some("")).unwrap(),
            checkout
        );
        assert_eq!(
            std::path::PathBuf::from(
                working_dir(TerminalKind::HostShell, &checkout, Some("docs")).unwrap()
            ),
            tmp.path().join("docs")
        );
        for kind in
            [TerminalKind::ContainerShell, TerminalKind::MicrosandboxShell, TerminalKind::AgentCli]
        {
            let err = working_dir(kind, &checkout, Some("docs")).unwrap_err();
            assert_eq!(err.code, "start_folder_unsupported");
        }
        let err = working_dir(TerminalKind::HostShell, &checkout, Some("missing")).unwrap_err();
        assert_eq!(err.code, "invalid_start_folder");
    }
}

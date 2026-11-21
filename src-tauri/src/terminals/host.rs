//! The host kinds: a Host shell (TRS-FR-QPBL) and an Agent CLI on the host
//! (TRS-FR-ASKO).

use super::model::CliVendor;
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
}

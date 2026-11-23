//! The tmux command lists of the module (TRS-FR-ANJK .. TRS-FR-TCBS). Each
//! function builds arguments and runs nothing.

use super::meta::{list_format, Meta, PANE_FORMAT};
use crate::terminals::process::HostCommand;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// The target of exactly one session, with no prefix match.
pub fn exact(session: &str) -> String {
    format!("={session}")
}

/// The target of the pane of one session. Options take a pane target, so an
/// option of a session also names it so.
fn pane(session: &str) -> String {
    format!("={session}:")
}

/// Joins commands into one tmux command list.
pub fn join(commands: Vec<Vec<String>>) -> Vec<String> {
    let mut out = Vec::new();
    for (i, command) in commands.into_iter().enumerate() {
        if i > 0 {
            out.push(";".to_string());
        }
        out.extend(command);
    }
    out
}

pub fn set_option(session: &str, key: &str, value: &str) -> Vec<String> {
    vec!["set-option".into(), "-t".into(), pane(session), key.into(), argv_safe(value)]
}

/// tmux reads an argument that ends with `;` as the end of a command. Such a
/// value ends with `\;`, which tmux reads as one literal `;`.
pub fn argv_safe(value: &str) -> String {
    match value.strip_suffix(';') {
        Some(body) => format!("{body}\\;"),
        None => value.to_string(),
    }
}

/// TRS-FR-ANJK, TRS-FR-KBOS: the server and global options. Each is set to a
/// fixed value, so to set them again changes nothing.
pub fn server_options() -> Vec<Vec<String>> {
    vec![
        strings(&["set-option", "-s", "exit-empty", "off"]),
        strings(&["set-option", "-s", "escape-time", "0"]),
        strings(&["set-option", "-s", "terminal-overrides[90]", "*:smcup@:rmcup@"]),
        strings(&["set-option", "-s", "terminal-features[90]", "xterm-256color:RGB"]),
        strings(&["set-option", "-g", "status", "off"]),
        strings(&["set-option", "-g", "prefix", "None"]),
        strings(&["set-option", "-g", "prefix2", "None"]),
        strings(&["set-option", "-g", "mouse", "off"]),
        strings(&["set-option", "-g", "history-limit", "10000"]),
        strings(&["set-option", "-g", "default-terminal", "xterm-256color"]),
        strings(&["set-option", "-gw", "remain-on-exit", "on"]),
        strings(&["set-option", "-gw", "window-size", "latest"]),
    ]
}

/// TRS-FR-ANJK: removes every key binding of the tables that a key reaches.
/// Each one is a command list of its own: tmux refuses a table with no key,
/// and a refusal ends the rest of its list, also with `-q`.
pub fn unbind_all() -> Vec<Vec<String>> {
    ["prefix", "root", "copy-mode", "copy-mode-vi"]
        .into_iter()
        .map(|table| strings(&["unbind-key", "-a", "-T", table]))
        .collect()
}

/// TRS-FR-KBOS: a pane that dies frees the client of its session, and a
/// client that attaches to a dead pane is freed at once, so the module reads
/// the exit status. The hooks name the session literally: tmux expands no
/// format in a hook's target.
pub fn session_hooks(session: &str) -> Vec<Vec<String>> {
    let detach = format!("detach-client -s {}", exact(session));
    vec![
        vec!["set-hook".into(), "-t".into(), pane(session), "pane-died".into(), detach.clone()],
        vec![
            "set-hook".into(),
            "-t".into(),
            pane(session),
            "client-attached".into(),
            format!("if-shell -F \"#{{pane_dead}}\" \"{detach}\""),
        ],
    ]
}

/// TRS-FR-KBOS: hides the "Pane is dead" line where tmux has the option
/// (tmux 3.3 and later). An older tmux refuses it, which changes nothing else.
pub fn quiet_dead_pane() -> Vec<String> {
    strings(&["set-option", "-gw", "remain-on-exit-format", ""])
}

/// TRS-FR-BHCB, TRS-FR-KMWO, TRS-FR-QMZM: starts the server where it does not
/// run, sets its options, creates the session, and writes the metadata, in one
/// command list. The options come first, so the new pane gets them.
pub fn create(session: &str, command: &HostCommand, meta: &Meta) -> Vec<String> {
    let mut commands = vec![strings(&["start-server"])];
    commands.extend(server_options());
    let mut new_session = strings(&["new-session", "-d", "-s"]);
    new_session.push(session.to_string());
    new_session.extend(strings(&["-x", "80", "-y", "24", "-c"]));
    // tmux expands formats in a start directory, so a `#` of the path is
    // written as `##`.
    new_session.push(argv_safe(&command.cwd.replace('#', "##")));
    for (name, value) in &command.env {
        new_session.push("-e".into());
        new_session.push(argv_safe(&format!("{name}={value}")));
    }
    new_session.push("--".into());
    new_session.extend(program_argv(command).iter().map(|a| argv_safe(a)));
    commands.push(new_session);
    commands.extend(session_hooks(session));
    commands.extend(meta.set_commands(session));
    join(commands)
}

/// The words after `--`. tmux runs one single word through a shell, which
/// splits a path with a space; so a program with no argument runs through
/// `exec "$0"`, which keeps the path whole and leaves the program alone in
/// the pane.
fn program_argv(command: &HostCommand) -> Vec<String> {
    if command.args.is_empty() {
        return vec!["/bin/sh".into(), "-c".into(), "exec \"$0\"".into(), command.program.clone()];
    }
    let mut argv = vec![command.program.clone()];
    argv.extend(command.args.iter().cloned());
    argv
}

/// TRS-FR-IEKU: every session of the server, with its metadata and its pane.
pub fn list() -> Vec<String> {
    vec!["list-sessions".into(), "-F".into(), list_format()]
}

/// TRS-FR-TCBS: the number of lines above the visible screen.
pub fn history_size(session: &str) -> Vec<String> {
    vec![
        "display-message".into(),
        "-p".into(),
        "-t".into(),
        pane(session),
        "#{history_size}".into(),
    ]
}

/// TRS-FR-KBOS: the pane state of one session. `display-message` succeeds with
/// no output for a session that is gone, so `has-session` comes first and
/// fails for it.
pub fn pane_state(session: &str) -> Vec<String> {
    join(vec![
        vec!["has-session".into(), "-t".into(), exact(session)],
        vec!["display-message".into(), "-p".into(), "-t".into(), pane(session), PANE_FORMAT.into()],
    ])
}

/// TRS-FR-TCBS, TRS-FR-HJZR: the pane's lines with their colours and with
/// wrapped lines joined. `above_screen` keeps only the lines above the
/// visible screen; else every line of the pane.
pub fn capture(session: &str, above_screen: bool) -> Vec<String> {
    let mut args = strings(&["capture-pane", "-p", "-e", "-J", "-t"]);
    args.push(pane(session));
    args.extend(strings(&["-S", "-"]));
    if above_screen {
        args.extend(strings(&["-E", "-1"]));
    }
    args
}

/// TRS-FR-CXSV: removes one session.
pub fn kill(session: &str) -> Vec<String> {
    vec!["kill-session".into(), "-t".into(), exact(session)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminals::model::*;

    fn meta() -> Meta {
        Meta {
            project: "/p".into(),
            label: "Host shell 1".into(),
            target: TargetRef::Worktree { path: "/p".into() },
            kind: TerminalKind::HostShell,
            vendor: None,
            runs_in: RunsIn::Host,
            pinned: false,
            theme: None,
            created_at: "2026-11-22T10:00:00Z".into(),
        }
    }

    fn command() -> HostCommand {
        HostCommand {
            program: "/bin/zsh".into(),
            args: vec!["-l".into(), "-i".into()],
            cwd: "/p/checkout".into(),
            env: crate::terminals::process::terminal_env(),
            env_remove: vec![],
        }
    }

    // TRS-FR-ANJK: the options make a plain terminal with no key of tmux.
    #[test]
    fn the_server_looks_like_a_plain_terminal() {
        let options = server_options();
        let has = |words: &[&str]| options.iter().any(|c| c == &strings(words));
        assert!(has(&["set-option", "-g", "status", "off"]));
        assert!(has(&["set-option", "-g", "prefix", "None"]));
        assert!(has(&["set-option", "-g", "prefix2", "None"]));
        assert!(has(&["set-option", "-g", "mouse", "off"]));
        assert!(has(&["set-option", "-g", "history-limit", "10000"]));
        assert!(has(&["set-option", "-s", "terminal-overrides[90]", "*:smcup@:rmcup@"]));
        assert!(has(&["set-option", "-gw", "remain-on-exit", "on"]));
        assert!(!options.iter().any(|c| c[0] == "unbind-key"));
        let unbinds = unbind_all();
        for table in ["prefix", "root", "copy-mode", "copy-mode-vi"] {
            assert!(unbinds.contains(&strings(&["unbind-key", "-a", "-T", table])), "{table}");
        }
    }

    // TRS-FR-BHCB, TRS-FR-KMWO, TRS-FR-QMZM: one command list sets the options
    // before it creates the session, gives the two variables with -e, runs the
    // command directly, and then writes the metadata.
    #[test]
    fn the_create_list() {
        let args = create("syn-abc-t1", &command(), &meta());
        let new_at = args.iter().position(|a| a == "new-session").unwrap();
        let history_at = args.iter().position(|a| a == "history-limit").unwrap();
        assert!(history_at < new_at);
        let tail = &args[new_at..];
        let expected = strings(&[
            "new-session",
            "-d",
            "-s",
            "syn-abc-t1",
            "-x",
            "80",
            "-y",
            "24",
            "-c",
            "/p/checkout",
            "-e",
            "TERM=xterm-256color",
            "-e",
            "COLORTERM=truecolor",
            "--",
            "/bin/zsh",
            "-l",
            "-i",
            ";",
        ]);
        assert_eq!(&tail[..expected.len()], expected.as_slice());
        assert!(args
            .windows(4)
            .any(|w| w
                == strings(&["set-option", "-t", "=syn-abc-t1:", "@synthesis_label"]).as_slice()));
        assert_eq!(args.iter().filter(|a| a.starts_with("-e")).count(), 2);
    }

    // TRS-FR-BHCB, TRS-FR-QMZM: a value that ends with `;` keeps it.
    #[test]
    fn a_trailing_semicolon_is_kept() {
        assert_eq!(argv_safe("a;"), "a\\;");
        assert_eq!(argv_safe("a;b"), "a;b");
        assert_eq!(set_option("s", "@k", "v;")[4], "v\\;");
        let mut odd = command();
        odd.cwd = "/p/odd;".into();
        assert!(create("s", &odd, &meta()).contains(&"/p/odd\\;".to_string()));
    }

    // TRS-FR-ASKO, TRS-FR-BHCB: a program with no argument, as an Agent CLI,
    // runs whole also where its path holds a space; a `#` of the start
    // directory is kept.
    #[test]
    fn a_lone_program_and_a_hash_in_the_directory() {
        let mut agent = command();
        agent.program = "/Users/Jane Doe/.local/bin/claude".into();
        agent.args.clear();
        agent.cwd = "/p/a#b".into();
        let args = create("s", &agent, &meta());
        let dashes = args.iter().position(|a| a == "--").unwrap();
        assert_eq!(
            &args[dashes + 1..dashes + 5],
            strings(&["/bin/sh", "-c", "exec \"$0\"", "/Users/Jane Doe/.local/bin/claude"])
                .as_slice()
        );
        assert!(args.contains(&"/p/a##b".to_string()));
    }

    // TRS-FR-CXSV, TRS-FR-TCBS: the session commands name one session exactly.
    #[test]
    fn targets_are_exact() {
        assert_eq!(kill("s"), strings(&["kill-session", "-t", "=s"]));
        assert!(capture("s", true).ends_with(&strings(&["-S", "-", "-E", "-1"])));
        assert!(capture("s", false).ends_with(&strings(&["-S", "-"])));
        assert!(pane_state("s").contains(&"=s:".to_string()));
    }
}

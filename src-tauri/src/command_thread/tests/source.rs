//! CEX-FR-DBBS: the static check that no command outside CEX-FR-IXRE can run
//! on the main thread.

use std::path::{Path, PathBuf};

/// CEX-FR-IXRE: the commands that run on the main thread, and the reason.
const MAIN_THREAD: &[(&str, &str)] = &[
    ("write_terminal_input", "keystrokes keep their order; the bytes go to a queue"),
    ("resize_terminal", "sizes keep their order; a resize is one ioctl"),
    ("acknowledge_terminal_output", "acknowledgements keep their order; it sets a counter"),
    ("set_save_menu_state", "the last state wins; it only queues a menu update"),
    ("set_find_menu_state", "the last state wins; it only queues a menu update"),
    ("set_zoom_menu_state", "the last state wins; it only queues a menu update"),
];

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

struct Command {
    file: String,
    name: String,
    attribute: String,
    is_async: bool,
}

/// Every function that carries a Tauri command attribute, with that attribute.
fn commands() -> Vec<Command> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    let mut found = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(&file).unwrap();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0;
        while i < lines.len() {
            let start = i;
            i += 1;
            let first = lines[start].trim();
            let is_command = first.starts_with("#[tauri::command")
                || first.starts_with("#[project_scope_macros::blocking_command")
                || first.starts_with("#[blocking_command");
            if !is_command {
                continue;
            }
            // An attribute can span lines, as `#[tauri::command(\n rename_all = ..)]`.
            let mut attribute = first.to_string();
            let mut end = start;
            while !attribute.ends_with(']') && end + 1 < lines.len() {
                end += 1;
                attribute.push_str(lines[end].trim());
            }
            let signature = lines[end + 1..]
                .iter()
                .map(|l| l.trim())
                .find(|l| !l.is_empty() && !l.starts_with("#[") && !l.starts_with("//"))
                .unwrap_or_default();
            let Some(after_fn) = signature.split("fn ").nth(1) else { continue };
            let name = after_fn.split(['(', '<']).next().unwrap_or_default().to_string();
            found.push(Command {
                file: file.strip_prefix(&root).unwrap().display().to_string(),
                name,
                attribute,
                is_async: signature.split("fn ").next().unwrap_or_default().contains("async"),
            });
        }
    }
    found
}

// CEX-FR-DBBS, CEX-FR-JBTB, CEX-FR-KFVT: a synchronous command outside
// CEX-FR-IXRE carries `blocking_command`, so it runs on a worker and tells the
// runtime that it blocks. The check names each command that breaks the rule.
#[test]
fn no_command_outside_the_list_runs_on_the_main_thread() {
    let all = commands();
    // The scan must see every registered command, or a command it cannot read
    // would escape the check.
    let unseen: Vec<&str> = crate::command_names::COMMAND_NAMES
        .iter()
        .copied()
        .filter(|name| !all.iter().any(|c| c.name == *name))
        .collect();
    assert!(unseen.is_empty(), "the scan does not see these commands: {unseen:?}");
    let offenders: Vec<String> = all
        .iter()
        .filter(|c| !c.is_async)
        .filter(|c| !c.attribute.starts_with("#[project_scope_macros::blocking_command"))
        .filter(|c| {
            let listed = MAIN_THREAD.iter().any(|(name, _)| *name == c.name);
            !(listed && c.attribute == "#[tauri::command]")
        })
        .map(|c| format!("{} in {} ({})", c.name, c.file, c.attribute))
        .collect();
    assert!(
        offenders.is_empty(),
        "these synchronous commands must carry #[project_scope_macros::blocking_command] \
         (CEX-FR-JBTB): {offenders:#?}"
    );
}

// CEX-FR-IXRE, CEX-FR-DBBS: each listed command exists and runs on the main
// thread, so the list never names a command that has moved or gone.
#[test]
fn every_listed_command_runs_on_the_main_thread() {
    let all = commands();
    for (name, _) in MAIN_THREAD {
        let command = all.iter().find(|c| c.name == *name);
        let command = command.unwrap_or_else(|| panic!("{name} is not a command"));
        assert!(!command.is_async, "{name} is async");
        assert_eq!(command.attribute, "#[tauri::command]", "{name}");
    }
}

// CEX-FR-JBTB: an async command carries the plain Tauri attribute; it already
// runs off the main thread.
#[test]
fn async_commands_take_the_plain_attribute() {
    let wrong: Vec<String> = commands()
        .into_iter()
        .filter(|c| c.is_async && c.attribute == "#[project_scope_macros::blocking_command]")
        .map(|c| c.name)
        .collect();
    assert!(wrong.is_empty(), "{wrong:?}");
}

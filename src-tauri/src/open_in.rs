//! Outside-app hand-offs of a Project panel item (PST-FR-JJGI, PST-FR-ZGNE,
//! PST-FR-AGMX): the system terminal application in the item's folder, and the
//! platform file manager with the item selected.

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::fs::{EntryKind, FsError, RootFs};
use crate::library::{ensure_inside_root, ERR_PATH_ESCAPES_ROOT};
use crate::logging::{log_info, log_warn, Domain, BUFFER};
use crate::project::ProjectState;

pub const ERR_PATH_NOT_FOUND: &str = "path not found";
pub const ERR_NO_TERMINAL_FOUND: &str = "no_terminal_found";
pub const ERR_TERMINAL_START_FAILED: &str = "terminal_start_failed";
pub const ERR_REVEAL_FAILED: &str = "reveal_failed";

/// The desktop platforms with a system terminal of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Windows,
    Linux,
}

impl Platform {
    pub fn current() -> Platform {
        if cfg!(target_os = "macos") {
            Platform::MacOs
        } else if cfg!(windows) {
            Platform::Windows
        } else {
            Platform::Linux
        }
    }
}

/// One way to start a terminal application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Launch {
    pub program: String,
    pub args: Vec<OsString>,
    pub cwd: PathBuf,
}

impl Launch {
    fn new(program: &str, args: Vec<OsString>, dir: &Path) -> Launch {
        Launch { program: program.to_string(), args, cwd: dir.to_path_buf() }
    }
}

/// `--flag=<dir>` as one argument.
fn flag_with(flag: &str, dir: &Path) -> OsString {
    let mut arg = OsString::from(flag);
    arg.push(dir.as_os_str());
    arg
}

/// PST-FR-JJGI: the terminal applications to try, in order, for `dir`.
pub fn launch_plan(platform: Platform, dir: &Path) -> Vec<Launch> {
    let d = || dir.as_os_str().to_os_string();
    match platform {
        Platform::MacOs => {
            vec![Launch::new("open", vec!["-a".into(), "Terminal".into(), d()], dir)]
        }
        // `cmd.exe` reads `&`, `|`, `<`, `>`, `^` and `%` in its command line,
        // and Windows Terminal reads `;` as a separator. So the folder is never
        // an argument of `cmd.exe`, the new window takes the working directory,
        // and Windows Terminal is not used for a folder with `;`.
        Platform::Windows => {
            let mut plan = Vec::new();
            if !dir.to_string_lossy().contains(';') {
                plan.push(Launch::new("wt.exe", vec!["-d".into(), d()], dir));
            }
            plan.push(Launch::new(
                "cmd.exe",
                vec!["/c".into(), "start".into(), "".into(), "cmd.exe".into()],
                dir,
            ));
            plan
        }
        Platform::Linux => vec![
            Launch::new("x-terminal-emulator", Vec::new(), dir),
            Launch::new("gnome-terminal", vec![flag_with("--working-directory=", dir)], dir),
            Launch::new("konsole", vec!["--workdir".into(), d()], dir),
            Launch::new("xfce4-terminal", vec![flag_with("--working-directory=", dir)], dir),
            Launch::new("xterm", Vec::new(), dir),
        ],
    }
}

/// Why no terminal started: the typed code, and the last installed program
/// that failed with the kind of its error. The kind carries no path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartFailure {
    pub code: &'static str,
    pub last: Option<(String, io::ErrorKind)>,
}

/// PST-FR-JJGI: starts the first launch of `plan` that starts, and returns its
/// program. A program that is not installed moves on to the next one. When
/// nothing starts, the error is `no_terminal_found` if no program was there,
/// else `terminal_start_failed`.
pub fn start_first(
    plan: &[Launch],
    mut spawn: impl FnMut(&Launch) -> io::Result<()>,
) -> Result<String, StartFailure> {
    let mut last = None;
    for launch in plan {
        match spawn(launch) {
            Ok(()) => return Ok(launch.program.clone()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => last = Some((launch.program.clone(), e.kind())),
        }
    }
    let code = if last.is_some() { ERR_TERMINAL_START_FAILED } else { ERR_NO_TERMINAL_FOUND };
    Err(StartFailure { code, last })
}

/// Starts `launch` with no standard streams, and reaps it on a thread of its
/// own so that no zombie stays behind.
fn spawn_detached(launch: &Launch) -> io::Result<()> {
    let mut child = Command::new(&launch.program)
        .args(&launch.args)
        .current_dir(&launch.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// The absolute location of the project-relative `path`. The tree root is `""`.
fn absolute(root: &RootFs, path: &str) -> PathBuf {
    path.split('/')
        .filter(|part| !part.is_empty())
        .fold(root.path().to_path_buf(), |p, part| p.join(part))
}

/// The kind of the entry at `path`, with a missing entry as `path not found`.
fn entry_kind(root: &RootFs, path: &str) -> Result<EntryKind, String> {
    match root.file_info(absolute(root, path)) {
        // PST-FR-27: a symbolic link resolves outside the root.
        Ok(info) if info.kind == EntryKind::Symlink => Err(ERR_PATH_ESCAPES_ROOT.to_string()),
        Ok(info) => Ok(info.kind),
        Err(FsError::NotFound { .. }) => Err(ERR_PATH_NOT_FOUND.to_string()),
        Err(
            FsError::SymlinkRefused { .. }
            | FsError::EscapesAllowedRoots { .. }
            | FsError::PathEscape { .. },
        ) => Err(ERR_PATH_ESCAPES_ROOT.to_string()),
        Err(other) => Err(other.to_string()),
    }
}

/// PST-FR-JJGI: the folder a terminal starts in: the folder at `path`, or the
/// folder that holds the file at `path`.
pub fn terminal_folder(root: &RootFs, path: &str) -> Result<PathBuf, String> {
    let location = absolute(root, path);
    match entry_kind(root, path)? {
        EntryKind::Dir => Ok(location),
        _ => Ok(location
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| root.path().to_path_buf())),
    }
}

fn warn<R: tauri::Runtime>(app: &tauri::AppHandle<R>, message: &str, error: &str) {
    log_warn(app, &BUFFER, &[Domain::Backend], message, crate::log_fields! { "error" => error });
}

/// PST-FR-JJGI, PST-FR-AGMX: start the system terminal application in the
/// folder of the project-relative `path`.
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn open_in_system_terminal(
    app: tauri::AppHandle,
    path: String,
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<(), String> {
    let root = project.require_root()?;
    ensure_inside_root(&app, &root, "open_in_system_terminal", &path)?;
    let dir = terminal_folder(&root, &path).inspect_err(|error| {
        warn(&app, "system terminal refused", error);
    })?;
    let result = start_first(&launch_plan(Platform::current(), &dir), spawn_detached);
    match result {
        Ok(program) => {
            log_info(
                &app,
                &BUFFER,
                &[Domain::Backend],
                "system terminal started",
                crate::log_fields! { "program" => program },
            );
            Ok(())
        }
        Err(failure) => {
            let (program, kind) = failure
                .last
                .as_ref()
                .map(|(program, kind)| (program.as_str(), format!("{kind:?}")))
                .unwrap_or(("", String::new()));
            log_warn(
                &app,
                &BUFFER,
                &[Domain::Backend],
                "system terminal did not start",
                crate::log_fields! {
                    "error" => failure.code,
                    "program" => program,
                    "kind" => kind,
                },
            );
            Err(failure.code.to_string())
        }
    }
}

/// PST-FR-ZGNE, PST-FR-AGMX: show the node at the project-relative `path`
/// selected in the platform file manager.
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn reveal_in_file_manager(
    app: tauri::AppHandle,
    path: String,
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<(), String> {
    let root = project.require_root()?;
    ensure_inside_root(&app, &root, "reveal_in_file_manager", &path)?;
    let result = entry_kind(&root, &path).and_then(|_| {
        // The opener error text can hold the path or a D-Bus message, so the
        // record names only the typed code.
        tauri_plugin_opener::reveal_item_in_dir(absolute(&root, &path)).map_err(|_| {
            warn(&app, "file manager reveal failed", ERR_REVEAL_FAILED);
            ERR_REVEAL_FAILED.to_string()
        })
    });
    match &result {
        Ok(()) => log_info(
            &app,
            &BUFFER,
            &[Domain::Backend],
            "file manager reveal started",
            crate::log_fields! {},
        ),
        Err(error) if error != ERR_REVEAL_FAILED => {
            warn(&app, "file manager reveal refused", error)
        }
        Err(_) => {}
    }
    result
}

#[cfg(test)]
#[path = "open_in/tests.rs"]
mod tests;

//! Structural checks of the terminal module's reach.

// TRS-FR-FDXW: no agent loop, tool, or remote module reaches a terminal. Only
// the module itself and the named lifetime hooks call into it.
#[test]
fn only_the_lifetime_hooks_reach_the_module() {
    use std::path::{Path, PathBuf};
    fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                rust_sources(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(path);
            }
        }
    }
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_sources(&src, &mut files);
    const ALLOWED: &[&str] = &[
        "lib.rs",
        "command_names.rs",
        "project/close.rs",
        "menu/exit.rs",
        "streams/removal.rs",
        "worktree/removal.rs",
    ];
    for file in files {
        let rel = file.strip_prefix(&src).unwrap().to_string_lossy().replace('\\', "/");
        if rel.starts_with("terminals") || rel == "terminals.rs" {
            continue;
        }
        let text = std::fs::read_to_string(&file).unwrap();
        // The interactive launch builds on the process shape alone.
        let reaches = text
            .lines()
            .filter(|l| {
                l.contains("terminals::")
                    && !l.contains("terminals::process")
                    && !l.contains("terminals::model")
            })
            .count();
        if reaches > 0 {
            assert!(ALLOWED.contains(&rel.as_str()), "{rel} reaches the terminal module");
        }
    }
}

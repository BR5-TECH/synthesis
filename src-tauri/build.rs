use std::path::{Path, PathBuf};

fn main() {
    embed_builtin_skills();
    tauri_build::build()
}

/// DSL-FR-WTPA: compile every file under `resources/skills/` into the binary.
///
/// The build writes `$OUT_DIR/builtin_skills.rs`, which holds one
/// `(relative path, include_str!(absolute path))` pair for each file, sorted by
/// path. `src/skills/builtin.rs` includes it. A file that is not UTF-8 text is
/// not embedded, because a skill is read as text only.
fn embed_builtin_skills() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    let root = manifest.join("..").join("resources").join("skills");
    // Cargo scans a whole folder for changes when this names a folder, so an
    // added, edited, or removed skill file starts a new build of this list.
    println!("cargo:rerun-if-changed={}", root.display());

    let mut files: Vec<(String, PathBuf)> = Vec::new();
    if root.is_dir() {
        collect(&root, &root, &mut files);
    }
    files.sort();

    let mut out = String::from("pub static BUILTIN_SKILL_FILES: &[(&str, &str)] = &[\n");
    for (rel, abs) in &files {
        out.push_str(&format!("    ({rel:?}, include_str!({:?})),\n", abs.display().to_string()));
    }
    out.push_str("];\n");

    let dest = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("builtin_skills.rs");
    std::fs::write(dest, out).expect("write the list of built-in skill files");
}

fn collect(root: &Path, dir: &Path, files: &mut Vec<(String, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        // DSL-FR-05 applies to the source tree too: a symbolic link is not
        // followed, so the binary holds only files that are in the repository.
        if kind.is_symlink() {
            continue;
        }
        // A hidden file, such as `.DS_Store`, is not part of a skill.
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if kind.is_dir() {
            collect(root, &path, files);
        } else if kind.is_file() {
            if std::fs::read_to_string(&path).is_err() {
                println!("cargo:warning=built-in skill file is not UTF-8 text: {}", path.display());
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .expect("a file under the root")
                .components()
                .map(|c| c.as_os_str().to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            let abs = path.canonicalize().unwrap_or(path);
            files.push((rel, abs));
        }
    }
}

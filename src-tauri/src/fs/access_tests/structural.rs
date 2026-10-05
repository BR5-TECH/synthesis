//! The structural guarantee of FSA-FR-19.

use super::*;

// ---------------------------------------------------------------------------
// FSA-FR-19 — the structural guarantee
// ---------------------------------------------------------------------------

/// Every filesystem operation in the backend goes through `FsAccess`.
///
/// This is the test that makes FSA-FR-19 a fact rather than an intention. The
/// module's public surface is checked first — the guarded operations exist only
/// as methods, so there is no free function to compose a path into — and then
/// the rest of the crate is swept for raw `std::fs` calls that would sidestep
/// them entirely.
///
/// It reads the source because there is no runtime artefact to inspect: a
/// module that called `std::fs::write` directly would compile, pass every other
/// test, and quietly write wherever it was pointed. The two exceptions are
/// named individually, so adding a third is a decision someone has to make here
/// rather than a line that slips through review.
#[test]
fn no_backend_module_reaches_the_filesystem_outside_the_helper() {
    use std::collections::BTreeSet;

    /// `path:function` pairs allowed to call `std::fs` directly, each because
    /// the path it touches lies outside every allowlistable root by
    /// construction. Both are documented at their call site.
    ///
    /// The key is a path suffix, not a bare file name. A module that outgrows
    /// one file moves its functions into submodules with common names, and a
    /// bare `runner.rs` would then both lose its own exemption and hand one to
    /// any other `runner.rs` in the crate. Matching on the suffix keeps an
    /// exemption attached to the function it was granted for.
    const EXCEPTIONS: &[(&str, &str)] = &[
        // A user-named CLI binary, anywhere on the machine (AII-FR-05).
        ("agentic/runner.rs", "is_executable"),
        // Git's own admin directory, which for a linked worktree lives under
        // the *primary* checkout.
        ("worktree.rs", "branch_from_head_file"),
        // The user-global store's bootstrap: its scope is the directory it
        // must first create, so no instance can exist to create it with.
        ("global_settings.rs", "with_path"),
        // Establishes what a path *is* rather than reading it — the same job
        // FSA-FR-18 does when it canonicalises a root. It runs on git workdirs
        // and git directories, which for a linked worktree sit outside the
        // active root, and it is what makes two spellings of one directory
        // comparable. No content is read.
        ("changes.rs", "canonicalize_lenient"),
        // Establishes what a repository identity *is* rather than reading it,
        // on exactly the terms `changes.rs` does above. It runs on the
        // repository's primary worktree, which for a linked checkout sits
        // outside the active root, and it is what makes two spellings of one
        // repository select one store (RMS-FR-BNKD). No content is read.
        ("repository_store.rs", "normalize_identity"),
    ];

    /// Spellings that reach the filesystem. Path-shaped *method* calls are in
    /// here too — `p.metadata()` stats exactly as `std::fs::metadata(p)` does,
    /// and `RootFs` derefs to `Path`, so `root.read_dir()` compiles and reads
    /// like a guarded call while being anything but.
    const REACHING: &[&str] = &[
        // Free-function spellings.
        "std::fs::read", "std::fs::read_to_string", "std::fs::write",
        "std::fs::create_dir", "std::fs::create_dir_all", "std::fs::remove_file",
        "std::fs::remove_dir", "std::fs::remove_dir_all", "std::fs::rename",
        "std::fs::copy", "std::fs::canonicalize", "std::fs::read_dir",
        "std::fs::metadata", "std::fs::symlink_metadata", "std::fs::read_link",
        "std::fs::set_permissions", "std::fs::hard_link", "std::fs::DirBuilder",
        "std::fs::File::create", "std::fs::File::open", "std::fs::OpenOptions",
        "std::os::unix::fs::symlink", "std::os::windows::fs::symlink",
        // Aliased or imported spellings, which the qualified list above misses.
        "use std::fs;", "use std::fs as", "use std::fs::",
        // Method spellings on a `Path`/`PathBuf`/`RootFs`.
        ".read_dir()", ".symlink_metadata()", ".canonicalize()", ".read_link()",
        ".metadata()",
    ];

    /// The production half of a source file.
    ///
    /// Anchored on a column-0 `#[cfg(test)]` immediately followed by `mod` —
    /// the real trailing test module. Splitting on the bare string instead
    /// truncates at the first `#[cfg(test)]` *anywhere*, which in this crate is
    /// routinely a test-only field, a test-only method, or (in `lib.rs`) the
    /// literal inside a doc comment on line 7. That mistake silently excludes
    /// thousands of production lines from the sweep, so the boundary is
    /// deliberately narrow and the assertion below double-checks it.
    fn production_half(source: &str) -> &str {
        // `mod tests` and `pub mod tests_support` alike: both are `cfg(test)`
        // modules, and a fixture inside either may build whatever tree it likes.
        let cut = ["\n#[cfg(test)]\nmod ", "\n#[cfg(test)]\npub mod "]
            .iter()
            .filter_map(|marker| source.find(marker))
            .min();
        match cut {
            Some(i) => &source[..i],
            None => source,
        }
    }

    fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                rust_sources(&p, out);
            } else if p.extension().and_then(|e| e.to_str()) == Some("rs") {
                out.push(p);
            }
        }
    }

    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_sources(&src, &mut files);
    assert!(files.len() > 20, "the sweep found no sources to check");

    // FSA-FR-19: a `tests/` directory is exempted by name, and a name is not
    // evidence. Each one must be a module its parent declares under
    // `#[cfg(test)]`, so a production directory somebody happens to call
    // `tests` cannot slip the whole sweep — the exemption never opens the file,
    // so no per-file check downstream would catch it.
    //
    // A declaration is `#[cfg(test)]` followed by `mod tests;`, with an
    // optional visibility and an optional `#[path]` between them. All three
    // spellings are in use, so matching only the bare one would reject a real
    // test module and, worse, invite somebody to "fix" it by widening the
    // exemption instead.
    fn declares_a_test_module(source: &str, name: &str) -> bool {
        let decl = format!("mod {name};");
        source.split("#[cfg(test)]").skip(1).any(|after| {
            let head: String = after
                .lines()
                .take(3)
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with("#[path"))
                .collect::<Vec<_>>()
                .join(" ");
            head.starts_with(&decl)
                || head.starts_with(&format!("pub {decl}"))
                || head.starts_with(&format!("pub(crate) {decl}"))
                || head.starts_with(&format!("pub(super) {decl}"))
        })
    }

    // The files that could declare `path` as its parent's test module.
    let declaring_candidates = |dir: &Path| -> Vec<PathBuf> {
        let parent = dir.parent().expect("a path under src has a parent");
        vec![
            parent.with_extension("rs"),
            parent.join("mod.rs"),
            src.join("lib.rs"),
        ]
    };

    // Is `path` declared as a test module under the name it carries?
    let declared_under_cfg_test = |path: &Path| -> bool {
        let Some(name) = path.file_stem().and_then(|n| n.to_str()) else {
            return false;
        };
        declaring_candidates(path).iter().any(|candidate| {
            declares_a_test_module(&fs::read_to_string(candidate).unwrap_or_default(), name)
        })
    };

    let mut exempt_directories: BTreeSet<PathBuf> = BTreeSet::new();
    for file in &files {
        if let Some(dir) = file.parent().filter(|p| p.ends_with("tests")) {
            exempt_directories.insert(dir.to_path_buf());
        }
    }
    for dir in &exempt_directories {
        let declared = declared_under_cfg_test(dir);
        assert!(
            declared,
            "{} is exempted from this sweep by its name alone: no module \
             declares it under #[cfg(test)], so it may hold production code \
             that reaches the filesystem outside FsAccess",
            dir.display(),
        );
    }

    // The same evidence is demanded of a `tests.rs` file. It is exempted by its
    // name exactly as a `tests/` directory is, so it needs the same proof that
    // the name is not the only thing making it a test module.
    for file in files.iter().filter(|f| f.file_name() == Some("tests.rs".as_ref())) {
        let declared = declared_under_cfg_test(file);
        assert!(
            declared,
            "{} is exempted from this sweep by its name alone: no module \
             declares it under #[cfg(test)], so it may hold production code \
             that reaches the filesystem outside FsAccess",
            file.display(),
        );
    }

    // A test module that outgrows one file becomes a directory that keeps the
    // module's own name — `access_tests/` beside `access.rs`. Such a directory
    // is exempt on exactly the proof above and on nothing else: an undeclared
    // one is swept like any other production directory, so the name alone buys
    // nothing.
    let mut proven_test_directories: BTreeSet<PathBuf> = BTreeSet::new();
    for file in &files {
        if let Some(dir) = file.parent() {
            let named_for_tests = dir
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("_tests"));
            if named_for_tests
                && !proven_test_directories.contains(dir)
                && declared_under_cfg_test(dir)
            {
                proven_test_directories.insert(dir.to_path_buf());
            }
        }
    }

    // The one predicate both the sweep and its exhaustiveness check use, so
    // they can never drift apart.
    let is_exempt = |file: &PathBuf| -> bool {
        let name = file.file_name().unwrap_or_default();
        file.parent().is_some_and(|p| p.ends_with("fs"))
            || name == "tests.rs"
            || file.parent().is_some_and(|p| p.ends_with("tests"))
            || file
                .parent()
                .is_some_and(|p| proven_test_directories.contains(p))
    };

    let mut swept = 0usize;
    let mut offenders: BTreeSet<String> = BTreeSet::new();
    for file in &files {
        let name = file.file_name().unwrap().to_string_lossy().into_owned();
        // The helper itself is where `std::fs` belongs, and a module's tests are
        // fixtures building the trees the code under test then reads. A test
        // module is a `tests.rs` beside its module, a `tests/` directory under
        // it, or a proven `*_tests/` directory, and the exemption is the same
        // either way: a module that outgrew one file did not thereby become
        // production code.
        if is_exempt(file) {
            continue;
        }
        swept += 1;
        let source = fs::read_to_string(file).unwrap();
        // Production half only: a test may build whatever tree it likes.
        let production = production_half(&source);
        // A file whose test module the split failed to find would be swept
        // whole, which is noisy but safe. A file truncated to almost nothing is
        // the dangerous direction, and this is what catches it.
        assert!(
            source.len() < 400 || production.len() * 20 > source.len(),
            "{name}: the production/test boundary looks wrong — only {} of {} bytes \
             would be swept, so most of the file is going unchecked",
            production.len(),
            source.len(),
        );

        let mut current_fn = String::new();
        for line in production.lines() {
            let trimmed = line.trim_start();
            // Every `fn` spelling, so a body never inherits the previous
            // function's name — which, if that name were an exception, would
            // silently excuse it.
            let after_vis = trimmed
                .strip_prefix("pub(crate) ")
                .or_else(|| trimmed.strip_prefix("pub(super) "))
                .or_else(|| trimmed.strip_prefix("pub(self) "))
                .or_else(|| trimmed.strip_prefix("pub "))
                .unwrap_or(trimmed);
            let after_mods = after_vis
                .strip_prefix("async ")
                .or_else(|| after_vis.strip_prefix("unsafe "))
                .or_else(|| after_vis.strip_prefix("const "))
                .unwrap_or(after_vis);
            if let Some(rest) = after_mods.strip_prefix("fn ") {
                current_fn = rest
                    .split(|c: char| !c.is_alphanumeric() && c != '_')
                    .next()
                    .unwrap_or("")
                    .to_string();
            }
            if trimmed.starts_with("//") {
                continue;
            }
            for call in REACHING {
                if line.contains(call)
                    && !EXCEPTIONS
                        .iter()
                        .any(|(f, func)| file.ends_with(f) && *func == current_fn)
                {
                    offenders.insert(format!("{name}::{current_fn} -> {call}"));
                }
            }
        }
    }

    // And every file the sweep passed over was passed over for a reason this
    // test examined.
    //
    // A share of the file count cannot carry this claim. A module whose tests
    // outgrow one file becomes a directory of small files, so the exempt share
    // moves with how tests are laid out and says nothing about how much
    // production code is read. What must hold is narrower and stronger: each
    // exemption is one of the three kinds above, and each kind is checked —
    // `src/fs` is the helper's own directory, and both test-module shapes are
    // proved by the declaration checks. Nothing is exempt by accident.
    let accounted = files.iter().filter(|file| is_exempt(file)).count();
    assert_eq!(
        swept + accounted,
        files.len(),
        "{} sources are neither swept nor exempted by a checked rule",
        files.len() - swept - accounted,
    );

    // A collapse of the sweep itself still fails loudly. The floor is absolute
    // rather than proportional, for the reason above.
    assert!(
        swept > 100,
        "only {swept} sources are swept — the sweep has collapsed",
    );

    assert!(
        offenders.is_empty(),
        "these reach the filesystem outside `FsAccess`, so nothing confines \
         them to the allowlist (FSA-FR-19). Route them through the instance, or \
         — if the path genuinely lies outside every root — document why at the \
         call site and add it to EXCEPTIONS here:\n  {}",
        offenders.into_iter().collect::<Vec<_>>().join("\n  ")
    );
}

/// The guarded operations are reachable only as methods (FSA-FR-19).
///
/// The companion of the sweep above: that one proves nothing bypasses the
/// module, this one proves the module offers no bypass. `discover_project_root`
/// and `app_data_dir` are the spec's own free functions and are expected here;
/// the rest are pure path or byte helpers that open nothing.
#[test]
fn the_modules_free_functions_are_only_the_ones_the_spec_names() {
    // The whole file: a `pub fn` at column 0 is a free function of the module,
    // and the test module's own functions are indented, so they cannot match.
    let source = include_str!("../mod.rs");

    let free: Vec<&str> = source
        .lines()
        .filter_map(|l| l.strip_prefix("pub fn "))
        .filter_map(|r| r.split(|c: char| !c.is_alphanumeric() && c != '_').next())
        .collect();

    let expected = [
        // The spec's free functions (Contract surface -> Free functions).
        "discover_project_root",
        "app_data_dir",
        "short_data_dir",
        // Pure helpers: no file is opened by any of them.
        "sha256_bytes",
        "merge_gitignore",
        "resolve_under",
        "resolve_inside",
        "map_dialog_result",
    ];
    for f in &free {
        assert!(
            expected.contains(f),
            "`fs::{f}` is a free function that reaches the filesystem. Every \
             guarded operation must be a method on `FsAccess`, or the allowlist \
             is advisory (FSA-FR-19)."
        );
    }
    for e in expected {
        if e == "sha256_bytes" || e == "merge_gitignore" {
            continue; // pure, may be inlined away by a future refactor
        }
        assert!(free.contains(&e), "`fs::{e}` should still be a free function");
    }
}

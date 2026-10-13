//! The `.gitignore` filter on a watcher batch (ASC-FR-RSGA, BMI-FR-GJLW,
//! CHC-FR-AADZ).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

/// A project root in a temporary directory, with `files` written as
/// `(path, contents)`. Parent folders are created as needed.
fn project(files: &[(&str, &str)]) -> (TempDir, crate::fs::RootFs) {
    let tmp = TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(tmp.path());
    for (rel, text) in files {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    (tmp, root)
}

/// The paths `drop_gitignored` keeps from `paths`, and how many it removed.
fn kept(root: &crate::fs::RootFs, paths: &[&str]) -> (Vec<String>, usize) {
    let mut out: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
    let removed = drop_gitignored(root, &mut out);
    (out, removed)
}

// ASC-FR-RSGA: with no ignore rule anywhere, every path stays.
#[test]
fn without_any_gitignore_nothing_is_removed() {
    let (_tmp, root) = project(&[("a.md", "x"), ("target/debug/x", "x")]);
    let (out, removed) = kept(&root, &["a.md", "target/debug/x", "gone.md"]);
    assert_eq!(out, vec!["a.md", "target/debug/x", "gone.md"]);
    assert_eq!(removed, 0);
}

// ASC-FR-RSGA, BMI-FR-GJLW, CHC-FR-AADZ: a build's writes into an ignored
// folder are removed, and the other paths keep their order.
#[test]
fn paths_in_an_ignored_folder_are_removed_and_the_rest_keep_their_order() {
    let (_tmp, root) = project(&[
        (".gitignore", "target/\nnode_modules/\n"),
        ("target/debug/x", "x"),
        ("node_modules/.vite/deps.json", "x"),
        ("src/a.md", "x"),
        ("b.md", "x"),
    ]);
    let (out, removed) = kept(
        &root,
        &["src/a.md", "target/debug/x", "b.md", "node_modules/.vite/deps.json", "target"],
    );
    assert_eq!(out, vec!["src/a.md", "b.md"]);
    assert_eq!(removed, 3);
}

// ASC-FR-RSGA: a deleted path has no kind left to read. Below an ignored
// folder it is still removed, because its ancestor is a folder by definition.
// A deleted path that only a folder-only rule matches is matched as a file,
// so it stays: one redundant pass rather than a missed change.
#[test]
fn deleted_paths_are_matched_through_their_ancestors() {
    let (_tmp, root) = project(&[(".gitignore", "target/\n*.tmp\n")]);
    let (out, _) = kept(&root, &["target/debug/gone", "gone.tmp", "target"]);
    assert_eq!(out, vec!["target"]);
}

// ASC-FR-RSGA: a file in an ignored folder stays ignored whatever a `!` rule
// says, as Git and the scan walk (ASC-FR-09) treat it. A `!` rule against a
// `*` pattern keeps its file.
#[test]
fn a_negation_cannot_reach_into_an_ignored_folder() {
    let (_tmp, root) = project(&[
        (".gitignore", "build/\n!build/keep.txt\nout/*\n!out/keep.txt\n"),
        ("build/keep.txt", "x"),
        ("out/keep.txt", "x"),
        ("out/other", "x"),
    ]);
    let (out, _) = kept(&root, &["build/keep.txt", "out/keep.txt", "out/other"]);
    assert_eq!(out, vec!["out/keep.txt"]);
}

// ASC-FR-RSGA: the deepest `.gitignore` with an opinion decides, and a
// pattern in a nested file is anchored to its own folder.
#[test]
fn nested_gitignore_files_apply_to_their_own_folder() {
    let (_tmp, root) = project(&[
        (".gitignore", "*.log\n"),
        ("sub/.gitignore", "!keep.log\n/gen\n"),
        ("sub/keep.log", "x"),
        ("sub/other.log", "x"),
        ("other.log", "x"),
        ("sub/gen/x", "x"),
        ("gen/x", "x"),
    ]);
    let (out, _) =
        kept(&root, &["sub/keep.log", "sub/other.log", "other.log", "sub/gen/x", "gen/x"]);
    assert_eq!(out, vec!["sub/keep.log", "gen/x"]);
}

// ASC-FR-RSGA: the repository's `info/exclude` ignores as the scan walk's
// `git_exclude(true)` does, and a `!` rule in a `.gitignore` overrides it.
#[test]
fn info_exclude_ignores_and_a_gitignore_negation_overrides_it() {
    let (_tmp, root) = project(&[
        (".git/info/exclude", "scratch/\n*.bak\n"),
        (".gitignore", "!keep.bak\n"),
        ("scratch/x", "x"),
        ("a.bak", "x"),
        ("keep.bak", "x"),
    ]);
    let (out, _) = kept(&root, &["scratch/x", "a.bak", "keep.bak"]);
    assert_eq!(out, vec!["keep.bak"]);
}

// ASC-FR-RSGA, DSL-FR-19: the four skill folders stay under an ignored
// parent, and so does a parent itself, whose delete removes every skill
// under it. Other ignored paths beside them, and look-alike prefixes, go.
#[test]
fn skill_folders_stay_even_when_ignored() {
    let (_tmp, root) =
        project(&[(".gitignore", ".claude/\n.codex/\n.github/\n.opencode/\n.claudex/\n")]);
    for (folder, _) in crate::skills::SKILL_FOLDERS {
        let parent = folder.split('/').next().unwrap();
        let skill = format!("{folder}/a/SKILL.md");
        let beside = format!("{parent}/settings.json");
        let look_alike = format!("{folder}x/y");
        let (out, _) = kept(&root, &[&skill, parent, folder, &beside, &look_alike]);
        assert_eq!(out, vec![skill.clone(), parent.to_string(), folder.to_string()], "{folder}");
    }
    let (out, _) = kept(&root, &[".claudex/skills/x"]);
    assert!(out.is_empty(), "a look-alike of `.claude` is not a skill folder");
}

// ASC-FR-RSGA, ASC-FR-20: `.synthesis/` stays even when ignored, because its
// consumers apply their own rules.
#[test]
fn synthesis_paths_stay_even_when_ignored() {
    let (_tmp, root) = project(&[(".gitignore", ".synthesis/\n")]);
    let (out, removed) = kept(&root, &[".synthesis/library.toml", ".synthesis"]);
    assert_eq!(out, vec![".synthesis/library.toml", ".synthesis"]);
    assert_eq!(removed, 0);
}

// ASC-FR-RSGA: a line that does not parse is skipped and the other rules
// still apply; CRLF line ends and comments are read as Git reads them.
#[test]
fn a_bad_line_crlf_and_comments_do_not_break_the_rules() {
    let (_tmp, root) = project(&[
        (".gitignore", "# build output\r\n[\r\ndist/\r\n"),
        ("dist/x", "x"),
        ("a.md", "x"),
    ]);
    let (out, _) = kept(&root, &["dist/x", "a.md"]);
    assert_eq!(out, vec!["a.md"]);
}

// ASC-FR-RSGA: the rules are read when the batch is filtered, so a batch that
// carries an edit of `.gitignore` is filtered by the new rules. The
// `.gitignore` itself is never ignored by them.
#[test]
fn the_rules_on_disk_at_filter_time_apply() {
    let (_tmp, root) = project(&[(".gitignore", "old/\n"), ("old/x", "x"), ("new/x", "x")]);
    fs::write(root.join(".gitignore"), "new/\n").unwrap();
    let (out, _) = kept(&root, &[".gitignore", "old/x", "new/x"]);
    assert_eq!(out, vec![".gitignore", "old/x"]);
}

// ASC-FR-RSGA: a linked worktree's `.git` is a file, so there is no
// `info/exclude` to read here. That is not an error.
#[test]
fn a_git_file_instead_of_a_git_folder_is_not_an_error() {
    let (_tmp, root) =
        project(&[(".git", "gitdir: /elsewhere/.git/worktrees/w\n"), (".gitignore", "target/\n")]);
    let (out, _) = kept(&root, &["target/x", "a.md"]);
    assert_eq!(out, vec!["a.md"]);
}

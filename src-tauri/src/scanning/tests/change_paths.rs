//! The changed and removed path channels, symbolic links, and the attribution
//! baseline (ASC-FR-10, ASC-FR-15, ASC-FR-20, ASC-FR-22, ASC-FR-23).
//!
//! One part of `../tests/mod.rs`, which holds the fixtures these run against.

use super::*;

// ASC-FR-22: a path is "removed" when nothing is at it once the burst is
// applied. Existence is read without following a link, so a link occupying
// the path means it is still there — the scan will not surface that link
// (ASC-FR-23), but the path itself is not gone.
#[test]
fn removed_rel_paths_reports_only_what_is_absent() {
    let tmp = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    fs::write(root.join("here.md"), b"x").unwrap();
    fs::create_dir_all(root.join("dir")).unwrap();

    let changed = vec![
        "here.md".to_string(),
        "dir".to_string(),
        "gone.md".to_string(),
        "gone-dir".to_string(),
    ];
    let removed = removed_rel_paths(&changed, root);
    assert_eq!(removed, vec!["gone.md".to_string(), "gone-dir".to_string()]);

    // Nothing removed -> an empty list, not a panic or a phantom entry.
    assert!(removed_rel_paths(&["here.md".to_string()], root).is_empty());
}

// ASC-FR-23: the scan surfaces no node for a symbolic link — neither a
// linked file nor a linked directory — and never descends one, so nothing
// reached through a link appears in the tree. Decided from the link alone:
// `inside` points within the project and is skipped exactly as `outside` is.
#[cfg(unix)]
#[test]
fn scan_skips_symlinks_and_never_descends_them() {
    use std::os::unix::fs::symlink;

    let tmp = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(tmp.path());
    let external = TempDir::new().unwrap();
    fs::write(external.path().join("secret.md"), b"x").unwrap();

    fs::create_dir_all(root.join("real")).unwrap();
    fs::write(root.join("real").join("a.md"), b"a").unwrap();
    symlink(external.path(), root.join("vendor")).unwrap();
    symlink(root.join("real"), root.join("mirror")).unwrap();
    symlink(root.join("real").join("a.md"), root.join("link.md")).unwrap();

    let tree = scan(root);
    let mut paths = Vec::new();
    fn collect(n: &TreeNode, out: &mut Vec<String>) {
        out.push(n.path.clone());
        if let Some(cs) = n.children.as_ref() {
            for c in cs {
                collect(c, out);
            }
        }
    }
    collect(&tree, &mut paths);

    assert!(paths.iter().any(|p| p == "real/a.md"), "real files still appear");
    for banned in ["vendor", "mirror", "link.md"] {
        assert!(
            !paths.iter().any(|p| p == banned),
            "symlink {banned} must not be a node; got {paths:?}"
        );
    }
    assert!(
        !paths.iter().any(|p| p.starts_with("vendor/") || p.starts_with("mirror/")),
        "nothing beneath a linked directory may be walked; got {paths:?}"
    );
}

// ASC-FR-15: changed_rel_paths relativizes under the root, coalesces dups
// (ASC-FR-10), and excludes pruned dirs, atomic-write temp files, and
// out-of-root paths. The attribution file is NOT excluded — that guarantee
// is pinned by `the_raw_path_channel_reports_gitignored_paths_and_prunes_
// only_git` below, which is where ASC-FR-20's filter set is enumerated.
#[test]
fn changed_rel_paths_relativizes_dedups_and_skips_noise() {
    use notify_debouncer_mini::{DebouncedEvent, DebouncedEventKind};
    let root = Path::new("/p");
    let ev = |p: &str| DebouncedEvent {
        path: PathBuf::from(p),
        kind: DebouncedEventKind::Any,
    };
    let batch = Ok(vec![
        ev("/p/a.md"),
        ev("/p/a.md"),                    // duplicate -> coalesced
        ev("/p/sub/b.md"),
        ev("/p/.git/objects/x"),          // pruned (ASC-FR-09)
        ev("/p/.synthesis/cache/c"),      // pruned
        // NTC-FR-16: a note write must not reach the tree channel, or every
        // save in the Notes panel would reload the Library.
        ev("/p/.synthesis/notes/n1.toml"),
        ev("/p/.b.md.tmp.123.456"), // atomic-write temp file
        ev("/other/z.md"),          // outside the watched root
    ]);
    let got = changed_rel_paths(&batch, root);
    assert_eq!(got, vec!["a.md".to_string(), "sub/b.md".to_string()]);
}

#[test]
fn the_raw_path_channel_reports_gitignored_paths_and_prunes_only_git() {
    // ASC-FR-20 / ASC-FR-17, ASC-FR-09, ASC-FR-14: this channel is deliberately NOT filtered by
    // `.gitignore`. `DSL-dynamic-skills-loading.md` enumerates four folders
    // a project may legitimately gitignore (DSL-FR-06), and a gitignored
    // file's edit changes nothing in the tree — so if this list dropped it,
    // that edit would reach no consumer at all and the skills index would
    // stay stale until some unrelated change happened to trigger a pass.
    //
    // The tree channel over-firing for a gitignored path is harmless (a
    // reload finds nothing new); this channel going silent for one is not,
    // which is why the guarantee is pinned here rather than assumed.
    use notify_debouncer_mini::{DebouncedEvent, DebouncedEventKind};
    let root = Path::new("/p");
    let ev = |p: &str| DebouncedEvent {
        path: PathBuf::from(p),
        kind: DebouncedEventKind::Any,
    };
    let batch = Ok(vec![
        // A skill under a folder a project may well gitignore entirely.
        ev("/p/.claude/skills/local/SKILL.md"),
        ev("/p/node_modules/pkg/index.js"),
        // The watcher's own churn.
        ev("/p/.git/refs/heads/main"),
        // Each of these has a dedicated channel of its own, so reporting
        // it here too would be a second notification for one change.
        ev("/p/.synthesis/drafts/d1/files/a.md"),
        ev("/p/.synthesis/notes/n1.toml"),
        ev("/p/.synthesis/comments/c1.toml"),
        ev("/p/.synthesis/cache/x"),
        // The attribution file is NOT one of those — it has no channel of
        // its own, and every file's resolved type derives from it
        // (ASC-FR-06). Dropping it here is what left a pulled folder-scope
        // assignment reaching no consumer at all (BMI-FR-17, BMI-FR-18).
        ev("/p/.synthesis/library.toml"),
        // The filter is subtree-scoped, not `.synthesis`-wide: bare
        // `.synthesis` and a file directly under it have no dedicated
        // channel either, so they ride this one like any other path.
        ev("/p/.synthesis/other.toml"),
    ]);
    assert_eq!(
        changed_rel_paths(&batch, root),
        vec![
            ".claude/skills/local/SKILL.md".to_string(),
            "node_modules/pkg/index.js".to_string(),
            LIBRARY_TOML_REL.to_string(),
            ".synthesis/other.toml".to_string(),
        ],
        "gitignored paths and the attribution file ride this channel; \
         `.git/` and the separately notified `.synthesis/` subtrees do not"
    );
}

#[test]
fn the_attribution_baseline_records_what_is_on_disk_and_clears_with_the_root() {
    let dir = TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();

    let baseline = AttributionBaseline::default();
    assert!(
        baseline.current().is_none(),
        "nothing is recorded until this application writes the file"
    );

    std::fs::write(dir.path().join(LIBRARY_TOML_REL), "[assignments]\n").unwrap();
    baseline.record_from_disk(&root);
    let first = baseline.current().expect("a written file records a checksum");

    // Re-recording identical bytes yields the identical checksum — this is
    // what makes `attribution_is_self_write` recognise our own echo.
    baseline.record_from_disk(&root);
    assert_eq!(baseline.current().as_deref(), Some(first.as_str()));

    // Different bytes, different checksum: an external write cannot be
    // mistaken for the one we recorded.
    std::fs::write(
        dir.path().join(LIBRARY_TOML_REL),
        "[assignments]\n[assignments.\"notes\"]\ntype = \"scenario\"\nscope = \"folder\"\n",
    )
    .unwrap();
    baseline.record_from_disk(&root);
    assert_ne!(baseline.current().as_deref(), Some(first.as_str()));

    // ASC-FR-16: dropped with the content root.
    baseline.clear();
    assert!(baseline.current().is_none());
}

#[cfg(unix)]
#[test]
fn a_symlinked_attribution_file_records_no_baseline() {
    // FSA-FR-10 / ASC-FR-23: `sha256_file` refuses a symlinked leaf, so a
    // symlinked attribution file records `None` and every write — this
    // application's included — reads as external. That is the fail-open
    // direction: it costs a redundant pass per assignment, never a
    // suppressed one. Pinned because it is an assumption, not a decision
    // made here, and `scanning::assign` refuses the same path for the same
    // reason, so the two agree.
    let dir = TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    let target = dir.path().join("elsewhere.toml");
    std::fs::write(&target, "[assignments]\n").unwrap();
    std::os::unix::fs::symlink(&target, dir.path().join(LIBRARY_TOML_REL)).unwrap();

    let baseline = AttributionBaseline::default();
    baseline.record_from_disk(&root);
    assert!(
        baseline.current().is_none(),
        "a symlinked attribution file is refused, not followed"
    );
}

#[test]
fn the_attribution_baseline_records_nothing_when_the_file_is_absent() {
    // An unreadable or absent file must record `None` rather than a stale
    // checksum: `None` reads as EXTERNAL at the next event, which costs one
    // redundant pass, where a stale checksum could suppress a real change.
    let dir = TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    let baseline = AttributionBaseline::default();
    baseline.record_from_disk(&root);
    assert!(baseline.current().is_none());
}

#[test]
fn changed_rel_paths_empty_and_errored_batches_yield_nothing() {
    let root = Path::new("/p");
    assert!(changed_rel_paths(&Ok(Vec::new()), root).is_empty());
    let errored: DebounceEventResult =
        Err(notify_debouncer_mini::notify::Error::generic("boom"));
    assert!(changed_rel_paths(&errored, root).is_empty());
}

#[test]
fn is_atomic_tmp_name_matches_only_the_real_temp_pattern() {
    // Real atomic-write temp files: `.{stem}.tmp.{pid}.{nanos}` -> skipped.
    assert!(is_atomic_tmp_name(".a.md.tmp.123.456"));
    assert!(is_atomic_tmp_name("sub/.report.tmp.42.99"));
    // Innocent dotfiles that merely contain ".tmp." must NOT be skipped.
    assert!(!is_atomic_tmp_name(".config.tmp.md"));
    assert!(!is_atomic_tmp_name(".a.tmp.1.2.3")); // trailing segment
    assert!(!is_atomic_tmp_name(".a.tmp.x.456")); // pid not digits
    assert!(!is_atomic_tmp_name("notes.tmp.1.2")); // no leading dot
    assert!(!is_atomic_tmp_name("plain.md"));
}

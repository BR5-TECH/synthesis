//! The loaders against a real filesystem.
//!
//! One part of `../tests/mod.rs`.

use super::*;

// ------------------------------------------------------------------
// The loaders against a real filesystem
// ------------------------------------------------------------------

/// Give `path` an exact modification time, so an ordering assertion does not
/// rest on how fast the test ran.
fn set_mtime(path: &std::path::Path, unix_secs: u64) {
    let file = std::fs::File::options().write(true).open(path).unwrap();
    file.set_times(
        std::fs::FileTimes::new().set_modified(at(unix_secs)),
    )
    .unwrap();
}

fn write(root: &crate::fs::RootFs, rel: &str, body: &str, unix_secs: u64) {
    let path = root.path().join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, body).unwrap();
    set_mtime(&path, unix_secs);
}

/// PST-FR-31 / PST-FR-23: the widget is ordered by the source
/// files' modification times, capped at five, and holds artifacts alone —
/// a file carrying no artifact type never appears however recently it was
/// written, and rewriting an artifact's `.synthesis/` metadata moves
/// nothing, metadata being no activity source.
#[test]
fn recently_edited_reads_the_project_and_its_filesystem_and_nothing_else() {
    let app = crate::tools::tests::mock_app();
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    for i in 0..8u64 {
        write(&root, &format!("specs/a{i}.spec.md", ), "body\n", 1_000 + i);
    }
    // Carries no artifact type, and is the most recently written file in the
    // project — so its absence below is the assertion (PST-FR-23).
    write(&root, "src/main.rs", "fn main() {}\n", 9_999);

    let items = recently_edited_impl(&app.handle().clone(), &root);
    assert_eq!(items.len(), DASHBOARD_ITEM_CAP, "the loader applies the cap");
    assert_eq!(
        items.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
        vec![
            "specs/a7.spec.md",
            "specs/a6.spec.md",
            "specs/a5.spec.md",
            "specs/a4.spec.md",
            "specs/a3.spec.md",
        ],
    );
    assert!(
        !items.iter().any(|i| i.id == "src/main.rs"),
        "a file carrying no artifact type is not an artifact and never appears, \
             however recently it was written (PST-FR-23)"
    );

    // An external editor rewrites the least recent artifact: it leads.
    set_mtime(&dir.path().join("specs/a0.spec.md"), 20_000);
    let after_external = recently_edited_impl(&app.handle().clone(), &root);
    assert_eq!(after_external[0].id, "specs/a0.spec.md");

    // Artifact metadata is not an activity source: the order is what it was.
    write(&root, ".synthesis/artifacts/a0.toml", "rewritten\n", 30_000);
    assert_eq!(
        recently_edited_impl(&app.handle().clone(), &root),
        after_external,
        "rewriting `.synthesis/artifacts/` metadata moves no item"
    );

    // ASC-FR-19: an artifact whose filename identifies nothing is named by
    // what the file declares. A skill read as `SKILL.md` here while the
    // Project panel reads it as `analyst` is two surfaces disagreeing about
    // one artifact. Written last, so it leads and the assertion names the
    // row it is about.
    write(
        &root,
        ".claude/skills/analyst/SKILL.md",
        "---\nname: analyst\n---\n\nbody\n",
        40_000,
    );
    let named = recently_edited_impl(&app.handle().clone(), &root);
    assert_eq!(
        named[0].name, "analyst",
        "the display name the file declares, not the basename every skill shares"
    );
}

/// PST-FR-17, PST-FR-31: the write's own modification time is what puts an
/// artifact at the head of the widget — no save history is consulted, because
/// none is kept (PST-FR-17) — and an artifact whose file has since gone is
/// absent while every returned item resolves to a file that exists.
#[test]
fn a_save_puts_its_artifact_first_and_a_removed_one_is_absent() {
    let app = crate::tools::tests::mock_app();
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    // `a` is the LEAST recently modified, so its move to the head below can
    // only be the write's doing.
    write(&root, "specs/a.spec.md", "one\n", 1_000);
    write(&root, "specs/b.spec.md", "two\n", 2_000);
    write(&root, "specs/gone.spec.md", "three\n", 3_000);
    assert_eq!(
        recently_edited_impl(&app.handle().clone(), &root)[0].id,
        "specs/gone.spec.md",
        "precondition: `a` is last, so nothing below passes by accident"
    );

    crate::artifacts::save_validated_artifact(
        &root,
        "specs/a.spec.md",
        "rewritten\n",
        &crate::artifacts::ContentTracker::default(),
    )
    .unwrap();
    std::fs::remove_file(dir.path().join("specs/gone.spec.md")).unwrap();

    let items = recently_edited_impl(&app.handle().clone(), &root);
    assert_eq!(
        items[0].id, "specs/a.spec.md",
        "the modification time the write gave the file is what moved it"
    );
    assert_eq!(items[0].kind, ArtifactKind::Markdown);
    assert!(
        !items.iter().any(|i| i.id == "specs/gone.spec.md"),
        "an artifact whose file has gone is absent"
    );
    for item in &items {
        assert!(
            root.file_info(root.path().join(&item.id)).is_ok(),
            "every returned item resolves to a file that exists: {}",
            item.id
        );
    }
}

/// PST-FR-32: five active drafts ordered by prompt-file
/// modification time, an archived and a graduated draft excluded, and the
/// stored record — not the filesystem — deciding each row's name and status.
#[test]
fn active_drafts_reads_the_prompt_times_and_the_stored_records() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();

    let mut ids = Vec::new();
    for i in 0..7u64 {
        let created =
            crate::drafts::create_draft_impl(&root, Some(&format!("draft-{i}")), None).unwrap();
        let prompt = dir
            .path()
            .join(".synthesis/drafts")
            .join(&created.draft.id)
            .join("files")
            .join(&created.file);
        set_mtime(&prompt, 1_000 + i);
        ids.push((created.draft.id, prompt));
    }
    // An archived and a graduated draft, both written most recently of all,
    // so their absence below is the assertion rather than a consequence of
    // the ordering.
    for (name, graduate) in [("archived", false), ("graduated", true)] {
        let created = crate::drafts::create_draft_impl(&root, Some(name), None).unwrap();
        let prompt = dir
            .path()
            .join(".synthesis/drafts")
            .join(&created.draft.id)
            .join("files")
            .join(&created.file);
        set_mtime(&prompt, 50_000);
        if graduate {
            crate::drafts::set_draft_graduated(&root, &created.draft.id, "run-1").unwrap();
        } else {
            crate::drafts::set_draft_status_impl(
                &root,
                &created.draft.id,
                DraftStatus::Archived,
            )
            .unwrap();
        }
    }

    let items = active_drafts_from(crate::drafts::list_drafts_impl(&root).drafts);
    assert_eq!(items.len(), DASHBOARD_ITEM_CAP);
    assert!(items.iter().all(|i| i.status == DraftStatus::Active));
    assert_eq!(
        items.iter().map(|i| i.name.as_str()).collect::<Vec<_>>(),
        vec!["draft-6", "draft-5", "draft-4", "draft-3", "draft-2"],
        "most recent prompt activity first, under each draft's stored name"
    );

    // An external editor rewrites the bottom draft's prompt: it leads.
    set_mtime(&ids[0].1, 60_000);
    let after_external = active_drafts_from(crate::drafts::list_drafts_impl(&root).drafts);
    assert_eq!(after_external[0].draft_id, ids[0].0);
    assert_eq!(after_external[0].name, "draft-0");

    // A rename moves the record and leaves the prompt's bytes — and so the
    // ordering — exactly where they were (DRS-FR-41). Renamed on a draft the
    // widget is actually showing, or the name assertion would be vacuous.
    crate::drafts::rename_draft_impl(&root, &ids[6].0, "renamed").unwrap();
    let after_rename = active_drafts_from(crate::drafts::list_drafts_impl(&root).drafts);
    assert_eq!(
        after_rename.iter().map(|i| i.draft_id.as_str()).collect::<Vec<_>>(),
        after_external.iter().map(|i| i.draft_id.as_str()).collect::<Vec<_>>(),
        "a rename reorders nothing"
    );
    assert!(
        after_rename.iter().any(|i| i.name == "renamed"),
        "and the row reports the new stored name"
    );
}

/// PST-FR-34: the four counts, from the change set and the local refs alone.
#[test]
fn pending_git_counts_the_change_set_and_answers_null_where_a_count_is_unavailable() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());

    // Outside a Git repository: all four counts unavailable, and no error.
    assert_eq!(pending_git_impl(&root).unwrap(), PendingGitActivity::default());

    let repo = git2::Repository::init(dir.path()).unwrap();
    let mut config = repo.config().unwrap();
    config.set_str("user.name", "Test").unwrap();
    config.set_str("user.email", "test@example.com").unwrap();
    drop(config);
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    for i in 0..3 {
        std::fs::create_dir_all(dir.path().join("specs")).unwrap();
        std::fs::write(dir.path().join(format!("specs/a{i}.spec.md")), "body\n").unwrap();
    }
    for i in 0..4 {
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join(format!("src/f{i}.rs")), "fn main() {}\n").unwrap();
    }

    let counts = pending_git_impl(&root).unwrap();
    assert_eq!(counts.modified_artifacts, Some(3));
    assert_eq!(counts.modified_source_files, Some(4));
    assert_eq!(
        counts.unpushed_commits, None,
        "a branch with no upstream reports the commit counts as unavailable, not as zero"
    );
    assert_eq!(counts.fetchable_commits, None);
}

/// PST-FR-34: the two commit counts, and which is which.
///
/// The counts are deliberately **unequal** (2 ahead, 1 behind): with equal
/// ones a loader that swapped `ahead` and `behind` — offering a push where a
/// fetch was due — would pass. No fetch is performed and no network is
/// reached: the upstream here is a purely local ref.
#[test]
fn pending_git_reports_commits_to_push_and_commits_to_fetch_the_right_way_round() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    let repo = git2::Repository::init(dir.path()).unwrap();
    {
        let mut config = repo.config().unwrap();
        config.set_str("user.name", "Test").unwrap();
        config.set_str("user.email", "test@example.com").unwrap();
    }
    // A remote has to be configured for a branch to have an upstream at all
    // — the URL is never dialled: `upstream_sync_state` resolves from local
    // refs alone (GTC-FR-21), which is what makes this loader free of the
    // network (PST-FR-34).
    repo.remote("origin", "https://example.invalid/repo.git").unwrap();

    let commit = |repo: &git2::Repository, message: &str, parent: Option<git2::Oid>| {
        let mut index = repo.index().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = repo.signature().unwrap();
        let parents: Vec<git2::Commit> = parent
            .map(|oid| repo.find_commit(oid).unwrap())
            .into_iter()
            .collect();
        let refs: Vec<&git2::Commit> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &refs)
            .unwrap()
    };

    // A shared base, then one commit only the "remote" holds and two only
    // the branch holds.
    let base = commit(&repo, "base", None);
    let behind_one = commit(&repo, "on the remote", Some(base));
    // The upstream ref, planted directly: this is what `upstream_sync_state`
    // reads, and planting it reaches no network.
    repo.reference("refs/remotes/origin/main", behind_one, true, "upstream")
        .unwrap();
    repo.reference("refs/heads/main", base, true, "rewind").unwrap();
    repo.set_head("refs/heads/main").unwrap();
    let ahead_one = commit(&repo, "mine 1", Some(base));
    commit(&repo, "mine 2", Some(ahead_one));
    {
        let mut branch = repo.find_branch("main", git2::BranchType::Local).unwrap();
        branch.set_upstream(Some("origin/main")).unwrap();
    }

    let counts = pending_git_impl(&root).unwrap();
    assert_eq!(
        counts.unpushed_commits,
        Some(2),
        "commits the branch holds above its upstream"
    );
    assert_eq!(
        counts.fetchable_commits,
        Some(1),
        "commits the upstream holds above the branch"
    );
}

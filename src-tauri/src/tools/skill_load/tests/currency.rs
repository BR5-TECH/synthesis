//! LSK-FR-17, LSK-FR-18: the registry as it stands, and read-only posture.

use super::*;

// ---------------------------------------------------------------------------
// LSK-FR-17 — answers from the registry as it stands (LSK-FR-17)
// ---------------------------------------------------------------------------

#[test]
fn a_skill_added_since_the_last_pass_is_not_yet_loadable_and_an_edited_body_is_current() {
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "present",
        "name: present\ndescription: Indexed at mount time.",
        "# Original body",
    );
    let root = dir.path().to_path_buf();
    let fixture = mounted(dir);

    // Added after the pass: the registry has no descriptor for it, so it is
    // unknown rather than loadable — the call does not go looking on disk.
    write_skill(
        &root,
        ".claude/skills",
        "latecomer",
        "name: latecomer\ndescription: Written after the pass.",
        "# Latecomer",
    );
    assert_eq!(
        call(&fixture, "latecomer", None).unwrap_err(),
        ToolRefusal::SkillNotFound,
        "LSK-FR-17: resolved against the registry as it stands",
    );
    // ...and reachable once a pass has run, which is what makes the assertion
    // above one about staleness rather than about an unwritable fixture.
    fixture.reindex();
    assert!(call(&fixture, "latecomer", None)
        .unwrap()
        .contains("# Latecomer"));

    // The body, by contrast, is read at call time: an edit since the pass that
    // admitted the skill is returned as the file reads now.
    std::fs::write(
        root.join(".claude/skills/present/SKILL.md"),
        "---\nname: present\ndescription: Indexed at mount time.\n---\n# Edited body\n",
    )
    .unwrap();
    assert_eq!(
        call(&fixture, "present", None).unwrap(),
        "# Edited body\n",
        "LSK-FR-17: as the file reads now",
    );
}

#[test]
fn a_call_resolves_without_waiting_on_an_index_pass() {
    // LSK-FR-17 / TLC-FR-16. `block_on` polls exactly once and panics on
    // `Pending`, so reaching the assertion at all is the claim.
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "settled",
        "name: settled\ndescription: Indexed before the call.",
        "# Settled",
    );
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = crate::tools::tests::mock_app();
    let handle = app.handle().clone();
    {
        // Mounted with a full pass PENDING and never run: the registry is
        // empty, and the tool must answer from it rather than block until the
        // first build lands.
        let indexer = app.state::<crate::bm25_index::Bm25Indexer>();
        indexer.mount(&crate::fs::RootFs::for_root(&root));
    }

    let refusal = block_on(SkillLoadTool::new(handle).call(LoadSkillArgs {
        name: "settled".to_string(),
        ecosystem: None,
    }))
    .unwrap_err();
    assert_eq!(
        refusal,
        ToolRefusal::SkillNotFound,
        "LSK-FR-17: what has been enumerated so far, returned immediately",
    );
}

#[test]
fn one_instance_serves_two_callers_without_either_observing_the_other() {
    // TLC-FR-15. Worth running against *this* tool specifically: it is the one
    // holding a filesystem handle, so it is the one where a shared instance
    // could plausibly interleave two reads.
    let fixture = collision_project();
    let tool = std::sync::Arc::new(SkillLoadTool::new(fixture.handle()));

    let a = std::sync::Arc::clone(&tool);
    let b = std::sync::Arc::clone(&tool);
    let one = std::thread::spawn(move || {
        block_on(a.call(LoadSkillArgs {
            name: "review".to_string(),
            ecosystem: Some("codex".to_string()),
        }))
    });
    let two = std::thread::spawn(move || {
        block_on(b.call(LoadSkillArgs {
            name: "review".to_string(),
            ecosystem: Some("opencode".to_string()),
        }))
    });

    assert!(one.join().unwrap().unwrap().contains("codex copy"));
    assert!(two.join().unwrap().unwrap().contains("opencode copy"));
}

// ---------------------------------------------------------------------------
// LSK-FR-18 — read-only (LSK-FR-18)
// ---------------------------------------------------------------------------

#[test]
fn loading_leaves_the_project_exactly_as_it_was() {
    let fixture = collision_project();
    let root = {
        let indexer = fixture.app.state::<crate::bm25_index::Bm25Indexer>();
        indexer.root().expect("mounted")
    };
    let before = snapshot(&root);

    for (name, ecosystem) in [
        ("analyst", None),
        ("review", Some("codex")),
        ("review", None),
        ("nothing at all", None),
        ("", None),
    ] {
        let _ = call(&fixture, name, ecosystem);
    }

    assert_eq!(
        before,
        snapshot(&root),
        "LSK-FR-18: the one file it opens is opened for reading alone",
    );
}

/// Every file under `root` with its contents and modification time.
///
/// The mtime is what makes this an assertion about *how* the file was opened: a
/// handle taken in append or truncate mode would leave the bytes equal here and
/// still be caught.
fn snapshot(root: &std::path::Path) -> Vec<(String, Vec<u8>, std::time::SystemTime)> {
    fn walk(
        dir: &std::path::Path,
        base: &std::path::Path,
        out: &mut Vec<(String, Vec<u8>, std::time::SystemTime)>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, base, out);
            } else if let (Ok(bytes), Ok(meta)) = (std::fs::read(&path), std::fs::metadata(&path)) {
                out.push((
                    path.strip_prefix(base)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .to_string(),
                    bytes,
                    meta.modified().unwrap_or(std::time::UNIX_EPOCH),
                ));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}


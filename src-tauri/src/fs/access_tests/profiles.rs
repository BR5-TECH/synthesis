//! The short user-scoped root, the application instance, and the agent profile.

use super::*;

// ---------------------------------------------------------------------------
// FSA-FR-KVWD — the short user-scoped root (FSA-FR-KVWD)
// ---------------------------------------------------------------------------

#[test]
fn tsxmpr_the_short_root_is_the_home_directory_and_one_segment() {
    let dir = crate::fs::short_data_dir().expect("a short data directory");

    // It is the OS user's home joined with `.synthesis`, and nothing deeper:
    // every segment above what stands here is a segment an agent reads on each
    // command it runs.
    let home = dirs::home_dir().expect("a home directory");
    assert_eq!(dir, home.join(".synthesis"));
    assert!(dir.is_dir(), "the first call creates it");

    // Stable across calls, which is what makes a run findable after a relaunch.
    assert_eq!(crate::fs::short_data_dir().expect("again"), dir);

    // The first call creates it. Proven against a home directory of this
    // test's own, because the developer's own root holds every graduation run
    // this machine has and is not something a test may remove.
    let home = TempDir::new().unwrap();
    let fresh = crate::fs::short_data_dir_in(home.path()).expect("a fresh root");
    assert_eq!(fresh, home.path().join(".synthesis"));
    assert!(fresh.is_dir(), "the first call creates it");
    assert_eq!(
        crate::fs::short_data_dir_in(home.path()).expect("again"),
        fresh,
        "and the second call is content with what the first made",
    );

    // And it is a root a container can be given, which is the whole reason it
    // is not `app_data_dir()`: shorter than that root and holding no space on
    // any platform whose home directory holds none.
    let app_data = crate::fs::app_data_dir().expect("a data directory");
    assert!(
        dir.as_os_str().len() < app_data.as_os_str().len(),
        "the short root must be shorter than {app_data:?}, was {dir:?}"
    );
    // What the application adds to the home directory holds no space, whatever
    // the platform's own data root does: a path with one is a path an agent
    // quotes and repeats rather than stands in.
    assert!(
        !dir.file_name()
            .expect("a last segment")
            .to_string_lossy()
            .contains(' '),
        "{dir:?} adds a segment holding a space",
    );
}

// ---------------------------------------------------------------------------
// FSA-FR-17 — the instance the application runs on (FSA-FR-21)
// ---------------------------------------------------------------------------

#[test]
fn ts30_the_application_instance_is_rebuilt_rather_than_widened() {
    let w1 = TempDir::new().unwrap();
    let w2 = TempDir::new().unwrap();
    let root1 = fs::canonicalize(w1.path()).unwrap();
    let root2 = fs::canonicalize(w2.path()).unwrap();

    let state = FsAccessState::default();
    // Nothing is installed until it is.
    assert!(state.get().is_none());

    state.install_for_worktree(&root1).unwrap();
    let access = state.get().unwrap();

    // Four roots: the worktree, both application-owned roots, and a session
    // temp directory.
    assert!(access.roots().iter().any(|r| *r == root1));
    assert!(access.session_temp_dir().is_some());
    for (label, expected) in [
        ("app_data_dir()", crate::fs::app_data_dir().unwrap()),
        ("short_data_dir()", crate::fs::short_data_dir().unwrap()),
    ] {
        assert!(
            access
                .roots()
                .iter()
                .any(|r| expected.starts_with(r) || *r == expected),
            "{label} must be reachable: roots were {:?}",
            access.roots()
        );
    }
    // FSA-FR-21: the policy is refusal — nothing asked for anything else.
    assert!(!access.follows_symlinks());

    // Writes under the active worktree work.
    access.write_text_atomic(root1.join("a.md"), "one").unwrap();

    // Changing the active worktree REPLACES the instance rather than widening
    // it: the outgoing worktree stops being reachable at that moment.
    state.install_for_worktree(&root2).unwrap();
    let access2 = state.get().unwrap();
    access2.write_text_atomic(root2.join("b.md"), "two").unwrap();
    let err = access2.write_text_atomic(root1.join("c.md"), "no").unwrap_err();
    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "the previous worktree must not stay reachable, got {err:?}"
    );
    assert!(!root1.join("c.md").exists());
}

// ---------------------------------------------------------------------------
// FSA-FR-29 — the agent profile (FSA-FR-29, FSA-FR-20)
// ---------------------------------------------------------------------------

#[test]
fn ts33_an_agent_session_reaches_the_project_and_its_own_scratch_and_nothing_else() {
    let w = TempDir::new().unwrap();
    let root = fs::canonicalize(w.path()).unwrap();
    let state = FsAccessState::default();
    state.install_for_worktree(&root).unwrap();

    let one = state.open_agent_session("session-one").unwrap();
    let two = state.open_agent_session("session-two").unwrap();

    // Exactly two roots: the worktree and a temp directory of its own.
    for (label, access) in [("one", &one), ("two", &two)] {
        let temp = access
            .session_temp_dir()
            .unwrap_or_else(|| panic!("{label} has its own scratch (FSA-FR-29)"))
            .to_path_buf();
        assert!(
            access.roots().iter().any(|r| *r == root),
            "{label} reaches the worktree",
        );
        assert!(!access.follows_symlinks(), "{label} refuses links");

        // FSA-FR-29's whole point: no root either application-owned path could
        // land in. `short_data_dir()` holds every graduation run's checkout, so
        // an agent session that could read it would be reading a run it is no
        // part of.
        for (root_name, owned, probe) in [
            ("app_data_dir()", crate::fs::app_data_dir().unwrap(), "synthesis.toml"),
            ("short_data_dir()", crate::fs::short_data_dir().unwrap(), "g"),
        ] {
            assert!(
                !access
                    .roots()
                    .iter()
                    .any(|r| owned.starts_with(r) || *r == owned),
                "{label} must not reach {root_name}: roots were {:?}",
                access.roots(),
            );
            let err = access.read_text(owned.join(probe)).unwrap_err();
            assert!(
                matches!(err, FsError::EscapesAllowedRoots { .. }),
                "{label}: {root_name} is unreachable, got {err:?}",
            );
        }

        // Both roots are read-write (FSA-FR-29).
        access
            .write_text_atomic(root.join(format!("{label}.md")), label)
            .unwrap();
        access.write_text_atomic(temp.join("scratch"), label).unwrap();
    }

    // Neither session can reach the other's scratch (FSA-FR-20).
    let temp_one = one.session_temp_dir().unwrap().to_path_buf();
    let temp_two = two.session_temp_dir().unwrap().to_path_buf();
    assert_ne!(temp_one, temp_two, "two sessions, two directories");
    let err = one.read_text(temp_two.join("scratch")).unwrap_err();
    assert!(
        matches!(err, FsError::EscapesAllowedRoots { .. }),
        "one session must not observe another's scratch, got {err:?}",
    );

    // A worktree change discards every agent instance with its scratch.
    let w2 = TempDir::new().unwrap();
    let root2 = fs::canonicalize(w2.path()).unwrap();
    state.install_for_worktree(&root2).unwrap();
    assert_eq!(state.agent_session_count(), 0, "discarded on a root change");
    assert!(state.agent_session("session-one").is_none());
    drop(one);
    drop(two);
    assert!(
        !temp_one.exists() && !temp_two.exists(),
        "a discarded session takes its scratch directory with it (FSA-FR-20)",
    );
}

#[test]
fn ts33_no_agent_instance_exists_without_an_open_project() {
    let state = FsAccessState::default();
    // Before anything is installed.
    assert!(matches!(
        state.open_agent_session("s").unwrap_err(),
        FsAccessBuildError::NoRoots,
    ));

    // And after a project is closed again: the pre-project instance reaches
    // app_data_dir(), which is precisely what an agent may not have, so there
    // is no instance to hand out rather than a narrower one.
    let w = TempDir::new().unwrap();
    let root = fs::canonicalize(w.path()).unwrap();
    state.install_for_worktree(&root).unwrap();
    let live = state.open_agent_session("s").unwrap();
    let temp = live.session_temp_dir().unwrap().to_path_buf();
    assert_eq!(state.agent_session_count(), 1);

    state.install_pre_project().unwrap();
    assert_eq!(state.agent_session_count(), 0);
    assert!(matches!(
        state.open_agent_session("s").unwrap_err(),
        FsAccessBuildError::NoRoots,
    ));
    drop(live);
    assert!(!temp.exists(), "closing takes the scratch with it");
}

#[test]
fn ts33_a_session_built_while_the_worktree_changes_is_never_installed() {
    // FSA-FR-29 says no instance outlives the root it was built against.
    // Building one does real filesystem work — canonicalising roots, creating a
    // temp directory — so a build that began before a worktree change can finish
    // after the discard that accompanied it. Without a check at insert time such
    // an instance is installed *behind* the discard and keeps full read-write
    // reach into a checkout the application has stopped showing.
    let a = TempDir::new().unwrap();
    let b = TempDir::new().unwrap();
    let root_a = fs::canonicalize(a.path()).unwrap();
    let root_b = fs::canonicalize(b.path()).unwrap();
    let state = std::sync::Arc::new(FsAccessState::default());
    state.install_for_worktree(&root_a).unwrap();

    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let openers: Vec<_> = (0..4)
        .map(|n| {
            let state = std::sync::Arc::clone(&state);
            let stop = std::sync::Arc::clone(&stop);
            std::thread::spawn(move || {
                let mut opened = 0usize;
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    if state.open_agent_session(&format!("s{n}")).is_ok() {
                        opened += 1;
                    }
                }
                opened
            })
        })
        .collect();

    // Flip the roots under them, checking the invariant after every flip: an
    // instance the state hands out must be rooted at the worktree in force.
    for round in 0..200 {
        let (want, other) = if round % 2 == 0 {
            (&root_b, &root_a)
        } else {
            (&root_a, &root_b)
        };
        state.install_for_worktree(want).unwrap();
        for n in 0..4 {
            if let Some(access) = state.agent_session(&format!("s{n}")) {
                assert!(
                    !access.roots().iter().any(|r| r == other),
                    "round {round}: a session survived a worktree change still \
                     rooted at {other:?} (FSA-FR-29)",
                );
            }
        }
    }

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    let total: usize = openers.into_iter().map(|h| h.join().unwrap()).sum();
    assert!(total > 0, "the openers must have raced, or nothing was tested");
}

#[test]
fn ts33_closing_one_session_leaves_the_others_alone() {
    let w = TempDir::new().unwrap();
    let root = fs::canonicalize(w.path()).unwrap();
    let state = FsAccessState::default();
    state.install_for_worktree(&root).unwrap();

    let keep = state.open_agent_session("keep").unwrap();
    let go = state.open_agent_session("go").unwrap();
    assert_eq!(state.agent_session_count(), 2);

    let keep_temp = keep
        .session_temp_dir()
        .expect("a session has a temp directory")
        .to_path_buf();
    let go_temp = go
        .session_temp_dir()
        .expect("a session has a temp directory")
        .to_path_buf();
    assert_ne!(keep_temp, go_temp, "two sessions, two temp directories");

    state.close_agent_session("go");
    assert_eq!(state.agent_session_count(), 1);
    assert!(state.agent_session("keep").is_some());
    assert!(state.agent_session("go").is_none());

    // FSA-FR-20: closing one session discards *its* temp directory and leaves the
    // one beside it whole — which is what makes "of its own" a lifetime rather
    // than a naming scheme. Map membership alone would not show it.
    //
    // The instance is dropped first: a temp directory lives as long as the last
    // `FsAccess` holding it, so a test still holding one would observe a survival
    // it had itself caused.
    drop(go);
    assert!(!go_temp.exists(), "the closed session's temp directory went with it");
    assert!(keep_temp.exists(), "the session left open kept its own");

    // The IDE instance is untouched by any of it.
    assert!(state.get().is_some());
}

#[test]
fn ts30_the_pre_project_instance_serves_app_data_before_any_project_opens() {
    // FSA-FR-21 / GSS-FR-01 / LGC-FR-20: user-global config and the log buffer
    // both answer with no content root, so the pre-project instance must reach
    // both application-owned roots and a scratch directory, and nothing else.
    let state = FsAccessState::default();
    state.install_pre_project().unwrap();
    let access = state.get().unwrap();

    assert!(access.session_temp_dir().is_some());
    for (label, root) in [
        ("app_data_dir()", crate::fs::app_data_dir().unwrap()),
        ("short_data_dir()", crate::fs::short_data_dir().unwrap()),
    ] {
        // Named uniquely and cleaned up unconditionally: this writes into the
        // developer's REAL directories, so a fixed name would collide between
        // concurrent `cargo test` runs and a cleanup that only ran on success
        // would litter them after a failure.
        let probe = root.join(format!(
            "fsa-pre-project-probe-{}-{:?}.tmp",
            std::process::id(),
            std::thread::current().id()
        ));
        let outcome = (|| {
            access.write_text_atomic(&probe, "reachable")?;
            access.read_text(&probe)
        })();
        let _ = access.delete_path(&probe, false);
        assert_eq!(outcome.unwrap(), "reachable", "{label} must be reachable");
        assert!(!probe.exists(), "the probe cleaned up after itself");
    }

    // No project root is reachable, because there is no project.
    let elsewhere = TempDir::new().unwrap();
    let err = access
        .write_text_atomic(elsewhere.path().join("x.md"), "x")
        .unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
}

// ---------------------------------------------------------------------------
// The remaining instance operations, positive and negative
// ---------------------------------------------------------------------------

#[test]
fn instance_reads_and_writes_carry_the_typed_errors_of_fsa_fr_06() {
    let (_tmp, access, root) = rooted();

    // Missing.
    let err = access.read_text(root.join("nope.md")).unwrap_err();
    assert!(matches!(err, FsError::NotFound { .. }), "got {err:?}");

    // Not valid UTF-8 — text only.
    let bad = root.join("bad.txt");
    fs::write(&bad, [0xFF_u8, 0xFE, 0x00]).unwrap();
    let err = access.read_text(&bad).unwrap_err();
    assert!(matches!(err, FsError::Utf8 { .. }), "got {err:?}");
    // The same bytes read fine as bytes.
    assert_eq!(access.read_bytes(&bad).unwrap(), vec![0xFF, 0xFE, 0x00]);

    // TOML round-trip and the malformed case.
    #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
    struct C {
        name: String,
    }
    let t = root.join("c.toml");
    access
        .write_toml_atomic(&t, &C { name: "x".into() })
        .unwrap();
    assert_eq!(access.read_toml::<C>(&t).unwrap(), C { name: "x".into() });
    fs::write(&t, b"name = = =\n").unwrap();
    let err = access.read_toml::<C>(&t).unwrap_err();
    assert!(matches!(err, FsError::Toml { .. }), "got {err:?}");
}

#[test]
fn instance_append_lines_and_checksum_behave_as_the_free_functions_do() {
    let (_tmp, access, root) = rooted();
    let log = root.join("log.jsonl");

    access
        .append_lines(&log, &["a".to_string(), "b".to_string()])
        .unwrap();
    assert_eq!(fs::read_to_string(&log).unwrap(), "a\nb\n");
    access.append_lines(&log, &["c".to_string()]).unwrap();
    assert_eq!(fs::read_to_string(&log).unwrap(), "a\nb\nc\n");
    // An empty batch writes nothing and creates nothing.
    let before = fs::read_to_string(&log).unwrap();
    access.append_lines(&log, &[]).unwrap();
    assert_eq!(fs::read_to_string(&log).unwrap(), before);
    let fresh = root.join("never.jsonl");
    access.append_lines(&fresh, &[]).unwrap();
    assert!(!fresh.exists(), "an empty batch must not conjure a file");

    // SHA-256("abc") is a known vector.
    let p = root.join("abc.bin");
    fs::write(&p, b"abc").unwrap();
    assert_eq!(
        access.sha256_file(&p).unwrap(),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );

    // Outside the roots, both refuse.
    let outside = TempDir::new().unwrap();
    let err = access
        .append_lines(outside.path().join("l.jsonl"), &["x".to_string()])
        .unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
    let err = access.sha256_file(outside.path().join("f")).unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
}

#[test]
fn instance_ensure_gitignored_creates_and_preserves() {
    let (_tmp, access, root) = rooted();
    let syn = root.join(".synthesis");
    fs::create_dir_all(&syn).unwrap();

    access.ensure_gitignored(&syn).unwrap();
    let content = fs::read_to_string(syn.join(".gitignore")).unwrap();
    // FSA-FR-08: anchored, so each names one entry of `.synthesis/` and nothing
    // deeper — an unanchored `proposals/` would also exclude every draft's own
    // `proposals/` folder (DRS-FR-04, DCP-FR-02).
    for required in ["/cache/", "/local.toml", "/proposals/"] {
        assert!(content.contains(required), "missing {required} in {content:?}");
    }
    // FSA-FR-08 / DRS-FR-04: draft storage is committed, so nothing here ignores
    // it.
    assert!(!content.contains("drafts"), "drafts are not ignored: {content:?}");

    // Existing user entries survive; missing required ones are appended.
    fs::write(syn.join(".gitignore"), "# mine\nnode_modules/\n").unwrap();
    access.ensure_gitignored(&syn).unwrap();
    let content = fs::read_to_string(syn.join(".gitignore")).unwrap();
    assert!(content.contains("node_modules/"));
    for required in ["/cache/", "/local.toml", "/proposals/"] {
        assert!(content.contains(required));
    }

    // Outside the roots it refuses.
    let outside = TempDir::new().unwrap();
    let err = access.ensure_gitignored(outside.path()).unwrap_err();
    assert!(matches!(err, FsError::EscapesAllowedRoots { .. }), "got {err:?}");
}



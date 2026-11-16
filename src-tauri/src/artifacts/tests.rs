use super::*;
use crate::watcher::external_divergence;

// ------------------------------------------------------------------
// Artifact contents + external-change detection
// (PST-FR-15 / PST-FR-16, backing `EXC-editor-external-change.md` EXC-FR-WCOM..VNLZ). The command fns need a
// Tauri runtime (State), but their pure cores (`*_impl`, `external_divergence`)
// and the `ContentTracker` are exercised here without one.
// ------------------------------------------------------------------

#[test]
fn load_artifact_contents_impl_returns_body_and_checksum_and_records_baseline() {
    // PST-FR-16: load returns body + checksum (the Editor's baseline).
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("a.md"), b"# hello\n").unwrap();
    let tracker = ContentTracker::default();
    let got = load_artifact_contents_impl(root, "a.md", &tracker).unwrap();
    assert_eq!(got.body, "# hello\n");
    let expected = root.sha256_file("a.md").unwrap();
    assert_eq!(got.checksum, expected);
    assert_eq!(tracker.current("a.md").as_deref(), Some(expected.as_str()));
}

#[test]
fn save_artifact_contents_impl_writes_atomically_and_returns_checksum() {
    // PST-FR-09, PST-FR-15 (checksum half) / PST-FR-16: save writes the body and
    // returns the checksum of the bytes written; parent dirs are created.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let tracker = ContentTracker::default();
    let res = save_artifact_contents_impl(root, "sub/b.md", "BODY", &tracker).unwrap();
    let on_disk = std::fs::read_to_string(root.join("sub/b.md")).unwrap();
    assert_eq!(on_disk, "BODY");
    let expected = root.sha256_file("sub/b.md").unwrap();
    assert_eq!(res.checksum, expected);
    assert_eq!(tracker.current("sub/b.md").as_deref(), Some(expected.as_str()));
}

#[test]
fn save_is_self_write_then_external_edit_is_a_divergence() {
    // PST-FR-17, PST-FR-31: a save records the new checksum, so the watcher's recompute
    // matches -> suppressed; a later EXTERNAL edit changes the checksum ->
    // divergence emitted.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let tracker = ContentTracker::default();
    save_artifact_contents_impl(root, "a.md", "v1", &tracker).unwrap();

    // Self-write: the on-disk checksum equals the recorded baseline.
    let after_save = root.sha256_file("a.md").ok();
    assert_eq!(
        external_divergence(tracker.current("a.md").as_deref(), after_save.as_deref()),
        None,
        "the Editor's own save must not raise a divergence"
    );

    // External edit: a different checksum -> divergence carrying the new one.
    std::fs::write(root.join("a.md"), b"v2-external").unwrap();
    let after_ext = root.sha256_file("a.md").ok();
    let div = external_divergence(tracker.current("a.md").as_deref(), after_ext.as_deref());
    assert!(div.is_some(), "an external edit must raise a divergence");
    assert_eq!(div.as_deref(), after_ext.as_deref());
}

#[test]
fn content_tracker_clear_drops_all_baselines() {
    // PST-FR-14: closing/switching a project unmounts the baselines.
    let t = ContentTracker::default();
    t.record("a.md", "x");
    t.record("b.md", "y");
    assert_eq!(t.len(), 2);
    t.clear();
    assert_eq!(t.len(), 0);
    assert!(t.current("a.md").is_none());
}

#[test]
fn artifact_contents_wire_shapes_are_camelcase() {
    let c =
        serde_json::to_value(ArtifactContents { body: "b".into(), checksum: "c".into() }).unwrap();
    assert!(c.get("body").is_some() && c.get("checksum").is_some());
    let s = serde_json::to_value(SaveResult { checksum: "c".into() }).unwrap();
    assert!(s.get("checksum").is_some());
    // The event payload speaks camelCase (`artifactId`) — pin it.
    let e = serde_json::to_value(ArtifactChangedPayload {
        artifact_id: "id".into(),
        checksum: "c".into(),
    })
    .unwrap();
    assert_eq!(e.get("artifactId").and_then(|v| v.as_str()), Some("id"));
    assert_eq!(e.get("checksum").and_then(|v| v.as_str()), Some("c"));
}

#[test]
fn artifact_content_command_functions_are_in_scope() {
    // Compile-time guard: renaming/removing either command without updating
    // `generate_handler!` / `COMMAND_NAMES` fails to compile here.
    let _ = load_artifact_contents_by_id;
    let _ = save_artifact_contents::<tauri::Wry>;
}

/// PST-FR-17 / PSS-FR-12: several saves leave the project-local store
/// **byte-for-byte** what it was.
///
/// The store is seeded with a real section first, and the comparison is over
/// its bytes rather than over the absence of the file. Asserting that
/// `local.toml` does not exist would pass in a temp directory where nothing
/// ever created one — and it would have passed against the implementation
/// this change removed, which kept its MRU in memory and never wrote that
/// file either. What PSS-FR-12 claims is that a save records nothing
/// anywhere, and only a store that exists can prove it.
#[test]
fn saves_leave_the_project_local_store_byte_for_byte_what_it_was() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    let local = dir.path().join(".synthesis/local.toml");
    std::fs::write(&local, "[notes_panel]\nscopePosition = \"all\"\n").unwrap();
    let before = std::fs::read(&local).unwrap();

    let tracker = ContentTracker::default();
    for (id, body) in [("a.spec.md", "ONE"), ("b.spec.md", "TWO"), ("a.spec.md", "ONE AGAIN")] {
        save_validated_artifact(root, id, body, &tracker).unwrap();
    }

    assert_eq!(root.read_text(root.join("a.spec.md")).unwrap(), "ONE AGAIN");
    assert_eq!(root.read_text(root.join("b.spec.md")).unwrap(), "TWO");
    assert_eq!(
        std::fs::read(&local).unwrap(),
        before,
        "no save records what it saved, here or in the project-local store"
    );
}

/// PST-FR-17: a save that fails changes no file, and so moves nothing in the
/// Dashboard's Recently edited widget — which follows modification times
/// rather than an account this application keeps of its own writes.
#[test]
fn a_failed_save_writes_nothing_and_leaves_no_baseline() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let tracker = ContentTracker::default();
    let result = save_validated_artifact(root, "../escape.md", "BODY", &tracker);
    assert!(result.is_err(), "a root-escaping id must fail the save");
    assert!(
        tracker.current("../escape.md").is_none(),
        "a refused write leaves no phantom checksum baseline"
    );
}

/// PST-FR-31, FGV-FR-01 / PST-FR-28: a valid Flow is written on the ordinary terms.
#[test]
fn a_valid_flow_is_written() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let tracker = ContentTracker::default();
    let body = r#"{"version":3,"name":"ok","items":[
        {"id":"m1","kind":"milestone","name":"a"}],"edges":[]}"#;
    assert_eq!(
        resolved_type(root, "flows/a.flow"),
        Some(scanning::ArtifactType::Flow),
        "the fixture must actually classify as a Flow or the gate is untested"
    );
    save_validated_artifact(root, "flows/a.flow", body, &tracker).unwrap();
    assert_eq!(root.read_text(root.join("flows/a.flow")).unwrap(), body);
}

/// PST-FR-31, FGV-FR-PMTY / PST-FR-28: an invalid Flow is refused before the write. The
/// file keeps the bytes it had, no baseline is recorded (so no self-write
/// suppression entry is left behind for a write that never happened), and
/// its modification time is untouched — so the artifact's position in
/// Recently edited is exactly what it was (PST-FR-31).
#[test]
fn an_invalid_flow_is_refused_and_nothing_is_written() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("flows")).unwrap();
    std::fs::write(root.join("flows/a.flow"), b"ORIGINAL").unwrap();
    let tracker = ContentTracker::default();
    // Three milestones that block each other in a ring (FGV-FR-PMTY).
    let body = r#"{"version":3,"name":"x","items":[
        {"id":"m1","kind":"milestone","name":"a"},
        {"id":"m2","kind":"milestone","name":"b"},
        {"id":"m3","kind":"milestone","name":"c"}
    ],"edges":[{"id":"e1","from":"m1","to":"m2","type":"blocks"},
               {"id":"e2","from":"m2","to":"m3","type":"blocks"},
               {"id":"e3","from":"m3","to":"m1","type":"blocks"}]}"#;
    let err = save_validated_artifact(root, "flows/a.flow", body, &tracker)
        .expect_err("an invalid Flow must be refused");
    assert!(err.contains("could not be written"), "{err}");
    assert!(err.contains("e1"), "the refusal names the offending edge: {err}");
    assert_eq!(
        std::fs::read(root.join("flows/a.flow")).unwrap(),
        b"ORIGINAL",
        "the file on disk keeps the bytes it had"
    );
    assert!(tracker.current("flows/a.flow").is_none());
}

/// PST-FR-28: no other artifact type is inspected — the same invalid text
/// saved as Markdown is written unexamined.
#[test]
fn a_markdown_artifact_is_not_validated() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let tracker = ContentTracker::default();
    save_validated_artifact(root, "a.spec.md", "{ not json", &tracker).unwrap();
    assert_eq!(root.read_text(root.join("a.spec.md")).unwrap(), "{ not json");
}

// PST-FR-08, PST-FR-ULAY, ESH-FR-SSDV: only a Flow opens in the Flow tab. An
// Architecture opens in the Editor like every other artifact.
#[test]
fn artifact_kind_from_artifact_type_only_flow_is_flow() {
    use scanning::ArtifactType;
    assert_eq!(ArtifactKind::from(ArtifactType::Flow), ArtifactKind::Flow);
    for t in [
        ArtifactType::Skill,
        ArtifactType::Agent,
        ArtifactType::Prompt,
        ArtifactType::Spec,
        ArtifactType::Architecture,
        ArtifactType::Instructions,
        ArtifactType::Scenario,
        ArtifactType::Scratchpad,
        ArtifactType::Doc,
    ] {
        assert_eq!(ArtifactKind::from(t), ArtifactKind::Markdown);
    }
}

#[test]
fn load_artifact_contents_impl_missing_file_is_error_and_records_no_baseline() {
    // Negative path: a non-existent id surfaces a typed error and must not
    // leave a phantom baseline behind.
    let dir = tempfile::TempDir::new().unwrap();
    let tracker = ContentTracker::default();
    let err = load_artifact_contents_impl(&fs::RootFs::for_root(dir.path()), "nope.md", &tracker)
        .unwrap_err();
    assert!(!err.is_empty());
    assert!(tracker.current("nope.md").is_none());
}

// ------------------------------------------------------------------
// PST-FR-31 — line-ending normalisation on write (PST-FR-23)
// ------------------------------------------------------------------

/// Configure `root`'s project-public line-ending convention.
fn set_convention(root: &crate::fs::RootFs, endings: project_settings::LineEndings) {
    project_settings::save_project_config_to(
        root,
        project_settings::ProjectConfig {
            line_endings: endings,
            draft_template: None,
            ..Default::default()
        },
    )
    .unwrap();
}

#[test]
fn a_crlf_project_writes_crlf_and_checksums_the_written_bytes() {
    // PST-FR-23, PST-FR-31: an LF body submitted to a CRLF project is written entirely
    // with CRLF, and the returned checksum is of those bytes — not of the
    // body that was submitted. That distinction is the whole contract: a
    // checksum over the submitted body would leave the Editor's baseline
    // disagreeing with the file, and the watcher's echo of this very write
    // would raise the external-change modal (EDT-FR-40 / PST-FR-16).
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    set_convention(root, project_settings::LineEndings::Crlf);
    let tracker = ContentTracker::default();

    let submitted = "one\ntwo\nthree\n";
    let res = save_artifact_contents_impl(root, "a.md", submitted, &tracker).unwrap();

    let on_disk = std::fs::read_to_string(root.join("a.md")).unwrap();
    assert_eq!(on_disk, "one\r\ntwo\r\nthree\r\n");
    assert_eq!(res.checksum, root.sha256_file("a.md").unwrap());
    assert_ne!(
        res.checksum,
        fs::sha256_bytes(submitted.as_bytes()),
        "the checksum is of the bytes written, not of the body submitted"
    );
    // And the tracker's baseline is the same one, so the watcher suppresses
    // this write rather than reporting it as an external change.
    assert_eq!(tracker.current("a.md").as_deref(), Some(res.checksum.as_str()));
}

#[test]
fn an_lf_project_flattens_a_mixed_body() {
    // PST-FR-23, PST-FR-31, second half: mixed LF and CRLF in, LF throughout out.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    set_convention(root, project_settings::LineEndings::Lf);
    let tracker = ContentTracker::default();

    save_artifact_contents_impl(root, "a.md", "one\r\ntwo\nthree\r\n", &tracker).unwrap();
    let on_disk = std::fs::read_to_string(root.join("a.md")).unwrap();
    assert_eq!(on_disk, "one\ntwo\nthree\n");
    assert!(!on_disk.contains('\r'));
}

#[test]
fn an_unconfigured_project_normalises_to_lf() {
    // PSS-FR-17's default reaching the write path: no `project.toml` at all
    // still normalises, it just normalises to LF.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let tracker = ContentTracker::default();
    save_artifact_contents_impl(root, "a.md", "one\r\ntwo\r\n", &tracker).unwrap();
    assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), "one\ntwo\n");
}

#[test]
fn normalisation_is_idempotent_so_a_resave_changes_no_byte() {
    // A body already in the project's convention must come out
    // byte-identical, or every save would "convert" the file again and the
    // checksum would churn (EDT-FR-40).
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    set_convention(root, project_settings::LineEndings::Crlf);
    let tracker = ContentTracker::default();

    let first = save_artifact_contents_impl(root, "a.md", "one\ntwo\n", &tracker).unwrap();
    let written = std::fs::read_to_string(root.join("a.md")).unwrap();
    let second = save_artifact_contents_impl(root, "a.md", &written, &tracker).unwrap();
    assert_eq!(first.checksum, second.checksum);
    assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), written);
}

#[test]
fn normalisation_leaves_a_body_with_no_line_endings_untouched() {
    // The degenerate case: a single line with no terminator at all is not a
    // line ending to convert, in either convention.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    set_convention(root, project_settings::LineEndings::Crlf);
    let tracker = ContentTracker::default();
    save_artifact_contents_impl(root, "a.md", "just one line", &tracker).unwrap();
    assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), "just one line");
}

#[test]
fn a_bare_cr_is_converted_too() {
    // A stray classic-Mac ending would otherwise survive a "normalisation"
    // and leave the file mixed — the one thing PST-FR-23 rules out.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    set_convention(root, project_settings::LineEndings::Lf);
    let tracker = ContentTracker::default();
    save_artifact_contents_impl(root, "a.md", "one\rtwo\r\nthree\n", &tracker).unwrap();
    assert_eq!(std::fs::read_to_string(root.join("a.md")).unwrap(), "one\ntwo\nthree\n");
}

#[test]
fn the_synthesis_toml_stores_are_unaffected_by_the_convention() {
    // PST-FR-23: only artifact content is normalised. `.synthesis/` metadata
    // goes through the FSA TOML primitive and keeps whatever that writes.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    set_convention(root, project_settings::LineEndings::Crlf);
    let toml = std::fs::read_to_string(root.join(".synthesis/project.toml")).unwrap();
    assert!(
        !toml.contains('\r'),
        "the TOML writer is not routed through the artifact normalisation: {toml:?}"
    );
}

#[test]
fn load_then_save_same_body_yields_a_stable_checksum() {
    // The whole self-write suppression scheme rests on sha256 being
    // content-stable across load/save/watcher-recompute.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let tracker = ContentTracker::default();
    std::fs::write(root.join("a.md"), b"same").unwrap();
    let loaded = load_artifact_contents_impl(root, "a.md", &tracker).unwrap();
    let saved = save_artifact_contents_impl(root, "a.md", "same", &tracker).unwrap();
    assert_eq!(loaded.checksum, saved.checksum);
    assert_eq!(saved.checksum, root.sha256_file("a.md").unwrap());
}

// -----------------------------------------------------------------------
// PST-FR-24 / PST-FR-25 — a plain text file resolves through the same
// commands, and never joins the recently-edited MRU
// -----------------------------------------------------------------------

#[test]
fn pst_ts26_an_unclassified_file_resolves_as_text_and_round_trips() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/main.rs"), b"fn main() {}\n").unwrap();

    // The routing hint says `text`, with the stable key it was asked for.
    let opened = open_artifact_by_id_impl(root, "src/main.rs").unwrap();
    assert_eq!(opened.key, "src/main.rs");
    assert_eq!(opened.kind, ArtifactKind::Text);
    assert_eq!(serde_json::to_value(&opened).unwrap().get("kind").unwrap(), "text");

    // And it loads and saves with the same checksum semantics as an artifact.
    let tracker = ContentTracker::default();
    let loaded = load_artifact_contents_impl(root, "src/main.rs", &tracker).unwrap();
    assert_eq!(loaded.body, "fn main() {}\n");
    let saved =
        save_validated_artifact(root, "src/main.rs", "fn main() { /* edited */ }\n", &tracker)
            .unwrap();
    assert_eq!(
        saved.checksum,
        root.sha256_file("src/main.rs").unwrap(),
        "the checksum is over the bytes that reached disk (PST-FR-15)"
    );
}

#[test]
fn a_classified_artifact_saves_on_the_same_terms() {
    // The other half of PST-FR-23, PST-FR-31: the type a file carries changes how it
    // ROUTES and nothing about how it is written. Both reach disk through
    // the one write path, and neither leaves a record of the save behind
    // (PST-FR-17).
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join(".claude/skills")).unwrap();
    std::fs::write(root.join(".claude/skills/foo.md"), b"body\n").unwrap();

    let tracker = ContentTracker::default();
    save_validated_artifact(root, ".claude/skills/foo.md", "edited\n", &tracker).unwrap();
    assert_eq!(root.read_text(root.join(".claude/skills/foo.md")).unwrap(), "edited\n");

    assert_eq!(
        open_artifact_by_id_impl(root, ".claude/skills/foo.md").unwrap().kind,
        ArtifactKind::Markdown
    );
}

#[test]
fn a_flow_resolves_to_the_flow_routing_hint() {
    // PST-FR-08: the hint is what sends a Flow to the Flow tab rather than
    // the Editor, and adding `text` must not disturb it.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    // ASC-FR-03: the `.flow` path convention, with no frontmatter to
    // fall back on — the only route by which a Flow document classifies.
    std::fs::write(root.join("review.flow"), b"{\"version\":1}").unwrap();
    assert_eq!(open_artifact_by_id_impl(root, "review.flow").unwrap().kind, ArtifactKind::Flow);
}

// The ordinary product path: New Artifact records a per-file assignment for
// whatever type was chosen (PST-FR-21), so a Flow the author named without
// the extension reaches the Flow canvas through the assignment alone.
#[test]
fn a_flow_named_without_the_extension_still_routes_to_the_flow_canvas() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join(".synthesis")).unwrap();
    // What New Artifact leaves behind for `Type: Flow, name: pipeline`: the
    // file, plus the per-file assignment `create_artifact` records (PST-FR-21).
    std::fs::write(root.join("pipeline"), b"{}").unwrap();
    scanning::assign(root, "pipeline", scanning::ArtifactType::Flow, scanning::Scope::File)
        .unwrap();

    assert_eq!(open_artifact_by_id_impl(root, "pipeline").unwrap().kind, ArtifactKind::Flow);

    // …and the converse: a `.flow` file assigned another type is not a Flow.
    std::fs::write(root.join("review.flow"), b"{}").unwrap();
    scanning::assign(root, "review.flow", scanning::ArtifactType::Spec, scanning::Scope::File)
        .unwrap();
    assert_eq!(open_artifact_by_id_impl(root, "review.flow").unwrap().kind, ArtifactKind::Markdown);
}

// PST-FR-QQAE, ASC-FR-02: the answer carries the resolved artifact type, in
// the lowercase name the tree nodes use, and omits it for a file with no
// type. Each of the ten types answers its name.
#[test]
fn open_artifact_by_id_answers_the_resolved_artifact_type() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join(".synthesis")).unwrap();
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("docs/guide.md"), b"# Guide\n").unwrap();
    std::fs::write(root.join("notes.txt"), b"plain\n").unwrap();
    scanning::assign(root, "docs", scanning::ArtifactType::Doc, scanning::Scope::Folder).unwrap();

    let doc = open_artifact_by_id_impl(root, "docs/guide.md").unwrap();
    assert_eq!(doc.artifact_type, Some(scanning::ArtifactType::Doc));
    assert_eq!(doc.kind, ArtifactKind::Markdown);
    let json = serde_json::to_value(&doc).unwrap();
    assert_eq!(json.get("artifactType").unwrap(), "doc");

    let plain = open_artifact_by_id_impl(root, "notes.txt").unwrap();
    assert_eq!(plain.artifact_type, None);
    assert_eq!(plain.kind, ArtifactKind::Text);
    let json = serde_json::to_value(&plain).unwrap();
    assert!(json.get("artifactType").is_none(), "no type gives no field");

    // Each of the ten types answers its lowercase name.
    use scanning::ArtifactType as T;
    let names = [
        (T::Skill, "skill"),
        (T::Agent, "agent"),
        (T::Prompt, "prompt"),
        (T::Spec, "spec"),
        (T::Flow, "flow"),
        (T::Architecture, "architecture"),
        (T::Instructions, "instructions"),
        (T::Scenario, "scenario"),
        (T::Scratchpad, "scratchpad"),
        (T::Doc, "doc"),
    ];
    for (n, (artifact_type, name)) in names.into_iter().enumerate() {
        let file = format!("typed-{n}.md");
        std::fs::write(root.join(&file), b"body\n").unwrap();
        scanning::assign(root, &file, artifact_type, scanning::Scope::File).unwrap();
        let opened = open_artifact_by_id_impl(root, &file).unwrap();
        let json = serde_json::to_value(&opened).unwrap();
        assert_eq!(json.get("artifactType").unwrap(), name);
    }
}

// PST-FR-08, PST-FR-ULAY, PST-FR-QQAE, ASC-FR-QVCN: a `*.calm.json` file is inferred as
// an Architecture and opens in the Editor, not in the Flow tab.
#[test]
fn a_calm_json_file_opens_as_an_architecture_in_the_editor() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("system.calm.json"), b"{}\n").unwrap();
    let opened = open_artifact_by_id_impl(root, "system.calm.json").unwrap();
    assert_eq!(opened.artifact_type, Some(scanning::ArtifactType::Architecture));
    assert_eq!(opened.kind, ArtifactKind::Markdown);
    let json = serde_json::to_value(&opened).unwrap();
    assert_eq!(json.get("artifactType").unwrap(), "architecture");
    assert_eq!(json.get("kind").unwrap(), "markdown");
}

#[test]
fn open_artifact_by_id_rejects_a_missing_path_and_one_that_escapes_the_root() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    assert!(open_artifact_by_id_impl(root, "nope.md").is_err());
    // FSA-FR-10: a crafted `..` path is rejected rather than resolved.
    assert!(open_artifact_by_id_impl(root, "../../etc/passwd").is_err());
    // A folder is not openable content.
    std::fs::create_dir_all(root.join("dir")).unwrap();
    assert!(open_artifact_by_id_impl(root, "dir").is_err());
}

#[test]
fn artifact_kind_covers_every_routing_destination() {
    for (kind, wire) in [
        (ArtifactKind::Markdown, "markdown"),
        (ArtifactKind::Flow, "flow"),
        (ArtifactKind::Text, "text"),
    ] {
        assert_eq!(serde_json::to_value(kind).unwrap(), wire);
    }
    assert_eq!(ArtifactKind::from_resolved(None), ArtifactKind::Text);
    assert_eq!(
        ArtifactKind::from_resolved(Some(scanning::ArtifactType::Spec)),
        ArtifactKind::Markdown
    );
}

#[test]
fn open_artifact_by_id_command_is_in_scope() {
    // Compile-time guard: renaming/removing it without updating
    // `generate_handler!` / `COMMAND_NAMES` fails to compile here.
    let _ = open_artifact_by_id;
}

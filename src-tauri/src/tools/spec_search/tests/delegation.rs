//! SPS-FR-03, SPS-FR-04: the delegated call, and what counts as a specification.

use super::*;

// ---------------------------------------------------------------------------
// SPS-FR-03 — everything comes from the delegated call (SPS-FR-03)
// ---------------------------------------------------------------------------

#[test]
fn every_result_comes_from_the_index_and_nothing_is_added() {
    let fixture = spec_project();
    let indexer = fixture.app.state::<Bm25Indexer>();

    let output = query(&fixture, "teardown", Some(5));
    // Every match must be traceable to a hit the delegated call produced, with
    // its own text and its own score carried through unaltered.
    let hits = crate::bm25_index::search(
        &indexer,
        &[crate::bm25_index::IndexId::Spec],
        "teardown",
        5 * OVERFETCH_FACTOR,
    );
    for matched in &output.specifications {
        assert!(
            hits.iter().any(|hit| hit.path == matched.path
                && hit.text == matched.excerpt
                && hit.score == matched.score),
            "{matched:?} is not one of the delegated call's hits (SPS-FR-03)",
        );
    }
    assert!(!output.specifications.is_empty(), "the fixture matches");
}

#[test]
fn the_module_reads_no_file_and_enumerates_no_folder() {
    // SPS-FR-03 in the source itself: this tool consults the index and nothing
    // else, which a behavioural test cannot distinguish from a tool that also
    // happened to read the right file.
    const SOURCE: &str = include_str!("../../spec_search.rs");
    // The suite lives in its own file, so SOURCE is the module and nothing else.
    let body = SOURCE;
    for forbidden in [
        "read_text",
        "read_bytes",
        "read_dir",
        "list_dir",
        "File::open",
        "WalkDir",
    ] {
        assert!(
            !body.contains(forbidden),
            "`{forbidden}` means this tool reached past the index (SPS-FR-03)",
        );
    }
    // SPS-FR-08: one delegated call per invocation, never escalated, repeated,
    // or widened. A behavioural test cannot see a second call that happens to
    // return the same thing, so the single call site is what is pinned.
    assert_eq!(
        body.matches("bm25_index::search(").count(),
        1,
        "exactly one call site for the delegated search (SPS-FR-08)",
    );
    // Comments first: this module's prose discusses the escalation it must not
    // perform, and a naive scan would flag that and fail for the opposite of the
    // reason the check exists.
    let code: String = body
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in ["while ", "loop {", "for _ in 0.."] {
        assert!(
            !code.contains(forbidden),
            "`{forbidden}` suggests an escalation this tool must not perform \
             (SPS-FR-08)",
        );
    }
}

// ---------------------------------------------------------------------------
// SPS-FR-04 — eligibility is ASC's classification alone (SPS-FR-04)
// ---------------------------------------------------------------------------

#[test]
fn what_counts_as_a_specification_is_the_scans_classification() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    // Inferred from its path.
    write(root, "specifications/core/a.md", &spec_body("teardown", 1));
    // Assigned `spec` although it sits nowhere near `specifications/`.
    write(root, "notes/b.md", &spec_body("teardown", 1));
    // Assigned a *different* type although it sits under `specifications/`.
    write(root, "specifications/d.md", &spec_body("teardown", 1));
    // Classified `skill`, so it lands in another index rather than in none —
    // which is what makes its absence below discriminating. A file indexed
    // nowhere is absent from this tool's results whatever the tool does.
    write(root, ".claude/skills/digger/SKILL.md", &spec_body("teardown", 1));
    // Inherited from a folder-scope assignment.
    write(root, "docs/c.md", &spec_body("teardown", 1));
    write(
        root,
        ".synthesis/library.toml",
        r#"[assignments."notes/b.md"]
type = "spec"
scope = "file"

[assignments."docs"]
type = "spec"
scope = "folder"

[assignments."specifications/d.md"]
type = "prompt"
scope = "file"
"#,
    );
    let fixture = mounted(dir);

    let output = query(&fixture, "teardown", Some(20));
    let paths: Vec<&str> = output
        .specifications
        .iter()
        .map(|m| m.path.as_str())
        .collect();

    assert!(
        paths.contains(&"specifications/core/a.md"),
        "a path-inferred spec is found (SPS-FR-04): {paths:?}",
    );
    assert!(
        paths.contains(&"notes/b.md"),
        "a file assigned `spec` outside `specifications/` is found on the same \
         terms (SPS-FR-04): {paths:?}",
    );
    assert!(
        paths.contains(&"docs/c.md"),
        "a file inheriting `spec` from its folder is found (SPS-FR-04): {paths:?}",
    );
    assert!(
        !paths.iter().any(|p| p.ends_with("d.md")),
        "a file under `specifications/` carrying another type is not found \
         (SPS-FR-04): {paths:?}",
    );
    assert!(
        !paths.iter().any(|p| p.contains("SKILL.md")),
        "a file classified into another index is unreachable here (SPS-FR-04, \
         SPS-FR-05): {paths:?}",
    );

    // The guard that keeps the two negatives honest: both files must actually
    // be indexed somewhere, or their absence above proves nothing at all.
    let indexer = fixture.app.state::<Bm25Indexer>();
    let elsewhere = crate::bm25_index::search(&indexer, &[], "teardown", 50);
    for expected in ["specifications/d.md", ".claude/skills/digger/SKILL.md"] {
        assert!(
            elsewhere.iter().any(|hit| hit.path == expected),
            "{expected} must be indexed under some other type, or its absence \
             from this tool's results is not evidence of anything",
        );
    }
}


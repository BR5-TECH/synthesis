//! SPS-FR-06 … SPS-FR-09: the limit, one match per specification, and ordering.

use super::*;

// ---------------------------------------------------------------------------
// SPS-FR-06 — the limit's default and clamp (SPS-FR-06)
// ---------------------------------------------------------------------------

#[test]
fn the_limit_defaults_to_five_and_clamps_into_one_through_twenty() {
    assert_eq!(normalize_limit(None), 5, "the documented default");
    assert_eq!(normalize_limit(Some(0)), 1);
    assert_eq!(normalize_limit(Some(-5)), 1);
    assert_eq!(normalize_limit(Some(1000)), 20);
    assert_eq!(normalize_limit(Some(7)), 7);
}

#[test]
fn thirty_matching_specifications_are_bounded_by_the_limit() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    for i in 0..30 {
        write(
            root,
            &format!("specifications/s{i:02}.md"),
            &spec_body("teardown", 1),
        );
    }
    let fixture = mounted(dir);

    assert_eq!(query(&fixture, "teardown", None).specifications.len(), 5);
    assert_eq!(query(&fixture, "teardown", Some(0)).specifications.len(), 1);
    assert_eq!(query(&fixture, "teardown", Some(-5)).specifications.len(), 1);
    assert_eq!(
        query(&fixture, "teardown", Some(1000)).specifications.len(),
        20,
    );
}

// ---------------------------------------------------------------------------
// SPS-FR-07 — one match per specification, its best passage (SPS-FR-07)
// ---------------------------------------------------------------------------

#[test]
fn a_specification_appears_once_carrying_its_best_matching_section() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    // Four sections, every one of them about the query.
    write(root, "specifications/many.md", &spec_body("teardown", 4));
    write(root, "specifications/other-a.md", &spec_body("teardown", 1));
    write(root, "specifications/other-b.md", &spec_body("teardown", 1));
    let fixture = mounted(dir);
    let indexer = fixture.app.state::<Bm25Indexer>();

    let output = query(&fixture, "teardown", Some(3));

    let appearances = output
        .specifications
        .iter()
        .filter(|m| m.path == "specifications/many.md")
        .count();
    assert_eq!(appearances, 1, "one entry per file (SPS-FR-07)");

    // The passage that stands for the file is its highest-scoring one, not
    // merely some passage of it.
    let hits = crate::bm25_index::search(
        &indexer,
        &[crate::bm25_index::IndexId::Spec],
        "teardown",
        3 * OVERFETCH_FACTOR,
    );
    let best = hits
        .iter()
        .filter(|h| h.path == "specifications/many.md")
        .map(|h| h.score)
        .fold(f32::MIN, f32::max);
    let carried = output
        .specifications
        .iter()
        .find(|m| m.path == "specifications/many.md")
        .expect("present");
    assert_eq!(carried.score, best, "the best section stands for the file");

    assert!(
        hits.iter()
            .filter(|h| h.path == "specifications/many.md")
            .count()
            > 1,
        "the fixture must actually present several passages of one file, \
         or this test proves nothing",
    );
}

// ---------------------------------------------------------------------------
// SPS-FR-08 — the limit counts files, filled by one over-fetch (SPS-FR-08)
// ---------------------------------------------------------------------------

#[test]
fn the_limit_is_filled_with_distinct_specifications() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    // One file whose every section is short and dense in the query's term, so
    // its sections take the top of the ranking together. This is the ordinary
    // shape of a well-matching spec, and it is exactly what a passage-counted
    // limit collapses to one or two results.
    let mut dominant = String::from("# teardown\n");
    for i in 0..8 {
        dominant.push_str(&format!("\n## S{i}\n\nteardown teardown teardown\n"));
    }
    write(root, "specifications/dominant.md", &dominant);
    // Nine more that match, each diluted by prose the query does not reach.
    for i in 0..9 {
        write(
            root,
            &format!("specifications/s{i}.md"),
            &format!(
                "# other {i}\n\n## Body\n\nA long passage of unrelated prose that happens to \
                 mention teardown once, surrounded by many other words that dilute it \
                 considerably for the purposes of ranking this document.\n"
            ),
        );
    }
    let fixture = mounted(dir);

    let output = query(&fixture, "teardown", Some(5));
    assert_eq!(
        output.specifications.len(),
        5,
        "five files, not five passages (SPS-FR-08)",
    );
    let distinct: std::collections::HashSet<&String> =
        output.specifications.iter().map(|m| &m.path).collect();
    assert_eq!(distinct.len(), 5, "all five are different files");

    // The mutation this guards: a limit passed straight down without the
    // over-fetch returns fewer than five here.
    let unfilled = crate::bm25_index::search(
        &fixture.app.state::<Bm25Indexer>(),
        &[crate::bm25_index::IndexId::Spec],
        "teardown",
        5,
    );
    let unfilled_distinct: std::collections::HashSet<&String> =
        unfilled.iter().map(|h| &h.path).collect();
    assert!(
        unfilled_distinct.len() < 5,
        "the fixture must be one where a passage-counted limit falls short, \
         or the over-fetch is untested (got {})",
        unfilled_distinct.len(),
    );
}

#[test]
fn a_project_with_fewer_specifications_than_the_limit_returns_what_it_has() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    write(root, "specifications/a.md", &spec_body("teardown", 3));
    write(root, "specifications/b.md", &spec_body("teardown", 3));
    let fixture = mounted(dir);

    let output = query(&fixture, "teardown", Some(5));
    assert_eq!(output.specifications.len(), 2, "two exist, two come back");
}

// ---------------------------------------------------------------------------
// SPS-FR-09 — ordering (SPS-FR-09)
// ---------------------------------------------------------------------------

#[test]
fn matches_are_ordered_by_descending_score() {
    let fixture = spec_project();
    let output = query(&fixture, "teardown", Some(20));

    assert!(output.specifications.len() > 1, "several match");
    for pair in output.specifications.windows(2) {
        assert!(
            pair[0].score >= pair[1].score,
            "descending score (SPS-FR-09): {:?}",
            output.specifications,
        );
    }
    assert_eq!(
        output.specifications[0].path, "specifications/core/teardown.md",
        "the most apt is first",
    );
}


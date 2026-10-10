//! Tests for `SST-skill-search-tool.md`.
//!
//! Ranking is the whole point of this tool, so these run against a real BM25
//! index built by a real pass rather than a stub — the claims worth pinning
//! (a body term ranking nothing, an apt description ranking first) are exactly
//! the ones a stub would answer by construction.

use rig::tool::{PortableTool, ToolErrorKind};
use tauri::Manager;

use super::*;
use crate::skills::Ecosystem;
use crate::tools::tests::{
    closed_project, collision_project, demo_project, empty_project, mounted, ranking_project,
    tied_project, write_skill,
};
use crate::tools::tests::block_on;
use crate::tools::ToolRefusal;
use crate::logging::{LogBuffer, LogFilter, LogLevel};

fn tool(
    fixture: &crate::tools::tests::Fixture,
) -> SkillSearchTool<tauri::test::MockRuntime> {
    SkillSearchTool::new(fixture.handle())
}

fn call(
    fixture: &crate::tools::tests::Fixture,
    query: &str,
    limit: Option<i64>,
) -> Result<SkillSearchOutput, ToolRefusal> {
    block_on(tool(fixture).call(SkillSearchArgs {
        query: query.to_string(),
        limit,
    }))
}

fn names(output: &SkillSearchOutput) -> Vec<&str> {
    output.skills.iter().map(|s| s.name.as_str()).collect()
}

fn paths(output: &SkillSearchOutput) -> Vec<&str> {
    output.skills.iter().map(|s| s.path.as_str()).collect()
}

// ---------------------------------------------------------------------------
// SST-FR-01 / SST-FR-02 — the tool and its definition
// ---------------------------------------------------------------------------

#[test]
fn the_tool_is_a_portable_tool_named_search_skills() {
    // SST-FR-01.
    let app = closed_project();
    let definition =
        rig::tool::tool_definition(&SkillSearchTool::new(app.handle().clone()));

    assert_eq!(definition.name, "search_skills");
    assert_eq!(
        <SkillSearchTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        "search_skills",
    );
    assert_eq!(NAME, "search_skills");
}

#[test]
fn the_definition_is_the_fixed_text_and_the_documented_schema() {
    // SST-FR-02.
    let app = closed_project();
    let definition =
        rig::tool::tool_definition(&SkillSearchTool::new(app.handle().clone()));

    assert_eq!(definition.description, DESCRIPTION);

    // Asserting `description() == DESCRIPTION` alone proves only that the
    // method returns its own constant. TLC-FR-05 makes this text contract, so
    // it is checked against the spec that defines it.
    const SPEC: &str = include_str!("../../../../specifications/tools/SST-skill-search-tool.md");
    assert!(
        spec_quotes(SPEC).contains(&DESCRIPTION.to_string()),
        "SST-FR-02: the description must be the spec's contract-surface text",
    );

    let schema = definition.parameters;
    assert_eq!(schema, parameters());
    assert_eq!(
        schema.pointer("/required").unwrap(),
        &serde_json::json!(["query"]),
        "SST-FR-02: query is required and limit is not",
    );
    for pointer in [
        "/properties/query/description",
        "/properties/limit/description",
    ] {
        let text = schema.pointer(pointer).and_then(|d| d.as_str()).unwrap();
        assert!(
            SPEC.contains(text),
            "SST-FR-02: {pointer} must be the spec's text, got {text:?}",
        );
    }
}

/// Every blockquote and italicised run in a spec, unwrapped to its plain text.
///
/// The contract-surface descriptions live in the specs as `> …` blockquotes and
/// `- \`name\` — *"…"*` bullets; this is what lets a test compare a compiled-in
/// constant against the document that defines it.
fn spec_quotes(spec: &str) -> Vec<String> {
    spec.lines()
        .filter_map(|line| line.trim().strip_prefix("> ").map(str::to_string))
        .collect()
}

// ---------------------------------------------------------------------------
// SST-FR-03 — delegation (SST-FR-03)
// ---------------------------------------------------------------------------

#[test]
fn the_tool_returns_exactly_what_the_skills_module_produced() {
    let fixture = demo_project();
    let indexer = fixture.app.state::<crate::bm25_index::Bm25Indexer>();

    let direct = crate::skills::search_skills(&indexer, "review a specification", 10);
    let through_tool = call(&fixture, "review a specification", None).unwrap();

    assert_eq!(through_tool.skills.len(), direct.len());
    for (returned, expected) in through_tool.skills.iter().zip(direct.iter()) {
        assert_eq!(returned.name, expected.skill.name);
        assert_eq!(returned.description, expected.skill.description);
        assert_eq!(returned.path, expected.skill.path);
        assert_eq!(returned.ecosystem, expected.skill.ecosystem);
        assert_eq!(returned.score, expected.score);
    }
}

#[test]
fn the_tool_reads_no_file_and_enumerates_no_folder_of_its_own() {
    // SST-FR-03: it consults no index directly and holds no registry. The
    // module's own source is the evidence — a tool that never names a read
    // cannot perform one.
    const SOURCE: &str = include_str!("../skill_search.rs");
    let code: String = SOURCE
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in [
        "std::fs",
        "read_dir",
        "read_text",
        "search_snapshot",
        "snapshot()",
        "IndexId",
    ] {
        assert!(
            !code.contains(forbidden),
            "SST-FR-03: this tool delegates and must not reach for {forbidden:?}",
        );
    }
}

// ---------------------------------------------------------------------------
// SST-FR-04 — ranked on the description, never the body (SST-FR-04)
// ---------------------------------------------------------------------------

#[test]
fn a_term_that_appears_only_in_a_body_ranks_nothing() {
    // The `analyst` fixture's body carries `sarcophagus`; its description does
    // not. DSL-FR-13 makes the indexed document a name and a description alone.
    let fixture = demo_project();

    let body_term = call(&fixture, "sarcophagus", None).unwrap();
    assert!(
        body_term.skills.is_empty(),
        "SST-FR-04: a skill is ranked on what it says about itself, not its body",
    );

    let described = call(&fixture, "review a specification", None).unwrap();
    assert_eq!(
        described.skills.first().map(|s| s.name.as_str()),
        Some("analyst"),
        "SST-FR-04: the apt description ranks first",
    );
}

// ---------------------------------------------------------------------------
// SST-FR-05 — the limit (SST-FR-05)
// ---------------------------------------------------------------------------

#[test]
fn the_limit_defaults_to_ten_and_clamps_into_its_documented_range() {
    // Thirty skills that all match one term, so the limit is the only thing
    // bounding the result.
    let dir = tempfile::TempDir::new().unwrap();
    for index in 0..30 {
        write_skill(
            dir.path(),
            ".claude/skills",
            &format!("skill{index:02}"),
            &format!("name: skill{index:02}\ndescription: Handles widget calibration number {index}."),
            "# Body",
        );
    }
    let fixture = mounted(dir);

    assert_eq!(
        call(&fixture, "widget calibration", None).unwrap().skills.len(),
        10,
        "SST-FR-05: the documented default",
    );
    assert_eq!(
        call(&fixture, "widget calibration", Some(0)).unwrap().skills.len(),
        1,
        "SST-FR-05: zero clamps up to one, so an empty result always means nothing matched",
    );
    assert_eq!(
        call(&fixture, "widget calibration", Some(-5)).unwrap().skills.len(),
        1,
    );
    assert_eq!(
        call(&fixture, "widget calibration", Some(1000)).unwrap().skills.len(),
        25,
        "SST-FR-05: the documented ceiling",
    );
    assert_eq!(
        call(&fixture, "widget calibration", Some(3)).unwrap().skills.len(),
        3,
    );

    // None of the five refused.
    assert_eq!(normalize_limit(Some(0)), MIN_LIMIT as usize);
    assert_eq!(normalize_limit(Some(i64::MIN)), MIN_LIMIT as usize);
    assert_eq!(normalize_limit(Some(i64::MAX)), MAX_LIMIT as usize);
}

// ---------------------------------------------------------------------------
// SST-FR-06 — an empty query refuses (SST-FR-06)
// ---------------------------------------------------------------------------

#[test]
fn an_empty_or_blank_query_refuses_rather_than_returning_an_empty_list() {
    let fixture = demo_project();

    for query in ["", "   ", "\t\n "] {
        let refusal = call(&fixture, query, None)
            .expect_err("SST-FR-06: a blank query is a mistake the model can correct");
        assert_eq!(refusal, ToolRefusal::InvalidArguments(EMPTY_QUERY));

        let error = refusal.to_execution_error();
        assert_eq!(error.kind(), ToolErrorKind::InvalidArgs);
        assert_eq!(error.retryable(), Some(true));
        assert_eq!(error.message(), EMPTY_QUERY);
    }
}

// ---------------------------------------------------------------------------
// SST-FR-07 — no project open (SST-FR-07)
// ---------------------------------------------------------------------------

#[test]
fn no_project_open_refuses_rather_than_returning_an_empty_list() {
    // SST-FR-07: a model told "no skill matched" would conclude the project has
    // none and stop looking. The distinction is the whole point of the refusal,
    // so this also pins what the underlying module answers instead.
    let app = closed_project();
    let handle = app.handle().clone();

    let refusal = block_on(SkillSearchTool::new(handle).call(SkillSearchArgs {
        query: "review a specification".to_string(),
        limit: None,
    }))
    .expect_err("SST-FR-07");
    assert_eq!(refusal, ToolRefusal::NoProjectOpen);

    let error = refusal.to_execution_error();
    assert_eq!(error.kind(), ToolErrorKind::NotFound);
    assert_eq!(error.retryable(), Some(false));

    // DSL-FR-15 would have answered an empty list here; the tool does not.
    let indexer = app.state::<crate::bm25_index::Bm25Indexer>();
    assert!(crate::skills::search_skills(&indexer, "review a specification", 10).is_empty());
}

// ---------------------------------------------------------------------------
// SST-FR-08 — an empty answer is a success (SST-FR-08)
// ---------------------------------------------------------------------------

#[test]
fn a_query_matching_nothing_and_a_project_with_no_skills_both_succeed() {
    let fixture = demo_project();
    let unmatched = call(&fixture, "zzzz nonexistent terminology", None)
        .expect("SST-FR-08: nothing matched is an answer");
    assert!(unmatched.skills.is_empty());

    let empty = empty_project();
    let none = call(&empty, "review a specification", None)
        .expect("SST-FR-08: a project with no eligible skill is an answer");
    assert!(none.skills.is_empty());
}

// ---------------------------------------------------------------------------
// SST-FR-09 / SST-FR-10 — the match shape (SST-FR-09, SST-FR-10)
// ---------------------------------------------------------------------------

#[test]
fn a_match_carries_exactly_the_documented_fields_and_no_body() {
    let fixture = demo_project();
    let output = call(&fixture, "review a specification", None).unwrap();
    let first = output.skills.first().expect("a match");

    let value = serde_json::to_value(first).unwrap();
    let mut keys: Vec<&str> = value.as_object().unwrap().keys().map(|k| k.as_str()).collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["description", "ecosystem", "name", "path", "score"],
        "SST-FR-09: exactly the documented fields, and no folder_name",
    );

    assert_eq!(first.name, "analyst");
    assert_eq!(first.ecosystem, Ecosystem::Claude);
    assert!(
        !first.description.contains("sarcophagus"),
        "SST-FR-09: no part of the skill's body appears in the output",
    );

    // SST-FR-10: `path` is the project-relative SKILL.md the description tells
    // the model to read, and it resolves.
    assert_eq!(first.path, ".claude/skills/analyst/SKILL.md");
    let indexer = fixture.app.state::<crate::bm25_index::Bm25Indexer>();
    let root = indexer.root().expect("mounted");
    let resolved = root.join(&first.path);
    assert!(resolved.is_file(), "the path resolves to a file on disk");
    assert!(std::fs::read_to_string(&resolved).unwrap().contains("sarcophagus"));
}

// ---------------------------------------------------------------------------
// SST-FR-11 — ordering (SST-FR-11)
// ---------------------------------------------------------------------------

#[test]
fn matches_are_ordered_by_descending_score_and_the_first_is_the_most_apt() {
    // Run against `ranking_project`, whose descriptions deliberately overlap.
    // `demo_project`'s are disjoint, so every query against it returns exactly
    // one match — and a one-element vector is in descending order under every
    // ordering there is, including a reversed one.
    let fixture = ranking_project();
    let output = call(&fixture, "review a specification for contradictions", None).unwrap();

    assert!(
        output.skills.len() >= 3,
        "the fixture must produce a set worth ordering, got {:?}",
        names(&output),
    );
    assert_eq!(
        output.skills.first().map(|s| s.name.as_str()),
        Some("spec-reviewer"),
        "SST-FR-11: the most apt match is first",
    );

    let scores: Vec<f32> = output.skills.iter().map(|s| s.score).collect();
    let mut sorted = scores.clone();
    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
    assert_eq!(scores, sorted, "SST-FR-11: descending score order");
    assert!(
        scores.first() > scores.last(),
        "SST-FR-11: the scores genuinely differ, so the order is load-bearing: {scores:?}",
    );
}

#[test]
fn skills_that_tie_on_score_come_back_in_a_stable_order() {
    // SST-FR-11 orders one result set; two skills whose descriptions are
    // identical score identically, and the order they arrive in must still be
    // the same on every call rather than whatever the map iterated.
    let fixture = tied_project();
    let first = call(&fixture, "review a pull request diff", None).unwrap();

    assert_eq!(first.skills.len(), 2);
    assert_eq!(
        first.skills[0].score, first.skills[1].score,
        "the fixture ties, which is what makes the order worth pinning",
    );
    // Deliberately NOT sorted before asserting: sorting here would discard the
    // very property under test.
    assert_eq!(
        paths(&first),
        vec![
            ".claude/skills/alpha/SKILL.md",
            ".claude/skills/zulu/SKILL.md",
        ],
    );
    assert_eq!(paths(&call(&fixture, "review a pull request diff", None).unwrap()), paths(&first));
}

#[test]
fn map_error_is_what_carries_a_refusal_to_the_model() {
    // TLC-FR-10 is about `map_error` specifically. Asserting on
    // `ToolRefusal::to_execution_error` alone would leave the trait method
    // free to return anything at all.
    let app = closed_project();
    let tool = SkillSearchTool::new(app.handle().clone());

    let mapped = tool.map_error(ToolRefusal::NoProjectOpen);
    assert_eq!(mapped.kind(), ToolErrorKind::NotFound);
    assert_eq!(mapped.retryable(), Some(false));
    assert_eq!(mapped.message(), crate::tools::NO_PROJECT_OPEN);

    let mapped = tool.map_error(ToolRefusal::InvalidArguments(EMPTY_QUERY));
    assert_eq!(mapped.kind(), ToolErrorKind::InvalidArgs);
    assert_eq!(mapped.retryable(), Some(true));
    assert_eq!(mapped.message(), EMPTY_QUERY);
}

#[test]
fn a_limit_in_any_spelling_a_model_might_send_is_accepted() {
    // TLC-FR-07: a model asked for an integer routinely sends a string or a
    // float. Losing the whole call — including a perfectly good query — over
    // the shape of an optional parameter is the opposite of forgiveness.
    for (json, expected) in [
        (serde_json::json!({ "query": "q", "limit": 5 }), 5),
        (serde_json::json!({ "query": "q", "limit": "5" }), 5),
        (serde_json::json!({ "query": "q", "limit": " 5 " }), 5),
        (serde_json::json!({ "query": "q", "limit": 5.0 }), 5),
        (serde_json::json!({ "query": "q", "limit": 5.9 }), 5),
        (serde_json::json!({ "query": "q", "limit": "5.9" }), 5),
        // Not a number in any spelling, and explicit null: the default.
        (serde_json::json!({ "query": "q", "limit": null }), 10),
        (serde_json::json!({ "query": "q", "limit": "many" }), 10),
        (serde_json::json!({ "query": "q", "limit": true }), 10),
        (serde_json::json!({ "query": "q" }), 10),
        // Still clamped after decoding.
        (serde_json::json!({ "query": "q", "limit": "1000" }), 25),
        (serde_json::json!({ "query": "q", "limit": "-4" }), 1),
    ] {
        let args: SkillSearchArgs = serde_json::from_value(json.clone())
            .unwrap_or_else(|e| panic!("TLC-FR-07: {json} must decode, got {e}"));
        assert_eq!(
            normalize_limit(args.limit),
            expected,
            "TLC-FR-07: {json} should resolve to {expected}",
        );
    }
}

#[test]
fn a_call_against_a_root_that_has_been_deleted_still_answers_from_the_snapshot() {
    // The registry is derived from disk on a pass, not read at call time
    // (DSL-FR-15/DSL-FR-16), so a root deleted after the last pass leaves the
    // tool answering from what it last indexed. Pinned so the behaviour is a
    // decision rather than an accident: the paths it returns are stale, and it
    // is the agent's own read of the file that discovers that — not a refusal
    // this tool could honestly make without re-walking the tree on every call.
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "ghost",
        "name: ghost\ndescription: Calibrate the widget assembly to tolerance.",
        "# Body",
    );
    let fixture = mounted(dir);
    let root = {
        let indexer = fixture.app.state::<crate::bm25_index::Bm25Indexer>();
        indexer.root().expect("mounted")
    };

    assert_eq!(names(&call(&fixture, "calibrate the widget", None).unwrap()), vec!["ghost"]);

    std::fs::remove_dir_all(root.join(".claude")).unwrap();
    let after = call(&fixture, "calibrate the widget", None)
        .expect("still an open project, so still an answer rather than a refusal");
    assert_eq!(names(&after), vec!["ghost"]);
    assert!(
        !root.join(&after.skills[0].path).exists(),
        "the path is stale until the next pass, which is what makes this worth pinning",
    );

    // And the next pass reconciles it away.
    fixture.reindex();
    assert!(call(&fixture, "calibrate the widget", None).unwrap().skills.is_empty());
}

// ---------------------------------------------------------------------------
// SST-FR-12 — a name appears once, and the better match keeps it (SST-FR-12)
// ---------------------------------------------------------------------------

#[test]
fn a_colliding_name_is_kept_by_whichever_copy_matched_better() {
    let fixture = collision_project();

    // Every query here leads with `review`, which all three copies share, so
    // all three are genuinely candidates and deduplication has a choice to
    // make. Querying only the discriminating terms would match one copy alone,
    // and the assertion below would then hold under any dedup rule at all —
    // including one that keeps the *worst* match.
    assert_eq!(
        candidates(&fixture, "review"),
        3,
        "the fixture must put all three copies in front of the dedup rule",
    );

    // `.codex` is neither first nor last in path order, so a `.codex` survivor
    // cannot be produced by path order, by walk order, or by chance.
    let output = call(&fixture, "review pull request diff regressions", None).unwrap();
    let reviews: Vec<&SkillMatch> = output.skills.iter().filter(|s| s.name == "review").collect();
    assert_eq!(reviews.len(), 1, "SST-FR-12: a name appears at most once");
    assert_eq!(
        reviews[0].ecosystem,
        Ecosystem::Codex,
        "SST-FR-12: the higher-scoring copy is the one kept",
    );
    assert_eq!(reviews[0].path, ".codex/skills/review/SKILL.md");

    // The complement: lead with the `.claude` copy's terms instead and the
    // survivor changes. Without this, a fixed preference for `.codex` — or for
    // the last path — would satisfy the assertions above.
    let other = call(&fixture, "review specification internal contradictions", None).unwrap();
    let reviews: Vec<&SkillMatch> = other.skills.iter().filter(|s| s.name == "review").collect();
    assert_eq!(reviews.len(), 1);
    assert_eq!(
        reviews[0].ecosystem,
        Ecosystem::Claude,
        "SST-FR-12: the survivor follows the score, not the path",
    );

    // And once more for the copy that sorts last, so no single position can
    // account for all three outcomes.
    let third = call(&fixture, "review database migration destructive", None).unwrap();
    let reviews: Vec<&SkillMatch> = third.skills.iter().filter(|s| s.name == "review").collect();
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].ecosystem, Ecosystem::Opencode);
}

#[test]
fn deduplication_here_is_insensitive_to_case_too() {
    // SST-FR-12. The normalization is shared with the list, but nothing in
    // *this* suite exercised the case fold — so a search-only regression would
    // have shown a model the same name twice while the list showed it once.
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "review",
        "name: Review\ndescription: Review a pull request diff for defects.",
        "# One",
    );
    write_skill(
        dir.path(),
        ".codex/skills",
        "review",
        "name: rEVIEw\ndescription: Review a pull request diff for regressions.",
        "# Two",
    );
    let fixture = mounted(dir);

    let output = call(&fixture, "review pull request diff", None).unwrap();
    assert_eq!(
        output.skills.len(),
        1,
        "one entry however the two spelled the name: {:?}",
        output.skills.iter().map(|s| &s.name).collect::<Vec<_>>(),
    );
}

/// How many skills the delegated call returns for `query`, before this tool
/// deduplicates them.
///
/// Used to assert that a dedup fixture actually presents a collision: a rule
/// that never sees two candidates is a rule no assertion can test.
fn candidates(fixture: &crate::tools::tests::Fixture, query: &str) -> usize {
    let indexer = fixture.app.state::<crate::bm25_index::Bm25Indexer>();
    crate::skills::search_skills(&indexer, query, 25)
        .iter()
        .filter(|r| r.skill.name == "review")
        .count()
}

#[test]
fn deduplication_does_not_refill_the_limit_from_lower_in_the_ranking() {
    // SST-FR-12: deduplication runs over what the delegated call returned for
    // the requested `limit`, so a collision costs the result set a match rather
    // than drawing one up from below.
    let fixture = collision_project();

    // All four skills describe reviewing or authoring; `limit` 2 fetches the
    // two best, which for this query are two of the three `review` copies.
    let output = call(&fixture, "review", Some(2)).unwrap();
    assert_eq!(
        output.skills.len(),
        1,
        "one of the two fetched matches was a duplicate, and nothing replaced it",
    );

    // And with room for all of them, the other names are still reachable — the
    // shrinkage above is the limit interacting with dedup, not dedup eating
    // skills.
    let full = call(&fixture, "review author requirements documents", Some(25)).unwrap();
    let names: Vec<&str> = full.skills.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"analyst") && names.contains(&"review"));
    assert_eq!(
        names.iter().filter(|n| **n == "review").count(),
        1,
        "SST-FR-12: still one entry per name at any limit",
    );
}

// ---------------------------------------------------------------------------
// SST-FR-13 — eligibility is the skills module's alone (SST-FR-13)
// ---------------------------------------------------------------------------

#[test]
fn a_skill_the_registry_excludes_is_unreachable_through_this_tool() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    // One eligible skill, so the query provably matches something.
    write_skill(
        root,
        ".claude/skills",
        "eligible",
        "name: eligible\ndescription: Calibrate the widget assembly to tolerance.",
        "# Body",
    );
    // DSL-FR-08: opted out of model invocation.
    write_skill(
        root,
        ".claude/skills",
        "disabled",
        "name: disabled\ndescription: Calibrate the widget assembly to tolerance.\ndisable-model-invocation: true",
        "# Body",
    );
    // DSL-FR-09: no description. The *folder* is named for the query rather
    // than left as `nodesc`, because a skill with no description indexes as
    // its name alone — so a neutrally-named one could never match this query
    // whether DSL excluded it or not, and the arm would pass with the
    // exclusion deleted.
    write_skill(
        root,
        ".claude/skills",
        "calibrate-the-widget-assembly",
        "name: calibrate the widget assembly to tolerance",
        "# Body",
    );
    // DSL-FR-07: frontmatter that does not parse — an unterminated block,
    // which is what the registry actually rejects. A closed block holding
    // malformed YAML under a key the registry does not read stays eligible by
    // design, so using one here would assert a behaviour nothing has.
    let broken = root.join(".claude/skills/broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(
        broken.join("SKILL.md"),
        "---\nname: broken\ndescription: Calibrate the widget assembly to tolerance.\n\n# Body\n",
    )
    .unwrap();
    // DSL-FR-05: reached through a symlink.
    let outside = tempfile::TempDir::new().unwrap();
    let target = outside.path().join("linked");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        target.join("SKILL.md"),
        "---\nname: linked\ndescription: Calibrate the widget assembly to tolerance.\n---\n\n# Body\n",
    )
    .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, root.join(".claude/skills/linked")).unwrap();

    let fixture = mounted(dir);
    let output = call(&fixture, "calibrate the widget assembly", None).unwrap();

    assert_eq!(
        names(&output),
        vec!["eligible"],
        "SST-FR-13: this tool adds no exclusion, and reaches none the registry excluded",
    );
}

// ---------------------------------------------------------------------------
// SST-FR-14 — never blocks (SST-FR-14)
// ---------------------------------------------------------------------------

#[test]
fn a_call_resolves_without_waiting_on_an_index_pass() {
    // SST-FR-14: `block_on` polls exactly once and panics on `Pending`, so a
    // call that waited on anything would fail here rather than hang.
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "a",
        "name: a\ndescription: Does a thing.",
        "# Body",
    );
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = crate::tools::tests::mock_app();
    let handle = app.handle().clone();
    {
        // Mounted but never passed over: the state the first build is in
        // moments after a project opens.
        let indexer = app.state::<crate::bm25_index::Bm25Indexer>();
        indexer.mount(&crate::fs::RootFs::for_root(&root));
    }

    let output = block_on(SkillSearchTool::new(handle).call(SkillSearchArgs {
        query: "does a thing".to_string(),
        limit: None,
    }))
    .expect("SST-FR-14: answers from the index as it stands");
    assert!(
        output.skills.is_empty(),
        "nothing has been indexed yet, and the call returned rather than waiting",
    );
}

// ---------------------------------------------------------------------------
// SST-FR-15 — read-only (SST-FR-15)
// ---------------------------------------------------------------------------

#[test]
fn searching_leaves_the_project_exactly_as_it_was() {
    let fixture = demo_project();
    let indexer = fixture.app.state::<crate::bm25_index::Bm25Indexer>();
    let root = indexer.root().expect("mounted");

    let before = std::fs::read_to_string(root.join(".claude/skills/analyst/SKILL.md")).unwrap();
    let before_entries = std::fs::read_dir(root.join(".claude/skills")).unwrap().count();

    for (query, limit) in [
        ("review a specification", None),
        ("deploy", Some(3)),
        ("nothing matches this", Some(25)),
    ] {
        let _ = call(&fixture, query, limit);
    }

    assert_eq!(
        before,
        std::fs::read_to_string(root.join(".claude/skills/analyst/SKILL.md")).unwrap(),
    );
    assert_eq!(
        before_entries,
        std::fs::read_dir(root.join(".claude/skills")).unwrap().count(),
    );
    assert!(
        !root.join(".synthesis").exists(),
        "SST-FR-15: nothing was written under .synthesis/",
    );
}

// ---------------------------------------------------------------------------
// SST-FR-16 — logging (SST-FR-16)
// ---------------------------------------------------------------------------

static SST_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn a_call_reports_its_match_count_and_never_the_query_or_a_result() {
    let fixture = demo_project();
    let handle = fixture.handle();
    SST_BUFFER.clear();

    let query = "review a specification for internal contradictions";
    block_on(
        SkillSearchTool::with_buffer(handle.clone(), &SST_BUFFER).call(SkillSearchArgs {
            query: query.to_string(),
            limit: None,
        }),
    )
    .unwrap();
    block_on(
        SkillSearchTool::with_buffer(handle, &SST_BUFFER).call(SkillSearchArgs {
            query: "  ".to_string(),
            limit: None,
        }),
    )
    .unwrap_err();

    let page = SST_BUFFER
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![crate::logging::Domain::Ai],
                ..LogFilter::default()
            },
            None,
            100,
        )
        .unwrap();
    assert_eq!(page.records.len(), 2);

    let info = &page.records[0];
    assert_eq!(info.level, LogLevel::Info);
    assert_eq!(info.fields.get("tool").unwrap(), "search_skills");
    assert_eq!(
        info.fields.get("matches").unwrap(),
        &serde_json::json!(1),
        "SST-FR-16: the count, which is the shape of the answer",
    );

    let warn = &page.records[1];
    assert_eq!(warn.level, LogLevel::Warn);
    assert_eq!(warn.fields.get("reason").unwrap(), "invalid_arguments");

    let serialised = serde_json::to_string(&page.records).unwrap();
    for leaked in [
        query,
        "internal contradictions",
        "analyst",
        ".claude/skills/analyst/SKILL.md",
        EMPTY_QUERY,
    ] {
        assert!(
            !serialised.contains(leaked),
            "SST-FR-16: a record must not carry {leaked:?}",
        );
    }
}

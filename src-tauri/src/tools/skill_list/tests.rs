//! Tests for `SLT-skill-list-tool.md`.

use rig::tool::{PortableTool, ToolErrorKind};
use tauri::Manager;

use super::*;
use crate::logging::{LogBuffer, LogFilter, LogLevel};
use crate::skills::Ecosystem;
use crate::tools::tests::{
    closed_project, collision_project, demo_project, empty_project, mounted, write_skill, Fixture,
};
use crate::tools::tests::block_on;
use crate::tools::ToolRefusal;

fn call(fixture: &Fixture) -> Result<SkillListOutput, ToolRefusal> {
    block_on(SkillListTool::new(fixture.handle()).call(SkillListArgs::default()))
}

fn names(output: &SkillListOutput) -> Vec<&str> {
    output.skills.iter().map(|s| s.name.as_str()).collect()
}

fn paths(output: &SkillListOutput) -> Vec<&str> {
    output.skills.iter().map(|s| s.path.as_str()).collect()
}

// ---------------------------------------------------------------------------
// SLT-FR-01 / SLT-FR-02, SLT-FR-03 — the tool and its definition
// ---------------------------------------------------------------------------

#[test]
fn the_tool_is_a_portable_tool_named_list_skills() {
    // SLT-FR-01.
    let app = closed_project();
    let definition = rig::tool::tool_definition(&SkillListTool::new(app.handle().clone()));

    assert_eq!(definition.name, "list_skills");
    assert_eq!(
        <SkillListTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        "list_skills",
    );
    assert_eq!(NAME, "list_skills");
}

#[test]
fn the_definition_is_the_fixed_text_and_a_schema_declaring_no_parameters() {
    // SLT-FR-02, SLT-FR-03.
    let app = closed_project();
    let definition = rig::tool::tool_definition(&SkillListTool::new(app.handle().clone()));

    assert_eq!(definition.description, DESCRIPTION);

    // Asserting `description() == DESCRIPTION` alone proves only that the
    // method returns its own constant. TLC-FR-05 makes this text contract, so
    // it is checked against the spec that defines it.
    const SPEC: &str = include_str!("../../../../specifications/tools/SLT-skill-list-tool.md");
    assert!(
        SPEC.lines()
            .any(|line| line.trim().strip_prefix("> ") == Some(DESCRIPTION)),
        "SLT-FR-02: the description must be the spec's contract-surface text",
    );

    assert!(
        DESCRIPTION.contains("prefer `search_skills`"),
        "SLT-FR-02: the description points a model holding a specific task at the ranked call",
    );
    assert!(
        DESCRIPTION.contains("takes no parameters"),
        "SLT-FR-03: the description says so as well as the schema",
    );

    let schema = definition.parameters;
    assert_eq!(schema, parameters());
    assert_eq!(schema.get("type").unwrap(), "object");
    assert_eq!(
        schema.get("properties").unwrap(),
        &serde_json::json!({}),
        "SLT-FR-03: no declared properties, so a model calls with {{}}",
    );
    assert_eq!(schema.get("required").unwrap(), &serde_json::json!([]));
}

// ---------------------------------------------------------------------------
// SLT-FR-04 — a stray argument is answered, not refused (SLT-FR-04, SLT-FR-07)
// ---------------------------------------------------------------------------

#[test]
fn a_call_carrying_an_invented_filter_still_returns_the_complete_list() {
    let fixture = demo_project();
    let handle = fixture.handle();

    let bare: SkillListArgs = serde_json::from_value(serde_json::json!({})).unwrap();
    let filtered: SkillListArgs =
        serde_json::from_value(serde_json::json!({ "ecosystem": "claude" })).unwrap();

    let a = block_on(SkillListTool::new(handle.clone()).call(bare)).unwrap();
    let b = block_on(SkillListTool::new(handle).call(filtered)).unwrap();

    assert_eq!(
        a, b,
        "SLT-FR-04: an invented filter changes nothing; the list stays complete (SLT-FR-07)",
    );
    assert_eq!(a.skills.len(), 3);
}

// ---------------------------------------------------------------------------
// SLT-FR-05 — delegation (SLT-FR-05)
// ---------------------------------------------------------------------------

#[test]
fn the_tool_returns_exactly_what_the_skills_module_produced() {
    let fixture = demo_project();
    let indexer = fixture.app.state::<crate::bm25_index::Bm25Indexer>();

    let direct = crate::skills::list_skills(&indexer);
    let through_tool = call(&fixture).unwrap();

    assert_eq!(through_tool.skills.len(), direct.len());
    for (returned, expected) in through_tool.skills.iter().zip(direct.iter()) {
        assert_eq!(returned.name, expected.name);
        assert_eq!(returned.description, expected.description);
        assert_eq!(returned.path, expected.path);
        assert_eq!(returned.ecosystem, expected.ecosystem);
    }
}

#[test]
fn the_tool_reads_no_file_and_enumerates_no_folder_of_its_own() {
    // SLT-FR-05.
    const SOURCE: &str = include_str!("../skill_list.rs");
    let code: String = SOURCE
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in ["std::fs", "read_dir", "read_text", "snapshot()", "SKILL_FOLDERS"] {
        assert!(
            !code.contains(forbidden),
            "SLT-FR-05: this tool delegates and must not reach for {forbidden:?}",
        );
    }
}

// ---------------------------------------------------------------------------
// SLT-FR-06 — what the set is (SLT-FR-06)
// ---------------------------------------------------------------------------

#[test]
fn all_four_ecosystem_folders_are_listed_and_a_nested_one_is_not() {
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();
    write_skill(root, ".claude/skills", "a", "name: a\ndescription: does a", "# A");
    write_skill(root, ".codex/skills", "b", "name: b\ndescription: does b", "# B");
    write_skill(root, ".github/skills", "c", "name: c\ndescription: does c", "# C");
    write_skill(root, ".opencode/skills", "d", "name: d\ndescription: does d", "# D");
    // DSL-FR-02: matched at the project root alone.
    write_skill(
        root,
        "packages/web/.claude/skills",
        "deep",
        "name: deep\ndescription: does deep",
        "# Deep",
    );

    let fixture = mounted(dir);
    let output = call(&fixture).unwrap();

    assert_eq!(names(&output), vec!["a", "b", "c", "d"]);
    assert!(
        !names(&output).contains(&"deep"),
        "SLT-FR-06: a package's own skills folder deeper in the tree is not one of the four",
    );

    let ecosystems: Vec<Ecosystem> = output.skills.iter().map(|s| s.ecosystem).collect();
    assert_eq!(
        ecosystems,
        vec![
            Ecosystem::Claude,
            Ecosystem::Codex,
            Ecosystem::Github,
            Ecosystem::Opencode,
        ],
        "each entry reports the folder family it came from",
    );
}

#[test]
fn a_skill_the_registry_excludes_is_absent_from_the_list() {
    // SLT-FR-06: eligibility is the registry's alone; this tool adds none.
    let dir = tempfile::TempDir::new().unwrap();
    let root = dir.path();

    for (name, front) in [
        ("keep-one", "name: keep-one\ndescription: eligible and listed"),
        ("keep-two", "name: keep-two\ndescription: also eligible"),
        ("keep-three", "name: keep-three\ndescription: eligible as well"),
    ] {
        write_skill(root, ".claude/skills", name, front, "# Body");
    }
    // DSL-FR-08: opted out of model invocation.
    write_skill(
        root,
        ".claude/skills",
        "disabled",
        "name: disabled\ndescription: opted out\ndisable-model-invocation: true",
        "# Body",
    );
    // DSL-FR-09: no description.
    write_skill(root, ".claude/skills", "nodesc", "name: nodesc", "# Body");
    // DSL-FR-07: unterminated frontmatter.
    let broken = root.join(".claude/skills/broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(
        broken.join("SKILL.md"),
        "---\nname: broken\ndescription: never closed\n\n# Body\n",
    )
    .unwrap();
    // DSL-FR-05: reached through a symlink.
    let outside = tempfile::TempDir::new().unwrap();
    let target = outside.path().join("linked");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(
        target.join("SKILL.md"),
        "---\nname: linked\ndescription: outside the project\n---\n\n# Body\n",
    )
    .unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, root.join(".claude/skills/linked")).unwrap();

    let fixture = mounted(dir);
    let output = call(&fixture).unwrap();

    assert_eq!(names(&output), vec!["keep-one", "keep-three", "keep-two"]);
}

// ---------------------------------------------------------------------------
// SLT-FR-07 — complete and never truncated (SLT-FR-07)
// ---------------------------------------------------------------------------

#[test]
fn a_hundred_skills_are_all_returned_with_nothing_elided_or_truncated() {
    let dir = tempfile::TempDir::new().unwrap();
    let long_description =
        "Handles a very specific and elaborately described job that goes on at some length so a truncating implementation would be caught here rather than passing by accident.";
    for index in 0..100 {
        write_skill(
            dir.path(),
            ".claude/skills",
            &format!("skill{index:03}"),
            &format!("name: skill{index:03}\ndescription: {long_description}"),
            "# Body",
        );
    }
    let fixture = mounted(dir);
    let output = call(&fixture).unwrap();

    assert_eq!(output.skills.len(), 100, "SLT-FR-07: no entry is elided");
    for entry in &output.skills {
        assert_eq!(
            entry.description, long_description,
            "SLT-FR-07: no field of any entry is truncated",
        );
    }
}

// ---------------------------------------------------------------------------
// SLT-FR-08 — order (SLT-FR-08)
// ---------------------------------------------------------------------------

#[test]
fn two_calls_return_the_same_entries_in_ascending_path_order() {
    let fixture = demo_project();

    let first = call(&fixture).unwrap();
    let second = call(&fixture).unwrap();
    assert_eq!(first, second, "SLT-FR-08: a model can refer back to what it saw");

    let ordered = paths(&first);
    let mut sorted = ordered.clone();
    sorted.sort();
    assert_eq!(ordered, sorted, "SLT-FR-08: ascending path order");
    // The registry's own walk visits the four folders in an order that is
    // already ascending by path prefix, so this assertion cannot distinguish
    // the sort from the walk. What it *can* pin — and what is this tool's
    // responsibility rather than the registry's — is that the tool preserves
    // whatever order it was handed, which
    // `the_tool_returns_exactly_what_the_skills_module_produced` asserts
    // position by position.
    assert_eq!(
        ordered,
        vec![
            ".claude/skills/analyst/SKILL.md",
            ".codex/skills/deployer/SKILL.md",
            ".github/skills/formatter/SKILL.md",
        ],
    );
}

// ---------------------------------------------------------------------------
// SLT-FR-09 — the entry shape (SLT-FR-09, SLT-FR-10)
// ---------------------------------------------------------------------------

#[test]
fn an_entry_carries_exactly_the_documented_fields_and_its_path_resolves() {
    let fixture = demo_project();
    let output = call(&fixture).unwrap();
    let entry = output.skills.first().expect("an entry");

    let value = serde_json::to_value(entry).unwrap();
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap()
        .keys()
        .map(|k| k.as_str())
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec!["description", "ecosystem", "name", "path"],
        "SLT-FR-09: no score — this tool ranks nothing — and no folder_name",
    );
    assert!(
        !entry.description.contains("sarcophagus"),
        "SLT-FR-09: no text drawn from the skill's body",
    );

    // SLT-FR-10.
    let indexer = fixture.app.state::<crate::bm25_index::Bm25Indexer>();
    let root = indexer.root().expect("mounted");
    assert!(root.join(&entry.path).is_file());
}

// ---------------------------------------------------------------------------
// SLT-FR-11 / SLT-FR-12 — no project versus no skills
// ---------------------------------------------------------------------------

#[test]
fn no_project_open_refuses_rather_than_returning_an_empty_list() {
    // SLT-FR-11: a model told the list is empty would conclude the project
    // offers no skills and stop looking.
    let app = closed_project();
    let handle = app.handle().clone();

    let refusal = block_on(SkillListTool::new(handle).call(SkillListArgs::default()))
        .expect_err("SLT-FR-11");
    assert_eq!(refusal, ToolRefusal::NoProjectOpen);

    let error = refusal.to_execution_error();
    assert_eq!(error.kind(), ToolErrorKind::NotFound);
    assert_eq!(error.retryable(), Some(false));

    // DSL-FR-15 would have answered an empty list here; the tool does not.
    let indexer = app.state::<crate::bm25_index::Bm25Indexer>();
    assert!(crate::skills::list_skills(&indexer).is_empty());
}

#[test]
fn map_error_is_what_carries_a_refusal_to_the_model() {
    // TLC-FR-10 is about `map_error` specifically. Asserting on
    // `ToolRefusal::to_execution_error` alone would leave the trait method
    // free to return anything at all.
    let app = closed_project();
    let mapped = SkillListTool::new(app.handle().clone()).map_error(ToolRefusal::NoProjectOpen);

    assert_eq!(mapped.kind(), ToolErrorKind::NotFound);
    assert_eq!(mapped.retryable(), Some(false));
    assert_eq!(mapped.message(), crate::tools::NO_PROJECT_OPEN);
}

#[test]
fn an_open_project_with_no_eligible_skill_succeeds_with_an_empty_list() {
    // SLT-FR-12: a project that ships no skill is a fact, not a failure.
    let output = call(&empty_project()).expect("SLT-FR-12");
    assert!(output.skills.is_empty());
}

// ---------------------------------------------------------------------------
// SLT-FR-08 — a name appears once, first in path order (SLT-FR-08, SLT-FR-13)
// ---------------------------------------------------------------------------

#[test]
fn a_colliding_name_is_kept_by_the_first_copy_in_path_order() {
    let fixture = collision_project();
    let output = call(&fixture).unwrap();

    // `.claude` beats `.codex` beats `.opencode`, those folder names sorting
    // that way — and the surviving entry keeps its position in the order rather
    // than being appended after the survivors were chosen (SLT-FR-08).
    // `packager` sits between the kept `.claude` review and the dropped
    // `.codex` and `.opencode` ones in path order, so a survivor appears both
    // before and after a dropped entry: an implementation that appended the
    // survivors rather than leaving them in place would reorder this.
    assert_eq!(
        paths(&output),
        vec![
            ".claude/skills/analyst/SKILL.md",
            ".claude/skills/review/SKILL.md",
            ".codex/skills/packager/SKILL.md",
        ],
        "SLT-FR-13: one entry per name, first in path order, order preserved",
    );
    assert_eq!(
        output.skills.iter().map(|s| s.ecosystem).collect::<Vec<_>>(),
        vec![Ecosystem::Claude, Ecosystem::Claude, Ecosystem::Codex],
    );

    // The description the surviving entry carries is its own, not merged from
    // the copies that lost.
    let review = output.skills.iter().find(|s| s.name == "review").unwrap();
    assert!(
        review.description.contains("specification"),
        "the kept entry carries the kept file's own description: {:?}",
        review.description,
    );
    assert!(!review.description.contains("pull request"));
}

#[test]
fn deduplication_is_insensitive_to_case() {
    // The three tools share one normalization: were the list to treat `Review`
    // and `review` as two skills while `load_skill` treats them as one, the
    // list would show a model two entries that `load_skill` then refuses as
    // ambiguous, and its description's promise that each name appears once
    // would be false.
    //
    // Only the case fold is asserted here. Padding cannot reach this path at
    // all — `skills::store` trims a declared name before it ever becomes a
    // descriptor — so a fixture declaring `name: "  review  "` would prove
    // nothing about `normalize_skill_name`'s own `trim`, which exists for the
    // model-supplied argument `load_skill` matches against.
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "review",
        "name: Review\ndescription: Review a specification.",
        "# One",
    );
    write_skill(
        dir.path(),
        ".codex/skills",
        "review",
        "name: rEVIEw\ndescription: Review a diff.",
        "# Two",
    );
    let fixture = mounted(dir);

    let output = call(&fixture).unwrap();
    assert_eq!(output.skills.len(), 1, "one name, however it was cased");
    assert_eq!(output.skills[0].ecosystem, Ecosystem::Claude);
    assert_eq!(output.skills[0].name, "Review", "and it keeps its own spelling");
}

// ---------------------------------------------------------------------------
// SLT-FR-14 — the two tools answer from one set of names (SLT-FR-14)
// ---------------------------------------------------------------------------

#[test]
fn the_two_tools_agree_on_names_and_may_differ_on_which_copy_stands_behind_one() {
    use crate::tools::skill_search::{SkillSearchArgs, SkillSearchTool};

    let fixture = collision_project();
    let listed = call(&fixture).unwrap();

    // The `.codex` copy matches this query best, so the search keeps it while
    // the list keeps `.claude`'s. SLT-FR-14 promises the *name*, not the file.
    let found = block_on(SkillSearchTool::new(fixture.handle()).call(SkillSearchArgs {
        query: "pull request diff regressions".to_string(),
        limit: Some(25),
    }))
    .unwrap();

    let listed_review = listed.skills.iter().find(|s| s.name == "review").unwrap();
    let found_review = found.skills.iter().find(|s| s.name == "review").unwrap();

    assert_eq!(
        listed_review.name, found_review.name,
        "SLT-FR-14: a name means the same capability in both",
    );
    assert_ne!(
        listed_review.path, found_review.path,
        "SLT-FR-14: and `path` and `ecosystem` need not agree",
    );
    assert_eq!(listed_review.ecosystem, Ecosystem::Claude);
    assert_eq!(found_review.ecosystem, Ecosystem::Codex);
}

#[test]
fn every_listed_skill_is_rankable_and_every_ranked_skill_is_listed() {
    use crate::tools::skill_search::{SkillSearchArgs, SkillSearchTool};

    let fixture = demo_project();
    let handle = fixture.handle();
    let listed = call(&fixture).unwrap();
    assert_eq!(listed.skills.len(), 3);

    // Every listed skill is one `search_skills` can rank.
    for entry in &listed.skills {
        let found = block_on(SkillSearchTool::new(handle.clone()).call(SkillSearchArgs {
            query: entry.description.clone(),
            limit: Some(25),
        }))
        .unwrap();
        assert!(
            found.skills.iter().any(|m| m.path == entry.path),
            "SLT-FR-14: {} is listed but not rankable",
            entry.path,
        );
    }

    // And every skill a broad query returns appears in the list.
    let broad = block_on(SkillSearchTool::new(handle).call(SkillSearchArgs {
        query: "specification deploy reformat source production style".to_string(),
        limit: Some(25),
    }))
    .unwrap();
    assert!(!broad.skills.is_empty());
    for matched in &broad.skills {
        assert!(
            listed.skills.iter().any(|e| e.path == matched.path),
            "SLT-FR-14: {} is rankable but not listed",
            matched.path,
        );
    }
}

// ---------------------------------------------------------------------------
// SLT-FR-15 — never blocks (SLT-FR-15)
// ---------------------------------------------------------------------------

#[test]
fn a_call_resolves_without_waiting_on_an_index_pass() {
    // SLT-FR-15: `block_on` polls once and panics on `Pending`.
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "a",
        "name: a\ndescription: does a thing",
        "# Body",
    );
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = crate::tools::tests::mock_app();
    let handle = app.handle().clone();
    {
        // Mounted but never passed over.
        let indexer = app.state::<crate::bm25_index::Bm25Indexer>();
        indexer.mount(&crate::fs::RootFs::for_root(&root));
    }

    let output = block_on(SkillListTool::new(handle).call(SkillListArgs::default()))
        .expect("SLT-FR-15: answers from the registry as it stands");
    assert!(output.skills.is_empty(), "the call returned rather than waiting");
}

// ---------------------------------------------------------------------------
// SLT-FR-16 — a declaration change lands on the next pass (SLT-FR-16)
// ---------------------------------------------------------------------------

#[test]
fn opting_out_and_back_in_removes_and_restores_the_skill() {
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "toggler",
        "name: toggler\ndescription: toggles",
        "# Body",
    );
    write_skill(
        dir.path(),
        ".claude/skills",
        "stable",
        "name: stable\ndescription: stays put",
        "# Body",
    );
    let fixture = mounted(dir);
    let file = {
        let indexer = fixture.app.state::<crate::bm25_index::Bm25Indexer>();
        indexer
            .root()
            .expect("mounted")
            .join(".claude/skills/toggler/SKILL.md")
    };

    assert_eq!(names(&call(&fixture).unwrap()), vec!["stable", "toggler"]);

    std::fs::write(
        &file,
        "---\nname: toggler\ndescription: toggles\ndisable-model-invocation: true\n---\n\n# Body\n",
    )
    .unwrap();
    fixture.reindex();
    assert_eq!(
        names(&call(&fixture).unwrap()),
        vec!["stable"],
        "SLT-FR-16: the opt-out lands on the next pass",
    );

    std::fs::write(
        &file,
        "---\nname: toggler\ndescription: toggles\n---\n\n# Body\n",
    )
    .unwrap();
    fixture.reindex();
    let restored = call(&fixture).unwrap();
    assert_eq!(
        names(&restored),
        vec!["stable", "toggler"],
        "SLT-FR-16: and so does removing it — exactly once",
    );
}

// ---------------------------------------------------------------------------
// SLT-FR-17 — read-only (SLT-FR-17)
// ---------------------------------------------------------------------------

#[test]
fn listing_leaves_the_project_exactly_as_it_was() {
    let fixture = demo_project();
    let root = {
        let indexer = fixture.app.state::<crate::bm25_index::Bm25Indexer>();
        indexer.root().expect("mounted")
    };
    let before = std::fs::read_to_string(root.join(".claude/skills/analyst/SKILL.md")).unwrap();

    for _ in 0..10 {
        let _ = call(&fixture);
    }

    assert_eq!(
        before,
        std::fs::read_to_string(root.join(".claude/skills/analyst/SKILL.md")).unwrap(),
    );
    assert!(!root.join(".synthesis").exists());
}

// ---------------------------------------------------------------------------
// SLT-FR-18 — logging (SLT-FR-18)
// ---------------------------------------------------------------------------

static SLT_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn a_call_reports_its_skill_count_and_never_a_name_description_or_path() {
    let fixture = demo_project();
    SLT_BUFFER.clear();

    block_on(
        SkillListTool::with_buffer(fixture.handle(), &SLT_BUFFER).call(SkillListArgs::default()),
    )
    .unwrap();

    let closed = closed_project();
    block_on(
        SkillListTool::with_buffer(closed.handle().clone(), &SLT_BUFFER)
            .call(SkillListArgs::default()),
    )
    .unwrap_err();

    let page = SLT_BUFFER
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
    assert_eq!(info.fields.get("tool").unwrap(), "list_skills");
    assert_eq!(info.fields.get("skills").unwrap(), &serde_json::json!(3));

    let warn = &page.records[1];
    assert_eq!(warn.level, LogLevel::Warn);
    assert_eq!(warn.fields.get("reason").unwrap(), "no_project_open");
    assert_eq!(warn.fields.get("retryable").unwrap(), &serde_json::json!(false));

    let serialised = serde_json::to_string(&page.records).unwrap();
    for leaked in [
        "analyst",
        "deployer",
        "formatter",
        ".claude/skills",
        "Review a specification",
        crate::tools::NO_PROJECT_OPEN,
    ] {
        assert!(
            !serialised.contains(leaked),
            "SLT-FR-18: a record must not carry {leaked:?}",
        );
    }
}

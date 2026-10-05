//! The skills and skill indexes, end to end.

use super::*;

// ---------------------------------------------------------------------------
// The eleventh index (BMI-FR-02, BMI-FR-05, and DSL-dynamic-skills-loading.md end to end)
// ---------------------------------------------------------------------------

/// A project holding one invocable skill, plus the same word in places that
/// must not answer a skills query.
fn project_with_a_skill() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let skill = root.join(".claude/skills/reviewer");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        // `teardown` sits in the description AND in the body, so a hit can be
        // attributed to one or the other rather than to the file as a whole;
        // `sarcophagus` sits in the body alone.
        "---\nname: Teardown reviewer\ndescription: reviews worktree teardown\n---\n\n\
         # Body\n\nA long discussion of teardown and of the sarcophagus.\n",
    )
    .unwrap();
    dir
}

#[test]
fn the_skills_index_holds_the_descriptor_while_the_skill_index_holds_the_body() {
    // BMI-FR-02, BMI-FR-05 / DSL-FR-13, DSL-FR-14: the same file under two different documents, in
    // two indexes that share no statistics.
    let dir = project_with_a_skill();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = indexed(&root);
    let indexer = app.state::<Bm25Indexer>();

    let skills = search(&indexer, &[IndexId::Skills], "teardown", 10);
    assert_eq!(skills.len(), 1, "one descriptor document, not one per section");
    assert_eq!(skills[0].path, ".claude/skills/reviewer/SKILL.md");
    assert_eq!(skills[0].chunk_ordinal, 0, "a skill is one whole document");
    assert_eq!(
        skills[0].node_id, None,
        "a skill carries no scan node id — its file may not be in the tree at all"
    );
    assert!(
        skills[0].text.contains("reviews worktree teardown")
            && !skills[0].text.contains("sarcophagus"),
        "the document is the descriptor, not the body: {:?}",
        skills[0].text
    );

    // The body term reaches the artifact index and nothing else.
    assert!(
        search(&indexer, &[IndexId::Skills], "sarcophagus", 10).is_empty(),
        "a term found only in the body ranks nothing in the skills index"
    );
    let body = search(&indexer, &[IndexId::Skill], "sarcophagus", 10);
    assert_eq!(
        body.len(),
        1,
        "the same file's body is chunked into the `skill` artifact index"
    );
    assert_eq!(
        body[0].node_id.as_deref(),
        Some(".claude/skills/reviewer/SKILL.md"),
        "an artifact hit still carries its scan node id"
    );
}

#[test]
fn list_and_search_skills_answer_from_one_snapshot() {
    // DSL-FR-16 / DSL-FR-18, DSL-FR-20: the registry and the index agree, and both
    // degenerate inputs and an unmounted indexer yield an empty list.
    use crate::skills::{list_skills, search_skills};
    let dir = project_with_a_skill();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = indexed(&root);
    let indexer = app.state::<Bm25Indexer>();

    let listed = list_skills(&indexer);
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "Teardown reviewer");
    assert_eq!(listed[0].folder_name, "reviewer");

    let ranked = search_skills(&indexer, "worktree teardown", 10);
    assert_eq!(ranked.len(), 1, "the listed skill is the rankable one");
    assert_eq!(ranked[0].skill, listed[0], "the same descriptor, not a copy");
    assert!(ranked[0].score > 0.0);

    assert!(search_skills(&indexer, "", 10).is_empty(), "empty query");
    assert!(search_skills(&indexer, "teardown", 0).is_empty(), "zero limit");
    let unmounted = Bm25Indexer::default();
    assert!(list_skills(&unmounted).is_empty(), "no project open");
    assert!(search_skills(&unmounted, "teardown", 10).is_empty());
}

#[test]
fn opting_out_removes_a_skill_from_both_the_registry_and_the_index() {
    // DSL-FR-18, DSL-FR-20 / DSL-FR-22: a change to what a SKILL.md declares takes effect
    // on the next pass, in both directions and in both halves at once.
    use crate::skills::{list_skills, search_skills};
    let dir = project_with_a_skill();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = indexed(&root);
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    let skill_md = root.join(".claude/skills/reviewer/SKILL.md");
    let original = std::fs::read_to_string(&skill_md).unwrap();
    assert_eq!(list_skills(&indexer).len(), 1, "precondition");

    // Opt out.
    std::fs::write(
        &skill_md,
        original.replacen(
            "description:",
            "disable-model-invocation: true\ndescription:",
            1,
        ),
    )
    .unwrap();
    let generation = indexer.generation();
    indexer
        .run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation)
        .expect("the pass publishes");
    assert!(
        list_skills(&indexer).is_empty(),
        "the registry drops it (DSL-FR-08)"
    );
    assert!(
        search_skills(&indexer, "teardown", 10).is_empty(),
        "and so does the index — they are never observed disagreeing (DSL-FR-18)"
    );
    assert!(
        !search(&indexer, &[IndexId::Skill], "sarcophagus", 10).is_empty(),
        "the artifact index is untouched by an opt-out — the file is still a file"
    );

    // Opt back in.
    std::fs::write(&skill_md, &original).unwrap();
    let generation = indexer.generation();
    indexer
        .run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation)
        .expect("the pass publishes");
    assert_eq!(
        list_skills(&indexer).len(),
        1,
        "present again, exactly once (DSL-FR-20)"
    );
    assert_eq!(search_skills(&indexer, "teardown", 10).len(), 1);
}

#[test]
fn a_gitignored_skill_is_indexed_and_a_pass_scoped_to_artifacts_still_re_enumerates() {
    // DSL-FR-06, DSL-FR-11 / DSL-FR-20 / DSL-FR-19: the skills half is enumerated in full
    // on every pass whatever scope it was asked for, which is what makes a
    // gitignored skill's edit — invisible to the scan, and so to the candidate
    // list the artifacts half reads — still land.
    use crate::skills::{list_skills, search_skills};
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::changes::canonicalize_lenient(dir.path());
    std::fs::write(root.join(".gitignore"), ".claude/\n").unwrap();
    let skill = root.join(".claude/skills/local");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: Local\ndescription: describes cartography\n---\n\n# Body\n",
    )
    .unwrap();

    let app = indexed(&root);
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    assert_eq!(list_skills(&indexer).len(), 1, "gitignored, but present");
    assert!(
        indexer.snapshot().paths(IndexId::Skill).is_empty(),
        "the scan honours .gitignore, so the artifact index has nothing — the \
         two enumerations disagree by design (DSL-FR-06)"
    );

    // Edit only the gitignored file, then run a pass scoped to artifacts —
    // the scope a watcher event produces.
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: Local\ndescription: describes bathymetry\n---\n\n# Body\n",
    )
    .unwrap();
    let generation = indexer.generation();
    indexer
        .run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation)
        .expect("the pass publishes");
    assert_eq!(
        list_skills(&indexer)[0].description,
        "describes bathymetry",
        "the registry reflects the edit"
    );
    assert_eq!(search_skills(&indexer, "bathymetry", 10).len(), 1);
    assert!(
        search_skills(&indexer, "cartography", 10).is_empty(),
        "the previous description's document was replaced, not added to"
    );
}

#[test]
fn a_removed_skill_folder_leaves_the_registry_and_the_index() {
    // DSL-FR-19 / DSL-FR-25: the pass re-enumerates rather than applying a
    // delta, so a deletion needs no event of its own to be noticed.
    use crate::skills::list_skills;
    let dir = project_with_a_skill();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = indexed(&root);
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    assert_eq!(list_skills(&indexer).len(), 1, "precondition");

    std::fs::remove_dir_all(root.join(".claude/skills/reviewer")).unwrap();
    let generation = indexer.generation();
    indexer
        .run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation)
        .expect("the pass publishes");
    assert!(list_skills(&indexer).is_empty());
    assert!(
        search(&indexer, &[IndexId::Skills], "teardown", 10).is_empty(),
        "no chunk of a removed skill survives"
    );
}

#[test]
fn closing_a_project_discards_the_skill_registry_with_the_indexes() {
    // DSL-FR-22: the registry rides the snapshot, so BMI-FR-25's
    // teardown carries it without needing one of its own.
    use crate::skills::{list_skills, search_skills};
    let dir = project_with_a_skill();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = indexed(&root);
    let indexer = app.state::<Bm25Indexer>();
    assert_eq!(list_skills(&indexer).len(), 1, "precondition");

    indexer.clear();
    assert!(list_skills(&indexer).is_empty(), "closed: an empty list");
    assert!(search_skills(&indexer, "teardown", 10).is_empty());
    assert_eq!(indexer.root(), None);
}

#[test]
fn indexing_skills_writes_nothing_anywhere_under_the_project() {
    // DSL-FR-23: enumeration reads and never writes.
    let dir = project_with_a_skill();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let before = tree_fingerprint(&root);
    let app = indexed(&root);
    let indexer = app.state::<Bm25Indexer>();
    let _ = crate::skills::search_skills(&indexer, "teardown", 10);
    let _ = crate::skills::list_skills(&indexer);
    assert_eq!(
        tree_fingerprint(&root),
        before,
        "no file under the project changed"
    );
}

#[test]
fn a_skills_query_selects_only_the_skills_index() {
    // DSL-FR-16: `search_skills` scores against the skills index alone, so a
    // spec that shares the descriptor's wording cannot be returned as a skill.
    use crate::skills::search_skills;
    let dir = project_with_a_skill();
    let root = crate::changes::canonicalize_lenient(dir.path());
    std::fs::create_dir_all(root.join("specifications")).unwrap();
    std::fs::write(
        root.join("specifications/wtc.md"),
        "# Worktree context\n\nreviews worktree teardown at length\n",
    )
    .unwrap();
    let app = indexed(&root);
    let indexer = app.state::<Bm25Indexer>();

    assert!(
        !search(&indexer, &[IndexId::Spec], "worktree teardown", 10).is_empty(),
        "precondition: the spec is indexed and matches"
    );
    let ranked = search_skills(&indexer, "worktree teardown", 10);
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].skill.path, ".claude/skills/reviewer/SKILL.md");
}

#[test]
fn a_drafts_scoped_pass_still_reconciles_the_skills_index() {
    // DSL-FR-19: "every pass re-enumerates all four folders whatever triggered
    // it". Without `PassScope::covers(Skills) == true`, a drafts-scoped pass
    // would set the registry from a fresh enumeration while leaving the
    // removed skill's document in the index — the registry and the index
    // disagreeing, which DSL-FR-18 forbids outright.
    use crate::skills::list_skills;
    let dir = project_with_a_skill();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let app = indexed(&root);
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    assert_eq!(list_skills(&indexer).len(), 1, "precondition");

    std::fs::remove_dir_all(root.join(".claude/skills/reviewer")).unwrap();
    let generation = indexer.generation();
    indexer
        .run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::DRAFTS, generation)
        .expect("the pass publishes");

    assert!(
        list_skills(&indexer).is_empty(),
        "the registry drops it even on a pass that was not about artifacts"
    );
    assert!(
        search(&indexer, &[IndexId::Skills], "teardown", 10).is_empty(),
        "and so does the index — the two never disagree (DSL-FR-18)"
    );
}

#[test]
fn skills_rank_by_relevance_and_honour_the_limit() {
    // DSL-FR-16 / DSL-FR-17: with more than one skill in the
    // registry, ordering and truncation become assertable — and so does the
    // `filter_map` in `search_skills` not silently dropping entries.
    use crate::skills::search_skills;
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let write = |folder: &str, name: &str, desc: &str| {
        let d = root.join(folder).join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {desc}\n---\n\n# Body\n"),
        )
        .unwrap();
    };
    write(".claude/skills", "specs", "reviews and critiques a specification document");
    write(".claude/skills", "fonts", "enumerates the fonts installed on this machine");
    write(".claude/skills", "ports", "checks which network ports are listening");
    // Same name as the first, different ecosystem (DSL-FR-17).
    write(".codex/skills", "specs", "an unrelated skill about baking bread");

    let app = indexed(&root);
    let indexer = app.state::<Bm25Indexer>();

    let ranked = search_skills(&indexer, "review a specification", 10);
    assert!(!ranked.is_empty(), "the query matches something");
    assert_eq!(
        ranked[0].skill.path, ".claude/skills/specs/SKILL.md",
        "the apt skill ranks first, not merely somewhere in the list: {:?}",
        ranked.iter().map(|r| (&r.skill.path, r.score)).collect::<Vec<_>>()
    );
    for pair in ranked.windows(2) {
        assert!(
            pair[0].score >= pair[1].score,
            "scores are non-increasing: {:?}",
            ranked.iter().map(|r| r.score).collect::<Vec<_>>()
        );
    }

    // Two same-named skills are separate entries told apart by path.
    let both = search_skills(&indexer, "specification bread", 10);
    let named: Vec<&str> = both
        .iter()
        .filter(|r| r.skill.name == "specs")
        .map(|r| r.skill.path.as_str())
        .collect();
    assert_eq!(
        named,
        vec![".claude/skills/specs/SKILL.md", ".codex/skills/specs/SKILL.md"],
        "a name is not an identity (DSL-FR-17): {named:?}"
    );

    // And the limit truncates rather than being advisory: the capped result is
    // the head of the uncapped one, so what was dropped is what scored least.
    let query = "specification bread fonts ports";
    let uncapped = search_skills(&indexer, query, 10);
    assert!(
        uncapped.len() > 2,
        "precondition: the query matches more than the cap, or truncation is untested"
    );
    let capped = search_skills(&indexer, query, 2);
    assert_eq!(capped.len(), 2, "the limit bounds the returned list");
    assert_eq!(
        capped.iter().map(|r| r.skill.path.as_str()).collect::<Vec<_>>(),
        uncapped
            .iter()
            .take(2)
            .map(|r| r.skill.path.as_str())
            .collect::<Vec<_>>(),
        "the truncation keeps the best-scoring entries rather than an arbitrary two"
    );
}

#[test]
fn a_registry_reached_incrementally_equals_one_enumerated_fresh() {
    // DSL-FR-25: the claim is not "two walks of one tree agree"
    // (which `read_dir`'s stability gives for free) but "a registry brought to
    // a state by a series of passes is identical to one enumerated from
    // scratch against the same tree".
    use crate::skills::list_skills;
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let write = |folder: &str, name: &str, body: &str| {
        let d = root.join(folder).join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("SKILL.md"), body).unwrap();
    };
    let front = |desc: &str| format!("---\nname: N\ndescription: {desc}\n---\n\n# Body\n");

    // Reach the target tree through a series of passes: add, edit, delete,
    // opt out and back in.
    write(".claude/skills", "alpha", &front("about alpha"));
    write(".claude/skills", "doomed", &front("about to be deleted"));
    let app = indexed(&root);
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();

    let step = |root: &std::path::Path| {
        let generation = indexer.generation();
        indexer
            .run_pass(&handle, &crate::fs::RootFs::for_root(root), PassScope::ARTIFACTS, generation)
            .expect("the pass publishes");
    };
    write(".codex/skills", "bravo", &front("about bravo"));
    step(&root);
    write(".claude/skills", "alpha", &front("about alpha, revised"));
    step(&root);
    std::fs::remove_dir_all(root.join(".claude/skills/doomed")).unwrap();
    step(&root);
    write(
        ".claude/skills",
        "toggled",
        "---\nname: N\ndescription: toggled\ndisable-model-invocation: true\n---\n",
    );
    step(&root);
    write(".claude/skills", "toggled", &front("toggled"));
    step(&root);

    let incremental = list_skills(&indexer);

    // A fresh walk of the tree the passes arrived at.
    let fresh = crate::skills::enumerate(&crate::fs::RootFs::for_root(root)).0;
    assert_eq!(
        incremental, fresh,
        "an incremental history never drifts from what a fresh walk produces"
    );
    assert_eq!(
        incremental.iter().map(|s| s.path.as_str()).collect::<Vec<_>>(),
        vec![
            ".claude/skills/alpha/SKILL.md",
            ".claude/skills/toggled/SKILL.md",
            ".codex/skills/bravo/SKILL.md",
        ],
        "and it is the tree we actually built"
    );
    assert_eq!(incremental[0].description, "about alpha, revised");
}

#[test]
fn a_draft_skill_becomes_eligible_only_once_it_is_published() {
    // DSL-FR-21: a draft's files live under `.synthesis/drafts/` rather than
    // in a special folder, so they are never eligible; the pass that carries a
    // graduation is what makes the published `SKILL.md` reachable.
    use crate::skills::list_skills;
    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let drafted = root.join(".synthesis/drafts/d1/files/.claude/skills/new");
    std::fs::create_dir_all(&drafted).unwrap();
    std::fs::write(
        drafted.join("SKILL.md"),
        "---\nname: Newborn\ndescription: still a draft\n---\n\n# Body\n",
    )
    .unwrap();

    let app = indexed(&root);
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();
    assert!(
        list_skills(&indexer).is_empty(),
        "a draft's SKILL.md is not in a special folder, whatever it is destined for"
    );

    // Publish it where a graduation would, and let the graduation's own pass
    // (which covers both halves) run.
    let published = root.join(".claude/skills/new");
    std::fs::create_dir_all(&published).unwrap();
    std::fs::write(
        published.join("SKILL.md"),
        "---\nname: Newborn\ndescription: published at last\n---\n\n# Body\n",
    )
    .unwrap();
    std::fs::remove_dir_all(root.join(".synthesis/drafts/d1")).unwrap();
    let generation = indexer.generation();
    indexer
        .run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ALL, generation)
        .expect("the pass publishes");

    let listed = list_skills(&indexer);
    assert_eq!(listed.len(), 1, "eligible in the pass that carried the publish");
    assert_eq!(listed[0].path, ".claude/skills/new/SKILL.md");
    assert_eq!(listed[0].description, "published at last");
}

#[test]
fn an_excluded_skill_is_logged_once_per_time_the_exclusion_arises() {
    // DSL-FR-24: the WARN has to exist — an author who wrote a
    // skill and cannot find it has nowhere else to look — and it has to name
    // the path and the reason and nothing of the file's contents.
    //
    // The query is scoped to a fixture-unique folder name rather than counting
    // records globally: `logging::BUFFER` is process-global and `cargo test`
    // runs in parallel, so a global count would be flaky by construction.
    use crate::logging::{Domain, LogFilter, LogLevel};
    const UNIQUE: &str = "zz-dsl-log-probe-9f3a";
    let secret = "hunter2-must-not-appear";

    let dir = tempfile::TempDir::new().unwrap();
    let root = crate::changes::canonicalize_lenient(dir.path());
    let d = root.join(".claude/skills").join(UNIQUE);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("SKILL.md"), format!("# {secret}\n\nno frontmatter\n")).unwrap();

    let app = indexed(&root);
    let handle = app.handle().clone();
    let indexer = app.state::<Bm25Indexer>();

    let ours = || {
        crate::logging::BUFFER
            .query(
                &LogFilter {
                    min_level: LogLevel::Warn,
                    domains: vec![Domain::Backend],
                    query: Some(UNIQUE.to_string()),
                    query_is_regex: false,
                },
                None,
                100,
            )
            .expect("the filter compiles")
            .records
    };

    let first = ours();
    assert_eq!(
        first.len(),
        1,
        "the exclusion is reported exactly once by the first pass: {first:?}"
    );
    assert_eq!(first[0].level, LogLevel::Warn);
    assert!(first[0].domains.contains(&Domain::Backend));
    let rendered = format!("{:?}", first[0]);
    assert!(
        rendered.contains(crate::skills::REASON_NO_FRONTMATTER),
        "the record names why: {rendered}"
    );
    assert!(
        !rendered.contains(secret),
        "and carries no part of the file's contents: {rendered}"
    );

    // A pass runs on every filesystem change anywhere in the project, and this
    // exclusion is a steady state. Re-reporting it on each one would push real
    // evidence out of a bounded ring to say the same thing repeatedly.
    for _ in 0..3 {
        let generation = indexer.generation();
        indexer
            .run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation)
            .expect("the pass publishes");
    }
    assert_eq!(
        ours().len(),
        1,
        "an unchanged exclusion is not re-reported by later passes"
    );

    // But a *changed* reason is news again.
    std::fs::write(d.join("SKILL.md"), "---\nname: Now named\n---\n\n# Body\n").unwrap();
    let generation = indexer.generation();
    indexer
        .run_pass(&handle, &crate::fs::RootFs::for_root(&root), PassScope::ARTIFACTS, generation)
        .expect("the pass publishes");
    let after = ours();
    assert_eq!(after.len(), 2, "a new reason is reported: {after:?}");
    assert!(
        format!("{:?}", after[1]).contains(crate::skills::REASON_NO_DESCRIPTION),
        "and it is the new reason: {:?}",
        after[1]
    );
}


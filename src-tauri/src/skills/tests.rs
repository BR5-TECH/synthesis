//! Tests for `DSL-dynamic-skills-loading.md`.
//!
//! Eligibility is filesystem-shaped, so most of these build a real temp tree
//! rather than stubbing one: the claims worth pinning — a symlink refused, a
//! basename compared case-sensitively, a folder one level too deep — are
//! precisely the ones a stub would answer by construction.

use super::*;
use std::path::PathBuf;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// A `SKILL.md` body with the given frontmatter block.
fn skill_md(front: &str) -> String {
    format!("---\n{front}\n---\n\n# Body\n\nSome prose about the skill.\n")
}

/// Write `.../<folder>/<name>/SKILL.md` with `front` as its frontmatter.
fn write_skill(root: &Path, folder: &str, name: &str, front: &str) -> PathBuf {
    let dir = root.join(folder).join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("SKILL.md");
    std::fs::write(&file, skill_md(front)).unwrap();
    file
}

fn paths(skills: &[SkillDescriptor]) -> Vec<&str> {
    skills.iter().map(|s| s.path.as_str()).collect()
}

fn reasons(excluded: &[Exclusion]) -> Vec<(&str, &str)> {
    excluded
        .iter()
        .map(|e| (e.path.as_str(), e.reason))
        .collect()
}

// ---------------------------------------------------------------------------
// DSL-FR-15, DSL-FR-02 — where a skill may live (DSL-FR-02, DSL-FR-12)
// ---------------------------------------------------------------------------

#[test]
fn all_four_special_folders_are_enumerated_with_their_ecosystem() {
    // DSL-FR-02, DSL-FR-12, DSL-FR-15: the four folder families, each reporting which one it came
    // from, in path order (DSL-FR-15).
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(root, ".claude/skills", "a", "name: A\ndescription: does a");
    write_skill(root, ".codex/skills", "b", "name: B\ndescription: does b");
    write_skill(root, ".github/skills", "c", "name: C\ndescription: does c");
    write_skill(root, ".opencode/skills", "d", "name: D\ndescription: does d");

    let (skills, excluded) = enumerate(root);
    assert_eq!(
        paths(&skills),
        vec![
            ".claude/skills/a/SKILL.md",
            ".codex/skills/b/SKILL.md",
            ".github/skills/c/SKILL.md",
            ".opencode/skills/d/SKILL.md",
        ],
        "all four folders are eligible, and the result is sorted by path"
    );
    assert_eq!(
        skills.iter().map(|s| s.ecosystem).collect::<Vec<_>>(),
        vec![
            Ecosystem::Claude,
            Ecosystem::Codex,
            Ecosystem::Github,
            Ecosystem::Opencode
        ]
    );
    assert_eq!(
        skills.iter().map(|s| s.folder_name.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c", "d"],
        "the <skill-name> segment is reported (DSL-FR-12)"
    );
    assert!(excluded.is_empty(), "nothing was excluded: {excluded:?}");
}

#[test]
fn a_special_folder_deeper_in_the_tree_is_not_one_of_the_four() {
    // DSL-FR-02: the four are matched at the project root alone, so a
    // monorepo package's own `.claude/skills/` yields nothing and cannot
    // collide with the root's.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(root, ".claude/skills", "shallow", "description: at the root");
    write_skill(
        root,
        "packages/web/.claude/skills",
        "deep",
        "description: in a package",
    );

    let (skills, _) = enumerate(root);
    assert_eq!(paths(&skills), vec![".claude/skills/shallow/SKILL.md"]);
}

// ---------------------------------------------------------------------------
// DSL-FR-03, DSL-FR-13 — the exact shape (DSL-FR-03, DSL-FR-04)
// ---------------------------------------------------------------------------

#[test]
fn only_the_exact_folder_and_basename_shape_is_a_skill() {
    // DSL-FR-03: one folder segment, the file in that folder rather than
    // directly in the special folder, and the basename `SKILL.md` exactly.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());

    // Too deep: `.claude/skills/a/b/SKILL.md`.
    write_skill(root, ".claude/skills", "a/b", "description: too deep");
    // Directly in the special folder, no skill folder at all.
    std::fs::create_dir_all(root.join(".claude/skills")).unwrap();
    std::fs::write(
        root.join(".claude/skills/foo.md"),
        skill_md("description: loose"),
    )
    .unwrap();
    // Wrong case on the basename.
    let lower = root.join(".claude/skills/c");
    std::fs::create_dir_all(&lower).unwrap();
    std::fs::write(lower.join("skill.md"), skill_md("description: lowercase")).unwrap();

    let (skills, excluded) = enumerate(root);
    assert!(
        skills.is_empty(),
        "none of the three shapes is a skill: {:?}",
        paths(&skills)
    );
    assert!(
        excluded.is_empty(),
        "a file that is merely not a skill is ignored, not reported (DSL-FR-24): {excluded:?}"
    );
}

#[test]
fn a_lowercase_skill_md_is_rejected_and_never_duplicates_a_correct_one() {
    // The case rule has to hold on a case-insensitive filesystem, where
    // `skill_dir.join("SKILL.md")` would happily open `skill.md`. Reading the
    // directory and comparing the name on disk is what makes it real.
    //
    // The rejection below is only load-bearing on a folding filesystem — on
    // ext4 the naive probe is correct too, so it passes there without
    // exercising the rule. The two halves therefore cover different
    // filesystems, and each states which one it is on rather than leaving the
    // other running green over an untested guarantee.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let only_lower = root.join(".claude/skills/only-lower");
    std::fs::create_dir_all(&only_lower).unwrap();
    std::fs::write(
        only_lower.join("skill.md"),
        skill_md("description: lowercase"),
    )
    .unwrap();

    let (skills, _) = enumerate(root);
    assert!(
        skills.is_empty(),
        "a folder holding only `skill.md` has no skill in it: {:?}",
        paths(&skills)
    );

    // On a case-SENSITIVE filesystem a folder can hold both names at once, and
    // the correct one must be found exactly once rather than twice. That tree
    // cannot be built on a folding one — the second write lands on the same
    // inode — so the case is probed rather than assumed, which is what makes
    // this assertion meaningful on the CI the half above is vacuous on.
    let both = root.join(".claude/skills/both");
    std::fs::create_dir_all(&both).unwrap();
    std::fs::write(both.join("SKILL.md"), skill_md("description: the real one")).unwrap();
    std::fs::write(both.join("skill.md"), skill_md("description: the decoy")).unwrap();

    let (skills, _) = enumerate(root);
    if crate::fs::case_probe::folds_case(dir.path()) {
        // The decoy overwrote the real one: they are one file. Nothing about
        // "which name was read" can be asked here, and the honest statement is
        // that the folder still yields exactly one skill rather than two.
        assert_eq!(
            paths(&skills),
            vec![".claude/skills/both/SKILL.md"],
            "one file under one name, found once"
        );
        assert_eq!(
            skills[0].description, "the decoy",
            "the second write landed on the same file, which is what folding means"
        );
    } else {
        assert_eq!(
            paths(&skills),
            vec![".claude/skills/both/SKILL.md"],
            "one entry, not two"
        );
        assert_eq!(
            skills[0].description, "the real one",
            "the uppercase file is the one that was read"
        );
    }
}

#[test]
fn supporting_files_beside_a_skill_are_not_entries_of_their_own() {
    // DSL-FR-04, DSL-FR-13: a skill's entry is its SKILL.md and nothing else.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(root, ".claude/skills", "analyst", "description: authors specs");
    let refs = root.join(".claude/skills/analyst/references");
    std::fs::create_dir_all(&refs).unwrap();
    std::fs::write(refs.join("rules.md"), "# Rules\n\nsarcophagus\n").unwrap();
    std::fs::write(
        root.join(".claude/skills/analyst/run.sh"),
        "#!/bin/sh\necho hi\n",
    )
    .unwrap();

    let (skills, _) = enumerate(root);
    assert_eq!(paths(&skills), vec![".claude/skills/analyst/SKILL.md"]);
    assert!(
        !skills[0].document().contains("sarcophagus"),
        "a supporting file contributes no text to the document (DSL-FR-13)"
    );
}

// ---------------------------------------------------------------------------
// DSL-FR-24 — symlinks (DSL-FR-05)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn every_symlinked_path_component_disqualifies_a_skill() {
    // DSL-FR-05, DSL-FR-24: a symlinked skill folder, a symlinked SKILL.md, and a
    // symlinked special folder are all refused, and nothing is resolved
    // through any of them — while real skills elsewhere are returned normally.
    use std::os::unix::fs::symlink;
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());

    // A real skill living outside the four folders, to be pointed at.
    let outside = root.join("outside/shared");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(
        outside.join("SKILL.md"),
        skill_md("name: Borrowed\ndescription: reached through a link"),
    )
    .unwrap();

    std::fs::create_dir_all(root.join(".claude/skills")).unwrap();
    // (a) the <skill-name> folder is a symlink
    symlink(&outside, root.join(".claude/skills/linked")).unwrap();
    // (b) the SKILL.md itself is a symlink
    let direct = root.join(".claude/skills/direct");
    std::fs::create_dir_all(&direct).unwrap();
    symlink(outside.join("SKILL.md"), direct.join("SKILL.md")).unwrap();
    // (c) the special folder itself is a symlink
    let codex_real = root.join("elsewhere/codexskills/x");
    std::fs::create_dir_all(&codex_real).unwrap();
    std::fs::write(
        codex_real.join("SKILL.md"),
        skill_md("description: under a linked folder"),
    )
    .unwrap();
    std::fs::create_dir_all(root.join(".codex")).unwrap();
    symlink(
        root.join("elsewhere/codexskills"),
        root.join(".codex/skills"),
    )
    .unwrap();
    // A genuine skill, which must survive all of the above.
    write_skill(root, ".claude/skills", "real", "description: an honest skill");

    let (skills, excluded) = enumerate(root);
    assert_eq!(
        paths(&skills),
        vec![".claude/skills/real/SKILL.md"],
        "only the non-symlinked skill is eligible"
    );
    let reported = reasons(&excluded);
    assert!(
        reported.contains(&(".claude/skills/linked", REASON_SYMLINK)),
        "a symlinked skill folder is reported: {reported:?}"
    );
    assert!(
        reported.contains(&(".claude/skills/direct/SKILL.md", REASON_SYMLINK)),
        "a symlinked SKILL.md is reported: {reported:?}"
    );
    assert!(
        reported.contains(&(".codex/skills", REASON_SYMLINK)),
        "a symlinked special folder is reported: {reported:?}"
    );
    assert!(
        !skills.iter().any(|s| s.name == "Borrowed"),
        "nothing was read through a link"
    );
}

// ---------------------------------------------------------------------------
// DSL-FR-11, DSL-FR-06 — enumeration is this module's own (DSL-FR-06)
// ---------------------------------------------------------------------------

#[test]
fn a_gitignored_skill_folder_is_still_enumerated() {
    // DSL-FR-06, DSL-FR-11: this is the one enumeration in the application that does not
    // inherit ASC-FR-09's exclusions — a skill an agent can invoke is present
    // whether or not it is committed. The scan's own view is asserted beside
    // it, so the deliberate disagreement is visible rather than implied.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join(".gitignore"), ".claude/\n").unwrap();
    write_skill(root, ".claude/skills", "local", "description: untracked but usable");

    let (skills, _) = enumerate(root);
    assert_eq!(paths(&skills), vec![".claude/skills/local/SKILL.md"]);

    let scanned = crate::scanning::scan(root);
    let surfaced = crate::scanning::candidate_files(&scanned)
        .iter()
        .any(|c| c.path == ".claude/skills/local/SKILL.md");
    assert!(
        !surfaced,
        "the scan honours .gitignore and does not surface it — the two disagree by design"
    );
}

#[test]
fn absent_and_unreadable_special_folders_are_not_errors() {
    // DSL-FR-06: a project with no `.codex/` or `.opencode/` at all still
    // yields its `.claude` skills, and so does one whose `.github/skills` is
    // not something that can be walked.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(root, ".claude/skills", "a", "description: the only one");
    // `.github/skills` exists but is a file, not a directory.
    std::fs::create_dir_all(root.join(".github")).unwrap();
    std::fs::write(root.join(".github/skills"), "not a directory").unwrap();

    let (skills, excluded) = enumerate(root);
    assert_eq!(paths(&skills), vec![".claude/skills/a/SKILL.md"]);
    assert!(excluded.is_empty(), "nothing to report: {excluded:?}");
}

#[cfg(unix)]
#[test]
fn a_special_folder_that_cannot_be_read_is_skipped_rather_than_fatal() {
    // DSL-FR-06's other branch: `read_dir` itself failing, which is a
    // different code path from the "not a directory" case above. Running as
    // root defeats the permission bit, so that case is skipped rather than
    // asserted falsely.
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(root, ".claude/skills", "a", "description: still found");
    let locked = root.join(".codex/skills");
    std::fs::create_dir_all(locked.join("hidden")).unwrap();
    std::fs::write(
        locked.join("hidden/SKILL.md"),
        skill_md("description: unreachable"),
    )
    .unwrap();
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let readable_anyway = std::fs::read_dir(&locked).is_ok();
    if !readable_anyway {
        let (skills, excluded) = enumerate(root);
        assert_eq!(
            paths(&skills),
            vec![".claude/skills/a/SKILL.md"],
            "the walkable folder still yields its skill"
        );
        assert!(excluded.is_empty(), "an unreadable folder is not a report");
    }
    // Restore so the TempDir can be cleaned up.
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn an_empty_project_yields_an_empty_registry() {
    let dir = TempDir::new().unwrap();
    let (skills, excluded) = enumerate(&crate::fs::RootFs::for_root(dir.path()));
    assert!(skills.is_empty());
    assert!(excluded.is_empty());
}

// ---------------------------------------------------------------------------
// DSL-FR-24 through DSL-FR-10 — frontmatter (DSL-FR-07 … DSL-FR-10)
// ---------------------------------------------------------------------------

#[test]
fn a_file_without_a_closed_frontmatter_block_is_excluded() {
    // DSL-FR-07, DSL-FR-24: no frontmatter at all, and a block that opens but never
    // closes. The latter is what keeps a leading horizontal rule from being
    // read as a declaration.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let bare = root.join(".claude/skills/bare");
    std::fs::create_dir_all(&bare).unwrap();
    std::fs::write(bare.join("SKILL.md"), "# Just a heading\n\nNo frontmatter.\n").unwrap();
    let open = root.join(".claude/skills/unterminated");
    std::fs::create_dir_all(&open).unwrap();
    std::fs::write(
        open.join("SKILL.md"),
        "---\nname: X\ndescription: never closed\n\n# Body\n",
    )
    .unwrap();

    let (skills, excluded) = enumerate(root);
    assert!(skills.is_empty(), "neither is eligible: {:?}", paths(&skills));
    assert_eq!(
        reasons(&excluded),
        vec![
            (".claude/skills/bare/SKILL.md", REASON_NO_FRONTMATTER),
            (".claude/skills/unterminated/SKILL.md", REASON_NO_FRONTMATTER),
        ],
        "each is reported once with its reason (DSL-FR-24)"
    );
}

#[test]
fn malformed_yaml_inside_a_closed_block_does_not_exclude() {
    // A skill is not excluded for a key this module does not read. Only the
    // three it does read matter.
    let front = "name: Kept\ndescription: still eligible\nallowed-tools: [ unclosed\n: : :";
    let parsed = parse_frontmatter(&skill_md(front)).expect("a closed block parses");
    assert_eq!(parsed.name.as_deref(), Some("Kept"));
    assert_eq!(parsed.description.as_deref(), Some("still eligible"));
}

#[test]
fn disable_model_invocation_excludes_only_when_truthy() {
    // DSL-FR-08: boolean `true` and the string "TRUE" both exclude; `false`,
    // a garbage value, and the key's absence all leave the skill included.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(
        root,
        ".claude/skills",
        "quiet",
        "description: opted out\ndisable-model-invocation: true",
    );
    write_skill(
        root,
        ".claude/skills",
        "quoted",
        "description: opted out loudly\ndisable-model-invocation: \"TRUE\"",
    );
    write_skill(
        root,
        ".claude/skills",
        "loud",
        "description: opted in\ndisable-model-invocation: false",
    );
    write_skill(
        root,
        ".claude/skills",
        "garbage",
        "description: nonsense value\ndisable-model-invocation: banana",
    );

    let (skills, excluded) = enumerate(root);
    assert_eq!(
        paths(&skills),
        vec![
            ".claude/skills/garbage/SKILL.md",
            ".claude/skills/loud/SKILL.md",
        ],
        "only a truthy value excludes"
    );
    assert_eq!(
        reasons(&excluded),
        vec![
            (".claude/skills/quiet/SKILL.md", REASON_DISABLED),
            (".claude/skills/quoted/SKILL.md", REASON_DISABLED),
        ]
    );
}

#[test]
fn a_missing_or_blank_description_excludes() {
    // DSL-FR-09, DSL-FR-24.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(root, ".claude/skills", "nodesc", "name: Nameless purpose");
    write_skill(root, ".claude/skills", "blankdesc", "description: \"   \"");
    write_skill(root, ".claude/skills", "ok", "description: has one");

    let (skills, excluded) = enumerate(root);
    assert_eq!(paths(&skills), vec![".claude/skills/ok/SKILL.md"]);
    assert_eq!(
        reasons(&excluded),
        vec![
            (".claude/skills/blankdesc/SKILL.md", REASON_NO_DESCRIPTION),
            (".claude/skills/nodesc/SKILL.md", REASON_NO_DESCRIPTION),
        ]
    );
}

#[test]
fn a_missing_name_falls_back_to_the_folder_rather_than_excluding() {
    // DSL-FR-10: the "go by the basename" rule of ASC-FR-19, applied to the
    // folder segment — a skill is never excluded for having no name.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(root, ".claude/skills", "code-review", "description: reviews code");
    write_skill(
        root,
        ".claude/skills",
        "blank-name",
        "name: \"\"\ndescription: also nameless",
    );

    let (skills, _) = enumerate(root);
    let named: Vec<(&str, &str)> = skills
        .iter()
        .map(|s| (s.folder_name.as_str(), s.name.as_str()))
        .collect();
    assert_eq!(
        named,
        vec![("blank-name", "blank-name"), ("code-review", "code-review")],
        "both fall back to their folder segment"
    );
}

#[test]
fn a_declared_name_wins_over_the_folder() {
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(
        root,
        ".claude/skills",
        "cr",
        "name: Code review\ndescription: reviews code",
    );
    let (skills, _) = enumerate(root);
    assert_eq!(skills[0].name, "Code review");
    assert_eq!(skills[0].folder_name, "cr");
}

#[test]
fn a_block_scalar_description_is_folded_into_one_document() {
    // A description long enough to want a block scalar is exactly the
    // description worth indexing, so `>-` and `|` must not fall through to the
    // "no description" exclusion.
    for indicator in [">", ">-", "|", "|-"] {
        let front = format!(
            "name: Folded\ndescription: {indicator}\n  first line of the description\n  second line about worktrees\nallowed-tools: Read"
        );
        let parsed = parse_frontmatter(&skill_md(&front))
            .unwrap_or_else(|| panic!("`{indicator}` must parse"));
        assert_eq!(
            parsed.description.as_deref(),
            Some("first line of the description second line about worktrees"),
            "`{indicator}` folds into one description"
        );
        assert_eq!(
            parsed.name.as_deref(),
            Some("Folded"),
            "`{indicator}` does not swallow the keys around it"
        );
    }
}

#[test]
fn a_block_scalar_ends_at_the_closing_delimiter() {
    // The fold must stop at `---`, not run into the body.
    let text = "---\ndescription: |\n  the description\n---\n\n# Body\n\nnot part of it\n";
    let parsed = parse_frontmatter(text).expect("parses");
    assert_eq!(parsed.description.as_deref(), Some("the description"));
}

#[test]
fn an_indented_key_belongs_to_something_else() {
    // Only top-level keys count, matching `scanning::read_content_facts`.
    let front = "metadata:\n  description: not the skill's own\ndescription: the real one";
    let parsed = parse_frontmatter(&skill_md(front)).expect("parses");
    assert_eq!(parsed.description.as_deref(), Some("the real one"));
}

// ---------------------------------------------------------------------------
// DSL-FR-11 — independence from the scan's classification (DSL-FR-11)
// ---------------------------------------------------------------------------

#[test]
fn eligibility_ignores_the_resolved_artifact_type_in_both_directions() {
    // DSL-FR-11: a SKILL.md in a special folder qualifies whatever the scan
    // made of it, and a file the scan types `skill` outside the four folders
    // never does.
    //
    // The assignments below are what make this test mean anything: without
    // them the scan resolves nothing interesting, and the assertion would hold
    // for the trivial reason that no classification exists to disagree with.
    // So each is asserted as a precondition through the scan itself first.
    use crate::scanning::ArtifactType;
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(root, ".claude/skills", "a", "description: in a special folder");
    let docs = root.join("docs");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("SKILL.md"), skill_md("description: not special")).unwrap();

    // The special-folder skill is typed something *other* than `skill`, and
    // the outsider is typed `skill` — the classification inverted relative to
    // where the files actually live.
    crate::scanning::assign(
        root,
        ".claude/skills/a/SKILL.md",
        ArtifactType::Prompt,
        crate::scanning::Scope::File,
    )
    .unwrap();
    crate::scanning::assign(
        root,
        "docs",
        ArtifactType::Skill,
        crate::scanning::Scope::Folder,
    )
    .unwrap();

    let classified = crate::scanning::candidate_files(&crate::scanning::scan(root));
    let type_of = |p: &str| {
        classified
            .iter()
            .find(|c| c.path == p)
            .and_then(|c| c.artifact_type)
    };
    assert_eq!(
        type_of(".claude/skills/a/SKILL.md"),
        Some(ArtifactType::Prompt),
        "precondition: the scan really did type the special-folder skill `prompt`"
    );
    assert_eq!(
        type_of("docs/SKILL.md"),
        Some(ArtifactType::Skill),
        "precondition: the scan really did type the outsider `skill`"
    );

    let (skills, _) = enumerate(root);
    assert_eq!(
        paths(&skills),
        vec![".claude/skills/a/SKILL.md"],
        "location decides, not classification — in both directions"
    );
}

// ---------------------------------------------------------------------------
// DSL-FR-23 — size and encoding (DSL-FR-23)
// ---------------------------------------------------------------------------

#[test]
fn an_unreadable_skill_is_excluded_rather_than_fatal() {
    // DSL-FR-23: neither an oversized nor a non-UTF-8 SKILL.md contributes,
    // and the skills beside them are returned normally.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(root, ".claude/skills", "fine", "description: readable");

    let huge = root.join(".claude/skills/huge");
    std::fs::create_dir_all(&huge).unwrap();
    let mut body = String::from("---\ndescription: enormous\n---\n");
    body.push_str(&"x".repeat((MAX_FILE_BYTES + 1) as usize));
    std::fs::write(huge.join("SKILL.md"), body).unwrap();

    let binary = root.join(".claude/skills/binary");
    std::fs::create_dir_all(&binary).unwrap();
    std::fs::write(binary.join("SKILL.md"), [0xff, 0xfe, 0x00, 0x01]).unwrap();

    let (skills, excluded) = enumerate(root);
    assert_eq!(paths(&skills), vec![".claude/skills/fine/SKILL.md"]);
    let reported = reasons(&excluded);
    assert!(
        reported.contains(&(".claude/skills/binary/SKILL.md", REASON_NOT_TEXT)),
        "a non-UTF-8 SKILL.md is reported: {reported:?}"
    );
    assert!(
        reported.contains(&(".claude/skills/huge/SKILL.md", REASON_TOO_LARGE)),
        "an oversized SKILL.md is reported: {reported:?}"
    );
}

// ---------------------------------------------------------------------------
// DSL-FR-25 — determinism (DSL-FR-25)
// ---------------------------------------------------------------------------

#[test]
fn the_same_tree_enumerates_identically_every_time() {
    // DSL-FR-25: membership, resolved name, description, and order all stable,
    // whatever order `read_dir` happens to hand entries back in.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    for name in ["zulu", "alpha", "mike", "bravo"] {
        write_skill(
            root,
            ".claude/skills",
            name,
            &format!("name: {name}\ndescription: about {name}"),
        );
    }
    write_skill(root, ".codex/skills", "alpha", "description: a codex alpha");

    // NOTE: `first == second` alone would NOT discriminate — `read_dir` order
    // is stable within a process, so two back-to-back walks of an unchanged
    // tree agree however unordered the code is. The sort is pinned by the
    // explicit path vector below, whose expected order differs from the order
    // the folders were created in.
    let first = enumerate(root).0;
    let second = enumerate(root).0;
    assert_eq!(first, second, "two enumerations of one tree agree exactly");
    assert_eq!(
        paths(&first),
        vec![
            ".claude/skills/alpha/SKILL.md",
            ".claude/skills/bravo/SKILL.md",
            ".claude/skills/mike/SKILL.md",
            ".claude/skills/zulu/SKILL.md",
            ".codex/skills/alpha/SKILL.md",
        ],
        "sorted by path rather than by directory order"
    );
}

// ---------------------------------------------------------------------------
// DSL-FR-17 — a name is not an identity (DSL-FR-17)
// ---------------------------------------------------------------------------

#[test]
fn two_skills_of_the_same_name_in_different_ecosystems_both_survive() {
    // DSL-FR-17: neither displaces the other, and `path` tells them apart.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(root, ".claude/skills", "review", "name: Review\ndescription: claude's");
    write_skill(root, ".codex/skills", "review", "name: Review\ndescription: codex's");

    let (skills, _) = enumerate(root);
    assert_eq!(skills.len(), 2);
    assert!(skills.iter().all(|s| s.name == "Review"));
    assert_eq!(
        skills.iter().map(|s| s.ecosystem).collect::<Vec<_>>(),
        vec![Ecosystem::Claude, Ecosystem::Codex]
    );
}

// ---------------------------------------------------------------------------
// DSL-FR-13 — the indexed document
// ---------------------------------------------------------------------------

#[test]
fn the_document_is_the_name_and_description_and_nothing_else() {
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(
        root,
        ".claude/skills",
        "a",
        "name: Teardown reviewer\ndescription: reviews worktree teardown",
    );
    let (skills, _) = enumerate(root);
    let doc = skills[0].document();
    assert!(doc.contains("Teardown reviewer"));
    assert!(doc.contains("reviews worktree teardown"));
    assert!(
        !doc.contains("Some prose about the skill"),
        "the body below the frontmatter contributes nothing: {doc:?}"
    );
}

// ---------------------------------------------------------------------------
// Parser edge cases — regression locks on behavior verified by hand
// ---------------------------------------------------------------------------

#[test]
fn a_utf8_bom_before_the_frontmatter_does_not_disqualify_a_skill() {
    // Windows editors emit a BOM routinely, and `str::lines` treats it as part
    // of the first line — so without the strip, a perfectly correct skill is
    // excluded for "no frontmatter" while looking right in every editor its
    // author opens it in.
    let text = "\u{feff}---\nname: Bommed\ndescription: authored on windows\n---\n\n# Body\n";
    let parsed = parse_frontmatter(text).expect("a BOM must not hide the frontmatter");
    assert_eq!(parsed.name.as_deref(), Some("Bommed"));
    assert_eq!(parsed.description.as_deref(), Some("authored on windows"));
}

#[test]
fn crlf_frontmatter_parses_like_lf() {
    let text = "---\r\nname: Windows\r\ndescription: carriage returns\r\n---\r\n\r\n# Body\r\n";
    let parsed = parse_frontmatter(text).expect("CRLF frontmatter parses");
    assert_eq!(parsed.name.as_deref(), Some("Windows"));
    assert_eq!(parsed.description.as_deref(), Some("carriage returns"));
}

#[test]
fn a_key_is_matched_whole_rather_than_by_prefix() {
    // `nameplate:` must not satisfy `name:`, and `description-of-thing:` must
    // not satisfy `description:` — the `strip_prefix(key)` chain only works
    // because the `:` is stripped separately, and that is worth pinning.
    let front = "nameplate: not a name\ndescription-of-thing: not a description\n\
                 name: The name\ndescription: The description";
    let parsed = parse_frontmatter(&skill_md(front)).expect("parses");
    assert_eq!(parsed.name.as_deref(), Some("The name"));
    assert_eq!(parsed.description.as_deref(), Some("The description"));
}

#[test]
fn every_key_is_first_declaration_wins() {
    // A file declaring a key twice is malformed either way; what matters is
    // that all three resolve by the same rule, so the outcome is predictable.
    let front = "name: First\nname: Second\ndescription: First desc\n\
                 description: Second desc\ndisable-model-invocation: true\n\
                 disable-model-invocation: false";
    let parsed = parse_frontmatter(&skill_md(front)).expect("parses");
    assert_eq!(parsed.name.as_deref(), Some("First"));
    assert_eq!(parsed.description.as_deref(), Some("First desc"));
    assert!(
        parsed.disable_model_invocation,
        "the boolean is first-wins too, not last-wins"
    );
}

#[test]
fn a_whitespace_only_name_falls_back_to_the_folder() {
    // A quoted run of spaces is not a name. Left untrimmed it would satisfy
    // the `is_some()` check and produce a descriptor whose `name` is three
    // spaces — neither the declaration nor the DSL-FR-10 fallback.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(
        root,
        ".claude/skills",
        "spaces",
        "name: \"   \"\ndescription: has a real description",
    );
    let (skills, _) = enumerate(root);
    assert_eq!(
        skills[0].name, "spaces",
        "a blank name falls back to the folder segment (DSL-FR-10)"
    );
}

#[test]
fn an_empty_file_and_an_empty_skill_folder_are_handled_distinctly() {
    // An empty `SKILL.md` sits at an eligible path and is worth reporting; an
    // empty skill folder holds no `SKILL.md` at all and is merely not a skill,
    // so it is silent (DSL-FR-24).
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let empty_file = root.join(".claude/skills/empty-file");
    std::fs::create_dir_all(&empty_file).unwrap();
    std::fs::write(empty_file.join("SKILL.md"), "").unwrap();
    std::fs::create_dir_all(root.join(".claude/skills/empty-folder")).unwrap();

    let (skills, excluded) = enumerate(root);
    assert!(skills.is_empty());
    assert_eq!(
        reasons(&excluded),
        vec![(".claude/skills/empty-file/SKILL.md", REASON_NO_FRONTMATTER)],
        "the empty folder is silent; the empty file is reported"
    );
}

#[test]
fn a_unicode_folder_name_survives_enumeration_and_the_name_fallback() {
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write_skill(root, ".claude/skills", "日本語-скилл", "description: unicode");
    let (skills, _) = enumerate(root);
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].folder_name, "日本語-скилл");
    assert_eq!(skills[0].name, "日本語-скилл", "the fallback is byte-exact");
    assert_eq!(skills[0].path, ".claude/skills/日本語-скилл/SKILL.md");
}

#[cfg(unix)]
#[test]
fn a_dangling_symlink_is_refused_rather_than_treated_as_missing() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir_all(root.join(".claude/skills")).unwrap();
    symlink(
        root.join("nowhere-at-all"),
        root.join(".claude/skills/dangling"),
    )
    .unwrap();

    let (skills, excluded) = enumerate(root);
    assert!(skills.is_empty());
    assert_eq!(
        reasons(&excluded),
        vec![(".claude/skills/dangling", REASON_SYMLINK)],
        "`symlink_metadata` succeeds on a dangling link, so it is refused as a \
         link rather than skipped as absent"
    );
}

// ---------------------------------------------------------------------------
// The log records this module produces (DSL-FR-24)
// ---------------------------------------------------------------------------

#[test]
fn an_exclusion_reason_never_carries_any_of_the_files_contents() {
    // DSL-FR-24: the reason is a fixed phrase chosen here, not anything read
    // out of the file. A `SKILL.md` is user content, and the only thing that
    // can enforce this is the emit site — nothing downstream redacts.
    let dir = TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let secret = "hunter2-do-not-log-me";
    let d = root.join(".claude/skills/leaky");
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("SKILL.md"),
        format!("---\nname: {secret}\ndisable-model-invocation: true\ndescription: {secret}\n---\n{secret}\n"),
    )
    .unwrap();

    let (_, excluded) = enumerate(root);
    assert_eq!(excluded.len(), 1);
    assert_eq!(excluded[0].reason, REASON_DISABLED);
    assert!(
        !excluded[0].path.contains(secret) && !excluded[0].reason.contains(secret),
        "neither the path nor the reason carries file content"
    );
    for reason in [
        REASON_SYMLINK,
        REASON_NO_FRONTMATTER,
        REASON_DISABLED,
        REASON_NO_DESCRIPTION,
        REASON_TOO_LARGE,
        REASON_NOT_TEXT,
    ] {
        assert!(
            !reason.is_empty() && reason.is_ascii(),
            "a reason is a fixed phrase this module chose"
        );
    }
}

// ---------------------------------------------------------------------------
// DSL-FR-01 — no frontend can reach this module
// ---------------------------------------------------------------------------

#[test]
fn no_part_of_this_module_is_reachable_from_the_frontend() {
    // DSL-FR-01: no `#[tauri::command]`, so nothing here can be `invoke`d.
    // Comment lines are stripped first — this file's own prose mentions the
    // attribute, and matching that would be a false positive.
    let source = include_str!("../skills.rs");
    let code: String = source
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !code.contains("#[tauri::command]"),
        "this module registers no Tauri command (DSL-FR-01)"
    );
}


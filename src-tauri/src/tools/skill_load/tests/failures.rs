//! LSK-FR-15, LSK-FR-16: an unreadable file, and no project open.

use super::*;

// ---------------------------------------------------------------------------
// LSK-FR-15 — an unreadable file (LSK-FR-15)
// ---------------------------------------------------------------------------

#[test]
fn a_file_gone_since_the_last_pass_refuses_rather_than_returning_empty_text() {
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "doomed",
        "name: doomed\ndescription: About to be deleted.",
        "# Doomed",
    );
    let path = dir.path().join(".claude/skills/doomed/SKILL.md");
    let fixture = mounted(dir);

    assert!(call(&fixture, "doomed", None).unwrap().contains("# Doomed"));

    // Deleted, and no pass run — the registry still names it (LSK-FR-17).
    std::fs::remove_file(&path).unwrap();
    let refusal = call(&fixture, "doomed", None).unwrap_err();
    assert_eq!(
        refusal,
        ToolRefusal::SkillUnreadable,
        "LSK-FR-15: a failed read is a refusal, not an empty success",
    );
    let error = refusal.to_execution_error();
    assert_eq!(error.kind(), ToolErrorKind::NotFound, "LSK-FR-15");
    assert_eq!(error.retryable(), Some(true), "LSK-FR-15");
    assert_eq!(error.message(), SKILL_UNREADABLE);
}

#[cfg(unix)]
#[test]
fn a_skill_that_became_a_symlink_is_refused_rather_than_followed() {
    let dir = tempfile::TempDir::new().unwrap();
    write_skill(
        dir.path(),
        ".claude/skills",
        "swapped",
        "name: swapped\ndescription: About to be replaced by a link.",
        "# Genuine",
    );
    let path = dir.path().join(".claude/skills/swapped/SKILL.md");
    let elsewhere = dir.path().join("elsewhere.md");
    std::fs::write(&elsewhere, "TARGET CONTENT NOBODY ASKED FOR\n").unwrap();
    let fixture = mounted(dir);

    assert!(call(&fixture, "swapped", None).unwrap().contains("# Genuine"));

    // The registry admitted a real file; it is a link now, and FSA-FR-17
    // refuses one whatever it points at.
    std::fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&elsewhere, &path).unwrap();

    let outcome = call(&fixture, "swapped", None);
    assert_eq!(outcome.clone().unwrap_err(), ToolRefusal::SkillUnreadable);
    assert!(
        !format!("{outcome:?}").contains("TARGET CONTENT"),
        "LSK-FR-15: the link's target was never read",
    );
}

// ---------------------------------------------------------------------------
// LSK-FR-16 — no project open (LSK-FR-16)
// ---------------------------------------------------------------------------

#[test]
fn no_project_open_is_the_shared_refusal_and_not_the_unknown_skill_one() {
    let app = closed_project();
    let refusal = block_on(SkillLoadTool::new(app.handle().clone()).call(LoadSkillArgs {
        name: "analyst".to_string(),
        ecosystem: None,
    }))
    .unwrap_err();

    assert_eq!(refusal, ToolRefusal::NoProjectOpen);
    assert_eq!(
        refusal.to_string(),
        NO_PROJECT_OPEN,
        "LSK-FR-16: a model told 'no skill by that name' would conclude the \
         project lacks a skill it has",
    );
    assert_ne!(refusal.to_string(), SKILL_NOT_FOUND);
    let error = refusal.to_execution_error();
    assert_eq!(error.kind(), ToolErrorKind::NotFound);
    assert_eq!(error.retryable(), Some(false));
}


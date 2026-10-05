//! LSK-FR-19: logging.

use super::*;

// ---------------------------------------------------------------------------
// LSK-FR-19 — logging (LSK-FR-19)
// ---------------------------------------------------------------------------

static LSK_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn a_call_reports_a_byte_count_and_never_the_name_path_or_instructions() {
    // A multi-byte body, so `bytes` cannot be satisfied by a character count:
    // LSK-FR-19 says the size in bytes, and on ASCII the two are equal.
    let dir = tempfile::TempDir::new().unwrap();
    let skill = dir.path().join(".claude/skills/analyst");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: analyst\ndescription: Author requirement documents.\n---\n# Анализ — 日本語\n",
    )
    .unwrap();
    let fixture = mounted(dir);
    LSK_BUFFER.clear();

    let body = block_on(
        SkillLoadTool::with_buffer(fixture.handle(), &LSK_BUFFER).call(LoadSkillArgs {
            name: "analyst".to_string(),
            ecosystem: Some("claude".to_string()),
        }),
    )
    .unwrap();
    assert!(
        body.len() != body.chars().count(),
        "the fixture must distinguish bytes from characters",
    );

    block_on(
        SkillLoadTool::with_buffer(fixture.handle(), &LSK_BUFFER).call(LoadSkillArgs {
            name: "a skill nobody wrote".to_string(),
            ecosystem: None,
        }),
    )
    .unwrap_err();

    let query = |domain| {
        LSK_BUFFER
            .query(
                &LogFilter {
                    min_level: LogLevel::Debug,
                    domains: vec![domain],
                    ..LogFilter::default()
                },
                None,
                1000,
            )
            .unwrap()
            .records
    };
    let ai = query(Domain::Ai);
    let backend = query(Domain::Backend);

    assert_eq!(ai.len(), 2, "one INFO and one WARN (LSK-FR-19)");
    assert_eq!(ai, backend, "LSK-FR-19: both domains return the same records");

    assert_eq!(ai[0].level, LogLevel::Info);
    assert_eq!(ai[0].fields.get("tool").unwrap(), "load_skill");
    assert_eq!(
        ai[0].fields.get("bytes").unwrap(),
        &serde_json::json!(body.len()),
        "LSK-FR-19: the size of the answer",
    );
    assert_eq!(ai[1].level, LogLevel::Warn);
    assert_eq!(ai[1].fields.get("reason").unwrap(), "skill_not_found");
    assert_eq!(
        ai[1].fields.get("tool").unwrap(),
        "load_skill",
        "the refusal names the tool too, not only the success",
    );

    let serialised = serde_json::to_string(&ai).unwrap();
    for leaked in [
        "analyst",
        "a skill nobody wrote",
        ".claude/skills",
        "SKILL.md",
        // The ecosystem the successful call actually passed. Omitting it from
        // this list is how a leak of the one argument under test goes unseen.
        "claude",
        "Анализ",
        "Author requirement documents",
    ] {
        assert!(
            !serialised.contains(leaked),
            "LSK-FR-19: a record must not carry {leaked:?}",
        );
    }
}

#[test]
fn every_refusal_this_tool_can_produce_is_logged_under_its_own_reason() {
    // The success path and one refusal are covered above. A `reason` is what a
    // reader filters the Logs panel on, so each must be distinct and each must
    // actually reach a record — logging one code for two causes is invisible
    // in a suite that only ever drives one of them.
    let reasons = [
        (ToolRefusal::NoProjectOpen, "no_project_open"),
        (ToolRefusal::SkillNotFound, "skill_not_found"),
        (ToolRefusal::SkillUnreadable, "skill_unreadable"),
        (ToolRefusal::SkillNameNotUnique, "skill_name_not_unique"),
        (
            ToolRefusal::SkillAmbiguous(vec![Ecosystem::Claude]),
            "skill_ambiguous",
        ),
        (
            ToolRefusal::InvalidArguments(BLANK_NAME),
            "invalid_arguments",
        ),
    ];
    let mut seen = std::collections::HashSet::new();
    for (refusal, reason) in &reasons {
        assert_eq!(&refusal.reason(), reason);
        assert!(seen.insert(*reason), "{reason} is used for two causes");
        // A reason is a code the emit site chose, never a rendering of the
        // message — which would put the ecosystems into a log field.
        assert!(!refusal.reason().contains(' '));
    }

    // And the two this tool reaches through a real call carry the right one.
    let fixture = collision_project();
    LSK_BUFFER.clear();
    let _ = block_on(
        SkillLoadTool::with_buffer(fixture.handle(), &LSK_BUFFER).call(LoadSkillArgs {
            name: "review".to_string(),
            ecosystem: None,
        }),
    );
    let records = LSK_BUFFER
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![Domain::Ai],
                ..LogFilter::default()
            },
            None,
            1000,
        )
        .unwrap()
        .records;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].fields.get("reason").unwrap(), "skill_ambiguous");
    assert!(
        !serde_json::to_string(&records).unwrap().contains("claude"),
        "LSK-FR-19: not even the ecosystems the refusal message names",
    );
}

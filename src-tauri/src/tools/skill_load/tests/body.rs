//! TLC-FR-08, LSK-FR-13, LSK-FR-14: the text the tool returns.

use super::*;

// ---------------------------------------------------------------------------
// TLC-FR-08 — the output is unwrapped text (LSK-FR-12)
// ---------------------------------------------------------------------------

#[test]
fn the_result_carries_no_wrapper_field_or_sentence_of_the_tools_own() {
    let fixture = collision_project();
    let body = call(&fixture, "analyst", None).unwrap();

    // The tool adds nothing before or after what the author wrote.
    assert!(
        body.starts_with("\n# Analyst"),
        "no prefix of the tool's own: {body:?}",
    );
    assert!(
        body.ends_with("Second paragraph.\n"),
        "no suffix of the tool's own: {body:?}",
    );
    for wrapper in ["\"content\"", "\"instructions\"", "\"skills\"", "{\"", "name\":"] {
        assert!(!body.contains(wrapper), "no JSON wrapper: {wrapper:?}");
    }

    // The block a model actually receives is text; the group-wide assertion
    // lives in `tools/tests.rs::a_document_shaped_output_is_one_unwrapped_text_block`.
    let output = rig::tool::IntoToolOutput::into_tool_output(body.clone()).unwrap();
    assert_eq!(output.as_text(), Some(body.as_str()));
}

// ---------------------------------------------------------------------------
// LSK-FR-13 — the header is stripped, the rest is verbatim (LSK-FR-13)
// ---------------------------------------------------------------------------

#[test]
fn the_body_begins_after_the_closing_delimiter_and_is_returned_byte_for_byte() {
    // Asserted against `strip_frontmatter` directly as well as end-to-end,
    // because the byte-for-byte claim is about characters a fixture writer
    // cannot easily round-trip through a temp file.
    assert_eq!(
        strip_frontmatter("---\nname: a\ndescription: b\n---\n# Body\n\nA later --- rule stays.\n"),
        "# Body\n\nA later --- rule stays.\n",
        "LSK-FR-13: the body begins after the closing delimiter's line break, \
         a later `---` is body text, and the trailing newline survives",
    );

    // CRLF, as an editor on Windows writes it.
    assert_eq!(
        strip_frontmatter("---\r\nname: a\r\n---\r\nBody\r\n"),
        "Body\r\n",
    );
    // A leading BOM is tolerated exactly as the registry tolerates it, so a
    // skill admitted despite one is not then returned with its header intact.
    assert_eq!(
        strip_frontmatter("\u{feff}---\nname: a\n---\nBody\n"),
        "Body\n",
    );
    // An indented `---` is not a closing delimiter, by space or by tab.
    assert_eq!(strip_frontmatter("---\nname: a\n  ---\n---\nBody\n"), "Body\n");
    assert_eq!(strip_frontmatter("---\nname: a\n\t---\n---\nBody\n"), "Body\n");
    // No opening delimiter: returned whole, BOM and all.
    assert_eq!(strip_frontmatter("# Just a body\n"), "# Just a body\n");
    assert_eq!(strip_frontmatter("\u{feff}# Body\n"), "\u{feff}# Body\n");
    // The closing delimiter as the file's last line, with no newline after it.
    assert_eq!(strip_frontmatter("---\nname: a\n---"), "");
    // An unterminated block is not frontmatter (DSL-FR-07), so nothing is
    // stripped rather than the whole file being swallowed by a horizontal rule
    // on line one.
    let unterminated = "---\nname: a\n\n# Body\n";
    assert_eq!(strip_frontmatter(unterminated), unterminated);
    assert_eq!(strip_frontmatter(""), "");
    // LSK-FR-13: frontmatter and nothing after it leaves empty text.
    assert_eq!(strip_frontmatter("---\nname: a\n---\n"), "");
}

#[test]
fn multi_byte_text_survives_stripping_byte_for_byte() {
    // The function slices by byte offset, so "byte-for-byte" is only a
    // meaningful claim where bytes and characters differ — and a slice landing
    // mid-character would panic rather than return anything at all.
    let header = "---\nname: анализ\ndescription: Обзор спецификации — на русском.\n---\n";
    let body = "# Заголовок\n\nШаг первый — не пропускайте его. 日本語も。\n\nこれで終わり。\n";
    let file = format!("{header}{body}");
    assert!(
        file.len() > file.chars().count(),
        "the fixture must actually be multi-byte",
    );
    assert_eq!(strip_frontmatter(&file), body);

    // And end to end, through a real registry and a real read.
    let dir = tempfile::TempDir::new().unwrap();
    let skill = dir.path().join(".claude/skills/анализ");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(skill.join("SKILL.md"), &file).unwrap();
    let fixture = mounted(dir);

    assert_eq!(call(&fixture, "анализ", None).unwrap(), body);
}

#[test]
fn a_repeated_leading_bom_is_tolerated_exactly_as_the_registry_tolerates_it() {
    // `skills::parse_frontmatter` strips *every* leading BOM, so a file
    // carrying two is admitted to the registry. Stripping only one here would
    // leave the second in front of the `---`, the header would go unrecognised,
    // and the model would be handed the frontmatter LSK-FR-13 promised to
    // remove — with no test failing, because the call still succeeds.
    assert_eq!(
        strip_frontmatter("\u{feff}\u{feff}---\nname: a\n---\nBody\n"),
        "Body\n",
    );

    let dir = tempfile::TempDir::new().unwrap();
    let skill = dir.path().join(".claude/skills/bommed");
    std::fs::create_dir_all(&skill).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "\u{feff}\u{feff}---\nname: bommed\ndescription: Carries two byte-order marks.\n---\n# Body\n",
    )
    .unwrap();
    let fixture = mounted(dir);

    let loaded = call(&fixture, "bommed", None)
        .expect("the registry admits it, so this tool must too");
    assert_eq!(loaded, "# Body\n");
    assert!(
        !loaded.contains("description:"),
        "LSK-FR-13: the header is gone whatever preceded it",
    );
}

#[test]
fn a_skill_that_is_only_frontmatter_succeeds_with_empty_text() {
    // LSK-FR-13, TLC-FR-12: an answer, not a failure.
    let dir = tempfile::TempDir::new().unwrap();
    let only = dir.path().join(".claude/skills/hollow");
    std::fs::create_dir_all(&only).unwrap();
    std::fs::write(
        only.join("SKILL.md"),
        "---\nname: hollow\ndescription: Declares itself and says nothing more.\n---\n",
    )
    .unwrap();
    let fixture = mounted(dir);

    assert_eq!(
        call(&fixture, "hollow", None).expect("a hollow skill is a success, not a refusal"),
        "",
    );
}

// ---------------------------------------------------------------------------
// LSK-FR-14 — never truncated (LSK-FR-14)
// ---------------------------------------------------------------------------

#[test]
fn a_large_body_is_returned_whole_with_no_marker_of_truncation() {
    let dir = tempfile::TempDir::new().unwrap();
    // A megabyte of instructions: over any page a tool might have imposed, and
    // deliberately under the indexing ceiling so the registry still admits it —
    // the point is that this tool adds no ceiling of its own.
    let paragraph = "Follow this step carefully and do not skip it.\n";
    let body: String = paragraph.repeat(1_000_000 / paragraph.len() + 1);
    let huge = dir.path().join(".claude/skills/huge");
    std::fs::create_dir_all(&huge).unwrap();
    std::fs::write(
        huge.join("SKILL.md"),
        format!("---\nname: huge\ndescription: A very long procedure.\n---\n{body}"),
    )
    .unwrap();
    let fixture = mounted(dir);

    let loaded = call(&fixture, "huge", None).unwrap();
    assert!(loaded.len() > 1_000_000, "the whole body, {} bytes", loaded.len());
    assert_eq!(loaded, body, "LSK-FR-14: nothing elided anywhere in it");
    for marker in ["…", "[truncated", "<truncated"] {
        assert!(!loaded.contains(marker), "LSK-FR-14: no marker of truncation");
    }
}


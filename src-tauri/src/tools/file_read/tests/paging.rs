//! RFT-FR-06 … RFT-FR-11: the whole file, paging, bare text, and line terminators.

use super::*;

// ---------------------------------------------------------------------------
// RFT-FR-06 — the whole file, uncapped (RFT-FR-06)
// ---------------------------------------------------------------------------

#[test]
fn a_large_file_is_returned_whole() {
    let project = project();
    // Comfortably past anything that would tempt a ceiling.
    let big: String = (0..120_000).map(|i| format!("row {i}\n")).collect();
    write(&project.root, "big.txt", &big);

    let returned = project.at("big.txt").unwrap();
    assert_eq!(returned, big, "byte-for-byte, no truncation (RFT-FR-06)");
    assert!(returned.len() > 1_000_000, "the fixture is genuinely large");
}

// ---------------------------------------------------------------------------
// RFT-FR-07 / RFT-FR-08 — paging (RFT-FR-07, RFT-FR-08, RFT-FR-09)
// ---------------------------------------------------------------------------

#[test]
fn offset_and_limit_select_lines_and_clamp_out_of_range_values() {
    let project = project();
    let page = |offset: Option<i64>, limit: Option<i64>| {
        project
            .read(ReadFileArgs {
                path: "long.txt".to_string(),
                offset,
                limit,
            })
            .unwrap()
    };

    assert_eq!(page(Some(0), Some(1)), "line 0\n");
    assert_eq!(page(Some(40), Some(3)), "line 40\nline 41\nline 42\n");
    assert_eq!(
        page(Some(-5), Some(1)),
        "line 0\n",
        "a negative offset is clamped to the first line (RFT-FR-07)",
    );
    assert_eq!(
        page(Some(10), Some(0)),
        "line 10\n",
        "a limit below 1 is clamped to 1 (RFT-FR-08)",
    );
    assert_eq!(page(Some(10), Some(-3)), "line 10\n");
    assert_eq!(
        page(Some(0), None),
        hundred_lines(),
        "an absent limit runs to the end (RFT-FR-08)",
    );
}

#[test]
fn a_range_past_the_end_returns_what_exists_and_an_offset_past_it_returns_nothing() {
    let project = project();
    let page = |offset: i64, limit: Option<i64>| {
        project
            .read(ReadFileArgs {
                path: "long.txt".to_string(),
                offset: Some(offset),
                limit,
            })
            .unwrap()
    };

    assert_eq!(
        page(95, Some(50)),
        "line 95\nline 96\nline 97\nline 98\nline 99\n",
        "a limit reaching past the end returns the lines that exist (RFT-FR-08)",
    );
    assert_eq!(page(100, None), "", "at the end (RFT-FR-09)");
    assert_eq!(page(5000, None), "", "well past it (RFT-FR-09)");
    assert_eq!(page(100, Some(5)), "", "with a limit too");

    assert_eq!(
        project.at("empty.txt").unwrap(),
        "",
        "an empty file answers the same way (RFT-FR-09)",
    );
}

// ---------------------------------------------------------------------------
// RFT-FR-10 — the output is one unwrapped text block (RFT-FR-10)
// ---------------------------------------------------------------------------

#[test]
fn whole_and_paged_reads_are_both_bare_text() {
    use rig::tool::IntoToolOutput;

    let project = project();
    let tool = project.tool();

    let whole = block_on(tool.call(ReadFileArgs {
        path: "long.txt".to_string(),
        offset: None,
        limit: None,
    }))
    .unwrap();
    let paged = block_on(tool.call(ReadFileArgs {
        path: "long.txt".to_string(),
        offset: Some(40),
        limit: Some(3),
    }))
    .unwrap();

    for (label, text) in [("whole", &whole), ("paged", &paged)] {
        let output = text.clone().into_tool_output().unwrap();
        assert_eq!(
            output.as_text().map(str::to_string),
            Some(text.to_string()),
            "{label}: one text block holding the file's characters (RFT-FR-10)",
        );
        assert!(
            output.as_json().is_none(),
            "{label}: never a JSON object wrapping the document in a string field",
        );
    }

    // Nothing composed by the tool: no marker, no path, no line numbering
    // beyond what the file itself carries.
    assert_eq!(paged, "line 40\nline 41\nline 42\n");
    assert!(!paged.contains("long.txt"));
    assert!(!paged.contains('['), "no range marker (RFT-FR-10)");
    assert!(
        whole.contains(&paged),
        "a paged read differs only in which characters it carries",
    );
}

// ---------------------------------------------------------------------------
// RFT-FR-11 — line terminators survive (RFT-FR-11)
// ---------------------------------------------------------------------------

#[test]
fn a_range_carries_each_lines_own_terminator() {
    let project = project();

    let crlf = project
        .read(ReadFileArgs {
            path: "crlf.txt".to_string(),
            offset: Some(1),
            limit: Some(2),
        })
        .unwrap();
    assert_eq!(crlf, "two\r\nthree\r\n", "CRLF reads back as CRLF (RFT-FR-11)");

    let lf = project
        .read(ReadFileArgs {
            path: "long.txt".to_string(),
            offset: Some(1),
            limit: Some(2),
        })
        .unwrap();
    assert_eq!(lf, "line 1\nline 2\n");

    // The range is exactly the span of the file those lines occupy.
    let on_disk = std::fs::read_to_string(project.root.join("crlf.txt")).unwrap();
    let start = on_disk.find("two").unwrap();
    assert_eq!(&on_disk[start..start + crlf.len()], crlf);

    let tail = project
        .read(ReadFileArgs {
            path: "no-final-newline.txt".to_string(),
            offset: Some(2),
            limit: Some(1),
        })
        .unwrap();
    assert_eq!(
        tail, "gamma",
        "a final line with no terminator gets none (RFT-FR-11)",
    );
}

#[test]
fn multi_byte_text_is_cut_on_line_boundaries_not_byte_counts() {
    let project = project();
    let body = std::fs::read_to_string(project.root.join("multibyte.txt")).unwrap();
    assert_ne!(
        body.len(),
        body.chars().count(),
        "the fixture must be multi-byte or this proves nothing",
    );

    let middle = project
        .read(ReadFileArgs {
            path: "multibyte.txt".to_string(),
            offset: Some(1),
            limit: Some(1),
        })
        .unwrap();
    assert_eq!(middle, "β√ç\n");
}


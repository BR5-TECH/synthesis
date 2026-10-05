//! RFT-FR-15, RFT-FR-16: a binary file, and symbolic links.

use super::*;

// ---------------------------------------------------------------------------
// RFT-FR-15 — a binary file (RFT-FR-15)
// ---------------------------------------------------------------------------

static RFT_BINARY_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn a_file_that_is_not_text_refuses_and_leaks_none_of_its_bytes() {
    let project = project();
    let tool = FileReadTool::with_buffer(
        project.fixture.handle(),
        project.session.clone(),
        &RFT_BINARY_BUFFER,
    );

    let refusal = block_on(tool.call(ReadFileArgs {
        path: "blob.png".to_string(),
        offset: None,
        limit: None,
    }))
    .expect_err("a binary file is a refusal (RFT-FR-15)");

    assert_eq!(refusal, ToolRefusal::FileNotText);
    assert_eq!(refusal.kind(), ToolErrorKind::Other);
    assert!(refusal.retryable(), "another path reaches a text file");
    assert_eq!(refusal.to_string(), crate::tools::FILE_NOT_TEXT);

    // Not one byte of the file in the message or in what it logged. The path is
    // the model's own argument and is recorded (RFT-FR-20); the file's bytes are
    // the project's material and are not.
    let rendered = format!(
        "{}{}",
        refusal,
        rendered_records(&RFT_BINARY_BUFFER),
    );
    for fragment in [BINARY_MARKER, "CONFIDENTIAL"] {
        assert!(
            !rendered.contains(fragment),
            "{fragment:?} escaped into a message or a record (RFT-FR-15)",
        );
    }
    assert!(
        !refusal.to_string().contains("blob.png"),
        "the refusal message is the fixed sentence, carrying no argument",
    );
    // The scan above can only catch a field someone thought to name. Pinning the
    // key set catches one nobody did.
    let records = records_of(&RFT_BINARY_BUFFER, Domain::Ai);
    assert_eq!(records.len(), 1, "the one refusal and nothing else");
    assert_eq!(
        records,
        records_of(&RFT_BINARY_BUFFER, Domain::Backend),
        "the same record under both domains (TLC-FR-14)",
    );
    for record in records {
        assert_eq!(
            field_keys(&record),
            vec!["path", "reason", "retryable", "tool"],
            "a refusal record carries these four fields and nothing else (RFT-FR-20)",
        );
        assert_eq!(
            record.fields["path"],
            serde_json::json!("blob.png"),
            "the path the model asked for (RFT-FR-20)",
        );
    }
}

// ---------------------------------------------------------------------------
// RFT-FR-16 — symbolic links (RFT-FR-16)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_link_and_a_path_through_one_are_both_refused() {
    let project = project();
    std::os::unix::fs::symlink(project.root.join("src/a.ts"), project.root.join("link.ts"))
        .unwrap();
    std::os::unix::fs::symlink(project.root.join("src"), project.root.join("linkdir")).unwrap();

    for path in ["link.ts", "linkdir/a.ts"] {
        let refusal = project
            .at(path)
            .unwrap_err_or_else(|| panic!("{path} must refuse (RFT-FR-16)"));
        assert_eq!(refusal, ToolRefusal::PathThroughLink, "{path}");
        assert_eq!(refusal.kind(), ToolErrorKind::PermissionDenied);
        assert!(refusal.retryable());
        assert_eq!(refusal.to_string(), crate::tools::PATH_THROUGH_LINK);
    }

    // The real file behind the link is untouched and still readable by its own
    // path, which is what the refusal message tells the model to do.
    assert_eq!(project.at("src/a.ts").unwrap(), "export const a = 1;\n");
}

/// `Result::unwrap_err` with a message, for the loop above.
trait UnwrapErrOrElse<T, E> {
    fn unwrap_err_or_else(self, f: impl FnOnce() -> E) -> E;
}

impl<T: std::fmt::Debug, E> UnwrapErrOrElse<T, E> for Result<T, E> {
    fn unwrap_err_or_else(self, f: impl FnOnce() -> E) -> E {
        match self {
            Err(e) => e,
            Ok(_) => f(),
        }
    }
}


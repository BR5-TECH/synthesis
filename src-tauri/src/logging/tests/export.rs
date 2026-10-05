//! The export (LGC-FR-18).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// LGC-FR-18 — the export
// -----------------------------------------------------------------------

#[test]
fn an_export_writes_the_whole_match_set_as_jsonl() {
    let buffer = LogBuffer::new();
    let mut batch = Vec::new();
    for n in 0..500 {
        let level = if n % 4 == 0 {
            LogLevel::Error
        } else {
            LogLevel::Info
        };
        batch.push(input(level, &[Domain::Backend], &format!("m{n}")));
    }
    append(&buffer, batch);

    let errors_only = LogFilter {
        min_level: LogLevel::Error,
        ..Default::default()
    };
    let (text, count) = buffer.export_text(&errors_only).unwrap();
    assert_eq!(count, 125);
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 125, "one JSON object per line");

    // Ascending by sequence, and every line is a complete record.
    let mut previous = None;
    for line in &lines {
        let v: serde_json::Value = serde_json::from_str(line).unwrap();
        let seq = v.get("sequence").unwrap().as_u64().unwrap();
        if let Some(p) = previous {
            assert!(seq > p, "ascending by sequence");
        }
        previous = Some(seq);
        assert_eq!(v.get("level").unwrap(), "ERROR");
    }

    // The export is bounded by no limit — it is the whole match set, not a
    // page (LGC-FR-18).
    let paged = buffer.query(&errors_only, None, 40).unwrap();
    assert_eq!(paged.records.len(), 40);
    assert_eq!(paged.matched_total, 125);

    // A filter matching nothing writes an empty file and returns zero.
    let none = LogFilter {
        query: Some("no-such-record".to_string()),
        ..Default::default()
    };
    assert_eq!(buffer.export_text(&none).unwrap(), (String::new(), 0));
}

#[test]
fn an_unwritable_destination_is_the_typed_export_failed() {
    // LGC-FR-19. A path whose parent is a *file* cannot be created, and
    // `write_text_atomic` creates parents (FSA-FR-05), so this is the one
    // shape that fails without needing a permissions fixture.
    let dir = std::env::temp_dir().join(format!("synthesis-log-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let blocker = dir.join("blocker");
    std::fs::write(&blocker, b"i am a file").unwrap();

    // FSA-FR-28: the instance's own allowlist is deliberately somewhere
    // else entirely — an export lands where the *user* pointed it, which is
    // typically outside every root. That the writes below succeed against
    // `dir` while the instance is rooted at `roots_elsewhere` is the whole
    // point of the user-choice write.
    let roots_elsewhere = tempfile::TempDir::new().unwrap();
    let access = fs::FsAccess::builder()
        .allow_root(roots_elsewhere.path())
        .build()
        .unwrap();

    // `write_export` now surfaces the REASON so the command can log it;
    // flattening to `EXPORT_FAILED` is `export_logs`' job (LGC-FR-19).
    let err = write_export(
        &access,
        &blocker.join("nested").join("out.jsonl"),
        "{}\n",
    )
    .unwrap_err();
    assert!(
        !err.is_empty() && err != EXPORT_FAILED,
        "the write half reports why, so the command has something to log; got {err:?}"
    );

    // LGC-FR-19: whatever the reason, the *command* flattens it to the one
    // typed string the UI matches on. Asserted here because the reason and
    // the contract are now produced in different places.
    assert_eq!(
        write_export(&access, &blocker.join("nested").join("x.jsonl"), "{}\n")
            .map_err(|_| EXPORT_FAILED.to_string())
            .unwrap_err(),
        EXPORT_FAILED,
    );

    // FSA-FR-28: a destination that is not absolute never came from a
    // dialog, so it is refused before any write is attempted.
    let err = write_export(&access, Path::new("relative/out.jsonl"), "{}\n").unwrap_err();
    assert!(
        err.contains("absolute"),
        "a relative destination must be refused on its own terms; got {err:?}"
    );
    assert!(!Path::new("relative/out.jsonl").exists());

    // And the happy path really writes.
    let out = dir.join("out.jsonl");
    write_export(&access, &out, "{\"a\":1}\n").unwrap();
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "{\"a\":1}\n");
    // A pre-existing file is replaced, not appended to (FSA-FR-28).
    write_export(&access, &out, "{\"b\":2}\n").unwrap();
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "{\"b\":2}\n");

    let _ = std::fs::remove_dir_all(&dir);
}

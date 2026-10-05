//! RFT-FR-12, RFT-FR-13, RFT-FR-14: blank, absent, folder, and containment.

use super::*;

// ---------------------------------------------------------------------------
// RFT-FR-12 — a blank path refuses (RFT-FR-12)
// ---------------------------------------------------------------------------

#[test]
fn a_blank_path_refuses() {
    let project = project();
    for path in ["", "   ", "\t\n"] {
        let refusal = project.at(path).expect_err("a blank path is a refusal");
        assert_eq!(refusal, ToolRefusal::InvalidArguments(crate::tools::PATH_BLANK));
        assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
        assert!(refusal.retryable());
        assert_eq!(refusal.to_string(), crate::tools::PATH_BLANK);
    }
}

// ---------------------------------------------------------------------------
// RFT-FR-13 — absent and folder (RFT-FR-13, RFT-FR-14)
// ---------------------------------------------------------------------------

#[test]
fn an_absent_path_and_a_folder_refuse_differently() {
    let project = project();

    let missing = project.at("src/gone.ts").expect_err("nothing there");
    assert_eq!(missing, ToolRefusal::FileNotFound);
    assert_eq!(missing.kind(), ToolErrorKind::NotFound);
    assert!(missing.retryable(), "a different path reaches a real file");
    assert_eq!(missing.to_string(), crate::tools::FILE_NOT_FOUND);

    let folder = project.at("src").expect_err("a folder is not a file");
    assert_eq!(folder, ToolRefusal::PathIsFolder);
    assert_eq!(folder.kind(), ToolErrorKind::InvalidArgs);
    assert!(folder.retryable());
    assert_eq!(folder.to_string(), crate::tools::PATH_IS_FOLDER);
    assert!(
        !folder.to_string().contains("a.ts") && !folder.to_string().contains("nested"),
        "no listing of the folder's contents (RFT-FR-14)",
    );

    // The project root itself, spelled every way a model plausibly would. Each
    // is a folder, so each earns the folder refusal rather than being reported
    // as outside the project it *is*.
    for spelling in [".", "./", "src/..", "src/nested/../.."] {
        assert_eq!(
            project.at(spelling).expect_err("a folder"),
            ToolRefusal::PathIsFolder,
            "{spelling:?} names the project root, which is a folder (RFT-FR-14)",
        );
    }
}

// ---------------------------------------------------------------------------
// Containment: where a path ENDS versus where it PASSED
// ---------------------------------------------------------------------------

#[test]
fn a_path_that_leaves_the_root_is_refused_even_when_it_returns() {
    let project = project();
    let basename = project
        .root
        .file_name()
        .and_then(|n| n.to_str())
        .expect("a named directory");

    // RFT-FR-04 and FSA-FR-10 both say containment is judged by where a path
    // ends. The gate is stricter than that sentence: it refuses the moment a
    // `..` would step above the root, so a path that leaves and comes back is
    // refused rather than read. The stricter rule is the safe direction for a
    // path a model composed, so it is what this pins — the spec sentence is the
    // thing that is wrong, not this behaviour.
    let out_and_back = format!("../{basename}/src/a.ts");
    assert_eq!(
        project.at(&out_and_back).expect_err("refused"),
        ToolRefusal::PathOutsideProject,
        "a path that steps above the root is refused, however it ends",
    );
    // While the same file reached without leaving is read.
    assert_eq!(project.at("src/a.ts").unwrap(), "export const a = 1;\n");
}

#[test]
fn an_absolute_path_is_judged_by_where_it_lands() {
    let project = project();
    // A model that pasted the full path rather than the project-relative one it
    // was given still names a file inside the project, and gets it — which is
    // the forgiveness TLC-FR-07 asks for, since refusing would cost a turn to
    // teach it nothing about the file it correctly identified.
    let inside = project.root.join("src/a.ts");
    assert_eq!(
        project.at(&inside.to_string_lossy()).unwrap(),
        "export const a = 1;\n",
        "an absolute path inside the project is the same file",
    );

    // The bound is where it lands, not how it was spelled.
    for outside in ["/etc/passwd", "/", "//etc/passwd"] {
        assert_eq!(
            project.at(outside).expect_err("outside"),
            ToolRefusal::PathOutsideProject,
            "{outside:?}",
        );
    }
}

#[test]
fn a_hostile_path_neither_panics_nor_escapes() {
    let project = project();
    // None of these may panic, and none may return a file's text.
    for path in [
        "src/a\0.ts",
        &"../".repeat(4_000),
        &format!("{}/a.ts", "sub/".repeat(2_000)),
        "C:\\Windows\\win.ini",
        "\\\\server\\share\\x",
        "src/a.ts\u{202e}",
        "..",
        "../..",
    ] {
        let outcome = project.at(path);
        assert!(
            outcome.is_err(),
            "{path:?} must not resolve to a readable file",
        );
    }
}

// ---------------------------------------------------------------------------
// The session's instance is resolved per call (FSA-FR-29, RFT-FR-05)
// ---------------------------------------------------------------------------

#[test]
fn a_session_discarded_with_its_project_stops_reading() {
    let project = project();
    let tool = project.tool();
    let read = |t: &FileReadTool<tauri::test::MockRuntime>| {
        block_on(t.call(ReadFileArgs {
            path: "src/a.ts".to_string(),
            offset: None,
            limit: None,
        }))
    };

    assert!(read(&tool).is_ok(), "the session is live");

    // A worktree change discards every agent instance (FSA-FR-29). The tool
    // resolves its instance per call, so it stops reading at that moment rather
    // than keeping reach into a checkout the application has stopped showing.
    let elsewhere = TempDir::new().unwrap();
    project
        .fixture
        .app
        .state::<crate::fs::FsAccessState>()
        .install_for_worktree(&std::fs::canonicalize(elsewhere.path()).unwrap())
        .unwrap();

    assert_eq!(
        read(&tool).expect_err("the session is gone"),
        ToolRefusal::NoProjectOpen,
        "a discarded session reads nothing (FSA-FR-29)",
    );
}


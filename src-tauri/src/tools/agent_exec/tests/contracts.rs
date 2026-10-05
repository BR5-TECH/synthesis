//! EAC-FR-37, EAC-FR-43, EAC-FR-44 — finding the runtime, and the named result contract.

use super::*;

// ---------------------------------------------------------------------------
// Finding the runtime when `PATH` does not hold it (EAC-FR-37)
// ---------------------------------------------------------------------------

/// A directory holding one program of `name` that starts and exits `0`.
fn program_that_starts(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, "#!/bin/sh\nexit 0\n").expect("the stand-in");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("executable");
    }
    path
}

/// EAC-FR-17 (EAC-FR-37): the seam looks past `PATH`, takes the first location
/// that starts, and resolves once.
#[tokio::test]
async fn eac_ts49_the_seam_finds_a_runtime_path_does_not_hold() {
    let dir = tempfile::TempDir::new().expect("a directory");
    let found = program_that_starts(dir.path(), "docker");
    let absent = dir.path().join("nowhere/docker");

    // A program name no `PATH` holds, and a search list whose *second* entry is
    // the one that starts.
    let seam = HostDockerCli::with_program_searching(
        "synthesis-no-such-runtime-xyz",
        vec![
            absent.to_string_lossy().into_owned(),
            found.to_string_lossy().into_owned(),
        ],
    );
    assert_eq!(seam.resolved_program(), None, "nothing is resolved before an operation");

    // The stand-in exits `0` whatever it is asked, so the liveness query passes.
    seam.ensure_available().await.expect("the second entry answered");
    assert_eq!(
        seam.resolved_program().as_deref(),
        Some(found.to_string_lossy().as_ref()),
        "the first entry was tried and passed over",
    );

    // Resolved once: a later operation runs the same program rather than
    // searching again.
    seam.ensure_image("some/image:1").await.expect("the same program");
    assert_eq!(
        seam.resolved_program().as_deref(),
        Some(found.to_string_lossy().as_ref()),
    );
}

/// EAC-FR-37 (EAC-FR-37, EAC-FR-17): a machine with no runtime anywhere fails
/// exactly as it did before the search existed.
#[tokio::test]
async fn eac_ts49_a_search_that_finds_nothing_falls_back_to_the_plain_name() {
    let dir = tempfile::TempDir::new().expect("a directory");
    let seam = HostDockerCli::with_program_searching(
        "synthesis-no-such-runtime-xyz",
        vec![
            dir.path().join("a/docker").to_string_lossy().into_owned(),
            dir.path().join("b/docker").to_string_lossy().into_owned(),
        ],
    );
    assert!(matches!(
        seam.ensure_available().await,
        Err(RuntimeError::Unavailable),
    ));
    assert_eq!(
        seam.resolved_program().as_deref(),
        Some("synthesis-no-such-runtime-xyz"),
        "the plain name, so the failure is the one it always was",
    );
}

/// EAC-FR-17 (EAC-FR-37): a caller that names its own program searches nowhere.
///
/// The substitution the whole suite rests on: a stand-in that cannot be started
/// must fail as itself rather than quietly become whatever the machine has.
#[tokio::test]
async fn eac_ts49_a_named_program_is_never_traded_for_a_found_one() {
    let dir = tempfile::TempDir::new().expect("a directory");
    let real = program_that_starts(dir.path(), "docker");
    assert!(real.exists());

    let seam = HostDockerCli::with_program("synthesis-no-such-runtime-xyz");
    assert!(matches!(
        seam.ensure_available().await,
        Err(RuntimeError::Unavailable),
    ));
    assert_eq!(
        seam.resolved_program().as_deref(),
        Some("synthesis-no-such-runtime-xyz"),
    );
}

/// EAC-FR-37: `$HOME` in a search entry is the home the process was given, and
/// an entry needing one is passed over where there is none.
#[test]
fn eac_ts49_the_search_locations_resolve_the_home_directory() {
    let resolved = descriptor::docker_search_paths();
    assert!(
        !resolved.is_empty(),
        "a machine with no HOME still has absolute locations to look in",
    );
    for path in &resolved {
        assert!(!path.contains("$HOME"), "an unresolved home reached the list: {path}");
        assert!(path.starts_with('/'), "a relative location: {path}");
        assert!(path.ends_with("/docker"), "a location naming no program: {path}");
    }
    // The plain `PATH` case comes first in the seam rather than in this list.
    assert!(!resolved.iter().any(|path| path == "docker"));
}

// ---------------------------------------------------------------------------
// The result contract (EAC-FR-43, EAC-FR-44, CCP-FR-28, CDX-FR-14)
// ---------------------------------------------------------------------------

/// The `--json-schema` argument of a generated vector.
fn schema_argument_of(argv: &[String]) -> String {
    let at = argv
        .iter()
        .position(|a| a == "--json-schema")
        .expect("--json-schema");
    argv[at + 1].clone()
}


/// EAC-FR-43, EAC-FR-07 / CDX-FR-14 — the named document reaches every vendor in the task
/// document, and only one vendor's vector carries it.
#[test]
fn a_named_result_contract_reaches_every_vendors_task_document() {
    let mut schemas = Vec::new();
    let mut documents: Vec<Vec<u8>> = Vec::new();
    for vendor in ["claude_code", "codex"] {
        let harness = harness_for(vendor);
        let stdout = match vendor {
            "codex" => codex_stdout(&envelope_json("success")),
            _ => valid_claude_stdout(),
        };
        let runtime = RecordingRuntime::replying(&stdout);
        let mut reviewing = task("judge the work");
        reviewing.result_contract = Some(ResultContract::ReviewVerdict);
        run(&harness, runtime.clone(), reviewing).expect("runs");
        let recorded = runtime.only_run();
        let (sent, _, schema) = split_whole_task_document(&recorded.stdin);
        assert_eq!(
            sent.result_contract,
            Some(ResultContract::ReviewVerdict),
            "{vendor} was handed the caller's task unaltered",
        );
        let schema = schema.expect("every dispatched document carries the named schema");
        // CDX-FR-14: the vendor that enforces nothing carries the document in
        // the task alone, and its vector is the vector it always is.
        if vendor == "codex" {
            for resume in [None, Some("thread")] {
                assert_eq!(
                    descriptor::CODEX.vendor_args(Some("gpt-5"), Some("low"), "IGNORED", resume, None),
                    descriptor::CODEX.vendor_args(
                        Some("gpt-5"),
                        Some("low"),
                        "IGNORED",
                        resume,
                        Some(ResultContract::ReviewVerdict),
                    ),
                    "a named contract changes nothing about Codex's vector",
                );
            }
            assert!(
                !recorded.argv.iter().any(|a| a.contains("review_verdict")
                    || a.contains("--output-schema")
                    || a.contains("\"oneOf\"")),
                "no part of the document may reach Codex's vector",
            );
        }
        schemas.push(schema);
        documents.push(recorded.stdin.clone());
    }
    // The bytes rather than the parsed values: `serde_json` sorts a map's keys
    // on the way in, so two documents that differ in what they say could still
    // compare equal once parsed.
    assert_eq!(
        documents[0], documents[1],
        "both vendors are handed the same document, byte for byte",
    );
    assert_eq!(
        schemas[0],
        serde_json::from_str::<serde_json::Value>(protocol::REVIEW_VERDICT_SCHEMA).unwrap(),
    );
    // EAC-FR-43: the document is fixed text, so no part of the task is in it.
    let written = String::from_utf8(documents[0].clone()).expect("utf-8");
    let at = written.find("\"result_schema\"").expect("the schema is in the document");
    assert!(
        !written[at..].contains("judge the work"),
        "no part of the task may reach the schema",
    );

    // A task naming no contract states no shape for its answer, anywhere.
    let harness = harness_for("claude_code");
    let runtime = RecordingRuntime::replying(&valid_claude_stdout());
    run(&harness, runtime.clone(), task("go")).expect("runs");
    let recorded = runtime.only_run();
    let (_, _, none) = split_whole_task_document(&recorded.stdin);
    assert!(none.is_none(), "no contract named, no schema supplied");
    assert!(
        !schema_argument_of(&recorded.argv).contains("\"oneOf\""),
        "the free-form result position stands where no contract was named",
    );

    // EAC-FR-07: the name is one of a closed set, and the document is the
    // executor's rather than anything a caller may write. Both refusals are at
    // the decode, which is what "before launch" rests on: a task reaches the
    // executor as a typed value, and neither an identifier outside the set nor
    // a caller-supplied document can be built as one at all.
    let unknown = r#"{"protocol_version":1,"instruction":"go","result_contract":"whatever",
            "execution":{"timeout_ms":1000,"cancellation":"caller_controlled"}}"#;
    let error = serde_json::from_str::<AgentTaskRequest>(unknown)
        .expect_err("an identifier outside the set does not decode");
    let refusal = error.to_string();
    assert!(
        refusal.contains("whatever") && refusal.contains("review_verdict"),
        "the refusal names what was asked for and what the set holds: {refusal}",
    );
    let supplied = r#"{"protocol_version":1,"instruction":"go","result_schema":{"type":"object"},
            "execution":{"timeout_ms":1000,"cancellation":"caller_controlled"}}"#;
    let error = serde_json::from_str::<AgentTaskRequest>(supplied)
        .expect_err("a caller-supplied schema is an undefined field");
    assert!(
        error.to_string().contains("result_schema"),
        "the refusal names the field: {error}",
    );
}

/// A draft-07 evaluator for exactly the keywords the result documents of
/// EAC-FR-44 use, and no others.
///
/// It exists so the comparison against the application's own validation is a
/// comparison of **answers** rather than of vocabulary: a test that merely
/// asserted the schema mentions `description` would pass over a document that
/// required it in the wrong branch. An unknown keyword is a failed assertion
/// rather than a silent pass, so a document that grows one cannot quietly stop
/// being checked here.
/// Every keyword [`schema_admits`] reads. `$schema` is an annotation rather than
/// an assertion, and is the one entry read by being ignored.
const KNOWN_KEYWORDS: &[&str] = &[
    "$schema",
    "oneOf",
    "type",
    "enum",
    "properties",
    "additionalProperties",
    "maxProperties",
    "required",
    "items",
    "minItems",
    "maxItems",
    "minLength",
    "pattern",
];

/// Assert that a whole schema is written in keywords [`schema_admits`] reads,
/// walking it rather than following a value through it.
///
/// Walking is what makes the claim hold: an evaluator that checked only the
/// nodes a value reached would pass over a subtree no case happens to visit —
/// an `affected_files` nobody fills in, an `options` nobody offers — and a
/// keyword added there would silently stop being enforced.
fn assert_keywords_are_read(schema: &serde_json::Value) {
    let Some(object) = schema.as_object() else { return };
    for (keyword, sub) in object {
        assert!(
            KNOWN_KEYWORDS.contains(&keyword.as_str()),
            "this evaluator does not read the keyword `{keyword}`",
        );
        match keyword.as_str() {
            "oneOf" => sub
                .as_array()
                .expect("a list of branches")
                .iter()
                .for_each(assert_keywords_are_read),
            "properties" => sub
                .as_object()
                .expect("a map of fields")
                .values()
                .for_each(assert_keywords_are_read),
            "items" => assert_keywords_are_read(sub),
            // `additionalProperties` is a leaf here because it is `false`
            // wherever it appears. A schema placed under it would be walked by
            // neither this nor `schema_admits`.
            _ => {}
        }
    }
}

fn schema_admits(schema: &serde_json::Value, value: &serde_json::Value) -> bool {
    assert_keywords_are_read(schema);
    // Draft-07 would AND a sibling keyword with the branch count. No node of
    // either schema carries one beside `oneOf`, and neither would this reading
    // nor `assert_keywords_are_read` catch it if one appeared — a limit worth
    // knowing before a document grows such a node.
    if let Some(branches) = schema.get("oneOf").and_then(|b| b.as_array()) {
        return branches.iter().filter(|b| schema_admits(b, value)).count() == 1;
    }
    if let Some(declared) = schema.get("type") {
        let names: Vec<&str> = match declared {
            serde_json::Value::String(one) => vec![one.as_str()],
            serde_json::Value::Array(many) => many.iter().filter_map(|v| v.as_str()).collect(),
            _ => panic!("a `type` this evaluator does not read"),
        };
        let actual = match value {
            serde_json::Value::Null => "null",
            serde_json::Value::Bool(_) => "boolean",
            // A whole number answers to both names, as draft-07 has it.
            serde_json::Value::Number(n) if n.is_f64() => "number",
            serde_json::Value::Number(_) => "integer",
            serde_json::Value::String(_) => "string",
            serde_json::Value::Array(_) => "array",
            serde_json::Value::Object(_) => "object",
        };
        let matched = names.contains(&actual)
            || (actual == "integer" && names.contains(&"number"));
        if !matched {
            return false;
        }
    }
    if let Some(allowed) = schema.get("enum").and_then(|e| e.as_array()) {
        if !allowed.contains(value) {
            return false;
        }
    }
    match value {
        serde_json::Value::String(text) => {
            if let Some(min) = schema.get("minLength").and_then(|m| m.as_u64()) {
                if (text.chars().count() as u64) < min {
                    return false;
                }
            }
            if let Some(pattern) = schema.get("pattern").and_then(|p| p.as_str()) {
                assert_eq!(pattern, "\\S", "this evaluator reads one pattern");
                // Rust's own idea of whitespace rather than the regex engine's,
                // which is the application's idea too — both trim. The two
                // disagree on a few characters ECMA-262 counts and `char` does
                // not, so this comparison is between the document and the
                // application rather than between the document and the CLI.
                if !text.chars().any(|c| !c.is_whitespace()) {
                    return false;
                }
            }
        }
        serde_json::Value::Array(items) => {
            if let Some(min) = schema.get("minItems").and_then(|m| m.as_u64()) {
                if (items.len() as u64) < min {
                    return false;
                }
            }
            if let Some(max) = schema.get("maxItems").and_then(|m| m.as_u64()) {
                if (items.len() as u64) > max {
                    return false;
                }
            }
            if let Some(item) = schema.get("items") {
                if !items.iter().all(|entry| schema_admits(item, entry)) {
                    return false;
                }
            }
        }
        serde_json::Value::Object(map) => {
            if let Some(max) = schema.get("maxProperties").and_then(|m| m.as_u64()) {
                if (map.len() as u64) > max {
                    return false;
                }
            }
            let properties = schema.get("properties").and_then(|p| p.as_object());
            if schema.get("additionalProperties") == Some(&json!(false)) {
                let known = properties.map(|p| p.keys().collect::<Vec<_>>()).unwrap_or_default();
                if map.keys().any(|key| !known.contains(&key)) {
                    return false;
                }
            }
            if let Some(required) = schema.get("required").and_then(|r| r.as_array()) {
                if !required
                    .iter()
                    .filter_map(|name| name.as_str())
                    .all(|name| map.contains_key(name))
                {
                    return false;
                }
            }
            if let Some(properties) = properties {
                for (key, sub) in properties {
                    if let Some(held) = map.get(key) {
                        if !schema_admits(sub, held) {
                            return false;
                        }
                    }
                }
            }
        }
        _ => {}
    }
    true
}


/// EAC-FR-FNFV, EAC-FR-40 (EAC-FR-41): a masking that **cannot be established** refuses the
/// launch rather than establishing part of it.
///
/// The walk that finds every repository-metadata path inside the execution
/// directory is bounded, so a pathological tree cannot hold a launch open. What
/// it must not do at that bound is stop walking and let the container start:
/// a tree walked halfway is a tree whose deeper `.git` entries are unmasked,
/// and EAC-FR-41 has no launch-anyway path. The bound is named here rather than
/// fixed so the refusal is reachable without two hundred thousand directories.
#[test]
fn eac_ts55_a_masking_walk_that_cannot_finish_refuses_the_launch() {
    let workspace = tempfile::tempdir().expect("workspace");
    let root = workspace.path();
    // Three nested directories, the deepest holding the repository metadata a
    // truncated walk would never reach.
    let deep = root.join("one").join("two").join("three");
    std::fs::create_dir_all(deep.join(".git")).expect("a nested checkout");
    let fs = FsAccess::builder()
        .allow_root(root)
        .build()
        .expect("fs");

    // Walked whole, the nested `.git` is found and can be masked.
    let found = super::super::repository_metadata_paths_within(&fs, root, 1_000)
        .expect("a walk that finishes");
    assert!(
        found.iter().any(|(rel, _)| rel.ends_with(".git")),
        "the nested repository is what the mask exists for: {found:?}",
    );

    // Bounded below what the tree needs, it **refuses** rather than returning
    // the part it managed to see.
    let refused = super::super::repository_metadata_paths_within(&fs, root, 1);
    assert!(
        matches!(
            refused,
            Err(AgentExecutionError::RepositoryMaskingUnavailable(_))
        ),
        "a walk that could not finish established no masking: {refused:?}",
    );
    assert_eq!(
        refused.unwrap_err().kind(),
        "repository_masking_unavailable",
    );
}

/// FSA-FR-KVWD, GSU-FR-RJRF, EAC-FR-08 (EAC-FR-40): the containment check is against **every** user-scoped
/// root the application owns, read the way a launch reads them rather than the
/// way a harness overrides them.
///
/// The bundle a graduation run supplies stands under the **short** root, which
/// is where `FSA-filesystem-access.md` FSA-FR-KVWD puts it and where
/// `GSU-graduation-start.md` GSU-FR-RJRF puts the run store that holds it. A
/// check that knew only `app_data_dir()` refused every semantic-rebase turn
/// before its container was created, and every test of it passed because every
/// test supplied a root of its own.
#[test]
fn eac_ts53_the_containment_check_knows_every_application_root() {
    let executor = AgentCliExecutor::new(RecordingRuntime::replying(""));
    let roots = executor
        .data_roots()
        .expect("the application resolves its own roots");

    let short = crate::fs::short_data_dir().expect("the short root");
    let app = crate::fs::app_data_dir().expect("the application data root");
    assert!(
        roots.contains(&short),
        "the short root is one the bundle may stand under: {roots:?}",
    );
    assert!(
        roots.contains(&app),
        "the application data root is one the bundle may stand under: {roots:?}",
    );

    // A harness root replaces both, so what a test exercises is the root it
    // named rather than the machine's.
    let harness = tempfile::tempdir().expect("harness");
    let overridden = AgentCliExecutor::new(RecordingRuntime::replying(""))
        .with_session_state_root(harness.path());
    assert_eq!(
        overridden.data_roots().expect("roots"),
        vec![harness.path().to_path_buf()],
    );
}

/// FSA-FR-KVWD, GSU-FR-RJRF, EAC-FR-08 (EAC-FR-40): a bundle inside **either** root is accepted, one
/// outside both is refused, and a link that leaves one root without entering the
/// other is refused with it.
#[test]
fn eac_ts53_a_bundle_under_either_root_is_accepted_and_one_outside_both_is_not() {
    let outside = tempfile::tempdir().expect("outside");
    // The roots the **application** resolves, never a set this test built. A
    // check bound to a harness-supplied root is what hid the defect this
    // scenario exists for, and building one here would hide it again.
    let executor = AgentCliExecutor::new(RecordingRuntime::replying(""));
    let roots = executor.data_roots().expect("the application's own roots");
    let mut builder = crate::fs::FsAccess::builder().allow_root(outside.path());
    for root in &roots {
        builder = builder.allow_root(root.clone());
    }
    let fs = builder
        .build()
        .expect("an accessor over every root under test");

    let check = |host: &std::path::Path| {
        validate_supplementary_mount(
            &fs,
            &roots,
            host,
            &outside.path().join("execution").to_string_lossy(),
            None,
            &[],
        )
    };

    // One bundle under each real root, the graduation one in the layout
    // `GSU-graduation-start.md` GSU-FR-RJRF actually gives a run's store.
    let short = crate::fs::short_data_dir().expect("the short root");
    let under_short = short.join("g").join("g-eac-ts53").join("r");
    std::fs::create_dir_all(&under_short).expect("the run store");
    let app_root = crate::fs::app_data_dir().expect("the application data root");
    let under_app = app_root.join("eac-ts53");
    std::fs::create_dir_all(&under_app).expect("the application store");
    for (root, which) in [
        (under_short.clone(), "the short root"),
        (under_app.clone(), "the application data root"),
    ] {
        let bundle = rebase_bundle(&root, "report");
        assert!(
            check(&bundle).is_ok(),
            "a bundle under {which} is mounted: {:?}",
            check(&bundle),
        );
        std::fs::remove_dir_all(&bundle).ok();
    }
    std::fs::remove_dir_all(&under_short).ok();
    std::fs::remove_dir_all(&under_app).ok();

    let elsewhere = rebase_bundle(outside.path(), "elsewhere");
    assert!(
        matches!(
            check(&elsewhere),
            Err(AgentExecutionError::SupplementaryMountInvalid(_)),
        ),
        "a bundle outside every root is refused",
    );

    // A link inside one root that resolves out of both is refused on the
    // canonical form, which is what closes the escape.
    #[cfg(unix)]
    {
        let inside = roots.first().expect("a root").join("eac-ts53-link");
        std::fs::create_dir_all(&inside).expect("a directory inside a root");
        let linked = inside.join("linked-out");
        std::os::unix::fs::symlink(&elsewhere, &linked).expect("link");
        assert!(
            matches!(
                check(&linked),
                Err(AgentExecutionError::SupplementaryMountInvalid(_)),
            ),
            "a link out of every root is refused",
        );
        std::fs::remove_dir_all(&inside).ok();
    }
}

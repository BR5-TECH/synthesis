//! EAC-FR-02, EAC-FR-IRRD, EAC-FR-41, EAC-FR-FNFV — the two turn kinds of a merge
//! run: how they are named, how they resolve the author's selections, and that
//! neither is masked.

use super::super::TurnKind;
use super::*;

const ALL: [TurnKind; 5] = [
    TurnKind::Work,
    TurnKind::Review,
    TurnKind::SemanticRebase,
    TurnKind::MergeWork,
    TurnKind::MergeReview,
];

/// EAC-FR-IRRD, EAC-FR-02: the turn kinds are a closed set of five names, and the
/// two merge kinds are `merge_work` and `merge_review` in every record.
#[test]
fn the_turn_kinds_are_five_names_and_the_merge_kinds_are_named_as_such() {
    let names: Vec<&str> = ALL.iter().map(|kind| kind.as_str()).collect();
    assert_eq!(
        names,
        vec!["work", "review", "semantic_rebase", "merge_work", "merge_review"]
    );
    let mut distinct = names.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(distinct.len(), 5, "no two kinds share a name");
}

/// EAC-FR-IRRD: a merge kind has no selection of its own: `merge_work` reads the
/// author's `work` selections, `merge_review` the `review` ones, and the update's
/// turn keeps its own.
#[test]
fn a_merge_kind_reads_the_authors_work_and_review_selections() {
    assert_eq!(TurnKind::Work.selection_key(), "work");
    assert_eq!(TurnKind::Review.selection_key(), "review");
    assert_eq!(TurnKind::SemanticRebase.selection_key(), "semantic_rebase");
    assert_eq!(TurnKind::MergeWork.selection_key(), "work");
    assert_eq!(TurnKind::MergeReview.selection_key(), "review");
}

/// EAC-FR-IRRD, GXD-FR-MKTZ, GXD-FR-GBBC: the model and the reasoning effort a
/// merge launch carries are the two the author chose for `work` and for
/// `review`, read off the generated vector, and the launch differs from the
/// ordinary one in nothing else.
#[test]
fn a_merge_launch_carries_the_work_or_review_model_and_effort() {
    let harness = harness_for("claude_code");
    for (kind, model, effort) in [
        ("work", "opus", "low"),
        ("review", "haiku", "high"),
        ("semantic_rebase", "sonnet", "xhigh"),
    ] {
        agentic::set_model_impl(&harness.store, &harness.ai, "claude_code", Some(kind), Some(model))
            .expect("a model per kind");
        agentic::set_effort_impl(&harness.store, &harness.ai, "claude_code", Some(kind), Some(effort))
            .expect("an effort per kind");
    }
    let argv_of = |kind: TurnKind| -> Vec<String> {
        let runtime =
            RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
        run_as(&harness, runtime.clone(), None, None, kind).expect("runs");
        runtime.only_run().argv
    };
    let after = |argv: &[String], flag: &str| -> String {
        let at = argv
            .iter()
            .position(|a| a == flag)
            .unwrap_or_else(|| panic!("no {flag} in {argv:?}"));
        argv[at + 1].clone()
    };

    for (kind, model, effort) in [
        (TurnKind::MergeWork, "opus", "low"),
        (TurnKind::MergeReview, "haiku", "high"),
        (TurnKind::Work, "opus", "low"),
        (TurnKind::Review, "haiku", "high"),
        (TurnKind::SemanticRebase, "sonnet", "xhigh"),
    ] {
        let argv = argv_of(kind);
        assert_eq!(after(&argv, "--model"), model, "the model of {kind:?}");
        assert_eq!(after(&argv, "--effort"), effort, "the effort of {kind:?}");
    }

    // The same author selections give the same launch, whichever name the turn
    // carries: the session identifier alone is generated per launch.
    let image = test_image_reference("claude_code");
    let stripped = |argv: &[String]| -> Vec<String> {
        let tail = &argv[argv.iter().position(|a| *a == image).expect("the image") + 1..];
        let mut out = Vec::new();
        let mut skip = false;
        for arg in tail {
            if skip {
                skip = false;
                continue;
            }
            if arg == "--session-id" {
                skip = true;
                continue;
            }
            out.push(arg.clone());
        }
        out
    };
    assert_eq!(stripped(&argv_of(TurnKind::MergeWork)), stripped(&argv_of(TurnKind::Work)));
    assert_eq!(stripped(&argv_of(TurnKind::MergeReview)), stripped(&argv_of(TurnKind::Review)));
}

/// EAC-FR-IRRD: where the author chose a `work` model and no `review` model, a
/// merge review resolves what a review resolves, which is the default.
#[test]
fn a_merge_review_falls_back_as_a_review_does() {
    let harness = harness_for("claude_code");
    agentic::set_model_impl(&harness.store, &harness.ai, "claude_code", None, Some("opus"))
        .expect("the default model");
    agentic::set_model_impl(&harness.store, &harness.ai, "claude_code", Some("work"), Some("sonnet"))
        .expect("a work model");
    let model_of = |kind: TurnKind| -> String {
        let runtime =
            RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
        run_as(&harness, runtime.clone(), None, None, kind).expect("runs");
        let argv = runtime.only_run().argv;
        let at = argv.iter().position(|a| a == "--model").expect("a model");
        argv[at + 1].clone()
    };

    assert_eq!(model_of(TurnKind::MergeWork), "sonnet");
    assert_eq!(model_of(TurnKind::MergeReview), "opus");
    assert_eq!(model_of(TurnKind::Review), "opus");
}

/// EAC-FR-41, EAC-FR-FNFV, EAC-FR-IRRD, GRB-FR-LTVI: over a real linked worktree,
/// `semantic_rebase` is masked and gets no repository grant, while `merge_work`
/// and `merge_review` are not masked and get the read-only grant a `work` turn
/// and a `review` turn get. No merge turn carries the `/rebase` mount.
#[test]
fn a_merge_turn_is_not_masked_and_reaches_its_repository_read_only() {
    let harness = harness_for("claude_code");
    linked_worktree_at(&harness.sessions_root(), &harness.workspace());
    let argv_for = |kind: TurnKind| -> Vec<String> {
        let runtime =
            RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
        run_as(&harness, runtime.clone(), None, None, kind).expect("runs");
        runtime.only_run().argv
    };
    let masked = |argv: &[String]| {
        argv.iter().any(|a| {
            a.contains(&format!("target={}/.git", harness.workspace_target()))
                && a.contains("repository-mask")
        })
    };
    let granted = |argv: &[String]| argv.iter().any(|a| a.starts_with("GIT_OPTIONAL_LOCKS"));
    let rebase_mount = |argv: &[String]| argv.iter().any(|a| a.contains("target=/rebase"));

    for kind in [TurnKind::Work, TurnKind::Review, TurnKind::MergeWork, TurnKind::MergeReview] {
        let argv = argv_for(kind);
        assert!(granted(&argv), "{kind:?} reaches its repository: {argv:?}");
        assert!(!masked(&argv), "{kind:?} is not masked: {argv:?}");
        assert!(!rebase_mount(&argv), "{kind:?} carries no /rebase mount: {argv:?}");
    }
    let rebase = argv_for(TurnKind::SemanticRebase);
    assert!(masked(&rebase), "semantic_rebase is masked: {rebase:?}");
    assert!(!granted(&rebase), "and gets no grant: {rebase:?}");
}

/// EAC-FR-41, EAC-FR-FNFV: the other masking condition is the mount, so a request
/// that carries the rebase bundle is masked whatever its kind, a merge kind
/// included. No merge turn is given one, and this holds if one ever were.
#[test]
fn a_mount_masks_whatever_kind_carries_it() {
    let harness = harness_for("claude_code");
    linked_worktree_at(&harness.sessions_root(), &harness.workspace());
    let bundle = rebase_bundle(&harness.sessions_root(), "merge-kinds-bundle");
    let masked_with_mount = |kind: TurnKind| -> bool {
        let runtime =
            RecordingRuntime::replying(&stdout_for("claude_code", &envelope_json("success")));
        run_as(
            &harness,
            runtime.clone(),
            Some(SupplementaryMount::SemanticRebaseArtifact {
                host_path: bundle.clone(),
            }),
            None,
            kind,
        )
        .expect("runs");
        runtime
            .only_run()
            .argv
            .iter()
            .any(|a| a.contains("repository-mask"))
    };
    for kind in ALL {
        assert!(masked_with_mount(kind), "a mount masks {kind:?}");
    }
}

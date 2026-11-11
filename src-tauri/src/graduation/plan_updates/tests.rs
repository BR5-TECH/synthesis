//! The data and the scratch folder of a plan update.

use super::*;
use std::path::Path;

const CHANGE: &str = "add-logout";

fn write(root: &Path, rel: &str, text: &[u8]) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn scratch_file(root: &Path, file: &str, text: &str) {
    write(root, &format!(".synthesis-plan/openspec/changes/{CHANGE}/{file}"), text.as_bytes());
}

fn change_file(root: &Path, file: &str, text: &str) {
    write(root, &format!("openspec/changes/{CHANGE}/{file}"), text.as_bytes());
}

fn refused(root: &Path) -> PlanRefusalCause {
    match scratch::read(root, CHANGE) {
        Err(scratch::ScratchError::Refused(cause)) => cause,
        other => panic!("the plan update is refused, not {other:?}"),
    }
}

// OPU-FR-BNFL, OPU-FR-WINO: the scratch folder and every path in it are the
// scratch folder; a path that only starts with its name is not.
#[test]
fn a_path_is_in_the_scratch_folder_by_its_first_segment() {
    assert!(scratch::is_scratch_path(".synthesis-plan"));
    assert!(scratch::is_scratch_path(".synthesis-plan/"));
    assert!(scratch::is_scratch_path(".synthesis-plan/openspec/changes/a/design.md"));
    assert!(scratch::is_scratch_path("./.synthesis-plan/x"));
    assert!(!scratch::is_scratch_path(".synthesis-planner/x"));
    assert!(!scratch::is_scratch_path("src/.synthesis-plan/x"));
}

// OPU-FR-MBIY, OPU-FR-MCEZ: no scratch folder gives no hunk; a file equal to
// the working copy gives no hunk; a changed file gives line hunks; a file the
// working copy does not hold gives one new-file hunk; the hunks follow the
// order of their paths, and each path is change-relative.
#[test]
fn the_scratch_folder_becomes_hunks_in_the_order_of_their_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    assert!(scratch::read(root, CHANGE).unwrap().is_empty(), "no folder, no hunk");

    change_file(root, "proposal.md", "# Why\nA reason.\n");
    change_file(root, "tasks.md", "- [x] One\n- [ ] Two\n");
    scratch_file(root, "tasks.md", "- [x] One\n- [ ] Two, done another way\n");
    scratch_file(root, "proposal.md", "# Why\nA reason.\n");
    scratch_file(root, "design.md", "# Design\n");
    scratch_file(root, "specs/auth/spec.md", "## ADDED Requirements\n");

    let hunks = scratch::read(root, CHANGE).unwrap();
    let paths: Vec<&str> = hunks.iter().map(|h| h.path.as_str()).collect();
    assert_eq!(paths, vec!["design.md", "specs/auth/spec.md", "tasks.md"]);
    assert_eq!(hunks[0].hunk.kind, crate::text_hunks::anchors::HunkKind::Add);
    assert_eq!(hunks[0].hunk.after.as_deref(), Some("# Design\n"));
    assert_eq!(hunks[2].hunk.before.as_deref(), Some("- [ ] Two\n"));
    assert_eq!(hunks[2].hunk.after.as_deref(), Some("- [ ] Two, done another way\n"));
}

// OPU-FR-XLVF: a file outside the change folder, a file of another change, a
// path with an empty segment, and a symbolic link are each
// `plan_path_outside_change`.
#[test]
fn an_entry_outside_the_change_folder_is_refused() {
    for rel in [
        ".synthesis-plan/src/main.ts",
        ".synthesis-plan/openspec/changes/other/design.md",
        ".synthesis-plan/openspec/changes/add-logout",
    ] {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), rel, b"text\n");
        assert_eq!(refused(dir.path()), PlanRefusalCause::PlanPathOutsideChange, "{rel}");
    }
    #[cfg(unix)]
    {
        let dir = tempfile::tempdir().unwrap();
        change_file(dir.path(), "design.md", "# Design\n");
        let folder = dir.path().join(format!(".synthesis-plan/openspec/changes/{CHANGE}"));
        std::fs::create_dir_all(&folder).unwrap();
        std::os::unix::fs::symlink(dir.path().join("x"), folder.join("design.md")).unwrap();
        assert_eq!(refused(dir.path()), PlanRefusalCause::PlanPathOutsideChange, "a link");
    }
}

// OPU-FR-XLVF: a file that is not UTF-8 is `plan_file_not_text`; an empty
// file where the working copy holds that path not empty is
// `plan_deletes_file`; an empty file for a path that holds nothing gives no
// hunk.
#[test]
fn a_binary_file_and_a_deleting_file_are_refused_and_an_empty_new_file_proposes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        &format!(".synthesis-plan/openspec/changes/{CHANGE}/design.md"),
        &[0xff, 0xfe, 0],
    );
    assert_eq!(refused(dir.path()), PlanRefusalCause::PlanFileNotText);

    let dir = tempfile::tempdir().unwrap();
    change_file(dir.path(), "design.md", "# Design\n");
    scratch_file(dir.path(), "design.md", "");
    assert_eq!(refused(dir.path()), PlanRefusalCause::PlanDeletesFile);

    let dir = tempfile::tempdir().unwrap();
    scratch_file(dir.path(), "notes.md", "");
    assert!(scratch::read(dir.path(), CHANGE).unwrap().is_empty());
}

// OPU-FR-XLVF, OPU-FR-BNFL: a scratch folder that is a file and not a folder
// is no regular file below the change folder.
#[test]
fn a_scratch_root_that_is_a_file_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), ".synthesis-plan", b"text\n");
    assert_eq!(refused(dir.path()), PlanRefusalCause::PlanPathOutsideChange);
}

// OPU-FR-XLVF: the first cause that applies wins, in the order of the
// contract surface, whatever the order of the paths.
#[test]
fn the_first_cause_in_the_order_of_the_rules_is_the_refusal() {
    let dir = tempfile::tempdir().unwrap();
    change_file(dir.path(), "a.md", "text\n");
    scratch_file(dir.path(), "a.md", "");
    write(dir.path(), &format!(".synthesis-plan/openspec/changes/{CHANGE}/b.md"), &[0xff]);
    write(dir.path(), ".synthesis-plan/z.md", b"outside\n");
    assert_eq!(refused(dir.path()), PlanRefusalCause::PlanPathOutsideChange);
    std::fs::remove_file(dir.path().join(".synthesis-plan/z.md")).unwrap();
    assert_eq!(refused(dir.path()), PlanRefusalCause::PlanFileNotText);
}

// OPU-FR-WJKF, OPU-FR-WINO: the folder is removed whole, and the exclude
// line is written once.
#[test]
fn the_scratch_folder_is_removed_and_hidden_once() {
    let dir = tempfile::tempdir().unwrap();
    git2::Repository::init(dir.path()).unwrap();
    scratch_file(dir.path(), "design.md", "# Design\n");
    assert!(scratch::clear(dir.path()).unwrap());
    assert!(!dir.path().join(".synthesis-plan").exists());
    assert!(!scratch::clear(dir.path()).unwrap(), "nothing stood the second time");

    assert!(scratch::ensure_excluded(dir.path()).unwrap());
    assert!(!scratch::ensure_excluded(dir.path()).unwrap());
    let exclude = std::fs::read_to_string(dir.path().join(".git/info/exclude")).unwrap();
    assert_eq!(exclude.lines().filter(|l| *l == scratch::EXCLUDE_LINE).count(), 1);
}

fn question() -> GraduationEscalationQuestion {
    GraduationEscalationQuestion {
        position: 1,
        question: "Keep the old table?".into(),
        options: vec![GraduationProposedResponse {
            answer: "Keep it".into(),
            summary: "Keep".into(),
            description: "The old table stays.".into(),
        }],
    }
}

// OPU contract surface, GRD-FR-WKMD: a plan rest carries its plan fields in
// camel case, the cause of a refused plan update reads as its snake_case
// string, and both round-trip through the TOML run store.
#[test]
fn a_plan_rest_carries_its_fields_and_round_trips_through_toml() {
    let recording = PlanRecording {
        rest: REST_PLAN_UPDATE.into(),
        origin: ORIGIN_REVIEW.into(),
        reason: "The design is wrong.".into(),
        questions: vec![question()],
        findings: vec![ReviewFinding {
            kind: Some(FindingKind::Plan),
            severity: ReviewSeverity::Major,
            description: "The design names no table.".into(),
            affected_files: vec![],
            correction: "Name the table.".into(),
        }],
        thread_id: Some("t1".into()),
        set_id: Some("s1".into()),
        proposal_id: Some("p1".into()),
        ..PlanRecording::default()
    };
    let mut rest = GraduationRest::of_recording("t0".into(), &recording);
    rest.answers = Some(vec![crate::comments::QuestionAnswer {
        question_position: 1,
        option_position: Some(1),
        option_value: Some("Keep".into()),
        own_answer: None,
        note: None,
    }]);
    let value = serde_json::to_value(&rest).unwrap();
    assert_eq!(value["reason"], "plan_update");
    assert_eq!(value["origin"], "review");
    assert_eq!(value["threadId"], "t1");
    assert_eq!(value["setId"], "s1");
    assert_eq!(value["proposalId"], "p1");
    assert_eq!(value["planReason"], "The design is wrong.");
    assert_eq!(value["questions"][0]["options"][0]["summary"], "Keep");
    assert_eq!(value["answers"][0]["questionPosition"], 1);
    assert_eq!(value["findings"][0]["kind"], "plan");

    let mut run = GraduationRun::new_for_test("g1", "d1", "t0");
    run.rest = Some(rest.clone());
    let text = toml::to_string(&run).expect("the record serializes");
    let back: GraduationRun = toml::from_str(&text).expect("the record reads back");
    assert_eq!(back.rest, Some(rest));

    let invalid =
        GraduationRest::plan_update_invalid("t0".into(), PlanRefusalCause::PlanDeletesFile);
    assert_eq!(serde_json::to_value(&invalid).unwrap()["cause"], "plan_deletes_file");
    run.rest = Some(invalid.clone());
    let text = toml::to_string(&run).unwrap();
    assert_eq!(toml::from_str::<GraduationRun>(&text).unwrap().rest, Some(invalid));
    let checks: GraduationRest = serde_json::from_value(
        serde_json::json!({ "reason": "checks_not_passed", "at": "t", "cause": "checks_red" }),
    )
    .unwrap();
    assert_eq!(checks.cause, Some(GraduationRestCause::Checks(GraduationCheckCause::ChecksRed)));
}

fn plan_update_execution(questions: usize) -> crate::tools::agent_exec::AgentExecution {
    use crate::tools::agent_exec::protocol::*;
    crate::tools::agent_exec::AgentExecution {
        process_outcome: crate::tools::agent_exec::ProcessOutcome::Completed,
        exit_code: Some(0),
        response: Some(AgentResponseEnvelope {
            protocol_version: PROTOCOL_VERSION,
            outcome: AgentOutcome::EscalationRequired,
            summary: "plan".into(),
            result: None,
            failure: None,
            escalation: Some(AgentEscalation {
                kind: Some(EscalationKind::PlanUpdate),
                reason: "r".into(),
                questions: (0..questions)
                    .map(|i| AgentEscalationQuestion { question: format!("q{i}"), options: vec![] })
                    .collect(),
            }),
            session: Some(SessionRef { session_id: Some("s9".into()), continuation_token: None }),
            metadata: None,
        }),
        stdout: Default::default(),
        stderr: Default::default(),
        duration_ms: 0,
        resume_unavailable: false,
        session: None,
        refusal: None,
        container_removed: true,
    }
}

// OPU-FR-RFFH: a plan update from a turn that may not give one is refused as
// `escalation_shape`, keeping the session so the answer correction resumes
// it. An ordinary escalation is returned as it came.
#[test]
fn a_plan_update_of_a_turn_that_may_not_give_one_is_refused_as_escalation_shape() {
    use crate::tools::agent_exec::protocol::EnvelopeInvalid;
    let refused = refuse_plan_update(plan_update_execution(0));
    assert_eq!(
        refused.process_outcome,
        crate::tools::agent_exec::ProcessOutcome::InvalidStructuredOutput
    );
    assert_eq!(refused.refusal, Some(EnvelopeInvalid::EscalationShape));
    assert!(refused.response.is_none());
    assert_eq!(refused.session.and_then(|s| s.session_id).as_deref(), Some("s9"));

    let mut ordinary = plan_update_execution(1);
    if let Some(escalation) = ordinary.response.as_mut().and_then(|r| r.escalation.as_mut()) {
        escalation.kind = None;
    }
    let kept = refuse_plan_update(ordinary.clone());
    assert_eq!(kept.process_outcome, ordinary.process_outcome);
    assert!(kept.response.is_some());
}

/// A commit of `files` (path, text; `None` removes the path) on `parent`,
/// signed with an identity of its own, so the repository may hold none.
fn commit_files(
    repo: &git2::Repository,
    parent: Option<&git2::Commit<'_>>,
    files: &[(&str, Option<&str>)],
) -> git2::Oid {
    let mut update = git2::build::TreeUpdateBuilder::new();
    for (path, text) in files {
        match text {
            Some(text) => {
                let blob = repo.blob(text.as_bytes()).unwrap();
                update.upsert(*path, blob, git2::FileMode::Blob);
            }
            None => {
                update.remove(*path);
            }
        }
    }
    let empty = repo.find_tree(repo.treebuilder(None).unwrap().write().unwrap()).unwrap();
    let start = parent.map(|p| p.tree().unwrap()).unwrap_or(empty);
    let tree = repo.find_tree(update.create_updated(repo, &start).unwrap()).unwrap();
    let signature = git2::Signature::now("Test", "test@example.com").unwrap();
    let parents: Vec<&git2::Commit<'_>> = parent.into_iter().collect();
    repo.commit(None, &signature, &signature, "commit", &tree, &parents).unwrap()
}

fn text_at(repo: &git2::Repository, tree: &git2::Tree<'_>, path: &str) -> Option<String> {
    let entry = tree.get_path(Path::new(path)).ok()?;
    let blob = entry.to_object(repo).unwrap().peel_to_blob().unwrap();
    Some(String::from_utf8(blob.content().to_vec()).unwrap())
}

// OPU-FR-YNXV, OPU-FR-FSCV, OPU-FR-YTKN: where the base commit is not the
// parent of the close commit, the plan base is the base's tree with only the
// paths that the close commit changed set as it holds them: a changed file, a
// new file, and a removed file. The run's work under the close commit stays
// out. Its one parent is the base, and the private ref moves to each newer
// plan base. A repository with no Git identity still gets the commit.
#[test]
fn a_plan_base_takes_only_the_paths_of_the_close_commit() {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    {
        // Set at repository level, which overrides any global identity.
        let mut config = repo.config().unwrap().open_level(git2::ConfigLevel::Local).unwrap();
        config.set_str("user.name", "").unwrap();
        config.set_str("user.email", "").unwrap();
    }
    assert!(repo.signature().is_err(), "the repository holds no Git identity");
    let proposal = "openspec/changes/add-logout/proposal.md";
    let design = "openspec/changes/add-logout/design.md";
    let old = "openspec/changes/add-logout/old.md";
    let base = commit_files(
        &repo,
        None,
        &[(proposal, Some("v0\n")), (old, Some("old\n")), ("README.md", Some("seed\n"))],
    );
    let base_commit = repo.find_commit(base).unwrap();
    let work = commit_files(
        &repo,
        Some(&base_commit),
        &[("src/run.ts", Some("run\n")), ("README.md", Some("run edit\n"))],
    );
    let work_commit = repo.find_commit(work).unwrap();
    let close = commit_files(
        &repo,
        Some(&work_commit),
        &[(proposal, Some("v1\n")), (design, Some("# Design\n")), (old, None)],
    );

    let made = plan_base::make(dir.path(), &base.to_string(), &close.to_string(), "run-1").unwrap();
    let commit = repo.find_commit(git2::Oid::from_str(&made).unwrap()).unwrap();
    assert_eq!(commit.parent_count(), 1);
    assert_eq!(commit.parent_id(0).unwrap(), base);
    let tree = commit.tree().unwrap();
    assert_eq!(text_at(&repo, &tree, proposal).as_deref(), Some("v1\n"));
    assert_eq!(text_at(&repo, &tree, design).as_deref(), Some("# Design\n"));
    assert_eq!(text_at(&repo, &tree, old), None, "the removed file");
    assert_eq!(text_at(&repo, &tree, "README.md").as_deref(), Some("seed\n"), "no run work");
    assert_eq!(text_at(&repo, &tree, "src/run.ts"), None, "no run work");
    assert_eq!(repo.refname_to_id(&plan_base::ref_name("run-1")).unwrap().to_string(), made);

    let close_commit = repo.find_commit(close).unwrap();
    let second = commit_files(&repo, Some(&close_commit), &[(design, Some("# Design v2\n"))]);
    let newer = plan_base::make(dir.path(), &made, &second.to_string(), "run-1").unwrap();
    let newer_commit = repo.find_commit(git2::Oid::from_str(&newer).unwrap()).unwrap();
    assert_eq!(newer_commit.parent_id(0).unwrap().to_string(), made);
    let tree = newer_commit.tree().unwrap();
    assert_eq!(text_at(&repo, &tree, design).as_deref(), Some("# Design v2\n"));
    assert_eq!(text_at(&repo, &tree, proposal).as_deref(), Some("v1\n"));
    assert_eq!(repo.refname_to_id(&plan_base::ref_name("run-1")).unwrap().to_string(), newer);

    assert_eq!(plan_base::remove_ref(dir.path(), "run-1"), Ok(true));
    assert_eq!(plan_base::remove_ref(dir.path(), "run-1"), Ok(false));
}

// OPU-FR-YTKN: a close commit that the repository does not hold makes no
// plan base and moves no ref.
#[test]
fn a_plan_base_of_a_missing_close_commit_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let repo = git2::Repository::init(dir.path()).unwrap();
    let base = commit_files(&repo, None, &[("README.md", Some("seed\n"))]);
    let missing = "0123456789012345678901234567890123456789";
    assert!(plan_base::make(dir.path(), &base.to_string(), missing, "run-1").is_err());
    assert!(repo.find_reference(&plan_base::ref_name("run-1")).is_err());
}

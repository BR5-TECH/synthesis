//! The archive refusals of a stream merge (`OAR-openspec-archive.md`
//! OAR-FR-CCIN, OAR-FR-GJVM, OAR-FR-BEIK, OAR-FR-GTGQ, OAR-FR-QTBK,
//! OAR-FR-EUXZ, OAR-FR-KBDD, OAR-FR-OQVF; `WKS-work-streams.md` WKS-FR-YTJY),
//! with the OpenSpec CLI faked.

use super::archive_support::*;
use super::*;
use crate::openspec::ProcessOutput;
use crate::streams::StreamMergeResult;

type Snapshot = (String, String, Vec<String>);

/// Everything a refused merge must leave as it was: both tips, the stream's
/// files and status, and the whole store, with no notice in it.
fn snapshot(fx: &Fixture, stream: &crate::streams::WorkStream) -> Snapshot {
    snapshot_where(fx, stream, |_| true)
}

/// The same, with the merge notice left out.
///
/// OAR-FR-CCIN, OFP-FR-YKGQ: the merge notice of `OFP-openspec-failure-policy.md`
/// OFP-FR-ETEN is the one write of a refused check whose cause is of the class
/// `content`, so its discussion and its record are not part of this snapshot.
/// The notice tests read them.
fn snapshot_but_notice(fx: &Fixture, stream: &crate::streams::WorkStream) -> Snapshot {
    snapshot_where(fx, stream, |path| {
        !path.contains("/comments/") && !path.ends_with("merge-notice.toml")
    })
}

fn snapshot_where(
    fx: &Fixture,
    stream: &crate::streams::WorkStream,
    keep: impl Fn(&str) -> bool,
) -> Snapshot {
    let mut files = super::tree_under(&stream.worktree()).into_iter().collect::<Vec<_>>();
    files.extend(super::tree_under(records_store(fx).path()).into_iter().filter(|(p, _)| keep(p)));
    let files = files.into_iter().map(|(k, v)| format!("{k}={v}")).collect();
    (tip(fx, &stream.branch).to_string(), tip(fx, &stream.base_branch).to_string(), files)
}

/// The discussions of `change` on the stream.
fn discussions_of(
    fx: &Fixture,
    stream: &crate::streams::WorkStream,
    change: &str,
) -> Vec<crate::comments::Discussion> {
    let target = crate::comments::DiscussionTarget::Change {
        stream_id: stream.id.clone(),
        change: change.to_string(),
    };
    crate::comments::list_discussions_in(&records_store(fx), &target)
}

// OAR-FR-BEIK, OAR-FR-GTGQ, OAR-FR-CCIN, OAR-FR-HQAI, WKS-FR-YTJY,
// CEP-FR-KBZY, NCP-FR-CYQJ, CAR-FR-MKQC: a pending item refuses the merge
// with `change_has_pending: <change>: <kinds>`, the kinds in contract order,
// nothing is archived, committed, or written, and a WARN record tells it.
#[test]
fn a_pending_item_refuses_the_merge_and_writes_nothing() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    write_record(
        &fx,
        &stream,
        "add-logout",
        "action-requests/r1.toml",
        &action_request(&stream, "add-logout", "pending"),
    );
    write_record(
        &fx,
        &stream,
        "add-logout",
        "proposals/p1.toml",
        &edit_proposal(&stream, "add-logout", "pending", false),
    );
    write_record(
        &fx,
        &stream,
        "add-logout",
        "new-changes/n1.toml",
        &new_change_proposal(&stream, "add-logout", "pending", false),
    );
    let before = snapshot(&fx, &stream);

    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();

    assert_eq!(
        refusal,
        "change_has_pending: add-logout: edit_proposal,new_change_proposal,action_request"
    );
    assert_eq!(snapshot(&fx, &stream), before);
    assert!(runner.archive_calls().is_empty());
    assert_eq!(runner.inner.calls_of("instructions"), 0, "pending is checked first");
    // OAR-FR-HQAI: the refusal is told as a WARN record with its code.
    let record = one_record(&stream, "openspec archive check refused");
    assert_eq!(record.level, crate::logging::LogLevel::Warn);
    assert_eq!(record.fields["code"], "change_has_pending");
    assert_eq!(record.fields["change"], "add-logout");
    assert_eq!(record.fields["reason"], "edit_proposal,new_change_proposal,action_request");
}

// OAR-FR-BEIK: a proposal that owes its commit is pending, a decided one is
// not, and a request that is not `pending` is not.
#[test]
fn an_owed_commit_is_pending_and_a_decided_item_is_not() {
    let (fx, _, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    let kinds = |fx: &Fixture| {
        crate::openspec_archive::pending_kinds(&records_store(fx), &stream.id, "add-logout")
            .iter()
            .map(|k| k.as_str())
            .collect::<Vec<_>>()
    };
    write_record(
        &fx,
        &stream,
        "add-logout",
        "proposals/p1.toml",
        &edit_proposal(&stream, "add-logout", "accepted", false),
    );
    write_record(
        &fx,
        &stream,
        "add-logout",
        "action-requests/r1.toml",
        &action_request(&stream, "add-logout", "started"),
    );
    write_record(
        &fx,
        &stream,
        "add-logout",
        "new-changes/n1.toml",
        &new_change_proposal(&stream, "add-logout", "rejected", false),
    );
    assert!(kinds(&fx).is_empty());
    write_record(
        &fx,
        &stream,
        "add-logout",
        "proposals/p1.toml",
        &edit_proposal(&stream, "add-logout", "accepted", true),
    );
    write_record(
        &fx,
        &stream,
        "add-logout",
        "new-changes/n1.toml",
        &new_change_proposal(&stream, "add-logout", "accepted", true),
    );
    assert_eq!(kinds(&fx), ["edit_proposal", "new_change_proposal"]);
}

// OAR-FR-GJVM, OAR-FR-CCIN: each change is checked in archive order, its
// pending items before its archivability, and the first refusal wins; a
// later change that is not ready refuses after an earlier one passed, and
// nothing is archived.
#[test]
fn the_first_refusal_in_archive_order_refuses_the_merge() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["early", "late"]);
    completed_apply(&fx, &stream, "early", "2026-11-10T09:00:00Z");
    completed_apply(&fx, &stream, "late", "2026-11-10T10:00:00Z");
    write_record(
        &fx,
        &stream,
        "late",
        "proposals/p1.toml",
        &edit_proposal(&stream, "late", "pending", false),
    );
    runner.inner.set("instructions", Ok(crate::openspec::tests::ok(READY)));
    let before = snapshot_but_notice(&fx, &stream);

    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();
    assert_eq!(refusal, "change_not_archivable: early: tasks_incomplete");

    runner.inner.set("instructions", Ok(crate::openspec::tests::ok(ALL_DONE)));
    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();
    assert_eq!(refusal, "change_has_pending: late: edit_proposal");
    assert_eq!(snapshot_but_notice(&fx, &stream), before);
    assert!(runner.archive_calls().is_empty());
}

// OAR-FR-QTBK, OAR-FR-AEOK, OAR-FR-HQAI, OFP-FR-YKGQ, OFP-FR-ETEN,
// OFP-FR-GPCO: the reasons a change cannot be archived: the mode is off, the
// CLI is not usable, the progress read fails, the tasks are not all done. Each
// refuses before any archive with the reason's own code, changes no state, and
// is told as a WARN record with its code and reason. A reason of the class
// `environment` or `transient` gives no merge notice: no discussion and no
// notice record. Only the `content` reason `tasks_incomplete` gives one.
#[test]
fn a_change_that_cannot_be_archived_refuses_with_its_reason() {
    for (case, expected) in [
        ("mode", "openspec_mode_inactive"),
        ("cli", "cli_unavailable"),
        ("progress", "cli_failed"),
        ("tasks", "tasks_incomplete"),
    ] {
        let (fx, runner, probe) = archive_fixture(ALL_DONE);
        let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
        completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
        match case {
            "mode" => {
                // PSS-FR-IMDN: the author sets the mode to off.
                std::fs::create_dir_all(fx.root().join(".synthesis")).unwrap();
                std::fs::write(
                    fx.root().join(".synthesis/project.toml"),
                    "openspecMode = \"off\"\n",
                )
                .unwrap();
                super::change_runs::commit_all(&fx.root(), "Turn OpenSpec mode off");
            }
            "cli" => probe.executables.lock().unwrap().clear(),
            "progress" => runner.inner.set(
                "instructions",
                Ok(ProcessOutput { exit_status: Some(2), stdout: Vec::new(), stderr: Vec::new() }),
            ),
            _ => runner.inner.set("instructions", Ok(crate::openspec::tests::ok(READY))),
        }
        let content = case == "tasks";
        let before =
            if content { snapshot_but_notice(&fx, &stream) } else { snapshot(&fx, &stream) };
        let refusal = merge_refusal(&fx, &stream);
        assert_eq!(refusal, format!("change_not_archivable: add-logout: {expected}"), "{case}");
        let after =
            if content { snapshot_but_notice(&fx, &stream) } else { snapshot(&fx, &stream) };
        assert_eq!(after, before, "{case}");
        let notice = crate::openspec_archive::read_notice_record(
            &records_store(&fx),
            &stream.id,
            "add-logout",
        );
        assert_eq!(
            discussions_of(&fx, &stream, "add-logout").len(),
            usize::from(content),
            "{case}"
        );
        assert_eq!(notice.is_some(), content, "{case}: a notice record for content alone");
        assert_eq!(refusal.openspec().unwrap().thread_id.is_some(), content, "{case}");
        assert!(runner.archive_calls().is_empty(), "{case}");
        // OAR-FR-HQAI: the last refusal record of the stream is this one.
        let record = stream_records(&stream)
            .into_iter()
            .rfind(|r| r.message == "openspec archive check refused")
            .expect("a refusal record");
        assert_eq!(record.level, crate::logging::LogLevel::Warn, "{case}");
        assert_eq!(record.fields["code"], "change_not_archivable", "{case}");
        assert_eq!(record.fields["reason"], expected, "{case}");
    }
}

// OAR-FR-EUXZ, OAR-FR-KBDD, OAR-FR-HQAI, OAR-FR-NEEI, WKS-FR-YTJY: an
// archive that the CLI refuses refuses the merge with the code of its first
// status entry, and every path it wrote below the three folders is restored,
// so the stream is clean and its tip has not moved; the records hold the code
// and not the message or the fix.
#[test]
fn a_failed_archive_is_restored_and_refuses_with_its_code() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    let stdout = r#"{ "archive": null, "status": [ { "code": "spec_merge_conflict",
        "message": "Two deltas change Login", "fix": "Edit the delta by hand" } ] }"#;
    runner.set_archive("add-logout", Archive::FailWriting { status: 1, stdout: stdout.into() });
    let before = snapshot(&fx, &stream);

    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();

    assert_eq!(refusal, "archive_failed: add-logout: spec_merge_conflict");
    assert!(dirty(&stream.worktree()).is_empty(), "{:?}", dirty(&stream.worktree()));
    assert_eq!(snapshot(&fx, &stream), before);
    // OAR-FR-HQAI, OAR-FR-NEEI: the failure is told as a WARN record with the
    // status code, and no record holds the message or the fix of the entry.
    let record = one_record(&stream, "openspec archive failed");
    assert_eq!(record.level, crate::logging::LogLevel::Warn);
    assert_eq!(record.fields["code"], "spec_merge_conflict");
    let filter = crate::logging::LogFilter {
        min_level: crate::logging::LogLevel::Debug,
        ..crate::logging::LogFilter::default()
    };
    let all = crate::logging::BUFFER.query(&filter, None, 5000).unwrap().records;
    assert!(!all.is_empty());
    for record in all {
        let text = serde_json::to_string(&record).unwrap();
        assert!(!text.contains("Two deltas") && !text.contains("by hand"), "{text}");
    }
}

// OAR-FR-EUXZ: output that does not decode gives the code of the read, and an
// archive that wrote nothing has no commit to make, which is `commit_failed`.
#[test]
fn an_unreadable_archive_and_a_failed_commit_give_their_codes() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    runner.set_archive(
        "add-logout",
        Archive::FailWriting { status: 1, stdout: "Error: something".into() },
    );
    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();
    assert_eq!(refusal, "archive_failed: add-logout: cli_failed");
    assert!(dirty(&stream.worktree()).is_empty());

    runner.set_archive("add-logout", Archive::SucceedWritingNothing);
    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();
    assert_eq!(refusal, "archive_failed: add-logout: commit_failed");
}

// OAR-FR-OQVF, OAR-FR-SOFQ, OAR-FR-KGDP: an archive commit stays when a later
// archive of the same merge fails; the refusal carries its own code alone;
// the next merge carries the commit and does not archive that change again.
#[test]
fn an_earlier_archive_commit_stays_when_a_later_one_fails() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["early", "late"]);
    completed_apply(&fx, &stream, "early", "2026-11-10T09:00:00Z");
    completed_apply(&fx, &stream, "late", "2026-11-10T10:00:00Z");
    let stdout = r#"{ "archive": null, "status": [ { "code": "late_refused" } ] }"#;
    runner.set_archive("late", Archive::FailWriting { status: 1, stdout: stdout.into() });
    let stream_before = tip(&fx, &stream.branch);
    let base_before = tip(&fx, &stream.base_branch);

    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();

    assert_eq!(refusal, "archive_failed: late: late_refused");
    assert_eq!(
        messages_since(&fx, &stream.branch, stream_before),
        ["Archive OpenSpec change early"]
    );
    assert_eq!(tip(&fx, &stream.base_branch), base_before, "the base branch did not move");
    assert!(dirty(&stream.worktree()).is_empty());

    runner.set_archive("late", Archive::Succeed);
    let result = merge(&fx, &stream, commit_publication()).expect("merged");
    let StreamMergeResult::Merged { archived, .. } = result else { panic!("{result:?}") };
    let names: Vec<&str> = archived.iter().map(|a| a.change.as_str()).collect();
    assert_eq!(names, ["late"]);
    let archives: Vec<String> = runner.archive_calls().into_iter().map(|a| a[1].clone()).collect();
    assert_eq!(archives, ["early", "late", "late"]);
    assert!(fx.root().join(format!("openspec/changes/archive/{DATE}-early")).is_dir());
}

// OAR-FR-SEQU, WKS-FR-PXYH, WKS-FR-UZHT: the archive step runs after the clean
// check of both sides, so a dirty stream and a dirty base refuse the merge
// with their own codes, with a change ready to archive, before any CLI
// process and with no tip moved.
#[test]
fn a_dirty_side_refuses_before_the_archive_step() {
    for (side, expected) in [("stream", "stream_dirty"), ("base", "base_dirty")] {
        let (fx, runner, _) = archive_fixture(ALL_DONE);
        let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
        completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
        let worktree = if side == "stream" { stream.worktree() } else { fx.root() };
        std::fs::write(worktree.join("README.md"), "uncommitted\n").unwrap();
        let tips = (tip(&fx, &stream.branch), tip(&fx, &stream.base_branch));

        let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();

        assert_eq!(refusal, format!("{expected}: README.md"), "{side}");
        assert_eq!(runner.inner.calls_of("instructions"), 0, "{side}");
        assert!(runner.archive_calls().is_empty(), "{side}");
        assert_eq!((tip(&fx, &stream.branch), tip(&fx, &stream.base_branch)), tips, "{side}");
    }
}

// OAR-FR-GTGQ, OAR-FR-BEIK: a plan proposal that the plan update of an apply
// run recorded is a change edit proposal, so while it is pending the merge is
// refused with `edit_proposal`.
#[test]
fn a_recorded_plan_proposal_is_a_pending_edit_proposal() {
    let (fx, runner, _) = archive_fixture(ALL_DONE);
    let stream = stream_with_changes(&fx, "editor", &["add-logout"]);
    let run = completed_apply(&fx, &stream, "add-logout", "2026-11-10T10:00:00Z");
    let store = records_store(&fx);
    let target = crate::comments::DiscussionTarget::Change {
        stream_id: stream.id.clone(),
        change: "add-logout".to_string(),
    };
    let thread = crate::comments::open_discussion_unchecked(
        &store,
        &target,
        None,
        "The plan changed.".into(),
        Vec::new(),
        &crate::comments::Participant::synthesis(),
        "2026-11-10T11:00:00Z",
    )
    .unwrap();
    let hunk = crate::change_proposals::PathedHunk {
        path: "proposal.md".into(),
        hunk: crate::graduation::driver::hunks_of(Some("# Why add-logout\n"), "# Why not\n")
            .remove(0),
    };
    crate::change_proposals::record_plan_proposal(
        &fx.app,
        &store,
        &crate::change_proposals::AppSites { app: &fx.app },
        crate::change_proposals::NewChangeProposal {
            stream_id: &stream.id,
            change: "add-logout",
            hunks: std::slice::from_ref(&hunk),
            rationale: "The plan changed.",
            agent: &crate::comments::Participant::synthesis(),
            thread_id: &thread.id,
        },
        &run.id,
    )
    .expect("a pending plan proposal");

    let refusal = merge(&fx, &stream, commit_publication()).unwrap_err();

    assert_eq!(refusal, "change_has_pending: add-logout: edit_proposal");
    assert!(runner.archive_calls().is_empty());
}

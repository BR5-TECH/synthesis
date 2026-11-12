//! What an OpenSpec scenario asserts: the processes of the fake CLI, the
//! recorded waits, the image probe, the plan rest, the change edit proposal of
//! a rework run, the archive commits of a merge, and the merge notice
//! (GTE-FR-QPMA through GTE-FR-YMEC).
//!
//! Every read comes from the run store, the repository, the machine store, or
//! the records of the fake CLI and the wait seam, never from a value the loop
//! returned.

use std::sync::Arc;

use tauri::Manager;

use super::dispatch_assert::DispatchAssert;
use super::openspec_arrange::change_file;
use super::openspec_cli::ARCHIVE_DATE;
use super::outcome::{sorted, Outcome};
use crate::graduation::GraduationRunState;
use crate::streams::StreamMergeResult;

impl DispatchAssert {
    /// GRL-FR-CDNI: the validation of the change this review turn carries.
    pub(crate) fn openspec_validation(self, valid: bool) -> Self {
        assert_eq!(
            self.seen.input.get("openspec_validation").and_then(|v| v.get("valid")),
            Some(&serde_json::json!(valid)),
            "[{}] dispatch {} carries the change as valid: {valid}",
            self.scenario,
            self.index
        );
        self
    }

    /// OCR-FR-XKTV: the task progress this review turn carries, as `(state,
    /// complete, total)`.
    pub(crate) fn apply_progress(self, state: &str, complete: u64, total: u64) -> Self {
        let progress = self.seen.input.get("openspec_apply_progress").cloned().unwrap_or_default();
        assert_eq!(
            (progress["state"].as_str(), progress["complete"].as_u64(), progress["total"].as_u64()),
            (Some(state), Some(complete), Some(total)),
            "[{}] dispatch {} carries the task progress",
            self.scenario,
            self.index
        );
        self
    }
}

impl Outcome {
    fn store(&self) -> crate::fs::RootFs {
        self.fx
            .app
            .state::<crate::project::ProjectState>()
            .require_store()
            .expect("the machine store")
    }

    // -- the fake CLI and the wait seam (GTE-FR-QPMA, GTE-FR-ZDCH) -------------

    /// GTE-FR-QPMA: how many processes of `subcommand` the fake CLI ran over
    /// the whole scenario.
    pub(crate) fn expect_cli_processes(self, subcommand: &str, count: usize) -> Self {
        assert_eq!(
            self.cli().calls_of(subcommand).len(),
            count,
            "[{}] the processes of `openspec {subcommand}`",
            self.name
        );
        self
    }

    /// GTE-FR-QPMA: a process of `subcommand` ran in the execution directory
    /// of dispatch `index`, so it read what that turn's working copy holds.
    pub(crate) fn expect_cli_read_the_directory_of(self, subcommand: &str, index: usize) -> Self {
        let directory = self.dispatches()[index].directory.clone();
        let ran: Vec<_> = self.cli().calls_of(subcommand).into_iter().map(|c| c.cwd).collect();
        assert!(
            ran.iter().any(|cwd| canonical(cwd) == canonical(&directory)),
            "[{}] `openspec {subcommand}` ran in the directory of dispatch {index}, \
             {directory:?}; it ran in {ran:?}",
            self.name
        );
        self
    }

    /// GTE-FR-ZDCH, OFP-FR-MJZU: the delays the wait seam was asked for, in
    /// order. No real wait took place.
    pub(crate) fn expect_waits(self, delays: &[u64]) -> Self {
        let held = self.fx.app.state::<Arc<crate::openspec::RecordedWait>>().delays();
        assert_eq!(held, delays, "[{}] the recorded waits", self.name);
        self
    }

    // -- the blocker (OFP-FR-TBHX) ----------------------------------------------

    /// OFP-FR-TBHX: the cause the blocker holds.
    pub(crate) fn expect_blocker_cause(self, cause: &str) -> Self {
        let held = self.run.blocker.as_ref().and_then(|b| b.cause.clone());
        assert_eq!(held.as_deref(), Some(cause), "[{}] the blocker's cause", self.name);
        self
    }

    /// OFP-FR-XBNJ: the detail the blocker holds.
    pub(crate) fn expect_blocker_detail(self, detail: &str) -> Self {
        let held = self.run.blocker.as_ref().and_then(|b| b.detail.clone());
        assert_eq!(held.as_deref(), Some(detail), "[{}] the blocker's detail", self.name);
        self
    }

    /// OFP-FR-TBHX: a diagnostic of the blocker holds `fragment`.
    pub(crate) fn expect_blocker_diagnostic(self, fragment: &str) -> Self {
        let messages: Vec<String> = self
            .run
            .blocker
            .as_ref()
            .map(|b| b.diagnostics.iter().map(|d| d.message.clone()).collect())
            .unwrap_or_default();
        assert!(
            messages.iter().any(|message| message.contains(fragment)),
            "[{}] a diagnostic of the blocker holds {fragment:?}: {messages:?}",
            self.name
        );
        self
    }

    // -- the image probe (OFP-FR-BZDP) -----------------------------------------

    /// OFP-FR-BZDP: how many image probes the last act made.
    pub(crate) fn expect_image_probes(self, count: usize) -> Self {
        assert_eq!(self.dispatch.probes_taken(), count, "[{}] the image probes", self.name);
        self
    }

    /// OFP-FR-WACS: the version of the image's CLI the checkpoint holds.
    pub(crate) fn expect_image_cli(self, version: Option<&str>) -> Self {
        let held = self.run.checkpoint.openspec_image_cli.as_ref().map(|cli| cli.version.clone());
        assert_eq!(held.as_deref(), version, "[{}] the image CLI of the checkpoint", self.name);
        self
    }

    // -- the propose run (GRD-FR-SYHE) ------------------------------------------

    /// GRD-FR-SYHE: the draft reports `graduated`, and the run its link
    /// names carries the change.
    pub(crate) fn expect_draft_graduated_into(self, change: &str) -> Self {
        let queue = crate::graduation::queue::project_queue(&self.fx.app).expect("the queue");
        let link = crate::graduation::queue::draft_graduation(&queue, &self.run.input.draft_id)
            .unwrap_or_else(|| panic!("[{}] the draft has a graduation link", self.name));
        assert!(link.graduated, "[{}] the draft reports graduated", self.name);
        let read = crate::graduation::get_draft_graduation(
            self.fx.app.clone(),
            self.run.input.draft_id.clone(),
        )
        .expect("the link is readable")
        .unwrap_or_else(|| panic!("[{}] the draft names a run", self.name));
        assert_eq!(
            read.openspec.as_ref().map(|data| (data.phase.as_str(), data.change.as_str())),
            Some(("propose", change)),
            "[{}] the run of the draft carries its change",
            self.name
        );
        self
    }

    // -- the rework run (OCR-FR-ZNJT, OCR-FR-WBZX) ------------------------------

    /// OCR-FR-QUSP: the work turn stood in the run's rework worktree, and no
    /// turn stood in the stream's working copy.
    pub(crate) fn expect_turns_stood_off_the_stream(self) -> Self {
        let base = crate::graduation::store_base(&self.fx.app).expect("the run store");
        let rework = canonical(&base.rework_worktree(&self.run.id));
        let seen = self.dispatches();
        assert!(!seen.is_empty(), "[{}] no turn was dispatched", self.name);
        for (index, dispatch) in seen.iter().enumerate() {
            let directory = canonical(&dispatch.directory);
            assert_ne!(
                directory,
                canonical(&self.worktree),
                "[{}] dispatch {index} stood in the stream's working copy",
                self.name
            );
            if dispatch.part == "work" {
                assert_eq!(
                    directory, rework,
                    "[{}] work dispatch {index} stood in the rework worktree",
                    self.name
                );
            }
        }
        self
    }

    /// OCR-FR-XKTV: every work turn stood in the stream's working copy.
    pub(crate) fn expect_work_turns_stood_in_the_stream(self) -> Self {
        let work: Vec<_> = self.dispatches().into_iter().filter(|d| d.part == "work").collect();
        assert!(!work.is_empty(), "[{}] no work turn was dispatched", self.name);
        for dispatch in work {
            assert_eq!(
                canonical(&dispatch.directory),
                canonical(&self.worktree),
                "[{}] a work turn stood in the stream's working copy",
                self.name
            );
        }
        self
    }

    /// OCR-FR-TQCW, GRL-FR-YKRI: every review turn stood in a review checkout
    /// of its own under the run's home, at the seed commit, holding the
    /// rework worktree's `path` with `content` as uncommitted work.
    pub(crate) fn expect_reviews_stood_in_checkouts_of_the_rework(
        self,
        path: &str,
        content: &str,
    ) -> Self {
        let base = crate::graduation::store_base(&self.fx.app).expect("the run store");
        let home = base.home(&self.run.id);
        let seed = self.run.base_commit.clone().expect("the seed commit");
        let reviews: Vec<(usize, super::super::Seen)> = self
            .dispatches()
            .into_iter()
            .enumerate()
            .filter(|(_, seen)| seen.part == "review")
            .collect();
        assert!(!reviews.is_empty(), "[{}] no review turn was dispatched", self.name);
        for (index, seen) in reviews {
            DispatchAssert { scenario: self.name, index, seen: seen.clone() }
                .directory_is_review_checkout_under(&home);
            assert_eq!(
                seen.head.as_deref(),
                Some(seed.as_str()),
                "[{}] review dispatch {index} stands at the seed commit",
                self.name
            );
            assert_eq!(
                seen.tree.get(path).map(String::as_str),
                Some(content),
                "[{}] review dispatch {index} holds the rework's {path}",
                self.name
            );
            assert!(
                seen.diff_paths.iter().any(|p| p == path),
                "[{}] review dispatch {index} holds {path} as uncommitted work",
                self.name
            );
        }
        self
    }

    /// OCR-FR-WBZX: the hunks of `file` in the one change edit proposal of
    /// `change` replace `before` with `after`, joined in order.
    pub(crate) fn expect_proposal_hunks_of(
        self,
        change: &str,
        file: &str,
        before: &str,
        after: &str,
    ) -> Self {
        let proposals =
            crate::change_proposals::list_proposals_impl(&self.store(), &self.stream_id, change);
        let proposal = proposals.first().expect("one change edit proposal");
        let hunks: Vec<_> = crate::change_proposals::load_hunks_impl(
            &self.store(),
            &crate::change_proposals::AppSites { app: &self.fx.app },
            &proposal.id,
        )
        .expect("the proposal's hunks")
        .hunks
        .into_iter()
        .filter(|hunk| hunk.path == file)
        .collect();
        let held: (String, String) = (
            hunks.iter().map(|h| h.hunk.before_text()).collect(),
            hunks.iter().map(|h| h.hunk.after_text()).collect(),
        );
        assert_eq!(
            held,
            (before.to_string(), after.to_string()),
            "[{}] what the hunks of {file} replace",
            self.name
        );
        self
    }

    /// OCR-FR-WBZX: one change edit proposal of the change, in the run's
    /// request thread, with the rationale and one hunk set for each file.
    pub(crate) fn expect_one_change_edit_proposal(
        self,
        change: &str,
        rationale: &str,
        files: &[&str],
    ) -> Self {
        let proposals =
            crate::change_proposals::list_proposals_impl(&self.store(), &self.stream_id, change);
        assert_eq!(proposals.len(), 1, "[{}] one change edit proposal", self.name);
        let proposal = &proposals[0];
        let data = self.run.openspec.clone().expect("the run's OpenSpec data");
        assert_eq!(
            data.proposal_id.as_deref(),
            Some(proposal.id.as_str()),
            "[{}] the run names its proposal",
            self.name
        );
        assert_eq!(
            Some(proposal.thread_id.as_str()),
            data.thread_id.as_deref(),
            "[{}] the proposal stands in the request thread",
            self.name
        );
        assert_eq!(proposal.rationale, rationale, "[{}] the proposal's rationale", self.name);
        let hunks = crate::change_proposals::load_hunks_impl(
            &self.store(),
            &crate::change_proposals::AppSites { app: &self.fx.app },
            &proposal.id,
        )
        .expect("the proposal's hunks")
        .hunks;
        let mut paths: Vec<String> = hunks.into_iter().map(|hunk| hunk.path).collect();
        paths.sort();
        paths.dedup();
        assert_eq!(paths, sorted(files), "[{}] the files the hunks edit", self.name);
        self
    }

    /// OCR-FR-ZNJT: the stream branch stands where it stood before the run,
    /// and its working copy holds no uncommitted path, so the change files are
    /// as they were.
    pub(crate) fn expect_stream_untouched(self) -> Self {
        assert_eq!(self.branch_tip(), self.base, "[{}] the stream branch did not move", self.name);
        assert_eq!(
            crate::streams::uncommitted_paths_of(&self.worktree),
            Vec::<String>::new(),
            "[{}] the stream's working copy holds what its branch holds",
            self.name
        );
        self
    }

    // -- the plan update (OPU-FR-QXUE) ------------------------------------------

    /// OPU-FR-QXUE: the run rests `awaiting_author` on `plan_update`, with its
    /// plan thread and its question set.
    pub(crate) fn expect_plan_rest(self, reason: &str) -> Self {
        assert_eq!(self.run.state, GraduationRunState::AwaitingAuthor, "[{}] the state", self.name);
        let rest = self.run.rest.clone().unwrap_or_else(|| panic!("[{}] a rest", self.name));
        assert_eq!(
            rest.reason,
            crate::graduation::plan_updates::REST_PLAN_UPDATE,
            "[{}] the rest",
            self.name
        );
        assert!(rest.set_id.is_some(), "[{}] the rest names its question set", self.name);
        let thread_id = rest
            .thread_id
            .unwrap_or_else(|| panic!("[{}] the rest names its plan thread", self.name));
        let change = self.run.openspec.as_ref().expect("the run's OpenSpec data").change.clone();
        let target =
            crate::comments::DiscussionTarget::Change { stream_id: self.stream_id.clone(), change };
        let thread = crate::comments::locate_discussion_of(&self.store(), &target, &thread_id)
            .unwrap_or_else(|| panic!("[{}] the plan thread is stored", self.name));
        let first = thread.comments.first().expect("the first comment of the plan thread");
        assert_eq!(
            first.author,
            crate::comments::Participant::synthesis(),
            "[{}] the plan thread opens as the Synthesis participant",
            self.name
        );
        assert_eq!(first.body, reason, "[{}] the first comment is the reason", self.name);
        self
    }

    /// OPU-FR-THOZ: the plan rest holds no answers.
    pub(crate) fn expect_no_plan_answers(self) -> Self {
        let rest = self.fx.reload(&self.run.id).rest.expect("the rest");
        assert_eq!(rest.answers, None, "[{}] the rest holds no answers", self.name);
        self
    }

    // -- the archive at merge (OAR-FR-IAKL, OAR-FR-LZLS, OAR-FR-SOFQ) -----------

    /// OAR-FR-SOFQ: the merge result names each archived change, in archive
    /// order.
    pub(crate) fn expect_changes_archived(self, changes: &[&str]) -> Self {
        let archived = match self.merge_answer.as_ref() {
            Some(Ok(StreamMergeResult::Merged { archived, .. })) => archived.clone(),
            other => panic!("[{}] expected `merged`, the answer was {other:?}", self.name),
        };
        let held: Vec<(String, String)> =
            archived.into_iter().map(|a| (a.change, a.archived_as)).collect();
        let expected: Vec<(String, String)> = changes
            .iter()
            .map(|change| (change.to_string(), format!("{ARCHIVE_DATE}-{change}")))
            .collect();
        assert_eq!(held, expected, "[{}] the archived changes, in order", self.name);
        self
    }

    /// OAR-FR-IAKL, OAR-FR-LZLS: the stream branch ends in one archive commit
    /// for each change, in archive order, each holding its change folder, its
    /// archive folder and the live spec; and the base branch holds the merge
    /// of that tip, so every archive commit came before the merge.
    pub(crate) fn expect_archive_commits_before_the_merge(self, changes: &[&str]) -> Self {
        let tip = self.branch_tip().expect("the stream branch");
        let repo = self.fx.repo();
        let mut commit = repo.find_commit(git2::Oid::from_str(&tip).unwrap()).unwrap();
        let mut archives = Vec::new();
        for _ in changes {
            let parent = commit.parent(0).expect("a parent");
            let diff = repo
                .diff_tree_to_tree(
                    Some(&parent.tree().unwrap()),
                    Some(&commit.tree().unwrap()),
                    None,
                )
                .unwrap();
            let mut tops: Vec<String> = diff
                .deltas()
                .filter_map(|d| d.new_file().path().or_else(|| d.old_file().path()))
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
            tops.sort();
            archives.push((commit.message().unwrap_or_default().to_string(), tops));
            commit = parent;
        }
        archives.reverse();
        for ((message, paths), change) in archives.iter().zip(changes) {
            assert_eq!(
                message,
                &format!("Archive OpenSpec change {change}"),
                "[{}] the archive commit of {change}, on one line",
                self.name
            );
            let folder = change_file(change, "");
            let archive = format!("openspec/changes/archive/{ARCHIVE_DATE}-{change}/");
            // OAR-FR-BLBJ: the commit moves the change folder into its archive
            // folder and changes the live spec.
            assert!(
                paths.iter().any(|p| p.starts_with(&folder)),
                "[{}] the archive commit of {change} removes nothing of {folder}: {paths:?}",
                self.name
            );
            assert!(
                paths.iter().any(|p| p.starts_with(&archive)),
                "[{}] the archive commit of {change} adds nothing below {archive}: {paths:?}",
                self.name
            );
            assert!(
                paths.iter().any(|p| p == "openspec/specs/auth/spec.md"),
                "[{}] the archive commit of {change} leaves the live spec unchanged: {paths:?}",
                self.name
            );
            for path in paths {
                assert!(
                    path.starts_with(&folder)
                        || path.starts_with(&archive)
                        || path.starts_with("openspec/specs/"),
                    "[{}] the archive commit of {change} holds {path}",
                    self.name
                );
            }
        }
        let base = repo
            .find_commit(git2::Oid::from_str(&self.base_tip()).unwrap())
            .expect("the base branch tip");
        assert!(
            base.parent_ids().any(|parent| parent.to_string() == tip),
            "[{}] the merge commit on the base holds the stream tip with its archive commits",
            self.name
        );
        self
    }

    // -- the merge notice (OFP-FR-ETEN, OFP-FR-XQDY, OFP-FR-PSVR) ---------------

    /// OFP-FR-ETEN, OFP-FR-XQDY: the change has `discussions` discussions and
    /// the project `runs` repair runs; the notice record names the one thread
    /// and the one run.
    pub(crate) fn expect_merge_notice(self, change: &str, discussions: usize, runs: usize) -> Self {
        let target = crate::comments::DiscussionTarget::Change {
            stream_id: self.stream_id.clone(),
            change: change.to_string(),
        };
        let threads = crate::comments::list_discussions_in(&self.store(), &target);
        assert_eq!(threads.len(), discussions, "[{}] the discussions of {change}", self.name);
        let repairs = self.repair_run_ids();
        assert_eq!(repairs.len(), runs, "[{}] the repair runs", self.name);
        let record =
            crate::openspec_archive::read_notice_record(&self.store(), &self.stream_id, change)
                .unwrap_or_else(|| panic!("[{}] the notice record of {change}", self.name));
        assert!(
            threads.iter().any(|thread| thread.id == record.thread_id),
            "[{}] the record names a notice thread of {change}",
            self.name
        );
        match record.repair_run_id.as_ref() {
            Some(run_id) => assert!(
                repairs.contains(run_id),
                "[{}] the record names a repair run: {run_id}",
                self.name
            ),
            None => assert_eq!(runs, 0, "[{}] the record names no repair run", self.name),
        }
        self
    }

    /// OFP-FR-YKGQ: the merge gave no notice of `change`: no discussion, no
    /// notice record, and no repair run.
    pub(crate) fn expect_no_merge_notice(self, change: &str) -> Self {
        let target = crate::comments::DiscussionTarget::Change {
            stream_id: self.stream_id.clone(),
            change: change.to_string(),
        };
        let threads = crate::comments::list_discussions_in(&self.store(), &target);
        assert!(threads.is_empty(), "[{}] no discussion of {change}", self.name);
        assert!(
            crate::openspec_archive::read_notice_record(&self.store(), &self.stream_id, change)
                .is_none(),
            "[{}] no notice record of {change}",
            self.name
        );
        assert!(self.repair_run_ids().is_empty(), "[{}] no repair run", self.name);
        self
    }

    /// OFP-FR-MQGG, OFP-FR-PSVR: the `OpenSpecFailure` of the last merge
    /// refusal carries the thread, and the repair run or the repair refusal,
    /// of the notice record of `change`.
    pub(crate) fn expect_refusal_carries_the_notice_record(self, change: &str) -> Self {
        let failure = self
            .merge_failure
            .clone()
            .unwrap_or_else(|| panic!("[{}] the merge was refused with an object", self.name));
        let record =
            crate::openspec_archive::read_notice_record(&self.store(), &self.stream_id, change)
                .unwrap_or_else(|| panic!("[{}] the notice record of {change}", self.name));
        assert_eq!(
            (failure.thread_id, failure.repair_run_id, failure.repair_refusal),
            (Some(record.thread_id), record.repair_run_id, record.repair_refusal),
            "[{}] the refusal carries the ids of the notice record",
            self.name
        );
        self
    }

    /// OFP-FR-YKGQ: the last merge refusal names the code and the cause, and
    /// no notice.
    pub(crate) fn expect_refusal_without_a_notice(self, code: &str, cause: &str) -> Self {
        let failure = self
            .merge_failure
            .clone()
            .unwrap_or_else(|| panic!("[{}] the merge was refused with an object", self.name));
        assert_eq!(
            (failure.code.as_str(), failure.cause.as_deref()),
            (code, Some(cause)),
            "[{}] the refusal",
            self.name
        );
        assert_eq!(
            (failure.thread_id, failure.repair_run_id),
            (None, None),
            "[{}] the refusal names no notice",
            self.name
        );
        self
    }

    /// GTE-FR-YAEB: the queue was offered the repair run alone, as a run
    /// enqueued with auto-start on is.
    pub(crate) fn expect_queue_offered_the_repair_run_alone(self) -> Self {
        let repairs = self.repair_run_ids();
        assert!(
            !self.offered.is_empty()
                && self.offered.iter().all(|(run_id, _)| repairs.contains(run_id)),
            "[{}] the queue was offered the repair run alone: {:?}",
            self.name,
            self.offered
        );
        self
    }

    /// GTE-FR-YAEB: the queue was offered no run.
    pub(crate) fn expect_queue_offered_no_run(self) -> Self {
        assert!(self.offered.is_empty(), "[{}] the queue offered {:?}", self.name, self.offered);
        self
    }
}

fn canonical(path: &std::path::Path) -> std::path::PathBuf {
    super::super::canonical(path)
}

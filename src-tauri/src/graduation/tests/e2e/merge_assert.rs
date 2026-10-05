//! What a merge scenario asserts: the answer of the request, the run the merge
//! was handed to, the snapshot and the worktree the run owns, and what both
//! branches hold (GTE-FR-TNZF, GTE-FR-LMXV, GTE-FR-SFTU, GTE-FR-DQLR).
//!
//! Every read comes from the repository, the run store or the queue, never from
//! a value the loop returned.

use tauri::Manager;

use super::outcome::{sorted, Outcome};
use crate::graduation::{GraduationMergeData, GraduationRunState};
use crate::streams::StreamMergeResult;

impl Outcome {
    /// The merge data of the run in focus, read back from the run store.
    pub(crate) fn merge_data(&self) -> GraduationMergeData {
        self.fx
            .reload(&self.run.id)
            .merge
            .unwrap_or_else(|| panic!("[{}] the run in focus carries merge data", self.name))
    }

    // -- the answer of the request (GTE-FR-TNZF) -------------------------------

    fn answer(&self) -> &Result<StreamMergeResult, String> {
        self.merge_answer
            .as_ref()
            .unwrap_or_else(|| panic!("[{}] no merge was requested", self.name))
    }

    /// WKS-FR-TVBM: the merge was clean and landed on the publication. The paths
    /// are the ones it wrote, and a commit id is present where it committed.
    pub(crate) fn expect_merged(self, paths: &[&str], committed: bool) -> Self {
        match self.answer() {
            Ok(StreamMergeResult::Merged { merged_paths, commit }) => {
                let mut held = merged_paths.clone();
                held.sort();
                assert_eq!(held, sorted(paths), "[{}] the paths the merge wrote", self.name);
                assert_eq!(
                    commit.is_some(),
                    committed,
                    "[{}] whether the merge made a commit",
                    self.name
                );
            }
            other => panic!("[{}] expected `merged`, the answer was {other:?}", self.name),
        }
        self
    }

    /// WKS-FR-FOTC: the stream holds nothing the base does not.
    pub(crate) fn expect_nothing_to_merge(self) -> Self {
        assert!(
            matches!(self.answer(), Ok(StreamMergeResult::NothingToMerge)),
            "[{}] expected `nothing_to_merge`, the answer was {:?}",
            self.name,
            self.answer()
        );
        self
    }

    /// WKS-FR-GKPX: Git could not settle these paths, and the answer names the
    /// run the merge was handed to.
    pub(crate) fn expect_conflicted(self, paths: &[&str]) -> Self {
        match self.answer() {
            Ok(StreamMergeResult::Conflicted { run_id, conflicted_paths }) => {
                let mut held = conflicted_paths.clone();
                held.sort();
                assert_eq!(held, sorted(paths), "[{}] the conflicted paths", self.name);
                assert_eq!(
                    run_id, &self.run.id,
                    "[{}] the answer names the run in focus",
                    self.name
                );
            }
            other => panic!("[{}] expected `conflicted`, the answer was {other:?}", self.name),
        }
        self
    }

    /// GTE-FR-TNZF: the request was refused with the typed code.
    pub(crate) fn expect_merge_refused(self, code: &str) -> Self {
        match self.answer() {
            Err(reason) => assert!(
                reason.starts_with(code),
                "[{}] the merge was refused with {reason:?}, not {code:?}",
                self.name
            ),
            Ok(answer) => panic!("[{}] the merge was accepted ({answer:?}), but {code} was expected", self.name),
        }
        self
    }

    // -- no run, no leftovers (GTE-FR-TNZF) ----------------------------------------

    /// WKS-FR-PZKD: the run store and the queue index hold no merge run, and the
    /// run store holds nothing a run did not make.
    pub(crate) fn expect_no_merge_run(self) -> Self {
        let queue = crate::graduation::list_graduation_queue(self.fx.app.clone())
            .unwrap_or_else(|reason| panic!("[{}] the queue was not readable: {reason}", self.name));
        assert!(
            queue.runs.iter().all(|run| !run.is_merge()),
            "[{}] the queue holds no merge run",
            self.name
        );
        let base = crate::graduation::store_base(&self.fx.app).expect("the run store");
        let store = super::settings::guard(&base.root);
        if let Ok(text) = store.read_text(base.queue_path(&queue.project_key)) {
            let index: toml::Table = toml::from_str(&text).expect("the queue index");
            let merges = index
                .get("runs")
                .and_then(|runs| runs.as_array())
                .map(|runs| {
                    runs.iter()
                        .filter(|entry| entry.get("merge").and_then(|v| v.as_bool()) == Some(true))
                        .count()
                })
                .unwrap_or(0);
            assert_eq!(merges, 0, "[{}] the queue index names no merge run", self.name);
        }
        let homes: Vec<String> = store
            .list_dir(base.root.join("g"))
            .map(|entries| entries.into_iter().map(|entry| entry.name).collect())
            .unwrap_or_default();
        for home in homes {
            assert!(
                queue.runs.iter().any(|run| run.id == home && !run.is_merge()),
                "[{}] the run store holds a directory {home} that no ordinary run owns",
                self.name
            );
        }
        self.expect_no_merge_leftovers()
    }

    /// GRD-FR-PFMD: no merge worktree, no registration of one, no scratch branch
    /// and no snapshot ref stand in the repository.
    pub(crate) fn expect_no_merge_leftovers(self) -> Self {
        let repo = self.fx.repo();
        let registered: Vec<String> = repo
            .worktrees()
            .map(|list| {
                list.iter()
                    .filter_map(|entry| entry.ok().flatten().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        assert!(
            registered.iter().all(|name| !name.ends_with("-mw")),
            "[{}] no merge worktree is registered: {registered:?}",
            self.name
        );
        let scratch: Vec<String> = repo
            .branches(Some(git2::BranchType::Local))
            .expect("the branches")
            .flatten()
            .filter_map(|(branch, _)| branch.name().ok().flatten().map(str::to_string))
            .filter(|name| name.starts_with("synthesis/merge-run/"))
            .collect();
        assert!(scratch.is_empty(), "[{}] no scratch branch is left: {scratch:?}", self.name);
        let refs: Vec<String> = repo
            .references_glob("refs/synthesis/merge/*")
            .expect("the references")
            .flatten()
            .filter_map(|reference| reference.name().ok().map(str::to_string))
            .collect();
        assert!(refs.is_empty(), "[{}] no snapshot ref is left: {refs:?}", self.name);
        self
    }

    /// GRD-FR-KZPT / GRD-FR-PFMD: what the run in focus owns is reclaimed.
    pub(crate) fn expect_merge_workspace_reclaimed(self) -> Self {
        let run = self.run.id.clone();
        let repo = self.fx.repo();
        assert!(
            !super::merge_arrange::present(&self.merge_worktree_path()),
            "[{}] the merge worktree directory is gone",
            self.name
        );
        assert!(
            !crate::streams::registers_worktree(&repo, &format!("{run}-mw")),
            "[{}] the merge worktree is not registered",
            self.name
        );
        assert!(
            repo.find_branch(&format!("synthesis/merge-run/{run}"), git2::BranchType::Local)
                .is_err(),
            "[{}] the scratch branch is gone",
            self.name
        );
        assert!(
            repo.find_reference(&format!("refs/synthesis/merge/{run}")).is_err(),
            "[{}] the snapshot ref is gone",
            self.name
        );
        self
    }

    /// GRD-FR-KZPT: the run owns its merge worktree, its registration, its
    /// scratch branch and the private ref of its snapshot.
    pub(crate) fn expect_merge_workspace_standing(self) -> Self {
        let run = self.run.id.clone();
        let repo = self.fx.repo();
        assert!(
            super::merge_arrange::present(&self.merge_worktree_path()),
            "[{}] the merge worktree stands",
            self.name
        );
        assert!(
            crate::streams::registers_worktree(&repo, &format!("{run}-mw")),
            "[{}] the merge worktree is registered as {run}-mw",
            self.name
        );
        assert!(
            repo.find_branch(&format!("synthesis/merge-run/{run}"), git2::BranchType::Local)
                .is_ok(),
            "[{}] the scratch branch stands",
            self.name
        );
        let reference = repo
            .find_reference(&format!("refs/synthesis/merge/{run}"))
            .unwrap_or_else(|_| panic!("[{}] the snapshot ref stands", self.name));
        assert_eq!(
            reference.peel_to_commit().expect("a commit").id().to_string(),
            self.merge_data().snapshot_commit,
            "[{}] the ref keeps the snapshot reachable",
            self.name
        );
        self
    }

    // -- the handoff (GTE-FR-LMXV) ------------------------------------------------

    /// GRD-FR-VCTH: the run is named `Merge <stream name>`, holds no draft, no
    /// standing work and no direct target.
    pub(crate) fn expect_merge_run_named(self, name: &str) -> Self {
        let run = self.fx.reload(&self.run.id);
        assert_eq!(self.merge_data().name, name, "[{}] the run's title", self.name);
        assert_eq!(run.input.draft_id, "", "[{}] the run names no draft", self.name);
        assert_eq!(run.input.draft_name, "", "[{}] the run names no draft title", self.name);
        assert_eq!(
            run.input.prompt_checksum,
            crate::fs::sha256_bytes(run.input.prompt.as_bytes()),
            "[{}] the checksum is that of the prompt",
            self.name
        );
        assert!(!run.input.prompt.trim().is_empty(), "[{}] the prompt states the merge", self.name);
        assert_eq!(
            run.standing_work,
            crate::graduation::StandingWork::Keep,
            "[{}] the standing-work choice is keep",
            self.name
        );
        assert!(run.direct_target.is_none(), "[{}] no direct target", self.name);
        assert!(run.is_merge(), "[{}] the run carries merge data", self.name);
        self
    }

    /// GRD-FR-KZPT: the run pinned both branch tips and the merge base as they
    /// stood before the merge started, and the snapshot is one commit whose two
    /// parents are those tips and whose unresolved paths hold diff3 markers.
    pub(crate) fn expect_merge_snapshot(self, unresolved: &[&str]) -> Self {
        let data = self.merge_data();
        let before = self
            .live_before
            .clone()
            .unwrap_or_else(|| panic!("[{}] no merge was started", self.name));
        assert_eq!(data.base_tip, before.base_tip_id(), "[{}] the pinned base tip", self.name);
        assert_eq!(
            Some(data.stream_tip.clone()),
            before.stream_tip_id(),
            "[{}] the pinned stream tip",
            self.name
        );
        let repo = self.fx.repo();
        let oid = |id: &str| git2::Oid::from_str(id).expect("a full object id");
        assert_eq!(data.base_tip.len(), 40, "[{}] full object ids are pinned", self.name);
        assert_eq!(
            repo.merge_base(oid(&data.base_tip), oid(&data.stream_tip))
                .expect("a merge base")
                .to_string(),
            data.merge_base,
            "[{}] the merge base",
            self.name
        );
        let snapshot = repo.find_commit(oid(&data.snapshot_commit)).expect("the snapshot commit");
        let parents: Vec<String> = snapshot.parent_ids().map(|id| id.to_string()).collect();
        assert_eq!(
            parents,
            vec![data.base_tip.clone(), data.stream_tip.clone()],
            "[{}] the snapshot's parents are the pinned tips",
            self.name
        );
        let tree = snapshot.tree().expect("the snapshot tree");
        for path in unresolved {
            let entry = tree
                .get_path(std::path::Path::new(path))
                .unwrap_or_else(|_| panic!("[{}] the snapshot holds {path}", self.name));
            let blob = repo.find_blob(entry.id()).expect("a blob");
            let text = String::from_utf8_lossy(blob.content()).into_owned();
            for marker in ["<<<<<<<", "|||||||", "=======", ">>>>>>>"] {
                assert!(
                    text.contains(marker),
                    "[{}] the snapshot's {path} holds the diff3 marker {marker}",
                    self.name
                );
            }
        }
        self.expect_snapshot_ref_standing()
    }

    /// GRD-FR-KZPT: the private ref keeps the snapshot reachable.
    pub(crate) fn expect_snapshot_ref_standing(self) -> Self {
        let run = self.run.id.clone();
        let repo = self.fx.repo();
        let reference = repo
            .find_reference(&format!("refs/synthesis/merge/{run}"))
            .unwrap_or_else(|_| panic!("[{}] the snapshot ref stands", self.name));
        assert_eq!(
            reference.peel_to_commit().expect("a commit").id().to_string(),
            self.merge_data().snapshot_commit,
            "[{}] the ref keeps the snapshot reachable",
            self.name
        );
        drop(reference);
        drop(repo);
        self
    }

    /// GRD-FR-KZPT: the paths the Git merge changes and the paths it could not
    /// merge, recorded on the run.
    pub(crate) fn expect_merge_paths(self, changed: &[&str], unresolved: &[&str]) -> Self {
        let data = self.merge_data();
        let mut held_changed = data.changed_paths.clone();
        held_changed.sort();
        let mut held_unresolved = data.unresolved_paths.clone();
        held_unresolved.sort();
        assert_eq!(held_changed, sorted(changed), "[{}] the changed paths", self.name);
        assert_eq!(held_unresolved, sorted(unresolved), "[{}] the unresolved paths", self.name);
        self
    }

    /// GRD-FR-KZPT: the publication the author chose is recorded on the run.
    pub(crate) fn expect_publication(self, publication: &crate::streams::StreamMergePublication) -> Self {
        assert_eq!(
            &self.merge_data().publication,
            publication,
            "[{}] the recorded publication",
            self.name
        );
        self
    }

    /// GTE-FR-YAEB: the queue was offered a dispatch of the merge run, with the
    /// stream free, and of nothing else.
    pub(crate) fn expect_queue_offered_the_merge_run(self) -> Self {
        assert_eq!(
            self.offered,
            vec![(self.run.id.clone(), true)],
            "[{}] the queue was offered the merge run once the guard was released",
            self.name
        );
        self
    }

    /// GTE-FR-YCQW: the queue had nothing to offer, because the project's slots
    /// are full.
    pub(crate) fn expect_queue_offered_nothing(self) -> Self {
        assert!(
            self.offered.is_empty(),
            "[{}] the queue offered nothing: {:?}",
            self.name,
            self.offered
        );
        self
    }

    /// GRD-FR-GRHC: the merge run waits for a project slot alone.
    pub(crate) fn expect_waiting_for_a_slot(self) -> Self {
        let capacity = crate::graduation::graduation_capacity_of(&self.fx.app);
        assert!(
            capacity.waiting_for_slot.contains(&self.run.id),
            "[{}] the capacity names the merge run as waiting for a slot: {capacity:?}",
            self.name
        );
        self
    }

    // -- what landed (GTE-FR-SFTU) ----------------------------------------------------

    /// GRD-FR-AQNW: the recorded result names the publication, the paths, and
    /// the commit where the publication made one.
    pub(crate) fn expect_merge_result(self, published: &str, committed: bool, paths: &[&str]) -> Self {
        let result = self
            .merge_data()
            .result
            .unwrap_or_else(|| panic!("[{}] the run records a merge result", self.name));
        let published_as = serde_json::to_value(result.published).expect("a publication");
        assert_eq!(published_as.as_str(), Some(published), "[{}] how the merge was published", self.name);
        assert_eq!(result.commit.is_some(), committed, "[{}] whether the result names a commit", self.name);
        let mut held = result.merged_paths.clone();
        held.sort();
        assert_eq!(held, sorted(paths), "[{}] the paths the result names", self.name);
        if let Some(commit) = result.commit {
            assert_eq!(
                self.run.commits,
                vec![commit],
                "[{}] the run's commits hold the merge commit",
                self.name
            );
        }
        self
    }

    /// GRD-FR-BHCS: a merge run records no result until it is `completed`.
    pub(crate) fn expect_no_merge_result(self) -> Self {
        assert!(
            self.merge_data().result.is_none(),
            "[{}] the run records no merge result",
            self.name
        );
        self
    }

    /// GRD-FR-AQNW: the base branch holds one new commit whose parents are the
    /// pinned tips, under the author's message.
    pub(crate) fn expect_base_holds_the_merge_commit(self, message: &str) -> Self {
        let data = self.merge_data();
        let repo = self.fx.repo();
        let head = repo
            .find_branch(&data.base_branch, git2::BranchType::Local)
            .and_then(|branch| branch.get().peel_to_commit())
            .expect("the base branch head");
        let parents: Vec<String> = head.parent_ids().map(|id| id.to_string()).collect();
        assert_eq!(
            parents,
            vec![data.base_tip.clone(), data.stream_tip.clone()],
            "[{}] the merge commit's parents are the pinned tips",
            self.name
        );
        assert_eq!(head.message().unwrap_or_default(), message, "[{}] the merge commit's message", self.name);
        assert_eq!(
            self.run.commits.last().map(String::as_str),
            Some(head.id().to_string().as_str()),
            "[{}] the run records the merge commit",
            self.name
        );
        self
    }

    /// GRD-FR-AQNW: the base branch stands where it stood, and the stream branch
    /// too: nothing was committed.
    pub(crate) fn expect_base_branch_at_the_pinned_tip(self) -> Self {
        let data = self.merge_data();
        assert_eq!(self.base_tip(), data.base_tip, "[{}] the base branch head", self.name);
        assert_eq!(
            self.branch_tip().as_deref(),
            Some(data.stream_tip.as_str()),
            "[{}] the stream branch head",
            self.name
        );
        self
    }

    /// GRD-FR-AQNW: the base working copy holds the file, unstaged where the
    /// publication was `uncommitted`.
    pub(crate) fn expect_base_working_copy_holds(self, path: &str, content: &str, unstaged: bool) -> Self {
        let root = self.fx.root();
        let found = super::settings::guard(&root)
            .read_text(root.join(path))
            .unwrap_or_default();
        assert_eq!(found, content, "[{}] what the base working copy holds at {path}", self.name);
        let repo = self.fx.repo();
        let status = repo
            .status_file(std::path::Path::new(path))
            .unwrap_or_else(|reason| panic!("[{}] {path} has a status: {reason}", self.name));
        if unstaged {
            assert!(
                status.is_wt_modified() || status.is_wt_new(),
                "[{}] {path} is an unstaged change: {status:?}",
                self.name
            );
            assert!(
                !status.is_index_modified() && !status.is_index_new(),
                "[{}] {path} is not staged: {status:?}",
                self.name
            );
        } else {
            assert!(
                status.is_empty(),
                "[{}] {path} is committed and clean: {status:?}",
                self.name
            );
        }
        self
    }

    // -- the stream's row (WKS-FR-KHJS) ----------------------------------------------------

    /// WKS-FR-KHJS: the stream's row in the listing carries its merge run and the
    /// state of that run.
    pub(crate) fn expect_stream_row_merge_run(self, state: GraduationRunState) -> Self {
        let rows = crate::streams::list_work_streams(self.fx.app.clone()).expect("the streams");
        let row = rows
            .into_iter()
            .find(|row| row.stream.id == self.stream_id)
            .unwrap_or_else(|| panic!("[{}] the stream is listed", self.name));
        let link = row
            .merge_run
            .unwrap_or_else(|| panic!("[{}] the stream's row carries a merge run", self.name));
        assert_eq!(link.run_id, self.run.id, "[{}] the row names the run", self.name);
        assert_eq!(link.name, self.merge_data().name, "[{}] the row carries the run's title", self.name);
        assert_eq!(link.state, state, "[{}] the row carries the run's state", self.name);
        self
    }

    /// WKS-FR-KHJS: a stream's row names no merge run.
    pub(crate) fn expect_stream_row_without_a_merge_run(self) -> Self {
        let rows = crate::streams::list_work_streams(self.fx.app.clone()).expect("the streams");
        let row = rows
            .into_iter()
            .find(|row| row.stream.id == self.stream_id)
            .unwrap_or_else(|| panic!("[{}] the stream is listed", self.name));
        assert!(row.merge_run.is_none(), "[{}] the row names no merge run", self.name);
        self
    }

    /// GRD-FR-JSBE: the base working copy holds no part of the merge.
    pub(crate) fn expect_base_working_copy_clean(self) -> Self {
        let dirty = crate::streams::uncommitted_paths_of(&self.fx.root());
        assert!(dirty.is_empty(), "[{}] the base working copy is clean: {dirty:?}", self.name);
        self
    }

    /// GRD-FR-MRNQ: the queue holds this many runs, whatever their kind.
    pub(crate) fn expect_runs_in_the_queue(self, count: usize) -> Self {
        let queue = crate::graduation::list_graduation_queue(self.fx.app.clone())
            .unwrap_or_else(|reason| panic!("[{}] the queue was not readable: {reason}", self.name));
        assert_eq!(queue.runs.len(), count, "[{}] the runs in the queue", self.name);
        self
    }

    /// GRD-FR-BHCS: the queue and the single-run read both answer for the merge
    /// run with its title and its merge data, as they do for every run.
    pub(crate) fn expect_listings_carry_the_merge_run(self) -> Self {
        let queue = crate::graduation::list_graduation_queue(self.fx.app.clone())
            .unwrap_or_else(|reason| panic!("[{}] the queue was not readable: {reason}", self.name));
        let listed = queue
            .runs
            .iter()
            .find(|run| run.id == self.run.id)
            .unwrap_or_else(|| panic!("[{}] the queue lists the merge run", self.name));
        let single = crate::graduation::get_graduation_run(self.fx.app.clone(), self.run.id.clone())
            .unwrap_or_else(|reason| panic!("[{}] the run was not readable: {reason}", self.name));
        for run in [listed, &single] {
            assert_eq!(
                run.merge.as_ref().map(|data| data.name.clone()),
                Some(self.merge_data().name),
                "[{}] a listing carries the run's title",
                self.name
            );
            assert_eq!(run.input.draft_id, "", "[{}] a listing carries no draft", self.name);
        }
        self
    }

    /// WKS-FR-HLGN: the repository update guard is free, so the next merge or
    /// update is not refused.
    pub(crate) fn expect_the_guard_free(self) -> Self {
        let state = self.fx.app.state::<crate::streams::StreamState>();
        assert!(
            !state.is_merging(&self.stream_id) && !state.is_updating(&self.stream_id),
            "[{}] neither a merge nor an update holds the guard",
            self.name
        );
        let hold = state
            .begin_repository_update(&self.stream_id, crate::streams::Reconciliation::Merge)
            .unwrap_or_else(|reason| panic!("[{}] the guard was taken: {reason}", self.name));
        drop(hold);
        self
    }

    /// GTE-FR-PFEO, GRD-FR-KKKN: how many project slots are held now.
    pub(crate) fn expect_slots_in_use(self, count: usize) -> Self {
        assert_eq!(
            self.fx.app.state::<crate::graduation::GraduationState>().working_count(),
            count,
            "[{}] the project slots held",
            self.name
        );
        self
    }

    /// GRD-FR-KZPT: the merge worktree holds this file now, as the run's earlier
    /// turns left it.
    pub(crate) fn expect_merge_worktree_holds(self, path: &str, content: &str) -> Self {
        self.merge_worktree_holds(path, content)
    }

    /// GRL-FR-XBUE: the run's own log says the budget was spent this many times,
    /// once for each time it rested on it.
    pub(crate) fn expect_rest_reason_count(self, reason: &str, count: usize) -> Self {
        let records = super::super::lines_of(
            &self.fx,
            &self.run.id,
            crate::graduation::GraduationLogStream::Structured,
        );
        let found = records
            .iter()
            .filter(|record| {
                record
                    .get("fields")
                    .and_then(|fields| fields.get("reason"))
                    .and_then(|v| v.as_str())
                    == Some(reason)
            })
            .count();
        assert_eq!(found, count, "[{}] how often the run's log states {reason:?}", self.name);
        self
    }

    /// GRD-FR-XHSE: the run ended `failed` with this typed code.
    pub(crate) fn expect_failure_code(self, code: &str) -> Self {
        let run = self.fx.reload(&self.run.id);
        assert_eq!(
            run.failure.as_ref().map(|failure| failure.code.as_str()),
            Some(code),
            "[{}] the code the run failed with",
            self.name
        );
        self
    }

    /// GRD-FR-RHNP: the merge run's place in its stream's queue, counted from
    /// the run that dispatches first.
    pub(crate) fn expect_queue_position(self, position: usize) -> Self {
        let queue = crate::graduation::list_graduation_queue(self.fx.app.clone())
            .unwrap_or_else(|reason| panic!("[{}] the queue was not readable: {reason}", self.name));
        let members: Vec<&str> = queue
            .runs
            .iter()
            .filter(|run| run.stream_id == self.stream_id)
            .map(|run| run.id.as_str())
            .collect();
        assert_eq!(
            members.iter().position(|id| *id == self.run.id),
            Some(position),
            "[{}] the merge run's place in its stream's queue: {members:?}",
            self.name
        );
        self
    }

    /// GRD-FR-JOFE: the run is filed away, so the stream's row no longer names it.
    pub(crate) fn expect_archived(self) -> Self {
        assert!(
            self.fx.reload(&self.run.id).archived,
            "[{}] the run is archived",
            self.name
        );
        self
    }

    /// GRD-FR-OYPY: the project's run order index names the run as a merge run, so
    /// a stream listing finds it without opening a run record.
    pub(crate) fn expect_queue_index_names_the_merge_run(self) -> Self {
        let base = crate::graduation::store_base(&self.fx.app).expect("the run store");
        let project_key = self.run.project_key.clone();
        let text = super::settings::guard(&base.root)
            .read_text(base.queue_path(&project_key))
            .expect("the queue index");
        let index: toml::Table = toml::from_str(&text).expect("the queue index");
        let entry = index
            .get("runs")
            .and_then(|runs| runs.as_array())
            .and_then(|runs| {
                runs.iter()
                    .find(|entry| entry.get("run_id").and_then(|v| v.as_str()) == Some(self.run.id.as_str())
                        || entry.get("runId").and_then(|v| v.as_str()) == Some(self.run.id.as_str()))
            })
            .unwrap_or_else(|| panic!("[{}] the index names the run", self.name));
        assert_eq!(
            entry.get("merge").and_then(|v| v.as_bool()),
            Some(true),
            "[{}] the index entry says it is a merge run",
            self.name
        );
        self
    }

    /// GRD-FR-YBUM / GRD-FR-KZPT: the run's base commit is its merge snapshot.
    pub(crate) fn expect_base_is_the_snapshot(self) -> Self {
        assert_eq!(
            self.fx.reload(&self.run.id).base_commit,
            Some(self.merge_data().snapshot_commit),
            "[{}] the run's base commit is the merge snapshot",
            self.name
        );
        self
    }
}

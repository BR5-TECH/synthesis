//! The reconciliation language (GTE-FR-NMUF): updating a graduated stream from
//! its base branch, merging it back, and what both branches then hold.
//!
//! Every request goes through the production command and the production job,
//! with the semantic turns scripted through the same seam a run's turns are.
//! Each request rebinds the scripted seam, so the dispatch assertions after it
//! read the semantic turns of that request alone.

use std::sync::Arc;

use super::super::Runtime;
use super::outcome::{sorted, Outcome};
use super::scenario::bind;
use super::script::TurnScript;
use super::settings::guard;
use crate::graduation::driver::GraduationDispatch;
use crate::graduation::{GraduationEscalation, GraduationEscalationAnswer, GraduationEscalationOrigin};
use crate::streams::{
    StreamUpdateRecord,
    StreamUpdateState, StreamUpdateStrategy,
};

impl Outcome {
    // -- arranging the base branch --------------------------------------------

    /// A commit on the branch the stream was created from, made in the
    /// project's own working copy as an author outside the application makes
    /// it.
    pub(crate) fn committing_on_the_base(self, path: &str, content: &str) -> Self {
        let root = self.fx.root();
        guard(&root)
            .write_text_atomic(root.join(path), content)
            .unwrap_or_else(|_| panic!("[{}] the base file {path} is writable", self.name));
        {
            let repo = self.fx.repo();
            let mut index = repo.index().expect("the base index");
            index
                .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
                .expect("the base change is staged");
            index.write().expect("the base index is written");
            let tree = repo
                .find_tree(index.write_tree().expect("a tree"))
                .expect("a tree");
            let signature = repo.signature().expect("a signature");
            let head = repo
                .head()
                .and_then(|head| head.peel_to_commit())
                .expect("the base head");
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                &format!("Base change to {path}"),
                &tree,
                &[&head],
            )
            .expect("the base change is committed");
        }
        self
    }

    /// WKS-FR-KFVJ: the revision the next update is pinned to, read now.
    pub(crate) fn pinning_the_base(mut self) -> Self {
        self.base_pin = Some(self.base_tip());
        self
    }

    /// GRB-FR-QIHE: an uncommitted file in the base branch's working copy.
    pub(crate) fn dirtying_the_base(self, path: &str, content: &str) -> Self {
        let root = self.fx.root();
        guard(&root)
            .write_text_atomic(root.join(path), content)
            .unwrap_or_else(|_| panic!("[{}] the base file {path} is writable", self.name));
        self
    }

    /// GRB-FR-QIHE: an uncommitted file in the stream's working copy.
    pub(crate) fn dirtying_the_stream(self, path: &str, content: &str) -> Self {
        guard(&self.worktree)
            .write_text_atomic(self.worktree.join(path), content)
            .unwrap_or_else(|_| panic!("[{}] the stream file {path} is writable", self.name));
        self
    }

    // -- updates ----------------------------------------------------------------

    /// WKS-FR-NRQT: the author updates the stream from its base branch, pinned
    /// to the revision the base stands at or to the one a scenario pinned.
    pub(crate) fn updated(mut self, strategy: StreamUpdateStrategy, scripts: Vec<TurnScript>) -> Self {
        let pinned = self.base_pin.take().unwrap_or_else(|| self.base_tip());
        let seam = self.rebind(scripts);
        self.reconcile_refusal = crate::streams::update_work_stream_with(
            &self.fx.app,
            &self.stream_id,
            strategy,
            pinned,
            crate::streams::scripted_update_start(seam),
        )
        .err();
        self
    }

    /// WKS-FR-CBXW: the author answers the update's escalation.
    pub(crate) fn update_answered(mut self, answers: &[&str], scripts: Vec<TurnScript>) -> Self {
        let record = self.update_record();
        let payload = answers_to(record.escalation.as_ref(), answers);
        self.attempt_before = Some(record.attempt_id);
        let seam = self.rebind(scripts);
        crate::streams::answer_work_stream_update_escalation_with(
            &self.fx.app,
            &self.stream_id,
            payload,
            crate::streams::scripted_update_start(seam),
        )
        .unwrap_or_else(|reason| panic!("[{}] the update answers were refused: {reason}", self.name));
        self
    }

    /// WKS-FR-DPNM: the author retries the update.
    pub(crate) fn update_retried(mut self, scripts: Vec<TurnScript>) -> Self {
        let seam = self.rebind(scripts);
        crate::streams::retry_work_stream_update_with(
            &self.fx.app,
            &self.stream_id,
            crate::streams::scripted_update_start(seam),
        )
        .unwrap_or_else(|reason| panic!("[{}] the update retry was refused: {reason}", self.name));
        self
    }

    /// GTE-FR-RZKC / WKS-FR-DPNM: a retry the update refuses starts nothing.
    pub(crate) fn refusing_update_retry(mut self, code: &str) -> Self {
        let before = self.update_record();
        let seam = self.rebind(Vec::new());
        let answer = crate::streams::retry_work_stream_update_with(
            &self.fx.app,
            &self.stream_id,
            crate::streams::scripted_update_start(seam),
        );
        self.assert_refused("update retry", code, answer.err());
        let after = self.update_record();
        assert_eq!(
            (after.state, after.attempt_id),
            (before.state, before.attempt_id),
            "[{}] the refused update retry changed nothing",
            self.name
        );
        self.expect_dispatch_count(0).expect_branches_unchanged()
    }

    /// GTE-FR-RZKC / WKS-FR-CBXW: answers the update refuses record nothing and
    /// start nothing.
    pub(crate) fn refusing_update_answers(mut self, answers: &[&str], code: &str) -> Self {
        let before = self.update_record();
        let payload = answers_to(before.escalation.as_ref(), answers);
        let seam = self.rebind(Vec::new());
        let answer = crate::streams::answer_work_stream_update_escalation_with(
            &self.fx.app,
            &self.stream_id,
            payload,
            crate::streams::scripted_update_start(seam),
        );
        self.assert_refused("update answers", code, answer.err());
        let after = self.update_record();
        assert_eq!(
            (after.state, after.attempt_id, after.escalation.is_some()),
            (before.state, before.attempt_id, before.escalation.is_some()),
            "[{}] the refused update answers changed nothing",
            self.name
        );
        self.expect_dispatch_count(0).expect_branches_unchanged()
    }

    fn update_record(&self) -> StreamUpdateRecord {
        crate::streams::get_work_stream_update(self.fx.app.clone(), self.stream_id.clone())
            .expect("a known stream")
            .unwrap_or_else(|| panic!("[{}] the stream holds an update record", self.name))
    }

    pub(crate) fn expect_update_state(self, state: StreamUpdateState) -> Self {
        let record = self.update_record();
        assert_eq!(record.state, state, "[{}] the update's state: {record:?}", self.name);
        self
    }

    pub(crate) fn expect_update_failure(self, code: &str) -> Self {
        let failure = self.update_record().failure;
        assert!(
            failure.starts_with(code),
            "[{}] the update failed with {code:?}, but it says {failure:?}",
            self.name
        );
        self
    }

    /// GRB-FR-YRHM: the semantic turns the update spent.
    pub(crate) fn expect_update_turns(self, turns: u32) -> Self {
        assert_eq!(
            self.update_record().semantic_turns,
            turns,
            "[{}] the semantic turns the update spent",
            self.name
        );
        self
    }

    pub(crate) fn expect_update_escalation_origin(self, origin: GraduationEscalationOrigin) -> Self {
        assert_eq!(
            self.update_record().escalation.map(|e| e.origin),
            Some(origin),
            "[{}] the phase the update's escalation came from",
            self.name
        );
        self
    }

    /// GRB-FR-YRHM: the paths the update could not settle.
    pub(crate) fn expect_update_unsettled(self, paths: &[&str]) -> Self {
        let mut held: Vec<String> = self
            .update_record()
            .conflicts
            .iter()
            .map(|c| c.path.clone())
            .collect();
        held.sort();
        assert_eq!(held, sorted(paths), "[{}] the paths the update left unsettled", self.name);
        self
    }

    pub(crate) fn expect_no_update_record(self) -> Self {
        let held = crate::streams::get_work_stream_update(self.fx.app.clone(), self.stream_id.clone())
            .expect("a known stream");
        assert!(held.is_none(), "[{}] no update was recorded", self.name);
        self
    }

    /// GRB-FR-WCHL: an answered update is planned again under a new attempt.
    pub(crate) fn expect_update_under_a_new_attempt(self) -> Self {
        assert_ne!(
            Some(self.update_record().attempt_id),
            self.attempt_before,
            "[{}] the answered update runs under a new attempt",
            self.name
        );
        self
    }

    // -- merges -------------------------------------------------------------------

    // -- both branches ------------------------------------------------------------

    /// The request was refused with the code before any job started.
    pub(crate) fn expect_reconciliation_refused(self, code: &str) -> Self {
        let refusal = self.reconcile_refusal.clone().unwrap_or_default();
        assert!(
            refusal.starts_with(code),
            "[{}] the request was refused with {code:?}, but the answer was {refusal:?}",
            self.name
        );
        self
    }

    /// GRB-FR-UHFE / GRB-FR-TXVL: neither branch moved.
    pub(crate) fn expect_branches_unchanged(self) -> Self {
        let before = self.tips_before.clone().expect("the tips before the request");
        assert_eq!(
            (self.branch_tip(), self.base_tip()),
            before,
            "[{}] neither branch moved",
            self.name
        );
        self
    }

    /// GRB-FR-ASWC: a merge moves the base branch and never the stream.
    pub(crate) fn expect_stream_branch_unchanged(self) -> Self {
        let before = self.tips_before.clone().expect("the tips before the request");
        assert_eq!(self.branch_tip(), before.0, "[{}] the stream branch did not move", self.name);
        self
    }

    /// GRB-FR-HJZC: an update moves the stream branch and never the base.
    pub(crate) fn expect_base_branch_unchanged(self) -> Self {
        let before = self.tips_before.clone().expect("the tips before the request");
        assert_eq!(self.base_tip(), before.1, "[{}] the base branch did not move", self.name);
        self
    }

    /// GRB-FR-ASWC: what the commit a merge made on the base branch says.
    pub(crate) fn expect_base_head_message_contains(self, fragment: &str) -> Self {
        let message = {
            let repo = self.fx.repo();
            let oid = git2::Oid::from_str(&self.base_tip()).expect("a revision");
            let message = repo
                .find_commit(oid)
                .expect("a commit")
                .message()
                .unwrap_or_default()
                .to_string();
            message
        };
        assert!(
            message.contains(fragment),
            "[{}] the base head says {fragment:?}, but it says {message:?}",
            self.name
        );
        self
    }

    /// What the base branch's working copy holds at a path.
    pub(crate) fn expect_base_holds(self, path: &str, content: &str) -> Self {
        let root = self.fx.root();
        let found = guard(&root).read_text(root.join(path)).unwrap_or_default();
        assert_eq!(found, content, "[{}] what the base working copy holds at {path}", self.name);
        self
    }

    /// GRB-FR-HJZC / GRB-FR-RMKD: the stream branch holds the base revision.
    pub(crate) fn expect_stream_contains_base(self) -> Self {
        let contains = {
            let repo = self.fx.repo();
            let stream = git2::Oid::from_str(&self.branch_tip().expect("the stream branch"))
                .expect("a revision");
            let base = git2::Oid::from_str(&self.base_tip()).expect("a revision");
            stream == base || repo.graph_descendant_of(stream, base).expect("a history")
        };
        assert!(contains, "[{}] the stream branch holds the base revision", self.name);
        self
    }

    // -- reading ------------------------------------------------------------------

    /// The revision the stream's base branch stands at now.
    pub(super) fn base_tip(&self) -> String {
        let base_branch = crate::streams::stream_of(&self.fx.app, &self.stream_id)
            .map(|stream| stream.base_branch)
            .unwrap_or_else(|| panic!("[{}] the stream is known", self.name));
        let repo = self.fx.repo();
        let tip = repo
            .find_branch(&base_branch, git2::BranchType::Local)
            .and_then(|branch| branch.get().peel_to_commit())
            .map(|commit| commit.id().to_string())
            .unwrap_or_else(|reason| panic!("[{}] the base branch is readable: {reason}", self.name));
        tip
    }

    /// Record both tips, and bind the scripts of the next request to the seam.
    fn rebind(&mut self, scripts: Vec<TurnScript>) -> Arc<dyn GraduationDispatch<Runtime>> {
        self.tips_before = Some((self.branch_tip(), self.base_tip()));
        self.dispatch = bind(&self.ctx, scripts);
        self.dispatch.clone() as Arc<dyn GraduationDispatch<Runtime>>
    }

    fn assert_refused(&self, what: &str, code: &str, refusal: Option<String>) {
        let refusal = refusal
            .unwrap_or_else(|| panic!("[{}] the {what} was accepted, but {code} was expected", self.name));
        assert!(
            refusal.starts_with(code),
            "[{}] the {what} was refused with {refusal:?}, not {code:?}",
            self.name
        );
    }
}

/// One answer per recorded question, in position order.
fn answers_to(escalation: Option<&GraduationEscalation>, answers: &[&str]) -> Vec<GraduationEscalationAnswer> {
    let positions: Vec<u32> = escalation
        .map(|e| e.questions.iter().map(|q| q.position).collect())
        .unwrap_or_default();
    answers
        .iter()
        .enumerate()
        .map(|(index, answer)| GraduationEscalationAnswer {
            position: positions.get(index).copied().unwrap_or(index as u32 + 1),
            answer: answer.to_string(),
            summary: answer.to_string(),
        })
        .collect()
}

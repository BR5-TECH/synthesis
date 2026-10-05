//! The scenario language (GTE-FR-BWKD).
//!
//! One chain per journey, in four stages and always in this order: **arrange**
//! the project, the stream and the settings; **script** the turns the agent
//! answers with; **act** on the graduation run; and **assert** the record and
//! the repository. Every method is named for what the author does rather than
//! for what the loop calls.
//!
//! Nothing here is a second loop. The chain arranges the production fixture,
//! binds the scripted dispatch at the one seam GXD-FR-QLFA names, and reads the
//! run back from the store afterwards (GTE-FR-QVHM, GTE-FR-TCUW).
//!
//! The language stands in several files, each under a thousand lines: the turn
//! script in `script.rs`, the hooks in `hooks.rs`, the settings in
//! `settings.rs`, the assertions in `outcome.rs` and `repository_assert.rs`,
//! and the acts on a resting run in `acts.rs`.

use std::collections::BTreeMap;
use std::sync::Arc;

use tauri::Manager;

use super::super::{Fixture, ScriptedDispatch, Turn};
use super::hooks::{hold_index_of, without_loop, OtherRun, Others, TurnContext};
pub(crate) use super::outcome::Outcome;
pub(crate) use super::script::{finding, TurnScript};
use super::settings::{guard, head_of, write_raw_setting, write_setting, write_setting_at};
pub(crate) use super::settings::{set_setting, Setting};
use crate::graduation::StandingWork;
use crate::streams::StreamMergePublication;

/// GTE-FR-BWKD: one journey, from arrangement to assertion.
///
/// GTE-FR-LRJT: the project repository, the stream, the working copy and the
/// run store are this scenario's own, and the fixture removes every one of them
/// when the scenario is dropped.
pub(crate) struct Scenario {
    name: &'static str,
    fx: Fixture,
    stream_name: String,
    prompt: String,
    scripts: Vec<TurnScript>,
    standing_work: StandingWork,
    standing_message: Option<String>,
    standing_files: Vec<(String, String)>,
    index_held: bool,
    branches: Vec<String>,
    /// `(label, stream, prompt)` of each run enqueued beside the scenario's own.
    others: Vec<(&'static str, String, String)>,
    /// GTE-FR-TNZF: what the arrangement commits on each branch before the
    /// scenario acts, in the order it was named.
    branch_commits: Vec<BranchCommit>,
    /// GTE-FR-LMXV: the scenario starts no draft run, so the stream holds the
    /// work the arrangement committed and nothing else.
    no_draft_run: bool,
    /// GTE-FR-TNZF: the publication the scenario's Merge is started with.
    merge: Option<StreamMergePublication>,
    /// GTE-FR-YCQW: streams whose claims fill project slots for the whole
    /// scenario.
    slot_holders: Vec<String>,
}

/// One commit the arrangement makes on a branch as another author does.
pub(super) struct BranchCommit {
    pub(super) on_base: bool,
    pub(super) path: String,
    pub(super) content: String,
}

impl Scenario {
    pub(crate) fn named(name: &'static str) -> Self {
        let fx = Fixture::new();
        // GTE-FR-YAEB: the production queue's dispatch is intercepted, and an
        // attempt fails the scenario unless it names one it expected.
        fx.app
            .manage(crate::graduation::driver::drive::SpawnTripwire::default());
        Self {
            name,
            fx,
            stream_name: "editor".to_string(),
            prompt: "Add the empty state to the panel.".to_string(),
            scripts: Vec::new(),
            standing_work: StandingWork::default(),
            standing_message: None,
            standing_files: Vec::new(),
            index_held: false,
            branches: Vec::new(),
            others: Vec::new(),
            branch_commits: Vec::new(),
            no_draft_run: false,
            merge: None,
            slot_holders: Vec::new(),
        }
    }

    /// GTE-FR-KAZX: the project setting this scenario's turns run under.
    pub(crate) fn with_setting(self, setting: Setting, value: u64) -> Self {
        write_setting(&self.fx, setting, Some(value));
        self
    }

    /// GTE-FR-KAZX: a stored value the project settings must repair rather than
    /// use (PSS-FR-ZLCF).
    pub(crate) fn with_raw_setting(self, setting: Setting, value: i64) -> Self {
        write_raw_setting(&self.fx, setting, value);
        self
    }

    pub(crate) fn with_stream(mut self, name: &str) -> Self {
        self.stream_name = name.to_string();
        self
    }

    pub(crate) fn with_prompt(mut self, prompt: &str) -> Self {
        self.prompt = prompt.to_string();
        self
    }

    /// GRD-FR-HQPD: what the run does with work standing in its stream.
    pub(crate) fn with_standing_work(mut self, choice: StandingWork) -> Self {
        self.standing_work = choice;
        self
    }

    /// GRD-FR-RJFC: the message a standing commit takes.
    pub(crate) fn with_standing_message(mut self, message: &str) -> Self {
        self.standing_message = Some(message.to_string());
        self
    }

    /// GRD-FR-HQPD: a file the author left uncommitted in the stream before
    /// the run was dispatched.
    pub(crate) fn with_standing_file(mut self, path: &str, content: &str) -> Self {
        self.standing_files.push((path.to_string(), content.to_string()));
        self
    }

    /// GTC-FR-19: the stream's index is held before the run is dispatched.
    pub(crate) fn with_index_held(mut self) -> Self {
        self.index_held = true;
        self
    }

    /// A branch of the project's repository, at the revision it stands at.
    pub(crate) fn with_branch(mut self, name: &str) -> Self {
        self.branches.push(name.to_string());
        self
    }

    /// GSU-FR-GLVQ: a machine configured to execute an agent, so a restart is
    /// reachable.
    pub(crate) fn with_execution_allowed(self) -> Self {
        self.fx.allow_execution();
        self
    }

    /// GTE-FR-YAEB: a second run, enqueued on the named stream after the
    /// scenario's own with auto-start off, so only the scenario drives it.
    pub(crate) fn with_run_on_stream(mut self, label: &'static str, stream: &str, prompt: &str) -> Self {
        self.others
            .push((label, stream.to_string(), prompt.to_string()));
        self
    }

    /// GTE-FR-TNZF: a commit on the stream's branch, made before the scenario
    /// acts.
    pub(crate) fn with_stream_commit(mut self, path: &str, content: &str) -> Self {
        self.branch_commits.push(BranchCommit {
            on_base: false,
            path: path.to_string(),
            content: content.to_string(),
        });
        self
    }

    /// GTE-FR-TNZF: a commit on the branch the stream was created from, made
    /// before the scenario acts.
    pub(crate) fn with_base_commit(mut self, path: &str, content: &str) -> Self {
        self.branch_commits.push(BranchCommit {
            on_base: true,
            path: path.to_string(),
            content: content.to_string(),
        });
        self
    }

    /// GTE-FR-LMXV: the scenario starts no draft run. Its stream holds what the
    /// arrangement committed, and the focus stays on no run until an act gives
    /// it one.
    pub(crate) fn without_a_draft_run(mut self) -> Self {
        self.no_draft_run = true;
        self
    }

    /// GTE-FR-TNZF: the author's Merge, started on the publication when the
    /// scenario runs. A merge Git cannot settle is handed to a run, and the
    /// scenario's merge turns drive it to rest. With no merge turn scripted the
    /// run is left `queued`.
    pub(crate) fn with_merge(mut self, publication: StreamMergePublication) -> Self {
        self.no_draft_run = true;
        self.merge = Some(publication);
        self
    }

    /// GTE-FR-YCQW: a claim on another stream, held for the whole scenario, so
    /// it takes one project slot.
    pub(crate) fn with_slot_held_by(mut self, stream: &str) -> Self {
        self.slot_holders.push(stream.to_string());
        self
    }

    /// GTE-FR-RDPE: the answer of the next `merge_work` turn, in dispatch order.
    pub(crate) fn merge_work_turn(self, script: TurnScript) -> Self {
        self.work_turn(script)
    }

    /// GTE-FR-RDPE: the answer of the next `merge_review` turn, in dispatch
    /// order.
    pub(crate) fn merge_review_turn(self, script: TurnScript) -> Self {
        self.review_turn(script)
    }

    /// Script the next work turn.
    pub(crate) fn work_turn(mut self, script: TurnScript) -> Self {
        self.scripts.push(script);
        self
    }

    /// Script the next review turn. The same list as the work turns: the loop
    /// decides the order, and a dispatch nobody scripted fails the scenario
    /// (GTE-FR-JYWB).
    pub(crate) fn review_turn(mut self, script: TurnScript) -> Self {
        self.scripts.push(script);
        self
    }

    // -----------------------------------------------------------------------
    // Act
    // -----------------------------------------------------------------------

    /// Drive the run to rest, and hold what it did.
    pub(crate) fn run(self) -> Outcome {
        let Scenario {
            name,
            fx,
            stream_name,
            prompt,
            scripts,
            standing_work,
            standing_message,
            standing_files,
            index_held,
            branches,
            others: other_runs,
            branch_commits,
            no_draft_run,
            merge,
            slot_holders,
        } = self;
        let mut streams: BTreeMap<String, crate::streams::WorkStream> = BTreeMap::new();
        let stream = fx.stream(&stream_name);
        streams.insert(stream_name.clone(), stream.clone());
        let worktree = stream.worktree();
        let base = head_of(&worktree);
        for (path, content) in &standing_files {
            guard(&worktree)
                .write_text_atomic(worktree.join(path), content)
                .unwrap_or_else(|_| panic!("[{name}] the standing file {path} is writable"));
        }
        for commit in &branch_commits {
            let repo = if commit.on_base {
                fx.repo()
            } else {
                git2::Repository::open(&worktree)
                    .unwrap_or_else(|_| panic!("[{name}] the stream's repository opens"))
            };
            super::merge_arrange::commit_file(&repo, &commit.path, &commit.content);
        }
        let run = if no_draft_run {
            super::merge_arrange::no_run()
        } else {
            fx.enqueue_under(&stream, &prompt, "d1", standing_work, standing_message)
        };

        let others = Arc::new(Others::default());
        for (label, other_stream, other_prompt) in &other_runs {
            let other = streams
                .entry(other_stream.clone())
                .or_insert_with(|| fx.stream(other_stream))
                .clone();
            let queued = fx.enqueue_for(&other, other_prompt, &format!("d-{label}"));
            without_loop(&fx.app, || {
                crate::graduation::set_graduation_auto_start(fx.app.clone(), queued.id.clone(), false)
            })
            .unwrap_or_else(|reason| panic!("[{name}] auto-start is settable: {reason}"));
            others.insert(
                label,
                OtherRun {
                    run_id: queued.id.clone(),
                    stream_id: other.id.clone(),
                    branch: other.branch.clone(),
                    worktree: other.worktree(),
                    claimed: None,
                },
            );
        }

        {
            let primary = fx.repo();
            let head = primary
                .head()
                .and_then(|h| h.peel_to_commit())
                .expect("the project's revision");
            for branch in &branches {
                primary
                    .branch(branch, &head, false)
                    .unwrap_or_else(|reason| panic!("[{name}] the branch {branch} exists: {reason}"));
            }
        }
        if index_held {
            hold_index_of(&worktree);
        }

        let ctx = TurnContext {
            app: fx.app.clone(),
            run_id: run.id.clone(),
            stream_id: stream.id.clone(),
            worktree: worktree.clone(),
            project_root: fx.root(),
            others,
            guard: Default::default(),
        };
        for holder in &slot_holders {
            let held = streams
                .entry(holder.clone())
                .or_insert_with(|| fx.stream(holder))
                .clone();
            fx.hold_stream(&held.id);
        }
        let merging = merge.is_some() || no_draft_run;
        let (dispatch, rested, pending) = if merging {
            (ScriptedDispatch::new(Vec::new()), run, scripts)
        } else {
            let dispatch = bind(&ctx, scripts);
            let rested = fx.drive(&run, dispatch.clone());
            (dispatch, rested, Vec::new())
        };
        let outcome = Outcome {
            name,
            fx,
            ctx,
            stream_id: stream.id.clone(),
            branch: stream.branch.clone(),
            worktree,
            base,
            run: rested,
            dispatch,
            previous: None,
            history_before: Vec::new(),
            base_pin: None,
            reconcile_refusal: None,
            tips_before: None,
            attempt_before: None,
            live_before: None,
            offered: Vec::new(),
            merge_answer: None,
        };
        match merge {
            Some(publication) => {
                let started = outcome.merging(publication);
                if pending.is_empty() || started.merge_run_id().is_none() {
                    started
                } else {
                    started.driving_the_merge(pending)
                }
            }
            None => outcome,
        }
    }
}

/// The scripted seam, with each turn's setting change and hooks folded into
/// the side effect the fixture already runs inside the turn.
pub(crate) fn bind(ctx: &TurnContext, scripts: Vec<TurnScript>) -> Arc<ScriptedDispatch> {
    let turns: Vec<Turn> = scripts
        .into_iter()
        .map(|script| {
            let TurnScript {
                mut inner,
                changes_setting,
                hooks,
            } = script;
            if changes_setting.is_some() || !hooks.is_empty() {
                let root = ctx.project_root.clone();
                let hook_ctx = ctx.clone();
                inner = inner.doing(move || {
                    if let Some((setting, value)) = changes_setting {
                        write_setting_at(&root, setting, value);
                    }
                    for hook in &hooks {
                        hook(&hook_ctx);
                    }
                });
            }
            inner
        })
        .collect();
    ScriptedDispatch::new(turns)
}

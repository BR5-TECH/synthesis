//! What an OpenSpec scenario arranges, and the acts of the author on an
//! OpenSpec run (GTE-FR-QPMA, GTE-FR-ZDCH).
//!
//! The project is in OpenSpec mode, and the fake CLI of `openspec_cli.rs`
//! stands behind the process seam of `OSM-openspec-mode.md`. Its retry wait
//! is the recorded wait seam of `OFP-openspec-failure-policy.md`
//! OFP-FR-MJZU, so no scenario waits. An apply run and a rework run start
//! through their production commands. A propose run is enqueued on the
//! production record shape, as every draft run of the suite is.

use std::sync::Arc;

use tauri::Manager;

use super::super::change_runs::recorded_seam;
use super::super::{canonical, Fixture, ProbeAnswer};
use super::hooks::without_loop;
use super::merge_arrange::commit_everything;
use super::openspec_cli::WorkingCopyCli;
use super::outcome::Outcome;
use super::scenario::{bind, Scenario};
use super::script::TurnScript;
use super::settings::guard;
use crate::graduation::{GraduationRun, StandingWork};
use crate::openspec::tests::{buffer, FakeProbe, Silent, CLI};

/// The live spec every OpenSpec project of the suite holds.
const LIVE_SPEC: &str = "openspec/specs/auth/spec.md";

/// GTE-FR-QPMA: the OpenSpec run a scenario starts in place of a draft run.
pub(crate) enum ChangeStart {
    /// GRD-FR-LKGC: a propose run that writes the named change.
    Propose(String),
    /// OCR-FR-XKTV: an apply run of a change the stream holds.
    Apply(String),
    /// OCR-FR-ZNJT: a rework run of a change the stream holds.
    Rework { change: String, instruction: String },
}

impl ChangeStart {
    /// The run, enqueued with the queue's own dispatch off (GTE-FR-YAEB).
    pub(super) fn enqueue(
        &self,
        name: &'static str,
        fx: &Fixture,
        stream: &crate::streams::WorkStream,
        prompt: &str,
    ) -> GraduationRun {
        match self {
            ChangeStart::Propose(change) => {
                let mut run = fx.enqueue_under(stream, prompt, "d1", StandingWork::default(), None);
                run.openspec = Some(crate::graduation::openspec_start::propose_data(change));
                crate::graduation::save_run(&fx.app, &mut run)
                    .unwrap_or_else(|reason| panic!("[{name}] the propose run is saved: {reason}"));
                run
            }
            ChangeStart::Apply(change) => start_apply(name, fx, &stream.id, change),
            ChangeStart::Rework { change, instruction } => {
                without_loop(&fx.app, || {
                    crate::graduation::start_change_rework(
                        fx.app.clone(),
                        stream.id.clone(),
                        change.clone(),
                        instruction.clone(),
                    )
                })
                .unwrap_or_else(|reason| panic!("[{name}] the rework start was refused: {reason}"))
                .run
            }
        }
    }
}

/// OCR-FR-IDFC: the author's apply start, with the queue's own dispatch off.
fn start_apply(name: &'static str, fx: &Fixture, stream_id: &str, change: &str) -> GraduationRun {
    without_loop(&fx.app, || {
        crate::graduation::start_change_apply(
            fx.app.clone(),
            stream_id.to_string(),
            change.to_string(),
        )
    })
    .unwrap_or_else(|reason| panic!("[{name}] the apply start was refused: {reason}"))
}

/// The path of one file of a change folder.
pub(crate) fn change_file(change: &str, file: &str) -> String {
    format!("openspec/changes/{change}/{file}")
}

impl Scenario {
    /// GTE-FR-QPMA, GTE-FR-ZDCH: the project uses OpenSpec, and the fake CLI
    /// answers behind the process seam with the recorded wait. The machine can
    /// execute an agent, so the apply and rework starts are reachable.
    pub(crate) fn in_openspec_mode(self) -> Self {
        let fx = &self.fx;
        let root = fx.root();
        // The machine store stands outside the project, so a discussion or a
        // proposal written there makes no working copy dirty.
        let store_root = canonical(fx.store.path());
        let machine = store_root.join("machine");
        guard(&store_root).create_dir(&machine).expect("the machine store is made");
        fx.app.state::<crate::project::ProjectState>().set_store(machine);
        let files = guard(&root);
        files
            .write_text_atomic(root.join("openspec/config.yaml"), "schema: spec-driven\n")
            .expect("the OpenSpec config is written");
        files.write_text_atomic(root.join(LIVE_SPEC), "# Auth\n").expect("the live spec");
        fx.allow_execution();
        commit_everything(&fx.repo(), "Use OpenSpec");

        let cli = Arc::new(WorkingCopyCli::default());
        fx.app.manage(cli.clone());
        let probe = Arc::new(FakeProbe::default().with_executable(CLI));
        let seam = recorded_seam(fx, cli, probe);
        // OSM-FR-ZNVZ: the version check is kept, as the open of a project
        // keeps it.
        seam.cli_state(&Silent, buffer());
        fx.app.manage(seam);
        self
    }

    /// A change the stream branch holds, as a propose run leaves it: a
    /// proposal, two tasks that are not done, and a delta of `auth`.
    pub(crate) fn with_change(self, change: &str) -> Self {
        self.with_stream_commit(
            &change_file(change, "proposal.md"),
            "# Why\n\nLogout is missing.\n",
        )
        .with_stream_commit(
            &change_file(change, "tasks.md"),
            "- [ ] Add the button\n- [ ] Wire the call\n",
        )
        .with_stream_commit(
            &change_file(change, "specs/auth/spec.md"),
            &format!("## ADDED Requirements\n### Requirement: {change}\n"),
        )
    }

    /// GRD-FR-LKGC: the run is a propose run of `change`.
    pub(crate) fn proposing(mut self, change: &str) -> Self {
        self.change_start = Some(ChangeStart::Propose(change.to_string()));
        self
    }

    /// OCR-FR-XKTV: the run is the author's apply run of `change`.
    pub(crate) fn applying(mut self, change: &str) -> Self {
        self.change_start = Some(ChangeStart::Apply(change.to_string()));
        self
    }

    /// OCR-FR-ZNJT: the run is the author's rework run of `change`.
    pub(crate) fn reworking(mut self, change: &str, instruction: &str) -> Self {
        self.change_start = Some(ChangeStart::Rework {
            change: change.to_string(),
            instruction: instruction.to_string(),
        });
        self
    }

    /// OFP-FR-BZDP: what the image probes of the first drive answer, in order.
    pub(crate) fn with_image_probes(mut self, answers: Vec<ProbeAnswer>) -> Self {
        self.probes = answers;
        self
    }

    /// OFP-FR-PQNS: the next `count` processes of `subcommand` time out.
    pub(crate) fn with_cli_timing_out(self, subcommand: &str, count: usize) -> Self {
        self.fx.app.state::<Arc<WorkingCopyCli>>().time_out(subcommand, count);
        self
    }
}

impl Outcome {
    /// The fake CLI of the scenario.
    pub(super) fn cli(&self) -> Arc<WorkingCopyCli> {
        self.fx.app.state::<Arc<WorkingCopyCli>>().inner().clone()
    }

    /// OCR-FR-IDFC: the author starts an apply run of another change on the
    /// stream, and it is driven under a script of its own. The focus moves to
    /// it.
    pub(crate) fn applying_another(mut self, change: &str, scripts: Vec<TurnScript>) -> Self {
        let started = start_apply(self.name, &self.fx, &self.stream_id, change);
        self.previous = Some(self.run.id.clone());
        self.base = self.branch_tip();
        self.ctx = self.ctx.for_run(&started.id, &self.stream_id, &self.worktree);
        self.dispatch = bind(&self.ctx, scripts);
        self.run = self.fx.drive(&started, self.dispatch.clone());
        self
    }

    /// OPU-FR-QXUE: the author answers the questions of the plan thread: an
    /// option for the first, words of their own for the second.
    pub(crate) fn answering_the_plan_questions(mut self) -> Self {
        let rest = self
            .fx
            .reload(&self.run.id)
            .rest
            .unwrap_or_else(|| panic!("[{}] the run rests on a plan update", self.name));
        super::super::change_plan_continue::submit(&self.fx, &rest)
            .unwrap_or_else(|reason| panic!("[{}] the answers were refused: {reason}", self.name));
        self.run = self.fx.reload(&self.run.id);
        self
    }

    /// GRD-FR-CYIB, OFP-FR-WACS: the author's Continue, and then the run
    /// driven again, with the image probes of that drive answering `answers`.
    pub(crate) fn continuing_with_image_probes(
        mut self,
        answers: Vec<ProbeAnswer>,
        scripts: Vec<TurnScript>,
    ) -> Self {
        let continued = self.fx.continue_run(&self.run);
        self.dispatch = bind(&self.ctx, scripts);
        self.dispatch.script_probes(answers);
        self.run = self.fx.drive(&continued, self.dispatch.clone());
        self
    }

    /// OPU-FR-THOZ: the author submits an answer to the first plan question
    /// alone. The submission is refused, so the rest holds no answers.
    pub(crate) fn answering_one_of_the_plan_questions(mut self) -> Self {
        let rest = self
            .fx
            .reload(&self.run.id)
            .rest
            .unwrap_or_else(|| panic!("[{}] the run rests on a plan update", self.name));
        let first = super::super::change_plan_continue::answers().remove(0);
        let refused = crate::comments::submit_discussion_question_answers(
            self.fx.app.clone(),
            rest.thread_id.clone().expect("the plan thread"),
            rest.set_id.clone().expect("the question set"),
            vec![first],
            self.fx.app.state(),
            self.fx.app.state(),
        );
        match refused {
            Ok(_) => panic!("[{}] a partial answer set was accepted", self.name),
            Err(code) => assert_eq!(
                code,
                crate::comments::ERR_QUESTION_ANSWERS_INCOMPLETE,
                "[{}] the partial answer set is refused",
                self.name
            ),
        }
        self.run = self.fx.reload(&self.run.id);
        self
    }

    /// OPU-FR-THOZ: the author's Continue is refused with `code`, and the
    /// whole stored run record is what it was.
    pub(crate) fn refusing_continue_and_changing_nothing(mut self, code: &str) -> Self {
        let record = |outcome: &Outcome| {
            serde_json::to_value(outcome.fx.reload(&outcome.run.id)).expect("a run record")
        };
        let before = record(&self);
        let refused = without_loop(&self.fx.app, || {
            crate::graduation::continue_graduation_run(self.fx.app.clone(), self.run.id.clone())
        });
        match refused {
            Ok(_) => panic!("[{}] the Continue was accepted, but {code} was expected", self.name),
            Err(reason) => assert!(
                reason.to_string().starts_with(code),
                "[{}] the Continue was refused with {reason}, not {code}",
                self.name
            ),
        }
        assert_eq!(record(&self), before, "[{}] the refused Continue changed nothing", self.name);
        self.run = self.fx.reload(&self.run.id);
        self
    }

    /// OFP-FR-PQNS: the next `count` processes of `archive` time out.
    pub(crate) fn timing_out_the_next_archives(self, count: usize) -> Self {
        self.cli().time_out("archive", count);
        self
    }

    /// OFP-FR-ETEN: the next `archive` process refuses the change with a
    /// `status` entry of `cause`, and writes nothing.
    pub(crate) fn refusing_the_next_archive(self, cause: &str) -> Self {
        self.cli().refuse_archive(cause);
        self
    }

    /// GRD-FR-EWTN: the author discards the repair run a merge notice started,
    /// so the stream holds no run when the author merges again.
    pub(crate) fn discarding_the_repair_run(self) -> Self {
        let repair: Vec<String> = self
            .repair_run_ids()
            .into_iter()
            .filter(|id| !self.fx.reload(id).state.is_terminal())
            .collect();
        assert_eq!(repair.len(), 1, "[{}] one repair run stands", self.name);
        without_loop(&self.fx.app, || {
            crate::graduation::discard_graduation_run(self.fx.app.clone(), repair[0].clone())
        })
        .unwrap_or_else(|reason| panic!("[{}] the discard was refused: {reason}", self.name));
        self
    }

    /// The rework runs of the project, discarded ones included.
    pub(super) fn repair_run_ids(&self) -> Vec<String> {
        crate::graduation::queue::project_queue(&self.fx.app)
            .map(|queue| queue.runs)
            .unwrap_or_default()
            .into_iter()
            .filter(|run| run.is_change_rework())
            .map(|run| run.id)
            .collect()
    }
}

//! Tests for `CAR-change-action-requests.md`.
//!
//! The working copy and the store are temporary folders. A [`FixedSites`]
//! resolver stands for the stream registry, and a [`FakeStarter`] stands for
//! the run queue of `OCR-openspec-change-runs.md`, whose starts are tested in
//! `graduation/tests/`. Fixtures build files with `std::fs`, which the FSA
//! guard allows in a test file.

use std::sync::{Arc, Mutex};

use tauri::Listener;
use tempfile::TempDir;

use super::*;
use crate::change_proposals::FixedSites;
use crate::comments::DiscussionTarget;
use crate::graduation::GraduationRun;

mod deciding;
mod recording;

pub(super) const STREAM: &str = "s1";
pub(super) const CHANGE: &str = "add-auth";
pub(super) const RATIONALE: &str = "The tasks are ready. <spoken>Shall I build it?</spoken>";
pub(super) const INSTRUCTION: &str = "Split task 2 into two tasks.";

pub(super) fn human() -> Participant {
    Participant::Human { login: "raver119".into(), display_name: None, email: None }
}

pub(super) fn agent() -> Participant {
    Participant::Agent {
        agent_id: "a1".into(),
        handle: "arch".into(),
        model: None,
        title: Some(String::new()),
    }
}

/// A run as a start of OCR answers it.
pub(super) fn fake_run(phase: &str) -> GraduationRun {
    let mut run = crate::graduation::build_run(
        "project",
        STREAM.into(),
        "Stream".into(),
        None,
        crate::graduation::CapturedGraduationInput::default(),
        crate::graduation::StandingWork::Keep,
        None,
    );
    run.openspec = Some(crate::graduation::RunOpenSpec::of_phase(phase, CHANGE));
    run
}

/// The run queue, as a confirmation reaches it. It records each start and
/// answers with `answer`: a run, or the refusal it holds.
#[derive(Default)]
pub(super) struct FakeStarter {
    pub(super) calls: Mutex<Vec<String>>,
    pub(super) refusal: Mutex<Option<String>>,
}

impl FakeStarter {
    pub(super) fn refusing(code: &str) -> FakeStarter {
        FakeStarter { calls: Mutex::default(), refusal: Mutex::new(Some(code.to_string())) }
    }

    pub(super) fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn answer(&self, phase: &str) -> Result<GraduationRun, String> {
        match self.refusal.lock().unwrap().clone() {
            Some(code) => Err(code),
            None => Ok(fake_run(phase)),
        }
    }
}

impl RunStarter for FakeStarter {
    fn start_apply(&self, stream_id: &str, change: &str) -> Result<GraduationRun, String> {
        self.calls.lock().unwrap().push(format!("apply {stream_id} {change}"));
        self.answer(crate::graduation::PHASE_APPLY)
    }

    fn start_rework(
        &self,
        stream_id: &str,
        change: &str,
        instruction: &str,
        thread_id: &str,
        requester: &Participant,
    ) -> Result<GraduationRun, String> {
        let handle = match requester {
            Participant::Agent { handle, .. } => handle.clone(),
            Participant::Human { .. } => "human".into(),
        };
        self.calls
            .lock()
            .unwrap()
            .push(format!("rework {stream_id} {change} {thread_id} {handle} {instruction}"));
        self.answer(crate::graduation::PHASE_REWORK)
    }
}

pub(super) struct Fixture {
    _wc_dir: TempDir,
    _store_dir: TempDir,
    pub(super) app: tauri::App<tauri::test::MockRuntime>,
    pub(super) store: fs::RootFs,
    pub(super) sites: FixedSites,
    pub(super) thread_id: String,
}

impl Fixture {
    pub(super) fn new() -> Fixture {
        let wc_dir = TempDir::new().expect("tempdir");
        let store_dir = TempDir::new().expect("tempdir");
        let base = wc_dir.path().join(format!("openspec/changes/{CHANGE}"));
        std::fs::create_dir_all(&base).expect("dirs");
        std::fs::write(base.join("proposal.md"), "## Why\n\nAuth.\n").expect("proposal");
        let wc = fs::RootFs::for_root(wc_dir.path());
        let store = fs::RootFs::for_root(store_dir.path());
        let thread = comments::open_discussion_unchecked(
            &store,
            &DiscussionTarget::Change { stream_id: STREAM.into(), change: CHANGE.into() },
            None,
            "@arch is it ready?".into(),
            Vec::new(),
            &human(),
            "2026-01-01T00:00:00Z",
        )
        .expect("discussion");
        Fixture {
            sites: FixedSites {
                sites: vec![(STREAM.into(), wc)],
                busy: Mutex::new(None),
                rework: Mutex::new(None),
            },
            _wc_dir: wc_dir,
            _store_dir: store_dir,
            app: crate::tools::tests::mock_app(),
            store,
            thread_id: thread.id,
        }
    }

    pub(super) fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle().clone()
    }

    pub(super) fn watch(&self) -> Arc<Mutex<Vec<serde_json::Value>>> {
        let seen: Arc<Mutex<Vec<serde_json::Value>>> = Arc::default();
        let sink = Arc::clone(&seen);
        self.app.listen(CHANGE_ACTION_REQUESTS_CHANGED, move |event| {
            if let Ok(value) = serde_json::from_str(event.payload()) {
                sink.lock().unwrap().push(value);
            }
        });
        seen
    }

    pub(super) fn record(
        &self,
        action: ChangeAction,
        instruction: Option<&str>,
    ) -> Result<ChangeActionRequest, String> {
        record_change_action_request(
            &self.handle(),
            &self.store,
            &self.sites,
            NewActionRequest {
                stream_id: STREAM,
                change: CHANGE,
                action,
                instruction,
                rationale: RATIONALE,
                agent: &agent(),
                thread_id: &self.thread_id,
            },
        )
    }

    pub(super) fn confirm(
        &self,
        starter: &FakeStarter,
        id: &str,
    ) -> Result<ChangeActionOutcome, String> {
        confirm_action_request(&self.handle(), &self.store, starter, id, &human())
    }

    pub(super) fn decline(
        &self,
        id: &str,
        feedback: Option<&str>,
    ) -> Result<ChangeActionOutcome, String> {
        decline_action_request(&self.handle(), &self.store, id, feedback, &human())
    }

    pub(super) fn thread(&self) -> comments::Discussion {
        locate_thread(&self.store, STREAM, CHANGE, &self.thread_id).expect("thread")
    }

    pub(super) fn requests(&self) -> Vec<ChangeActionRequest> {
        list_requests_impl(&self.store, STREAM, CHANGE)
    }

    /// Every file below the store's `action-requests/` folder of the change.
    pub(super) fn files(&self) -> Vec<String> {
        let dir = requests_dir(&self.store, STREAM, CHANGE).expect("dir");
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .map(|e| e.file_name().to_string_lossy().into())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }
}

/// The backend log records that mention `needle`, as JSON.
pub(super) fn backend_records(needle: &str) -> Vec<serde_json::Value> {
    crate::logging::BUFFER
        .query(
            &crate::logging::LogFilter {
                min_level: crate::logging::LogLevel::Debug,
                domains: vec![crate::logging::Domain::Backend],
                ..Default::default()
            },
            None,
            1000,
        )
        .expect("buffer")
        .records
        .iter()
        .map(|r| {
            serde_json::json!({
                "message": r.message,
                "level": format!("{:?}", r.level),
                "fields": r.fields,
            })
        })
        .filter(|r| r.to_string().contains(needle))
        .collect()
}

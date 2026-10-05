//! The tests of GitHub polling
//! (`../../../specifications/core/GPP-github-polling.md`).
//!
//! Everything runs against a real temporary project through a real
//! [`crate::fs::RootFs`] and a **fake** Projects client, because the
//! requirements are statements about what reached GitHub, in what order, and
//! what is on disk afterwards.

use std::sync::{mpsc, Arc, Mutex};

use tauri::Manager;
use tempfile::TempDir;

use super::client::*;
use super::{client, view};
use super::eligibility::*;
use super::records::*;
use super::session::*;

mod app_claims;
mod app_polling;
mod claiming;
mod client_parsing;
mod logging_and_secrets;
mod polling_rules;
mod session_rules;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

pub(super) const SECRET: &str = "ghp_secret_token_value";
pub(super) const PROJECT: &str = "PVT_project1";

pub(super) fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    dir
}

pub(super) fn repo() -> RepositoryRef {
    RepositoryRef { owner: "acme".into(), name: "widgets".into() }
}

pub(super) fn key(dir: &TempDir) -> SessionKey {
    SessionKey { project: "acme-project".into(), worktree: dir.path().to_path_buf() }
}

pub(super) fn settings(project: Option<&str>) -> GithubPollingSettings {
    GithubPollingSettings { project_node_id: project.map(str::to_string), interval_minutes: Some(5) }
}

pub(super) fn valid_shape() -> ProjectShape {
    ProjectShape {
        title: "Roadmap".into(),
        status_field: Some(StatusField {
            field_id: "FIELD_status".into(),
            options: vec![
                StatusOption { id: "OPT_todo".into(), name: "Todo".into() },
                StatusOption { id: "OPT_ready".into(), name: "Ready".into() },
                StatusOption { id: "OPT_progress".into(), name: "In Progress".into() },
                StatusOption { id: "OPT_done".into(), name: "Done".into() },
            ],
        }),
    }
}

/// One Project item holding an issue of `owner/name`.
pub(super) fn item(
    number: u64,
    status: Option<&str>,
    state: &str,
    issue_type: Option<&str>,
    owner: &str,
    name: &str,
) -> ProjectItem {
    ProjectItem {
        item_id: format!("ITEM_{number}"),
        status: status.map(str::to_string),
        issue: Some(ItemIssue {
            number,
            title: format!("Task {number}"),
            url: format!("https://github.com/{owner}/{name}/issues/{number}"),
            state: state.into(),
            issue_type: issue_type.map(str::to_string),
            repository_owner: owner.into(),
            repository_name: name.into(),
        }),
    }
}

/// A ready Task of the polling repository.
pub(super) fn ready(number: u64) -> ProjectItem {
    item(number, Some("Ready"), "OPEN", Some("Task"), "acme", "widgets")
}

/// One recorded GitHub call, so a test asserts what reached the wire and in
/// what order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Call {
    Viewer,
    Owner(String),
    Shape(String),
    Items(String),
    Fetch(u64),
    SetStatus { item: String, option: String },
}

/// A fake Projects client. Nothing here opens a socket.
pub(super) struct FakeProjects {
    pub calls: Mutex<Vec<Call>>,
    pub shape: Mutex<Result<ProjectShape, String>>,
    pub items: Mutex<Result<Vec<ProjectItem>, String>>,
    /// The issues `fetch_issue` answers with, by number.
    pub issues: Mutex<Vec<FetchedIssue>>,
    pub fetch_error: Mutex<Option<String>>,
    pub set_status_error: Mutex<Option<String>>,
    pub viewer: Mutex<Vec<GithubProjectOption>>,
    pub owned: Mutex<Vec<GithubProjectOption>>,
    /// When set, `set_item_status` reports that it was entered and then waits
    /// for a release, so a test can hold a claim in flight.
    pub pause: Mutex<Option<(mpsc::Sender<()>, mpsc::Receiver<()>)>>,
}

impl FakeProjects {
    pub fn new() -> Self {
        FakeProjects {
            calls: Mutex::new(Vec::new()),
            shape: Mutex::new(Ok(valid_shape())),
            items: Mutex::new(Ok(Vec::new())),
            issues: Mutex::new(Vec::new()),
            fetch_error: Mutex::new(None),
            set_status_error: Mutex::new(None),
            viewer: Mutex::new(Vec::new()),
            owned: Mutex::new(Vec::new()),
            pause: Mutex::new(None),
        }
    }

    pub fn with_items(items: Vec<ProjectItem>) -> Self {
        let fake = FakeProjects::new();
        *fake.items.lock().unwrap() = Ok(items);
        fake
    }

    /// Hold an issue that `fetch_issue` returns, in the selected Project with
    /// the given status.
    pub fn hold_issue(&self, number: u64, status: &str, issue_type: Option<&str>, state: &str) {
        self.issues.lock().unwrap().push(FetchedIssue {
            number,
            title: format!("Task {number}"),
            body: format!("Body of task {number}\nsecond line\n"),
            url: format!("https://github.com/acme/widgets/issues/{number}"),
            state: state.into(),
            issue_type: issue_type.map(str::to_string),
            repository_owner: "acme".into(),
            repository_name: "widgets".into(),
            project_items: vec![IssueProjectItem {
                item_id: format!("ITEM_{number}"),
                project_id: PROJECT.into(),
                status: Some(status.into()),
            }],
        });
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, call: Call) {
        self.calls.lock().unwrap().push(call);
    }
}

impl GithubProjects for FakeProjects {
    fn viewer_projects(&self, secret: &str) -> Result<Paged<GithubProjectOption>, String> {
        assert_eq!(secret, SECRET);
        self.record(Call::Viewer);
        Ok(Paged { items: self.viewer.lock().unwrap().clone(), truncated: false })
    }

    fn owner_projects(
        &self,
        secret: &str,
        owner: &str,
    ) -> Result<Paged<GithubProjectOption>, String> {
        assert_eq!(secret, SECRET);
        self.record(Call::Owner(owner.into()));
        Ok(Paged { items: self.owned.lock().unwrap().clone(), truncated: false })
    }

    fn project_shape(&self, secret: &str, project_id: &str) -> Result<ProjectShape, String> {
        assert_eq!(secret, SECRET);
        self.record(Call::Shape(project_id.into()));
        self.shape.lock().unwrap().clone()
    }

    fn project_items(&self, secret: &str, project_id: &str) -> Result<Paged<ProjectItem>, String> {
        assert_eq!(secret, SECRET);
        self.record(Call::Items(project_id.into()));
        self.items.lock().unwrap().clone().map(|items| Paged { items, truncated: false })
    }

    fn fetch_issue(
        &self,
        secret: &str,
        _owner: &str,
        _repo: &str,
        number: u64,
    ) -> Result<Option<FetchedIssue>, String> {
        assert_eq!(secret, SECRET);
        self.record(Call::Fetch(number));
        if let Some(error) = self.fetch_error.lock().unwrap().clone() {
            return Err(error);
        }
        Ok(self.issues.lock().unwrap().iter().find(|i| i.number == number).cloned())
    }

    fn set_item_status(
        &self,
        secret: &str,
        project_id: &str,
        item_id: &str,
        field_id: &str,
        option_id: &str,
    ) -> Result<(), String> {
        assert_eq!(secret, SECRET);
        assert_eq!(project_id, PROJECT);
        assert_eq!(field_id, "FIELD_status");
        self.record(Call::SetStatus { item: item_id.into(), option: option_id.into() });
        let pause = self.pause.lock().unwrap().take();
        if let Some((entered, release)) = pause {
            entered.send(()).unwrap();
            release.recv().unwrap();
        }
        if let Some(error) = self.set_status_error.lock().unwrap().clone() {
            return Err(error);
        }
        Ok(())
    }
}

pub(super) fn context<'a>(
    root: &'a crate::fs::RootFs,
    repository: &'a RepositoryRef,
) -> super::claims::ClaimContext<'a> {
    super::claims::ClaimContext { root, secret: SECRET, repository, project_id: PROJECT }
}

pub(super) fn link(number: u64) -> crate::drafts::GithubIssueLink {
    crate::drafts::GithubIssueLink {
        repository_owner: "acme".into(),
        repository_name: "widgets".into(),
        issue_number: number,
        issue_url: format!("https://github.com/acme/widgets/issues/{number}"),
        project_node_id: PROJECT.into(),
        claim_state: crate::drafts::GithubClaimState::Claimed,
    }
}

pub(super) fn pending(number: u64, draft_id: Option<&str>) -> GithubPendingClaim {
    GithubPendingClaim {
        repository_owner: "acme".into(),
        repository_name: "widgets".into(),
        issue_number: number,
        issue_url: format!("https://github.com/acme/widgets/issues/{number}"),
        project_node_id: PROJECT.into(),
        draft_id: draft_id.map(str::to_string),
        claimed_at: "2026-10-02T09:00:00Z".into(),
    }
}

pub(super) fn success(numbers: &[u64]) -> PollSuccess {
    let items: Vec<ProjectItem> = numbers.iter().map(|n| ready(*n)).collect();
    PollSuccess {
        repository: repo(),
        project_title: "Roadmap".into(),
        tasks: eligible_tasks(&items, &repo()),
        truncated: false,
    }
}

// ---------------------------------------------------------------------------
// A mock application driving the command bodies
// ---------------------------------------------------------------------------

/// A publication probe that finds every repository publishable, so the
/// polling repository resolves through GitHub publication without a network.
pub(super) struct OpenRepositories;

impl crate::github_publication::GithubIssues for OpenRepositories {
    fn probe(&self, _: &str, _: &str, _: &str) -> crate::github_publication::ProbeOutcome {
        crate::github_publication::ProbeOutcome::Publishable
    }
    fn find_by_marker(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<Option<crate::github_publication::IssueRef>, String> {
        unreachable!("polling never searches issues")
    }
    fn create_issue(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &crate::github_publication::client::IssueFields,
    ) -> Result<crate::github_publication::IssueRef, String> {
        unreachable!("polling never creates an issue")
    }
    fn update_issue(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: u64,
        _: &str,
        _: &str,
        _: &crate::github_publication::client::IssueFields,
    ) -> Result<crate::github_publication::IssueRef, String> {
        unreachable!("polling never edits an issue")
    }
    fn get_issue(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: u64,
    ) -> Result<Option<crate::github_publication::IssueRef>, String> {
        unreachable!("polling never reads a publication parent")
    }
    fn list_parent_issues(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &[String],
    ) -> Result<Vec<crate::github_publication::IssueRef>, String> {
        unreachable!("polling never lists publication parents")
    }
    fn list_issue_types(&self, _: &str, _: &str) -> Result<Vec<String>, String> {
        unreachable!("polling never lists issue Types")
    }
    fn list_milestones(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<Vec<crate::github_publication::PublicationMilestone>, String> {
        unreachable!("polling never lists milestones")
    }
    fn link_sub_issue(&self, _: &str, _: &str, _: &str, _: u64, _: u64, _: bool) -> Result<(), String> {
        unreachable!("polling never links a sub-issue")
    }
    fn unlink_sub_issue(&self, _: &str, _: &str, _: &str, _: u64, _: u64) -> Result<(), String> {
        unreachable!("polling never unlinks a sub-issue")
    }
}

pub(super) type Mock = tauri::test::MockRuntime;

/// A project in a real repository with a GitHub `origin`, opened in a mock
/// application whose Projects client is a fake.
pub(super) struct Harness {
    pub app: tauri::App<Mock>,
    pub dir: TempDir,
    pub store: TempDir,
    pub fake: Arc<FakeProjects>,
}

fn canonical(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

impl Harness {
    pub fn new() -> Self {
        Self::with_buffer(&crate::logging::BUFFER)
    }

    pub fn with_buffer(buffer: &'static crate::logging::LogBuffer) -> Self {
        let dir = project();
        let store = TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        repo.remote("origin", "https://github.com/acme/widgets.git").unwrap();
        let fake = Arc::new(FakeProjects::new());
        let app = tauri::test::mock_app();
        app.manage(crate::fs::FsAccessState::default());
        app.manage(crate::project::ProjectState::default());
        app.manage(crate::graduation::GraduationState::rooted_at(canonical(store.path())));
        app.manage(crate::github_publication::GithubIssuesSeam(Arc::new(OpenRepositories)));
        app.manage(GithubProjectsSeam(fake.clone()));
        app.manage(super::GithubPollingState::with_buffer(buffer));
        app.manage(super::TestSecret(SECRET.into()));
        let harness = Harness { app, dir, store, fake };
        harness.open(&harness.dir.path().to_path_buf());
        harness
    }

    pub fn handle(&self) -> tauri::AppHandle<Mock> {
        self.app.handle().clone()
    }

    /// Make `path` the open project's worktree.
    pub fn open(&self, path: &std::path::Path) {
        let root = canonical(path);
        let handle = self.handle();
        handle
            .state::<crate::fs::FsAccessState>()
            .install_for_worktree_and(&root, &canonical(self.store.path()))
            .unwrap();
        let access = handle.state::<crate::fs::FsAccessState>().get().unwrap();
        let state = handle.state::<crate::project::ProjectState>();
        state.set_root_with_access(root.clone(), Some(access));
        state.set_anchor(root.to_string_lossy().into_owned());
        state.set_store(root);
    }

    pub fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(self.dir.path())
    }

    pub fn select(&self, project: Option<&str>) -> GithubPollingView {
        super::set_settings_impl(&self.handle(), project.map(str::to_string), Some(5))
            .expect("settings saved")
    }

    pub fn view(&self) -> GithubPollingView {
        super::current_view(&self.handle()).expect("a view")
    }

    pub fn calls_matching(&self, f: impl Fn(&Call) -> bool) -> usize {
        self.fake.calls().iter().filter(|c| f(c)).count()
    }
}


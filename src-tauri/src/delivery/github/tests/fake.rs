//! The test doubles of the GitHub delivery plan: a tracker adapter that holds
//! a repository and a Project in memory and opens no socket (GDP-FR-UPXR), and
//! a mock application with a project open on it.
//!
//! The delivery source tests (`crate::delivery::source`) use the same
//! harness.

use std::sync::{Arc, Mutex};

use tauri::Manager;
use tempfile::TempDir;

use super::super::adapter::*;

pub const SECRET: &str = "ghp_secret_token_value";
pub const PROJECT: &str = "PVT_delivery1";
pub const PROJECT_URL: &str = "https://github.com/orgs/acme/projects/7";

/// What one Project item holds in the fake.
#[derive(Clone, Debug)]
pub enum Board {
    Issue(u64),
    PullRequest,
    Draft,
}

/// A test's call for each write of the fake, with the call text.
pub type WriteHook = Box<dyn FnMut(&str) + Send>;

/// The state of the fake tracker.
pub struct FakeState {
    pub issues: Vec<TrackerIssue>,
    pub milestones: Vec<TrackerMilestone>,
    pub board: Vec<(String, Option<String>, Board)>,
    pub status_field: Option<StatusFieldRead>,
    pub types: Vec<String>,
    /// The label names of the repository.
    pub labels: Vec<String>,
    /// The labels of each issue, by number.
    pub issue_labels: std::collections::HashMap<u64, Vec<String>>,
    pub title: String,
    /// Every call, in order, by name and its main argument.
    pub calls: Vec<String>,
    /// Fail every write after this many writes succeeded.
    pub writes_before_failure: Option<usize>,
    pub writes: usize,
    /// The start of the call text of one write whose answer is lost: the
    /// write takes effect, and the call answers `github_unreachable`. It
    /// applies once. Only `add_project_item` honours it.
    pub lost_answer: Option<String>,
    /// The highest issue number that the fake made, so a deleted issue never
    /// gives its number to a new one.
    pub last_issue: u64,
    /// Fail every read with this code.
    pub read_error: Option<String>,
    /// Fail `read_issue` of this issue alone with this code, so the other
    /// reads still answer.
    pub read_issue_error: Option<(u64, String)>,
    pub clock: u64,
    /// When set, `read_project` reads the board, reports that it was entered,
    /// and then waits for a release, so a test can hold a read in flight with
    /// the data of its start.
    pub pause: Option<(std::sync::mpsc::Sender<()>, std::sync::mpsc::Receiver<()>)>,
    /// The numbers of the repository's pull requests. `read_issue` answers
    /// `None` for one, as the `github` adapter does.
    pub pull_requests: Vec<u64>,
    /// Each `set_parent` call: the issue and the number of the current parent
    /// it named.
    pub current_parents: Vec<(u64, Option<u64>)>,
    /// Called with the call text of each write before the write runs, so a
    /// test can see what was stored before each request.
    pub on_write: Option<WriteHook>,
}

pub struct FakeTracker {
    pub repository: TrackerRepository,
    pub state: Mutex<FakeState>,
}

pub fn repository() -> TrackerRepository {
    TrackerRepository { host: "github.com".into(), owner: "acme".into(), name: "widgets".into() }
}

pub fn status_field() -> StatusFieldRead {
    let option = |id: &str, name: &str| StatusChoice { id: id.into(), name: name.into() };
    StatusFieldRead {
        field_id: "FIELD_status".into(),
        options: vec![
            option("OPT_todo", "Todo"),
            option("OPT_ready", "Ready"),
            option("OPT_progress", "In Progress"),
            option("OPT_done", "Done"),
        ],
    }
}

/// An open issue of the repository with the Type `issue_type`.
pub fn issue(number: u64, issue_type: Option<&str>) -> TrackerIssue {
    TrackerIssue {
        number,
        url: format!("https://github.com/acme/widgets/issues/{number}"),
        node_id: format!("I_{number}"),
        owner: "acme".into(),
        repo: "widgets".into(),
        title: format!("Issue {number}"),
        body: format!("Body {number}"),
        state: "open".into(),
        state_reason: None,
        issue_type: issue_type.map(str::to_string),
        milestone: None,
        parent: None,
        blocked_by: Vec::new(),
        assignees: Vec::new(),
        updated_at: format!("2026-01-01T00:00:{number:02}Z"),
    }
}

pub fn link(number: u64, state: &str) -> IssueLink {
    IssueLink {
        owner: "acme".into(),
        repo: "widgets".into(),
        number,
        url: format!("https://github.com/acme/widgets/issues/{number}"),
        title: format!("Issue {number}"),
        state: state.into(),
    }
}

pub fn milestone(number: u64, due: Option<&str>, state: &str) -> TrackerMilestone {
    TrackerMilestone {
        number,
        url: format!("https://github.com/acme/widgets/milestone/{number}"),
        title: format!("Milestone {number}"),
        description: String::new(),
        state: state.into(),
        due: due.map(str::to_string),
        updated_at: format!("2026-01-02T00:00:{number:02}Z"),
    }
}

impl FakeTracker {
    pub fn new() -> Self {
        FakeTracker {
            repository: repository(),
            state: Mutex::new(FakeState {
                issues: Vec::new(),
                milestones: Vec::new(),
                board: Vec::new(),
                status_field: Some(status_field()),
                types: vec!["Epic".into(), "Feature".into(), "Task".into(), "Bug".into()],
                labels: vec!["bug".into(), "docs".into(), "Needs Review".into()],
                issue_labels: std::collections::HashMap::new(),
                title: "Roadmap".into(),
                calls: Vec::new(),
                writes_before_failure: None,
                writes: 0,
                lost_answer: None,
                last_issue: 0,
                read_error: None,
                read_issue_error: None,
                clock: 0,
                pause: None,
                pull_requests: Vec::new(),
                current_parents: Vec::new(),
                on_write: None,
            }),
        }
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, FakeState> {
        self.state.lock().unwrap()
    }

    /// Put `issue` in the repository and on the board with `status`.
    pub fn board_issue(&self, issue: TrackerIssue, status: Option<&str>) {
        let mut s = self.lock();
        let number = issue.number;
        s.issues.push(issue);
        s.board.push((format!("ITEM_{number}"), status.map(str::to_string), Board::Issue(number)));
    }

    pub fn calls(&self) -> Vec<String> {
        self.lock().calls.clone()
    }

    pub fn writes(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| !c.starts_with("read") && !c.starts_with("list"))
            .collect()
    }

    fn read(&self, call: String) -> Result<std::sync::MutexGuard<'_, FakeState>, String> {
        let mut s = self.lock();
        s.calls.push(call);
        match s.read_error.clone() {
            Some(code) => Err(code),
            None => Ok(s),
        }
    }

    /// Record a write, and fail it where the failure point is reached. A
    /// successful write moves the clock.
    fn write(&self, call: String) -> Result<std::sync::MutexGuard<'_, FakeState>, String> {
        let mut s = self.lock();
        if let Some(hook) = s.on_write.as_mut() {
            hook(&call);
        }
        s.calls.push(call);
        if s.writes_before_failure.is_some_and(|n| s.writes >= n) {
            return Err(crate::github_polling::records::ERR_REQUEST_FAILED.into());
        }
        s.writes += 1;
        s.clock += 1;
        Ok(s)
    }
}

impl FakeState {
    /// A timestamp that is different for each value of the clock.
    fn stamp(&self) -> String {
        let c = self.clock;
        format!(
            "2026-06-{:02}T{:02}:{:02}:{:02}Z",
            1 + c / 86_400,
            c / 3600 % 24,
            c / 60 % 60,
            c % 60
        )
    }

    /// Lose the answer of `call` where `lost_answer` names it.
    fn lose(&mut self, call: &str) -> Result<(), String> {
        if self.lost_answer.as_deref().is_some_and(|p| call.starts_with(p)) {
            self.lost_answer = None;
            return Err(crate::github_polling::records::ERR_GITHUB_UNREACHABLE.into());
        }
        Ok(())
    }

    fn issue_mut(&mut self, number: u64) -> Result<&mut TrackerIssue, String> {
        let stamp = self.stamp();
        let issue =
            self.issues.iter_mut().find(|i| i.number == number).ok_or("github_request_failed")?;
        issue.updated_at = stamp;
        Ok(issue)
    }

    fn milestone_mut(&mut self, number: u64) -> Result<&mut TrackerMilestone, String> {
        let stamp = self.stamp();
        let m = self
            .milestones
            .iter_mut()
            .find(|m| m.number == number)
            .ok_or("github_request_failed")?;
        m.updated_at = stamp;
        Ok(m)
    }

    fn next_number(&self) -> u64 {
        let issues = self.issues.iter().map(|i| i.number).max().unwrap_or(0);
        issues.max(self.last_issue).max(100) + 1
    }
}

impl FakeTracker {
    /// The Project as the fake holds it now.
    fn project_now(&self, project_id: &str) -> Result<ProjectRead, String> {
        let s = self.read(format!("read_project {project_id}"))?;
        let items = s
            .board
            .iter()
            .map(|(item_id, status, content)| ProjectItemRead {
                item_id: item_id.clone(),
                status: status.clone(),
                content: match content {
                    Board::Issue(n) => s
                        .issues
                        .iter()
                        .find(|i| i.number == *n)
                        .cloned()
                        .map(ItemContent::Issue)
                        .unwrap_or(ItemContent::Draft),
                    Board::PullRequest => ItemContent::PullRequest,
                    Board::Draft => ItemContent::Draft,
                },
            })
            .collect();
        Ok(ProjectRead {
            node_id: project_id.into(),
            title: s.title.clone(),
            short_description: "The plan".into(),
            owner_login: "acme".into(),
            number: 7,
            url: PROJECT_URL.into(),
            status_field: s.status_field.clone(),
            items,
        })
    }
}

impl TrackerAdapter for FakeTracker {
    fn repository(&self) -> &TrackerRepository {
        &self.repository
    }

    fn read_project(&self, project_id: &str) -> Result<ProjectRead, String> {
        let pause = self.lock().pause.take();
        let read = self.project_now(project_id);
        // The pause comes after the read of the board, so a paused read holds
        // the Project as it was when the read began.
        if let Some((entered, release)) = pause {
            entered.send(()).unwrap();
            release.recv().unwrap();
        }
        read
    }

    fn read_milestones(&self) -> Result<Vec<TrackerMilestone>, String> {
        Ok(self.read("read_milestones".into())?.milestones.clone())
    }

    fn read_issue(&self, number: u64) -> Result<Option<TrackerIssue>, String> {
        let s = self.read(format!("read_issue {number}"))?;
        if let Some((_, code)) = s.read_issue_error.clone().filter(|(n, _)| *n == number) {
            return Err(code);
        }
        if s.pull_requests.contains(&number) {
            return Ok(None);
        }
        Ok(s.issues.iter().find(|i| i.number == number).cloned())
    }

    fn read_milestone(&self, number: u64) -> Result<Option<TrackerMilestone>, String> {
        let s = self.read(format!("read_milestone {number}"))?;
        Ok(s.milestones.iter().find(|m| m.number == number).cloned())
    }

    fn list_issue_types(&self) -> Result<Vec<String>, String> {
        Ok(self.read("list_issue_types".into())?.types.clone())
    }

    fn create_issue(&self, new: &NewIssue) -> Result<TrackerIssue, String> {
        let mut s = self.write("create_issue".into())?;
        let number = s.next_number();
        s.last_issue = number;
        let mut created = issue(number, new.issue_type.as_deref());
        created.title = new.title.clone();
        created.body = new.body.clone();
        created.milestone = new.milestone;
        created.updated_at = s.stamp();
        s.issues.push(created.clone());
        if !new.labels.is_empty() {
            s.issue_labels.insert(number, new.labels.clone());
        }
        Ok(created)
    }

    fn update_issue(&self, number: u64, change: &IssueChange) -> Result<(), String> {
        let mut s = self.write(format!("update_issue {number}"))?;
        let issue = s.issue_mut(number)?;
        if let Some(t) = &change.title {
            issue.title = t.clone();
        }
        if let Some(b) = &change.body {
            issue.body = b.clone();
        }
        if let Some(t) = &change.issue_type {
            issue.issue_type = Some(t.clone());
        }
        Ok(())
    }

    fn set_parent(
        &self,
        number: u64,
        current: Option<&IssueLink>,
        parent: Option<u64>,
    ) -> Result<(), String> {
        let mut s = self.write(format!("set_parent {number} {parent:?}"))?;
        s.current_parents.push((number, current.map(|c| c.number)));
        s.issue_mut(number)?.parent = parent.map(|p| link(p, "open"));
        Ok(())
    }

    fn set_milestone(&self, number: u64, milestone: Option<u64>) -> Result<(), String> {
        let mut s = self.write(format!("set_milestone {number} {milestone:?}"))?;
        s.issue_mut(number)?.milestone = milestone;
        Ok(())
    }

    fn add_blocked_by(&self, number: u64, blocker: u64) -> Result<(), String> {
        let mut s = self.write(format!("add_blocked_by {number} {blocker}"))?;
        s.issue_mut(number)?.blocked_by.push(link(blocker, "open"));
        Ok(())
    }

    fn remove_blocked_by(&self, number: u64, blocker: u64) -> Result<(), String> {
        let mut s = self.write(format!("remove_blocked_by {number} {blocker}"))?;
        s.issue_mut(number)?.blocked_by.retain(|b| b.number != blocker);
        Ok(())
    }

    fn add_project_item(&self, project_id: &str, issue_node_id: &str) -> Result<String, String> {
        let call = format!("add_project_item {project_id} {issue_node_id}");
        let mut s = self.write(call.clone())?;
        let number: u64 =
            issue_node_id.trim_start_matches("I_").parse().map_err(|_| "github_request_failed")?;
        let item = format!("ITEM_{number}");
        s.board.push((item.clone(), None, Board::Issue(number)));
        s.lose(&call)?;
        Ok(item)
    }

    fn remove_project_item(&self, project_id: &str, item_id: &str) -> Result<(), String> {
        let mut s = self.write(format!("remove_project_item {project_id} {item_id}"))?;
        s.board.retain(|(id, _, _)| id != item_id);
        Ok(())
    }

    fn create_milestone(&self, new: &NewMilestone) -> Result<TrackerMilestone, String> {
        let mut s = self.write("create_milestone".into())?;
        let number = s.milestones.iter().map(|m| m.number).max().unwrap_or(0) + 1;
        let mut m = milestone(number, new.due.as_deref(), "open");
        m.title = new.title.clone();
        m.description = new.description.clone();
        m.updated_at = s.stamp();
        s.milestones.push(m.clone());
        Ok(m)
    }

    fn update_milestone(&self, number: u64, change: &MilestoneChange) -> Result<(), String> {
        let mut s = self.write(format!("update_milestone {number}"))?;
        let m = s.milestone_mut(number)?;
        if let Some(t) = &change.title {
            m.title = t.clone();
        }
        if let Some(d) = &change.description {
            m.description = d.clone();
        }
        if let Some(due) = &change.due {
            m.due = due.clone();
        }
        Ok(())
    }

    fn close_milestone(&self, number: u64) -> Result<(), String> {
        let mut s = self.write(format!("close_milestone {number}"))?;
        s.milestone_mut(number)?.state = "closed".into();
        Ok(())
    }

    fn close_issue(&self, number: u64, reason: CloseWith) -> Result<(), String> {
        let mut s = self.write(format!("close_issue {number} {reason:?}"))?;
        let issue = s.issue_mut(number)?;
        issue.state = "closed".into();
        issue.state_reason = Some(match reason {
            CloseWith::Completed => StateReason::Completed,
            CloseWith::NotPlanned => StateReason::NotPlanned,
        });
        Ok(())
    }

    fn list_labels(&self) -> Result<Vec<String>, String> {
        Ok(self.read("list_labels".into())?.labels.clone())
    }

    fn read_status_field(&self, project_id: &str) -> Result<Option<StatusFieldRead>, String> {
        Ok(self.read(format!("read_status_field {project_id}"))?.status_field.clone())
    }

    fn read_issue_detail(
        &self,
        number: u64,
        project_id: &str,
    ) -> Result<Option<IssueDetail>, String> {
        let s = self.read(format!("read_issue_detail {number} {project_id}"))?;
        if s.pull_requests.contains(&number) {
            return Ok(None);
        }
        let Some(issue) = s.issues.iter().find(|i| i.number == number) else { return Ok(None) };
        let status = s
            .board
            .iter()
            .find(|(_, _, b)| matches!(b, Board::Issue(n) if *n == number))
            .and_then(|(_, status, _)| status.clone());
        Ok(Some(IssueDetail {
            number,
            url: issue.url.clone(),
            owner: issue.owner.clone(),
            repo: issue.repo.clone(),
            title: issue.title.clone(),
            body: issue.body.clone(),
            state: issue.state.clone(),
            state_reason: issue.state_reason,
            issue_type: issue.issue_type.clone(),
            labels: s.issue_labels.get(&number).cloned().unwrap_or_default(),
            milestone: issue.milestone,
            milestone_title: issue
                .milestone
                .and_then(|m| s.milestones.iter().find(|x| x.number == m))
                .map(|m| m.title.clone()),
            parent: issue.parent.clone(),
            sub_issues: s
                .issues
                .iter()
                .filter(|i| i.parent.as_ref().is_some_and(|p| p.number == number))
                .map(|i| link(i.number, &i.state))
                .collect(),
            blocked_by: issue.blocked_by.clone(),
            status,
            updated_at: issue.updated_at.clone(),
        }))
    }

    fn add_labels(&self, number: u64, labels: &[String]) -> Result<(), String> {
        let mut s = self.write(format!("add_labels {number} {}", labels.join(",")))?;
        s.issue_mut(number)?;
        let held = s.issue_labels.entry(number).or_default();
        for label in labels {
            if !held.contains(label) {
                held.push(label.clone());
            }
        }
        Ok(())
    }

    fn remove_label(&self, number: u64, label: &str) -> Result<(), String> {
        let mut s = self.write(format!("remove_label {number} {label}"))?;
        s.issue_mut(number)?;
        s.issue_labels.entry(number).or_default().retain(|l| l != label);
        Ok(())
    }

    fn set_project_status(
        &self,
        project_id: &str,
        item_id: &str,
        field_id: &str,
        option_id: &str,
    ) -> Result<(), String> {
        let mut s = self
            .write(format!("set_project_status {project_id} {item_id} {field_id} {option_id}"))?;
        let name = s
            .status_field
            .as_ref()
            .filter(|f| f.field_id == field_id)
            .and_then(|f| f.options.iter().find(|o| o.id == option_id))
            .map(|o| o.name.clone())
            .ok_or("github_request_failed")?;
        let entry =
            s.board.iter_mut().find(|(id, _, _)| id == item_id).ok_or("github_request_failed")?;
        entry.1 = Some(name);
        Ok(())
    }
}

/// The connector that hands out the one fake.
pub struct FakeConnector(pub Arc<FakeTracker>);

impl TrackerConnector for FakeConnector {
    fn connect(&self, repository: &TrackerRepository, secret: &str) -> Arc<dyn TrackerAdapter> {
        // GDP-FR-ZGJB: the polling repository, with the project's secret.
        assert_eq!(secret, SECRET, "the adapter gets the project's secret");
        assert_eq!(
            *repository,
            self::repository(),
            "the adapter is bound to the polling repository"
        );
        self.0.clone()
    }
}

/// A publication probe that finds every repository publishable, so the
/// polling repository resolves without a network.
pub struct OpenRepositories;

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
        unreachable!("the delivery plan never searches issues")
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
        unreachable!("the delivery plan writes through its own adapter")
    }
    fn update_issue(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: u64,
        _: &crate::github_publication::client::IssueEdit<'_>,
    ) -> Result<crate::github_publication::IssueRef, String> {
        unreachable!("the delivery plan writes through its own adapter")
    }
    fn get_issue(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: u64,
    ) -> Result<Option<crate::github_publication::IssueRef>, String> {
        unreachable!("the delivery plan reads through its own adapter")
    }
    fn list_parent_issues(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &[String],
    ) -> Result<Vec<crate::github_publication::IssueRef>, String> {
        unreachable!("the delivery plan reads through its own adapter")
    }
    fn list_issue_types(&self, _: &str, _: &str) -> Result<Vec<String>, String> {
        unreachable!("the delivery plan reads through its own adapter")
    }
    fn list_milestones(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<Vec<crate::github_publication::PublicationMilestone>, String> {
        unreachable!("the delivery plan reads through its own adapter")
    }
    fn list_labels(
        &self,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<Vec<crate::github_publication::PublicationLabel>, String> {
        unreachable!("the delivery plan reads through its own adapter")
    }
    fn link_sub_issue(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: u64,
        _: u64,
        _: bool,
    ) -> Result<(), String> {
        unreachable!("the delivery plan writes through its own adapter")
    }
    fn unlink_sub_issue(&self, _: &str, _: &str, _: &str, _: u64, _: u64) -> Result<(), String> {
        unreachable!("the delivery plan writes through its own adapter")
    }
}

pub type Mock = tauri::test::MockRuntime;

fn canonical(path: &std::path::Path) -> std::path::PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// A project in a Git repository with a GitHub `origin`, opened in a mock
/// application whose tracker adapter is a fake.
pub struct Harness {
    pub app: tauri::App<Mock>,
    pub dir: TempDir,
    pub store: TempDir,
    pub fake: Arc<FakeTracker>,
}

impl Harness {
    pub fn new() -> Self {
        Self::with_buffer(&crate::logging::BUFFER)
    }

    pub fn with_buffer(buffer: &'static crate::logging::LogBuffer) -> Self {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
        let store = TempDir::new().unwrap();
        let repo = git2::Repository::init(dir.path()).unwrap();
        repo.remote("origin", "https://github.com/acme/widgets.git").unwrap();
        let fake = Arc::new(FakeTracker::new());
        let app = tauri::test::mock_app();
        app.manage(crate::fs::FsAccessState::default());
        app.manage(crate::project::ProjectState::default());
        app.manage(crate::graduation::GraduationState::rooted_at(canonical(store.path())));
        app.manage(crate::github_publication::GithubIssuesSeam(Arc::new(OpenRepositories)));
        app.manage(crate::github_polling::GithubPollingState::with_buffer(buffer));
        app.manage(crate::github_polling::TestSecret(SECRET.into()));
        app.manage(TrackerSeam(Arc::new(FakeConnector(fake.clone()))));
        app.manage(super::super::GithubDeliveryState::with_buffer(buffer));
        app.manage(crate::artifacts::ContentTracker::default());
        let harness = Harness { app, dir, store, fake };
        harness.open();
        harness
    }

    pub fn handle(&self) -> tauri::AppHandle<Mock> {
        self.app.handle().clone()
    }

    fn open(&self) {
        let root = canonical(self.dir.path());
        let handle = self.handle();
        handle
            .state::<crate::fs::FsAccessState>()
            .install_for_worktree_and(&root, &canonical(self.store.path()))
            .unwrap();
        let access = handle.state::<crate::fs::FsAccessState>().get().unwrap();
        let state = handle.state::<crate::project::ProjectState>();
        state.set_root_with_access(root.clone(), Some(access));
        state.set_anchor(root.to_string_lossy().into_owned());
        state.set_store(canonical(self.store.path()));
    }

    /// Close the project, so each operation refuses with `no_project_open`.
    pub fn close(&self) {
        self.handle().state::<crate::project::ProjectState>().clear_root();
    }

    pub fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(canonical(self.dir.path()))
    }

    pub fn machine_store(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(canonical(self.store.path()))
    }

    /// Select the Project for polling, as the GitHub Project settings do.
    pub fn select_project(&self, project: Option<&str>) {
        let settings = crate::project_settings::GithubPollingSettings {
            project_node_id: project.map(str::to_string),
            interval_minutes: None,
            ready_status: None,
            in_progress_status: None,
            blocked_status: None,
        };
        crate::project_settings::save_github_polling_settings_to(&self.root(), &settings).unwrap();
    }

    pub fn delivery_settings(&self) -> crate::delivery::settings::DeliverySettings {
        crate::project_settings::load_delivery_settings_from(&self.root()).unwrap()
    }

    pub fn save_delivery_settings(&self, settings: &crate::delivery::settings::DeliverySettings) {
        crate::project_settings::save_delivery_settings_to(&self.root(), settings).unwrap();
    }

    /// Write a file of the project.
    pub fn write(&self, rel: &str, body: &str) {
        let path = self.dir.path().join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    /// Read a file of the project, as stored.
    pub fn read(&self, rel: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(rel)).unwrap()
    }

    /// The read cache of the selected Project, as stored.
    pub fn cache(&self) -> Option<super::super::cache::TrackerRead> {
        super::super::cache::read_cache(
            &self.handle(),
            &crate::logging::BUFFER,
            &self.machine_store(),
            PROJECT,
        )
        .unwrap()
    }

    /// A GitHub delivery source with a plan of an epic (#1), a feature under
    /// it (#2), and a task under the feature (#3) in milestone 1.
    pub fn github_plan() -> Self {
        let h = Self::new();
        h.select_project(Some(PROJECT));
        h.fake.lock().milestones.push(milestone(1, Some("2026-12-01"), "open"));
        h.fake.board_issue(issue(1, Some("Epic")), Some("Todo"));
        let mut feature = issue(2, Some("Feature"));
        feature.parent = Some(link(1, "open"));
        h.fake.board_issue(feature, Some("Todo"));
        let mut task = issue(3, Some("Task"));
        task.parent = Some(link(2, "open"));
        task.milestone = Some(1);
        h.fake.board_issue(task, Some("Ready"));
        h
    }
}

/// Listen for one event on the mock application, and collect its payloads.
pub fn listen(app: &tauri::AppHandle<Mock>, event: &str) -> Arc<Mutex<Vec<String>>> {
    use tauri::Listener;
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    app.listen(event, move |e| sink.lock().unwrap().push(e.payload().to_string()));
    seen
}

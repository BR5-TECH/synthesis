//! The tests of GitHub publication
//! (`../../../specifications/core/GHP-github-publication.md`).
//!
//! Everything runs against a real temporary project through a real
//! [`crate::fs::RootFs`] and a **fake** issues client, because almost every
//! requirement here is a statement about what is on disk and about which GitHub
//! calls were made: an attempt record written before the first request, a
//! search performed before every create, a history that only ever grows.

use std::sync::Mutex;

use tempfile::TempDir;

use super::client::{self, GithubIssues, IssueRef, ProbeOutcome};
use super::*;

mod attempts;
mod client_rules;
mod eligibility;
mod enterprise_hosts;
mod github_shadow;
mod legacy_status;
mod machine_store;
mod machine_store_legacy;
mod metadata_cache_rules;
mod metadata_refresh;
mod metadata_schedule;
mod note_publication;
mod publication_choice;
mod publication_commands;
mod publishing;
mod remote_selection;
mod update;

// ---------------------------------------------------------------------------
// Harness
// ---------------------------------------------------------------------------

pub(super) fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    std::fs::create_dir_all(dir.path().join(".synthesis")).unwrap();
    dir
}

pub(super) fn draft(root: &crate::fs::RootFs, name: &str) -> String {
    crate::drafts::create_draft_impl(root, Some(name), None).unwrap().draft.id
}

pub(super) fn save_prompt(root: &crate::fs::RootFs, id: &str, body: &str) {
    let prompt = crate::drafts::require_prompt(root, id).unwrap();
    crate::drafts::save_draft_file_impl(root, id, &prompt, body).unwrap();
}

/// The draft's publication store in the repository machine store.
///
/// A test passes the project root as that store too, as
/// `ProjectState::set_root` does, so the file sits at
/// `<root>/drafts/<id>/publication.toml`: beside `.synthesis/`, and apart from
/// the legacy store inside the draft's own folder (DRS-FR-EJBM, DRS-FR-ULKN).
pub(super) fn machine_store_path(root: &crate::fs::RootFs, id: &str) -> std::path::PathBuf {
    root.path().join("drafts").join(id).join("publication.toml")
}

/// What `list_drafts` returns: the walk, and the publication records the
/// command attaches from the machine store (DRS-FR-PXHW).
pub(super) fn listed_with_publication(root: &crate::fs::RootFs) -> crate::drafts::DraftHierarchy {
    let mut listed = crate::drafts::list_drafts_impl(root);
    crate::drafts::attach_publication(root, root, &mut listed.drafts);
    listed
}

/// DRS-FR-VKQO: no operation sets `published` any more, so a test writes the
/// legacy value into the stored record directly, as an older build left it.
pub(super) fn store_legacy_published(root: &crate::fs::RootFs, id: &str) {
    let path =
        crate::drafts::draft_publication_path(root, id).unwrap().with_file_name("draft.toml");
    let text = root.read_text(&path).unwrap();
    let legacy = text.replace("status = \"active\"", "status = \"published\"");
    assert_ne!(legacy, text, "the record holds no `active` status to replace");
    root.write_text_atomic(&path, &legacy).unwrap();
}

/// One successful publication record in the draft's store, written without
/// GitHub.
pub(super) fn record_publication(root: &crate::fs::RootFs, id: &str, marker: &str, number: u64) {
    let attempt = flow::open_attempt(root, root, id, &origin(), marker.to_string()).unwrap();
    let issue = IssueRef {
        number,
        url: format!("https://github.com/acme/widgets/issues/{number}"),
        title: "widget".into(),
        body: "body".into(),
        ..Default::default()
    };
    store::complete_attempt(root, root, id, flow::record_of(&attempt, &issue)).unwrap();
}

/// One recorded GitHub call, so a test can assert what reached the wire and in
/// what order — which is the whole of the duplicate-issue guarantee.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Call {
    Probe(String),
    Search(String),
    Create(String),
    Update(u64),
    /// A parent read (`get_issue`).
    Read(u64),
    /// `link_sub_issue(parent, sub_issue_id, replace_parent)`.
    Link(u64, u64, bool),
    /// `unlink_sub_issue(parent, sub_issue_id)`.
    Unlink(u64, u64),
    ListParents,
    ListTypes,
    ListMilestones,
    ListLabels,
}

/// A fake issues client. Nothing here opens a socket.
pub(super) struct FakeGithub {
    pub calls: Mutex<Vec<Call>>,
    /// The issues the repository already holds, in creation order. A test seeds
    /// parents here too.
    pub issues: Mutex<Vec<IssueRef>>,
    pub probe: Mutex<ProbeOutcome>,
    /// When set, every search fails with this typed code.
    pub search_error: Mutex<Option<String>>,
    /// When set, every create fails with this typed code.
    pub create_error: Mutex<Option<String>>,
    /// When set, every link and unlink fails with this typed code.
    pub link_error: Mutex<Option<String>>,
    /// When set, every issue read fails with this typed code.
    pub read_error: Mutex<Option<String>>,
    /// When set, every edit fails with this typed code.
    pub update_error: Mutex<Option<String>>,
    /// Runs after an edit is accepted, so a test can change the store while
    /// the edit is in flight.
    #[allow(clippy::type_complexity)] // One optional callback; a type alias says no more.
    pub after_update: Mutex<Option<Box<dyn Fn() + Send>>>,
    pub next_number: Mutex<u64>,
    /// The Types the owner holds; `None` makes the read fail.
    pub types: Mutex<Option<Vec<String>>>,
    /// The open milestones; `None` makes the read fail.
    pub milestones: Mutex<Option<Vec<PublicationMilestone>>>,
    /// The labels of the repository; `None` makes the read fail.
    pub labels: Mutex<Option<Vec<PublicationLabel>>>,
    /// When true, the parent list read fails.
    pub parents_fail: Mutex<bool>,
    /// The Type and milestone fields of every create and edit, in order.
    pub fields: Mutex<Vec<client::IssueFields>>,
    /// The Type filter of every parent list read, in order.
    pub parent_filters: Mutex<Vec<Vec<String>>>,
}

impl Default for FakeGithub {
    fn default() -> Self {
        FakeGithub {
            calls: Mutex::default(),
            issues: Mutex::default(),
            probe: Mutex::new(ProbeOutcome::Publishable),
            search_error: Mutex::default(),
            create_error: Mutex::default(),
            link_error: Mutex::default(),
            read_error: Mutex::default(),
            update_error: Mutex::default(),
            after_update: Mutex::default(),
            next_number: Mutex::new(1),
            types: Mutex::new(Some(vec!["Feature".into(), "Task".into(), "Bug".into()])),
            milestones: Mutex::new(Some(vec![
                PublicationMilestone { number: 7, title: "v1.2".into() },
                PublicationMilestone { number: 8, title: "v1.3".into() },
            ])),
            labels: Mutex::new(Some(vec![
                PublicationLabel { name: "bug".into(), color: "d73a4a".into(), description: None },
                PublicationLabel {
                    name: "ui".into(),
                    color: "0e8a16".into(),
                    description: Some("User interface".into()),
                },
            ])),
            parents_fail: Mutex::default(),
            fields: Mutex::default(),
            parent_filters: Mutex::default(),
        }
    }
}

impl FakeGithub {
    pub fn publishable() -> Self {
        // No `forget_probes()` here. That clears the whole process-wide cache,
        // not this fake's key, so building a fake on one test thread would drop
        // the entry another test is counting requests against. Every case below
        // passes `cache: false` instead, and the one case that does exercise the
        // cache clears it itself and owns a key no other case uses.
        FakeGithub::default()
    }

    pub fn with_probe(outcome: ProbeOutcome) -> Self {
        let fake = FakeGithub::publishable();
        *fake.probe.lock().unwrap() = outcome;
        fake
    }

    pub fn calls(&self) -> Vec<Call> {
        self.calls.lock().unwrap().clone()
    }

    fn record(&self, call: Call) {
        self.calls.lock().unwrap().push(call);
    }

    /// Seed an open issue of the given Type and milestone, as a parent
    /// candidate. Returns its number.
    pub fn seed(&self, issue_type: &str, milestone: Option<(u64, &str)>) -> u64 {
        let mut next = self.next_number.lock().unwrap();
        let number = *next;
        *next += 1;
        self.issues.lock().unwrap().push(IssueRef {
            number,
            id: number + 1000,
            url: format!("https://github.com/acme/widgets/issues/{number}"),
            title: format!("Seeded {number}"),
            open: true,
            issue_type: Some(issue_type.to_string()),
            milestone: milestone
                .map(|(number, title)| PublicationMilestone { number, title: title.to_string() }),
            ..Default::default()
        });
        number
    }

    /// Change one seeded issue.
    pub fn edit(&self, number: u64, change: impl FnOnce(&mut IssueRef)) {
        let mut issues = self.issues.lock().unwrap();
        change(issues.iter_mut().find(|i| i.number == number).expect("seeded issue"));
    }

    pub fn issue(&self, number: u64) -> IssueRef {
        self.issues.lock().unwrap().iter().find(|i| i.number == number).cloned().expect("issue")
    }
}

/// The fake derives `Default`, and the probe outcome it starts from is the one
/// that lets a test say nothing about eligibility when eligibility is not what
/// it is asserting.
impl Default for ProbeOutcome {
    fn default() -> Self {
        ProbeOutcome::Publishable
    }
}

impl GithubIssues for FakeGithub {
    fn probe(&self, _secret: &str, owner: &str, repo: &str) -> ProbeOutcome {
        self.record(Call::Probe(format!("{owner}/{repo}")));
        *self.probe.lock().unwrap()
    }

    fn find_by_marker(
        &self,
        _secret: &str,
        owner: &str,
        repo: &str,
        marker: &str,
    ) -> Result<Option<IssueRef>, String> {
        self.record(Call::Search(format!("{owner}/{repo}:{marker}")));
        if let Some(error) = self.search_error.lock().unwrap().clone() {
            return Err(error);
        }
        Ok(self.issues.lock().unwrap().iter().find(|i| i.body.contains(marker)).cloned())
    }

    fn create_issue(
        &self,
        _secret: &str,
        owner: &str,
        repo: &str,
        title: &str,
        body: &str,
        fields: &client::IssueFields,
    ) -> Result<IssueRef, String> {
        self.record(Call::Create(format!("{owner}/{repo}")));
        self.fields.lock().unwrap().push(fields.clone());
        if let Some(error) = self.create_error.lock().unwrap().clone() {
            return Err(error);
        }
        let mut next = self.next_number.lock().unwrap();
        let milestone = fields.milestone.and_then(|number| {
            self.milestones
                .lock()
                .unwrap()
                .as_ref()
                .and_then(|all| all.iter().find(|m| m.number == number).cloned())
        });
        let issue = IssueRef {
            number: *next,
            id: *next + 1000,
            url: format!("https://github.com/{owner}/{repo}/issues/{next}"),
            title: title.to_string(),
            body: body.to_string(),
            open: true,
            issue_type: fields.issue_type.clone(),
            milestone,
            ..Default::default()
        };
        *next += 1;
        self.issues.lock().unwrap().push(issue.clone());
        Ok(issue)
    }

    fn update_issue(
        &self,
        _secret: &str,
        _owner: &str,
        _repo: &str,
        number: u64,
        edit: &client::IssueEdit<'_>,
    ) -> Result<IssueRef, String> {
        let client::IssueEdit { title, body, fields } = *edit;
        self.record(Call::Update(number));
        self.fields.lock().unwrap().push(fields.clone());
        if let Some(error) = self.update_error.lock().unwrap().clone() {
            return Err(error);
        }
        let milestones = self.milestones.lock().unwrap().clone().unwrap_or_default();
        let mut issues = self.issues.lock().unwrap();
        let issue = issues
            .iter_mut()
            .find(|i| i.number == number)
            .ok_or_else(|| ERR_ISSUE_UPDATE_FAILED.to_string())?;
        issue.title = title.to_string();
        issue.body = body.to_string();
        if let Some(issue_type) = &fields.issue_type {
            issue.issue_type = Some(issue_type.clone());
        }
        if let Some(wanted) = fields.milestone {
            issue.milestone = milestones.into_iter().find(|m| m.number == wanted);
        }
        let edited = issue.clone();
        drop(issues);
        if let Some(after) = self.after_update.lock().unwrap().as_ref() {
            after();
        }
        Ok(edited)
    }

    fn get_issue(
        &self,
        _secret: &str,
        _owner: &str,
        _repo: &str,
        number: u64,
    ) -> Result<Option<IssueRef>, String> {
        self.record(Call::Read(number));
        if let Some(error) = self.read_error.lock().unwrap().clone() {
            return Err(error);
        }
        Ok(self.issues.lock().unwrap().iter().find(|i| i.number == number).cloned())
    }

    fn list_parent_issues(
        &self,
        _secret: &str,
        _owner: &str,
        _repo: &str,
        types: &[String],
    ) -> Result<Vec<IssueRef>, String> {
        self.record(Call::ListParents);
        self.parent_filters.lock().unwrap().push(types.to_vec());
        if *self.parents_fail.lock().unwrap() {
            return Err(ERR_PARENT_ISSUES_UNREADABLE.to_string());
        }
        Ok(self
            .issues
            .lock()
            .unwrap()
            .iter()
            .filter(|i| client::is_parent_candidate(i, types))
            .cloned()
            .collect())
    }

    fn list_issue_types(&self, _secret: &str, _owner: &str) -> Result<Vec<String>, String> {
        self.record(Call::ListTypes);
        self.types.lock().unwrap().clone().ok_or_else(|| ERR_ISSUE_TYPES_UNREADABLE.to_string())
    }

    fn list_milestones(
        &self,
        _secret: &str,
        _owner: &str,
        _repo: &str,
    ) -> Result<Vec<PublicationMilestone>, String> {
        self.record(Call::ListMilestones);
        self.milestones.lock().unwrap().clone().ok_or_else(|| ERR_MILESTONES_UNREADABLE.to_string())
    }

    fn list_labels(
        &self,
        _secret: &str,
        _owner: &str,
        _repo: &str,
    ) -> Result<Vec<PublicationLabel>, String> {
        self.record(Call::ListLabels);
        self.labels.lock().unwrap().clone().ok_or_else(|| ERR_LABELS_UNREADABLE.to_string())
    }

    fn link_sub_issue(
        &self,
        _secret: &str,
        owner: &str,
        repo: &str,
        parent_number: u64,
        sub_issue_id: u64,
        replace_parent: bool,
    ) -> Result<(), String> {
        self.record(Call::Link(parent_number, sub_issue_id, replace_parent));
        if let Some(error) = self.link_error.lock().unwrap().clone() {
            return Err(error);
        }
        let mut issues = self.issues.lock().unwrap();
        if let Some(sub) = issues.iter_mut().find(|i| i.id == sub_issue_id) {
            sub.parent = Some(client::ParentRef {
                owner: owner.to_string(),
                repo: repo.to_string(),
                number: parent_number,
            });
        }
        Ok(())
    }

    fn unlink_sub_issue(
        &self,
        _secret: &str,
        _owner: &str,
        _repo: &str,
        parent_number: u64,
        sub_issue_id: u64,
    ) -> Result<(), String> {
        self.record(Call::Unlink(parent_number, sub_issue_id));
        if let Some(error) = self.link_error.lock().unwrap().clone() {
            return Err(error);
        }
        let mut issues = self.issues.lock().unwrap();
        if let Some(sub) = issues.iter_mut().find(|i| i.id == sub_issue_id) {
            sub.parent = None;
        }
        Ok(())
    }
}

/// The one eligible GitHub remote most tests publish to.
pub(super) fn origin() -> PublicationRemote {
    PublicationRemote {
        repository_host: Some("github.com".to_string()),
        name: "origin".to_string(),
        url: "github.com/acme/widgets".to_string(),
        kind: RemoteKind::Github,
        repository_owner: Some("acme".to_string()),
        repository_name: Some("widgets".to_string()),
        eligibility: RemoteEligibility::Eligible,
        reason: None,
        tls_failure: None,
    }
}

pub(super) fn configured(name: &str, url: &str) -> ConfiguredRemote {
    ConfiguredRemote { name: name.to_string(), url: url.to_string() }
}

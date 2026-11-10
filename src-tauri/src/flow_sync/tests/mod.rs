//! The test scenarios of `specifications/core/FTS-flow-tracker-sync.md`.
//!
//! Every test drives the sync through [`FakeTracker`], an in-memory tracker,
//! so no test makes a network request. This module holds the fake and the
//! fixtures; the topic modules hold the tests.

use std::cell::RefCell;
use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::adapter::*;
use super::engine::{run, SyncOutcome, SyncRequest};
use super::records::*;
use crate::flow_validation::model::{parse_valid, FlowDocument, Item, SyncLink};
use crate::project_settings::FlowTrackerSettings;

mod conflicts;
mod create;
mod edges_of_removal;
mod flow_write;
mod github;
mod links;
mod merge;
mod pull;
mod serialize;
mod structure;

pub const HOST: &str = "github.com";
pub const OWNER: &str = "acme";
pub const REPO: &str = "payments";
pub const FLOW_ID: &str = "plans/payments.flow";

/// The state of the in-memory tracker.
#[derive(Clone, Debug, Default)]
pub struct FakeState {
    pub issues: BTreeMap<u64, TrackerIssue>,
    pub milestones: BTreeMap<u64, TrackerMilestone>,
    /// Sub-issues in another repository, by the number of their parent.
    pub foreign_sub_issues: Vec<(u64, TrackerIssue)>,
    /// Issue numbers that are pull requests: a read finds no issue there.
    pub pull_requests: Vec<u64>,
    pub types: Vec<String>,
    pub next_number: u64,
    /// Every call, in the order it was made: `operation subject`, and for a
    /// write the arguments it was made with (see [`FakeTracker::call`]).
    pub calls: Vec<String>,
    /// The operation that fails, and after how many successful calls of it.
    pub fail: Option<(String, usize)>,
}

pub struct FakeTracker {
    pub state: RefCell<FakeState>,
}

pub fn url_of(number: u64) -> String {
    format!("https://{HOST}/{OWNER}/{REPO}/issues/{number}")
}

pub fn milestone_url(number: u64) -> String {
    format!("https://{HOST}/{OWNER}/{REPO}/milestone/{number}")
}

pub fn key(number: u64) -> IssueKey {
    IssueKey { owner: OWNER.into(), repo: REPO.into(), number, url: url_of(number) }
}

/// An open issue of the repository, with no relations.
pub fn issue(number: u64, title: &str, issue_type: Option<&str>) -> TrackerIssue {
    TrackerIssue {
        number,
        url: url_of(number),
        owner: OWNER.into(),
        repo: REPO.into(),
        title: title.into(),
        body: String::new(),
        state: "open".into(),
        issue_type: issue_type.map(str::to_string),
        milestone: None,
        parent: None,
        blocked_by: Vec::new(),
    }
}

/// [`issue`] with a parent and a milestone.
pub fn child(
    number: u64,
    title: &str,
    issue_type: &str,
    parent: Option<u64>,
    milestone: Option<u64>,
) -> TrackerIssue {
    let mut i = issue(number, title, Some(issue_type));
    i.parent = parent.map(key);
    i.milestone = milestone;
    i
}

pub fn milestone(number: u64, title: &str) -> TrackerMilestone {
    TrackerMilestone {
        number,
        url: milestone_url(number),
        title: title.into(),
        description: String::new(),
        state: "open".into(),
        due: None,
    }
}

impl FakeTracker {
    pub fn new() -> Self {
        Self {
            state: RefCell::new(FakeState {
                types: vec!["Epic".into(), "Feature".into(), "Task".into(), "Bug".into()],
                next_number: 100,
                ..FakeState::default()
            }),
        }
    }

    pub fn with_issue(self, i: TrackerIssue) -> Self {
        self.state.borrow_mut().issues.insert(i.number, i);
        self
    }

    pub fn with_milestone(self, m: TrackerMilestone) -> Self {
        self.state.borrow_mut().milestones.insert(m.number, m);
        self
    }

    pub fn failing(self, operation: &str, after: usize) -> Self {
        self.state.borrow_mut().fail = Some((operation.to_string(), after));
        self
    }

    pub fn edit(&self, number: u64, change: impl FnOnce(&mut TrackerIssue)) {
        change(self.state.borrow_mut().issues.get_mut(&number).expect("issue"));
    }

    pub fn issue(&self, number: u64) -> TrackerIssue {
        self.state.borrow().issues[&number].clone()
    }

    pub fn calls(&self) -> Vec<String> {
        self.state.borrow().calls.clone()
    }

    pub fn clear_calls(&self) {
        self.state.borrow_mut().calls.clear();
    }

    /// The write calls alone: every call that is not a read.
    pub fn writes(&self) -> Vec<String> {
        self.calls()
            .into_iter()
            .filter(|c| !c.starts_with("read") && !c.starts_with("list"))
            .collect()
    }

    /// Record one call as `operation subject`, or `operation subject args`
    /// where `args` is not empty, and fail it where [`Self::failing`] asks.
    /// The arguments of each write:
    /// - `create_issue 100 type=Task milestone=5`
    /// - `update_issue 3 title,body,type` (the fields it writes)
    /// - `set_parent 3 2->6` (the current parent to the new one; `none` for
    ///   no parent, `owner/repo#9` for a parent in another repository)
    /// - `set_milestone 6 5->7` (the milestone it had to the new one)
    /// - `add_blocked_by 3 4` and `remove_blocked_by 3 4` (the blocker)
    /// - `create_milestone 100` and `update_milestone 5 title,description,due`
    fn call(&self, operation: &str, subject: u64, args: &str) -> Result<(), String> {
        let mut state = self.state.borrow_mut();
        let entry = if args.is_empty() {
            format!("{operation} {subject}")
        } else {
            format!("{operation} {subject} {args}")
        };
        if let Some((failing, after)) = state.fail.clone() {
            if failing == operation {
                let made =
                    state.calls.iter().filter(|c| c.split(' ').next() == Some(operation)).count();
                if made >= after {
                    state.calls.push(format!("{entry} FAILED"));
                    return Err(ERR_GITHUB_UNREACHABLE.into());
                }
            }
        }
        state.calls.push(entry);
        Ok(())
    }
}

fn opt_number(n: Option<u64>) -> String {
    n.map_or_else(|| "none".to_string(), |n| n.to_string())
}

/// A parent as [`FakeTracker::call`] records it.
fn parent_text(k: Option<&IssueKey>) -> String {
    match k {
        None => "none".into(),
        Some(k) if k.owner == OWNER && k.repo == REPO => k.number.to_string(),
        Some(k) => format!("{}/{}#{}", k.owner, k.repo, k.number),
    }
}

fn fields_text(fields: &[(&str, bool)]) -> String {
    fields.iter().filter(|(_, on)| *on).map(|(name, _)| *name).collect::<Vec<_>>().join(",")
}

impl TrackerAdapter for FakeTracker {
    fn repository(&self) -> (&str, &str, &str) {
        (HOST, OWNER, REPO)
    }

    fn read_issues(&self, numbers: &[u64]) -> Result<Vec<(u64, Option<TrackerIssue>)>, String> {
        self.call("read_issues", numbers.len() as u64, "")?;
        let state = self.state.borrow();
        Ok(numbers
            .iter()
            .map(|n| {
                let found = if state.pull_requests.contains(n) {
                    None
                } else {
                    state.issues.get(n).cloned()
                };
                (*n, found)
            })
            .collect())
    }

    fn read_milestones(
        &self,
        numbers: &[u64],
    ) -> Result<Vec<(u64, Option<TrackerMilestone>)>, String> {
        self.call("read_milestones", numbers.len() as u64, "")?;
        let state = self.state.borrow();
        Ok(numbers.iter().map(|n| (*n, state.milestones.get(n).cloned())).collect())
    }

    fn read_sub_issues(&self, parents: &[u64]) -> Result<Vec<(u64, Vec<TrackerIssue>)>, String> {
        self.call("read_sub_issues", parents.len() as u64, "")?;
        let state = self.state.borrow();
        Ok(parents
            .iter()
            .map(|p| {
                let mut children: Vec<TrackerIssue> = state
                    .issues
                    .values()
                    .filter(|i| i.parent.as_ref().is_some_and(|k| k.number == *p))
                    .cloned()
                    .collect();
                children.extend(
                    state.foreign_sub_issues.iter().filter(|(q, _)| q == p).map(|(_, i)| i.clone()),
                );
                (*p, children)
            })
            .collect())
    }

    fn create_issue(&self, new: &NewIssue) -> Result<Created, String> {
        let number = self.state.borrow().next_number;
        let args = format!(
            "type={} milestone={}",
            new.issue_type.as_deref().unwrap_or("none"),
            opt_number(new.milestone)
        );
        self.call("create_issue", number, &args)?;
        let mut state = self.state.borrow_mut();
        state.next_number += 1;
        let mut i = issue(number, &new.title, new.issue_type.as_deref());
        i.body = new.body.clone();
        i.milestone = new.milestone;
        state.issues.insert(number, i);
        Ok(Created { number, url: url_of(number) })
    }

    fn update_issue(&self, number: u64, change: &IssueChange) -> Result<(), String> {
        let args = fields_text(&[
            ("title", change.title.is_some()),
            ("body", change.body.is_some()),
            ("type", change.issue_type.is_some()),
        ]);
        self.call("update_issue", number, &args)?;
        let mut state = self.state.borrow_mut();
        let i = state.issues.get_mut(&number).ok_or(ERR_GITHUB_UNREACHABLE)?;
        if let Some(t) = &change.title {
            i.title = t.clone();
        }
        if let Some(b) = &change.body {
            i.body = b.clone();
        }
        if let Some(t) = &change.issue_type {
            i.issue_type = t.clone();
        }
        Ok(())
    }

    fn set_parent(
        &self,
        number: u64,
        current: Option<&IssueKey>,
        parent: Option<u64>,
    ) -> Result<(), String> {
        let args = format!("{}->{}", parent_text(current), opt_number(parent));
        self.call("set_parent", number, &args)?;
        let mut state = self.state.borrow_mut();
        let i = state.issues.get_mut(&number).ok_or(ERR_GITHUB_UNREACHABLE)?;
        i.parent = parent.map(key);
        Ok(())
    }

    fn set_milestone(&self, number: u64, milestone: Option<u64>) -> Result<(), String> {
        let had = self.state.borrow().issues.get(&number).and_then(|i| i.milestone);
        self.call(
            "set_milestone",
            number,
            &format!("{}->{}", opt_number(had), opt_number(milestone)),
        )?;
        let mut state = self.state.borrow_mut();
        state.issues.get_mut(&number).ok_or(ERR_GITHUB_UNREACHABLE)?.milestone = milestone;
        Ok(())
    }

    fn add_blocked_by(&self, number: u64, blocker: u64) -> Result<(), String> {
        self.call("add_blocked_by", number, &blocker.to_string())?;
        let mut state = self.state.borrow_mut();
        state.issues.get_mut(&number).ok_or(ERR_GITHUB_UNREACHABLE)?.blocked_by.push(key(blocker));
        Ok(())
    }

    fn remove_blocked_by(&self, number: u64, blocker: u64) -> Result<(), String> {
        self.call("remove_blocked_by", number, &blocker.to_string())?;
        let mut state = self.state.borrow_mut();
        let i = state.issues.get_mut(&number).ok_or(ERR_GITHUB_UNREACHABLE)?;
        i.blocked_by.retain(|k| k.number != blocker);
        Ok(())
    }

    fn create_milestone(&self, new: &NewMilestone) -> Result<Created, String> {
        let number = self.state.borrow().next_number;
        self.call("create_milestone", number, "")?;
        let mut state = self.state.borrow_mut();
        state.next_number += 1;
        let mut m = milestone(number, &new.title);
        m.description = new.description.clone();
        m.due = new.due.clone();
        state.milestones.insert(number, m);
        Ok(Created { number, url: milestone_url(number) })
    }

    fn update_milestone(&self, number: u64, change: &MilestoneChange) -> Result<(), String> {
        let args = fields_text(&[
            ("title", change.title.is_some()),
            ("description", change.description.is_some()),
            ("due", change.due.is_some()),
        ]);
        self.call("update_milestone", number, &args)?;
        let mut state = self.state.borrow_mut();
        let m = state.milestones.get_mut(&number).ok_or(ERR_GITHUB_UNREACHABLE)?;
        if let Some(t) = &change.title {
            m.title = t.clone();
        }
        if let Some(d) = &change.description {
            m.description = d.clone();
        }
        if let Some(d) = &change.due {
            m.due = d.clone();
        }
        Ok(())
    }

    fn list_issue_types(&self) -> Result<Vec<String>, String> {
        self.call("list_issue_types", 0, "")?;
        Ok(self.state.borrow().types.clone())
    }
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

/// A JSON item. `parent` is the container id, or `None` at the top level.
pub fn item(id: &str, kind: &str, name: &str, parent: Option<&str>) -> Value {
    let mut v = json!({ "id": id, "kind": kind, "name": name });
    if let Some(p) = parent {
        v["parentId"] = json!(p);
    }
    v["position"] = json!({ "x": 20, "y": 60 });
    if !matches!(kind, "task" | "defect") {
        v["size"] = json!({ "width": 400, "height": 300 });
    }
    v
}

pub fn ref_json(kind: &str, number: u64) -> Value {
    let url = if kind == "milestone" { milestone_url(number) } else { url_of(number) };
    json!({ "kind": kind, "host": HOST, "owner": OWNER, "repo": REPO, "number": number, "url": url })
}

/// The base of an issue link.
pub fn issue_base(
    name: &str,
    issue_type: Option<&str>,
    parent: Option<u64>,
    milestone: Option<u64>,
    blocked_by: &[u64],
) -> Value {
    json!({
        "name": name,
        "description": "",
        "state": "open",
        "type": issue_type,
        "parent": parent.map(|p| ref_json("issue", p)),
        "milestone": milestone,
        "blockedBy": blocked_by.iter().map(|b| ref_json("issue", *b)).collect::<Vec<_>>(),
    })
}

pub fn milestone_base(name: &str, due: Option<&str>) -> Value {
    json!({ "name": name, "description": "", "state": "open", "due": due })
}

pub fn link(item_id: &str, kind: &str, number: u64, base: Value) -> Value {
    json!({ "itemId": item_id, "ref": ref_json(kind, number), "base": base })
}

pub fn blocks(id: &str, from: &str, to: &str) -> Value {
    json!({ "id": id, "from": from, "to": to, "type": "blocks" })
}

/// A valid document of these items, edges, and links.
pub fn doc(items: Vec<Value>, edges: Vec<Value>, links: Vec<Value>) -> FlowDocument {
    doc_ignoring(items, edges, links, vec![])
}

/// [`doc`] with these `sync.ignored` references.
pub fn doc_ignoring(
    items: Vec<Value>,
    edges: Vec<Value>,
    links: Vec<Value>,
    ignored: Vec<Value>,
) -> FlowDocument {
    let body = json!({
        "version": 2,
        "name": "Payments",
        "items": items,
        "edges": edges,
        "sync": { "tracker": "github", "links": links, "ignored": ignored },
    })
    .to_string();
    match parse_valid(&body) {
        Ok(d) => d,
        Err(report) => panic!("fixture is not a valid Flow: {report:?}"),
    }
}

pub fn settings() -> FlowTrackerSettings {
    FlowTrackerSettings::default()
}

pub fn sync(tracker: &FakeTracker, doc: FlowDocument) -> SyncOutcome {
    resolve(tracker, doc, &[])
}

pub fn resolve(
    tracker: &FakeTracker,
    doc: FlowDocument,
    resolutions: &[ConflictResolution],
) -> SyncOutcome {
    let settings = settings();
    run(tracker, SyncRequest { flow_id: FLOW_ID, doc, settings: &settings, resolutions })
        .unwrap_or_else(|e| panic!("sync failed: {e:?}"))
}

/// The outcome must be a valid Flow, as the save path requires (PST-FR-28).
pub fn assert_valid(doc: &FlowDocument) {
    let report = crate::flow_validation::validate(&super::serialize::write_flow(doc));
    assert!(report.valid, "the synced Flow is not valid: {report:?}");
}

pub fn find<'a>(doc: &'a FlowDocument, id: &str) -> &'a Item {
    doc.items.iter().find(|i| i.id == id).unwrap_or_else(|| panic!("no item {id}"))
}

pub fn link_of<'a>(doc: &'a FlowDocument, id: &str) -> &'a SyncLink {
    doc.sync
        .as_ref()
        .and_then(|s| s.links.iter().find(|l| l.item_id == id))
        .unwrap_or_else(|| panic!("no link of {id}"))
}

pub fn has_link(doc: &FlowDocument, id: &str) -> bool {
    doc.sync.as_ref().is_some_and(|s| s.links.iter().any(|l| l.item_id == id))
}

pub fn choose(conflict: &SyncConflict, choice: Choice) -> ConflictResolution {
    ConflictResolution { conflict_id: conflict.id.clone(), choice }
}

/// A plan in sync with its tracker: milestone `m1` (milestone 5) holds epic
/// `ep1` (issue 1), which holds feature `f1` (issue 2), which holds task `t1`
/// (issue 3). The epic sits directly in the milestone, so its issue has no
/// parent (FTS-FR-HLVJ). The link of `t1` is the fourth link.
pub fn plan() -> (FakeTracker, FlowDocument) {
    let tracker = FakeTracker::new()
        .with_issue(child(1, "Card payments", "Epic", None, Some(5)))
        .with_issue(child(2, "Card capture", "Feature", Some(1), Some(5)))
        .with_issue(child(3, "Tokenize", "Task", Some(2), Some(5)))
        .with_milestone(milestone(5, "Beta"));
    let d = doc(
        vec![
            item("m1", "milestone", "Beta", None),
            item("ep1", "epic", "Card payments", Some("m1")),
            item("f1", "feature", "Card capture", Some("ep1")),
            item("t1", "task", "Tokenize", Some("f1")),
        ],
        vec![],
        vec![
            link("m1", "milestone", 5, milestone_base("Beta", None)),
            link("ep1", "issue", 1, issue_base("Card payments", Some("Epic"), None, Some(5), &[])),
            link(
                "f1",
                "issue",
                2,
                issue_base("Card capture", Some("Feature"), Some(1), Some(5), &[]),
            ),
            link("t1", "issue", 3, issue_base("Tokenize", Some("Task"), Some(2), Some(5), &[])),
        ],
    );
    (tracker, d)
}

/// [`plan`] with one more task `t2` (issue 4) beside `t1`, both in `f1`.
pub fn plan_with_two_tasks() -> (FakeTracker, FlowDocument) {
    let (tracker, mut d) = plan();
    let tracker = tracker.with_issue(child(4, "Card form", "Task", Some(2), Some(5)));
    let mut t2 = find(&d, "t1").clone();
    t2.id = "t2".into();
    t2.name = "Card form".into();
    t2.position.x = 220.0;
    d.items.push(t2);
    let base: crate::flow_validation::model::SyncLink = serde_json::from_value(link(
        "t2",
        "issue",
        4,
        issue_base("Card form", Some("Task"), Some(2), Some(5), &[]),
    ))
    .unwrap();
    d.sync.as_mut().unwrap().links.push(base);
    (tracker, d)
}

//! FTS-FR-ARMD: the tracker adapter, the one boundary of all tracker access.
//!
//! An adapter is bound to one repository and to the credential it uses, so the
//! sync logic above it never holds a secret. Every method answers in the terms
//! of this module, and every error is a fixed code that carries no request
//! text (FTS NFR: no token leaves the backend).

/// An issue of some repository, as a relation names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IssueKey {
    pub owner: String,
    pub repo: String,
    pub number: u64,
    /// The issue's web address, where the tracker reports one.
    pub url: String,
}

/// One issue, with the fields of FTS-FR-PNMA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackerIssue {
    pub number: u64,
    pub url: String,
    /// The repository the issue is in now. An issue that was transferred
    /// reports its new repository.
    pub owner: String,
    pub repo: String,
    pub title: String,
    pub body: String,
    /// `open` or `closed`.
    pub state: String,
    pub issue_type: Option<String>,
    pub milestone: Option<u64>,
    pub parent: Option<IssueKey>,
    pub blocked_by: Vec<IssueKey>,
}

impl TrackerIssue {
    pub fn key(&self) -> IssueKey {
        IssueKey {
            owner: self.owner.clone(),
            repo: self.repo.clone(),
            number: self.number,
            url: self.url.clone(),
        }
    }
}

/// One milestone, with the fields of FTS-FR-PNMA.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrackerMilestone {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub description: String,
    /// `open` or `closed`.
    pub state: String,
    /// A calendar date `YYYY-MM-DD`.
    pub due: Option<String>,
}

/// What a create returns: the new item's number and address.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Created {
    pub number: u64,
    pub url: String,
}

/// FTS-FR-HDRR: the values of a new issue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewIssue {
    pub title: String,
    pub body: String,
    pub issue_type: Option<String>,
    pub milestone: Option<u64>,
}

/// The fields an issue update writes. A `None` field is not written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IssueChange {
    pub title: Option<String>,
    pub body: Option<String>,
    pub issue_type: Option<Option<String>>,
}

impl IssueChange {
    pub fn is_empty(&self) -> bool {
        self.title.is_none() && self.body.is_none() && self.issue_type.is_none()
    }
}

/// The values of a new milestone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewMilestone {
    pub title: String,
    pub description: String,
    pub due: Option<String>,
}

/// The fields a milestone update writes. A `None` field is not written.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MilestoneChange {
    pub title: Option<String>,
    pub description: Option<String>,
    pub due: Option<Option<String>>,
}

impl MilestoneChange {
    pub fn is_empty(&self) -> bool {
        self.title.is_none() && self.description.is_none() && self.due.is_none()
    }
}

/// FTS-FR-ARMD: every tracker operation a sync makes.
///
/// The read operations take many numbers at once, so a sync of several hundred
/// items makes a number of requests that grows with the items and not with the
/// fields (FTS NFR). No operation closes, reopens, or deletes anything.
pub trait TrackerAdapter {
    /// The host, the owner, and the name of the bound repository.
    fn repository(&self) -> (&str, &str, &str);

    /// The issues of the bound repository, each number with `None` where no
    /// such issue exists, where the number is a pull request, or where the
    /// issue was moved to another repository.
    fn read_issues(&self, numbers: &[u64]) -> Result<Vec<(u64, Option<TrackerIssue>)>, String>;

    /// The milestones of the bound repository, each with `None` where none
    /// exists.
    fn read_milestones(
        &self,
        numbers: &[u64],
    ) -> Result<Vec<(u64, Option<TrackerMilestone>)>, String>;

    /// The sub-issues of each issue in `parents`, open and closed, in the
    /// tracker's order. A sub-issue can be in another repository.
    fn read_sub_issues(&self, parents: &[u64]) -> Result<Vec<(u64, Vec<TrackerIssue>)>, String>;

    fn create_issue(&self, issue: &NewIssue) -> Result<Created, String>;

    fn update_issue(&self, number: u64, change: &IssueChange) -> Result<(), String>;

    /// Make `parent` the parent of `number`, or remove the parent `current`
    /// where `parent` is `None`. `current` is the parent the tracker reported,
    /// which can be in another repository.
    fn set_parent(
        &self,
        number: u64,
        current: Option<&IssueKey>,
        parent: Option<u64>,
    ) -> Result<(), String>;

    fn set_milestone(&self, number: u64, milestone: Option<u64>) -> Result<(), String>;

    fn add_blocked_by(&self, number: u64, blocker: u64) -> Result<(), String>;

    fn remove_blocked_by(&self, number: u64, blocker: u64) -> Result<(), String>;

    fn create_milestone(&self, milestone: &NewMilestone) -> Result<Created, String>;

    fn update_milestone(&self, number: u64, change: &MilestoneChange) -> Result<(), String>;

    /// The issue Types the bound repository offers.
    fn list_issue_types(&self) -> Result<Vec<String>, String>;
}

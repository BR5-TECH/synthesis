//! FTS-FR-FDDV: every tracker request with its operation and its outcome.
//!
//! The adapter below this one does the work; this one only writes a record of
//! each call. A record carries the operation, a count or an item number, and
//! the outcome code. It never carries a title, a description, a URL, or a
//! secret.

use crate::log_fields;
use crate::logging::{log_debug, log_warn, Domain, LogSink, BUFFER};

use super::adapter::*;

pub struct LoggedAdapter<'a, S> {
    inner: &'a dyn TrackerAdapter,
    sink: &'a S,
}

impl<'a, S: LogSink + Clone + Send + 'static> LoggedAdapter<'a, S> {
    pub fn new(sink: &'a S, inner: &'a dyn TrackerAdapter) -> Self {
        Self { inner, sink }
    }

    fn record<T>(
        &self,
        operation: &str,
        subject: u64,
        outcome: Result<T, String>,
    ) -> Result<T, String> {
        match &outcome {
            Ok(_) => log_debug(
                self.sink,
                &BUFFER,
                &[Domain::Backend, Domain::Remote],
                "tracker request",
                log_fields! { "operation" => operation, "subject" => subject, "outcome" => "ok" },
            ),
            Err(code) => log_warn(
                self.sink,
                &BUFFER,
                &[Domain::Backend, Domain::Remote],
                "tracker request failed",
                log_fields! { "operation" => operation, "subject" => subject, "outcome" => code },
            ),
        }
        outcome
    }
}

impl<S: LogSink + Clone + Send + 'static> TrackerAdapter for LoggedAdapter<'_, S> {
    fn repository(&self) -> (&str, &str, &str) {
        self.inner.repository()
    }

    fn read_issues(&self, numbers: &[u64]) -> Result<Vec<(u64, Option<TrackerIssue>)>, String> {
        self.record("read_issues", numbers.len() as u64, self.inner.read_issues(numbers))
    }

    fn read_milestones(
        &self,
        numbers: &[u64],
    ) -> Result<Vec<(u64, Option<TrackerMilestone>)>, String> {
        self.record("read_milestones", numbers.len() as u64, self.inner.read_milestones(numbers))
    }

    fn read_sub_issues(&self, parents: &[u64]) -> Result<Vec<(u64, Vec<TrackerIssue>)>, String> {
        self.record("read_sub_issues", parents.len() as u64, self.inner.read_sub_issues(parents))
    }

    fn create_issue(&self, issue: &NewIssue) -> Result<Created, String> {
        let outcome = self.inner.create_issue(issue);
        let number = outcome.as_ref().map(|c| c.number).unwrap_or_default();
        self.record("create_issue", number, outcome)
    }

    fn update_issue(&self, number: u64, change: &IssueChange) -> Result<(), String> {
        self.record("update_issue", number, self.inner.update_issue(number, change))
    }

    fn set_parent(
        &self,
        number: u64,
        current: Option<u64>,
        parent: Option<u64>,
    ) -> Result<(), String> {
        self.record("set_parent", number, self.inner.set_parent(number, current, parent))
    }

    fn set_milestone(&self, number: u64, milestone: Option<u64>) -> Result<(), String> {
        self.record("set_milestone", number, self.inner.set_milestone(number, milestone))
    }

    fn add_blocked_by(&self, number: u64, blocker: u64) -> Result<(), String> {
        self.record("add_blocked_by", number, self.inner.add_blocked_by(number, blocker))
    }

    fn remove_blocked_by(&self, number: u64, blocker: u64) -> Result<(), String> {
        self.record("remove_blocked_by", number, self.inner.remove_blocked_by(number, blocker))
    }

    fn create_milestone(&self, milestone: &NewMilestone) -> Result<Created, String> {
        let outcome = self.inner.create_milestone(milestone);
        let number = outcome.as_ref().map(|c| c.number).unwrap_or_default();
        self.record("create_milestone", number, outcome)
    }

    fn update_milestone(&self, number: u64, change: &MilestoneChange) -> Result<(), String> {
        self.record("update_milestone", number, self.inner.update_milestone(number, change))
    }

    fn list_issue_types(&self) -> Result<Vec<String>, String> {
        let outcome = self.inner.list_issue_types();
        let count = outcome.as_ref().map(|t| t.len() as u64).unwrap_or_default();
        self.record("list_issue_types", count, outcome)
    }
}

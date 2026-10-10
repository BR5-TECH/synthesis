//! The polling session (GPP-FR-DATH, GPP-FR-XZTP, GPP-FR-ELWS, GPP-FR-GPYE,
//! GPP-FR-PUXT, GPP-FR-YROY, GPP-FR-RRPB, GPP-FR-ANDE, GPP-FR-RAQP).
//!
//! Everything here lives in memory only. The slot is keyed by the open project
//! and its active worktree, and resets itself the first time it is entered
//! under another key, so a session ends when either changes. A generation
//! counter moves on every reset and on every settings write, which is what lets
//! a poll that started before either change be discarded when it settles.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use super::eligibility::{PollFailure, PollSuccess};
use super::records::*;

/// GPP-FR-DATH: what a session belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionKey {
    pub project: String,
    pub worktree: PathBuf,
}

/// GPP-FR-DATH: the state of one polling session.
#[derive(Clone, Debug, Default)]
pub struct PollingSession {
    /// The configuration validated in this session, for the Project node id it
    /// answers for. `None` is the `unchecked` state.
    pub configuration: Option<(String, GithubPollingConfiguration)>,
    pub repository: Option<RepositoryRef>,
    /// The token of the poll in flight, if one runs (GPP-FR-XZTP).
    pub in_flight: Option<u64>,
    pub tasks: Vec<GithubReadyTask>,
    pub stale: bool,
    pub last_error_code: Option<String>,
    pub last_error: Option<String>,
    pub last_success_at: Option<String>,
    /// GPP-FR-YROY: the keys of the previous successful poll.
    pub previous: HashSet<IssueKey>,
    /// GPP-FR-RRPB: every key reported as new in this session.
    pub reported: HashSet<IssueKey>,
    /// GPP-FR-IGER: claims whose status update succeeded and whose record
    /// could not be written to disk. Held here so a retry is still offered.
    pub unsaved_claims: Vec<GithubPendingClaim>,
}

/// What a poll carries from its start to its settlement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PollTicket {
    pub key: SessionKey,
    pub generation: u64,
    pub token: u64,
    pub project_id: String,
}

/// GPP-FR-XZTP: whether a poll call started a poll.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PollStart {
    Started(PollTicket),
    AlreadyRunning,
}

/// How one poll settled.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Settled {
    /// GPP-FR-ELWS: the project, the worktree, or the settings changed.
    Discarded,
    /// GPP-FR-GPYE / GPP-FR-YROY: the new issues of this poll.
    Succeeded(Vec<NewIssue>),
    /// GPP-FR-PUXT: the typed error of the failure.
    Failed(String),
}

/// The session and the counters that outlive one session.
#[derive(Debug, Default)]
pub struct SessionSlot {
    key: Option<SessionKey>,
    session: PollingSession,
    generation: u64,
    next_token: u64,
}

impl SessionSlot {
    /// GPP-FR-DATH: the session for `key`, a fresh one where the key changed.
    pub fn enter(&mut self, key: &SessionKey) -> &mut PollingSession {
        if self.key.as_ref() != Some(key) {
            self.key = Some(key.clone());
            self.session = PollingSession::default();
            self.generation += 1;
        }
        &mut self.session
    }

    /// GPP-FR-DATH: the project closed, so the session ends.
    pub fn end(&mut self) {
        self.key = None;
        self.session = PollingSession::default();
        self.generation += 1;
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// GPP-FR-ELWS / GPP-FR-ANDE: the settings changed. A poll in flight is
    /// discarded when it settles, and the configuration is unchecked again
    /// until the new settings are validated.
    pub fn settings_changed(&mut self, key: &SessionKey) {
        let session = self.enter(key);
        session.configuration = None;
        self.generation += 1;
    }

    /// GPP-FR-IURX: record the result of a validation, unless the session or
    /// the settings changed after it started.
    pub fn record_configuration(
        &mut self,
        key: &SessionKey,
        generation: u64,
        project_id: &str,
        configuration: GithubPollingConfiguration,
    ) {
        if self.key.as_ref() != Some(key) || self.generation != generation {
            return;
        }
        self.session.configuration = Some((project_id.to_string(), configuration));
    }

    /// The configuration the view reports for these settings.
    pub fn configuration_for(
        &self,
        key: &SessionKey,
        settings: &GithubPollingSettings,
    ) -> GithubPollingConfiguration {
        let Some(project_id) = settings.project_node_id.as_deref() else {
            return GithubPollingConfiguration::unset();
        };
        if self.key.as_ref() != Some(key) {
            return GithubPollingConfiguration::unchecked();
        }
        match &self.session.configuration {
            Some((checked, configuration)) if checked == project_id => configuration.clone(),
            _ => GithubPollingConfiguration::unchecked(),
        }
    }

    /// GPP-FR-ANDE: whether the configuration of `project_id` is invalid.
    pub fn is_invalid(&self, key: &SessionKey, project_id: &str) -> bool {
        self.key.as_ref() == Some(key)
            && matches!(
                &self.session.configuration,
                Some((checked, c)) if checked == project_id && c.state == ConfigurationState::Invalid
            )
    }

    /// GPP-FR-EWLZ / GPP-FR-ANDE: the selected Project a poll or a claim
    /// may run against, or the refusal.
    pub fn require_pollable(
        &self,
        key: &SessionKey,
        settings: &GithubPollingSettings,
    ) -> Result<String, String> {
        let Some(project_id) = settings.project_node_id.clone() else {
            return Err(ERR_UNCONFIGURED.to_string());
        };
        if self.is_invalid(key, &project_id) {
            return Err(ERR_CONFIGURATION_INVALID.to_string());
        }
        Ok(project_id)
    }

    /// GPP-FR-XZTP: start a poll, or report the one already running.
    ///
    /// `generation` is the one the caller read before it loaded the settings,
    /// so a settings write that raced the start discards the result.
    pub fn begin_poll(&mut self, key: &SessionKey, project_id: &str, generation: u64) -> PollStart {
        self.enter(key);
        if self.session.in_flight.is_some() {
            return PollStart::AlreadyRunning;
        }
        self.next_token += 1;
        let token = self.next_token;
        self.session.in_flight = Some(token);
        PollStart::Started(PollTicket {
            key: key.clone(),
            generation,
            token,
            project_id: project_id.to_string(),
        })
    }

    /// GPP-FR-GPYE / GPP-FR-PUXT / GPP-FR-ELWS / GPP-FR-YROY / GPP-FR-RRPB:
    /// settle a poll.
    pub fn settle_poll(
        &mut self,
        ticket: &PollTicket,
        outcome: Result<PollSuccess, PollFailure>,
        now: &str,
    ) -> Settled {
        if self.key.as_ref() != Some(&ticket.key) {
            return Settled::Discarded;
        }
        if self.session.in_flight == Some(ticket.token) {
            self.session.in_flight = None;
        }
        if self.generation != ticket.generation {
            return Settled::Discarded;
        }
        let session = &mut self.session;
        match outcome {
            Ok(success) => {
                let keys: HashSet<IssueKey> = success.tasks.iter().map(IssueKey::of_task).collect();
                let mut new_issues = Vec::new();
                for task in &success.tasks {
                    let key = IssueKey::of_task(task);
                    if !session.previous.contains(&key) && session.reported.insert(key) {
                        new_issues.push(NewIssue {
                            issue_number: task.issue_number,
                            title: task.title.clone(),
                        });
                    }
                }
                session.previous = keys;
                session.tasks = success.tasks;
                session.stale = false;
                session.last_error_code = None;
                session.last_error = None;
                session.last_success_at = Some(now.to_string());
                session.repository = Some(success.repository);
                session.configuration = Some((
                    ticket.project_id.clone(),
                    GithubPollingConfiguration::valid(&success.project_title),
                ));
                Settled::Succeeded(new_issues)
            }
            Err(failure) => {
                session.stale = true;
                if is_configuration_error(&failure.code) {
                    // GPP-FR-PUXT: recorded as the configuration state instead.
                    session.configuration = Some((
                        ticket.project_id.clone(),
                        GithubPollingConfiguration::invalid(&failure.code, failure.project_title),
                    ));
                } else {
                    session.last_error = Some(error_text(&failure.code));
                    session.last_error_code = Some(failure.code.clone());
                }
                Settled::Failed(failure.code)
            }
        }
    }

    /// The session for a read, when it belongs to `key`.
    pub fn session_for(&self, key: &SessionKey) -> Option<&PollingSession> {
        (self.key.as_ref() == Some(key)).then_some(&self.session)
    }

    /// Remember the polling repository a claim or a settings write resolved.
    /// A key that is no longer the session's changes nothing.
    pub fn remember_repository(&mut self, key: &SessionKey, repository: &RepositoryRef) {
        if self.key.as_ref() == Some(key) {
            self.session.repository = Some(repository.clone());
        }
    }

    /// GPP-FR-XZTP: a poll that ended without settling — it panicked — no
    /// longer counts as in flight.
    pub fn abandon(&mut self, key: &SessionKey, token: u64) {
        if self.key.as_ref() == Some(key) && self.session.in_flight == Some(token) {
            self.session.in_flight = None;
        }
    }

    /// GPP-FR-IGER: the claims held in memory for this session.
    pub fn unsaved_claims(&self, key: &SessionKey) -> Vec<GithubPendingClaim> {
        self.session_for(key).map(|s| s.unsaved_claims.clone()).unwrap_or_default()
    }

    /// GPP-FR-IGER: hold a claim the disk did not take, in place of any held
    /// claim of the same issue. A stale key changes nothing.
    pub fn keep_unsaved(&mut self, key: &SessionKey, claim: GithubPendingClaim) {
        if self.key.as_ref() != Some(key) {
            return;
        }
        let claims = &mut self.session.unsaved_claims;
        claims.retain(|c| {
            !c.names(
                &claim.repository_host,
                &claim.repository_owner,
                &claim.repository_name,
                claim.issue_number,
            )
        });
        claims.push(claim);
    }

    /// GPP-FR-BSLI: drop the held claim of one issue; whether one was held.
    pub fn forget_unsaved(&mut self, key: &SessionKey, repository: &RepositoryRef, number: u64) -> bool {
        if self.key.as_ref() != Some(key) {
            return false;
        }
        let claims = &mut self.session.unsaved_claims;
        let before = claims.len();
        claims.retain(|c| !c.names(&repository.host, &repository.owner, &repository.name, number));
        claims.len() != before
    }
}

/// GPP-FR-RAQP: the issues a claim or a retry is running for.
#[derive(Debug, Default)]
pub struct ClaimGuards {
    running: Mutex<HashSet<IssueKey>>,
}

/// Held for the length of one claim or retry; releases the issue on drop.
pub struct ClaimGuard<'a> {
    guards: &'a ClaimGuards,
    key: IssueKey,
}

impl ClaimGuards {
    /// GPP-FR-RAQP: take the issue, or refuse with `claim_in_progress`.
    pub fn acquire(&self, key: IssueKey) -> Result<ClaimGuard<'_>, String> {
        let mut running = self.running.lock().unwrap_or_else(|e| e.into_inner());
        if !running.insert(key.clone()) {
            return Err(ERR_CLAIM_IN_PROGRESS.to_string());
        }
        Ok(ClaimGuard { guards: self, key })
    }
}

impl Drop for ClaimGuard<'_> {
    fn drop(&mut self) {
        let mut running = self.guards.running.lock().unwrap_or_else(|e| e.into_inner());
        running.remove(&self.key);
    }
}

//! The metadata cache of GitHub publication (GHP-FR-IQTK, GHP-FR-ROBR,
//! GHP-FR-AWEZ, GHP-FR-EYGR, GHP-FR-JIAU, GHP-FR-RVCL).
//!
//! The rules live here as pure functions over plain values, so each one is
//! testable without a Tauri runtime and without a network. The application
//! glue that reads GitHub, emits the events, and runs the timer is in
//! [`super::metadata_refresh`].
//!
//! The cache lives in memory only. It is never written to disk, and a project
//! close, a project switch, or a change of the active worktree empties it.

use std::path::PathBuf;

use serde::Serialize;

use super::records::*;

/// GHP-FR-IQTK: the three lists of one resolved publication remote.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CachedLists {
    pub remote_name: String,
    pub repository_host: String,
    pub repository_owner: String,
    pub repository_name: String,
    pub parents: MetadataList<PublicationParentIssue>,
    pub issue_types: MetadataList<PublicationIssueType>,
    pub milestones: MetadataList<PublicationMilestone>,
}

impl CachedLists {
    /// Whether `other` names the same repository on the same host, through
    /// the same remote.
    fn same_repository(&self, other: &CachedLists) -> bool {
        self.remote_name == other.remote_name
            && self.repository_host == other.repository_host
            && self.repository_owner == other.repository_owner
            && self.repository_name == other.repository_name
    }

    /// GHP-FR-MSLY: the typed error of the first list that failed to read.
    pub fn first_failure(&self) -> Option<String> {
        [&self.parents.error_code, &self.issue_types.error_code, &self.milestones.error_code]
            .into_iter()
            .find_map(Clone::clone)
    }
}

/// GHP-FR-EYGR: one list merged into the cache. A loaded list replaces the
/// cached one. A failed list keeps the cached one, and is stored as failed only
/// where the cache holds no list for this repository.
fn merge_list<T: Clone>(
    cached: Option<&MetadataList<T>>,
    fresh: MetadataList<T>,
) -> MetadataList<T> {
    match (fresh.state, cached) {
        (MetadataState::Failed, Some(cached)) => cached.clone(),
        _ => fresh,
    }
}

/// GHP-FR-EYGR: the cache after one successful read of the lists.
pub fn merge(cached: Option<&CachedLists>, fresh: CachedLists) -> CachedLists {
    let Some(cached) = cached.filter(|c| c.same_repository(&fresh)) else {
        return fresh;
    };
    CachedLists {
        parents: merge_list(Some(&cached.parents), fresh.parents),
        issue_types: merge_list(Some(&cached.issue_types), fresh.issue_types),
        milestones: merge_list(Some(&cached.milestones), fresh.milestones),
        ..fresh
    }
}

/// Why a refresh started. The trigger decides only who hears of a failure:
/// the startup refresh tells the main window (GHP-FR-ISJI), and every other
/// refresh only logs it (GHP-FR-RVCL, GHP-FR-JDMU).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshTrigger {
    Startup,
    Periodic,
    Invalidation,
    Request,
}

impl RefreshTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            RefreshTrigger::Startup => "startup",
            RefreshTrigger::Periodic => "periodic",
            RefreshTrigger::Invalidation => "invalidation",
            RefreshTrigger::Request => "request",
        }
    }
}

/// GHP-FR-SIMK: how one refresh ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RefreshOutcomeKind {
    Changed,
    Unchanged,
    Failed,
}

/// GHP-FR-SIMK: the outcome of one refresh, without the cache it left.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefreshOutcome {
    pub kind: RefreshOutcomeKind,
    pub error_code: Option<String>,
}

impl RefreshOutcome {
    pub fn failed(code: &str) -> Self {
        RefreshOutcome { kind: RefreshOutcomeKind::Failed, error_code: Some(code.to_string()) }
    }
}

/// What the cache belongs to: the open project and its active worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CacheKey {
    pub project: String,
    pub worktree: PathBuf,
}

/// What a refresh carries from its start to its settlement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RefreshTicket {
    pub key: CacheKey,
    pub generation: u64,
    pub trigger: RefreshTrigger,
}

/// GHP-FR-AWEZ: whether a request started a refresh or joined one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RefreshStart {
    Started(RefreshTicket),
    Joined,
}

/// The result of one settlement, for the glue to act on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settlement {
    pub outcome: RefreshOutcome,
    /// GHP-FR-JIAU: the cache changed, so the change event is due.
    pub changed: bool,
    /// GHP-FR-JDMU: an invalidation came while this refresh ran, so one more
    /// refresh is due with the new values.
    pub rerun: bool,
}

/// The cache, its in-flight refresh, and its timer epoch.
///
/// `W` is what a joined request waits on. It is generic so the rules here stay
/// free of any channel type and testable with plain values.
#[derive(Debug)]
pub struct CacheSlot<W> {
    key: Option<CacheKey>,
    generation: u64,
    lists: Option<CachedLists>,
    in_flight: bool,
    /// GHP-FR-JDMU: an invalidation joined the running refresh.
    rerun: bool,
    waiters: Vec<W>,
    epoch: u64,
    /// The number of refreshes that started, and the trigger of the last one.
    /// The log and the tests read them.
    starts: u64,
    last_trigger: Option<RefreshTrigger>,
    /// GHP-FR-ISJI / GHP-FR-KPXF: the typed error of the failed startup
    /// refresh, kept until it is taken.
    startup_failure: Option<String>,
}

impl<W> Default for CacheSlot<W> {
    fn default() -> Self {
        CacheSlot {
            key: None,
            generation: 0,
            lists: None,
            in_flight: false,
            rerun: false,
            waiters: Vec::new(),
            epoch: 0,
            starts: 0,
            last_trigger: None,
            startup_failure: None,
        }
    }
}

impl<W> CacheSlot<W> {
    /// GHP-FR-ROBR / GHP-FR-ISJI: a project opened, switched, or changed its
    /// worktree, or closed (`key` is `None`). The cache and a kept startup
    /// failure empty, and a refresh still in flight is discarded when it
    /// settles. Returns the waiters of that refresh, which no settlement
    /// answers any more.
    pub fn reset(&mut self, key: Option<CacheKey>) -> Vec<W> {
        self.startup_failure = None;
        self.key = key;
        self.empty()
    }

    /// GHP-FR-JDMU / GHP-FR-HGVL: no GitHub Project is selected any more. The
    /// cache empties for the same key, and a refresh still in flight is
    /// discarded when it settles. A kept startup failure stays until it is
    /// taken.
    pub fn empty(&mut self) -> Vec<W> {
        self.generation += 1;
        self.lists = None;
        self.in_flight = false;
        self.rerun = false;
        self.epoch += 1;
        std::mem::take(&mut self.waiters)
    }

    pub fn key(&self) -> Option<&CacheKey> {
        self.key.as_ref()
    }

    pub fn lists(&self) -> Option<&CachedLists> {
        self.lists.as_ref()
    }

    /// The generation of the cache, which every reset moves on.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn is_running(&self) -> bool {
        self.in_flight
    }

    /// The number of refreshes that started since the application started.
    pub fn starts(&self) -> u64 {
        self.starts
    }

    /// The trigger of the last refresh that started.
    pub fn last_trigger(&self) -> Option<RefreshTrigger> {
        self.last_trigger
    }

    /// The number of requests that wait on the running refresh.
    pub fn waiter_count(&self) -> usize {
        self.waiters.len()
    }

    /// GHP-FR-KPXF: the kept startup failure, which this call discards.
    pub fn take_startup_failure(&mut self) -> Option<String> {
        self.startup_failure.take()
    }

    /// The epoch of the live timer.
    pub fn timer_epoch(&self) -> u64 {
        self.epoch
    }

    /// GHP-FR-AWEZ: start a refresh, or join the one in flight. `waiter` is
    /// answered when the refresh ends; `None` asks for no answer. Returns
    /// `None` where no project is open, and drops the waiter.
    pub fn begin(&mut self, trigger: RefreshTrigger, waiter: Option<W>) -> Option<RefreshStart> {
        let key = self.key.clone()?;
        self.waiters.extend(waiter);
        if self.in_flight {
            // GHP-FR-JDMU: the running refresh may have read the old values,
            // so an invalidation asks for one more refresh after it.
            self.rerun |= trigger == RefreshTrigger::Invalidation;
            return Some(RefreshStart::Joined);
        }
        self.in_flight = true;
        self.starts += 1;
        self.last_trigger = Some(trigger);
        Some(RefreshStart::Started(RefreshTicket { key, generation: self.generation, trigger }))
    }

    /// GHP-FR-RVCL / GHP-FR-JDMU: restart the 30-minute timer. Returns the
    /// epoch of the new timer, which [`CacheSlot::is_current_timer`] checks
    /// when it falls due.
    pub fn arm_timer(&mut self) -> u64 {
        self.epoch += 1;
        self.epoch
    }

    /// Whether the timer of `epoch` is still the live one: no refresh started
    /// and no reset happened after it was armed.
    pub fn is_current_timer(&self, epoch: u64) -> bool {
        self.key.is_some() && self.epoch == epoch
    }

    /// GHP-FR-ROBR / GHP-FR-EYGR / GHP-FR-MSLY / GHP-FR-JIAU: settle one
    /// refresh. `None` means the refresh is discarded: it belongs to a cache
    /// that a reset has replaced. The waiters come back for the glue to answer.
    pub fn settle(
        &mut self,
        ticket: &RefreshTicket,
        result: Result<CachedLists, String>,
    ) -> Option<(Settlement, Vec<W>)> {
        if ticket.generation != self.generation || self.key.as_ref() != Some(&ticket.key) {
            return None;
        }
        self.in_flight = false;
        let rerun = std::mem::take(&mut self.rerun);
        let waiters = std::mem::take(&mut self.waiters);
        let settlement = match result {
            Ok(fresh) => {
                let failure = fresh.first_failure();
                let merged = merge(self.lists.as_ref(), fresh);
                let changed = self.lists.as_ref() != Some(&merged);
                if changed {
                    self.lists = Some(merged);
                }
                let outcome = match failure {
                    Some(code) => RefreshOutcome::failed(&code),
                    None if changed => {
                        RefreshOutcome { kind: RefreshOutcomeKind::Changed, error_code: None }
                    }
                    None => {
                        RefreshOutcome { kind: RefreshOutcomeKind::Unchanged, error_code: None }
                    }
                };
                Settlement { outcome, changed, rerun }
            }
            // GHP-FR-JDMU: no GitHub Project is selected, so the cache empties.
            Err(code) if code == ERR_GITHUB_PROJECT_NOT_SELECTED => {
                let changed = self.lists.take().is_some();
                Settlement { outcome: RefreshOutcome::failed(&code), changed, rerun }
            }
            // GHP-FR-MSLY: a failed or timed out refresh keeps every list.
            Err(code) => {
                Settlement { outcome: RefreshOutcome::failed(&code), changed: false, rerun }
            }
        };
        // GHP-FR-ISJI: kept under the same lock as the settlement, so a reset
        // that comes later always discards it.
        if ticket.trigger == RefreshTrigger::Startup {
            if let Some(code) = settlement.outcome.error_code.as_deref() {
                if code != ERR_GITHUB_PROJECT_NOT_SELECTED {
                    self.startup_failure = Some(code.to_string());
                }
            }
        }
        Some((settlement, waiters))
    }
}

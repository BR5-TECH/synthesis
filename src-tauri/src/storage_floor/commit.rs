//! The draft-event commits of application-owned storage
//! (`PST-project-storage.md` PST-FR-DQZT, PST-FR-TYNC, PST-FR-YWXF,
//! PST-FR-BIPA, PST-FR-KGRW).
//!
//! The application commits a draft's **committed draft storage** — its record,
//! its prompt, and its images (`DRS-draft-storage.md` DRS-FR-ZIVL) — at three
//! draft events and at no other time: the draft is created, the draft is
//! deleted, and a graduation run of it is started. A save of the prompt and an
//! accepted change to it make no commit, so neither the author's typing nor an
//! agent's proposal reaches the history on its own.
//!
//! **Nothing waits for it.** Events are queued and committed one at a time, in
//! order, on this module's own thread. No command is delayed by a commit and no
//! caller is handed its failure. A commit that did not happen leaves the files
//! complete on disk for the next event of that draft or for the author.
//!
//! The commit goes through `crate::git::commit_exact_paths`, the `GTC-git.md`
//! GTC-FR-19 contract with no rename pairing. That gives it three properties
//! it would otherwise have to reproduce: a path the author staged is not named
//! and therefore stays staged and uncommitted, a path that is gone is recorded
//! as a deletion, and no path Git would pair with a named one is carried in.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Instant;

use crate::fs::RootFs;
use crate::log_fields;
use crate::logging::{self, Domain, Fields, LogSink, BUFFER};

/// PST-FR-DQZT: the three moments a draft's committed storage is committed at.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DraftEvent {
    /// The draft was created (`DRS-draft-storage.md` DRS-FR-06).
    Created,
    /// The draft was deleted (`DRS-draft-storage.md` DRS-FR-21).
    Deleted,
    /// A graduation run of it was started (`GSU-graduation-start.md`
    /// GSU-FR-RNOM).
    GraduationStarted,
}

impl DraftEvent {
    /// PST-FR-YWXF: the words the commit message opens with.
    fn verb(self) -> &'static str {
        match self {
            Self::Created => "create",
            Self::Deleted => "delete",
            Self::GraduationStarted => "graduate",
        }
    }

    /// The fixed token a record names the event by. A record carries no draft
    /// identity and no name (PST-FR-KGRW), so the event is what tells two
    /// records apart.
    fn token(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Deleted => "deleted",
            Self::GraduationStarted => "graduation_started",
        }
    }
}

/// PST-FR-YWXF: the message of one draft-event commit — the event and the
/// draft's current name, and nothing else.
///
/// A control character in the name, a line break above all, would turn the
/// one-line message into a subject and a body, so each one is read as a space.
pub fn message(event: DraftEvent, draft_name: &str) -> String {
    let name: String = draft_name
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    format!("draft: {} \"{}\"", event.verb(), name.trim())
}

/// Why a commit was not taken. Each is a silent no-op rather than a fault
/// (PST-FR-KGRW), and each is reported at most once in a row for one root.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Skip {
    RootUnavailable,
    NotARepository,
    BareRepository,
    RepositoryBusy,
    ConflictInIndex,
    NoCommitYet,
    NoAuthoringIdentity,
    IndexHeld,
    NothingToCommit,
    StatusUnreadable,
}

impl Skip {
    fn reason(self) -> &'static str {
        match self {
            Self::RootUnavailable => "root_unavailable",
            Self::NotARepository => "not_a_repository",
            Self::BareRepository => "bare_repository",
            Self::RepositoryBusy => "repository_busy",
            Self::ConflictInIndex => "conflict_in_index",
            Self::NoCommitYet => "no_commit_yet",
            Self::NoAuthoringIdentity => "no_authoring_identity",
            Self::IndexHeld => "index_held",
            Self::NothingToCommit => "nothing_to_commit",
            Self::StatusUnreadable => "status_unreadable",
        }
    }

    /// A status that could not be read is a fault rather than a quiet case,
    /// so it is named at `WARN` where every other skip is named at `DEBUG`.
    fn level(self) -> logging::LogLevel {
        match self {
            Self::StatusUnreadable => logging::LogLevel::Warn,
            _ => logging::LogLevel::Debug,
        }
    }
}

/// What a record is written through, with the runtime already erased.
///
/// The committer holds its queue in a process-wide static, so it cannot be
/// generic over the Tauri runtime. Boxing a closure that captured the concrete
/// sink keeps this module runtime-agnostic.
type Emit = Box<dyn Fn(logging::LogLevel, &str, Fields) + Send + 'static>;

/// One draft event waiting for its commit.
struct Job {
    root: RootFs,
    draft_id: String,
    event: DraftEvent,
    message: String,
}

#[derive(Default)]
struct State {
    /// PST-FR-BIPA: every event, in the order it occurred. Two events are never
    /// folded into one commit, so the history says what happened in the order
    /// it happened.
    pending: VecDeque<Job>,
    /// Whether a commit is being taken right now, so a waiter can tell a
    /// committer that is idle from one that is merely between jobs. Under test
    /// the waiter *is* the committer, so there is no such moment to observe.
    #[cfg_attr(test, allow(dead_code))]
    running: bool,
    /// Where a record goes, remembered once for the process.
    ///
    /// A draft event is raised from write paths many commands share, and not
    /// all of them hold the application's sink. There is exactly one sink in
    /// the process, so remembering the first one offered is the same handle
    /// every caller would have passed. Until one is offered the committer still
    /// commits and records nothing.
    emit: Option<Emit>,
    /// The reason each root last did not commit — a skip **or** a failure, on
    /// one vocabulary — so a repository with no authoring identity writes one
    /// record rather than one per event for a whole session (PST-FR-KGRW). A
    /// root that commits forgets its reason, so the same one is reported again
    /// if it comes back.
    last_reason: BTreeMap<PathBuf, &'static str>,
}

struct Committer {
    state: Mutex<State>,
    wake: Condvar,
}

static COMMITTER: OnceLock<Committer> = OnceLock::new();

/// PST-FR-KGRW: what a root last stopped for, so the same reason is not
/// recorded twice in a row — and is forgotten once a commit is taken.
fn remember_reason(state: &mut State, root: &std::path::Path, reason: Option<&'static str>) {
    match reason {
        Some(reason) => {
            state.last_reason.insert(root.to_path_buf(), reason);
        }
        None => {
            state.last_reason.remove(root);
        }
    }
}

fn committer() -> &'static Committer {
    COMMITTER.get_or_init(|| Committer {
        state: Mutex::new(State::default()),
        wake: Condvar::new(),
    })
}

/// Start the committer's thread once, the first time an event is raised.
///
/// **Under test there is no thread.** A committer firing in the middle of an
/// unrelated test would commit into a repository whose `HEAD` that test was
/// asserting on. So a test build queues the event exactly as production does
/// and commits it only when [`wait_for_committer`] asks.
#[cfg(not(test))]
fn ensure_thread() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        std::thread::Builder::new()
            .name("draft-event-committer".to_string())
            .spawn(run)
            .ok();
    });
}

#[cfg(test)]
fn ensure_thread() {}

/// PST-FR-KGRW: offer the application's sink, so the committer can record why
/// a commit was skipped or dropped. Returns at once and never fails.
pub fn offer_sink<S>(sink: &S)
where
    S: LogSink + Clone + Send + 'static,
{
    let mut state = committer().state.lock().unwrap_or_else(|e| e.into_inner());
    if state.emit.is_none() {
        let sink = sink.clone();
        state.emit = Some(Box::new(move |level, message, fields| {
            logging::log(&sink, &BUFFER, level, &[Domain::Backend], message, fields);
        }));
    }
}

/// PST-FR-DQZT / PST-FR-BIPA: queue one draft event for its commit.
///
/// Returns at once and never fails: this is the whole of what a draft operation
/// has to do about committing (`DRS-draft-storage.md` DRS-FR-HRIB). The message
/// is composed now, from the name the draft holds at the event, so a rename
/// that lands before the commit is taken does not rewrite what happened.
pub fn draft_event(root: &RootFs, draft_id: &str, draft_name: &str, event: DraftEvent) {
    ensure_thread();
    let committer = committer();
    let mut state = committer.state.lock().unwrap_or_else(|e| e.into_inner());
    state.pending.push_back(Job {
        root: root.clone(),
        draft_id: draft_id.to_string(),
        event,
        message: message(event, draft_name),
    });
    let queued = state.pending.len();
    if let Some(emit) = state.emit.as_ref() {
        emit(
            logging::LogLevel::Debug,
            "draft event queued",
            log_fields! { "event" => event.token(), "queued" => queued },
        );
    }
    committer.wake.notify_all();
}

/// Write one record through the sink the committer remembers, for a draft
/// storage path that holds no sink of its own. Nothing is written until a sink
/// has been offered (PST-FR-KGRW).
///
/// The caller passes a fixed message and fields that carry no path, no draft
/// identity, and no name.
pub fn record(level: logging::LogLevel, message: &str, fields: Fields) {
    let state = committer().state.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(emit) = state.emit.as_ref() {
        emit(level, message, fields);
    }
}

/// Wait until the committer has nothing left to do.
///
/// Production never calls this — the whole of PST-FR-KGRW is that nothing
/// waits.
#[cfg(not(test))]
pub fn wait_for_committer() {
    ensure_thread();
    let committer = committer();
    let mut state = committer.state.lock().unwrap_or_else(|e| e.into_inner());
    while !state.pending.is_empty() || state.running {
        state = committer
            .wake
            .wait(state)
            .unwrap_or_else(|e| e.into_inner());
    }
}

/// Under test the caller **is** the committer: this root's queued events are
/// taken, in order, and committed here. Events of every other root stay queued,
/// so a test can never commit into another test's repository.
#[cfg(test)]
pub fn wait_for_committer(root: &RootFs) {
    loop {
        let committer = committer();
        let (job, last_reason) = {
            let mut state = committer.state.lock().unwrap_or_else(|e| e.into_inner());
            let Some(index) = state.pending.iter().position(|job| job.root.path() == root.path())
            else {
                return;
            };
            let job = state.pending.remove(index).expect("the index was just found");
            let last_reason = state.last_reason.get(root.path()).copied();
            (job, last_reason)
        };
        let (reason, records) = commit_now(&job, last_reason);
        let mut state = committer.state.lock().unwrap_or_else(|e| e.into_inner());
        remember_reason(&mut state, job.root.path(), reason);
        if let Some(emit) = state.emit.as_ref() {
            for (level, message, fields) in records {
                emit(level, &message, fields);
            }
        }
    }
}

/// The events of one root still waiting, oldest first, for a test to read.
#[cfg(test)]
pub fn pending_events(root: &RootFs) -> Vec<(String, DraftEvent, String)> {
    let state = committer().state.lock().unwrap_or_else(|e| e.into_inner());
    state
        .pending
        .iter()
        .filter(|job| job.root.path() == root.path())
        .map(|job| (job.draft_id.clone(), job.event, job.message.clone()))
        .collect()
}

#[cfg(not(test))]
fn run() {
    loop {
        let committer = committer();
        let (job, last_reason) = {
            let mut state = committer.state.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if let Some(job) = state.pending.pop_front() {
                    state.running = true;
                    let last_reason = state.last_reason.get(job.root.path()).copied();
                    break (job, last_reason);
                }
                state.running = false;
                committer.wake.notify_all();
                state = committer
                    .wake
                    .wait(state)
                    .unwrap_or_else(|e| e.into_inner());
            }
        };
        // The commit runs with **no lock of this module held**, so an event
        // raised mid-commit is queued rather than waiting behind an index
        // write. It therefore collects what it wants recorded, and the records
        // are written afterwards through the one sink the committer remembers.
        //
        // A panic in one commit must not take the committer with it: every
        // later event would then never be committed for the rest of the
        // session, silently.
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            commit_now(&job, last_reason)
        }));
        let (reason, records) = outcome.unwrap_or_else(|_| {
            (
                Some("commit_panicked"),
                vec![(
                    logging::LogLevel::Warn,
                    "draft event commit dropped".to_string(),
                    log_fields! { "event" => job.event.token(), "reason" => "commit_panicked" },
                )],
            )
        });
        let mut state = committer.state.lock().unwrap_or_else(|e| e.into_inner());
        remember_reason(&mut state, job.root.path(), reason);
        // Cleared **before** the records are written. A sink that panics while
        // emitting would otherwise leave `running` set for the rest of the
        // session, and a waiter would block on a committer that is never coming
        // back.
        state.running = false;
        committer.wake.notify_all();
        if let Some(emit) = state.emit.as_ref() {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                for (level, message, fields) in records {
                    emit(level, &message, fields);
                }
            }));
        }
    }
}

/// One commit of one event, taken now, for a test to reach the decision table
/// of PST-FR-KGRW without a thread.
#[cfg(test)]
fn commit_once(root: &RootFs, draft_id: &str, event: DraftEvent) -> Option<&'static str> {
    commit_once_recording(root, draft_id, event, None).0
}

/// [`commit_once`] with the records it wanted written, and with the reason the
/// previous attempt stopped for — the once-per-reason rule is about two
/// attempts rather than one.
#[cfg(test)]
fn commit_once_recording(
    root: &RootFs,
    draft_id: &str,
    event: DraftEvent,
    last_reason: Option<&'static str>,
) -> (Option<&'static str>, Vec<Record>) {
    let job = Job {
        root: root.clone(),
        draft_id: draft_id.to_string(),
        event,
        message: message(event, "A draft"),
    };
    commit_now(&job, last_reason)
}

/// One record the commit wants written, held until the sink is reachable.
type Record = (logging::LogLevel, String, Fields);

/// One commit of one draft event: the reason there was none, and what to
/// record.
///
/// `None` is a commit that was taken. Anything else is the token that says why
/// it was not — a skip's or a failure's, on one vocabulary, so PST-FR-KGRW's
/// once-per-reason rule covers both.
fn commit_now(job: &Job, last_reason: Option<&'static str>) -> (Option<&'static str>, Vec<Record>) {
    let mut records: Vec<Record> = Vec::new();
    let event = job.event.token();
    // PST-FR-KGRW: a reason equal to the last one reported for this root is not
    // recorded again.
    let mut report = |skip: Skip, count: usize| -> (Option<&'static str>, Vec<Record>) {
        if last_reason != Some(skip.reason()) {
            records.push((
                skip.level(),
                "draft event commit skipped".to_string(),
                log_fields! { "event" => event, "reason" => skip.reason(), "paths" => count },
            ));
        }
        (Some(skip.reason()), std::mem::take(&mut records))
    };

    if !job.root.path().exists() {
        return report(Skip::RootUnavailable, 0);
    }
    let Ok(repo) = crate::changes::open_repo(job.root.path()) else {
        return report(Skip::NotARepository, 0);
    };
    // `project_prefix` answers `""` for a bare repository, which would scope
    // every pathspec wrongly, so this is refused rather than inferred.
    if repo.workdir().is_none() {
        return report(Skip::BareRepository, 0);
    }

    // PST-FR-TYNC / DRS-FR-VECL: the paths of this one draft's committed
    // storage that differ from `HEAD`, and the drafts root's two Git files, and
    // no other path. A deletion names every path under the draft's folder. An
    // entry Git paired as a rename is named at **each** of its two locations
    // that is this draft's: a draft moved between folders is recorded as gone
    // from the old one and present in the new one, never as both.
    let Ok(status) = crate::git::working_tree_status(&job.root) else {
        return report(Skip::StatusUnreadable, 0);
    };
    let deletion = job.event == DraftEvent::Deleted;
    let named = |path: &str| super::is_draft_event_path(path, &job.draft_id, deletion);
    let mut paths: Vec<String> = Vec::new();
    for entry in status {
        if let Some(previous) = entry.previous_path.filter(|previous| named(previous)) {
            paths.push(previous);
        }
        if named(&entry.path) {
            paths.push(entry.path);
        }
    }
    if paths.is_empty() {
        return report(Skip::NothingToCommit, 0);
    }

    if let Some(skip) = repository_unready(&repo) {
        return report(skip, paths.len());
    }

    // PST-FR-KGRW: the index is held for the write, which excludes a concurrent
    // publication and the author's own `git` command alike. A committer that
    // cannot take it drops this event rather than blocking: the files stay on
    // disk for the next event of the draft or for the author.
    let Ok(Some(_lock)) = crate::git::index_lock::IndexLock::try_acquire(&repo) else {
        return report(Skip::IndexHeld, paths.len());
    };
    // Re-read after the lock: both conditions can change between the reading
    // and the write, and the lock is what makes the second reading hold.
    if let Some(skip) = repository_unready(&repo) {
        return report(skip, paths.len());
    }

    let started = Instant::now();
    match crate::git::commit_exact_paths(&job.root, &job.message, &paths) {
        Ok(outcome) => {
            records.push((
                logging::LogLevel::Debug,
                "draft event committed".to_string(),
                log_fields! {
                    "event" => event,
                    "paths" => outcome.committed_paths.len(),
                    "commit" => &outcome.commit_id,
                    "durationMs" => started.elapsed().as_millis() as u64,
                },
            ));
            (None, records)
        }
        // The status read raced the diff: something else committed these paths
        // between the two. Not a fault, and not worth a `WARN`.
        Err(reason) if reason == crate::git::ERR_NOTHING_TO_COMMIT => {
            report(Skip::NothingToCommit, paths.len())
        }
        Err(reason) => {
            // PST-FR-KGRW: attempted and failed, so it is named at `WARN` and
            // dropped. A count and a reason, and no path, no draft identity,
            // and no file content — which is why the underlying message is
            // classified rather than forwarded.
            let token = failure_reason(&reason);
            if last_reason != Some(token) {
                records.push((
                    logging::LogLevel::Warn,
                    "draft event commit dropped".to_string(),
                    log_fields! { "event" => event, "paths" => paths.len(), "reason" => token },
                ));
            }
            (Some(token), records)
        }
    }
}

/// PST-FR-KGRW: which stage of the commit failed, as a fixed token.
///
/// `commit_named_paths` reports its failures as prose, and some of that prose
/// carries the pathspec it stopped on (`GTC-git.md` GTC-FR-19's staging and
/// deletion arms both name one). Every path this module commits is a file of a
/// draft's own folder, the prompt the author named among them, so forwarding
/// the message would put a draft's identity, and a name the author chose, into
/// a record this requirement keeps both out of — and a record travels to the
/// Logs panel, the clipboard, and any file a user exports. Classifying keeps what a reader needs, which is **which stage
/// failed**, and drops the one part of the message that cannot be there.
fn failure_reason(message: &str) -> &'static str {
    const STAGES: [(&str, &str); 8] = [
        ("failed to stage ", "stage_failed"),
        ("failed to record the deletion of ", "deletion_failed"),
        ("failed to open the index", "index_unopenable"),
        ("failed to write the index", "index_unwritable"),
        ("failed to build the commit tree", "tree_unbuildable"),
        ("failed to read the commit tree", "tree_unreadable"),
        ("no Git authoring identity is configured", "no_authoring_identity"),
        ("failed to create the commit", "commit_rejected"),
    ];
    for (prefix, token) in STAGES {
        if message.starts_with(prefix) {
            return token;
        }
    }
    // The typed refusals of GTC-FR-20 are already tokens and carry no path, but
    // they are matched rather than forwarded so that this function is the only
    // thing that decides what reaches a record.
    match message {
        crate::git::ERR_NOTHING_TO_COMMIT => "nothing_to_commit",
        crate::git::ERR_EMPTY_COMMIT_MESSAGE => "empty_commit_message",
        crate::git::ERR_NO_PATHS_SELECTED => "no_paths_selected",
        _ => "commit_failed",
    }
}

/// The conditions under which committing would be wrong rather than merely
/// unnecessary (PST-FR-KGRW).
fn repository_unready(repo: &git2::Repository) -> Option<Skip> {
    // A commit into a merge, a rebase, a cherry-pick, or a bisect lands inside
    // an operation the author is in the middle of, where `git rebase --continue`
    // would squash or drop it with no trace.
    if repo.state() != git2::RepositoryState::Clean {
        return Some(Skip::RepositoryBusy);
    }
    // PST-FR-KGRW: this resolves no conflict and overwrites no conflicted file —
    // a path Git reports in conflict is left for the author (per
    // `DRS-draft-storage.md` DRS-FR-JPVB). The state check above covers the
    // conflict a merge, a rebase, a cherry-pick, or a bisect leaves; an index
    // holding conflicts in a repository Git calls clean — what a conflicted
    // `git stash pop` leaves — is caught here, because staging such a path would
    // commit its conflict markers as though they were the file.
    if repo.index().map(|index| index.has_conflicts()).unwrap_or(false) {
        return Some(Skip::ConflictInIndex);
    }
    // A repository with no commit yet: this would otherwise become the
    // project's first commit and would create its branch. The draft stays
    // uncommitted until the author makes a first commit of their own, which is
    // the right outcome.
    if repo.head().is_err() {
        return Some(Skip::NoCommitYet);
    }
    if repo.signature().is_err() {
        return Some(Skip::NoAuthoringIdentity);
    }
    // A **detached** `HEAD` is committed to rather than skipped. Every
    // operation that leaves one as an intermediate state is already excluded
    // above, so what remains is a checkout an author may sit in for days, and
    // refusing there would leave every draft event of that checkout
    // uncommitted. The commit moves that `HEAD` alone and moves no branch.
    None
}

#[cfg(test)]
mod tests;

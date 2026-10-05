//! The watch over the selected paths (DCL-FR-ZYQC).
//!
//! Every available source is watched: a folder recursively, and a file through
//! its parent folder. A change that reaches a watched path runs one refresh after
//! a short debounce, so a burst of changes costs one refresh. Closing the project
//! stops the watch (DCL-FR-XHSJ).
//!
//! The watch registers interest with the operating system and reads no file, so
//! it does not go through `FsAccess`; every read the refresh then makes does
//! (DCL-FR-BAQY).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use notify_debouncer_mini::notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};

use crate::logging::LogLevel;

use super::log::DocLog;
use super::manager::lock;
use super::model::{normalise_path, Availability, DocumentSource, SourceKind};

/// How long a burst of changes is collected before it costs one refresh.
pub const DEBOUNCE: Duration = Duration::from_millis(400);

/// One path to watch.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct WatchTarget {
    pub path: PathBuf,
    pub recursive: bool,
}

/// DCL-FR-ZYQC: the paths to watch for these sources — an available folder
/// recursively, an available file through its parent folder. An unavailable
/// source has nothing to watch. Sorted and free of repeats.
pub fn watch_targets(sources: &[DocumentSource]) -> Vec<WatchTarget> {
    let mut targets: Vec<WatchTarget> = Vec::new();
    for source in sources.iter().filter(|s| s.status == Availability::Available) {
        let normalised = normalise_path(&source.path);
        let target = match source.kind {
            SourceKind::Folder => WatchTarget {
                path: PathBuf::from(normalised),
                recursive: true,
            },
            SourceKind::File => {
                let Some((parent, _)) = normalised.rsplit_once('/') else {
                    continue;
                };
                WatchTarget {
                    path: PathBuf::from(if parent.is_empty() { "/" } else { parent }),
                    recursive: false,
                }
            }
        };
        if !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets.sort();
    targets
}

#[derive(Default)]
struct Watching {
    debouncer: Option<Debouncer<RecommendedWatcher>>,
    targets: Vec<WatchTarget>,
    /// Bumped by every `stop`, so a watch that was being set up when the stop
    /// came is recognised and dropped instead of installed (DCL-FR-XHSJ).
    epoch: u64,
}

/// The live watch and the scheduler of background refreshes, as managed state.
#[derive(Default)]
pub struct DocumentsWatcher {
    inner: Mutex<Watching>,
    /// A refresh has been asked for and has not started yet.
    pub(super) pending: AtomicBool,
    /// A background thread is draining `pending`.
    pub(super) running: AtomicBool,
}

/// How a target is registered with the operating system. The real one watches
/// the path; a test substitutes one that waits.
type Register<'a> = dyn Fn(&mut Debouncer<RecommendedWatcher>, &WatchTarget) -> bool + 'a;

fn register_with_os(debouncer: &mut Debouncer<RecommendedWatcher>, target: &WatchTarget) -> bool {
    let mode = if target.recursive {
        RecursiveMode::Recursive
    } else {
        RecursiveMode::NonRecursive
    };
    debouncer.watcher().watch(&target.path, mode).is_ok()
}

impl DocumentsWatcher {
    /// DCL-FR-XHSJ: stop watching. Idempotent, and bounded in time: it holds the
    /// lock only to take the watch out, so it never waits for a watch that is
    /// still registering its paths, and the teardown of the old watch runs on a
    /// thread of its own.
    pub fn stop(&self) {
        let old = {
            let mut inner = lock(&self.inner);
            inner.epoch += 1;
            inner.targets.clear();
            inner.debouncer.take()
        };
        self.pending.store(false, Ordering::SeqCst);
        if let Some(old) = old {
            std::thread::spawn(move || drop(old));
        }
    }

    /// The paths watched now.
    pub fn targets(&self) -> Vec<WatchTarget> {
        lock(&self.inner).targets.clone()
    }

    /// Whether a watch is armed.
    pub fn is_armed(&self) -> bool {
        lock(&self.inner).debouncer.is_some()
    }

    /// Replace the watch with one over `targets`. Best effort: a path the
    /// operating system cannot watch is logged and skipped, and the refresh that
    /// follows an add or a remove still runs.
    pub fn watch(
        &self,
        targets: Vec<WatchTarget>,
        log: &dyn DocLog,
        on_change: impl Fn() + Send + 'static,
    ) {
        self.watch_with(targets, log, on_change, &register_with_os);
    }

    /// `watch` with the registration of each target given. The registration of a
    /// large folder can take long, so it runs outside the lock; a `stop` that
    /// came meanwhile discards the watch when it is done.
    pub(super) fn watch_with(
        &self,
        targets: Vec<WatchTarget>,
        log: &dyn DocLog,
        on_change: impl Fn() + Send + 'static,
        register: &Register<'_>,
    ) {
        let (epoch, old) = {
            let mut inner = lock(&self.inner);
            inner.targets = targets.clone();
            (inner.epoch, inner.debouncer.take())
        };
        drop(old);
        if targets.is_empty() {
            return;
        }
        let mut debouncer = match new_debouncer(DEBOUNCE, move |result: DebounceEventResult| {
            if result.is_ok() {
                on_change();
            }
        }) {
            Ok(debouncer) => debouncer,
            Err(_) => {
                log.log(
                    LogLevel::Warn,
                    "documents watch could not start",
                    crate::log_fields! { "targets" => targets.len() },
                );
                return;
            }
        };
        let mut watched = 0usize;
        for target in &targets {
            if register(&mut debouncer, target) {
                watched += 1;
            } else {
                log.log(
                    LogLevel::Warn,
                    "documents path could not be watched",
                    crate::log_fields! { "recursive" => target.recursive },
                );
            }
        }
        if watched == 0 {
            return;
        }
        let mut inner = lock(&self.inner);
        if inner.epoch != epoch {
            // A stop came while the paths were registered.
            drop(inner);
            std::thread::spawn(move || drop(debouncer));
            return;
        }
        inner.debouncer = Some(debouncer);
    }
}

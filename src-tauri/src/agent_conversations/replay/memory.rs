//! The replay memory (`ERP-exchange-replay.md` ERP-FR-ZVJT, ERP-FR-SQZM,
//! ERP-FR-NCEH).
//!
//! One memory per open project, reached through the project store. It holds
//! the replay entries of each discussion and agent, in memory alone: nothing of
//! it is written to disk, to a log record, or to an event payload.

use super::super::*;
use super::sources::SourceStamp;
use std::collections::{BTreeSet, HashMap};
use std::sync::Mutex;

/// ERP-FR-NCEH: the most entries one discussion and agent hold.
pub(in crate::agent_conversations) const MAX_ENTRIES_PER_KEY: usize = 20;

/// ERP-FR-NCEH: the most characters of entries one memory holds.
pub(in crate::agent_conversations) const MAX_MEMORY_CHARS: usize = 32_000_000;

/// ERP-FR-ZVJT: the key of the entries: the discussion and the agent.
#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub(in crate::agent_conversations) struct ReplayKey {
    pub discussion_id: String,
    pub agent_id: String,
}

/// ERP-FR-SQZM: one captured turn.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::agent_conversations) struct ReplayEntry {
    /// The order of the capture in this memory, which also names the entry.
    pub seq: u64,
    pub trigger_comment_id: String,
    /// The comment that holds the answer. `None` for a turn that ended on a
    /// contribution (ERP-FR-FEAD).
    pub answer_comment_id: Option<String>,
    pub provider: String,
    pub model_id: String,
    /// The names of the tools the turn was offered.
    pub tool_names: BTreeSet<String>,
    /// ERP-FR-GWPG: the messages after the opening, without the answer.
    pub rounds: Vec<rig::completion::Message>,
    /// ERP-FR-UFBV: the `discussion_state` of the turn's input, or the stub.
    pub state: Vec<InputSection>,
    /// ERP-FR-NKLF: the stamps of the results in `rounds`.
    pub stamps: Vec<SourceStamp>,
    /// When the entry was last selected, on the clock of the memory.
    pub last_used: u64,
}

impl ReplayEntry {
    /// The characters the entry holds: its rounds and its state.
    pub(in crate::agent_conversations) fn chars(&self) -> usize {
        rounds_chars(&self.rounds)
            + self.state.iter().map(|s| s.body.chars().count()).sum::<usize>()
    }
}

/// The characters of text of `rounds`, as the estimate counts them
/// (CVL-FR-PGII).
pub(in crate::agent_conversations) fn rounds_chars(rounds: &[rig::completion::Message]) -> usize {
    rounds.iter().map(|m| compaction::message_text(m).chars().count()).sum()
}

#[derive(Default)]
struct Inner {
    entries: HashMap<ReplayKey, Vec<ReplayEntry>>,
    /// ERP-FR-IICX: the first comment that the history of a key kept.
    history_start: HashMap<ReplayKey, String>,
    /// The characters of each stored entry, by its seq, counted once when the
    /// entry is stored, so a capture does not count the whole memory again.
    sizes: HashMap<u64, usize>,
    clock: u64,
    seq: u64,
}

impl Inner {
    fn total_chars(&self) -> usize {
        self.sizes.values().sum()
    }

    /// Forget the sizes of the entries no longer stored.
    fn prune_sizes(&mut self) {
        let live: std::collections::HashSet<u64> =
            self.entries.values().flatten().map(|e| e.seq).collect();
        self.sizes.retain(|seq, _| live.contains(seq));
    }

    /// ERP-FR-NCEH: drop the entries used least recently first, until the
    /// memory holds at most `MAX_MEMORY_CHARS` characters.
    fn evict_to_cap(&mut self) {
        let mut total = self.total_chars();
        while total > MAX_MEMORY_CHARS {
            let oldest = self
                .entries
                .iter()
                .flat_map(|(key, list)| list.iter().map(move |e| (e.last_used, e.seq, key.clone())))
                .min();
            let Some((_, seq, key)) = oldest else { break };
            if let Some(list) = self.entries.get_mut(&key) {
                if let Some(at) = list.iter().position(|e| e.seq == seq) {
                    list.remove(at);
                    total -= self.sizes.remove(&seq).unwrap_or_default();
                }
            }
        }
        self.entries.retain(|_, list| !list.is_empty());
    }
}

/// ERP-FR-ZVJT: the replay memory of one open project.
#[derive(Default)]
pub struct ReplayMemory {
    inner: Mutex<Inner>,
}

impl ReplayMemory {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// ERP-FR-FEAD, ERP-FR-NCEH: store `entry` for `key`. A capture beyond
    /// the cap of the key drops its oldest entry, and one beyond the cap of the
    /// memory drops the entries used least recently first. An entry for the
    /// same trigger replaces the older one. Returns the seq of the entry.
    pub(in crate::agent_conversations) fn capture(
        &self,
        key: ReplayKey,
        mut entry: ReplayEntry,
    ) -> u64 {
        let mut inner = self.lock();
        inner.seq += 1;
        inner.clock += 1;
        entry.seq = inner.seq;
        entry.last_used = inner.clock;
        let seq = entry.seq;
        inner.sizes.insert(seq, entry.chars());
        let list = inner.entries.entry(key).or_default();
        list.retain(|e| e.trigger_comment_id != entry.trigger_comment_id);
        list.push(entry);
        while list.len() > MAX_ENTRIES_PER_KEY {
            list.remove(0);
        }
        inner.prune_sizes();
        inner.evict_to_cap();
        seq
    }

    /// ERP-FR-CMMA: the entries of `key`, oldest capture first.
    pub(in crate::agent_conversations) fn entries_of(&self, key: &ReplayKey) -> Vec<ReplayEntry> {
        self.lock().entries.get(key).cloned().unwrap_or_default()
    }

    /// ERP-FR-NCEH: mark the entries `seqs` of `key` as used now.
    pub(in crate::agent_conversations) fn mark_used(&self, key: &ReplayKey, seqs: &[u64]) {
        let mut inner = self.lock();
        inner.clock += 1;
        let now = inner.clock;
        if let Some(list) = inner.entries.get_mut(key) {
            for entry in list.iter_mut().filter(|e| seqs.contains(&e.seq)) {
                entry.last_used = now;
            }
        }
    }

    /// ERP-FR-RVYX, ERP-FR-GVHU: write `entry` back in place of the stored
    /// entry with its seq. An entry no longer stored is not added again.
    pub(in crate::agent_conversations) fn replace(&self, key: &ReplayKey, entry: &ReplayEntry) {
        let mut inner = self.lock();
        if let Some(list) = inner.entries.get_mut(key) {
            if let Some(stored) = list.iter_mut().find(|e| e.seq == entry.seq) {
                *stored = entry.clone();
                inner.sizes.insert(entry.seq, entry.chars());
            }
        }
    }

    /// ERP-FR-BBXA, ERP-FR-GBOK: remove the entry `seq` of `key`.
    pub(in crate::agent_conversations) fn remove(&self, key: &ReplayKey, seq: u64) {
        let mut inner = self.lock();
        if let Some(list) = inner.entries.get_mut(key) {
            list.retain(|e| e.seq != seq);
        }
        inner.entries.retain(|_, list| !list.is_empty());
        inner.sizes.remove(&seq);
    }

    /// ERP-FR-IICX: the first comment that the history of `key` kept.
    pub(in crate::agent_conversations) fn history_start(&self, key: &ReplayKey) -> Option<String> {
        self.lock().history_start.get(key).cloned()
    }

    /// ERP-FR-IICX: keep `start` as the first comment of the history of `key`,
    /// or forget it where `start` is `None`.
    pub(in crate::agent_conversations) fn set_history_start(
        &self,
        key: &ReplayKey,
        start: Option<String>,
    ) {
        let mut inner = self.lock();
        match start {
            Some(id) => {
                inner.history_start.insert(key.clone(), id);
            }
            None => {
                inner.history_start.remove(key);
            }
        }
    }

    /// The characters the memory holds, for the tests of the cap.
    #[cfg(test)]
    pub(in crate::agent_conversations) fn total_chars(&self) -> usize {
        self.lock().total_chars()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(agent: &str) -> ReplayKey {
        ReplayKey { discussion_id: "d".into(), agent_id: agent.into() }
    }

    fn entry(trigger: &str, text: &str) -> ReplayEntry {
        ReplayEntry {
            seq: 0,
            trigger_comment_id: trigger.into(),
            answer_comment_id: None,
            provider: "p".into(),
            model_id: "m".into(),
            tool_names: BTreeSet::new(),
            rounds: vec![rig::completion::Message::assistant(text)],
            state: Vec::new(),
            stamps: Vec::new(),
            last_used: 0,
        }
    }

    // ERP-FR-ZVJT, ERP-FR-CMMA: the entries of a key are its own, and a
    // capture for the same trigger replaces the older one.
    #[test]
    fn keys_hold_their_own_entries() {
        let memory = ReplayMemory::default();
        memory.capture(key("a"), entry("t1", "one"));
        memory.capture(key("b"), entry("t1", "other"));
        memory.capture(key("a"), entry("t2", "two"));
        let a = memory.entries_of(&key("a"));
        assert_eq!(
            a.iter().map(|e| e.trigger_comment_id.as_str()).collect::<Vec<_>>(),
            ["t1", "t2"]
        );
        assert_eq!(memory.entries_of(&key("b")).len(), 1);
        memory.capture(key("a"), entry("t1", "again"));
        let a = memory.entries_of(&key("a"));
        assert_eq!(a.len(), 2);
        assert_eq!(a[1].trigger_comment_id, "t1");
        assert!(memory.entries_of(&key("c")).is_empty());
    }

    // ERP-FR-NCEH: a key holds at most 20 entries, and the oldest goes.
    #[test]
    fn a_key_holds_at_most_twenty_entries() {
        let memory = ReplayMemory::default();
        for i in 0..25 {
            memory.capture(key("a"), entry(&format!("t{i}"), "x"));
        }
        let a = memory.entries_of(&key("a"));
        assert_eq!(a.len(), MAX_ENTRIES_PER_KEY);
        assert_eq!(a[0].trigger_comment_id, "t5");
    }

    // ERP-FR-NCEH: above the cap of the memory, the entries used least
    // recently go first.
    #[test]
    fn the_memory_drops_the_least_recently_used_entries() {
        let memory = ReplayMemory::default();
        // Four entries fit the cap, and a fifth is above it.
        let big = "x".repeat(MAX_MEMORY_CHARS / 4 - 1_000);
        let first = memory.capture(key("a"), entry("t1", &big));
        memory.capture(key("b"), entry("t1", &big));
        memory.mark_used(&key("a"), &[first]);
        memory.capture(key("c"), entry("t1", &big));
        memory.capture(key("d"), entry("t1", &big));
        assert_eq!(memory.entries_of(&key("b")).len(), 1, "four entries fit");
        memory.capture(key("e"), entry("t1", &big));
        assert!(memory.total_chars() <= MAX_MEMORY_CHARS);
        assert_eq!(memory.entries_of(&key("a")).len(), 1, "used recently, kept");
        assert!(memory.entries_of(&key("b")).is_empty(), "used least recently, dropped");
        assert_eq!(memory.entries_of(&key("e")).len(), 1);
    }

    // ERP-FR-RVYX, ERP-FR-GBOK: an entry is written back in place and removed
    // by its seq; a removed entry is not added again.
    #[test]
    fn entries_are_replaced_and_removed_by_seq() {
        let memory = ReplayMemory::default();
        let seq = memory.capture(key("a"), entry("t1", "one"));
        let mut changed = memory.entries_of(&key("a")).remove(0);
        changed.rounds = vec![rig::completion::Message::assistant("two")];
        memory.replace(&key("a"), &changed);
        assert_eq!(memory.entries_of(&key("a"))[0].rounds, changed.rounds);
        memory.remove(&key("a"), seq);
        assert!(memory.entries_of(&key("a")).is_empty());
        memory.replace(&key("a"), &changed);
        assert!(memory.entries_of(&key("a")).is_empty());
    }

    // ERP-FR-IICX: the memory keeps the first comment of a key's history.
    #[test]
    fn the_history_start_is_kept_per_key() {
        let memory = ReplayMemory::default();
        assert_eq!(memory.history_start(&key("a")), None);
        memory.set_history_start(&key("a"), Some("c5".into()));
        assert_eq!(memory.history_start(&key("a")).as_deref(), Some("c5"));
        assert_eq!(memory.history_start(&key("b")), None);
        memory.set_history_start(&key("a"), None);
        assert_eq!(memory.history_start(&key("a")), None);
    }
}

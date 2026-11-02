//! The commits of one `openai` Realtime session and the chunks that they make
//! (VOP-FR-KIMU, VOP-FR-TWEZ, VOP-FR-GLRN, VOP-FR-QQYG).
//!
//! Each commit goes to a **group**: the open chunk when it is sent. A chunk
//! end seals the open group, and the next group opens at once. So an interim
//! commit that the server has not named yet still belongs to the chunk that
//! was open when it went out. The server names the commits in the order that
//! they were sent.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use super::super::realtime::Transcript;
use super::Reply;

#[derive(Default)]
pub(crate) struct CommitLedger {
    /// The id of the open group.
    open: usize,
    /// The group of each commit that the server did not name yet, in send
    /// order.
    waiting: VecDeque<usize>,
    /// The group of each item that a commit named.
    named: HashMap<String, usize>,
    /// The sealed chunks that wait for the text of their items.
    replies: BTreeMap<usize, Reply>,
    /// The items of sealed chunks. The live text holds none of them.
    closed: HashSet<String>,
    /// Whether any commit went out.
    any: bool,
}

impl CommitLedger {
    /// A commit went out for the open group.
    pub(crate) fn sent(&mut self) {
        self.waiting.push_back(self.open);
        self.any = true;
    }

    /// Whether any commit went out.
    pub(crate) fn any_sent(&self) -> bool {
        self.any
    }

    /// The id of the open group.
    pub(crate) fn open_group(&self) -> usize {
        self.open
    }

    /// The commits that the server did not name yet.
    pub(crate) fn waiting_count(&self) -> usize {
        self.waiting.len()
    }

    /// The named items that are not completed.
    pub(crate) fn incomplete_count(&self, transcript: &Transcript) -> usize {
        self.named.keys().filter(|item| !transcript.is_completed(item)).count()
    }

    /// The server named the oldest commit that waits. `None` is a commit that
    /// held no audio. Gives `false` when no commit waited.
    pub(crate) fn named(&mut self, item: Option<&str>) -> bool {
        let Some(group) = self.waiting.pop_front() else { return false };
        if let Some(item) = item {
            self.named.insert(item.to_string(), group);
            if group < self.open {
                self.closed.insert(item.to_string());
            }
        }
        true
    }

    /// VOP-FR-GLRN: a chunk end. The open group is sealed, and its items leave
    /// the live text at once. `known` is each item that the transcript holds
    /// now: it is audio of this chunk or of an earlier one.
    pub(crate) fn seal<'a>(&mut self, reply: Reply, known: impl Iterator<Item = &'a String>) {
        self.replies.insert(self.open, reply);
        self.closed.extend(known.cloned());
        let open = self.open;
        self.closed
            .extend(self.named.iter().filter(|(_, g)| **g == open).map(|(item, _)| item.clone()));
        self.open += 1;
    }

    /// VOP-FR-GLRN: reply to each sealed chunk whose commits are all named and
    /// whose items are all completed. The text joins its items in commit order
    /// with one space.
    pub(crate) fn settle(&mut self, transcript: &Transcript) {
        let ready: Vec<usize> = self
            .replies
            .keys()
            .copied()
            .filter(|group| {
                !self.waiting.contains(group)
                    && self
                        .named
                        .iter()
                        .filter(|(_, g)| *g == group)
                        .all(|(item, _)| transcript.is_completed(item))
            })
            .collect();
        for group in ready {
            let items: HashSet<String> = self
                .named
                .iter()
                .filter(|(_, g)| **g == group)
                .map(|(item, _)| item.clone())
                .collect();
            if let Some(reply) = self.replies.remove(&group) {
                let _ = reply.send(Ok(transcript.text_of(&items)));
            }
        }
    }

    /// VOP-FR-QQYG: every commit is named and every named item is completed.
    pub(crate) fn idle(&self, transcript: &Transcript) -> bool {
        self.waiting.is_empty() && self.named.keys().all(|item| transcript.is_completed(item))
    }

    pub(crate) fn closed(&self) -> &HashSet<String> {
        &self.closed
    }

    pub(crate) fn fail_all(&mut self, code: &str) {
        for (_, reply) in std::mem::take(&mut self.replies) {
            let _ = reply.send(Err(code.to_string()));
        }
    }
}

#[cfg(test)]
mod tests;

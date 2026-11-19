//! The selection and the layout of a replay (`ERP-exchange-replay.md`
//! ERP-FR-CMMA to ERP-FR-SBBE, ERP-FR-RVYX, ERP-FR-UUAD to ERP-FR-IICX).
//!
//! At dispatch, a turn selects the entries of its own discussion and agent,
//! checks their stamps, fits them in its room, and builds one block for each.
//! Its input then holds only what follows the last replayed trigger.

use super::super::*;
use super::memory::{ReplayEntry, ReplayKey, ReplayMemory};
use super::rounds::{call_ids, omit_results, replace_result, rounds_for_turn, Identity};
use super::sources::outdated_line;
use std::collections::{BTreeSet, HashMap, HashSet};

/// ERP-FR-RVYX: the most stamps a turn reads again.
pub(in crate::agent_conversations) const MAX_STAMP_CHECKS: usize = 200;

/// What a replay is planned against.
pub(in crate::agent_conversations) struct ReplayInputs<'a> {
    pub roots: Roots<'a>,
    pub thread: &'a comments::Discussion,
    /// The trigger comment. Empty for the estimate, whose history holds every
    /// comment (AGC-FR-JFBZ).
    pub trigger_comment_id: &'a str,
    pub key: ReplayKey,
    /// The names of the tools the turn is offered.
    pub offered: BTreeSet<String>,
    pub provider: &'a str,
    pub model_id: &'a str,
    pub window: u64,
    /// The bound of AGC-FR-HOND, in characters.
    pub history_bound: usize,
    /// The estimated tokens of the compiled prompt and the tool definitions.
    pub instruction_tokens: u64,
    /// False for the estimate, which changes no entry (ERP-FR-GDLV).
    pub update_memory: bool,
}

/// What a replay planned, for the request and for its record (ERP-FR-GOXP).
#[derive(Debug, Default)]
pub(in crate::agent_conversations) struct ReplayPlan {
    pub blocks: Vec<ReplayBlock>,
    /// CVL-FR-39: the number of material sections of the first opening.
    pub head: usize,
    pub demoted: usize,
    pub omitted: usize,
    pub outdated: usize,
    pub tokens: u64,
}

/// ERP-FR-UUAD: the replay room of a turn, in tokens.
pub(in crate::agent_conversations) fn replay_room(window: u64, estimate_without: u64) -> u64 {
    (window * 15 / 100).min((window * 60 / 100).saturating_sub(estimate_without))
}

/// ERP-FR-UUAD: the estimated tokens of the rounds and states of `entries`.
fn entries_tokens(entries: &[ReplayEntry]) -> u64 {
    estimate_tokens(entries.iter().map(ReplayEntry::chars).sum::<usize>() as u64)
}

/// ERP-FR-IICX: the start of the history of the turn. Where comments must be
/// dropped, the history section of `input` is built again from that start, and
/// the memory keeps it. `None` where the history is left as the builder made
/// it: the newest comment alone does not fit, or the anchor does not.
fn history_start(
    memory: &ReplayMemory,
    inputs: &ReplayInputs<'_>,
    comments: &ThreadComments,
    input: &mut [InputSection],
) -> Option<usize> {
    let state_chars: usize = input
        .iter()
        .filter(|s| s.tag == TAG_DISCUSSION_STATE)
        .map(|s| s.body.chars().count())
        .sum();
    let room = inputs.history_bound.saturating_sub(state_chars);
    let anchor_chars = comments.anchor.chars().count();
    if anchor_chars > room {
        return None;
    }
    let sizes: Vec<usize> = comments.comments.iter().map(|(_, parts)| text_chars(parts)).collect();
    let hint = memory
        .history_start(&inputs.key)
        .and_then(|id| comments.comments.iter().position(|(c, _)| *c == id));
    let start = first_kept_comment(anchor_chars, &sizes, room, hint)?;
    if inputs.update_memory {
        let kept = (start > 0).then(|| comments.comments[start].0.clone());
        memory.set_history_start(&inputs.key, kept);
    }
    if start > 0 {
        let section = compose_history(
            &comments.anchor,
            start,
            comments.comments[start..].iter().map(|(_, parts)| parts.clone()),
            true,
        );
        if let Some(history) = input.iter_mut().find(|s| s.tag == TAG_DISCUSSION_HISTORY) {
            *history = section;
        }
    }
    Some(start)
}

/// ERP-FR-CMMA, ERP-FR-BBXA: the entries of the key that the turn selects, in
/// the order of their triggers. An entry whose comments are gone is dropped
/// from the memory.
fn select_entries(
    memory: &ReplayMemory,
    inputs: &ReplayInputs<'_>,
    position: &HashMap<&str, usize>,
    start: usize,
    trigger_at: usize,
) -> Vec<ReplayEntry> {
    let mut selected = Vec::new();
    for entry in memory.entries_of(&inputs.key) {
        let trigger = position.get(entry.trigger_comment_id.as_str()).copied();
        let answer = entry.answer_comment_id.as_deref().map(|id| position.get(id).copied());
        let (Some(trigger), Some(answer)) = (trigger, answer.unwrap_or(Some(usize::MAX))) else {
            if inputs.update_memory {
                memory.remove(&inputs.key, entry.seq);
            }
            continue;
        };
        let answer_before = answer == usize::MAX || (answer > trigger && answer < trigger_at);
        if trigger >= start && trigger < trigger_at && answer_before {
            selected.push((trigger, entry));
        }
    }
    selected.sort_by_key(|(trigger, entry)| (*trigger, entry.seq));
    selected.into_iter().map(|(_, entry)| entry).collect()
}

/// ERP-FR-RVYX: read the stamps again, at most [`MAX_STAMP_CHECKS`], and
/// replace each result whose file changed. Returns how many were replaced.
fn check_stamps(
    memory: &ReplayMemory,
    inputs: &ReplayInputs<'_>,
    entries: &mut [ReplayEntry],
) -> usize {
    let mut checks = 0usize;
    let mut outdated = 0usize;
    for entry in entries.iter_mut() {
        let mut changed = false;
        let mut kept = Vec::new();
        for stamp in std::mem::take(&mut entry.stamps) {
            if checks >= MAX_STAMP_CHECKS {
                kept.push(stamp);
                continue;
            }
            checks += 1;
            if stamp.source.digest(inputs.roots).as_deref() == Some(stamp.sha256.as_str()) {
                kept.push(stamp);
            } else {
                replace_result(
                    &mut entry.rounds,
                    &stamp.call_id,
                    &outdated_line(&stamp.source.label()),
                );
                outdated += 1;
                changed = true;
            }
        }
        entry.stamps = kept;
        if changed && inputs.update_memory {
            memory.replace(&inputs.key, entry);
        }
    }
    outdated
}

/// ERP-FR-UUAD, ERP-FR-GVHU, ERP-FR-GBOK: fit the entries in the room. Above
/// it, the results of the oldest entries are omitted first, then the oldest
/// entries are demoted, until they fit three quarters of the room. Returns
/// how many results were omitted and how many entries were demoted.
fn fit_room(
    memory: &ReplayMemory,
    inputs: &ReplayInputs<'_>,
    entries: &mut Vec<ReplayEntry>,
    room: u64,
) -> (usize, usize) {
    if entries_tokens(entries) <= room {
        return (0, 0);
    }
    let target = room * 3 / 4;
    let mut omitted = 0;
    for index in 0..entries.len() {
        if entries_tokens(entries) <= target {
            break;
        }
        let changed = omit_results(&mut entries[index].rounds);
        if changed > 0 {
            omitted += changed;
            entries[index].stamps.clear();
            if inputs.update_memory {
                memory.replace(&inputs.key, &entries[index]);
            }
        }
    }
    let mut demoted = 0;
    while !entries.is_empty() && entries_tokens(entries) > target {
        let entry = entries.remove(0);
        if inputs.update_memory {
            memory.remove(&inputs.key, entry.seq);
        }
        demoted += 1;
    }
    (omitted, demoted)
}

/// ERP-FR-OPRO to ERP-FR-FMMT: plan the replay of a turn, and change `input`
/// to the current input message: the material moves into the first opening,
/// and the history holds only the comments after the last replayed trigger.
/// With no entry selected, `input` keeps its sections (ERP-FR-SBBE), with the
/// history start of ERP-FR-IICX.
pub(in crate::agent_conversations) fn plan_replay(
    memory: &ReplayMemory,
    inputs: &ReplayInputs<'_>,
    input: &mut Vec<InputSection>,
) -> ReplayPlan {
    let mut plan = ReplayPlan::default();
    let thread = inputs.thread;
    let trigger_at = if inputs.trigger_comment_id.is_empty() {
        thread.comments.len()
    } else {
        match thread.comments.iter().position(|c| c.id == inputs.trigger_comment_id) {
            Some(at) => at,
            None => return plan,
        }
    };
    let comments = thread_comments(inputs.roots, thread, inputs.trigger_comment_id);
    let Some(start) = history_start(memory, inputs, &comments, input) else { return plan };
    let position: HashMap<&str, usize> =
        thread.comments.iter().enumerate().map(|(i, c)| (c.id.as_str(), i)).collect();
    let mut entries = select_entries(memory, inputs, &position, start, trigger_at);
    if entries.is_empty() {
        return plan;
    }
    plan.outdated = check_stamps(memory, inputs, &mut entries);
    let without = inputs.instruction_tokens + split_of_sections(0, input).total();
    let room = replay_room(inputs.window, without);
    (plan.omitted, plan.demoted) = fit_room(memory, inputs, &mut entries, room);
    if entries.is_empty() {
        return plan;
    }
    plan.tokens = entries_tokens(&entries);
    if inputs.update_memory {
        let seqs: Vec<u64> = entries.iter().map(|e| e.seq).collect();
        memory.mark_used(&inputs.key, &seqs);
    }

    // ERP-FR-FMMT: an answer an entry replays is in no history section.
    let covered: HashSet<&str> =
        entries.iter().filter_map(|e| e.answer_comment_id.as_deref()).collect();
    let range = |from: usize, to: usize| -> Vec<Vec<InputPart>> {
        comments
            .comments
            .iter()
            .enumerate()
            .filter(|(index, (id, _))| {
                *index >= from && *index < to && !covered.contains(id.as_str())
            })
            .map(|(_, (_, parts))| parts.clone())
            .collect()
    };
    // ERP-FR-GAXX: the ids that more than one selected entry holds.
    let mut seen: HashMap<String, usize> = HashMap::new();
    for entry in &entries {
        for id in call_ids(&entry.rounds).into_iter().collect::<HashSet<_>>() {
            *seen.entry(id).or_default() += 1;
        }
    }
    let head = rig_messages::stable_head_of(input);
    let material: Vec<InputSection> = input[..head].to_vec();
    let mut from = start;
    for (index, entry) in entries.iter().enumerate() {
        let trigger = position[entry.trigger_comment_id.as_str()];
        let mut opening = Vec::new();
        if index == 0 {
            // ERP-FR-ISTB: the material, and the history from its start.
            opening.extend(material.iter().cloned());
            opening.push(compose_history(&comments.anchor, start, range(from, trigger), start > 0));
        } else {
            // ERP-FR-VMRY: the comments after the trigger before it.
            opening.push(compose_history("", 0, range(from, trigger), false));
        }
        opening.push(current_comment_section(inputs.roots, thread, &entry.trigger_comment_id));
        opening.extend(entry.state.iter().cloned());
        let answer = entry.answer_comment_id.as_deref().and_then(|id| {
            thread
                .comments
                .iter()
                .find(|c| c.id == id)
                .map(|c| strip_citation_markers(&c.body).into_owned())
        });
        let same_model = entry.provider == inputs.provider && entry.model_id == inputs.model_id;
        let repeated =
            call_ids(&entry.rounds).iter().any(|id| seen.get(id).is_some_and(|n| *n > 1));
        let identity = if !same_model {
            Identity::Canonical
        } else if inputs.provider == ai_api::custom_limits::CUSTOM || repeated {
            Identity::Renamed
        } else {
            Identity::Stored
        };
        plan.blocks.push(ReplayBlock {
            opening,
            rounds: rounds_for_turn(&entry.rounds, &inputs.offered, identity, entry.seq),
            answer,
        });
        from = trigger + 1;
    }
    plan.head = head;
    // ERP-FR-LKUP: the current input holds the comments after the last
    // replayed trigger, and no material section.
    let history = compose_history("", 0, range(from, trigger_at), false);
    input.drain(..head);
    match input.iter_mut().find(|s| s.tag == TAG_DISCUSSION_HISTORY) {
        Some(section) => *section = history,
        None => input.insert(0, history),
    }
    plan
}

#[cfg(test)]
mod tests;

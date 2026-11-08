//! The state the members of one agent group share (AGC-FR-KOHJ).
//!
//! Session state: it lives in memory for as long as a member or the waiter holds
//! it, and a relaunch finds none of it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use super::advice::{Advice, AdviceEntry};
use super::join::Join;
use crate::agent_conversations::SkipReason;

/// How often a waiting thread looks at its cancellation flag.
const CANCEL_CHECK: Duration = Duration::from_millis(200);

/// AGC-FR-NWDA: the two grouped modes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupMode {
    Advisor,
    Synthesis,
}

impl GroupMode {
    pub fn label(self) -> &'static str {
        match self {
            GroupMode::Advisor => "advisor",
            GroupMode::Synthesis => "synthesis",
        }
    }
}

/// How a member turn ended, as the join sees it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberEnd {
    Finished,
    Failed,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemberStatus {
    Pending,
    Finished,
    Skipped(SkipReason),
}

/// A member that the join ended while it still ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingMember {
    pub turn_id: String,
    pub nickname: String,
}

/// What a member offered to a question set or to a proposal, held for the
/// synthesis turn (AGC-FR-DIZM).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Held {
    pub nickname: String,
    pub text: String,
}

/// The result of waiting for the join.
#[derive(Debug, PartialEq, Eq)]
pub enum JoinOutcome {
    /// AGC-FR-ZLWN: the join ended. `pending` are the members still running,
    /// already marked skipped as `timed_out`; the caller cancels them.
    Over { pending: Vec<PendingMember>, timed_out: bool },
    /// The waiter's own turn was cancelled.
    Cancelled,
}

struct Member {
    turn_id: String,
    nickname: String,
    status: MemberStatus,
}

#[derive(Default)]
struct Inner {
    members: Vec<Member>,
    join: Option<Join>,
    advice: Vec<AdviceEntry>,
    questions: Vec<Held>,
    proposals: Vec<Held>,
    replies: Vec<Held>,
}

/// The shared state of one group.
pub struct GroupState {
    pub id: String,
    pub mode: GroupMode,
    inner: Mutex<Inner>,
    changed: Condvar,
}

impl GroupState {
    pub fn new(id: impl Into<String>, mode: GroupMode) -> Self {
        Self { id: id.into(), mode, inner: Mutex::new(Inner::default()), changed: Condvar::new() }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// AGC-FR-ZOCI: a dispatched agent that the join waits for.
    pub fn add_member(&self, turn_id: &str, nickname: &str) {
        self.lock().members.push(Member {
            turn_id: turn_id.to_string(),
            nickname: nickname.to_string(),
            status: MemberStatus::Pending,
        });
    }

    /// AGC-FR-ZOCI: a member turn ended. A member the join already skipped keeps
    /// its status.
    pub fn member_ended(&self, turn_id: &str, end: MemberEnd, now: Instant) {
        let mut inner = self.lock();
        let Some(member) = inner.members.iter_mut().find(|m| m.turn_id == turn_id) else {
            return;
        };
        if member.status != MemberStatus::Pending {
            return;
        }
        member.status = match end {
            MemberEnd::Finished => MemberStatus::Finished,
            MemberEnd::Failed => MemberStatus::Skipped(SkipReason::Failed),
            MemberEnd::Cancelled => MemberStatus::Skipped(SkipReason::Cancelled),
        };
        if let Some(join) = inner.join.as_mut() {
            match end {
                MemberEnd::Finished => join.note_finished(now),
                _ => join.note_unfinished(),
            }
        }
        drop(inner);
        self.changed.notify_all();
    }

    /// AGC-FR-ZOCI: no member is added after this. The quorum is counted over
    /// the members as they stand, and the members that already ended count at
    /// once.
    pub fn seal(&self, timeout: Duration, now: Instant) {
        let mut inner = self.lock();
        let mut join = Join::new(inner.members.len(), timeout);
        for member in &inner.members {
            match member.status {
                MemberStatus::Finished => join.note_finished(now),
                MemberStatus::Skipped(_) => join.note_unfinished(),
                MemberStatus::Pending => {}
            }
        }
        inner.join = Some(join);
        drop(inner);
        self.changed.notify_all();
    }

    /// AGC-FR-ZLWN: wait until the join is over. Returns at once for a join that
    /// is over. The members still running are marked skipped as `timed_out`.
    pub fn wait_join(&self, cancelled: &AtomicBool) -> JoinOutcome {
        let mut inner = self.lock();
        loop {
            if cancelled.load(Ordering::SeqCst) {
                return JoinOutcome::Cancelled;
            }
            let now = Instant::now();
            let mut wait = CANCEL_CHECK;
            if let Some(join) = inner.join.as_ref() {
                if join.is_over(now) {
                    let timed_out = join.timed_out(now);
                    let mut pending = Vec::new();
                    for member in inner.members.iter_mut() {
                        if member.status == MemberStatus::Pending {
                            member.status = MemberStatus::Skipped(SkipReason::TimedOut);
                            pending.push(PendingMember {
                                turn_id: member.turn_id.clone(),
                                nickname: member.nickname.clone(),
                            });
                        }
                    }
                    return JoinOutcome::Over { pending, timed_out };
                }
                if let Some(deadline) = join.deadline() {
                    wait = wait.min(deadline.saturating_duration_since(now));
                }
            }
            inner = self
                .changed
                .wait_timeout(inner, wait)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
    }

    /// AGC-FR-ANSK: the waiter's own turn was cancelled, so every member still
    /// running is marked skipped for `reason`. The caller cancels them.
    pub fn skip_pending(&self, reason: SkipReason) -> Vec<PendingMember> {
        let mut inner = self.lock();
        let mut pending = Vec::new();
        for member in inner.members.iter_mut() {
            if member.status == MemberStatus::Pending {
                member.status = MemberStatus::Skipped(reason);
                pending.push(PendingMember {
                    turn_id: member.turn_id.clone(),
                    nickname: member.nickname.clone(),
                });
            }
        }
        drop(inner);
        self.changed.notify_all();
        pending
    }

    /// AGC-FR-GNMP: the advice of a finished advisor.
    pub fn record_advice(&self, nickname: &str, advice: Advice) {
        let mut inner = self.lock();
        inner.advice.retain(|entry| entry.nickname != nickname);
        inner.advice.push(AdviceEntry { nickname: nickname.to_string(), advice });
    }

    /// AGC-FR-MWYP: the advice of the advisors that finished, in the order the
    /// members were dispatched.
    pub fn advice_entries(&self) -> Vec<AdviceEntry> {
        let inner = self.lock();
        inner
            .members
            .iter()
            .filter(|m| m.status == MemberStatus::Finished)
            .filter_map(|m| inner.advice.iter().find(|a| a.nickname == m.nickname).cloned())
            .collect()
    }

    /// AGC-FR-VDOO: each member with its status, in dispatch order.
    pub fn statuses(&self) -> Vec<(String, MemberStatus)> {
        self.lock().members.iter().map(|m| (m.nickname.clone(), m.status)).collect()
    }

    pub fn finished_count(&self) -> usize {
        self.lock().members.iter().filter(|m| m.status == MemberStatus::Finished).count()
    }

    pub fn member_count(&self) -> usize {
        self.lock().members.len()
    }

    /// AGC-FR-DIZM: questions an addressed agent offered and the group holds.
    pub fn hold_questions(&self, nickname: &str, text: String) {
        self.lock().questions.push(Held { nickname: nickname.to_string(), text });
    }

    /// AGC-FR-DIZM: a proposal an addressed agent offered and the group holds.
    pub fn hold_proposal(&self, nickname: &str, text: String) {
        self.lock().proposals.push(Held { nickname: nickname.to_string(), text });
    }

    /// AGC-FR-CZCU: the text of a reply that an addressed agent delivered, for the
    /// synthesis turn. The conversation a turn reads ends at the comment that
    /// addressed it, so the replies that follow reach the driver through here.
    pub fn record_reply(&self, nickname: &str, text: String) {
        self.lock().replies.push(Held { nickname: nickname.to_string(), text });
    }

    pub fn replies(&self) -> Vec<Held> {
        self.lock().replies.clone()
    }

    pub fn held_questions(&self) -> Vec<Held> {
        self.lock().questions.clone()
    }

    pub fn held_proposals(&self) -> Vec<Held> {
        self.lock().proposals.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(members: &[(&str, &str)]) -> GroupState {
        let group = GroupState::new("group-1", GroupMode::Synthesis);
        for (turn, nick) in members {
            group.add_member(turn, nick);
        }
        group
    }

    // AGC-FR-ZOCI: a join whose members all finished ends at once with nothing
    // to cancel.
    #[test]
    fn a_join_of_finished_members_ends_at_once() {
        let g = group(&[("t1", "a"), ("t2", "b")]);
        g.seal(Duration::from_secs(60), Instant::now());
        g.member_ended("t1", MemberEnd::Finished, Instant::now());
        g.member_ended("t2", MemberEnd::Finished, Instant::now());
        let never = AtomicBool::new(false);
        assert_eq!(g.wait_join(&never), JoinOutcome::Over { pending: vec![], timed_out: false });
        assert_eq!(g.finished_count(), 2);
    }

    // AGC-FR-ANSK, AGC-FR-ZLWN: at the timeout the running member is skipped as
    // timed out and is returned for cancelling.
    #[test]
    fn the_timeout_skips_the_members_still_running() {
        let g = group(&[("t1", "a"), ("t2", "b"), ("t3", "c")]);
        g.seal(Duration::ZERO, Instant::now());
        g.member_ended("t1", MemberEnd::Finished, Instant::now());
        g.member_ended("t2", MemberEnd::Finished, Instant::now());
        let never = AtomicBool::new(false);
        let outcome = g.wait_join(&never);
        assert_eq!(
            outcome,
            JoinOutcome::Over {
                pending: vec![PendingMember { turn_id: "t3".into(), nickname: "c".into() }],
                timed_out: true,
            }
        );
        // The cancellation that follows does not rewrite the reason.
        g.member_ended("t3", MemberEnd::Cancelled, Instant::now());
        assert_eq!(g.statuses()[2].1, MemberStatus::Skipped(SkipReason::TimedOut));
    }

    // AGC-FR-ZOCI, AGC-FR-ANSK: failed and cancelled members are skipped with
    // their own reason and do not count toward the quorum.
    #[test]
    fn failed_and_cancelled_members_are_skipped_and_do_not_count() {
        let g = group(&[("t1", "a"), ("t2", "b"), ("t3", "c")]);
        g.member_ended("t1", MemberEnd::Failed, Instant::now());
        g.member_ended("t2", MemberEnd::Cancelled, Instant::now());
        g.member_ended("t3", MemberEnd::Finished, Instant::now());
        g.seal(Duration::from_secs(60), Instant::now());
        let never = AtomicBool::new(false);
        // All three settled, so the join is over although the quorum of two was
        // never reached.
        assert_eq!(g.wait_join(&never), JoinOutcome::Over { pending: vec![], timed_out: false });
        assert_eq!(
            g.statuses().into_iter().map(|(_, s)| s).collect::<Vec<_>>(),
            vec![
                MemberStatus::Skipped(SkipReason::Failed),
                MemberStatus::Skipped(SkipReason::Cancelled),
                MemberStatus::Finished
            ]
        );
        assert_eq!(g.finished_count(), 1);
    }

    // AGC-FR-MWYP: a cancelled waiter leaves the wait.
    #[test]
    fn a_cancelled_waiter_leaves_the_wait() {
        let g = group(&[("t1", "a")]);
        g.seal(Duration::from_secs(60), Instant::now());
        let cancelled = AtomicBool::new(true);
        assert_eq!(g.wait_join(&cancelled), JoinOutcome::Cancelled);
    }

    // AGC-FR-MWYP: only the advice of a finished advisor reaches the Meta turn.
    #[test]
    fn only_finished_advisors_give_advice() {
        let g = group(&[("t1", "a"), ("t2", "b")]);
        g.record_advice("a", Advice { points: vec!["p".into()], questions: vec![] });
        g.record_advice("b", Advice { points: vec!["late".into()], questions: vec![] });
        g.member_ended("t1", MemberEnd::Finished, Instant::now());
        g.member_ended("t2", MemberEnd::Failed, Instant::now());
        let entries = g.advice_entries();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].nickname, "a");
    }

    // AGC-FR-CZCU: the group keeps the replies for the synthesis turn.
    #[test]
    fn the_group_keeps_the_replies_in_the_order_they_arrived() {
        let g = group(&[("t1", "a"), ("t2", "b")]);
        g.record_reply("b", "second member, first reply".into());
        g.record_reply("a", "first member, second reply".into());
        let replies = g.replies();
        assert_eq!(replies[0].nickname, "b");
        assert_eq!(replies[1].text, "first member, second reply");
    }

    // AGC-FR-DIZM: the group holds what its members offered.
    #[test]
    fn the_group_holds_questions_and_proposals() {
        let g = group(&[("t1", "a")]);
        g.hold_questions("a", "Who owns it?".into());
        g.hold_proposal("a", "Change the intro".into());
        assert_eq!(g.held_questions().len(), 1);
        assert_eq!(g.held_proposals()[0].text, "Change the intro");
    }
}

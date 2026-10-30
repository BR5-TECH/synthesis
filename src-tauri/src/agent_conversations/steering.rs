//! Steering: a comment the author posts to an agent while its turn runs.
//!
//! A dispatch for an agent whose turn is running in the same conversation gives
//! the comment to that turn (AGC-FR-ISKZ). The loop appends it to the exchange
//! before its next model call (CVL-FR-EZKC), and taking it restarts the
//! whole-turn deadline (CVL-FR-SZDM, AGC-FR-URXV). A turn that can take no
//! further message sends it to a new turn (AGC-FR-CEKN).

use super::*;

/// CVL-FR-QPEL: the fixed line that opens a steering message.
pub(super) const STEERING_PREFACE: &str = "The author posted this comment while you were \
working on your reply. Read it and take it into account before you continue.";

/// AGC-FR-URXV: write the deadline of a turn record from the instant it began.
fn stamp_deadline(turn: &mut AgentTurn, timeout: Duration) {
    let started = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let ends = started.saturating_add(timeout.as_millis() as i64);
    turn.deadline_started_at = Some(crate::notes::format_rfc3339_millis_utc(started));
    turn.deadline_at = Some(crate::notes::format_rfc3339_millis_utc(ends));
}

impl TurnRegistry {
    /// AGC-FR-URXV / CVL-FR-16: start the whole-turn deadline of a turn that
    /// now holds its slot, publish it, and answer with the instant it expires.
    ///
    /// Published under the lock for the reason
    /// [`begin_tool_call`](Self::begin_tool_call) gives.
    pub(super) fn start_deadline<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        turn_id: &str,
        timeout: Duration,
    ) -> std::time::Instant {
        let deadline = std::time::Instant::now() + timeout;
        let mut inner = self.lock();
        if let Some(live) = inner.live.get_mut(turn_id) {
            live.deadline = Some((timeout, deadline));
            stamp_deadline(&mut live.turn, timeout);
            publish(app, &live.turn.clone());
        }
        deadline
    }

    /// CVL-FR-SZDM: the instant the turn's whole-turn deadline expires now, or
    /// `fallback` where the turn is not live or has no deadline.
    pub(super) fn deadline_of(
        &self,
        turn_id: &str,
        fallback: std::time::Instant,
    ) -> std::time::Instant {
        self.lock()
            .live
            .get(turn_id)
            .and_then(|live| live.deadline)
            .map(|(_, deadline)| deadline)
            .unwrap_or(fallback)
    }

    /// AGC-FR-ISKZ: give a comment to the running turn of `agent_id` in
    /// `origin`, where one still takes steering messages (CVL-FR-YNOE).
    ///
    /// The most recently started such turn takes it. Taking it restarts the
    /// turn's deadline where the turn holds one (CVL-FR-SZDM) and publishes the
    /// turn (AGC-FR-URXV). `None` where no turn takes it, and the caller then
    /// registers a new turn (AGC-FR-CEKN).
    pub(super) fn offer_steering<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        agent_id: &str,
        origin: &ConversationOrigin,
        comment_id: &str,
    ) -> Option<AgentTurn> {
        let mut inner = self.lock();
        let live = inner
            .live
            .values_mut()
            .filter(|live| {
                live.turn.agent_id == agent_id
                    && &live.turn.origin == origin
                    && live.steering_open
                    && !live.delivering
                    && !live.cancelled.load(Ordering::SeqCst)
            })
            .max_by_key(|live| ordinal(&live.turn.id))?;
        live.steering.push(comment_id.to_string());
        if let Some((timeout, _)) = live.deadline {
            live.deadline = Some((timeout, std::time::Instant::now() + timeout));
            stamp_deadline(&mut live.turn, timeout);
        }
        let turn = live.turn.clone();
        publish(app, &turn);
        Some(turn)
    }

    /// CVL-FR-EZKC: the steering messages the turn took and the loop did not
    /// append yet, oldest first. The turn holds none of them afterwards.
    pub(super) fn take_steering(&self, turn_id: &str) -> Vec<String> {
        self.lock()
            .live
            .get_mut(turn_id)
            .map(|live| std::mem::take(&mut live.steering))
            .unwrap_or_default()
    }

    /// CVL-FR-YNOE: the turn takes no further steering message. Answers with
    /// the messages it took and the loop did not append yet, oldest first.
    pub(super) fn close_steering(&self, turn_id: &str) -> Vec<String> {
        let mut inner = self.lock();
        let Some(live) = inner.live.get_mut(turn_id) else { return Vec::new() };
        live.steering_open = false;
        std::mem::take(&mut live.steering)
    }

    /// Whether the turn holds a steering message the loop did not append yet.
    pub(super) fn has_steering(&self, turn_id: &str) -> bool {
        self.lock().live.get(turn_id).is_some_and(|live| !live.steering.is_empty())
    }
}

/// CVL-FR-EZKC / CVL-FR-QPEL: one user message for one steering comment. It
/// opens with the fixed preface, and carries the comment on the terms the
/// `current_comment` section carries the comment that addressed the turn.
fn steering_message(
    roots: Roots<'_>,
    thread: &Discussion,
    comment_id: &str,
    with_images: bool,
) -> rig::completion::Message {
    let section = current_comment_section(roots, thread, comment_id);
    if with_images {
        if let Some(rig::completion::Message::User { content }) =
            multimodal_message_of(std::slice::from_ref(&section))
        {
            let mut contents =
                vec![rig::completion::message::UserContent::text(STEERING_PREFACE.to_string())];
            contents.extend(content);
            return rig::completion::Message::User { content: contents };
        }
    }
    rig::completion::Message::user(format!(
        "{STEERING_PREFACE}\n\n{}",
        render_sections(std::slice::from_ref(&section))
    ))
}

/// CVL-FR-EZKC / CVL-FR-YNOE / CVL-FR-ASBR: append the steering messages the
/// turn took to the exchange, before the next model call. Before the last call
/// under the logical bound, the turn first stops taking further messages.
///
/// Answers with the number of messages it appended.
pub(super) fn append_steering<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    conversation: &mut compaction::Conversation,
    last_call: bool,
) -> usize {
    let ids = if last_call {
        turns.close_steering(&plan.turn_id)
    } else {
        turns.take_steering(&plan.turn_id)
    };
    if ids.is_empty() {
        return 0;
    }
    let roots = plan.roots.borrowed();
    let Some(thread) = resolve_origin(roots, &plan.origin) else {
        logging::log_warn(
            app,
            turns.buffer(),
            &[Domain::Ai, Domain::Backend],
            "steering messages could not be read",
            log_fields! {
                "turnId" => &plan.turn_id,
                "agent" => &plan.agent.nickname,
                "agentId" => &plan.agent.id,
                "provider" => &plan.provider,
                "model" => &plan.agent.model_id,
                "commentIds" => ids.join(","),
            },
        );
        return 0;
    };
    for id in &ids {
        conversation.exchange.push(steering_message(roots, &thread, id, conversation.with_images));
    }
    // CVL-FR-ASBR: the ids and the count, never a body.
    logging::log_debug(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Backend],
        "agent turn took steering messages",
        log_fields! {
            "turnId" => &plan.turn_id,
            "agent" => &plan.agent.nickname,
            "agentId" => &plan.agent.id,
            "provider" => &plan.provider,
            "model" => &plan.agent.model_id,
            "commentIds" => ids.join(","),
            "appended" => ids.len(),
        },
    );
    ids.len()
}

/// AGC-FR-CEKN / AGC-FR-YBVM: what happens to the steering messages a turn
/// still held when it ended. A cancelled turn discards them. Any other end
/// registers one new turn for the same agent and origin, triggered by the
/// newest of them, and the author sees no refusal for it.
pub(super) fn redispatch_unread_steering<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    plan: &TurnPlan,
    state: AgentTurnState,
    unread: Vec<String>,
) {
    use tauri::Manager;
    let turns = app.state::<TurnRegistry>();
    let Some(newest) = unread.last() else { return };
    if state == AgentTurnState::Cancelled {
        logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "steering messages discarded with a cancelled turn",
            log_fields! { "turnId" => &plan.turn_id, "discarded" => unread.len() },
        );
        return;
    }
    let outcome = dispatch_impl(
        app,
        plan.roots.borrowed(),
        &plan.project_key,
        &plan.agent.nickname,
        plan.origin.clone(),
        newest,
    );
    match outcome {
        Ok(turn) => logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "steering messages went to a new turn",
            log_fields! {
                "turnId" => &plan.turn_id,
                "newTurnId" => &turn.id,
                "commentIds" => unread.join(","),
            },
        ),
        Err(failure) => logging::log_warn(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "steering messages could not go to a new turn",
            log_fields! {
                "turnId" => &plan.turn_id,
                "commentIds" => unread.join(","),
                "failure" => failure,
            },
        ),
    }
}

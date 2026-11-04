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

/// CVL-FR-VCYR: the fixed line that opens a chunk message.
pub const PARTIAL_SPEECH_LINE: &str = "Partial speech: the author is still speaking. This is \
a part of what the author says. More text follows. Do not reply to this part alone. You can use \
a tool to prepare, or give an empty reply.";

/// CVL-FR-APIT: the fixed line that the loop appends when the hold of a
/// placeholder ends with no steering message to append.
pub const SPEECH_WITHDRAWN_LINE: &str = "Speech ended: the author stopped speaking and posted \
no comment. Disregard the partial speech. Reply to the comment that addressed you.";

/// CVL-FR-WLTP: the fixed line that opens the final steering message of a
/// speech, in place of [`STEERING_PREFACE`].
pub const FULL_SPEECH_LINE: &str = "Full speech: the author stopped speaking. This comment is \
all that the author said. It replaces the partial speech before it.";

/// AGC-FR-ISKZ, AGC-FR-KXOJ, AGC-FR-TCDQ: one steering message that a turn
/// took. The registry holds it in memory only, and nothing writes it to disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SteeringItem {
    /// A comment that the author posted while the turn ran (AGC-FR-ISKZ).
    Comment(String),
    /// A chunk of the author's speech. It is not a comment (AGC-FR-KXOJ).
    Chunk(String),
    /// The comment that ends a speech (AGC-FR-TCDQ, CVL-FR-WLTP).
    SpeechEnd(String),
}

impl SteeringItem {
    /// AGC-FR-JPPB: the comment id of a comment item. A chunk has none.
    pub(super) fn comment_id(&self) -> Option<&str> {
        match self {
            SteeringItem::Comment(id) | SteeringItem::SpeechEnd(id) => Some(id),
            SteeringItem::Chunk(_) => None,
        }
    }
}

/// AGC-FR-ISKZ, AGC-FR-KXOJ: put one steering message on a live turn.
///
/// CVL-FR-SZDM: the whole-turn deadline restarts where the turn holds one.
/// AGC-FR-ISCZ: a turn that holds its reply has no deadline, and the end of the
/// hold restarts it.
pub(super) fn give_steering(live: &mut Live, item: SteeringItem) {
    // AGC-FR-DTCL: a comment item is a received comment. A chunk is not.
    if let Some(comment_id) = item.comment_id() {
        live.received.push(comment_id.to_string());
    }
    live.steering.push(item);
    if live.turn.reply_held {
        return;
    }
    if let Some((timeout, _)) = live.deadline {
        live.deadline = Some((timeout, std::time::Instant::now() + timeout));
        stamp_deadline(&mut live.turn, timeout);
    }
}

/// AGC-FR-URXV: write the deadline of a turn record from the instant it began.
pub(super) fn stamp_deadline(turn: &mut AgentTurn, timeout: Duration) {
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
    ///
    /// AGC-FR-SJQP: a chunk of a speech is given on the same terms, and
    /// `speech_id` then binds the turn to that speech (AGC-FR-QYNZ).
    pub(super) fn offer_steering<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        agent_id: &str,
        origin: &ConversationOrigin,
        item: SteeringItem,
        speech_id: Option<&str>,
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
        give_steering(live, item);
        if let Some(speech_id) = speech_id {
            live.turn.speech_id = Some(speech_id.to_string());
        }
        let turn = live.turn.clone();
        publish(app, &turn);
        drop(inner);
        self.speech_changed.notify_all();
        Some(turn)
    }

    /// CVL-FR-EZKC: the steering messages the turn took and the loop did not
    /// append yet, oldest first. The turn holds none of them afterwards.
    pub(super) fn take_steering(&self, turn_id: &str) -> Vec<SteeringItem> {
        self.lock()
            .live
            .get_mut(turn_id)
            .map(|live| std::mem::take(&mut live.steering))
            .unwrap_or_default()
    }

    /// CVL-FR-YNOE: the turn takes no further steering message. Answers with
    /// the messages it took and the loop did not append yet, oldest first.
    pub(super) fn close_steering(&self, turn_id: &str) -> Vec<SteeringItem> {
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
///
/// CVL-FR-WLTP: the final steering message of a speech has `preface` in place
/// of the line of CVL-FR-QPEL.
fn steering_message(
    roots: Roots<'_>,
    thread: &Discussion,
    comment_id: &str,
    with_images: bool,
    preface: &str,
) -> rig::completion::Message {
    let section = current_comment_section(roots, thread, comment_id);
    if with_images {
        if let Some(rig::completion::Message::User { content }) =
            multimodal_message_of(std::slice::from_ref(&section))
        {
            let mut contents =
                vec![rig::completion::message::UserContent::text(preface.to_string())];
            contents.extend(content);
            return rig::completion::Message::User { content: contents };
        }
    }
    rig::completion::Message::user(format!(
        "{preface}\n\n{}",
        render_sections(std::slice::from_ref(&section))
    ))
}

/// CVL-FR-VCYR: one user message for one chunk of speech, under the fixed
/// partial-speech line.
pub(super) fn chunk_message(text: &str) -> rig::completion::Message {
    rig::completion::Message::user(format!("{PARTIAL_SPEECH_LINE}\n\n{text}"))
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
    let items = if last_call {
        turns.close_steering(&plan.turn_id)
    } else {
        turns.take_steering(&plan.turn_id)
    };
    if items.is_empty() {
        return 0;
    }
    let ids: Vec<&str> = items.iter().filter_map(SteeringItem::comment_id).collect();
    let chunks = items.len() - ids.len();
    let roots = plan.roots.borrowed();
    let thread = if ids.is_empty() { None } else { resolve_origin(roots, &plan.origin) };
    if !ids.is_empty() && thread.is_none() {
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
    }
    let mut appended = 0;
    for item in &items {
        let message = match (item, thread.as_ref()) {
            // CVL-FR-VCYR: a chunk needs no read of the conversation.
            (SteeringItem::Chunk(text), _) => chunk_message(text),
            (SteeringItem::Comment(id), Some(thread)) => {
                steering_message(roots, thread, id, conversation.with_images, STEERING_PREFACE)
            }
            (SteeringItem::SpeechEnd(id), Some(thread)) => {
                steering_message(roots, thread, id, conversation.with_images, FULL_SPEECH_LINE)
            }
            _ => continue,
        };
        conversation.exchange.push(message);
        appended += 1;
    }
    if appended == 0 {
        return 0;
    }
    // CVL-FR-ASBR: the ids and the count, never a body. A chunk has no id, so
    // the record counts chunks and never carries their text.
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
            "chunks" => chunks,
            "appended" => appended,
        },
    );
    appended
}

/// AGC-FR-CEKN / AGC-FR-YBVM: what happens to the steering messages a turn
/// still held when it ended. A cancelled turn discards them. Any other end
/// registers one new turn for the same agent and origin, triggered by the
/// newest of them, and the author sees no refusal for it.
pub(super) fn redispatch_unread_steering<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    plan: &TurnPlan,
    state: AgentTurnState,
    unread: Vec<SteeringItem>,
) {
    use tauri::Manager;
    let turns = app.state::<TurnRegistry>();
    // AGC-FR-JPPB: only the comments count. A chunk registers no new turn.
    let unread: Vec<&str> = unread.iter().filter_map(SteeringItem::comment_id).collect();
    let Some(newest) = unread.last().copied() else { return };
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

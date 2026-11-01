//! Speech turns: a speech of the author steers the turns of the agents that it
//! reaches, while the author speaks.
//!
//! The first chunk of a speech registers a turn or steers a running one
//! (AGC-FR-SJQP). Each later chunk steers the bound turns as a chunk message,
//! which is held in memory only (AGC-FR-KXOJ). While the speech is open, a
//! bound turn holds a reply that asks for no tool (CVL-FR-SZKK). The speech
//! ends with the posted comment (AGC-FR-TCDQ), with no comment (AGC-FR-NFYK),
//! or when it is idle for too long (AGC-FR-YLJW). A speech ends once
//! (AGC-FR-TCXX).

use std::collections::HashSet;

use super::*;

/// AGC-FR-BMLY: the typed refusal of a dispatch for a speech that ended.
pub const FAIL_SPEECH_ENDED: &str = "speech_ended";

/// AGC-FR-YLJW: a speech that gets no command for this long ends with no
/// comment.
pub const SPEECH_IDLE_LIMIT: Duration = Duration::from_secs(120);

/// How often a held turn reads the speech clock again when nothing wakes it.
/// This decides only how soon an idle speech is found. No result depends on
/// it, because each command and each round of the loop also look for idle
/// speeches.
const HOLD_POLL: Duration = Duration::from_secs(1);

/// AGC-FR-YLJW: where the idle limit reads the time. A collaborator, so that a
/// test moves the time forward and does not wait for it.
pub trait SpeechClock: Send + Sync {
    fn now(&self) -> std::time::Instant;
}

/// The clock of the application: the monotonic clock of the system.
pub struct SystemSpeechClock;

impl SpeechClock for SystemSpeechClock {
    fn now(&self) -> std::time::Instant {
        std::time::Instant::now()
    }
}

/// AGC-FR-VILT: one agent that a speech reached, in one conversation.
#[derive(Clone)]
pub(super) struct Reach {
    agent_id: String,
    nickname: String,
    origin: ConversationOrigin,
    project_key: String,
}

/// One open speech.
pub(super) struct OpenSpeech {
    /// AGC-FR-YLJW: the last instant that a command named this speech.
    touched: std::time::Instant,
    reached: Vec<Reach>,
}

/// AGC-FR-TCXX, AGC-FR-BMLY: the open speeches, and the ids of the ended ones.
/// Memory only, until the application quits.
pub(super) struct SpeechBook {
    clock: Arc<dyn SpeechClock>,
    open: HashMap<String, OpenSpeech>,
    ended: HashSet<String>,
}

impl Default for SpeechBook {
    fn default() -> Self {
        Self { clock: Arc::new(SystemSpeechClock), open: HashMap::new(), ended: HashSet::new() }
    }
}

impl SpeechBook {
    fn is_open(&self, speech_id: &str) -> bool {
        self.open.contains_key(speech_id)
    }

    /// AGC-FR-YLJW: the open speeches that are idle for the limit or longer.
    fn idle(&self) -> Vec<String> {
        let now = self.clock.now();
        self.open
            .iter()
            .filter(|(_, speech)| {
                now.saturating_duration_since(speech.touched) >= SPEECH_IDLE_LIMIT
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// AGC-FR-YLJW: a command named this speech. Answers whether it is open.
    fn touch(&mut self, speech_id: &str) -> bool {
        let now = self.clock.now();
        match self.open.get_mut(speech_id) {
            Some(speech) => {
                speech.touched = now;
                true
            }
            None => false,
        }
    }
}

/// How a speech ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SpeechEnding<'a> {
    /// AGC-FR-TCDQ: the author posted this comment.
    Comment(&'a str),
    /// AGC-FR-NFYK: there is no comment.
    Nothing,
}

/// What ending a speech did to one bound turn, under the registry lock.
struct Unbound {
    turn: AgentTurn,
    /// AGC-FR-NFYK: the speech registered this turn and it has no trigger
    /// comment, so a speech with no comment cancels it.
    cancel: bool,
    /// AGC-FR-TCDQ: the turn took the final steering message.
    took_comment: bool,
}

impl TurnRegistry {
    /// The clock of the idle limit. Test-only: the application reads the
    /// system clock.
    #[cfg(test)]
    pub(super) fn set_speech_clock(&self, clock: Arc<dyn SpeechClock>) {
        self.lock().speeches.clock = clock;
    }

    /// AGC-FR-BMLY: open the speech where it is new, and touch it
    /// (AGC-FR-YLJW). `false` where a speech with this id already ended.
    ///
    /// Opened before the dispatch, so a turn that the dispatch binds is bound
    /// to an open speech from its first round (CVL-FR-SZKK).
    fn open_speech(&self, speech_id: &str) -> bool {
        let mut inner = self.lock();
        if inner.speeches.ended.contains(speech_id) {
            return false;
        }
        let now = inner.speeches.clock.now();
        inner
            .speeches
            .open
            .entry(speech_id.to_string())
            .or_insert_with(|| OpenSpeech { touched: now, reached: Vec::new() })
            .touched = now;
        true
    }

    /// AGC-FR-SJQP / AGC-FR-VILT: record that the speech reached the agent of
    /// `turn`, and open the speech where it is new.
    ///
    /// A speech that ended while the dispatch ran binds nothing: the turn is
    /// unbound at once (AGC-FR-QYNZ).
    fn record_reach<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        speech_id: &str,
        turn: &AgentTurn,
        project_key: &str,
    ) -> AgentTurn {
        let mut inner = self.lock();
        if inner.speeches.ended.contains(speech_id) {
            let Some(live) = inner.live.get_mut(&turn.id) else { return turn.clone() };
            if live.turn.speech_id.as_deref() == Some(speech_id) {
                live.turn.speech_id = None;
                publish(app, &live.turn.clone());
            }
            return live.turn.clone();
        }
        let now = inner.speeches.clock.now();
        let speech = inner
            .speeches
            .open
            .entry(speech_id.to_string())
            .or_insert_with(|| OpenSpeech { touched: now, reached: Vec::new() });
        speech.touched = now;
        let known = speech
            .reached
            .iter()
            .any(|reach| reach.agent_id == turn.agent_id && reach.origin == turn.origin);
        if !known {
            speech.reached.push(Reach {
                agent_id: turn.agent_id.clone(),
                nickname: turn.nickname.clone(),
                origin: turn.origin.clone(),
                project_key: project_key.to_string(),
            });
        }
        inner.live.get(&turn.id).map(|live| live.turn.clone()).unwrap_or_else(|| turn.clone())
    }

    /// AGC-FR-KXOJ: give `text` as a chunk message to each `running` turn that
    /// is bound to the speech and still takes steering messages. A turn that
    /// takes no further message drops it. Empty for a speech that is unknown
    /// or ended (AGC-FR-TCXX).
    fn steer_speech<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        speech_id: &str,
        text: &str,
    ) -> Vec<AgentTurn> {
        let mut inner = self.lock();
        if !inner.speeches.touch(speech_id) {
            return Vec::new();
        }
        let mut taken = Vec::new();
        for live in inner.live.values_mut().filter(|live| {
            live.turn.speech_id.as_deref() == Some(speech_id)
                && live.steering_open
                && !live.delivering
                && !live.cancelled.load(Ordering::SeqCst)
        }) {
            give_steering(live, SteeringItem::Chunk(text.to_string()));
            publish(app, &live.turn);
            taken.push(live.turn.clone());
        }
        drop(inner);
        self.speech_changed.notify_all();
        taken.sort_by_key(|turn| ordinal(&turn.id));
        taken
    }

    /// AGC-FR-TCXX: end the speech once. Each bound turn is unbound, and it
    /// drops the chunk messages the loop did not append yet (AGC-FR-FPYB,
    /// AGC-FR-NFYK). With a comment, each bound turn that still takes steering
    /// messages takes it as its final steering message (AGC-FR-TCDQ).
    ///
    /// `None` where the speech is unknown or already ended.
    fn close_speech<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        speech_id: &str,
        ending: SpeechEnding<'_>,
    ) -> Option<(OpenSpeech, Vec<Unbound>)> {
        let mut inner = self.lock();
        let speech = inner.speeches.open.remove(speech_id)?;
        inner.speeches.ended.insert(speech_id.to_string());
        let mut unbound = Vec::new();
        for live in
            inner.live.values_mut().filter(|live| live.turn.speech_id.as_deref() == Some(speech_id))
        {
            live.turn.speech_id = None;
            live.steering.retain(|item| !matches!(item, SteeringItem::Chunk(_)));
            let takes =
                live.steering_open && !live.delivering && !live.cancelled.load(Ordering::SeqCst);
            let took_comment = match ending {
                SpeechEnding::Comment(comment_id) if takes => {
                    give_steering(live, SteeringItem::SpeechEnd(comment_id.to_string()));
                    if live.registered_by.as_deref() == Some(speech_id) {
                        live.turn.trigger_comment_id = Some(comment_id.to_string());
                    }
                    true
                }
                _ => false,
            };
            let cancel = ending == SpeechEnding::Nothing
                && live.registered_by.as_deref() == Some(speech_id)
                && live.turn.trigger_comment_id.is_none();
            publish(app, &live.turn);
            unbound.push(Unbound { turn: live.turn.clone(), cancel, took_comment });
        }
        drop(inner);
        self.speech_changed.notify_all();
        unbound.sort_by_key(|u| ordinal(&u.turn.id));
        Some((speech, unbound))
    }

    /// AGC-FR-YLJW: the open speeches that are idle for the limit or longer.
    fn idle_speeches(&self) -> Vec<String> {
        self.lock().speeches.idle()
    }

    /// CVL-FR-SZKK, AGC-FR-QYNZ, AGC-FR-ISCZ: hold the reply of a turn that is
    /// bound to an open speech, until the turn takes a steering message or the
    /// speech ends. `reply_held` is true and the deadline is null while it
    /// waits, and the deadline restarts in full when the wait ends
    /// (CVL-FR-TVNM).
    ///
    /// Returns at once for a turn bound to no open speech, and for a turn that
    /// takes no further steering message (CVL-FR-WHEY). Answers whether it
    /// held the reply.
    pub(super) fn wait_for_speech<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        turn_id: &str,
    ) -> bool {
        let mut inner = self.lock();
        let Some(speech_id) = inner.live.get(turn_id).and_then(|l| l.turn.speech_id.clone()) else {
            return false;
        };
        let open = inner.speeches.is_open(&speech_id);
        let Some(live) = inner.live.get_mut(turn_id) else { return false };
        if !open {
            // AGC-FR-QYNZ: a turn is bound to no speech that ended.
            live.turn.speech_id = None;
            publish(app, &live.turn);
            return false;
        }
        if !live.steering_open || !live.steering.is_empty() {
            return false;
        }
        live.turn.reply_held = true;
        live.turn.deadline_started_at = None;
        live.turn.deadline_at = None;
        publish(app, &live.turn);
        logging::log_debug(
            app,
            self.buffer(),
            &[Domain::Ai, Domain::Backend],
            "agent reply held for a speech",
            log_fields! {
                "turnId" => turn_id,
                "agent" => &live.turn.nickname,
                "agentId" => &live.turn.agent_id,
                "speechId" => &speech_id,
            },
        );
        loop {
            let stop = match inner.live.get(turn_id) {
                None => true,
                Some(live) => {
                    live.cancelled.load(Ordering::SeqCst)
                        || !live.steering.is_empty()
                        || live.turn.speech_id.as_deref() != Some(speech_id.as_str())
                        || !inner.speeches.is_open(&speech_id)
                }
            };
            if stop {
                break;
            }
            if !inner.speeches.idle().is_empty() {
                drop(inner);
                expire_idle_speeches(app);
                inner = self.lock();
                continue;
            }
            inner = match self.speech_changed.wait_timeout(inner, HOLD_POLL) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
        if let Some(live) = inner.live.get_mut(turn_id) {
            live.turn.reply_held = false;
            if let Some((timeout, _)) = live.deadline {
                live.deadline = Some((timeout, std::time::Instant::now() + timeout));
                stamp_deadline(&mut live.turn, timeout);
            }
            publish(app, &live.turn);
        }
        true
    }
}

/// AGC-FR-SJQP / AGC-FR-06: the input of a turn that a speech registers. The
/// history holds the whole conversation as it stands (AGC-FR-13), and
/// `current_comment` holds the first chunk under the partial-speech line
/// (CVL-FR-VCYR), named for the speaking author as a comment of that author
/// is (AGC-FR-ANBX, AGC-FR-08).
pub(super) fn speech_input(
    roots: Roots<'_>,
    origin: &ConversationOrigin,
    text: &str,
    speaker: &Participant,
    budget: &DocumentBudget,
) -> BuiltInput {
    // No comment has an empty id, so the history stops at no comment.
    let mut built = build_input_within(roots, origin, "", budget);
    let chunk =
        InputPart::Text(format!("{PARTIAL_SPEECH_LINE}\n\n@{}:\n{text}\n", handle_of(speaker)));
    let current = section_of_parts(TAG_CURRENT_COMMENT, Vec::new(), vec![chunk]);
    // AGC-FR-12: an origin that resolves to nothing has no input at all.
    if let Some(section) = built.sections.iter_mut().find(|s| s.tag == TAG_CURRENT_COMMENT) {
        *section = current;
    }
    built
}

/// AGC-FR-SJQP / AGC-FR-BMLY: the whole of `dispatch_speech_turn`, reachable
/// without the Tauri runtime.
#[allow(clippy::too_many_arguments)] // The contract arguments, the roots, and the speaker.
pub fn dispatch_speech_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    project_key: &str,
    nickname: &str,
    origin: ConversationOrigin,
    speech_id: &str,
    text: &str,
    speaker: &Participant,
) -> Result<AgentTurn, String> {
    use tauri::Manager;
    expire_idle_speeches(app);
    let turns = app.state::<TurnRegistry>();
    if !turns.open_speech(speech_id) {
        logging::log_warn(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "speech turn refused",
            log_fields! {
                "agent" => nickname,
                "speechId" => speech_id,
                "failure" => FAIL_SPEECH_ENDED,
            },
        );
        return Err(FAIL_SPEECH_ENDED.to_string());
    }
    let trigger = Trigger::Speech { speech_id, text, speaker };
    let turn = dispatch_with(app, roots, project_key, nickname, origin, trigger)?;
    let turn = turns.record_reach(app, speech_id, &turn, project_key);
    // The ids and the shape, never the text of the speech.
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai],
        "speech reached an agent turn",
        log_fields! {
            "turnId" => &turn.id,
            "agent" => &turn.nickname,
            "agentId" => &turn.agent_id,
            "speechId" => speech_id,
            "chunkChars" => text.chars().count(),
        },
    );
    Ok(turn)
}

/// AGC-FR-KXOJ / AGC-FR-TCXX: the whole of `steer_speech_turn`.
pub fn steer_speech_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    speech_id: &str,
    text: &str,
) -> Vec<AgentTurn> {
    use tauri::Manager;
    expire_idle_speeches(app);
    let turns = app.state::<TurnRegistry>();
    let taken = turns.steer_speech(app, speech_id, text);
    logging::log_debug(
        app,
        turns.buffer(),
        &[Domain::Ai],
        "speech chunk steered agent turns",
        log_fields! {
            "speechId" => speech_id,
            "turns" => taken.len(),
            "chunkChars" => text.chars().count(),
        },
    );
    taken
}

/// AGC-FR-TCDQ, AGC-FR-FPYB, AGC-FR-VILT, AGC-FR-NFYK, AGC-FR-TCXX: the whole
/// of `end_speech_turn`.
///
/// `roots` and `comment_id` come together: a speech that ends with a comment
/// can register a new turn (AGC-FR-VILT), and that reads the project. Answers
/// the turns that the end reached.
pub fn end_speech_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Option<Roots<'_>>,
    speech_id: &str,
    comment_id: Option<&str>,
) -> Vec<AgentTurn> {
    expire_idle_speeches(app);
    let ending = match (comment_id, roots) {
        (Some(id), Some(roots)) => (SpeechEnding::Comment(id), Some(roots)),
        _ => (SpeechEnding::Nothing, None),
    };
    end_speech(app, speech_id, ending.0, ending.1)
}

/// AGC-FR-YLJW: end each speech that is idle for the limit or longer, as a
/// speech with no comment ends (AGC-FR-NFYK).
pub fn expire_idle_speeches<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    use tauri::Manager;
    let Some(turns) = app.try_state::<TurnRegistry>() else { return };
    for speech_id in turns.idle_speeches() {
        logging::log_info(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "idle speech ended",
            log_fields! { "speechId" => &speech_id },
        );
        end_speech(app, &speech_id, SpeechEnding::Nothing, None);
    }
}

fn end_speech<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    speech_id: &str,
    ending: SpeechEnding<'_>,
    roots: Option<Roots<'_>>,
) -> Vec<AgentTurn> {
    use tauri::Manager;
    let turns = app.state::<TurnRegistry>();
    let Some((speech, unbound)) = turns.close_speech(app, speech_id, ending) else {
        logging::log_debug(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "speech end changed nothing",
            log_fields! { "speechId" => speech_id },
        );
        return Vec::new();
    };
    let progress_registry = app.state::<ProgressRegistry>();
    let mut reached: Vec<AgentTurn> = Vec::new();
    let mut served: Vec<(String, ConversationOrigin)> = Vec::new();
    for entry in unbound {
        if entry.took_comment {
            served.push((entry.turn.agent_id.clone(), entry.turn.origin.clone()));
        }
        if entry.cancel {
            // AGC-FR-NFYK: the turn and its output go.
            match cancel_impl(app, &turns, &progress_registry, &entry.turn.id) {
                Ok(turn) => reached.push(turn),
                Err(_) => reached.push(entry.turn),
            }
        } else {
            reached.push(entry.turn);
        }
    }
    // AGC-FR-VILT: an agent that the speech reached and whose bound turn took
    // no final message gets a new turn with that comment as its trigger.
    let mut redispatched = 0usize;
    if let (SpeechEnding::Comment(comment_id), Some(roots)) = (ending, roots) {
        for reach in &speech.reached {
            let key = (reach.agent_id.clone(), reach.origin.clone());
            if served.contains(&key) {
                continue;
            }
            served.push(key);
            match dispatch_impl(
                app,
                roots,
                &reach.project_key,
                &reach.nickname,
                reach.origin.clone(),
                comment_id,
            ) {
                Ok(turn) => {
                    redispatched += 1;
                    reached.push(turn);
                }
                Err(failure) => logging::log_warn(
                    app,
                    turns.buffer(),
                    &[Domain::Ai],
                    "speech comment could not go to a new turn",
                    log_fields! {
                        "speechId" => speech_id,
                        "agent" => &reach.nickname,
                        "failure" => failure,
                    },
                ),
            }
        }
    }
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai],
        "speech ended",
        log_fields! {
            "speechId" => speech_id,
            "withComment" => matches!(ending, SpeechEnding::Comment(_)),
            "turns" => reached.len(),
            "newTurns" => redispatched,
        },
    );
    reached
}

// ---------------------------------------------------------------------------
// Tauri commands (AGC contract surface)
// ---------------------------------------------------------------------------

/// AGC-FR-SJQP: `dispatch_speech_turn(nickname, origin, speech_id, text)`.
#[tauri::command]
pub fn dispatch_speech_turn<R: tauri::Runtime>(
    nickname: String,
    origin: ConversationOrigin,
    speech_id: String,
    text: String,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
    settings: State<'_, GlobalSettingsStore>,
) -> Result<AgentTurn, String> {
    let key = project.anchor().ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    let store = project.require_store()?;
    // AGC-FR-ANBX: the speaking author is the participant this machine writes
    // as. A speech is not refused for an identity that does not resolve: it is
    // then the local participant.
    let speaker = comments::acting_participant(&settings, &project).unwrap_or_else(|reason| {
        use tauri::Manager;
        logging::log_warn(
            &app,
            app.state::<TurnRegistry>().buffer(),
            &[Domain::Ai],
            "speaking author did not resolve",
            log_fields! { "reason" => reason },
        );
        Participant::local_human()
    });
    dispatch_speech_impl(
        &app,
        Roots { worktree: &root, store: &store },
        &key,
        &nickname,
        origin,
        &speech_id,
        &text,
        &speaker,
    )
}

/// AGC-FR-KXOJ: `steer_speech_turn(speech_id, text)`.
#[tauri::command]
pub fn steer_speech_turn<R: tauri::Runtime>(
    speech_id: String,
    text: String,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
) -> Result<Vec<AgentTurn>, String> {
    if project.anchor().is_none() {
        return Err(ERR_NO_PROJECT_OPEN.to_string());
    }
    Ok(steer_speech_impl(&app, &speech_id, &text))
}

/// AGC-FR-TCDQ / AGC-FR-NFYK: `end_speech_turn(speech_id, comment_id?)`.
#[tauri::command]
pub fn end_speech_turn<R: tauri::Runtime>(
    speech_id: String,
    comment_id: Option<String>,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
) -> Result<Vec<AgentTurn>, String> {
    if project.anchor().is_none() {
        return Err(ERR_NO_PROJECT_OPEN.to_string());
    }
    let Some(comment_id) = comment_id else {
        return Ok(end_speech_impl(&app, None, &speech_id, None));
    };
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    let store = project.require_store()?;
    Ok(end_speech_impl(
        &app,
        Some(Roots { worktree: &root, store: &store }),
        &speech_id,
        Some(&comment_id),
    ))
}

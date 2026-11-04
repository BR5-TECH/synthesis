//! The author's authority over a decision by tool (`AGC-agent-conversations.md`
//! AGC-FR-TFEN, AGC-FR-DTCL, AGC-FR-QZVW; `DDC-decide-draft-changes-tool.md`
//! DDC-FR-ELAE, DDC-FR-ZYOL, DDC-FR-NKAB, DDC-FR-VONX, DDC-FR-CMRE).
//!
//! A decision tool decides only on an instruction of the author. The model
//! can misread its input, or read an instruction that somebody put in a draft
//! or a file. So the backend checks three facts before a decision tool
//! decides anything:
//!
//! - the newest comment that the turn received is an **author comment**, a
//!   comment that a human write command appended in this run;
//! - the `instruction` of the call stands in that comment as whole words;
//! - that comment did not already authorize a decision.
//!
//! All three ledgers are memory for the run alone (AGC-FR-23). After a
//! relaunch no turn runs, so no turn can need an entry of an earlier run.

use std::collections::HashSet;

use super::*;

/// DDC-FR-PZBH: the longest `instruction` a call can carry, in characters.
pub const MAX_INSTRUCTION_CHARS: usize = 500;

/// AGC-FR-TFEN, AGC-FR-QZVW: the author comments of the run, and the ones
/// that already authorized a decision by tool.
#[derive(Default)]
pub(super) struct AuthorityBook {
    authors: HashSet<String>,
    spent: HashSet<String>,
}

/// Why a decision tool may not decide (DDC-FR-VYLJ, in that order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorityRefusal {
    /// DDC-FR-ZYOL: the turn is bound to an open speech.
    SpeechNotPosted,
    /// DDC-FR-ELAE: the newest received comment is not an author comment, or
    /// the turn received no comment.
    NotAuthorized,
    /// DDC-FR-NKAB: the instruction is not in the authorizing comment.
    InstructionNotFound,
    /// DDC-FR-VONX: the authorizing comment already authorized a decision.
    InstructionUsed,
}

impl AuthorityRefusal {
    /// A stable code for the log record.
    pub fn reason(self) -> &'static str {
        match self {
            AuthorityRefusal::SpeechNotPosted => "speech_not_posted",
            AuthorityRefusal::NotAuthorized => "not_authorized",
            AuthorityRefusal::InstructionNotFound => "instruction_not_found",
            AuthorityRefusal::InstructionUsed => "instruction_used",
        }
    }
}

/// DDC-FR-PZBH: whether an `instruction` is well formed. It is not blank, it
/// holds a letter or a digit, and it is 500 characters or fewer.
pub fn instruction_is_well_formed(instruction: &str) -> bool {
    let trimmed = instruction.trim();
    !trimmed.is_empty()
        && trimmed.chars().any(char::is_alphanumeric)
        && instruction.chars().count() <= MAX_INSTRUCTION_CHARS
}

/// DDC-FR-NKAB: lower case, each run of characters that are not letters or
/// digits changed to one space, and trimmed.
pub fn normalise_instruction_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut gap = false;
    for c in text.chars() {
        if c.is_alphanumeric() {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.extend(c.to_lowercase());
        } else {
            gap = true;
        }
    }
    out
}

/// DDC-FR-NKAB: whether `instruction` stands in `body` as a run of whole
/// words, both normalised.
pub fn instruction_matches(instruction: &str, body: &str) -> bool {
    let needle = normalise_instruction_text(instruction);
    if needle.is_empty() {
        return false;
    }
    let hay = normalise_instruction_text(body);
    format!(" {hay} ").contains(&format!(" {needle} "))
}

impl TurnRegistry {
    /// AGC-FR-TFEN: add one comment to the author comments of the run.
    pub(super) fn note_author(&self, comment_id: &str) {
        self.lock().authority.authors.insert(comment_id.to_string());
    }

    /// AGC-FR-DTCL: the newest comment that a turn received, and whether the
    /// turn is bound to an open speech. `None` for a turn that is not live.
    pub(super) fn received_by(&self, turn_id: &str) -> Option<(Option<String>, bool)> {
        let inner = self.lock();
        let live = inner.live.get(turn_id)?;
        Some((live.received.last().cloned(), live.turn.speech_id.is_some()))
    }

    /// DDC-FR-ELAE: whether a comment is an author comment of the run.
    pub(super) fn is_author_comment(&self, comment_id: &str) -> bool {
        self.lock().authority.authors.contains(comment_id)
    }

    /// AGC-FR-QZVW / DDC-FR-CMRE: reserve a comment for one call. The check
    /// and the reservation are one step under the registry lock, so two turns
    /// never reserve one comment.
    pub(super) fn reserve_authority(&self, comment_id: &str) -> bool {
        self.lock().authority.spent.insert(comment_id.to_string())
    }

    /// AGC-FR-QZVW: give back a comment whose call decided nothing.
    pub(super) fn release_authority(&self, comment_id: &str) {
        self.lock().authority.spent.remove(comment_id);
    }
}

/// AGC-FR-TFEN / CMS-FR-VRZA: the human write commands of
/// `CMS-comments-storage.md` call this for each human comment they append. No
/// other path calls it, so a decision comment, a confirmation comment, and an
/// agent comment are never author comments.
pub fn note_author_comment<R: tauri::Runtime>(app: &tauri::AppHandle<R>, comment_id: &str) {
    use tauri::Manager;
    if let Some(turns) = app.try_state::<TurnRegistry>() {
        turns.note_author(comment_id);
    }
}

/// AGC-FR-QZVW: an authorizing comment, reserved for one call.
///
/// [`commit`](Self::commit) spends it. Dropped without a commit, it gives the
/// comment back, so a call that decided nothing, a refusal, and a panic all
/// leave the comment unspent (DDC-FR-VONX).
pub struct AuthorityGrant<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    comment_id: String,
    spent: bool,
}

impl<R: tauri::Runtime> AuthorityGrant<R> {
    /// The id of the authorizing comment.
    pub fn comment_id(&self) -> &str {
        &self.comment_id
    }

    /// DDC-FR-VONX: the call decided at least one entry, so the comment is
    /// spent for the run.
    pub fn commit(mut self) {
        self.spent = true;
    }
}

impl<R: tauri::Runtime> Drop for AuthorityGrant<R> {
    fn drop(&mut self) {
        use tauri::Manager;
        if self.spent {
            return;
        }
        if let Some(turns) = self.app.try_state::<TurnRegistry>() {
            turns.release_authority(&self.comment_id);
        }
    }
}

/// DDC-FR-ZYOL, DDC-FR-ELAE, DDC-FR-NKAB, DDC-FR-VONX: check the author's
/// authority for one call of a decision tool, in the order of DDC-FR-VYLJ,
/// and reserve the authorizing comment.
///
/// `turn_id` is the turn that calls the tool. The body of the authorizing
/// comment is read from the turn's own discussion: steering comes from the
/// same origin (AGC-FR-ISKZ), so every received comment is in it.
pub fn authorize_decision<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    origin: &ConversationOrigin,
    turn_id: &str,
    instruction: &str,
) -> Result<AuthorityGrant<R>, AuthorityRefusal> {
    use tauri::Manager;
    let turns = app.try_state::<TurnRegistry>().ok_or(AuthorityRefusal::NotAuthorized)?;
    let (newest, speech_open) =
        turns.received_by(turn_id).ok_or(AuthorityRefusal::NotAuthorized)?;
    if speech_open {
        return Err(AuthorityRefusal::SpeechNotPosted);
    }
    let comment_id = newest.ok_or(AuthorityRefusal::NotAuthorized)?;
    if !turns.is_author_comment(&comment_id) {
        return Err(AuthorityRefusal::NotAuthorized);
    }
    let body = resolve_origin(roots, origin)
        .and_then(|discussion| discussion.comments.into_iter().find(|c| c.id == comment_id))
        .filter(|comment| matches!(comment.author, Participant::Human { .. }))
        .map(|comment| comment.body)
        .ok_or(AuthorityRefusal::NotAuthorized)?;
    if !instruction_matches(instruction, &body) {
        return Err(AuthorityRefusal::InstructionNotFound);
    }
    if !turns.reserve_authority(&comment_id) {
        return Err(AuthorityRefusal::InstructionUsed);
    }
    Ok(AuthorityGrant { app: app.clone(), comment_id, spent: false })
}

/// Test seams: a turn registered with a trigger comment, the comments it
/// takes as steering messages, and the end of its speech, without a loop.
#[cfg(test)]
pub(crate) mod authority_for_test {
    use super::*;
    use tauri::Manager;

    fn registry<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> tauri::State<'_, TurnRegistry> {
        if app.try_state::<TurnRegistry>().is_none() {
            app.manage(TurnRegistry::default());
        }
        app.state::<TurnRegistry>()
    }

    /// A live turn in `origin`, triggered by `trigger` and bound to `speech`.
    pub(crate) fn register_turn<R: tauri::Runtime>(
        app: &tauri::AppHandle<R>,
        origin: ConversationOrigin,
        trigger: Option<&str>,
        speech: Option<&str>,
    ) -> String {
        let agent = crate::agents::Agent::default();
        registry(app).register(&agent, origin, trigger, "test-project", speech).0.id
    }

    /// AGC-FR-ISKZ: the turn takes `comment_id` as a steering message.
    pub(crate) fn steer_comment<R: tauri::Runtime>(
        app: &tauri::AppHandle<R>,
        turn_id: &str,
        comment_id: &str,
    ) {
        let turns = registry(app);
        let mut inner = turns.lock();
        let live = inner.live.get_mut(turn_id).expect("live turn");
        give_steering(live, SteeringItem::Comment(comment_id.to_string()));
    }

    /// AGC-FR-KXOJ: the turn takes a chunk message, which is not a comment.
    pub(crate) fn steer_chunk<R: tauri::Runtime>(app: &tauri::AppHandle<R>, turn_id: &str) {
        let turns = registry(app);
        let mut inner = turns.lock();
        let live = inner.live.get_mut(turn_id).expect("live turn");
        give_steering(live, SteeringItem::Chunk("accept it".to_string()));
    }

    /// AGC-FR-TCDQ / AGC-FR-QYNZ: the speech ends with `comment_id`.
    pub(crate) fn end_speech<R: tauri::Runtime>(
        app: &tauri::AppHandle<R>,
        turn_id: &str,
        comment_id: &str,
    ) {
        let turns = registry(app);
        let mut inner = turns.lock();
        let live = inner.live.get_mut(turn_id).expect("live turn");
        live.turn.speech_id = None;
        live.turn.trigger_comment_id = Some(comment_id.to_string());
        give_steering(live, SteeringItem::SpeechEnd(comment_id.to_string()));
    }

    /// An author comment of the run, the trigger of a new turn in `origin`.
    /// The spent comments are cleared, so a test makes several calls on one
    /// comment of its fixture.
    pub(crate) fn authorized_turn<R: tauri::Runtime>(
        app: &tauri::AppHandle<R>,
        origin: ConversationOrigin,
        comment_id: &str,
    ) -> String {
        registry(app).lock().authority.spent.clear();
        note_author_comment(app, comment_id);
        register_turn(app, origin, Some(comment_id), None)
    }

    /// The number of live turns, so a test sees that a call dispatched none.
    pub(crate) fn live_turns<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> usize {
        registry(app).lock().live.len()
    }

    /// Whether `comment_id` is an author comment of the run (AGC-FR-TFEN).
    pub(crate) fn is_author<R: tauri::Runtime>(
        app: &tauri::AppHandle<R>,
        comment_id: &str,
    ) -> bool {
        registry(app).is_author_comment(comment_id)
    }

    /// Whether `comment_id` is spent.
    pub(crate) fn is_spent<R: tauri::Runtime>(app: &tauri::AppHandle<R>, comment_id: &str) -> bool {
        registry(app).lock().authority.spent.contains(comment_id)
    }
}

#[cfg(test)]
mod tests;

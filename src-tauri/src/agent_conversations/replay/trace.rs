//! The `agent call prefix` record (`CVL-conversation-loop.md` CVL-FR-MOTF,
//! CVL-FR-XTMG).
//!
//! The record says, with hashes alone, which part of a call is the same as the
//! last call of the previous turn of the same agent in the same discussion. That
//! is the prefix a provider cache can reuse (CVL-FR-23), so the record is what
//! shows why a turn did or did not get a cache hit. It holds no part of the
//! exchange: every value is a salted, cut hash or a count.

use super::super::*;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// CVL-FR-XTMG: the characters of hexadecimal that each hash keeps.
const HASH_CHARS: usize = 8;

/// CVL-FR-XTMG: the salt of this run of the application, drawn at random one
/// time. A hash of a known text is then not the same from one run to the next.
fn salt() -> &'static [u8; 16] {
    static SALT: OnceLock<[u8; 16]> = OnceLock::new();
    SALT.get_or_init(|| {
        use std::hash::{BuildHasher, Hasher};
        let mut bytes = [0u8; 16];
        for half in bytes.chunks_mut(8) {
            let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
            hasher.write_u128(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or_default(),
            );
            half.copy_from_slice(&hasher.finish().to_le_bytes());
        }
        bytes
    })
}

/// The full salted digest of `bytes`.
fn digest(bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(salt());
    hasher.update(bytes);
    hasher.finalize().into()
}

/// CVL-FR-XTMG: the cut hexadecimal form of a digest.
fn cut(digest: &[u8; 32]) -> String {
    digest.iter().take(HASH_CHARS / 2).map(|b| format!("{b:02x}")).collect()
}

/// CVL-FR-MOTF: the hashes of one call.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::agent_conversations) struct CallPrefix {
    pub tools: String,
    pub system: String,
    pub head: String,
    /// One cumulative hash per message: the hash of message `i` covers every
    /// message up to and including it.
    pub messages: Vec<String>,
}

impl CallPrefix {
    /// CVL-FR-MOTF: the hashes of a call that presents `exchange` for
    /// `request`.
    pub(in crate::agent_conversations) fn of(
        request: &AgentRequest,
        exchange: &[rig::completion::Message],
    ) -> Self {
        let tools =
            serde_json::to_vec(&(&request.tools, &request.native_tools)).unwrap_or_default();
        let head = stable_head_text(request, exchange).unwrap_or_default();
        let mut previous = [0u8; 32];
        let mut messages = Vec::with_capacity(exchange.len());
        for message in exchange {
            let mut bytes = previous.to_vec();
            bytes.extend(serde_json::to_vec(message).unwrap_or_default());
            previous = digest(&bytes);
            messages.push(cut(&previous));
        }
        Self {
            tools: cut(&digest(&tools)),
            system: cut(&digest(request.instructions.as_bytes())),
            head: cut(&digest(head.as_bytes())),
            messages,
        }
    }
}

/// CVL-FR-MOTF: where a call first differs from the last call of the previous
/// turn. `None` where every message of the previous call is a prefix of this
/// one, else the index of the first message that differs. Pure.
pub(in crate::agent_conversations) fn first_difference(
    previous: &CallPrefix,
    now: &CallPrefix,
) -> Option<usize> {
    if previous.tools != now.tools || previous.system != now.system {
        return Some(0);
    }
    previous.messages.iter().zip(&now.messages).position(|(a, b)| a != b).or_else(|| {
        // A previous call longer than this one is not a prefix of it.
        (previous.messages.len() > now.messages.len()).then_some(now.messages.len())
    })
}

/// The calls of one conversation key: the last call of the previous turn, and
/// the last call of the turn that runs now.
#[derive(Default)]
struct KeyCalls {
    previous_turn: Option<CallPrefix>,
    current_turn: Option<(String, CallPrefix)>,
}

/// CVL-FR-MOTF: the last calls of each conversation key, for the life of the
/// process alone. It holds hashes alone.
fn calls() -> &'static Mutex<HashMap<String, KeyCalls>> {
    static CALLS: OnceLock<Mutex<HashMap<String, KeyCalls>>> = OnceLock::new();
    CALLS.get_or_init(Default::default)
}

/// The cap on the number of keys the memory of [`calls`] holds. Above it, the
/// memory starts again empty: a lost entry costs one record its comparison.
const MAX_KEYS: usize = 4_096;

/// CVL-FR-MOTF: hold `prefix` as the call of `turn_id` for `key`, and give the
/// first difference against the last call of the previous turn of that key.
/// The outer `None` says there is no previous turn to compare with.
pub(in crate::agent_conversations) fn observe(
    key: &str,
    turn_id: &str,
    prefix: &CallPrefix,
) -> Option<Option<usize>> {
    let mut calls = calls().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if calls.len() >= MAX_KEYS && !calls.contains_key(key) {
        calls.clear();
    }
    let entry = calls.entry(key.to_string()).or_default();
    let new_turn = entry.current_turn.as_ref().is_none_or(|(id, _)| id != turn_id);
    if new_turn {
        entry.previous_turn = entry.current_turn.take().map(|(_, prefix)| prefix);
    }
    entry.current_turn = Some((turn_id.to_string(), prefix.clone()));
    entry.previous_turn.as_ref().map(|previous| first_difference(previous, prefix))
}

/// CVL-FR-MOTF, CVL-FR-XTMG: emit the record of one call after its first
/// attempt. `reply` is the answer of that attempt, where it answered.
pub(in crate::agent_conversations) fn emit_call_prefix<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    buffer: &'static logging::LogBuffer,
    plan: &TurnPlan,
    endpoint: &AiApiCall,
    round: usize,
    prefix: &CallPrefix,
    reply: Option<&ModelReply>,
) {
    let key = plan.request.cache_key.as_deref().unwrap_or(&plan.turn_id);
    let difference = observe(key, &plan.turn_id, prefix);
    let diverges = match difference {
        None => "no previous turn".to_string(),
        Some(None) => "none".to_string(),
        Some(Some(at)) => at.to_string(),
    };
    let cached = reply.and_then(|r| r.input_tokens).map(|t| t.cached.to_string());
    let writes = reply.and_then(|r| r.cache_write_tokens).map(|t| t.to_string());
    logging::log_debug(
        app,
        buffer,
        &[Domain::Ai, Domain::Backend],
        "agent call prefix",
        log_fields! {
            "turnId" => &plan.turn_id,
            "agent" => &plan.agent.nickname,
            "agentId" => &plan.agent.id,
            "provider" => &endpoint.provider,
            "model" => endpoint.model_id.as_deref().unwrap_or("<provider default>"),
            "round" => round,
            "toolsHash" => &prefix.tools,
            "systemHash" => &prefix.system,
            "headHash" => &prefix.head,
            "messages" => prefix.messages.join(","),
            "divergesFromPreviousTurnAt" => diverges,
            "cachedInputTokens" => cached.unwrap_or_else(|| "not reported".to_string()),
            "cacheWriteTokens" => writes.unwrap_or_else(|| "not reported".to_string()),
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use rig::completion::Message;

    fn request(instructions: &str) -> AgentRequest {
        AgentRequest { instructions: instructions.to_string(), ..AgentRequest::default() }
    }

    // CVL-FR-MOTF, CVL-FR-XTMG: each hash is 8 hexadecimal characters, the
    // message hashes are cumulative, and no hash shows the text it covers.
    #[test]
    fn hashes_are_cut_cumulative_and_hold_no_text() {
        let exchange = vec![Message::user("secret text"), Message::assistant("reply")];
        let prefix = CallPrefix::of(&request("system words"), &exchange);
        for hash in
            [&prefix.tools, &prefix.system, &prefix.head].into_iter().chain(prefix.messages.iter())
        {
            assert_eq!(hash.len(), HASH_CHARS);
            assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "{hash}");
        }
        assert_eq!(prefix.messages.len(), 2);
        let shorter = CallPrefix::of(&request("system words"), &exchange[..1]);
        assert_eq!(shorter.messages[0], prefix.messages[0], "the same first message");
        let other = CallPrefix::of(&request("system words"), &[Message::user("other")]);
        assert_ne!(other.messages[0], prefix.messages[0]);
        let rendered = format!("{prefix:?}");
        assert!(!rendered.contains("secret") && !rendered.contains("system words"));
    }

    // CVL-FR-MOTF: a call that extends the previous one differs nowhere, a
    // changed message names its index, and a changed prompt names index 0.
    #[test]
    fn the_first_difference_names_the_first_changed_message() {
        let a = Message::user("a");
        let b = Message::assistant("b");
        let c = Message::user("c");
        let previous = CallPrefix::of(&request("p"), &[a.clone(), b.clone()]);
        let extended = CallPrefix::of(&request("p"), &[a.clone(), b.clone(), c.clone()]);
        assert_eq!(first_difference(&previous, &extended), None);
        let changed = CallPrefix::of(&request("p"), &[a.clone(), c.clone(), b.clone()]);
        assert_eq!(first_difference(&previous, &changed), Some(1));
        let shorter = CallPrefix::of(&request("p"), std::slice::from_ref(&a));
        assert_eq!(first_difference(&previous, &shorter), Some(1));
        let prompt = CallPrefix::of(&request("q"), &[a, b, c]);
        assert_eq!(first_difference(&previous, &prompt), Some(0));
    }

    // CVL-FR-MOTF: a call compares with the last call of the previous turn of
    // its key, not with an earlier call of its own turn.
    #[test]
    fn a_call_compares_with_the_previous_turn_of_its_key() {
        let key = "trace-test-key";
        let one = CallPrefix::of(&request("p"), &[Message::user("a")]);
        let two = CallPrefix::of(&request("p"), &[Message::user("a"), Message::assistant("b")]);
        let other = CallPrefix::of(&request("p"), &[Message::user("z")]);
        assert_eq!(observe(key, "t1", &one), None, "no previous turn");
        assert_eq!(observe(key, "t1", &two), None, "still the first turn");
        assert_eq!(observe(key, "t2", &other), Some(Some(0)));
        assert_eq!(observe(key, "t2", &other), Some(Some(0)), "the same previous turn");
        let three = CallPrefix::of(
            &request("p"),
            &[Message::user("z"), Message::assistant("y"), Message::user("x")],
        );
        assert_eq!(observe(key, "t3", &three), Some(None));
    }
}

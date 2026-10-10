//! Typed failures, the shared constants, and the retry policy.
//!
//! The failure classes of AGC-FR-15 and the backoff schedule of CVL-FR-17 ..
//! CVL-FR-21 stand together because the schedule is decided by the class.

use super::*;

/// AGC-FR-21. Kebab-case rather than the spec's abstract wording for the reason
/// every other channel in this application is: Tauri validates event names and
/// rejects one carrying a space on both sides, leaving a silently dead channel.
/// Matches `src/events.ts` byte-for-byte.
pub const AGENT_TURN_STATE_CHANGED: &str = "agent-turn-state-changed";

// ---------------------------------------------------------------------------
// Typed failures (AGC-FR-15)
// ---------------------------------------------------------------------------

/// The nickname resolves to no agent enrolled in this project.
pub const FAIL_AGENT_NOT_FOUND: &str = "agent_not_found";
/// It resolves, but its provider or model no longer serves it.
pub const FAIL_AGENT_UNAVAILABLE: &str = "agent_unavailable";
/// The conversation takes no further contribution.
pub const FAIL_THREAD_LOCKED: &str = "discussion_locked";
/// None of the material the builder needs could be read.
pub const FAIL_CONTEXT_UNAVAILABLE: &str = "context_unavailable";
/// The host could not be reached at all.
pub const FAIL_UNREACHABLE: &str = "unreachable";
/// The endpoint answered and refused the credentials.
pub const FAIL_REJECTED: &str = "rejected";
/// It did not answer within the bounded timeout.
pub const FAIL_TIMED_OUT: &str = "timed_out";
/// It answered with no content at all.
pub const FAIL_EMPTY_REPLY: &str = "empty_reply";
/// The OS credential store would not produce the provider's key.
pub const FAIL_KEYCHAIN_UNAVAILABLE: &str = "keychain_unavailable";
/// The provider's certificate is not trusted (AGC-FR-RWPT). The turn also
/// carries the host and the cause.
pub const FAIL_TLS_UNTRUSTED: &str = "tls_untrusted";
/// The provider answered with a reply that could not be read (CVL-FR-21).
pub const FAIL_INVALID_RESPONSE: &str = "invalid_response";
/// The framework refused to build the request, so nothing was sent
/// (CVL-FR-21).
pub const FAIL_INVALID_REQUEST: &str = "invalid_request";
/// Every command here answers only while a project is open.
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
/// `cancel_agent_turn` was given an id no turn carries.
pub const ERR_TURN_NOT_FOUND: &str = "turn_not_found";

/// AGC-FR-11: how much of one section's material a request carries. A builder
/// that gathers more says so in the section rather than trimming silently,
/// because an agent handed half a document without being told is an agent
/// answering about a document that does not exist.
pub(super) const SECTION_MAX_CHARS: usize = 40_000;

/// AGC-FR-25: how many model calls may be in flight at one moment. A property of
/// this module rather than of any provider, so an author who has addressed a
/// dozen agents at once does not present a dozen simultaneous requests to one
/// endpoint.
pub(super) const DEFAULT_CONCURRENCY: usize = 4;

/// CVL-FR-16: how long a turn may run where neither its provider nor the
/// project sets a turn timeout of its own.
pub(super) const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

// ---------------------------------------------------------------------------
// The retry policy (CVL-FR-17 .. CVL-FR-21)
// ---------------------------------------------------------------------------

/// CVL-FR-17: the deadline on **one** `complete` invocation, applied as the
/// lesser of this and the time remaining under the whole-turn deadline.
///
/// The cap is what keeps the two bounds from disagreeing: a call begun with
/// ninety seconds left on the turn is given ninety seconds, so no per-call
/// deadline ever outlives the turn it belongs to and no expiry is ambiguous
/// about which bound it was.
pub(super) const PROVIDER_CALL_TIMEOUT: Duration = Duration::from_secs(300);

/// CVL-FR-19: how many physical attempts one logical invocation may make where
/// the project configures no retry budget of its own — the original call and up
/// to two retries.
pub const MAX_ATTEMPTS: usize = 3;

/// CVL-FR-19: the base delay before the first retry. Each retry after it
/// doubles the one before, so the schedule is defined for whatever budget the
/// project holds (`../core/PSS-project-settings-storage.md` PSS-FR-WPKS) and
/// the ceiling below is what keeps it bounded.
const BACKOFF_FIRST_BASE: Duration = Duration::from_millis(500);

/// CVL-FR-19: the delay actually taken is drawn uniformly within ±20% of the
/// base.
const BACKOFF_JITTER: f64 = 0.20;

/// CVL-FR-19: no delay is ever above this, whatever the schedule says.
pub(super) const BACKOFF_CEILING: Duration = Duration::from_secs(2);

/// CVL-FR-21: the normalized failure classes, distinguishable enough to act on.
///
/// A class is what a record names, what the retry decision is taken on, and what
/// the turn's `AgentTurnFailure` is derived from, so a log line, a retry, and a
/// reported failure can never disagree about what happened.
pub mod class {
    /// A name that would not resolve.
    pub const DNS: &str = "dns";
    /// A connection that would not open.
    pub const CONNECT: &str = "connect";
    /// A connection reset in flight.
    pub const RESET: &str = "reset";
    /// A response the provider actually returned, carrying its status alone.
    pub const HTTP_STATUS: &str = "http_status";
    /// The per-call deadline of CVL-FR-17.
    pub const PROVIDER_TIMEOUT: &str = "provider_timeout";
    /// A reply that carried neither usable prose nor a tool request.
    pub const EMPTY_REPLY: &str = "empty_reply";
    /// A transport error matching none of the above. Normalized all the same: a
    /// safe code and a redacted diagnostic category, never the underlying
    /// error's own payload, because a driver's error string is the one place a
    /// URL, a header, or a token reaches a log by accident.
    pub const TRANSPORT_OTHER: &str = "transport_other";
    /// The endpoint answered and refused the credentials.
    pub const AUTH: &str = "auth";
    /// The TLS check refused the endpoint's certificate (CVL-FR-21).
    pub const TLS: &str = "tls";
    /// A reply the provider returned that could not be read (CVL-FR-21).
    pub const DECODE: &str = "decode";
    /// A request the framework refused to build before it sent anything
    /// (CVL-FR-21).
    pub const REQUEST: &str = "request";
}

/// CVL-FR-21: one failed attempt, reduced to what is safe to report.
///
/// Carries no part of the provider's own message. The `status`, the
/// `provider_code`, and the `provider_request_id` are all things the provider
/// chose and none of them is anything it echoed back of the request that
/// produced it (CVL-FR-35).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallFailure {
    /// One of AGC-FR-15's own values.
    pub failure: &'static str,
    /// One of [`class`].
    pub class: &'static str,
    /// Present only on [`class::HTTP_STATUS`].
    pub status: Option<u16>,
    /// CVL-FR-35: the error code the provider itself chose, where it answered
    /// with a structured error rather than with a transport failure. A number of
    /// the provider's own vocabulary — nothing of the request is in it.
    pub provider_code: Option<i64>,
    /// CVL-FR-35: the provider's own identifier for the request that failed.
    /// Opaque, and the one field that lets a failure here be matched to the
    /// provider's own record of it.
    pub provider_request_id: Option<String>,
    /// CVL-FR-35: the message the provider itself returned with a **structured**
    /// error, which is the only thing that says *why* a request was refused.
    ///
    /// Carried only from a structured error and never from a transport one: the
    /// distinction is the whole safety of it. A transport error's string is the
    /// driver's, and a driver routinely renders the request it made — URL,
    /// headers, and bearer token included — which is what CVL-FR-21 keeps out of
    /// a log. A structured error's message is the provider's own account of the
    /// request it refused, it reaches this application through a parsed `error`
    /// object rather than through a rendered request, and it is what makes a
    /// refusal readable rather than merely counted.
    pub provider_message: Option<String>,
    /// CVL-FR-21 / AAP-FR-HZTB: the host and the cause, present only on
    /// [`class::TLS`]. Nothing of the request is in it.
    pub tls: Option<crate::tls::TlsFailure>,
    /// CVL-FR-TQRD: what the Custom gateway repair changed in the reply before
    /// the framework refused it. The default on every failure where the repair
    /// changed nothing, or where no reply came back.
    pub reply_repairs: super::responses_repair::ReplyRepairs,
}

impl CallFailure {
    pub const fn new(failure: &'static str, class: &'static str) -> Self {
        Self {
            failure,
            class,
            status: None,
            provider_code: None,
            provider_request_id: None,
            provider_message: None,
            tls: None,
            reply_repairs: super::responses_repair::ReplyRepairs {
                text_format: false,
                null_text_parts: 0,
            },
        }
    }

    /// AGC-FR-RWPT: a certificate the TLS check refused.
    pub fn tls_untrusted(failure: crate::tls::TlsFailure) -> Self {
        Self {
            tls: Some(failure),
            ..Self::new(FAIL_TLS_UNTRUSTED, class::TLS)
        }
    }

    /// A host that could not be reached, of the given class.
    pub const fn unreachable(class: &'static str) -> Self {
        Self::new(FAIL_UNREACHABLE, class)
    }

    /// CVL-FR-18: whether repeating the same call, against the same exchange,
    /// could plausibly succeed.
    ///
    /// Six failures are recoverable: `unreachable` — where a name that will
    /// not resolve, a connection that will not open, and a connection reset
    /// mid-answer all surface — the provider-call `timed_out` of CVL-FR-17,
    /// `empty_reply`, `tls_untrusted`, `invalid_response`, and
    /// `invalid_request`. Only the first
    /// three are repeated by the loop itself ([`Self::repeatable`]). Everything
    /// else names a condition of the application rather than of the call, and
    /// calling again changes none of them.
    pub fn recoverable(&self) -> bool {
        is_recoverable(self.failure)
    }

    /// CVL-FR-18: whether the loop repeats the call by itself. A refused
    /// certificate is recoverable but is not repeated: the same certificate is
    /// refused again until the author changes the trust on the machine. A reply
    /// that could not be read is not repeated either: the endpoint returns the
    /// same shape again. A request that could not be built is not repeated: the
    /// same exchange builds the same request again.
    pub fn repeatable(&self) -> bool {
        is_repeated_automatically(self.failure)
    }
}

impl From<&'static str> for CallFailure {
    /// A failure whose class is implied by the value alone — the shape every
    /// path that already knew its own outcome produces.
    fn from(failure: &'static str) -> Self {
        let class = match failure {
            FAIL_TIMED_OUT => class::PROVIDER_TIMEOUT,
            FAIL_REJECTED => class::AUTH,
            FAIL_EMPTY_REPLY => class::EMPTY_REPLY,
            FAIL_INVALID_RESPONSE => class::DECODE,
            FAIL_INVALID_REQUEST => class::REQUEST,
            _ => class::TRANSPORT_OTHER,
        };
        Self::new(failure, class)
    }
}

/// CVL-FR-18: the recoverable values of the failure vocabulary.
///
/// `timed_out` is recoverable **here** because every value this function is
/// asked about came out of one `complete` invocation, where the only deadline
/// that can have expired is the per-call one of CVL-FR-17. A turn that ran out
/// of time under CVL-FR-16 never reaches this: it is failed by the loop itself,
/// which is why the whole-turn `timed_out` stays non-recoverable while sharing a
/// spelling with the retryable one.
pub fn is_recoverable(failure: &str) -> bool {
    matches!(
        failure,
        FAIL_UNREACHABLE
            | FAIL_TIMED_OUT
            | FAIL_EMPTY_REPLY
            | FAIL_TLS_UNTRUSTED
            | FAIL_INVALID_RESPONSE
            | FAIL_INVALID_REQUEST
    )
}

/// CVL-FR-18: the three recoverable values the loop repeats by itself.
pub fn is_repeated_automatically(failure: &str) -> bool {
    matches!(failure, FAIL_UNREACHABLE | FAIL_TIMED_OUT | FAIL_EMPTY_REPLY)
}

/// CVL-FR-19: the jitter source, a seam like the completion seam beside it.
///
/// Substituted wholesale under test so a schedule is asserted to the millisecond
/// rather than tolerated to a margin, which is also what lets a suite exercising
/// exhaustion run in the time three scripted failures take rather than three
/// real backoffs.
pub trait JitterSource: Send + Sync {
    /// A fraction in `[0, 1]`, mapped onto ±[`BACKOFF_JITTER`] of the base.
    fn fraction(&self) -> f64;
}

/// The production jitter: a xorshift seeded from the clock.
///
/// Deliberately not a cryptographic source and deliberately not a new
/// dependency — what is wanted is that two turns failing at the same instant do
/// not retry at the same instant, which any decorrelation achieves.
pub struct SystemJitter;

impl JitterSource for SystemJitter {
    fn fraction(&self) -> f64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64 ^ (d.as_secs() << 17))
            .unwrap_or(0x9E37_79B9_7F4A_7C15);
        let mut x = seed | 1;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// CVL-FR-19 / CVL-FR-22: the retry schedule, as one value.
///
/// A collaborator of the registry on exactly the terms the concurrency bound of
/// AGC-FR-25 and the turn timeout of CVL-FR-16 already are: the application
/// constructs it from the constants above and never from anything else, and a
/// test substitutes one so a suite exercising exhaustion runs in the time three
/// scripted failures take rather than three real backoffs. CVL-FR-22 binds the
/// surfaces an author can reach — no command accepts one of its waits, no
/// provider descriptor carries one, and no agent's definition names one. The
/// attempt count and the per-call deadline are read over it from the project's
/// settings before each turn (CVL-FR-PDXK).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RetryPolicy {
    pub max_attempts: usize,
    pub first_base: Duration,
    pub jitter: f64,
    pub ceiling: Duration,
    pub call_timeout: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: MAX_ATTEMPTS,
            first_base: BACKOFF_FIRST_BASE,
            jitter: BACKOFF_JITTER,
            ceiling: BACKOFF_CEILING,
            call_timeout: PROVIDER_CALL_TIMEOUT,
        }
    }
}

/// CVL-FR-19: the wait before `attempt`, jittered and capped.
///
/// `attempt` is the 1-based number of the attempt **about to be made**, so the
/// first retry is attempt 2 and reads `bases[0]`.
pub(super) fn backoff_delay(policy: &RetryPolicy, attempt: usize, jitter: &dyn JitterSource) -> Duration {
    // CVL-FR-19: the first retry takes the base delay and each retry after it
    // doubles the one before. The ceiling below is what bounds the schedule,
    // whatever budget the project holds.
    let Some(step) = attempt.checked_sub(2) else {
        return Duration::ZERO;
    };
    if attempt > policy.max_attempts {
        return Duration::ZERO;
    }
    let base = &policy
        .first_base
        .saturating_mul(1u32.checked_shl(step.min(20) as u32).unwrap_or(u32::MAX));
    // A fraction of 0 is the base less its jitter fraction, 1 is the base plus
    // it, and 0.5 is the base exactly.
    let factor = 1.0 - policy.jitter + (jitter.fraction().clamp(0.0, 1.0) * 2.0 * policy.jitter);
    base.mul_f64(factor).min(policy.ceiling)
}

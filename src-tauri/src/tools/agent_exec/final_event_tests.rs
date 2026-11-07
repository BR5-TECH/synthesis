//! Tests of the final-event watch (EAC-FR-HZIE, EAC-FR-QHAY) and of the final
//! events the descriptors declare (CCP-FR-RGVA).
//!
//! Every test that reads the grace runs on paused tokio time, so no result
//! depends on how fast the machine is.

use std::sync::Mutex;
use std::time::Duration;

use tokio::time::advance;

use super::*;
use crate::tools::agent_exec::descriptor::{claude_code, CLAUDE_CODE, CODEX};

const GRACE: Duration = Duration::from_secs(30);
const MS: Duration = Duration::from_millis(1);

/// A final event that is the line `R` alone, so a test controls exactly which
/// line is one.
fn is_r(line: &[u8]) -> bool {
    line == b"R"
}

fn rule() -> FinalEventRule {
    FinalEventRule { is_final: is_r, grace: GRACE }
}

fn stdout(watch: &FinalEventWatch<'_>, line: &[u8]) {
    watch.line(StreamChannel::Stdout, line, true);
}

/// An observer that keeps every call it is handed.
#[derive(Default)]
struct Recording(Mutex<Vec<(StreamChannel, Vec<u8>, bool)>>);

impl StreamObserver for Recording {
    fn line(&self, channel: StreamChannel, bytes: &[u8], whole: bool) {
        self.0.lock().unwrap().push((channel, bytes.to_vec(), whole));
    }
}

// EAC-FR-HZIE, EAC-FR-QHAY: a vendor that declares a final event gets the
// grace of 30 seconds, and a vendor that declares none gets no rule.
#[test]
fn a_rule_exists_only_for_a_declared_final_event_and_holds_the_grace() {
    assert!(FinalEventRule::for_vendor(None).is_none());
    let rule = FinalEventRule::for_vendor(Some(is_r)).expect("a rule");
    assert_eq!(rule.grace, FINAL_EVENT_GRACE);
    assert_eq!(FINAL_EVENT_GRACE, Duration::from_secs(30));
}

// EAC-FR-QHAY: with no final event on stdout, the grace never ends, however
// long the run takes.
#[tokio::test(start_paused = true)]
async fn no_final_event_starts_no_grace() {
    let watch = FinalEventWatch::new(rule(), None);
    assert!(!watch.grace_over());
    stdout(&watch, b"progress");
    stdout(&watch, b"more progress");
    advance(Duration::from_secs(3600)).await;
    assert!(!watch.grace_over());
}

// EAC-FR-HZIE: the grace ends exactly its length after the final event, and
// stays over.
#[tokio::test(start_paused = true)]
async fn the_grace_ends_its_length_after_the_final_event() {
    let watch = FinalEventWatch::new(rule(), None);
    stdout(&watch, b"R");
    assert!(!watch.grace_over());
    advance(GRACE - MS).await;
    assert!(!watch.grace_over());
    advance(MS).await;
    assert!(watch.grace_over());
    advance(Duration::from_secs(600)).await;
    assert!(watch.grace_over());
}

// EAC-FR-HZIE: a later stdout line cancels the grace, a line of spaces too.
#[tokio::test(start_paused = true)]
async fn a_later_stdout_line_cancels_the_grace() {
    for later in [&b"progress"[..], &b" "[..]] {
        let watch = FinalEventWatch::new(rule(), None);
        stdout(&watch, b"R");
        advance(GRACE / 2).await;
        stdout(&watch, later);
        advance(GRACE * 4).await;
        assert!(!watch.grace_over(), "{later:?} did not cancel the grace");
    }
}

// EAC-FR-HZIE: a second final event starts the grace again from itself.
#[tokio::test(start_paused = true)]
async fn a_second_final_event_starts_the_grace_again() {
    let watch = FinalEventWatch::new(rule(), None);
    stdout(&watch, b"R");
    advance(Duration::from_secs(20)).await;
    stdout(&watch, b"R");
    advance(Duration::from_secs(20)).await;
    assert!(!watch.grace_over());
    advance(Duration::from_secs(10)).await;
    assert!(watch.grace_over());
}

// EAC-FR-HZIE: only stdout counts. A final event on stderr starts nothing, and
// a stderr line does not cancel a grace.
#[tokio::test(start_paused = true)]
async fn stderr_neither_starts_nor_cancels_the_grace() {
    let watch = FinalEventWatch::new(rule(), None);
    watch.line(StreamChannel::Stderr, b"R", true);
    advance(GRACE * 2).await;
    assert!(!watch.grace_over());

    stdout(&watch, b"R");
    watch.line(StreamChannel::Stderr, b"a warning", true);
    advance(GRACE).await;
    assert!(watch.grace_over());
}

// EAC-FR-DKXD, EAC-FR-32: the empty delivery at the end of the stream does not
// cancel the grace. A blank vendor line has the same form, so it does not
// cancel it either.
#[tokio::test(start_paused = true)]
async fn the_end_of_stream_delivery_does_not_cancel_the_grace() {
    let watch = FinalEventWatch::new(rule(), None);
    stdout(&watch, b"R");
    stdout(&watch, b"");
    advance(GRACE).await;
    assert!(watch.grace_over());
}

// EAC-FR-HZIE: a final event longer than the observer's line bound arrives in
// pieces. It is read when its last piece arrives, and its first piece cancels
// an earlier grace.
#[tokio::test(start_paused = true)]
async fn a_final_event_in_pieces_is_read_when_it_is_whole() {
    let result = format!(
        r#"{{"type":"result","result":"{}"}}"#,
        "x".repeat(crate::tools::agent_exec::runtime::LIMIT_OBSERVED_LINE)
    );
    let (head, tail) =
        result.as_bytes().split_at(crate::tools::agent_exec::runtime::LIMIT_OBSERVED_LINE);
    let rule = FinalEventRule { is_final: claude_code::is_final_event, grace: GRACE };
    let watch = FinalEventWatch::new(rule, None);

    stdout(&watch, br#"{"type":"result"}"#);
    watch.line(StreamChannel::Stdout, head, false);
    advance(GRACE * 2).await;
    assert!(!watch.grace_over(), "the first piece did not cancel the earlier grace");

    stdout(&watch, tail);
    advance(GRACE - MS).await;
    assert!(!watch.grace_over());
    advance(MS).await;
    assert!(watch.grace_over());
}

// EAC-FR-HZIE: a line that ends exactly on the piece bound is delivered as a
// piece and then an empty whole delivery. The joined line is read.
#[tokio::test(start_paused = true)]
async fn a_final_event_that_ends_on_the_piece_bound_is_read() {
    let watch = FinalEventWatch::new(rule(), None);
    watch.line(StreamChannel::Stdout, b"R", false);
    stdout(&watch, b"");
    advance(GRACE).await;
    assert!(watch.grace_over());
}

// EAC-FR-DKXD: a line longer than 4 MiB is not read as a final event and
// cancels a running grace. The next short final event is read again, and a
// line of exactly 4 MiB is read.
#[tokio::test(start_paused = true)]
async fn a_line_over_the_bound_is_not_read_and_the_next_one_is() {
    assert_eq!(LIMIT_FINAL_LINE, 4 * 1024 * 1024);
    let is_long_r = |line: &[u8]| line.first() == Some(&b'R');
    let rule = FinalEventRule { is_final: is_long_r, grace: GRACE };

    let watch = FinalEventWatch::new(rule, None);
    stdout(&watch, b"R");
    let mut first = vec![b'x'; LIMIT_FINAL_LINE];
    first[0] = b'R';
    watch.line(StreamChannel::Stdout, &first, false);
    stdout(&watch, b"x");
    advance(GRACE * 2).await;
    assert!(!watch.grace_over(), "a line over the bound was read or did not cancel");

    stdout(&watch, b"R");
    advance(GRACE).await;
    assert!(watch.grace_over(), "the line after an overflow was not read");

    let watch = FinalEventWatch::new(rule, None);
    let (head, tail) = first.split_at(LIMIT_FINAL_LINE - 1);
    watch.line(StreamChannel::Stdout, head, false);
    stdout(&watch, tail);
    advance(GRACE).await;
    assert!(watch.grace_over(), "a line of exactly the bound was not read");
}

// EAC-FR-HZIE: the pieces of one line do not stay for the next line.
#[tokio::test(start_paused = true)]
async fn the_pieces_of_one_line_do_not_reach_the_next() {
    let watch = FinalEventWatch::new(rule(), None);
    watch.line(StreamChannel::Stdout, b"long", false);
    stdout(&watch, b"line");
    stdout(&watch, b"R");
    advance(GRACE).await;
    assert!(watch.grace_over());
}

// EAC-FR-32, EAC-FR-HZIE: the watch hands every call on unchanged and in
// order, and with no observer behind it the grace still works.
#[tokio::test(start_paused = true)]
async fn every_line_reaches_the_observer_behind_the_watch() {
    let inner = Recording::default();
    let watch = FinalEventWatch::new(rule(), Some(&inner));
    let calls: Vec<(StreamChannel, Vec<u8>, bool)> = vec![
        (StreamChannel::Stdout, b"progress".to_vec(), true),
        (StreamChannel::Stderr, b"warning".to_vec(), true),
        (StreamChannel::Stdout, b"pie".to_vec(), false),
        (StreamChannel::Stdout, b"ce".to_vec(), true),
        (StreamChannel::Stdout, b"R".to_vec(), true),
        (StreamChannel::Stdout, Vec::new(), true),
    ];
    for (channel, bytes, whole) in &calls {
        watch.line(*channel, bytes, *whole);
    }
    assert_eq!(*inner.0.lock().unwrap(), calls);

    let alone = FinalEventWatch::new(rule(), None);
    stdout(&alone, b"R");
    advance(GRACE).await;
    assert!(alone.grace_over());
}

// EAC-FR-HZIE: a runtime hands its streams the watch where there is one, and
// otherwise the caller's own observer.
#[tokio::test(start_paused = true)]
async fn the_streams_go_through_the_watch_where_there_is_one() {
    let inner = Recording::default();
    let none: Option<FinalEventWatch<'_>> = None;
    observer_of(&none, Some(&inner)).expect("the inner observer").line(
        StreamChannel::Stdout,
        b"direct",
        true,
    );
    assert_eq!(inner.0.lock().unwrap().len(), 1);
    assert!(observer_of(&none, None).is_none());

    let watch = Some(FinalEventWatch::new(rule(), Some(&inner)));
    observer_of(&watch, Some(&inner)).expect("the watch").line(StreamChannel::Stdout, b"R", true);
    assert_eq!(inner.0.lock().unwrap().len(), 2, "the watch did not pass the line on");
    advance(GRACE).await;
    assert!(watch.as_ref().is_some_and(FinalEventWatch::grace_over));
}

// CCP-FR-RGVA: the final event of Claude Code is a stdout line whose `type` is
// `result`, and nothing else.
#[test]
fn claude_code_reads_a_result_line_as_its_final_event() {
    let finals: [&[u8]; 4] = [
        br#"{"type":"result","subtype":"success","is_error":false,"result":"{}"}"#,
        br#"{"type":"result"}"#,
        b"  {\"type\":\"result\"}\r",
        br#"{"type":"result","subtype":"error_during_execution","is_error":true}"#,
    ];
    for line in finals {
        assert!(claude_code::is_final_event(line), "{}", String::from_utf8_lossy(line));
    }
    let others: [&[u8]; 11] = [
        br#"{"type":"assistant"}"#,
        br#"{"type":"system","subtype":"init"}"#,
        br#"{"type":"user","message":{"type":"result"}}"#,
        br#"{"type":"Result"}"#,
        br#"{"type":1}"#,
        br#"[{"type":"result"}]"#,
        br#""result""#,
        br#"{"type":"result""#,
        b"not json",
        b"",
        &[0xff, 0xfe, b'{'],
    ];
    for line in others {
        assert!(!claude_code::is_final_event(line), "{}", String::from_utf8_lossy(line));
    }
}

// EAC-FR-HZIE, CCP-FR-RGVA, EAC-FR-QHAY: Claude Code declares its result line
// as its final event, and Codex declares none.
#[test]
fn the_descriptors_declare_their_final_events() {
    let is_final = CLAUDE_CODE.final_event.expect("Claude Code declares a final event");
    assert!(is_final(br#"{"type":"result"}"#));
    assert!(CODEX.final_event.is_none());
    assert!(format!("{CLAUDE_CODE:?}").contains("final_event: true"));
    assert!(format!("{CODEX:?}").contains("final_event: false"));
}

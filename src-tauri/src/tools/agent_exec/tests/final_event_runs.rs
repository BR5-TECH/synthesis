//! EAC-FR-HZIE, EAC-FR-QHAY — the end of a run after its final event, on the
//! Docker CLI backend with a real child process.
//!
//! A child that should be stopped sleeps 60 seconds or more against a grace of
//! 100 ms, and a child that should exit inside the grace exits at once against
//! a grace of 60 seconds. A test that expects the deadline gives it 3 seconds
//! and asserts that the child's line was read before it.

use super::super::final_event::FinalEventRule;
use super::*;
use crate::tools::agent_exec::descriptor::claude_code;

const RESULT: &str = r#"printf '{"type":"result"}\n'"#;
const SHORT: Duration = Duration::from_millis(100);
const LONG: Duration = Duration::from_secs(60);

fn claude_rule(grace: Duration) -> Option<FinalEventRule> {
    Some(FinalEventRule { is_final: claude_code::is_final_event, grace })
}

fn run_sh(
    script: &str,
    timeout: Duration,
    final_event: Option<FinalEventRule>,
    observer: Option<&dyn StreamObserver>,
    cancel: CancellationToken,
) -> RuntimeOutcome {
    let sh = HostDockerCli::with_program("sh");
    block_on(sh.run(RunRequest {
        argv: &["-c".to_string(), script.to_string()],
        container: None,
        env: &BTreeMap::new(),
        stdin: b"",
        timeout,
        cancel,
        stdout_limit: 2 * 1024 * 1024,
        stderr_limit: 4096,
        observer,
        final_event,
    }))
    .expect("the child ran")
}

/// An observer that keeps every line, and cancels a token when it sees the
/// final event.
#[derive(Default)]
struct Seen {
    lines: StdMutex<Vec<(StreamChannel, Vec<u8>, bool)>>,
    cancel_on_result: Option<CancellationToken>,
}

impl StreamObserver for Seen {
    fn line(&self, channel: StreamChannel, bytes: &[u8], whole: bool) {
        self.lines.lock().unwrap().push((channel, bytes.to_vec(), whole));
        if claude_code::is_final_event(bytes) {
            if let Some(token) = &self.cancel_on_result {
                token.cancel();
            }
        }
    }
}

// EAC-FR-HZIE: a child that still runs when the grace ends is stopped, the run
// ends after its final event with no exit code, and the stdout it wrote is kept.
#[test]
fn a_child_alive_after_its_final_event_is_stopped_after_the_grace() {
    let outcome = run_sh(
        &format!("{RESULT}; exec sleep 60"),
        Duration::from_secs(120),
        claude_rule(SHORT),
        None,
        CancellationToken::new(),
    );
    assert_eq!(outcome.end, RunEnd::EndedAfterFinalEvent);
    assert_eq!(outcome.exit_code, None);
    assert_eq!(outcome.stdout.bytes, b"{\"type\":\"result\"}\n");
}

// EAC-FR-QHAY: a child that exits inside the grace keeps its own exit code.
#[test]
fn a_child_that_exits_inside_the_grace_keeps_its_exit_code() {
    for (code, script) in [(0, format!("{RESULT}; exit 0")), (3, format!("{RESULT}; exit 3"))] {
        let outcome = run_sh(
            &script,
            Duration::from_secs(120),
            claude_rule(LONG),
            None,
            CancellationToken::new(),
        );
        assert_eq!(outcome.end, RunEnd::Exited, "exit {code}");
        assert_eq!(outcome.exit_code, Some(code));
    }
}

// EAC-FR-HZIE, EAC-FR-24: the stop after the grace reaches the whole process
// tree, also a grandchild that holds stdout open.
#[test]
fn the_stop_after_the_grace_reaches_the_grandchildren() {
    let script = format!("{RESULT}; sh -c 'sleep 600' & wait");
    let started = std::time::Instant::now();
    let outcome = run_sh(
        &script,
        Duration::from_secs(120),
        claude_rule(SHORT),
        None,
        CancellationToken::new(),
    );
    assert_eq!(outcome.end, RunEnd::EndedAfterFinalEvent);
    // The call returns only when stdout closes, and the grandchild holds it
    // for 600 seconds unless it was killed.
    assert!(started.elapsed() < Duration::from_secs(300), "the grandchild held the run open");
}

// EAC-FR-QHAY, EAC-FR-24: with no final-event rule, only the deadline ends a
// child that wrote a result line and stays alive.
#[test]
fn without_a_rule_only_the_deadline_ends_the_run() {
    let outcome = run_sh(
        &format!("{RESULT}; exec sleep 60"),
        Duration::from_secs(3),
        None,
        None,
        CancellationToken::new(),
    );
    assert_eq!(outcome.end, RunEnd::TimedOut);
    assert_eq!(outcome.stdout.bytes, b"{\"type\":\"result\"}\n", "the line was not read");
}

// EAC-FR-24, EAC-FR-HZIE: a deadline that comes inside the grace ends the run
// as a timeout. The deadline bounds the run.
#[test]
fn a_deadline_inside_the_grace_is_a_timeout() {
    let outcome = run_sh(
        &format!("{RESULT}; exec sleep 60"),
        Duration::from_secs(3),
        claude_rule(LONG),
        None,
        CancellationToken::new(),
    );
    assert_eq!(outcome.end, RunEnd::TimedOut);
    assert_eq!(outcome.stdout.bytes, b"{\"type\":\"result\"}\n", "the line was not read");
}

// EAC-FR-HZIE: a final event on stderr starts no grace.
#[test]
fn a_final_event_on_stderr_starts_no_grace() {
    let outcome = run_sh(
        r#"printf '{"type":"result"}\n' >&2; exec sleep 60"#,
        Duration::from_secs(3),
        claude_rule(SHORT),
        None,
        CancellationToken::new(),
    );
    assert_eq!(outcome.end, RunEnd::TimedOut);
    assert_eq!(outcome.stderr.bytes, b"{\"type\":\"result\"}\n", "the line was not read");
}

// EAC-FR-HZIE, EAC-FR-32: a result line longer than the observer's line bound
// reaches the watch in pieces through a real pipe and still starts the grace.
#[test]
fn a_result_line_longer_than_the_line_bound_starts_the_grace() {
    let script = r#"printf '{"type":"result","result":"'; head -c 307200 /dev/zero | tr '\0' x; printf '"}\n'; exec sleep 60"#;
    let outcome = run_sh(
        script,
        Duration::from_secs(120),
        claude_rule(SHORT),
        None,
        CancellationToken::new(),
    );
    assert_eq!(outcome.end, RunEnd::EndedAfterFinalEvent);
    assert!(outcome.stdout.bytes.len() > 300 * 1024);
}

// EAC-FR-HZIE: a final event with no line ending is read when the child closes
// stdout, and the grace then starts.
#[test]
fn an_unterminated_result_line_starts_the_grace_when_stdout_closes() {
    let outcome = run_sh(
        r#"printf '{"type":"result"}'; exec 1>&-; exec sleep 60"#,
        Duration::from_secs(120),
        claude_rule(SHORT),
        None,
        CancellationToken::new(),
    );
    assert_eq!(outcome.end, RunEnd::EndedAfterFinalEvent);
}

// EAC-FR-32, EAC-FR-HZIE: the caller's observer behind the watch receives the
// result line and the end of the stream.
#[test]
fn the_observer_behind_the_watch_receives_every_line() {
    let seen = Seen::default();
    let outcome = run_sh(
        &format!("printf 'working\\n'; {RESULT}; exec sleep 60"),
        Duration::from_secs(120),
        claude_rule(SHORT),
        Some(&seen),
        CancellationToken::new(),
    );
    assert_eq!(outcome.end, RunEnd::EndedAfterFinalEvent);
    let lines = seen.lines.lock().unwrap().clone();
    let stdout: Vec<&[u8]> = lines
        .iter()
        .filter(|(channel, _, _)| *channel == StreamChannel::Stdout)
        .map(|(_, bytes, _)| bytes.as_slice())
        .collect();
    assert_eq!(stdout, vec![&b"working"[..], &b"{\"type\":\"result\"}"[..], &b""[..]]);
}

// EAC-FR-25, EAC-FR-HZIE: a cancellation inside the grace ends the run as
// cancelled.
#[test]
fn a_cancellation_inside_the_grace_is_a_cancellation() {
    let cancel = CancellationToken::new();
    let seen = Seen { cancel_on_result: Some(cancel.clone()), ..Seen::default() };
    let outcome = run_sh(
        &format!("{RESULT}; exec sleep 60"),
        Duration::from_secs(120),
        claude_rule(LONG),
        Some(&seen),
        cancel,
    );
    assert_eq!(outcome.end, RunEnd::Cancelled);
}

//! Coalescing of the change event (LGC-FR-17).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// LGC-FR-17 — coalescing
// -----------------------------------------------------------------------

#[test]
fn a_burst_collapses_to_one_event_and_the_trailing_flush_is_owed() {
    // LGC-FR-17: the leading edge emits, the rest coalesce, and the trailing
    // flush carries the latest state. Without that flush a burst that ends
    // inside the window leaves the panel permanently stale.
    let buffer = LogBuffer::new();
    let sink = Recorder::default();
    let base = t0();

    let first = buffer.append_at(
        vec![input(LogLevel::Info, &[Domain::Backend], "1")],
        base,
    );
    assert!(first.emit_now, "the leading edge is delivered");
    sink.publish(&first.state);

    for n in 2..=1_000u64 {
        let outcome = buffer.append_at(
            vec![input(LogLevel::Info, &[Domain::Backend], &format!("{n}"))],
            after(base, 1),
        );
        assert!(!outcome.emit_now, "inside the window: coalesced");
    }
    assert_eq!(sink.len(), 1, "1000 appends, 1 event so far");

    // Nothing is owed until the window elapses...
    assert!(buffer.take_pending_flush(after(base, 1)).is_none());
    // ...and then exactly one flush is, carrying the latest state.
    let flushed = buffer.take_pending_flush(base + COALESCE_WINDOW * 2).unwrap();
    sink.publish(&flushed);
    assert_eq!(sink.len(), 2);
    assert_eq!(sink.last().buffer_total, 1_000);
    assert_eq!(sink.last().highest_sequence, Some(999));

    // The debt is owed once.
    assert!(buffer
        .take_pending_flush(base + COALESCE_WINDOW * 4)
        .is_none());
}

#[test]
fn a_debt_incurred_after_a_leading_edge_emit_is_never_stranded() {
    // The interleaving that strands a trailing flush, driven on the clock
    // directly so no thread is involved:
    //
    //   t=0   append A  -> leading edge, last_emit = 0
    //   t=10  append B  -> coalesced; a flush thread is scheduled for ~60
    //   t=55  append C  -> window elapsed, leading edge, last_emit = 55
    //   t=56  append D  -> coalesced; loses the scheduling race, no timer
    //   t=60  the thread wakes: 60 - 55 = 5 < 50, so nothing is owed *yet*
    //
    // A thread that returned at t=60 would leave D's state unpublished for
    // as long as emission stayed quiet — which is exactly when the last
    // records matter most. The debt must still be visible so the scheduling
    // loop keeps sleeping on it.
    let buffer = LogBuffer::new();
    let base = t0();
    let at = |ms: u64| input(LogLevel::Info, &[Domain::Backend], &format!("t{ms}"));

    assert!(buffer.append_at(vec![at(0)], base).emit_now);
    assert!(!buffer.append_at(vec![at(10)], after(base, 10)).emit_now);
    assert!(
        buffer.append_at(vec![at(55)], after(base, 55)).emit_now,
        "past the window: delivered on the leading edge, resetting it"
    );
    assert!(!buffer.append_at(vec![at(56)], after(base, 56)).emit_now);

    // The woken thread finds the window has not elapsed...
    assert!(buffer.take_pending_flush(after(base, 60)).is_none());
    // ...but the debt is still owed, which is what keeps the loop alive.
    assert!(
        buffer.is_flush_pending(),
        "D's state is still owed; a thread that exited here would strand it"
    );
    // And one window later it is settled, carrying every record.
    let flushed = buffer
        .take_pending_flush(after(base, 110))
        .expect("the debt must eventually be payable");
    assert_eq!(flushed.buffer_total, 4);
    assert!(!buffer.is_flush_pending());
}

#[test]
fn a_clear_restarts_the_coalescing_window() {
    // The teardown clears and then immediately appends the record naming
    // what the fresh buffer is fresh for. Left at its pre-clear value,
    // `last_emit` would swallow that record's event and the Logs panel
    // would sit blank after a worktree switch (LGC-FR-15).
    let buffer = LogBuffer::new();
    let base = t0();
    buffer.append_at(vec![input(LogLevel::Info, &[Domain::Backend], "before")], base);

    buffer.clear();
    let outcome = buffer.append_at(
        vec![input(LogLevel::Info, &[Domain::Backend], "content root changed")],
        after(base, 1),
    );
    assert!(
        outcome.emit_now,
        "the first record after a clear is delivered, not coalesced"
    );
    assert_eq!(outcome.state.generation, 1);
    assert_eq!(outcome.state.buffer_total, 1);
}

#[test]
fn an_append_past_the_window_is_delivered_immediately() {
    let buffer = LogBuffer::new();
    let base = t0();
    assert!(buffer
        .append_at(vec![input(LogLevel::Info, &[Domain::Backend], "a")], base)
        .emit_now);
    assert!(buffer
        .append_at(
            vec![input(LogLevel::Info, &[Domain::Backend], "b")],
            base + COALESCE_WINDOW,
        )
        .emit_now);
}

#[test]
fn a_clear_is_never_coalesced_away() {
    // A consumer that misses a clear renders a buffer that no longer exists,
    // so `clear` returns the state unconditionally rather than through the
    // coalescing path.
    let buffer = LogBuffer::new();
    let sink = Recorder::default();
    let base = t0();
    buffer.append_at(vec![input(LogLevel::Info, &[Domain::Backend], "a")], base);
    // An append inside the window, so a flush is pending.
    buffer.append_at(
        vec![input(LogLevel::Info, &[Domain::Backend], "b")],
        after(base, 1),
    );

    clear_and_publish(&sink, &buffer);
    assert_eq!(sink.len(), 1);
    assert_eq!(sink.last().buffer_total, 0);
    assert_eq!(sink.last().generation, 1);
    assert!(
        buffer.take_pending_flush(base + COALESCE_WINDOW * 4).is_none(),
        "the clear settled the pending debt; a stale flush must not follow it"
    );
}

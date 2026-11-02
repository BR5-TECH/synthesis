//! Tests for the commit ledger of an `openai` Realtime session.

use std::collections::HashSet;

use tokio::sync::oneshot;

use super::*;

fn reply() -> (Reply, oneshot::Receiver<Result<String, String>>) {
    oneshot::channel()
}

/// The server names the oldest commit and completes its item.
fn name_and_complete(ledger: &mut CommitLedger, t: &mut Transcript, item: &str, text: &str) {
    t.committed(Some(item), None);
    assert!(ledger.named(Some(item)));
    t.mark_ours(item);
    t.completed(Some(item), text);
}

// VOP-FR-GLRN, VOP-FR-TWEZ, VOP-FR-XXAB
#[test]
fn a_chunk_joins_the_items_of_its_interim_commits_with_one_space() {
    let mut ledger = CommitLedger::default();
    let mut t = Transcript::default();
    ledger.sent();
    name_and_complete(&mut ledger, &mut t, "a", "Check the second");
    ledger.sent();
    name_and_complete(&mut ledger, &mut t, "b", "paragraph.");
    assert_eq!(t.text_except(ledger.closed()), "Check the second paragraph.");
    let (chunk, mut answer) = reply();
    ledger.sent();
    ledger.seal(chunk, t.known_items());
    ledger.settle(&t);
    assert!(answer.try_recv().is_err(), "the last commit is not named yet");
    t.committed(Some("c"), Some("b"));
    assert!(ledger.named(Some("c")));
    t.mark_ours("c");
    t.completed(Some("c"), "Thanks.");
    ledger.settle(&t);
    assert_eq!(answer.try_recv().unwrap().unwrap(), "Check the second paragraph. Thanks.");
}

// VOP-FR-TWEZ: the items of a sealed chunk leave the live text when the chunk
// end is sent, also an item that the server names later.
#[test]
fn the_items_of_a_sealed_chunk_leave_the_live_text_at_once() {
    let mut ledger = CommitLedger::default();
    let mut t = Transcript::default();
    ledger.sent();
    name_and_complete(&mut ledger, &mut t, "a", "Hello.");
    // An interim commit that the server did not name yet.
    ledger.sent();
    let (chunk, mut answer) = reply();
    ledger.seal(chunk, t.known_items());
    assert_eq!(t.text_except(ledger.closed()), "", "the open chunk has no text");
    t.committed(Some("b"), Some("a"));
    assert!(ledger.named(Some("b")));
    t.delta(Some("b"), "Bye");
    assert_eq!(t.text_except(ledger.closed()), "", "a late item of the old chunk");
    // The next chunk.
    ledger.sent();
    t.committed(Some("c"), Some("b"));
    assert!(ledger.named(Some("c")));
    t.mark_ours("c");
    t.delta(Some("c"), "Next");
    assert_eq!(t.text_except(ledger.closed()), "Next");
    t.completed(Some("b"), "Bye.");
    ledger.settle(&t);
    assert_eq!(answer.try_recv().unwrap().unwrap(), "Hello. Bye.");
}

// VOP-FR-TWEZ: a chunk end with no commit replies with the items it holds,
// and an empty chunk replies with no text.
#[test]
fn a_chunk_end_with_no_new_audio_replies_with_its_items() {
    let mut ledger = CommitLedger::default();
    let mut t = Transcript::default();
    let (empty, mut nothing) = reply();
    ledger.seal(empty, t.known_items());
    ledger.settle(&t);
    assert_eq!(nothing.try_recv().unwrap().unwrap(), "");
    ledger.sent();
    name_and_complete(&mut ledger, &mut t, "a", "Only this.");
    let (chunk, mut answer) = reply();
    ledger.seal(chunk, t.known_items());
    ledger.settle(&t);
    assert_eq!(answer.try_recv().unwrap().unwrap(), "Only this.");
}

// VOP-FR-GLRN: a commit with no audio is named with no item.
#[test]
fn an_empty_commit_names_no_item() {
    let mut ledger = CommitLedger::default();
    let t = Transcript::default();
    assert!(!ledger.named(None), "no commit waits");
    ledger.sent();
    let (chunk, mut answer) = reply();
    ledger.seal(chunk, t.known_items());
    ledger.settle(&t);
    assert!(answer.try_recv().is_err());
    assert!(ledger.named(None));
    ledger.settle(&t);
    assert_eq!(answer.try_recv().unwrap().unwrap(), "");
}

// VOP-FR-QQYG: the session is idle only when every commit is named and every
// named item is completed, interim items included.
#[test]
fn the_session_ends_after_each_interim_item_is_completed() {
    let mut ledger = CommitLedger::default();
    let mut t = Transcript::default();
    assert!(ledger.idle(&t));
    ledger.sent();
    assert!(!ledger.idle(&t), "a commit waits");
    t.committed(Some("a"), None);
    ledger.named(Some("a"));
    assert!(!ledger.idle(&t), "the item is not completed");
    t.completed(Some("a"), "Done.");
    assert!(ledger.idle(&t));
}

// VOP-FR-XXAB: an item that the server starts by itself keeps the new line.
#[test]
fn a_server_item_continues_on_a_new_line_and_ours_after_a_space() {
    let mut t = Transcript::default();
    t.completed(Some("a"), "One.");
    t.committed(Some("b"), Some("a"));
    t.completed(Some("b"), "Two.");
    assert_eq!(t.text(), "One.\nTwo.");
    t.committed(Some("c"), Some("b"));
    t.mark_ours("c");
    t.completed(Some("c"), "Three.");
    assert_eq!(t.text(), "One.\nTwo. Three.");
    let only: HashSet<String> = ["a".to_string(), "c".to_string()].into();
    assert_eq!(t.text_of(&only), "One. Three.");
}

// VOP-FR-KIMU: a word gap commits after 1 s of audio that no commit holds, and
// the audio alone commits at 4 s.
#[test]
fn an_interim_commit_is_due_at_a_gap_after_one_second_or_at_four_seconds() {
    let rate = 16_000;
    assert!(!super::super::interim_due(0, rate, true));
    assert!(!super::super::interim_due(15_999, rate, true));
    assert!(super::super::interim_due(16_000, rate, true));
    assert!(!super::super::interim_due(63_999, rate, false));
    assert!(super::super::interim_due(64_000, rate, false));
}

// A failed session fails each chunk that waits.
#[test]
fn a_failure_fails_each_waiting_chunk() {
    let mut ledger = CommitLedger::default();
    let t = Transcript::default();
    ledger.sent();
    let (chunk, mut answer) = reply();
    ledger.seal(chunk, t.known_items());
    ledger.fail_all("voice_session_lost");
    assert_eq!(answer.try_recv().unwrap().unwrap_err(), "voice_session_lost");
}

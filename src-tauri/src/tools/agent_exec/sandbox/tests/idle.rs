//! The stop of an idle queue's VM (EAC-FR-LQRI).

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::Poll;

use super::*;

/// A pool whose `vm-a` booted once with `spec`.
fn booted_pool(fx: &Fixture, spec: &SandboxSpec) -> (Arc<RecordingSandbox>, SandboxPool) {
    let runtime = RecordingSandbox::with_boots(Vec::new());
    let pool = SandboxPool::new(runtime.clone());
    let s = settings();
    block_on(pool.launch(&Silent, launch(fx, "vm-a", spec, &s, None), process(&[]))).unwrap();
    (runtime, pool)
}

// EAC-FR-LQRI, EAC-FR-UEFI: a queue that is still idle when the stop holds its
// VM has the VM stopped, and the next launch of the scope boots a new VM.
#[test]
fn an_idle_queue_stops_its_vm_and_the_next_launch_boots_again() {
    let fx = fixture();
    let spec = spec("agent:1", "/w/idle");
    let (runtime, pool) = booted_pool(&fx, &spec);
    let s = settings();
    let stopped = block_on(pool.stop_when("vm-a", || true));
    assert!(stopped);
    assert_eq!(pool.booted("vm-a"), None);
    block_on(pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&[]))).unwrap();
    assert_eq!(lifecycle(&runtime), vec!["boot", "stop", "boot"]);
}

// EAC-FR-LQRI: a queue that has work again when the stop holds its VM keeps
// the VM, and the next launch reuses it.
#[test]
fn a_queue_with_work_again_keeps_its_vm() {
    let fx = fixture();
    let spec = spec("agent:1", "/w/busy");
    let (runtime, pool) = booted_pool(&fx, &spec);
    let s = settings();
    let asked = AtomicUsize::new(0);
    let stopped = block_on(pool.stop_when("vm-a", || {
        asked.fetch_add(1, Ordering::SeqCst);
        false
    }));
    assert!(!stopped);
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    assert_eq!(pool.booted("vm-a"), Some(spec.clone()));
    block_on(pool.launch(&Silent, launch(&fx, "vm-a", &spec, &s, None), process(&[]))).unwrap();
    assert_eq!(lifecycle(&runtime), vec!["boot", "refresh"]);
}

// EAC-FR-LQRI: a scope with no VM, or whose VM already stopped, stops nothing
// and is not asked whether it is idle.
#[test]
fn a_scope_with_no_vm_stops_nothing() {
    let fx = fixture();
    let spec = spec("agent:1", "/w/none");
    let (runtime, pool) = booted_pool(&fx, &spec);
    let asked = AtomicUsize::new(0);
    let ask = || {
        asked.fetch_add(1, Ordering::SeqCst);
        true
    };
    block_on(async {
        assert!(!pool.stop_when("vm-unknown", ask).await);
        assert!(pool.stop_when("vm-a", ask).await);
        assert!(!pool.stop_when("vm-a", ask).await);
    });
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    assert_eq!(lifecycle(&runtime), vec!["boot", "stop"]);
}

// EAC-FR-LQRI: the stop waits for a launch that holds the VM, and asks whether
// the queue is idle only after that launch ended.
#[test]
fn the_stop_waits_for_the_launch_that_holds_the_vm() {
    let fx = fixture();
    let spec = spec("agent:1", "/w/held");
    let (runtime, pool) = booted_pool(&fx, &spec);
    let asked = AtomicUsize::new(0);
    let stopped = block_on(async {
        let slot = pool.slot("vm-a");
        let held = slot.lock().await;
        let mut stopping = std::pin::pin!(pool.stop_when("vm-a", || {
            asked.fetch_add(1, Ordering::SeqCst);
            true
        }));
        let first = std::future::poll_fn(|cx| Poll::Ready(stopping.as_mut().poll(cx))).await;
        assert!(first.is_pending(), "the stop waits while a launch holds the VM");
        assert_eq!(asked.load(Ordering::SeqCst), 0, "asked while a launch held the VM");
        assert_eq!(lifecycle(&runtime), vec!["boot"]);
        drop(held);
        stopping.await
    });
    assert!(stopped);
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    assert_eq!(lifecycle(&runtime), vec!["boot", "stop"]);
}

// EAC-FR-LQRI: the stop asks whether the queue is idle when it holds the VM,
// not when the stop was asked for. A queue that got work while the stop
// waited keeps its VM.
#[test]
fn a_queue_that_gets_work_while_the_stop_waits_keeps_its_vm() {
    let fx = fixture();
    let spec = spec("agent:1", "/w/again");
    let (runtime, pool) = booted_pool(&fx, &spec);
    let idle = std::sync::atomic::AtomicBool::new(true);
    let stopped = block_on(async {
        let slot = pool.slot("vm-a");
        let held = slot.lock().await;
        let mut stopping = std::pin::pin!(pool.stop_when("vm-a", || idle.load(Ordering::SeqCst)));
        let first = std::future::poll_fn(|cx| Poll::Ready(stopping.as_mut().poll(cx))).await;
        assert!(first.is_pending(), "the stop waits while a launch holds the VM");
        idle.store(false, Ordering::SeqCst);
        drop(held);
        stopping.await
    });
    assert!(!stopped);
    assert_eq!(pool.booted("vm-a"), Some(spec));
    assert_eq!(lifecycle(&runtime), vec!["boot"]);
}

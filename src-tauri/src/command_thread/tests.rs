mod source;

use super::run_blocking;
use crate::project_scope::context;

// CEX-FR-VYEQ: with no async runtime, the body runs on the calling thread.
#[test]
fn runs_the_body_on_the_calling_thread_with_no_runtime() {
    let caller = std::thread::current().id();
    let ran_on = run_blocking(|| std::thread::current().id());
    assert_eq!(ran_on, caller);
}

// CEX-FR-VYEQ: in a single-threaded runtime, the body runs as a plain call and
// does not panic, which `block_in_place` would.
#[test]
fn runs_the_body_in_a_single_threaded_runtime() {
    let runtime = tokio::runtime::Builder::new_current_thread().build().unwrap();
    let value = runtime.block_on(async { run_blocking(|| 7) });
    assert_eq!(value, 7);
}

// CEX-FR-KFVT: on the only worker of a multi-threaded runtime, a body that
// blocks until another task runs does not stop that task. Without the hand-off
// of `block_in_place`, the other task never runs and the wait times out.
#[test]
fn a_blocking_body_starves_no_other_task_on_a_one_worker_runtime() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(1).build().unwrap();
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    let outcome = runtime.block_on(async move {
        let blocking = tokio::spawn(async move {
            run_blocking(move || rx.recv_timeout(std::time::Duration::from_secs(30)))
        });
        tokio::task::yield_now().await;
        let other = tokio::spawn(async move { tx.send(()).unwrap() });
        let outcome = blocking.await.unwrap();
        other.await.unwrap();
        outcome
    });
    assert_eq!(outcome, Ok(()));
}

// CEX-FR-VKTJ: the body sees the thread-local project context of the caller.
#[test]
fn the_body_sees_the_project_context_on_a_worker() {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).build().unwrap();
    let seen = runtime.block_on(async {
        tokio::spawn(async { context::run_in("project-a", || run_blocking(context::current)) })
            .await
            .unwrap()
    });
    assert_eq!(seen.as_deref(), Some("project-a"));
}

//! The project of the running code (`PWN-project-windows.md` PWN-FR-CTXT).
//!
//! A command that serves one project runs inside the context of that project.
//! Code that the command calls reads the context with [`current`] and gets the
//! values of that project, without a project id in each signature. A task or a
//! thread that the command starts must carry the context on: use [`spawn`],
//! [`spawn_blocking`], and [`thread_spawn`] in place of the plain calls, and
//! [`wrap_fn`] for a thread that another API starts.

use std::cell::RefCell;
use std::future::Future;

tokio::task_local! {
    static TASK_PROJECT: String;
}

thread_local! {
    static THREAD_PROJECT: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// The project id of the running code, if a command or a task that carries a
/// context started it.
pub fn current() -> Option<String> {
    TASK_PROJECT
        .try_with(|id| id.clone())
        .ok()
        .or_else(|| THREAD_PROJECT.with(|slot| slot.borrow().clone()))
}

struct Restore(Option<String>);

impl Drop for Restore {
    fn drop(&mut self) {
        let previous = self.0.take();
        THREAD_PROJECT.with(|slot| *slot.borrow_mut() = previous);
    }
}

/// Run a closure in the context of one project. The context of the thread comes
/// back when the closure ends.
pub fn run_in<T>(project_id: &str, work: impl FnOnce() -> T) -> T {
    let previous = THREAD_PROJECT.with(|slot| slot.borrow_mut().replace(project_id.to_string()));
    let _restore = Restore(previous);
    work()
}

/// Run a future in the context of one project. Every poll of the future sees it.
pub async fn run_future_in<F: Future>(project_id: String, work: F) -> F::Output {
    TASK_PROJECT.scope(project_id, work).await
}

/// [`run_in`] for the project id of a call. An empty id is a call that names no
/// project: it runs as it is, so code below it can still find the one open
/// project.
pub fn run_in_project<T>(project_id: &str, work: impl FnOnce() -> T) -> T {
    if project_id.is_empty() {
        work()
    } else {
        run_in(project_id, work)
    }
}

/// [`run_future_in`] for the project id of a call. An empty id runs the future
/// as it is.
pub async fn run_future_in_project<F: Future>(project_id: String, work: F) -> F::Output {
    if project_id.is_empty() {
        work.await
    } else {
        run_future_in(project_id, work).await
    }
}

/// Run a closure in a captured context. `None` runs it as it is.
pub fn run_captured<T>(captured: Option<String>, work: impl FnOnce() -> T) -> T {
    match captured {
        Some(id) => run_in(&id, work),
        None => work(),
    }
}

/// Wrap a closure so that it runs in the context that the caller has now.
pub fn wrap_fn<T>(work: impl FnOnce() -> T) -> impl FnOnce() -> T {
    let captured = current();
    move || run_captured(captured, work)
}

/// Wrap an event handler so that every call runs in the context that the caller
/// has now. A file watch uses it: its callback runs on a thread that the watch
/// library starts, which has no context of its own.
pub fn wrap_handler<A, T>(mut work: impl FnMut(A) -> T) -> impl FnMut(A) -> T {
    let captured = current();
    move |argument| run_captured(captured.clone(), || work(argument))
}

/// `tauri::async_runtime::spawn`, with the context of the caller.
pub fn spawn<F>(work: F) -> tauri::async_runtime::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    match current() {
        Some(id) => tauri::async_runtime::spawn(run_future_in(id, work)),
        None => tauri::async_runtime::spawn(work),
    }
}

/// `tauri::async_runtime::spawn_blocking`, with the context of the caller.
pub fn spawn_blocking<F, R>(work: F) -> tauri::async_runtime::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    tauri::async_runtime::spawn_blocking(wrap_fn(work))
}

/// `std::thread::spawn`, with the context of the caller.
pub fn thread_spawn<F, R>(work: F) -> std::thread::JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    std::thread::spawn(wrap_fn(work))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_context_by_default() {
        assert_eq!(current(), None);
    }

    #[test]
    fn run_in_sets_and_restores_the_context() {
        run_in("a", || {
            assert_eq!(current().as_deref(), Some("a"));
            run_in("b", || assert_eq!(current().as_deref(), Some("b")));
            assert_eq!(current().as_deref(), Some("a"));
        });
        assert_eq!(current(), None);
    }

    #[test]
    fn a_thread_carries_the_context() {
        let seen = run_in("a", || thread_spawn(current).join().unwrap());
        assert_eq!(seen.as_deref(), Some("a"));
        assert_eq!(thread_spawn(current).join().unwrap(), None);
    }

    #[test]
    fn a_blocking_task_carries_the_context() {
        let seen = run_in("a", || tauri::async_runtime::block_on(spawn_blocking(current)).unwrap());
        assert_eq!(seen.as_deref(), Some("a"));
    }

    #[test]
    fn a_future_carries_the_context() {
        let seen = tauri::async_runtime::block_on(async {
            run_future_in("a".to_string(), async {
                let inner = spawn(async { current() }).await.unwrap();
                (current(), inner)
            })
            .await
        });
        assert_eq!(seen.0.as_deref(), Some("a"));
        assert_eq!(seen.1.as_deref(), Some("a"));
    }

    #[test]
    fn two_threads_keep_their_own_contexts() {
        let a = thread_spawn(|| {
            run_in("a", || {
                std::thread::yield_now();
                current()
            })
        });
        let b = thread_spawn(|| {
            run_in("b", || {
                std::thread::yield_now();
                current()
            })
        });
        assert_eq!(a.join().unwrap().as_deref(), Some("a"));
        assert_eq!(b.join().unwrap().as_deref(), Some("b"));
    }
}

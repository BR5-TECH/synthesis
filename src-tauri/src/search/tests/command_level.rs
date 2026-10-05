//! The commands against a real headless app
//! (SCC-FR-02, SCC-FR-17, SCC-FR-18, SCC-FR-19, PRG-FR-09).
//!
//! One part of `../tests/mod.rs`, which holds the sink these run against.

use super::*;

// -----------------------------------------------------------------------
// Command level (SCC-FR-02, SCC-FR-19 / SCC-FR-17 / SCC-FR-18, PRG-FR-09) — a real, headless app
// -----------------------------------------------------------------------

/// A mock app carrying every store `start_search` reaches for, rooted on
/// `root` when one is given (`None` = no project open).
fn mock_search_app(root: Option<&Path>) -> tauri::App<tauri::test::MockRuntime> {
    use tauri::Manager;
    let app = tauri::test::mock_app();
    app.manage(crate::project::ProjectState::default());
    app.manage(CandidateStore::default());
    app.manage(SearchRegistry::default());
    app.manage(ProgressRegistry::default());
    if let Some(root) = root {
        app.state::<crate::project::ProjectState>()
            .set_root(root.to_path_buf());
    }
    app
}

/// Subscribe to `"search ended"` and return a receiver of its reasons.
fn watch_endings(
    app: &tauri::App<tauri::test::MockRuntime>,
) -> std::sync::mpsc::Receiver<String> {
    use tauri::Listener;
    let (tx, rx) = std::sync::mpsc::channel();
    app.listen(SEARCH_ENDED, move |event| {
        let payload: serde_json::Value =
            serde_json::from_str(event.payload()).unwrap_or_default();
        let reason = payload
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let _ = tx.send(reason);
    });
    rx
}

/// Wait for a search to finish, or fail loudly rather than hang the suite.
fn await_ending(rx: &std::sync::mpsc::Receiver<String>) -> String {
    rx.recv_timeout(Duration::from_secs(10))
        .expect("SCC-FR-13: a search must always emit its terminal event")
}

#[test]
fn scc_ts20_a_search_attributes_progress_and_always_leaves_the_in_flight_set() {
    // SCC-FR-18 / PRG-FR-09: every search registers under `kind = "search"`
    // and terminates when it ends, whatever the reason. The half worth
    // pinning is the one a bug leaves broken forever: an operation stranded
    // as permanently running would sit in the status bar for the rest of the
    // session.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    for n in 0..40 {
        write(root, &format!("f{n:02}.md"), "needle\n");
    }
    let app = mock_search_app(Some(root));
    let endings = watch_endings(&app);

    let id = start_search(
        app.handle().clone(),
        "needle".into(),
        SearchMode::LiteralInsensitive,
        SearchScope::Full,
        app.state::<crate::project::ProjectState>(),
        app.state::<SearchRegistry>(),
    )
    .expect("a valid query starts");
    // SCC-FR-02, SCC-FR-19: the id comes back immediately, before the sweep is done.
    assert!(id.starts_with("search-"), "{id}");

    assert_eq!(await_ending(&endings), "completed");
    // The operation is terminated on the search's own thread, just after the
    // terminal event; give that last step a bounded moment to land.
    let progress = app.state::<ProgressRegistry>();
    for _ in 0..200 {
        if !progress.in_flight().iter().any(|o| o.kind == "search") {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !progress.in_flight().iter().any(|o| o.kind == "search"),
        "the search's operation was left running: {:?}",
        progress.in_flight()
    );
}

#[test]
fn scc_ts19_with_no_project_open_a_search_ends_completed_and_terminates_its_operation() {
    // SCC-FR-17 through the command, which is where the no-project branch
    // actually lives — and where the PRG operation registered a moment
    // earlier has to be terminated on the way out.
    use tauri::Manager;
    let app = mock_search_app(None);
    let endings = watch_endings(&app);

    let id = start_search(
        app.handle().clone(),
        "anything".into(),
        SearchMode::LiteralInsensitive,
        SearchScope::Capped,
        app.state::<crate::project::ProjectState>(),
        app.state::<SearchRegistry>(),
    )
    .expect("no project open is not an error");
    assert!(id.starts_with("search-"));

    assert_eq!(await_ending(&endings), "completed");
    let progress = app.state::<ProgressRegistry>();
    for _ in 0..200 {
        if !progress.in_flight().iter().any(|o| o.kind == "search") {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(
        !progress.in_flight().iter().any(|o| o.kind == "search"),
        "the no-project branch stranded its operation: {:?}",
        progress.in_flight()
    );
}

#[test]
fn ts7_an_invalid_query_starts_nothing_at_the_command_level() {
    // SCC-FR-06: no id is issued, no producer or consumer starts, and
    // neither event is emitted — so nothing is registered with PRG either.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_search_app(Some(dir.path()));

    let err = start_search(
        app.handle().clone(),
        "foo(".into(),
        SearchMode::Regex,
        SearchScope::Capped,
        app.state::<crate::project::ProjectState>(),
        app.state::<SearchRegistry>(),
    )
    .unwrap_err();

    assert_eq!(err, INVALID_QUERY);
    assert!(
        app.state::<ProgressRegistry>().in_flight().is_empty(),
        "a rejected query must register no operation"
    );
}

#[test]
fn cancelling_an_unknown_search_id_is_a_noop() {
    // SCC-FR-14: an id that has already ended — or never existed — is a
    // no-op, not an error, so a consumer need not track whether its search
    // is still in flight.
    use tauri::Manager;
    let app = mock_search_app(None);
    cancel_search("search-never-existed".into(), app.state::<SearchRegistry>());
}

#[test]
fn a_zero_width_regex_match_produces_a_usable_snippet() {
    // `a*` matches the empty string at offset 0 of every line, so `find`
    // returns a zero-length range. The snippet windowing must not panic or
    // produce an empty snippet for a hit it reported as a content match.
    let dir = tempfile::TempDir::new().unwrap();
    let root = &crate::fs::RootFs::for_root(dir.path());
    write(root, "zero.md", "some ordinary text\n");

    let (sink, _) = search(root, "x*", SearchMode::Regex, SearchScope::Full);
    let hits = sink.hits();
    assert_eq!(hits.len(), 1, "{hits:?}");
    assert_eq!(hits[0].line, Some(1));
    assert_eq!(hits[0].snippet.as_deref(), Some("some ordinary text"));
}

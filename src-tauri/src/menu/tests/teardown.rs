//! Held teardowns (SNV-FR-25 / SNV-FR-26 / EDT-FR-33) — the exit gate and the
//! Close-Project path.

use super::*;

// ------------------------------------------------------------------
// Held teardowns (SNV-FR-25 / SNV-FR-26 / EDT-FR-33). Emitting and the run
// loop need a GUI runtime, so what is pinned here is the pure gate logic
// that decides whether a quit is held — including the property that makes
// the application impossible to wedge.
// ------------------------------------------------------------------

#[test]
fn exit_gate_holds_a_quit_until_the_frontend_answers() {
    // SNV-FR-26: the first request is held so the frontend can write pending
    // changes (EDT-FR-33) — and so is a repeat that arrives while it is
    // still working, since killing the process mid-write is exactly what
    // this hold exists to prevent.
    let gate = ExitGate::default();
    let t0 = Instant::now();
    assert!(gate.claim_at(t0, EXIT_FLUSH_GRACE), "first quit must be held");
    assert!(
        gate.claim_at(t0 + Duration::from_secs(1), EXIT_FLUSH_GRACE),
        "a repeat while the flush is in flight must also be held"
    );
}

#[test]
fn exit_gate_gives_up_on_an_unanswered_quit() {
    // The anti-wedge escape: once the grace period since the FIRST
    // unanswered request has elapsed, the next request quits regardless, so
    // a broken frontend can never make the application unquittable.
    let gate = ExitGate::default();
    let t0 = Instant::now();
    assert!(gate.claim_at(t0, EXIT_FLUSH_GRACE));
    assert!(
        !gate.claim_at(t0 + EXIT_FLUSH_GRACE, EXIT_FLUSH_GRACE),
        "an unanswered quit must stop being held once its grace runs out"
    );
}

#[test]
fn exit_gate_repeats_do_not_extend_the_deadline() {
    // The deadline is measured from the first unanswered request, so a user
    // hammering the quit key cannot push it out indefinitely.
    let gate = ExitGate::default();
    let t0 = Instant::now();
    assert!(gate.claim_at(t0, EXIT_FLUSH_GRACE));
    for i in 1..5 {
        assert!(gate.claim_at(t0 + Duration::from_secs(i), EXIT_FLUSH_GRACE));
    }
    assert!(!gate.claim_at(t0 + EXIT_FLUSH_GRACE, EXIT_FLUSH_GRACE));
}

#[test]
fn exit_gate_holds_again_after_release() {
    // `finish_exit` releases the hold whichever way the frontend answered,
    // so a LATER quit is held and flushed too rather than slipping through
    // unflushed (EDT-FR-33).
    let gate = ExitGate::default();
    let t0 = Instant::now();
    assert!(gate.claim_at(t0, EXIT_FLUSH_GRACE));
    gate.release();
    assert!(
        gate.claim_at(t0 + Duration::from_secs(1), EXIT_FLUSH_GRACE),
        "a quit after a release must be held again"
    );
}

#[test]
fn cancelling_a_quit_keeps_the_application_running_and_frees_the_gate() {
    // EDT-FR-32: a write blocked by an unresolved divergence cancels the
    // quit. `exit_decision` is the pure half of `finish_exit`, so this pins
    // that `proceed: false` does NOT quit — and that the hold is released
    // either way.
    let gate = ExitGate::default();
    let t0 = Instant::now();
    assert!(gate.claim_at(t0, EXIT_FLUSH_GRACE));

    assert!(!exit_decision(false, &gate), "a cancelled quit must not exit");
    assert!(
        gate.claim_at(t0 + Duration::from_secs(1), EXIT_FLUSH_GRACE),
        "cancelling releases the hold, so the next quit is flushed too"
    );

    assert!(exit_decision(true, &gate), "an answered quit must exit");
}

#[test]
fn a_programmatic_exit_is_never_held() {
    // `finish_exit` quits via `app.exit(0)`, which re-enters ExitRequested
    // carrying a code. Holding that would deadlock the quit it just
    // authorised, so a coded request always passes through — and must not
    // consume the gate either.
    let gate = ExitGate::default();
    assert!(!begin_exit_decision(
        Some(0),
        &gate,
        Instant::now(),
        EXIT_FLUSH_GRACE
    ));
    assert!(
        gate.claim_at(Instant::now(), EXIT_FLUSH_GRACE),
        "a programmatic exit must leave the gate untouched"
    );
    assert!(begin_exit_decision(
        None,
        &ExitGate::default(),
        Instant::now(),
        EXIT_FLUSH_GRACE
    ));
}


#[test]
fn close_project_menu_item_leaves_the_project_open_for_the_frontend_to_flush() {
    // SNV-FR-25 / EDT-FR-33: the teardown must NOT happen here. The frontend
    // has to write every dirty artifact first, and `save_artifact_contents`
    // needs the project root — tearing down on the menu event would make
    // every one of those writes fail with "no project open", which would
    // cancel the close forever (EDT-FR-32 treats a failed write as a
    // blocker). Pinned behaviourally, since this is an ordering property
    // rather than a shape.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_project(dir.path());

    handle_menu_event(&app.handle(), MENU_CLOSE_PROJECT);

    assert!(
        app.state::<ProjectState>().require_root().is_ok(),
        "Close project must relay only; the teardown waits for the \
         frontend's close_project call so its writes can land first"
    );
}

#[test]
fn close_project_command_performs_the_teardown() {
    // …and when the frontend does call it, the project is closed for real
    // (PST-FR-14 / ASC-FR-14).
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_project(dir.path());

    crate::project::close_project(
        app.handle().clone(),
        app.state::<ProjectState>(),
        app.state::<ProjectWatcher>(),
        app.state::<ContentTracker>(),
        app.state::<crate::draft_watcher::DraftsWatcher>(),
        app.state::<crate::progress::ProgressRegistry>(),
        app.state::<crate::scanning::CandidateStore>(),
        app.state::<crate::search::SearchRegistry>(),
        app.state::<crate::agent_conversations::TurnRegistry>(),
    );

    assert_eq!(
        app.state::<ProjectState>().require_root().unwrap_err(),
        "no project open"
    );
}

#[test]
fn close_project_terminates_the_projects_in_flight_operations() {
    // PRG-FR-13: closing a project terminates every operation scoped to its
    // content root, so none is left running against a root the application
    // has stopped reading. An unscoped operation — a plugin install, which
    // can be in flight before or between projects (PRG-FR-15) — survives.
    //
    // The call lives in `close_project`, not in `deactivate_project`, so no
    // teardown test covers it; and it must run BEFORE the root is cleared or
    // there is nothing left to scope by.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_project(dir.path());
    let registry = app.state::<crate::progress::ProgressRegistry>();
    let (_, scoped) = registry.register_at(
        "scan",
        "Indexing…",
        Some(dir.path().to_path_buf()),
        None,
        std::time::Instant::now(),
    );
    let (_, unscoped) = registry.register_at(
        "install",
        "Installing…",
        None,
        None,
        std::time::Instant::now(),
    );

    crate::project::close_project(
        app.handle().clone(),
        app.state::<ProjectState>(),
        app.state::<ProjectWatcher>(),
        app.state::<ContentTracker>(),
        app.state::<crate::draft_watcher::DraftsWatcher>(),
        app.state::<crate::progress::ProgressRegistry>(),
        app.state::<crate::scanning::CandidateStore>(),
        app.state::<crate::search::SearchRegistry>(),
        app.state::<crate::agent_conversations::TurnRegistry>(),
    );

    let remaining: Vec<String> = app
        .state::<crate::progress::ProgressRegistry>()
        .in_flight()
        .into_iter()
        .map(|o| o.id)
        .collect();
    assert!(
        !remaining.contains(&scoped.id),
        "the closed project's scan must be terminated: {remaining:?}"
    );
    assert!(
        remaining.contains(&unscoped.id),
        "an unscoped operation outlives the project: {remaining:?}"
    );
}

#[test]
fn a_quit_is_never_held_once_the_last_window_is_gone() {
    // The application-hang regression. A hold is released ONLY by the
    // frontend calling `finish_exit`, so holding a quit when no webview is
    // left to receive the request — the exact state the run loop's own exit
    // request arrives in, since it fires after the last window is destroyed
    // — keeps the process alive forever with no window and no way to quit
    // it. The user sees a dock icon and nothing else.
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_project(dir.path());

    assert!(
        !begin_exit(&app.handle().clone(), &app.state::<ExitGate>(), None),
        "a quit with no webview left to answer it must not be held"
    );
    // A grace period later, so this distinguishes an untouched gate from one
    // that was just claimed (which would still be inside its grace).
    assert!(
        app.state::<ExitGate>()
            .claim_at(Instant::now() + EXIT_FLUSH_GRACE, EXIT_FLUSH_GRACE),
        "refusing to hold must not consume the gate either"
    );
}

#[test]
fn finish_exit_releases_the_hold_and_only_quits_when_told_to() {
    // SNV-FR-26 / EDT-FR-32: the command body itself — a cancelled quit
    // (`proceed: false`) must leave the application running, and must free
    // the gate so the NEXT quit is held and flushed rather than slipping
    // through. Only the cancel path can be driven through the command:
    // `app.exit` is `unimplemented!()` on the mock runtime and panics, so a
    // `proceed: true` call cannot be made here — which is also what makes
    // this test a real detector, since an inverted condition would quit and
    // panic. The quit/no-quit decision itself is `exit_decision`.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_project(dir.path());
    let t0 = Instant::now();
    assert!(app.state::<ExitGate>().claim_at(t0, EXIT_FLUSH_GRACE));

    finish_exit(false, app.handle().clone(), app.state::<ExitGate>());

    assert!(
        app.state::<ExitGate>()
            .claim_at(t0 + Duration::from_secs(1), EXIT_FLUSH_GRACE),
        "a cancelled quit must release the hold"
    );
}

#[test]
fn begin_exit_holds_a_user_quit_and_passes_a_programmatic_one_through() {
    // `begin_exit` is handed the request's exit code; this pins that a
    // code-carrying (programmatic) request is never held — otherwise the
    // `app.exit(0)` that `finish_exit` performs would be held again and the
    // application would hang for the whole grace period on every quit.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_window(dir.path());

    assert!(
        !begin_exit(&app.handle().clone(), &app.state::<ExitGate>(), Some(0)),
        "a programmatic exit must pass straight through"
    );
    assert!(
        begin_exit(&app.handle().clone(), &app.state::<ExitGate>(), None),
        "a user quit must be held so pending changes can be written"
    );
}

#[test]
fn a_wedged_frontend_stops_holding_the_window_closed() {
    // The anti-wedge escape, end to end. A frontend that never answers keeps
    // the close held only until the grace period runs out; the next request
    // is not held, so the window closes — and the run loop's follow-up exit
    // request is not held either (no webview left), so the process exits.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_window(dir.path());
    let gate = app.state::<ExitGate>();
    let handle = app.handle().clone();
    let t0 = Instant::now();

    assert!(
        begin_exit_at(&handle, &gate, None, t0),
        "the first close is held so pending changes can be written"
    );
    assert!(
        begin_exit_at(&handle, &gate, None, t0 + Duration::from_secs(1)),
        "a second close while the flush is in flight is held too"
    );
    assert!(
        !begin_exit_at(&handle, &gate, None, t0 + EXIT_FLUSH_GRACE),
        "…but once the grace runs out the close proceeds, so a frontend \
         that never answers cannot make the window unclosable"
    );
}

#[test]
fn only_a_close_request_holds_the_application_open() {
    // The close interception must key on `CloseRequested` specifically.
    // Watching the wrong variant is the whole bug in miniature: `Destroyed`
    // fires only once the window is already gone, so a hold placed there
    // could never be answered — the application would run on with no window.
    // `CloseRequested` carries a `CloseRequestApi` that cannot be built
    // outside the runtime, so this pins the negative half: no OTHER window
    // event may claim the gate.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_window(dir.path());
    let window = app
        .webview_windows()
        .remove("main")
        .expect("mock window")
        .as_ref()
        .window();

    for event in [
        tauri::WindowEvent::Destroyed,
        tauri::WindowEvent::Focused(true),
        tauri::WindowEvent::Focused(false),
    ] {
        handle_window_event(&window, &event);
    }

    // Claiming a whole grace period later succeeds only from an UNCLAIMED
    // gate: a hold placed just now would still be inside its grace and would
    // refuse this. (`claim_at(now)` cannot tell the two apart — it answers
    // "keep holding" in both cases.)
    assert!(
        app.state::<ExitGate>()
            .claim_at(Instant::now() + EXIT_FLUSH_GRACE, EXIT_FLUSH_GRACE),
        "no window event other than a close request may hold a quit"
    );
}

#[test]
fn exit_handlers_compile_with_a_generic_runtime() {
    // A compile-time guard only: these keep the signatures `on_window_event`
    // and `on_menu_event` require. That they are actually REGISTERED on the
    // builder is pinned in `lib.rs`.
    let _window = handle_window_event::<tauri::Wry>;
    let _menu = handle_menu_event::<tauri::Wry>;
    let _request = request_exit::<tauri::Wry>;
}

#[test]
fn exit_requested_event_name_matches_the_frontend_listener_literal() {
    // Cross-process contract guard: the frontend hard-codes this literal in
    // `src/events.ts` (MENU_EXIT_REQUESTED).
    assert_eq!(MENU_EXIT_REQUESTED, "menu:exit-requested");
}

#[test]
fn exit_requested_is_not_a_menu_id() {
    // It is emitted from the run loop, not from a menu item — treating it as
    // an owned menu id would misroute the predefined quit role.
    assert_eq!(menu_event_action(MENU_EXIT_REQUESTED), None);
}

#[test]
fn exit_helpers_compile_with_a_generic_runtime() {
    let _begin = begin_exit::<tauri::Wry>;
    let _finish = finish_exit::<tauri::Wry>;
}

#[test]
fn quitting_from_the_menu_asks_the_frontend_to_flush_first() {
    // SNV-FR-26 / EDT-FR-33: activating Exit must not quit on the spot — it
    // claims the gate and waits for `finish_exit`, so the frontend gets to
    // write its pending changes (and to cancel the quit if one of those
    // writes is blocked, EDT-FR-32).
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_window(dir.path());

    handle_menu_event(&app.handle().clone(), MENU_EXIT);

    assert!(
        !app.state::<ExitGate>()
            .claim_at(Instant::now() + EXIT_FLUSH_GRACE, EXIT_FLUSH_GRACE),
        "Exit must leave a hold outstanding for the frontend to answer"
    );
}

#[test]
fn menu_event_names_match_the_frontend_listener_literals_byte_for_byte() {
    // Cross-process contract guard (conventions.md: event names are
    // byte-for-byte). These ids double as the Tauri event names the frontend
    // listens for in `src/App.tsx`. Every OTHER Rust test references the
    // CONSTANTS, so they would all still pass if a constant's VALUE drifted;
    // the JS side hard-codes the literals. Pinning the literals here makes a
    // rename on the Rust side fail a test instead of silently no-op'ing the
    // menu at runtime.
    assert_eq!(MENU_NEW_FILE, "menu:new-file");
    assert_eq!(MENU_NEW_FOLDER, "menu:new-folder");
    assert_eq!(MENU_NEW_ARTIFACT, "menu:new-artifact");
    assert_eq!(MENU_SAVE, "menu:save");
    assert_eq!(MENU_SAVE_ALL, "menu:save-all");
    assert_eq!(MENU_CLOSE_PROJECT, "menu:close-project");
    assert_eq!(MENU_FIND, "menu:find");
    assert_eq!(MENU_FIND_REPLACE, "menu:find-replace");
    assert_eq!(MENU_ABOUT, "menu:about");
}

#[test]
fn file_menu_full_order_is_the_ten_items_the_spec_names() {
    // SNV-FR-23 / SWN-FR-14 / ABT-FR-KMVD: off macOS the File menu shows
    // EXACTLY ten items in order. `build_app_menu` renders the
    // `file_menu_order` labels then the `file_menu_tail` ones (About, Exit),
    // so composing the labels the same way pins the full visible sequence and
    // the ten-item count — guarding against an eleventh item leaking in.
    // The native menu cannot be built without a GUI runtime, so this is the
    // structural proxy for TS-17. For TS-20 ("Exit quits")
    // the proxy is `quitting_from_the_menu_asks_the_frontend_to_flush_first`,
    // which drives the item's id through the real dispatcher.
    let order = file_menu_order(false);
    let mut labels: Vec<&str> = order.iter().map(|(_, label, _)| *label).collect();
    labels.extend(file_menu_tail(false).iter().map(|(_, label, _)| *label));
    assert_eq!(
        labels,
        [
            "New File",
            "New Artifact",
            "New Folder",
            "Save",
            "Save All",
            "Global settings",
            "Project settings",
            "Close Project",
            "About",
            "Exit"
        ]
    );
}

#[test]
fn setting_the_save_menu_state_before_the_menu_is_installed_is_inert() {
    // `set_save_menu_state` can be called before `install_app_menu` has
    // stored the handles (the frontend pushes its first state as the shell
    // mounts). With no handles it must return quietly rather than panicking
    // on an unwrap — a panic inside a command aborts the IPC call and the
    // frontend's render path with it.
    use tauri::Manager;
    let dir = tempfile::TempDir::new().unwrap();
    let app = mock_app_with_window(dir.path());
    app.manage(SaveMenuItems::<tauri::test::MockRuntime>::default());

    set_save_menu_state(true, true, app.handle().clone(), app.state());
    set_save_menu_state(false, false, app.handle().clone(), app.state());
}

#[test]
fn save_menu_state_command_compiles_with_a_generic_runtime() {
    let _cmd = set_save_menu_state::<tauri::Wry>;
}

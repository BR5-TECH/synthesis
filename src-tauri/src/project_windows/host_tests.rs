use super::*;
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::time::Duration;
use tauri::Listener;

fn entry(id: &str) -> ProjectWindowEntry {
    ProjectWindowEntry {
        project_id: id.to_string(),
        name: id.to_string(),
        window_label: format!("{PROJECT_WINDOW_PREFIX}{id}"),
        needs_attention: false,
    }
}

// PWN-FR-SCFZ, CEX-FR-FUUP: commands that change the list on several threads
// at the same time emit one event for each change, and the last event holds
// the current list.
#[test]
fn the_last_event_holds_the_current_list_when_changes_run_at_once() {
    let app = tauri::test::mock_app();
    app.manage(ProjectWindows::default());
    let handle = app.handle().clone();
    {
        let windows = handle.state::<ProjectWindows>();
        let mut list = windows.lock();
        for id in ["a", "b", "c"] {
            list.open(entry(id));
        }
    }
    let (events_tx, events) = channel::<String>();
    handle.listen_any(EVENT_WINDOWS_CHANGED, move |event| {
        let _ = events_tx.send(event.payload().to_string());
    });
    let workers: Vec<_> = (0..8usize)
        .map(|n| {
            let app = handle.clone();
            std::thread::spawn(move || {
                for k in 0..25usize {
                    let id = ["a", "b", "c"][(n + k) % 3];
                    let flag = (n * 7 + k) % 2 == 0;
                    set_project_window_attention(id.into(), flag, app.clone(), app.state())
                        .unwrap();
                    if k % 5 == 0 {
                        activate_project_window(id.into(), app.clone(), app.state()).unwrap();
                    }
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let windows = handle.state::<ProjectWindows>();
    let requested = windows.requested.load(Ordering::SeqCst);
    assert!(requested > 0);
    let mut last = String::new();
    for _ in 0..requested {
        last = events.recv_timeout(Duration::from_secs(30)).expect("one event per change");
    }
    let expected = serde_json::to_value(windows.lock().snapshot()).unwrap();
    let got: serde_json::Value = serde_json::from_str(&last).unwrap();
    assert_eq!(got, expected);
}

// PWN-FR-NEYF, PWN-FR-SCFZ: the focus handler, which runs on the main thread,
// changes the list and returns; the event that follows from the publisher
// thread holds the change.
#[test]
fn a_change_from_the_focus_handler_publishes_the_list() {
    let app = tauri::test::mock_app();
    app.manage(ProjectWindows::default());
    let handle = app.handle().clone();
    {
        let windows = handle.state::<ProjectWindows>();
        let mut list = windows.lock();
        list.open(entry("a"));
        list.open(entry("b"));
        list.set_attention("a", true);
    }
    let (events_tx, events) = channel::<String>();
    handle.listen_any(EVENT_WINDOWS_CHANGED, move |event| {
        let _ = events_tx.send(event.payload().to_string());
    });
    note_window_focused(&handle, &format!("{PROJECT_WINDOW_PREFIX}a"));
    let event = events.recv_timeout(Duration::from_secs(30)).unwrap();
    let got: serde_json::Value = serde_json::from_str(&event).unwrap();
    assert_eq!(got["activeProjectId"], "a");
    assert_eq!(got["windows"][0]["projectId"], "a");
    assert_eq!(got["windows"][0]["needsAttention"], false);
    assert_eq!(handle.state::<ProjectWindows>().requested.load(Ordering::SeqCst), 1);
}

// PWN-FR-SCFZ: the events reach a window in the order of the changes. Each
// event shows a list at least as new as the event before it.
#[test]
fn events_follow_the_order_of_the_changes() {
    let app = tauri::test::mock_app();
    app.manage(ProjectWindows::default());
    let handle = app.handle().clone();
    handle.state::<ProjectWindows>().lock().open(entry("a"));
    let (events_tx, events) = channel::<String>();
    handle.listen_any(EVENT_WINDOWS_CHANGED, move |event| {
        let _ = events_tx.send(event.payload().to_string());
    });
    // Each change adds one entry, so the entry count tells the order.
    for n in 0..50 {
        handle.state::<ProjectWindows>().lock().open(entry(&format!("p{n}")));
        publish(&handle);
    }
    let mut last = 0usize;
    for _ in 0..50 {
        let event = events.recv_timeout(Duration::from_secs(30)).unwrap();
        let got: serde_json::Value = serde_json::from_str(&event).unwrap();
        let count = got["windows"].as_array().unwrap().len();
        assert!(count >= last, "an event showed {count} entries after one with {last}");
        last = count;
    }
    assert_eq!(last, 51);
}

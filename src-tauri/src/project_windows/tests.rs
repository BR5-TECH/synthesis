use super::*;

fn entry(id: &str) -> ProjectWindowEntry {
    ProjectWindowEntry {
        project_id: id.to_string(),
        name: format!("Project {id}"),
        window_label: format!("project-{id}"),
        needs_attention: false,
    }
}

fn list_of(ids: &[&str]) -> ProjectWindowList {
    let mut list = ProjectWindowList::default();
    for id in ids {
        list.open(entry(id));
    }
    list
}

fn order(list: &ProjectWindowList) -> Vec<&str> {
    list.entries().iter().map(|e| e.project_id.as_str()).collect()
}

#[test]
fn a_new_project_joins_at_the_end_and_becomes_active() {
    let mut list = ProjectWindowList::default();
    assert_eq!(list.open(entry("a")), OpenOutcome::Added);
    assert_eq!(list.open(entry("b")), OpenOutcome::Added);
    assert_eq!(order(&list), ["a", "b"]);
    assert_eq!(list.active_project_id(), Some("b"));
}

#[test]
fn an_open_project_is_not_opened_twice_and_its_tab_is_activated() {
    let mut list = list_of(&["a", "b"]);
    let outcome = list.open(entry("a"));
    assert_eq!(outcome, OpenOutcome::AlreadyOpen { window_label: "project-a".to_string() });
    assert_eq!(list.len(), 2);
    assert_eq!(list.active_project_id(), Some("a"));
}

#[test]
fn activating_a_tab_clears_its_needs_attention_flag() {
    let mut list = list_of(&["a", "b"]);
    assert!(list.set_attention("a", true));
    assert!(list.find("a").is_some_and(|e| e.needs_attention));
    assert_eq!(list.activate("a"), Some("project-a".to_string()));
    assert!(list.find("a").is_some_and(|e| !e.needs_attention));
}

#[test]
fn a_tab_holds_the_flag_until_it_is_activated() {
    let mut list = list_of(&["a", "b"]);
    assert!(list.set_attention("b", true));
    assert!(list.find("b").is_some_and(|e| e.needs_attention));
    list.activate("b");
    assert!(list.find("b").is_some_and(|e| !e.needs_attention));
}

#[test]
fn setting_the_flag_to_its_current_value_reports_no_change() {
    let mut list = list_of(&["a", "b"]);
    assert!(list.set_attention("a", true));
    assert!(!list.set_attention("a", true));
    assert!(list.set_attention("a", false));
    assert!(!list.set_attention("missing", true));
}

#[test]
fn closing_a_tab_that_is_not_active_keeps_the_active_tab() {
    let mut list = list_of(&["a", "b", "c"]);
    list.activate("b");
    let outcome = list.close("a").expect("a is open");
    assert!(!outcome.active_changed);
    assert_eq!(outcome.active_window_label.as_deref(), Some("project-b"));
    assert_eq!(list.active_project_id(), Some("b"));
    assert_eq!(order(&list), ["b", "c"]);
}

#[test]
fn closing_the_active_tab_activates_the_tab_that_followed_it() {
    let mut list = list_of(&["a", "b", "c"]);
    list.activate("b");
    let outcome = list.close("b").expect("b is open");
    assert!(outcome.active_changed);
    assert_eq!(outcome.active_window_label.as_deref(), Some("project-c"));
    assert_eq!(list.active_project_id(), Some("c"));
}

#[test]
fn closing_the_last_active_tab_activates_the_tab_before_it() {
    let mut list = list_of(&["a", "b", "c"]);
    let outcome = list.close("c").expect("c is open");
    assert_eq!(outcome.active_window_label.as_deref(), Some("project-b"));
    assert_eq!(list.active_project_id(), Some("b"));
}

#[test]
fn the_activated_tab_after_a_close_loses_its_needs_attention_flag() {
    let mut list = list_of(&["a", "b"]);
    list.set_attention("a", true);
    list.close("b");
    assert!(list.find("a").is_some_and(|e| !e.needs_attention));
}

#[test]
fn closing_the_only_tab_leaves_no_active_tab() {
    let mut list = list_of(&["a"]);
    let outcome = list.close("a").expect("a is open");
    assert_eq!(outcome.active_window_label, None);
    assert!(list.is_empty());
    assert_eq!(list.active_project_id(), None);
}

#[test]
fn closing_an_unknown_project_changes_nothing() {
    let mut list = list_of(&["a", "b"]);
    assert_eq!(list.close("missing"), None);
    assert_eq!(list.len(), 2);
}

#[test]
fn this_tab_replaces_the_project_and_keeps_the_place() {
    let mut list = list_of(&["a", "b", "c"]);
    list.activate("b");
    assert!(list.replace_project("project-b", "d", "Project d"));
    assert_eq!(order(&list), ["a", "d", "c"]);
    assert_eq!(list.active_project_id(), Some("d"));
    assert_eq!(list.find("d").map(|e| e.window_label.as_str()), Some("project-b"));
}

#[test]
fn this_tab_refuses_a_project_that_another_window_holds() {
    let mut list = list_of(&["a", "b"]);
    assert!(!list.replace_project("project-a", "b", "Project b"));
    assert_eq!(order(&list), ["a", "b"]);
}

#[test]
fn next_and_previous_wrap_around() {
    let mut list = list_of(&["a", "b", "c"]);
    list.activate("c");
    assert_eq!(list.neighbour(1), Some("a"));
    list.activate("a");
    assert_eq!(list.neighbour(-1), Some("c"));
    assert_eq!(list.neighbour(1), Some("b"));
}

#[test]
fn next_and_previous_need_two_tabs() {
    assert_eq!(list_of(&["a"]).neighbour(1), None);
    assert_eq!(ProjectWindowList::default().neighbour(1), None);
}

#[test]
fn the_snapshot_lists_every_window_in_order_with_the_active_project() {
    let mut list = list_of(&["a", "b"]);
    list.set_attention("a", true);
    let snapshot = list.snapshot();
    assert_eq!(snapshot.active_project_id.as_deref(), Some("b"));
    assert_eq!(snapshot.windows.len(), 2);
    assert!(snapshot.windows[0].needs_attention);
}

#[test]
fn a_new_list_is_empty_after_a_restart() {
    let list = ProjectWindowList::default();
    assert!(list.is_empty());
    assert_eq!(list.snapshot().active_project_id, None);
}

#[test]
fn the_title_is_the_full_name() {
    let name = "A very long project name that no tab strip would ever shorten";
    assert_eq!(project_window_title(name, false), name);
}

#[test]
fn the_title_starts_with_a_dot_when_the_project_needs_attention() {
    assert_eq!(project_window_title("Atlas", true), "● Atlas");
}

#[test]
fn the_first_window_and_the_project_windows_are_project_windows() {
    assert!(is_project_window_label("main"));
    assert!(is_project_window_label("project-3"));
    assert!(!is_project_window_label("settings"));
}

#[test]
fn the_host_state_starts_empty_and_restores_nothing() {
    // PWN-FR-JFLE: a new run starts with no window in the list.
    let windows = ProjectWindows::default();
    assert!(windows.entries().is_empty());
    assert_eq!(windows.label_of("/p/a"), None);
    assert_eq!(windows.project_of("main"), None);
}

#[test]
fn each_new_window_gets_the_next_label() {
    let windows = ProjectWindows::default();
    assert_eq!(windows.next_label(), "project-1");
    assert_eq!(windows.next_label(), "project-2");
}

#[test]
fn the_host_finds_a_window_by_project_and_a_project_by_window() {
    let windows = ProjectWindows::default();
    windows.lock().open(entry("a"));
    assert_eq!(windows.label_of("a").as_deref(), Some("project-a"));
    assert_eq!(windows.project_of("project-a").as_deref(), Some("a"));
}

#[test]
fn a_notification_of_an_open_project_activates_its_tab() {
    use tauri::Manager;
    let app = tauri::test::mock_app();
    let windows = ProjectWindows::default();
    {
        let mut list = windows.lock();
        list.open(entry("a"));
        list.open(entry("b"));
        list.set_attention("a", true);
    }
    app.manage(windows);
    let label = activate_project_for_notification(app.handle(), "a");
    assert_eq!(label.as_deref(), Some("project-a"));
    let state = app.state::<ProjectWindows>();
    let list = state.lock();
    assert_eq!(list.active_project_id(), Some("a"));
    assert!(list.find("a").is_some_and(|e| !e.needs_attention));
}

#[test]
fn a_notification_of_no_open_project_activates_nothing() {
    use tauri::Manager;
    let app = tauri::test::mock_app();
    app.manage(ProjectWindows::default());
    assert_eq!(activate_project_for_notification(app.handle(), "a"), None);
    assert_eq!(activate_project_for_notification(app.handle(), ""), None);
    assert_eq!(window_label_of_project(app.handle(), "a"), None);
}

// PWN-FR-KQZB: the page of a created window marks it as a project window and
// names the project, escaped so a name cannot add a parameter.
#[test]
fn a_created_window_boots_as_a_project_window_that_names_its_project() {
    assert_eq!(project_window_url("acme"), "index.html?projectWindow=1&name=acme");
    assert_eq!(project_window_url("a&b c#d"), "index.html?projectWindow=1&name=a%26b%20c%23d");
}

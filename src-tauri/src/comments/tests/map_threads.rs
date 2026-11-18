//! The Map owner: the whole-map threads and the node threads (`CMS-comments-storage.md`
//! CMS-FR-RHXL, CMS-FR-SJTY, CMS-FR-ACLX, CMS-FR-GWBK, CMS-FR-QUMW, CMS-FR-OIUO;
//! `MPR-map-plan-proposals.md` MPR-FR-CEKS, MPR-FR-NRNZ, MPR-FR-MYLA,
//! MPR-FR-MAXH, MPR-FR-RVTT, MPR-FR-KZRN).

use tauri::Manager;

use super::*;
use crate::agent_conversations::authority_for_test::{is_author, register_turn};

fn node(kind: MapNodeKind, id: &str, label: &str) -> MapNodeRef {
    MapNodeRef { lens: MapLens::Delivery, kind, id: id.into(), label: label.into() }
}

fn opening(body: &str) -> Opening {
    Opening { body: body.into(), attachments: Vec::new(), spoken_original: None }
}

fn at(second: u32) -> String {
    format!("2026-03-01T10:00:{second:02}Z")
}

/// Write to the Map owner as a human at second `second`.
fn write(
    root: &crate::fs::RootFs,
    map_node: Option<MapNodeRef>,
    body: &str,
    second: u32,
) -> MapWrite {
    open_map_discussion_in(
        root,
        map_node,
        opening(body),
        Stamp { by: &human("raver119"), at: &at(second) },
    )
    .unwrap()
}

/// Reply in a Map thread as a human at second `second`.
fn reply(root: &crate::fs::RootFs, thread_id: &str, body: &str, second: u32) -> Discussion {
    add_comment_reporting_id(
        root,
        ThreadRef::map_discussion(),
        thread_id,
        body.into(),
        Vec::new(),
        Vec::new(),
        None,
        Stamp { by: &human("raver119"), at: &at(second) },
    )
    .unwrap()
    .0
}

/// Resolve a Map thread as a human at second `second`.
fn resolve(root: &crate::fs::RootFs, thread_id: &str, second: u32) {
    set_resolution_to(
        root,
        ThreadRef::map_discussion(),
        thread_id,
        true,
        &human("raver119"),
        &at(second),
    )
    .unwrap();
}

/// An app with a project open, the settings store, and a turn registry, so the
/// commands run as the window calls them.
struct Opened {
    app: tauri::App<tauri::test::MockRuntime>,
    _dir: tempfile::TempDir,
    root: crate::fs::RootFs,
}

impl Opened {
    fn new() -> Opened {
        let dir = temp_root();
        let path = crate::changes::canonicalize_lenient(dir.path());
        let root = crate::fs::RootFs::for_root(&path);
        let app = tauri::test::mock_app();
        app.manage(crate::project::ProjectState::default());
        app.manage(crate::global_settings::GlobalSettingsStore::in_memory());
        app.state::<crate::project::ProjectState>().set_root(path);
        register_turn(
            &app.handle().clone(),
            crate::agent_conversations::ConversationOrigin::stub_map("t"),
            None,
            None,
        );
        Opened { app, _dir: dir, root }
    }

    fn handle(&self) -> tauri::AppHandle<tauri::test::MockRuntime> {
        self.app.handle().clone()
    }

    fn open(&self, map_node: Option<MapNodeRef>, body: &str) -> Result<Discussion, String> {
        open_map_discussion(
            self.handle(),
            map_node,
            body.into(),
            Vec::new(),
            None,
            self.app.state(),
            self.app.state::<crate::project::ProjectState>().into(),
        )
    }

    fn find(&self, map_node: Option<MapNodeRef>) -> Result<Option<Discussion>, String> {
        find_map_discussion(
            self.handle(),
            map_node,
            self.app.state::<crate::project::ProjectState>().into(),
        )
    }
}

/// The log of the Map owner, as lines.
fn log_lines(root: &crate::fs::RootFs) -> usize {
    match root.read_text(map_discussion_log_path(root).unwrap()) {
        Ok(text) => text.lines().count(),
        Err(_) => 0,
    }
}

// MPR-FR-CEKS: a node reference is `{ lens, kind, id, label }`. The lens is
// `delivery` or `specifications`. The kind is `item`, `milestone`, `index`, or
// `spec`. Nothing else is a reference.
#[test]
fn a_node_reference_has_four_fields_and_known_words() {
    for (lens, kind) in [
        ("delivery", "item"),
        ("delivery", "milestone"),
        ("specifications", "index"),
        ("specifications", "spec"),
    ] {
        let wire =
            serde_json::json!({ "lens": lens, "kind": kind, "id": "issue-12", "label": "Relay" });
        let parsed: MapNodeRef = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!((parsed.lens.as_str(), parsed.kind.as_str()), (lens, kind));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), wire);
    }
    for bad in [
        serde_json::json!({ "lens": "plan", "kind": "item", "id": "a", "label": "A" }),
        serde_json::json!({ "lens": "delivery", "kind": "node", "id": "a", "label": "A" }),
        serde_json::json!({ "lens": "delivery", "kind": "item", "id": "a" }),
    ] {
        assert!(serde_json::from_value::<MapNodeRef>(bad).is_err());
    }
}

// MPR-FR-MYLA: a node reference matches on lens, kind, and id; the label takes no
// part in the match.
#[test]
fn a_node_reference_matches_on_lens_kind_and_id() {
    let a = node(MapNodeKind::Item, "issue-12", "Relay");
    assert!(a.names_same_node(&node(MapNodeKind::Item, "issue-12", "Another label")));
    assert!(!a.names_same_node(&node(MapNodeKind::Milestone, "issue-12", "Relay")));
    assert!(!a.names_same_node(&node(MapNodeKind::Item, "issue-13", "Relay")));
    let other_lens = MapNodeRef { lens: MapLens::Specifications, ..a.clone() };
    assert!(!a.names_same_node(&other_lens));
}

// MPR-FR-NRNZ, CMS-FR-SJTY, MPR-FR-MAXH: the Map holds any number of
// whole-map threads and any number of threads for one node reference. Each
// opening creates a new thread, and the reference of a thread never changes.
#[test]
fn each_opening_creates_a_new_thread() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let first = write(&root, None, "Plan the map", 1);
    let second = write(&root, None, "More on the map", 2);
    assert_ne!(first.discussion.id, second.discussion.id);
    assert_eq!(second.discussion.comments.len(), 1);
    assert_eq!(second.discussion.map_node, None);

    let relay = node(MapNodeKind::Item, "issue-12", "Relay");
    let relabelled = node(MapNodeKind::Item, "issue-12", "Relay, renamed");
    let one = write(&root, Some(relay.clone()), "About the relay", 3);
    let two = write(&root, Some(relabelled.clone()), "Still the relay", 4);
    assert_ne!(one.discussion.id, two.discussion.id);
    assert_eq!(one.discussion.map_node.as_ref(), Some(&relay));
    assert_eq!(two.discussion.map_node.as_ref(), Some(&relabelled));

    let threads = fold_map_discussions(&root);
    assert_eq!(threads.len(), 4);
    assert!(threads.iter().all(|t| t.comments.len() == 1), "no opening joined another thread");
}

// MPR-FR-NRNZ, CMS-FR-SJTY: a resolved thread limits no other. A new thread of
// the same node opens beside it, and the resolved one stays as it was.
#[test]
fn a_resolved_thread_blocks_no_new_thread() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let relay = node(MapNodeKind::Item, "issue-12", "Relay");
    let old = write(&root, Some(relay.clone()), "Old", 1).discussion;
    resolve(&root, &old.id, 2);
    let new = write(&root, Some(relay), "New", 3).discussion;
    assert_ne!(new.id, old.id);
    assert!(!new.resolved);
    let old_now = ThreadRef::map_discussion().find(&root, &old.id).unwrap();
    assert!(old_now.resolved);
    assert_eq!(old_now.comments.len(), 1);
}

// MPR-FR-NRNZ: every Map thread lists in the order it was opened, resolved
// ones included. A later reply does not move a thread.
#[test]
fn the_map_threads_list_in_opening_order() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let a = write(&root, Some(node(MapNodeKind::Item, "a", "A")), "A", 1).discussion.id;
    let b = write(&root, Some(node(MapNodeKind::Item, "b", "B")), "B", 2).discussion.id;
    let map = write(&root, None, "Map", 3).discussion.id;
    let c = write(&root, Some(node(MapNodeKind::Spec, "c", "C")), "C", 4).discussion.id;
    reply(&root, &a, "A again", 5);
    resolve(&root, &b, 6);
    let order: Vec<String> = list_map_discussions_in(&root).into_iter().map(|d| d.id).collect();
    assert_eq!(order, vec![a, b, map, c]);
}

// MPR-FR-MYLA, CMS-FR-QUMW: `find_map_discussion` returns the open thread of
// the reference, or the open whole-map thread where the reference is absent, or
// nothing. It creates nothing.
#[test]
fn find_returns_the_thread_and_creates_nothing() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let relay = node(MapNodeKind::Item, "issue-12", "Relay");
    assert!(find_map_discussion_in(&root, None).is_none());
    assert!(find_map_discussion_in(&root, Some(&relay)).is_none());
    assert_eq!(log_lines(&root), 0, "a find creates nothing");

    let map = write(&root, None, "Map", 1).discussion;
    let thread = write(&root, Some(relay.clone()), "Relay", 2).discussion;
    let lines = log_lines(&root);
    assert_eq!(find_map_discussion_in(&root, None).unwrap().id, map.id);
    let relabelled = MapNodeRef { label: "Renamed".into(), ..relay.clone() };
    assert_eq!(find_map_discussion_in(&root, Some(&relabelled)).unwrap().id, thread.id);
    assert!(find_map_discussion_in(&root, Some(&node(MapNodeKind::Item, "issue-13", "Other")))
        .is_none());
    assert_eq!(log_lines(&root), lines, "a find writes nothing");
}

// MPR-FR-MYLA: of several open threads of one node, the find returns the one
// with the latest `updated_at`, and it never returns a resolved thread.
#[test]
fn find_returns_the_latest_open_thread() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let relay = node(MapNodeKind::Item, "issue-12", "Relay");
    let older = write(&root, Some(relay.clone()), "Older", 1).discussion;
    let newer = write(&root, Some(relay.clone()), "Newer", 2).discussion;
    assert_eq!(find_map_discussion_in(&root, Some(&relay)).unwrap().id, newer.id);

    // A reply makes the older thread the latest one.
    reply(&root, &older.id, "Back to this one", 3);
    assert_eq!(find_map_discussion_in(&root, Some(&relay)).unwrap().id, older.id);

    // A resolved thread is passed over, however late its last write.
    resolve(&root, &older.id, 4);
    assert_eq!(find_map_discussion_in(&root, Some(&relay)).unwrap().id, newer.id);
    resolve(&root, &newer.id, 5);
    assert!(find_map_discussion_in(&root, Some(&relay)).is_none());

    // The same holds for the whole-map threads.
    let map = write(&root, None, "Map", 6).discussion;
    resolve(&root, &map.id, 7);
    assert!(find_map_discussion_in(&root, None).is_none());
}

// MPR-FR-MYLA: a whole-map find never returns a node thread, a node find never
// returns a whole-map thread, and two open threads with one `updated_at` give
// the larger id, whatever the order of the log.
#[test]
fn find_keeps_whole_map_and_node_threads_apart_and_breaks_a_tie_by_id() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let relay = node(MapNodeKind::Item, "issue-12", "Relay");
    write(&root, Some(relay.clone()), "Relay", 1);
    assert!(find_map_discussion_in(&root, None).is_none());

    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    write(&root, None, "Map", 1);
    assert!(find_map_discussion_in(&root, Some(&relay)).is_none());

    let a = write(&root, Some(relay.clone()), "A", 2).discussion.id;
    let b = write(&root, Some(relay.clone()), "B", 2).discussion.id;
    let larger = if a > b { a } else { b };
    assert_eq!(find_map_discussion_in(&root, Some(&relay)).unwrap().id, larger);
}

// MPR-FR-NRNZ: two threads opened in the same second list by id.
#[test]
fn threads_opened_together_list_by_id() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let a = write(&root, None, "A", 1).discussion.id;
    let b = write(&root, None, "B", 1).discussion.id;
    let mut expected = vec![a, b];
    expected.sort();
    let order: Vec<String> = list_map_discussions_in(&root).into_iter().map(|d| d.id).collect();
    assert_eq!(order, expected);
}

// MPR-FR-MAXH, CMS-FR-UYVK: the opening comment keeps its focus attachment and
// the spoken original the author said.
#[test]
fn an_opening_keeps_its_focus_and_its_spoken_original() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let relay = node(MapNodeKind::Item, "issue-12", "Relay");
    let created = open_map_discussion_in(
        &root,
        Some(relay.clone()),
        Opening {
            body: "Plan the relay".into(),
            attachments: vec![AttachmentInput::MapNode { node: relay.clone() }],
            spoken_original: Some("plan the relay".into()),
        },
        Stamp { by: &human("raver119"), at: &at(1) },
    )
    .unwrap();
    let comment = &created.discussion.comments[0];
    assert_eq!(comment.attachments, vec![Attachment::MapNode { node: relay }]);
    assert_eq!(comment.spoken_original.as_deref(), Some("plan the relay"));
}

// MPR-FR-MAXH: an opening creates the thread and its first comment as one step,
// and an empty opening creates nothing.
#[test]
fn an_opening_creates_the_thread_and_its_comment_in_one_step() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let relay = node(MapNodeKind::Item, "issue-12", "Relay");
    let blank = open_map_discussion_in(
        &root,
        Some(relay.clone()),
        opening("  "),
        Stamp { by: &human("raver119"), at: &at(1) },
    );
    assert_eq!(blank.err().as_deref(), Some(ERR_EMPTY_BODY));
    assert_eq!(log_lines(&root), 0, "nothing was written");

    let created = write(&root, Some(relay), "First", 2);
    assert_eq!(log_lines(&root), 2, "the opening and the comment are written together");
    assert_eq!(created.discussion.comments.len(), 1);
    assert_eq!(created.discussion.comments[0].id, created.comment_id);
    assert_eq!(created.discussion.comments[0].body, "First");
}

// MPR-FR-MAXH: callers that open together each get a whole thread of their own.
#[test]
fn racing_openings_each_create_one_thread() {
    let dir = temp_root();
    let path = dir.path().to_path_buf();
    let ids: Vec<String> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..6)
            .map(|n| {
                let path = path.clone();
                scope.spawn(move || {
                    let root = crate::fs::RootFs::for_root(&path);
                    let relay = node(MapNodeKind::Item, "issue-12", "Relay");
                    write(&root, Some(relay), &format!("opening {n}"), n).discussion.id
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let unique: std::collections::BTreeSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), 6, "{ids:?}");
    let root = crate::fs::RootFs::for_root(&path);
    let threads = fold_map_discussions(&root);
    assert_eq!(threads.len(), 6);
    assert!(threads.iter().all(|t| t.comments.len() == 1));
}

// MPR-FR-MAXH, CMS-FR-VRZA, AGC-FR-TFEN: `open_map_discussion` is a human write
// command: the comment it writes is an author comment, and each call opens a
// new thread.
#[test]
fn the_command_notes_the_author_comment() {
    let o = Opened::new();
    let relay = node(MapNodeKind::Item, "issue-12", "Relay");
    let created = o.open(Some(relay.clone()), "First").unwrap();
    assert!(is_author(&o.handle(), &created.comments[0].id));
    let another = o.open(Some(relay), "Second").unwrap();
    assert_ne!(another.id, created.id);
    assert!(is_author(&o.handle(), &another.comments[0].id));
    assert_eq!(o.find(None).unwrap(), None);
    // The two threads were opened on the real clock and can tie, so only the
    // node of the answer is checked here. The choice between them is MPR-FR-MYLA's.
    assert_eq!(o.find(created.map_node.clone()).unwrap().unwrap().map_node, created.map_node);
    assert_eq!(
        list_map_discussions(o.handle(), o.app.state::<crate::project::ProjectState>().into())
            .unwrap()
            .len(),
        2
    );
}

// MPR-FR-RVTT, MPR-FR-KZRN: each operation refuses with `no_project_open` when no
// project is open, and logs the refusal with its operation and its typed error
// and nothing of the node or the comment.
#[test]
fn each_operation_refuses_with_no_project_open_and_logs_it() {
    let app = tauri::test::mock_app();
    app.manage(crate::project::ProjectState::default());
    app.manage(crate::global_settings::GlobalSettingsStore::in_memory());
    let handle = app.handle().clone();
    let relay = node(MapNodeKind::Item, "issue-12", "RELAY-LABEL-SECRET");
    let refused = [
        find_map_discussion(
            handle.clone(),
            Some(relay.clone()),
            app.state::<crate::project::ProjectState>().into(),
        )
        .unwrap_err(),
        open_map_discussion(
            handle.clone(),
            Some(relay),
            "BODY-SECRET".into(),
            Vec::new(),
            None,
            app.state(),
            app.state::<crate::project::ProjectState>().into(),
        )
        .unwrap_err(),
        list_map_discussions(handle, app.state::<crate::project::ProjectState>().into())
            .unwrap_err(),
    ];
    assert_eq!(
        refused,
        [
            "no_project_open".to_string(),
            "no_project_open".to_string(),
            "no_project_open".to_string()
        ]
    );
    let page = crate::logging::BUFFER
        .query(
            &crate::logging::LogFilter {
                min_level: crate::logging::LogLevel::Debug,
                ..Default::default()
            },
            None,
            1000,
        )
        .unwrap();
    let text = serde_json::to_string(&page.records).unwrap();
    for operation in ["find", "write", "list"] {
        assert!(text.contains(&format!("\"operation\":\"{operation}\"")), "{operation}: {text}");
    }
    assert!(text.contains("map discussion operation refused") && text.contains("no_project_open"));
    assert!(!text.contains("SECRET"));
}

// CMS-FR-GWBK: `list_all_discussions` returns the Map threads with the
// discussions of the other owners, `list_discussions` for a `map` target returns
// them, and `read_discussion` reads one by its id.
#[test]
fn the_map_threads_are_listed_with_the_discussions_of_other_owners() {
    let o = Opened::new();
    let draft_id = make_draft(&o.root, "spec");
    let draft = open_discussion_in(
        &o.root,
        &o.root,
        &DiscussionTarget::Draft { draft_id },
        None,
        "About a draft".into(),
        Vec::new(),
        Stamp { by: &human("raver119"), at: &at(1) },
    )
    .unwrap();
    let map = o.open(None, "About the map").unwrap();
    let node_thread =
        o.open(Some(node(MapNodeKind::Spec, "CMS", "Comments")), "About CMS").unwrap();

    let all = list_all_discussions(o.app.state::<crate::project::ProjectState>().into()).unwrap();
    let ids: Vec<&str> = all.iter().map(|item| item.discussion.id.as_str()).collect();
    for id in [&draft.id, &map.id, &node_thread.id] {
        assert!(ids.contains(&id.as_str()), "{id} missing from {ids:?}");
    }
    let listed_map: Vec<&DiscussionListItem> =
        all.iter().filter(|i| i.discussion.is_map()).collect();
    assert_eq!(listed_map.len(), 2);
    assert!(listed_map.iter().all(|item| !item.owner_unavailable));
    let by_target = list_discussions(
        DiscussionTarget::Map,
        o.app.state::<crate::project::ProjectState>().into(),
    )
    .unwrap();
    assert_eq!(by_target.len(), 2);
    for id in [&map.id, &node_thread.id] {
        let read =
            read_discussion((*id).clone(), o.app.state::<crate::project::ProjectState>().into())
                .unwrap();
        assert_eq!(read.id, *id);
        assert_eq!(read.target, DiscussionTarget::Map);
    }
    assert_eq!(
        read_discussion(draft.id.clone(), o.app.state::<crate::project::ProjectState>().into())
            .unwrap()
            .id,
        draft.id
    );
}

// CMS-FR-ACLX: the log of the Map owner is `comments/map/discussion.jsonl` in
// the machine store and holds every Map thread. Its attachments live under
// `comments/map/attachments/` and its plan proposals under
// `comments/map/proposals/`.
#[test]
fn the_map_owner_has_one_log_and_two_folders() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    write(&root, None, "Map", 1);
    write(&root, Some(node(MapNodeKind::Item, "a", "A")), "A", 2);
    let base = dir.path().join(COMMENTS_REL).join("map");
    assert_eq!(map_discussion_log_path(&root).unwrap(), base.join("discussion.jsonl"));
    assert_eq!(map_proposals_dir(&root).unwrap(), base.join("proposals"));
    assert_eq!(log_lines(&root), 4, "two threads, two openings, in one log");
}

// CMS-FR-QUMW, CMS-FR-RHXL: `open_discussion` refuses a `map` target with
// `not_supported` and appends nothing, and a fragment target of the Map is
// `invalid_fragment`.
#[test]
fn the_map_is_opened_only_by_its_own_command() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let refused = open_discussion_in(
        &root,
        &root,
        &DiscussionTarget::Map,
        None,
        "Opening".into(),
        Vec::new(),
        Stamp { by: &human("raver119"), at: &at(1) },
    );
    assert_eq!(refused.err().as_deref(), Some(ERR_NOT_SUPPORTED));
    assert_eq!(log_lines(&root), 0);
    let fragment = FragmentTarget {
        owner: DiscussionTarget::Map,
        path: String::new(),
        start: 0,
        end: 1,
        quote: "x".into(),
    };
    let invalid = super::super::opening::validate_fragment(&DiscussionTarget::Map, &fragment);
    assert_eq!(invalid.err().as_deref(), Some(ERR_INVALID_FRAGMENT));
}

// CMS-FR-OIUO, MPR-FR-PMLX: a `map_node` attachment is stored and folded as
// appended and validates nothing about the node it names. A caller may supply it,
// and the `map_plan_proposal` kind is out of reach of a caller.
#[test]
fn a_map_node_attachment_is_stored_as_appended() {
    let dir = temp_root();
    let root = crate::fs::RootFs::for_root(dir.path());
    let ghost = node(MapNodeKind::Item, "issue-9999", "A node that does not exist");
    let created = open_map_discussion_in(
        &root,
        None,
        Opening {
            body: "About a ghost".into(),
            attachments: vec![AttachmentInput::MapNode { node: ghost.clone() }],
            spoken_original: None,
        },
        Stamp { by: &human("raver119"), at: &at(1) },
    )
    .unwrap();
    let stored = &created.discussion.comments[0].attachments;
    assert_eq!(stored, &vec![Attachment::MapNode { node: ghost.clone() }]);
    let wire = serde_json::to_value(&stored[0]).unwrap();
    assert_eq!(wire["kind"], "mapNode");
    assert_eq!(wire["node"]["id"], "issue-9999");
    let proposal = serde_json::to_value(Attachment::MapPlanProposal {
        proposal_id: "p1".into(),
        update: None,
    })
    .unwrap();
    assert_eq!(proposal["kind"], "mapPlanProposal");
    assert_eq!(proposal["proposalId"], "p1");
    let input = serde_json::json!({ "kind": "mapPlanProposal", "proposalId": "p1" });
    assert!(serde_json::from_value::<AttachmentInput>(input).is_err());
}

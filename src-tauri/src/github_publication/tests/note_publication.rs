//! Note publication end to end: the availability read, the publish command
//! against a fake GitHub, the deletion of the note, and the open check
//! (GHP-FR-CNPX through GHP-FR-MAMT, NTC-FR-VPJZ).

use tauri::{Listener, Manager};

use super::metadata_refresh::select_github_project;
use super::publication_commands::{session, Session};
use super::*;
use crate::github_publication::note::{self, NotePublicationChoiceInput, NotePublicationOutcome};

/// A session with a GitHub Project selected, the note registries managed, and
/// one project-wide note.
fn note_session() -> (Session, String) {
    let s = session();
    s.app.manage(crate::agent_conversations::TurnRegistry::default());
    s.app.manage(crate::progress::ProgressRegistry::default());
    select_github_project(&s, Some("PVT_1"));
    let id = crate::notes::create_note_in(
        &s.root(),
        crate::notes::NoteScope::Project,
        "Check the FR numbering.".into(),
        Some("2030-01-01T09:00:00Z".into()),
        None,
        "2025-03-04T10:00:00Z",
    )
    .unwrap()
    .id;
    (s, id)
}

fn publish(
    s: &Session,
    id: &str,
    title: &str,
    choice: NotePublicationChoiceInput,
) -> Result<NotePublicationOutcome, String> {
    note::publish_note_to_github_impl(
        &s.handle(),
        id.to_string(),
        title.to_string(),
        "The **body** of the issue.".to_string(),
        choice,
    )
}

fn root_choice() -> NotePublicationChoiceInput {
    NotePublicationChoiceInput::default()
}

fn creates(s: &Session) -> usize {
    s.fake.calls().iter().filter(|c| matches!(c, Call::Create(_))).count()
}

fn note_exists(s: &Session, id: &str) -> bool {
    crate::notes::note_exists(&s.root(), id)
}

// GHP-FR-CNPX / GHP-FR-HGVL: with a GitHub Project selected and an eligible
// remote, a note can be published, and the answer names the repository.
#[test]
fn availability_names_the_publication_repository() {
    let (s, _) = note_session();
    let answer = note::get_note_publication_availability_impl(&s.handle()).unwrap();
    assert!(answer.available);
    assert_eq!(answer.remote_name.as_deref(), Some("origin"));
    assert_eq!(answer.repository_owner.as_deref(), Some("acme"));
    assert_eq!(answer.repository_name.as_deref(), Some("widgets"));
    assert_eq!(answer.reason_code, None);
    assert!(s.fake.calls().iter().all(|c| !matches!(c, Call::Create(_))));
}

// GHP-FR-CNPX: with no GitHub Project selected, the answer is the typed
// refusal with its displayable text, and GitHub is not read.
#[test]
fn availability_without_a_github_project_is_refused() {
    let (s, _) = note_session();
    select_github_project(&s, None);
    let answer = note::get_note_publication_availability_impl(&s.handle()).unwrap();
    assert!(!answer.available);
    assert_eq!(answer.reason_code.as_deref(), Some(ERR_GITHUB_PROJECT_NOT_SELECTED));
    assert!(answer.reason.unwrap().contains("GitHub Project"));
    assert_eq!(answer.remote_name, None);
    assert!(s.fake.calls().is_empty());
}

// GHP-FR-CNPX / GHP-FR-ZRFP: a remote that does not resolve gives its own
// refusal and text.
#[test]
fn availability_names_the_remote_refusal() {
    let (s, _) = note_session();
    *s.fake.probe.lock().unwrap() = ProbeOutcome::IssuesDisabled;
    let answer = note::get_note_publication_availability_impl(&s.handle()).unwrap();
    assert!(!answer.available);
    assert_eq!(answer.reason_code.as_deref(), Some(ERR_ISSUES_DISABLED));
    assert!(answer.reason.is_some());
}

// GHP-FR-JGNI / GHP-FR-WJTX / GHP-FR-WJBM / GHP-FR-HKMC / GHP-FR-GCLP /
// NTC-FR-VPJZ: a root publication creates one issue with the trimmed title and
// the body as sent, deletes the note with its reminder, and writes no
// publication store.
#[test]
fn a_root_publication_creates_one_issue_and_deletes_the_note() {
    let (s, id) = note_session();
    let drafts_changed = record(&s, DRAFT_PUBLICATION_CHANGED);
    let outcome = publish(&s, &id, "  Check numbering  ", root_choice()).unwrap();
    let NotePublicationOutcome::Published { issue_number, issue_url } = outcome else {
        panic!("expected published, got {outcome:?}");
    };
    let issue = s.fake.issue(issue_number);
    assert_eq!(issue.title, "Check numbering");
    assert_eq!(issue.body, "The **body** of the issue.");
    assert!(!issue.body.contains("synthesis-publication-marker"));
    assert_eq!(issue_url, issue.url);
    assert_eq!(creates(&s), 1);
    assert!(!note_exists(&s, &id), "the note and its reminder are gone");
    assert!(crate::notes::reminder_notes(&s.root()).is_empty());
    assert!(drafts_changed.lock().unwrap().is_empty());
}

fn record(s: &Session, name: &'static str) -> std::sync::Arc<std::sync::Mutex<Vec<String>>> {
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = seen.clone();
    s.handle().listen_any(name, move |e| sink.lock().unwrap().push(e.payload().to_string()));
    seen
}

// NTC-FR-VPJZ / GHP-FR-WJBM: the note's discussion goes with it.
#[test]
fn a_publication_deletes_the_discussion_of_the_note() {
    let (s, id) = note_session();
    let root = s.root();
    let machine = s.machine();
    let by =
        crate::comments::Participant::Human { login: "me".into(), display_name: None, email: None };
    crate::comments::get_or_create_note_discussion_in(
        &machine,
        &root,
        &id,
        "why is this open?".into(),
        Vec::new(),
        &by,
        "2025-03-04T11:00:00Z",
    )
    .unwrap();
    assert!(crate::comments::note_discussion_index(&machine).contains_key(&id));
    let outcome = publish(&s, &id, "Title", root_choice()).unwrap();
    assert!(matches!(outcome, NotePublicationOutcome::Published { .. }));
    assert!(!crate::comments::note_discussion_index(&machine).contains_key(&id));
    assert!(!note_exists(&s, &id));
}

// GHP-FR-HKMC: an empty title, a title over 256 characters, and a body over
// 65,536 characters are refused before any request, and the note stays.
#[test]
fn a_title_or_a_body_out_of_bounds_is_refused() {
    let (s, id) = note_session();
    for title in ["", "   ", &"é".repeat(257)] {
        assert_eq!(
            publish(&s, &id, title, root_choice()),
            Err(ERR_INVALID_NOTE_PUBLICATION.to_string())
        );
    }
    assert_eq!(note::validated_content(&"é".repeat(256), "").as_deref(), Ok(&*"é".repeat(256)));
    assert_eq!(
        note::validated_content("t", &"x".repeat(65_537)),
        Err(ERR_INVALID_NOTE_PUBLICATION.to_string())
    );
    assert!(note::validated_content("t", &"x".repeat(65_536)).is_ok());
    assert_eq!(creates(&s), 0);
    assert!(note_exists(&s, &id));
}

// GHP-FR-JGNI: an id that names no note is refused, and nothing is created.
#[test]
fn an_unknown_note_is_refused() {
    let (s, _) = note_session();
    assert_eq!(
        publish(&s, "no-such-note", "Title", root_choice()),
        Err(ERR_NOTE_NOT_FOUND.to_string())
    );
    assert_eq!(creates(&s), 0);
}

// GHP-FR-JGNI / GHP-FR-HGVL: with no GitHub Project selected, nothing is
// created and the note stays.
#[test]
fn publication_without_a_github_project_is_refused() {
    let (s, id) = note_session();
    select_github_project(&s, None);
    assert_eq!(
        publish(&s, &id, "Title", root_choice()),
        Err(ERR_GITHUB_PROJECT_NOT_SELECTED.to_string())
    );
    assert_eq!(creates(&s), 0);
    assert!(note_exists(&s, &id));
}

// GHP-FR-VLDP / GHP-FR-WJTX: the Type, the milestone, and the labels go in the
// one create request, each in GitHub's own spelling.
#[test]
fn the_type_milestone_and_labels_reach_the_create_request() {
    let (s, id) = note_session();
    let choice = NotePublicationChoiceInput {
        issue_type: Some("bug".into()),
        milestone_number: Some(8),
        labels: vec!["UI".into(), "bug".into(), "ui".into()],
        ..Default::default()
    };
    publish(&s, &id, "Title", choice).unwrap();
    let fields = s.fake.fields.lock().unwrap().clone();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].issue_type.as_deref(), Some("Bug"));
    assert_eq!(fields[0].milestone, Some(8));
    assert_eq!(fields[0].labels, vec!["ui".to_string(), "bug".to_string()]);
}

// GHP-FR-VLDP: a Type, a milestone, or a label that GitHub does not offer is
// refused before the create, and the note stays.
#[test]
fn an_unavailable_type_milestone_or_label_is_refused() {
    let (s, id) = note_session();
    let cases = [
        (
            NotePublicationChoiceInput { issue_type: Some("Epic".into()), ..Default::default() },
            ERR_ISSUE_TYPE_UNAVAILABLE,
        ),
        (
            NotePublicationChoiceInput { milestone_number: Some(99), ..Default::default() },
            ERR_MILESTONE_UNAVAILABLE,
        ),
        (
            NotePublicationChoiceInput { labels: vec!["nope".into()], ..Default::default() },
            ERR_LABEL_UNAVAILABLE,
        ),
    ];
    for (choice, code) in cases {
        assert_eq!(publish(&s, &id, "Title", choice), Err(code.to_string()));
    }
    *s.fake.labels.lock().unwrap() = None;
    assert_eq!(
        publish(
            &s,
            &id,
            "Title",
            NotePublicationChoiceInput { labels: vec!["bug".into()], ..Default::default() }
        ),
        Err(ERR_LABEL_UNAVAILABLE.to_string())
    );
    assert_eq!(creates(&s), 0);
    assert!(note_exists(&s, &id));
}

// GHP-FR-WXJD / GHP-FR-WJTX: a sub-issue carries the Type and the milestone of
// the choice, not the sub-issue settings, and is linked to its parent.
#[test]
fn a_sub_issue_takes_the_choice_and_is_linked() {
    let (s, id) = note_session();
    let parent = s.fake.seed("Feature", Some((7, "v1.2")));
    let choice = NotePublicationChoiceInput {
        parent_issue_number: Some(parent),
        issue_type: Some("Feature".into()),
        milestone_number: None,
        labels: vec![],
    };
    let outcome = publish(&s, &id, "Title", choice).unwrap();
    let NotePublicationOutcome::Published { issue_number, .. } = outcome else {
        panic!("expected published, got {outcome:?}");
    };
    let issue = s.fake.issue(issue_number);
    assert_eq!(issue.issue_type.as_deref(), Some("Feature"));
    assert_eq!(issue.milestone, None, "the parent's milestone is not inherited");
    assert!(issue.parent.unwrap().names("acme", "widgets", parent));
    assert!(!note_exists(&s, &id));
}

// GHP-FR-WXJD: a parent that is closed or not of a parent Type is refused
// before the create.
#[test]
fn an_unusable_parent_is_refused() {
    let (s, id) = note_session();
    let task = s.fake.seed("Task", None);
    let closed = s.fake.seed("Feature", None);
    s.fake.edit(closed, |i| i.open = false);
    for parent in [task, closed, 999] {
        let choice =
            NotePublicationChoiceInput { parent_issue_number: Some(parent), ..Default::default() };
        assert_eq!(
            publish(&s, &id, "Title", choice),
            Err(ERR_PARENT_ISSUE_UNAVAILABLE.to_string())
        );
    }
    assert_eq!(creates(&s), 0);
    assert!(note_exists(&s, &id));
}

// GHP-FR-BYZQ: a create that GitHub refuses returns its typed error, and the
// note stays.
#[test]
fn a_failed_create_keeps_the_note() {
    let (s, id) = note_session();
    *s.fake.create_error.lock().unwrap() = Some(ERR_ISSUE_CREATE_FAILED.into());
    assert_eq!(publish(&s, &id, "Title", root_choice()), Err(ERR_ISSUE_CREATE_FAILED.to_string()));
    assert!(note_exists(&s, &id));
}

// GHP-FR-QWFD: a link that fails answers with the created issue, and the note
// stays.
#[test]
fn a_failed_link_answers_with_the_issue_and_keeps_the_note() {
    let (s, id) = note_session();
    let parent = s.fake.seed("Feature", None);
    *s.fake.link_error.lock().unwrap() = Some(ERR_SUB_ISSUE_LINK_FAILED.into());
    let choice =
        NotePublicationChoiceInput { parent_issue_number: Some(parent), ..Default::default() };
    let outcome = publish(&s, &id, "Title", choice).unwrap();
    let NotePublicationOutcome::SubIssueLinkFailed { issue_number, issue_url } = outcome else {
        panic!("expected sub_issue_link_failed, got {outcome:?}");
    };
    assert_eq!(s.fake.issue(issue_number).url, issue_url);
    assert!(note_exists(&s, &id));
}

// GHP-FR-GIVY: a deletion that fails after the create answers
// `note_not_removed` with the issue and the deletion's code.
#[test]
fn a_failed_deletion_answers_note_not_removed() {
    let fake = FakeGithub::publishable();
    let fields = client::IssueFields::default();
    let outcome =
        note::publish_with(&fake, "s", "acme", "widgets", "T", "B", &fields, None, || {
            Err(crate::notes::ERR_DISCUSSION_CLEANUP_FAILED.to_string())
        })
        .unwrap();
    let NotePublicationOutcome::NoteNotRemoved { issue_number, error_code, .. } = outcome else {
        panic!("expected note_not_removed, got {outcome:?}");
    };
    assert_eq!(issue_number, 1);
    assert_eq!(error_code, crate::notes::ERR_DISCUSSION_CLEANUP_FAILED);
    let other = note::publish_with(&fake, "s", "acme", "widgets", "T", "B", &fields, None, || {
        Err("disk full at /Users/me/project".to_string())
    })
    .unwrap();
    assert!(matches!(
        other,
        NotePublicationOutcome::NoteNotRemoved { ref error_code, .. }
            if error_code == note::ERR_NOTE_DELETE_FAILED
    ));
}

// GHP-FR-GCLP: a second call for the same text is a new publication.
#[test]
fn each_call_is_a_new_publication() {
    let fake = FakeGithub::publishable();
    let fields = client::IssueFields::default();
    for _ in 0..2 {
        note::publish_with(&fake, "s", "acme", "widgets", "T", "B", &fields, None, || Ok(()))
            .unwrap();
    }
    assert_eq!(fake.calls().iter().filter(|c| matches!(c, Call::Create(_))).count(), 2);
    assert!(fake.calls().iter().all(|c| !matches!(c, Call::Search(_))));
}

// GHP-FR-MAMB: only an HTTPS issue address of the publication repository
// passes the check.
#[test]
fn only_an_issue_address_of_the_repository_opens() {
    let ok = |url: &str| note::is_repository_issue_url(url, "github.com", "acme", "widgets");
    assert!(ok("https://github.com/acme/widgets/issues/12"));
    assert!(ok("https://GitHub.com/Acme/Widgets/issues/12"));
    for bad in [
        "http://github.com/acme/widgets/issues/12",
        "https://github.com/acme/other/issues/12",
        "https://github.com/acme/widgets/pull/12",
        "https://github.com/acme/widgets/issues/12x",
        "https://github.com/acme/widgets/issues/",
        "https://evil.com/acme/widgets/issues/12",
        "https://user@github.com/acme/widgets/issues/12",
        "https://github.com/acme/widgets/issues/12?x=1",
        "https://github.com/acme/widgets/issues/12/extra",
        "file:///etc/passwd",
    ] {
        assert!(!ok(bad), "{bad} must be refused");
    }
}

// GHP-FR-MAMB: the command check resolves the publication repository and
// needs no draft.
#[test]
fn the_open_check_resolves_the_repository_without_a_draft() {
    let (s, _) = note_session();
    assert_eq!(
        note::require_repository_issue_url(&s.handle(), "https://github.com/acme/widgets/issues/3"),
        Ok(())
    );
    assert_eq!(
        note::require_repository_issue_url(&s.handle(), "https://github.com/acme/other/issues/3"),
        Err(ERR_ISSUE_URL_REFUSED.to_string())
    );
}

// GHP-FR-MAMT: no log line of a note publication carries the title or the
// body.
#[test]
fn the_log_carries_no_note_text() {
    let (s, id) = note_session();
    publish(&s, &id, "SECRET-TITLE-TEXT", root_choice()).unwrap();
    let filter: crate::logging::LogFilter = serde_json::from_value(serde_json::json!({})).unwrap();
    let records = crate::logging::BUFFER.query(&filter, None, 5000).expect("a page").records;
    assert!(records.iter().any(|r| r.message == "note published to GitHub"
        && r.fields.get("note_id").and_then(|v| v.as_str()) == Some(id.as_str())));
    let joined = format!("{records:?}");
    assert!(!joined.contains("SECRET-TITLE-TEXT"));
    assert!(!joined.contains("of the issue"));
}

// GHP-FR-XLXU / GHP-FR-MDLD / GHP-FR-DZLB: the metadata read holds the label
// list, and a label read that fails leaves the other lists as they are.
#[test]
fn the_metadata_read_holds_the_label_list() {
    let fake = FakeGithub::publishable();
    let settings = GithubPublicationSettings::default();
    let metadata = flow::load_metadata(&fake, "s", "acme", "widgets", settings.clone());
    assert_eq!(metadata.labels.state, MetadataState::Loaded);
    let names: Vec<&str> = metadata.labels.items.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names, ["bug", "ui"]);
    assert_eq!(metadata.labels.items[1].description.as_deref(), Some("User interface"));

    *fake.labels.lock().unwrap() = None;
    let failed = flow::load_metadata(&fake, "s", "acme", "widgets", settings);
    assert_eq!(failed.labels.state, MetadataState::Failed);
    assert_eq!(failed.labels.error_code.as_deref(), Some(ERR_LABELS_UNREADABLE));
    assert!(failed.labels.error.is_some());
    assert_eq!(failed.milestones.state, MetadataState::Loaded);
    assert_eq!(failed.issue_types.state, MetadataState::Loaded);
}

// GHP-FR-JGXW: the note issue Type resolves to the spelling of the Type list,
// to a null spelling where the list does not hold it, and to none for No Type.
#[test]
fn the_note_issue_type_resolves_against_the_type_list() {
    let types = MetadataList::loaded(vec![
        PublicationIssueType { name: "Feature".into() },
        PublicationIssueType { name: "Bug".into() },
    ]);
    let with = |name: Option<&str>| GithubPublicationSettings {
        note_issue_type: name.map(str::to_string),
        ..Default::default()
    };
    assert_eq!(
        flow::resolve_note_issue_type(&GithubPublicationSettings::default(), &types),
        Some(IssueTypeResolution { name: "Feature".into(), resolved: Some("Feature".into()) })
    );
    assert_eq!(
        flow::resolve_note_issue_type(&with(Some("bug")), &types),
        Some(IssueTypeResolution { name: "bug".into(), resolved: Some("Bug".into()) })
    );
    assert_eq!(
        flow::resolve_note_issue_type(&with(Some("Epic")), &types),
        Some(IssueTypeResolution { name: "Epic".into(), resolved: None })
    );
    assert_eq!(flow::resolve_note_issue_type(&with(None), &types), None);
}

// GHP-FR-KVRH / GHP-FR-NQWX: the note issue Type defaults to `Feature`, a
// null one is No Type, and a blank one is refused.
#[test]
fn the_note_issue_type_setting_is_validated() {
    assert_eq!(GithubPublicationSettings::default().note_issue_type.as_deref(), Some("Feature"));
    let saved = |note: Option<String>| {
        flow::validated_settings(
            vec!["Feature".into()],
            "Task".into(),
            MilestonePolicy::default(),
            None,
            note,
        )
    };
    assert_eq!(saved(Some(" Bug ".into())).unwrap().note_issue_type.as_deref(), Some("Bug"));
    assert_eq!(saved(None).unwrap().note_issue_type, None);
    assert_eq!(saved(Some("  ".into())), Err(ERR_INVALID_PUBLICATION_SETTINGS.to_string()));
}

// GHP-FR-NQWX: the settings command persists the note issue Type, and No Type
// reads back as No Type.
#[test]
fn the_settings_command_persists_the_note_issue_type() {
    let (s, _) = note_session();
    let write = |note: Option<&str>| {
        crate::github_publication::set_github_publication_settings_impl(
            &s.handle(),
            vec!["Feature".into()],
            "Task".into(),
            MilestonePolicy::default(),
            None,
            note.map(str::to_string),
        )
        .unwrap()
    };
    assert_eq!(write(Some("Bug")).note_issue_type.as_deref(), Some("Bug"));
    let read = crate::project_settings::load_github_publication_settings_from(&s.root()).unwrap();
    assert_eq!(read.note_issue_type.as_deref(), Some("Bug"));
    write(None);
    let read = crate::project_settings::load_github_publication_settings_from(&s.root()).unwrap();
    assert_eq!(read.note_issue_type, None);
}

// GHP-FR-IQTK / GHP-FR-EYGR / GHP-FR-AWEZ: the label list is a list of the
// cache. A loaded one replaces the cached one, and a failed one keeps it.
#[test]
fn the_cache_merges_the_label_list() {
    use crate::github_publication::metadata_cache::{merge, CachedLists};
    let label = |name: &str| PublicationLabel {
        name: name.into(),
        color: "ffffff".into(),
        description: None,
    };
    let lists = |labels: MetadataList<PublicationLabel>| CachedLists {
        remote_name: "origin".into(),
        repository_host: "github.com".into(),
        repository_owner: "acme".into(),
        repository_name: "widgets".into(),
        parents: MetadataList::loaded(vec![]),
        issue_types: MetadataList::loaded(vec![]),
        milestones: MetadataList::loaded(vec![]),
        labels,
    };
    let cached = lists(MetadataList::loaded(vec![label("bug")]));
    let fresh = merge(Some(&cached), lists(MetadataList::loaded(vec![label("ui")])));
    assert_eq!(fresh.labels.items, vec![label("ui")]);
    let failed = lists(MetadataList::failed(ERR_LABELS_UNREADABLE));
    assert_eq!(failed.first_failure().as_deref(), Some(ERR_LABELS_UNREADABLE));
    let kept = merge(Some(&cached), failed.clone());
    assert_eq!(kept.labels, cached.labels);
    assert_eq!(merge(None, failed).labels.state, MetadataState::Failed);
}

// GHP-FR-AWEZ / GHP-FR-JTYL: a refresh reads the label list into the cache,
// and the cached view carries it with the resolved note issue Type.
#[test]
fn a_refresh_reads_the_labels_into_the_cache() {
    let (s, _) = note_session();
    s.app.manage(crate::github_publication::metadata_refresh::PublicationMetadataCache::default());
    super::metadata_refresh::refresh(
        &s,
        crate::github_publication::metadata_cache::RefreshTrigger::Request,
    );
    assert!(s.fake.calls().contains(&Call::ListLabels));
    let cached =
        crate::github_publication::metadata_refresh::cached_metadata_for(&s.handle(), "origin")
            .expect("the cache holds lists");
    assert_eq!(cached.labels.items.len(), 2);
    assert_eq!(cached.note_issue_type.and_then(|t| t.resolved).as_deref(), Some("Feature"));
}

// GHP-FR-XLXU / GHP-FR-WJTX: a label row reads from GitHub's JSON, and the
// create request names labels only where the choice holds some.
#[test]
fn labels_read_from_json_and_reach_the_request() {
    let row = serde_json::json!({ "name": "bug", "color": "d73a4a", "description": "" });
    assert_eq!(
        client::label_from_json(&row),
        Some(PublicationLabel { name: "bug".into(), color: "d73a4a".into(), description: None })
    );
    assert_eq!(client::label_from_json(&serde_json::json!({ "color": "x" })), None);
    let plain = client::issue_payload("T", "B", &client::IssueFields::default());
    assert!(plain.get("labels").is_none());
    let labelled = client::issue_payload(
        "T",
        "B",
        &client::IssueFields { labels: vec!["bug".into()], ..Default::default() },
    );
    assert_eq!(labelled["labels"], serde_json::json!(["bug"]));
}

// GHP-FR-JGNI: the outcome serializes as the frontend reads it.
#[test]
fn the_outcome_serializes_as_the_frontend_reads_it() {
    let json = serde_json::to_value(NotePublicationOutcome::NoteNotRemoved {
        issue_number: 4,
        issue_url: "u".into(),
        error_code: "c".into(),
    })
    .unwrap();
    assert_eq!(
        json,
        serde_json::json!({ "kind": "note_not_removed", "issueNumber": 4, "issueUrl": "u", "errorCode": "c" })
    );
}

//! Root and sub-issue publication, the milestone policies, the saved choice, and
//! the retry and recovery comparison (GHP-FR-MDLD, GHP-FR-DZLB, GHP-FR-FTMC,
//! GHP-FR-BWNI, GHP-FR-LCKZ, GHP-FR-RMVQ, GHP-FR-ETJD, GHP-FR-AZPF,
//! GHP-FR-ATCH, GHP-FR-OHGY, GHP-FR-SWKU, GHP-FR-IBXN, GHP-FR-PWJG,
//! GHP-FR-CMPR, GHP-FR-RCNL, GHP-FR-HSCH, GHP-FR-PLQE, GHP-FR-CDVT,
//! GHP-FR-UXOT, GHP-FR-KVRH, GHP-FR-NQWX).

use super::*;
use crate::project_settings::{
    load_github_publication_settings_from, save_github_publication_settings_to,
};

struct Fixture {
    _dir: TempDir,
    root: crate::fs::RootFs,
    id: String,
    github: FakeGithub,
}

fn fixture() -> Fixture {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "Widget window");
    save_prompt(&root, &id, "Do the thing.\n");
    Fixture { _dir: dir, root, id, github: FakeGithub::publishable() }
}

fn settings(policy: MilestonePolicy) -> GithubPublicationSettings {
    GithubPublicationSettings { sub_issue_milestone_policy: policy, ..Default::default() }
}

fn sub_of(parent: u64) -> PublicationChoiceInput {
    PublicationChoiceInput { parent_issue_number: Some(parent), ..Default::default() }
}

impl Fixture {
    fn resolve(
        &self,
        input: &PublicationChoiceInput,
        settings: &GithubPublicationSettings,
    ) -> Result<PublicationChoice, String> {
        flow::resolve_choice(input, settings, &self.github, "secret", "acme", "widgets")
    }

    fn attempt_with(&self, marker: &str, choice: PublicationChoice) -> PublicationAttempt {
        flow::open_attempt_with(&self.root, &self.id, &origin(), marker.into(), Some(choice))
            .unwrap()
    }

    fn publish(&self, attempt: &PublicationAttempt) -> Result<PublicationOutcome, String> {
        flow::publish_with_attempt(&self.root, &self.id, attempt, "secret", &self.github)
    }

    fn store(&self) -> PublicationStore {
        store::read_store(&self.root, &self.id).unwrap()
    }
}

fn published(outcome: Result<PublicationOutcome, String>) -> PublicationRecord {
    match outcome.unwrap() {
        PublicationOutcome::Published { record } => record,
        other => panic!("expected a publication, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Root publication
// ---------------------------------------------------------------------------

/// GHP-FR-PLQE / GHP-FR-ATCH / GHP-FR-HSCH: a root issue with no Type and no
/// milestone sends the title and the body alone, and its record says it is a
/// root issue.
#[test]
fn a_plain_root_publication_sends_no_metadata() {
    let f = fixture();
    let choice = f.resolve(&PublicationChoiceInput::default(), &settings(MilestonePolicy::default())).unwrap();
    assert_eq!(choice, PublicationChoice::root());
    let attempt = f.attempt_with("pub-1", choice);

    let record = published(f.publish(&attempt));
    assert_eq!(f.github.fields.lock().unwrap().as_slice(), &[client::IssueFields::default()]);
    assert_eq!(client::issue_payload("t", "b", &client::IssueFields::default()),
        serde_json::json!({ "title": "t", "body": "b" }));
    assert_eq!(record.choice.unwrap().kind, PublicationKind::Root);
    assert!(!f.github.calls().iter().any(|c| matches!(c, Call::Link(..) | Call::Read(_))));
}

/// GHP-FR-BWNI / GHP-FR-LCKZ / GHP-FR-OHGY: a root issue carries the selected
/// Type and the selected open milestone.
#[test]
fn a_root_publication_carries_the_selected_type_and_milestone() {
    let f = fixture();
    let input = PublicationChoiceInput {
        parent_issue_number: None,
        issue_type: Some("bug".into()),
        milestone_number: Some(8),
    };
    let choice = f.resolve(&input, &settings(MilestonePolicy::default())).unwrap();
    assert_eq!(choice.kind, PublicationKind::Root);
    assert_eq!(choice.issue_type.as_deref(), Some("Bug"), "GitHub's own spelling");
    assert_eq!(choice.milestone_number, Some(8));
    assert_eq!(choice.milestone_title.as_deref(), Some("v1.3"));

    let attempt = f.attempt_with("pub-1", choice);
    let record = published(f.publish(&attempt));
    assert_eq!(
        f.github.fields.lock().unwrap().as_slice(),
        &[client::IssueFields { issue_type: Some("Bug".into()), milestone: Some(8) }]
    );
    let issue = f.github.issue(record.issue_number);
    assert_eq!(issue.issue_type.as_deref(), Some("Bug"));
    assert_eq!(issue.milestone.unwrap().title, "v1.3");
}

/// GHP-FR-LCKZ: a Type GitHub does not list, a milestone that is not open, and
/// a Type list that cannot be read each refuse a root choice, and nothing is
/// written.
#[test]
fn a_root_choice_naming_unavailable_metadata_is_refused() {
    let f = fixture();
    let by_type = PublicationChoiceInput { issue_type: Some("Epic".into()), ..Default::default() };
    assert_eq!(f.resolve(&by_type, &settings(MilestonePolicy::default())), Err(ERR_ISSUE_TYPE_UNAVAILABLE.into()));
    let by_milestone = PublicationChoiceInput { milestone_number: Some(99), ..Default::default() };
    assert_eq!(f.resolve(&by_milestone, &settings(MilestonePolicy::default())), Err(ERR_MILESTONE_UNAVAILABLE.into()));

    *f.github.types.lock().unwrap() = None;
    let named = PublicationChoiceInput { issue_type: Some("Bug".into()), ..Default::default() };
    assert_eq!(f.resolve(&named, &settings(MilestonePolicy::default())), Err(ERR_ISSUE_TYPE_UNAVAILABLE.into()));
    assert!(f.github.calls().iter().all(|c| !matches!(c, Call::Create(_))));
    let path = crate::drafts::draft_publication_path(&f.root, &f.id).unwrap();
    assert!(!path.exists(), "a refused choice writes no attempt record");
}

/// GHP-FR-UXOT: with every metadata read failing, an unadorned root choice
/// still resolves, so root publication is never blocked by a failed read.
#[test]
fn a_root_choice_resolves_while_every_metadata_read_fails() {
    let f = fixture();
    *f.github.types.lock().unwrap() = None;
    *f.github.milestones.lock().unwrap() = None;
    *f.github.parents_fail.lock().unwrap() = true;
    let choice = f.resolve(&PublicationChoiceInput::default(), &settings(MilestonePolicy::default())).unwrap();
    assert_eq!(choice, PublicationChoice::root());
    assert!(f.github.calls().is_empty(), "an unadorned root choice reads nothing");
}

// ---------------------------------------------------------------------------
// Sub-issue publication
// ---------------------------------------------------------------------------

/// GHP-FR-RMVQ / GHP-FR-ETJD / GHP-FR-AZPF / GHP-FR-ATCH / GHP-FR-OHGY: a
/// sub-issue takes the configured Type, inherits the parent's milestone by
/// default, saves all of it before the first mutation, creates the issue with
/// it, and then links it to the parent.
#[test]
fn a_sub_issue_takes_the_configured_type_and_the_parent_milestone_and_links() {
    let f = fixture();
    let parent = f.github.seed("Feature", Some((7, "v1.2")));
    let choice = f.resolve(&sub_of(parent), &settings(MilestonePolicy::InheritParent)).unwrap();
    assert_eq!(choice.kind, PublicationKind::SubIssue);
    assert_eq!(choice.parent_issue_number, Some(parent));
    assert_eq!(choice.parent_repository_owner.as_deref(), Some("acme"));
    assert_eq!(choice.parent_repository_name.as_deref(), Some("widgets"));
    assert_eq!(choice.issue_type.as_deref(), Some("Task"));
    assert_eq!(choice.milestone_policy, Some(MilestonePolicy::InheritParent));
    assert_eq!(choice.milestone_number, Some(7));
    assert_eq!(choice.milestone_title.as_deref(), Some("v1.2"));

    let attempt = f.attempt_with("pub-1", choice.clone());
    assert_eq!(f.store().attempt.unwrap().choice, Some(choice.clone()), "saved before the first mutation");
    assert!(f.github.calls().iter().all(|c| !matches!(c, Call::Create(_))));

    let record = published(f.publish(&attempt));
    let created = f.github.issue(record.issue_number);
    assert_eq!(created.issue_type.as_deref(), Some("Task"));
    assert_eq!(created.milestone.unwrap().number, 7);
    let calls = f.github.calls();
    let create_at = calls.iter().position(|c| matches!(c, Call::Create(_))).unwrap();
    let link_at = calls.iter().position(|c| matches!(c, Call::Link(..))).unwrap();
    assert!(create_at < link_at, "the link follows the create");
    assert!(calls.contains(&Call::Link(parent, created.id, false)));
    assert_eq!(created.parent.unwrap().number, parent);
    assert_eq!(record.choice, Some(choice));
}

/// GHP-FR-AZPF: an inheriting sub-issue of a parent with no milestone has none.
#[test]
fn inherit_parent_with_no_parent_milestone_omits_the_milestone() {
    let f = fixture();
    let parent = f.github.seed("Feature", None);
    let choice = f.resolve(&sub_of(parent), &settings(MilestonePolicy::InheritParent)).unwrap();
    assert_eq!(choice.milestone_number, None);
    assert_eq!(choice.milestone_title, None);
}

/// GHP-FR-AZPF: `no_milestone` omits the milestone whatever the parent holds.
#[test]
fn the_no_milestone_policy_omits_the_milestone() {
    let f = fixture();
    let parent = f.github.seed("Feature", Some((7, "v1.2")));
    let choice = f.resolve(&sub_of(parent), &settings(MilestonePolicy::NoMilestone)).unwrap();
    assert_eq!(choice.milestone_policy, Some(MilestonePolicy::NoMilestone));
    assert_eq!(choice.milestone_number, None);
}

/// GHP-FR-AZPF: `author_selected` uses the open milestone the author named, or
/// none; a milestone under any other policy is refused.
#[test]
fn the_author_selected_policy_uses_the_named_open_milestone() {
    let f = fixture();
    let parent = f.github.seed("Feature", Some((7, "v1.2")));
    let policy = settings(MilestonePolicy::AuthorSelected);

    let named = PublicationChoiceInput { parent_issue_number: Some(parent), milestone_number: Some(8), ..Default::default() };
    let choice = f.resolve(&named, &policy).unwrap();
    assert_eq!(choice.milestone_number, Some(8));
    assert_eq!(choice.milestone_title.as_deref(), Some("v1.3"));

    assert_eq!(f.resolve(&sub_of(parent), &policy).unwrap().milestone_number, None);
    let unknown = PublicationChoiceInput { parent_issue_number: Some(parent), milestone_number: Some(99), ..Default::default() };
    assert_eq!(f.resolve(&unknown, &policy), Err(ERR_MILESTONE_UNAVAILABLE.into()));

    for other in [MilestonePolicy::InheritParent, MilestonePolicy::NoMilestone] {
        assert_eq!(f.resolve(&named, &settings(other)), Err(ERR_INVALID_PUBLICATION_CHOICE.into()));
    }
}

/// GHP-FR-ETJD: a configured Type the repository does not list, and a Type
/// list that cannot be read, publish the sub-issue without a Type; a configured
/// Type resolves to GitHub's spelling.
#[test]
fn an_unavailable_sub_issue_type_is_omitted() {
    let f = fixture();
    let parent = f.github.seed("Feature", None);
    let mut configured = settings(MilestonePolicy::NoMilestone);

    configured.sub_issue_type = "task".into();
    assert_eq!(f.resolve(&sub_of(parent), &configured).unwrap().issue_type.as_deref(), Some("Task"));

    configured.sub_issue_type = "Chore".into();
    assert_eq!(f.resolve(&sub_of(parent), &configured).unwrap().issue_type, None);

    *f.github.types.lock().unwrap() = None;
    configured.sub_issue_type = "Task".into();
    assert_eq!(f.resolve(&sub_of(parent), &configured).unwrap().issue_type, None);
}

/// GHP-FR-RMVQ: a sub-issue choice cannot carry a Type of its own.
#[test]
fn a_sub_issue_choice_cannot_override_the_type() {
    let f = fixture();
    let parent = f.github.seed("Feature", None);
    let input = PublicationChoiceInput { parent_issue_number: Some(parent), issue_type: Some("Bug".into()), ..Default::default() };
    assert_eq!(f.resolve(&input, &settings(MilestonePolicy::default())), Err(ERR_INVALID_PUBLICATION_CHOICE.into()));
}

/// GHP-FR-RMVQ: a parent that is missing, closed, a pull request, or of a Type
/// the settings do not name is refused with `parent_issue_unavailable`.
#[test]
fn an_unusable_parent_is_refused() {
    let f = fixture();
    let closed = f.github.seed("Feature", None);
    f.github.edit(closed, |i| i.open = false);
    let pull = f.github.seed("Feature", None);
    f.github.edit(pull, |i| i.is_pull_request = true);
    let wrong_type = f.github.seed("Bug", None);
    let untyped = f.github.seed("Feature", None);
    f.github.edit(untyped, |i| i.issue_type = None);

    for number in [closed, pull, wrong_type, untyped, 999] {
        assert_eq!(
            f.resolve(&sub_of(number), &settings(MilestonePolicy::default())),
            Err(ERR_PARENT_ISSUE_UNAVAILABLE.into()),
            "parent {number}"
        );
    }
}

/// GHP-FR-KVRH: the parent Types come from the settings and support several.
#[test]
fn the_parent_filter_honours_several_configured_types() {
    let f = fixture();
    let bug = f.github.seed("Bug", None);
    let mut multi = settings(MilestonePolicy::default());
    multi.parent_issue_types = vec!["Feature".into(), "bug".into()];
    assert!(f.resolve(&sub_of(bug), &multi).is_ok());
    assert_eq!(f.resolve(&sub_of(bug), &settings(MilestonePolicy::default())), Err(ERR_PARENT_ISSUE_UNAVAILABLE.into()));
}

// ---------------------------------------------------------------------------
// Metadata reads
// ---------------------------------------------------------------------------

/// GHP-FR-MDLD / GHP-FR-FTMC / GHP-FR-DZLB: the parent list holds open
/// issues of a configured Type only, each with its facts; the lists load
/// independently and nothing is mutated.
#[test]
fn the_metadata_lists_hold_only_parent_candidates() {
    let f = fixture();
    let feature = f.github.seed("Feature", Some((7, "v1.2")));
    let closed = f.github.seed("Feature", None);
    f.github.edit(closed, |i| i.open = false);
    let pull = f.github.seed("Feature", None);
    f.github.edit(pull, |i| i.is_pull_request = true);
    f.github.seed("Bug", None);

    let metadata = flow::load_metadata(&f.github, "secret", "acme", "widgets", GithubPublicationSettings::default());
    assert_eq!(metadata.parents.state, MetadataState::Loaded);
    assert_eq!(metadata.parents.items.len(), 1);
    let row = &metadata.parents.items[0];
    assert_eq!(row.number, feature);
    assert_eq!(row.issue_type, "Feature");
    assert_eq!(row.url, format!("https://github.com/acme/widgets/issues/{feature}"));
    assert_eq!(row.milestone, Some(PublicationMilestone { number: 7, title: "v1.2".into() }));
    assert_eq!(metadata.issue_types.items.len(), 3);
    assert_eq!(metadata.milestones.items.len(), 2);
    assert_eq!(metadata.sub_issue_type, SubIssueTypeResolution { name: "Task".into(), resolved: Some("Task".into()) });
    assert!(f.github.calls().iter().all(|c| matches!(c, Call::ListParents | Call::ListTypes | Call::ListMilestones)));
}

/// GHP-FR-DZLB / GHP-FR-YSPJ / GHP-FR-GHEA / GHP-FR-ETJD: one failed list
/// carries its own typed error and leaves the others loaded; an empty list is
/// loaded, not failed.
#[test]
fn a_failed_list_does_not_fail_the_others() {
    let f = fixture();
    *f.github.parents_fail.lock().unwrap() = true;
    let metadata = flow::load_metadata(&f.github, "secret", "acme", "widgets", GithubPublicationSettings::default());
    assert_eq!(metadata.parents.state, MetadataState::Failed);
    assert_eq!(metadata.parents.error_code.as_deref(), Some(ERR_PARENT_ISSUES_UNREADABLE));
    assert!(metadata.parents.error.is_some());
    assert_eq!(metadata.issue_types.state, MetadataState::Loaded);
    assert_eq!(metadata.milestones.state, MetadataState::Loaded);

    *f.github.parents_fail.lock().unwrap() = false;
    *f.github.types.lock().unwrap() = None;
    *f.github.milestones.lock().unwrap() = None;
    let metadata = flow::load_metadata(&f.github, "secret", "acme", "widgets", GithubPublicationSettings::default());
    assert_eq!(metadata.parents.state, MetadataState::Loaded);
    assert!(metadata.parents.items.is_empty());
    assert_eq!(metadata.issue_types.error_code.as_deref(), Some(ERR_ISSUE_TYPES_UNREADABLE));
    assert_eq!(metadata.milestones.error_code.as_deref(), Some(ERR_MILESTONES_UNREADABLE));
    assert_eq!(metadata.sub_issue_type.resolved, None, "an unreadable Type list leaves the Type unavailable");

    *f.github.types.lock().unwrap() = Some(Vec::new());
    let metadata = flow::load_metadata(&f.github, "secret", "acme", "widgets", GithubPublicationSettings::default());
    assert_eq!(metadata.issue_types.state, MetadataState::Loaded);
    assert!(metadata.issue_types.items.is_empty());
    assert_eq!(metadata.sub_issue_type.resolved, None);
}

/// GHP-FR-FTMC: the parent filter is sent in GitHub's spelling of each Type
/// where the owner lists it, and as saved where it does not.
#[test]
fn the_parent_filter_uses_githubs_spelling_of_each_type() {
    let f = fixture();
    let mut saved = GithubPublicationSettings::default();
    saved.parent_issue_types = vec!["feature".into(), "Retired".into()];
    flow::load_metadata(&f.github, "secret", "acme", "widgets", saved);
    assert_eq!(
        f.github.parent_filters.lock().unwrap().as_slice(),
        &[vec!["Feature".to_string(), "Retired".to_string()]]
    );

    *f.github.types.lock().unwrap() = None;
    let mut unreadable = GithubPublicationSettings::default();
    unreadable.parent_issue_types = vec!["feature".into()];
    flow::load_metadata(&f.github, "secret", "acme", "widgets", unreadable);
    assert_eq!(f.github.parent_filters.lock().unwrap()[1], vec!["feature".to_string()]);
}

// ---------------------------------------------------------------------------
// Retry, recovery, and restart
// ---------------------------------------------------------------------------

/// GHP-FR-NDWB / GHP-FR-YPGL / GHP-FR-IBXN: a new attempt replaces the standing one in a
/// single write, with a new marker and the same saved choice; with no attempt
/// standing it refuses and writes nothing.
#[test]
fn a_restarted_attempt_replaces_the_standing_one_and_keeps_the_choice() {
    let f = fixture();
    let parent = f.github.seed("Feature", Some((7, "v1.2")));
    let choice = f.resolve(&sub_of(parent), &settings(MilestonePolicy::InheritParent)).unwrap();

    assert_eq!(
        flow::restart_attempt(&f.root, &f.id, &origin(), "pub-2".into(), Some(choice.clone())),
        Err(ERR_NO_ATTEMPT.to_string())
    );
    assert_eq!(f.store(), PublicationStore::default());

    f.attempt_with("pub-1", choice.clone());
    flow::set_attempt_state(&f.root, &f.id, AttemptState::AwaitingChoice).unwrap();
    let fresh = flow::restart_attempt(&f.root, &f.id, &origin(), "pub-2".into(), Some(choice.clone())).unwrap();

    let standing = f.store().attempt.unwrap();
    assert_eq!(standing, fresh);
    assert_eq!(standing.marker, "pub-2");
    assert_eq!(standing.state, AttemptState::Open);
    assert_eq!(standing.choice, Some(choice));
    assert!(f.store().publication.is_empty());
}

/// GHP-FR-SWKU / GHP-FR-IBXN / GHP-FR-CMPR / GHP-FR-RCNL: a link that fails
/// leaves the attempt standing with its choice; the retry reads no setting and
/// no list, finds the issue without its parent, offers the recovery choice
/// naming the parent, and **Update existing** links it.
#[test]
fn a_failed_link_is_recovered_by_updating_the_existing_issue() {
    let f = fixture();
    let parent = f.github.seed("Feature", Some((7, "v1.2")));
    let choice = f.resolve(&sub_of(parent), &settings(MilestonePolicy::InheritParent)).unwrap();
    let attempt = f.attempt_with("pub-1", choice.clone());
    *f.github.link_error.lock().unwrap() = Some(ERR_SUB_ISSUE_LINK_FAILED.into());

    assert_eq!(f.publish(&attempt), Err(ERR_SUB_ISSUE_LINK_FAILED.to_string()));
    let standing = f.store().attempt.unwrap();
    assert_eq!(standing.state, AttemptState::Open);
    assert_eq!(standing.choice, Some(choice.clone()));
    assert!(f.store().publication.is_empty());

    *f.github.link_error.lock().unwrap() = None;
    f.github.calls.lock().unwrap().clear();
    let outcome = f.publish(&standing).unwrap();
    let PublicationOutcome::RecoveryRequired { mismatches, .. } = outcome else { panic!("recovery") };
    assert_eq!(mismatches, vec!["parent".to_string()]);
    assert_eq!(f.github.calls(), vec![Call::Search("acme/widgets:pub-1".into())], "no list, no setting, no create");
    assert_eq!(f.store().attempt.unwrap().state, AttemptState::AwaitingChoice);

    f.github.calls.lock().unwrap().clear();
    let record = published(flow::update_existing(&f.root, &f.id, &f.store().attempt.unwrap(), "secret", &f.github));
    let issue = f.github.issue(record.issue_number);
    assert_eq!(issue.parent.unwrap().number, parent);
    assert!(f.github.calls().contains(&Call::Link(parent, issue.id, false)));
    assert!(!f.github.calls().iter().any(|c| matches!(c, Call::Create(_))));
    assert_eq!(record.choice, Some(choice));
    assert_eq!(f.store().publication.len(), 1);
    assert_eq!(f.store().attempt, None);
}

/// GHP-FR-PWJG: a saved parent that is closed, missing, a pull request, or in
/// another repository returns `parent_issue_unavailable`, creates nothing, and
/// keeps the attempt `open`; no root issue is published in its place.
#[test]
fn a_changed_parent_keeps_the_attempt_recoverable_and_publishes_no_root() {
    let f = fixture();
    let parent = f.github.seed("Feature", None);
    let choice = f.resolve(&sub_of(parent), &settings(MilestonePolicy::NoMilestone)).unwrap();

    let mut elsewhere = choice.clone();
    elsewhere.parent_repository_owner = Some("other".into());
    let cases = [
        ("closed", choice.clone(), Box::new(|f: &Fixture| f.github.edit(1, |i| i.open = false)) as Box<dyn Fn(&Fixture)>),
        ("pull request", choice.clone(), Box::new(|f: &Fixture| f.github.edit(1, |i| i.is_pull_request = true))),
        ("another repository", elsewhere, Box::new(|_: &Fixture| {})),
        ("missing", choice.clone(), Box::new(|f: &Fixture| f.github.issues.lock().unwrap().clear())),
    ];
    for (name, saved, change) in cases {
        let f = fixture();
        f.github.seed("Feature", None);
        let attempt = f.attempt_with("pub-1", saved);
        change(&f);
        assert_eq!(f.publish(&attempt), Err(ERR_PARENT_ISSUE_UNAVAILABLE.to_string()), "{name}");
        assert!(!f.github.calls().iter().any(|c| matches!(c, Call::Create(_))), "{name}: no create");
        let standing = f.store().attempt.unwrap();
        assert_eq!(standing.state, AttemptState::Open, "{name}");
        assert_eq!(standing.choice.unwrap().kind, PublicationKind::SubIssue, "{name}");
        assert!(f.store().publication.is_empty(), "{name}");
    }
}

/// GHP-FR-OWLB / GHP-FR-TMBK / GHP-FR-CMPR: a create whose response was lost is
/// found by its marker; matching parent, Type, and milestone reuse the issue
/// and create no second one.
#[test]
fn a_lost_response_is_reused_when_the_metadata_matches() {
    let f = fixture();
    let parent = f.github.seed("Feature", Some((7, "v1.2")));
    let choice = f.resolve(&sub_of(parent), &settings(MilestonePolicy::InheritParent)).unwrap();
    let attempt = f.attempt_with("pub-1", choice.clone());
    // The create and the link landed; the response did not.
    let content = flow::issue_content(&f.root, &f.id, "pub-1").unwrap();
    let created = f
        .github
        .create_issue("s", "acme", "widgets", &content.title, &content.body, &flow::fields_of(&choice))
        .unwrap();
    f.github.link_sub_issue("s", "acme", "widgets", parent, created.id, false).unwrap();
    f.github.calls.lock().unwrap().clear();

    let record = published(f.publish(&attempt));
    assert_eq!(record.issue_number, created.number);
    assert_eq!(f.github.calls(), vec![Call::Search("acme/widgets:pub-1".into())]);
    assert_eq!(f.github.issues.lock().unwrap().len(), 2, "the parent and the one created issue");
}

/// GHP-FR-HRUN / GHP-FR-CMPR: a found issue whose Type, milestone, or parent
/// differs from the saved choice returns the recovery choice naming each
/// difference, and edits and creates nothing.
#[test]
fn a_metadata_mismatch_returns_the_recovery_choice() {
    let f = fixture();
    let parent = f.github.seed("Feature", Some((7, "v1.2")));
    let choice = f.resolve(&sub_of(parent), &settings(MilestonePolicy::InheritParent)).unwrap();
    let attempt = f.attempt_with("pub-1", choice.clone());
    let content = flow::issue_content(&f.root, &f.id, "pub-1").unwrap();
    // Created with a different Type and no milestone, and never linked.
    f.github
        .create_issue("s", "acme", "widgets", &content.title, &content.body, &client::IssueFields { issue_type: Some("Bug".into()), milestone: None })
        .unwrap();
    f.github.calls.lock().unwrap().clear();

    let PublicationOutcome::RecoveryRequired { mismatches, .. } = f.publish(&attempt).unwrap() else {
        panic!("recovery")
    };
    assert_eq!(mismatches, vec!["parent".to_string(), "type".to_string(), "milestone".to_string()]);
    assert_eq!(f.github.calls(), vec![Call::Search("acme/widgets:pub-1".into())]);
    assert!(f.store().publication.is_empty());
    assert_eq!(f.store().attempt.unwrap().state, AttemptState::AwaitingChoice);
}

/// GHP-FR-RCNL: **Update existing** sets only the Type and milestone that
/// differ, replaces a different parent, and removes the parent of a root
/// choice.
#[test]
fn update_existing_reconciles_type_milestone_and_parent() {
    let f = fixture();
    let first = f.github.seed("Feature", None);
    let second = f.github.seed("Feature", Some((8, "v1.3")));
    let choice = f.resolve(&sub_of(second), &settings(MilestonePolicy::InheritParent)).unwrap();
    let attempt = f.attempt_with("pub-1", choice.clone());
    let content = flow::issue_content(&f.root, &f.id, "pub-1").unwrap();
    let wrong = f
        .github
        .create_issue("s", "acme", "widgets", &content.title, &content.body, &client::IssueFields { issue_type: Some("Bug".into()), milestone: None })
        .unwrap();
    f.github.link_sub_issue("s", "acme", "widgets", first, wrong.id, false).unwrap();
    f.github.calls.lock().unwrap().clear();
    f.github.fields.lock().unwrap().clear();

    let record = published(flow::update_existing(&f.root, &f.id, &attempt, "secret", &f.github));
    assert_eq!(record.issue_number, wrong.number);
    assert_eq!(
        f.github.fields.lock().unwrap().as_slice(),
        &[client::IssueFields { issue_type: Some("Task".into()), milestone: Some(8) }]
    );
    assert!(f.github.calls().contains(&Call::Link(second, wrong.id, true)), "a different parent is replaced");
    let issue = f.github.issue(wrong.number);
    assert_eq!(issue.issue_type.as_deref(), Some("Task"));
    assert_eq!(issue.milestone.unwrap().number, 8);
    assert_eq!(issue.parent.unwrap().number, second);
}

/// GHP-FR-CMPR: a Type and a milestone the saved choice does not name are not
/// compared, so an issue that holds them still matches a plain root choice.
#[test]
fn metadata_a_root_choice_does_not_name_is_not_compared() {
    let f = fixture();
    let attempt = f.attempt_with("pub-1", PublicationChoice::root());
    let content = flow::issue_content(&f.root, &f.id, "pub-1").unwrap();
    let fields = client::IssueFields { issue_type: Some("Bug".into()), milestone: Some(7) };
    f.github.create_issue("s", "acme", "widgets", &content.title, &content.body, &fields).unwrap();

    assert!(matches!(f.publish(&attempt), Ok(PublicationOutcome::Published { .. })));
}

/// GHP-FR-CMPR / GHP-FR-RCNL: a root choice expects no parent, so a found issue
/// with one is a mismatch and **Update existing** unlinks it.
#[test]
fn a_root_choice_removes_a_stray_parent() {
    let f = fixture();
    let parent = f.github.seed("Feature", None);
    let attempt = f.attempt_with("pub-1", PublicationChoice::root());
    let content = flow::issue_content(&f.root, &f.id, "pub-1").unwrap();
    let found = f
        .github
        .create_issue("s", "acme", "widgets", &content.title, &content.body, &Default::default())
        .unwrap();
    f.github.link_sub_issue("s", "acme", "widgets", parent, found.id, false).unwrap();

    let PublicationOutcome::RecoveryRequired { mismatches, .. } = f.publish(&attempt).unwrap() else {
        panic!("recovery")
    };
    assert_eq!(mismatches, vec!["parent".to_string()]);
    published(flow::update_existing(&f.root, &f.id, &f.store().attempt.unwrap(), "secret", &f.github));
    assert!(f.github.calls().contains(&Call::Unlink(parent, found.id)));
    assert!(f.github.issue(found.number).parent.is_none());
}

/// GHP-FR-RCNL: a link or unlink that fails returns `sub_issue_link_failed` and
/// keeps the attempt in `awaiting_choice`.
#[test]
fn a_failed_reconcile_link_keeps_the_choice_open() {
    let f = fixture();
    let parent = f.github.seed("Feature", None);
    let choice = f.resolve(&sub_of(parent), &settings(MilestonePolicy::NoMilestone)).unwrap();
    let attempt = f.attempt_with("pub-1", choice);
    let content = flow::issue_content(&f.root, &f.id, "pub-1").unwrap();
    f.github.create_issue("s", "acme", "widgets", &content.title, &content.body, &Default::default()).unwrap();
    f.publish(&attempt).unwrap();
    *f.github.link_error.lock().unwrap() = Some(ERR_SUB_ISSUE_LINK_FAILED.into());

    let standing = f.store().attempt.unwrap();
    assert_eq!(
        flow::update_existing(&f.root, &f.id, &standing, "secret", &f.github),
        Err(ERR_SUB_ISSUE_LINK_FAILED.to_string())
    );
    assert_eq!(f.store().attempt.unwrap().state, AttemptState::AwaitingChoice);
    assert!(f.store().publication.is_empty());
}

/// GHP-FR-CDVT / GHP-FR-ATCH: the saved choice survives a restart exactly, and
/// the retry after it needs no memory of the first run.
#[test]
fn the_saved_choice_survives_a_restart() {
    let f = fixture();
    let parent = f.github.seed("Feature", Some((7, "v1.2")));
    let choice = f.resolve(&sub_of(parent), &settings(MilestonePolicy::InheritParent)).unwrap();
    f.attempt_with("pub-1", choice.clone());

    let relaunched = store::read_store(&crate::fs::RootFs::for_root(f._dir.path()), &f.id).unwrap();
    let attempt = relaunched.attempt.unwrap();
    assert_eq!(attempt.choice, Some(choice));
    let record = published(f.publish(&attempt));
    assert_eq!(f.github.issue(record.issue_number).parent.unwrap().number, parent);
}

/// GHP-FR-CDVT / GHP-FR-HSCH / DRS-FR-LWQT: an attempt and a record written
/// before the choice existed parse, and read as a root issue.
#[test]
fn a_store_without_a_choice_reads_as_a_root_issue() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    let path = crate::drafts::draft_publication_path(&root, &id).unwrap();
    std::fs::write(
        &path,
        r#"
[[publication]]
provider = "github"
repositoryOwner = "acme"
repositoryName = "widgets"
issueNumber = 4
issueUrl = "https://github.com/acme/widgets/issues/4"
publishedAt = "2025-01-01T00:00:00Z"
marker = "pub-old"

[attempt]
marker = "pub-new"
remoteName = "origin"
remoteUrl = "github.com/acme/widgets"
repositoryOwner = "acme"
repositoryName = "widgets"
state = "open"
startedAt = "2025-01-02T00:00:00Z"
updatedAt = "2025-01-02T00:00:00Z"
"#,
    )
    .unwrap();

    let read = store::read_store(&root, &id).unwrap();
    assert_eq!(read.publication[0].choice, None);
    let attempt = read.attempt.unwrap();
    assert_eq!(attempt.choice, None);
    assert_eq!(attempt.effective_choice(), PublicationChoice::root());
}

/// DRS-FR-LWQT / GHP-FR-HSCH: a choice round-trips through the TOML store, a
/// null value being absent from the file.
#[test]
fn a_choice_round_trips_through_the_store_file() {
    let f = fixture();
    let parent = f.github.seed("Feature", Some((7, "v1.2")));
    let choice = f.resolve(&sub_of(parent), &settings(MilestonePolicy::InheritParent)).unwrap();
    let attempt = f.attempt_with("pub-1", choice.clone());
    published(f.publish(&attempt));

    let path = crate::drafts::draft_publication_path(&f.root, &f.id).unwrap();
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.contains("parentIssueNumber"));
    assert!(!text.contains("null"));
    assert_eq!(f.store().publication[0].choice, Some(choice));

    let root_attempt = f.attempt_with("pub-2", PublicationChoice::root());
    let text = std::fs::read_to_string(crate::drafts::draft_publication_path(&f.root, &f.id).unwrap()).unwrap();
    assert_eq!(text.matches("parentIssueNumber").count(), 1, "a root choice holds no parent");
    let _ = root_attempt;
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

/// GHP-FR-NQWX: the settings a caller may save are cleaned, and an empty Type
/// list or an empty Type name is refused.
#[test]
fn the_settings_validation_refuses_what_cannot_be_used() {
    let ok = flow::validated_settings(
        vec![" Feature ".into(), "feature".into(), "Epic".into()],
        " Task ".into(),
        MilestonePolicy::AuthorSelected,
    )
    .unwrap();
    assert_eq!(ok.parent_issue_types, vec!["Feature".to_string(), "Epic".to_string()]);
    assert_eq!(ok.sub_issue_type, "Task");

    for (types, sub) in [(vec![], "Task"), (vec!["  ".to_string()], "Task"), (vec!["Feature".to_string()], "  ")] {
        assert_eq!(
            flow::validated_settings(types, sub.to_string(), MilestonePolicy::default()),
            Err(ERR_INVALID_PUBLICATION_SETTINGS.to_string())
        );
    }
}

/// GHP-FR-KVRH / GHP-FR-NQWX: a saved Type that GitHub does not list stays
/// saved, and the settings never consult GitHub.
#[test]
fn saved_settings_are_kept_as_written() {
    let f = fixture();
    let saved = flow::validated_settings(vec!["Retired".into()], "Gone".into(), MilestonePolicy::NoMilestone).unwrap();
    save_github_publication_settings_to(&f.root, &saved).unwrap();
    assert_eq!(load_github_publication_settings_from(&f.root).unwrap(), saved);
    assert!(f.github.calls().is_empty());
}

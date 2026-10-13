//! The configurable Ready status and In Progress status (GPP-FR-VXSB,
//! GPP-FR-NNSJ, GPP-FR-YNOT, GPP-FR-DBPI, GPP-FR-IURX, GPP-FR-NLPG,
//! GPP-FR-DEZO, GPP-FR-EHRC, GPP-FR-BOQX, GPP-FR-ILGG, GPP-FR-ANDE,
//! GPP-FR-SUFH).

use super::super::claims;
use super::super::{claim_impl, list_statuses_impl, poll_impl, set_settings_impl};
use super::*;

/// A Project whose `Status` field holds the given option names, in order.
fn shape_of(names: &[&str]) -> ProjectShape {
    ProjectShape {
        title: "Roadmap".into(),
        status_field: Some(StatusField {
            field_id: "FIELD_status".into(),
            options: names
                .iter()
                .map(|name| StatusOption {
                    id: format!("OPT_{}", name.to_lowercase().replace(' ', "_")),
                    name: (*name).into(),
                })
                .collect(),
        }),
    }
}

fn names(ready: Option<&str>, in_progress: Option<&str>) -> StatusNames {
    StatusNames { ready: ready.map(str::to_string), in_progress: in_progress.map(str::to_string) }
}

fn chosen(project: &str, ready: Option<&str>, in_progress: Option<&str>) -> GithubPollingSettings {
    GithubPollingSettings {
        project_node_id: Some(project.into()),
        interval_minutes: Some(5),
        ready_status: ready.map(str::to_string),
        in_progress_status: in_progress.map(str::to_string),
    }
}

/// GPP-FR-VXSB: an unset name resolves to the first option that matches its
/// default case-insensitively; a stored name matches exactly and never falls
/// back to the default.
#[test]
fn a_status_name_resolves_exactly_or_by_its_default() {
    let shape = shape_of(&["Todo", "ready", "In progress", "In Progress", "Done"]);
    let options = &shape.status_field.as_ref().unwrap().options;

    assert_eq!(resolve_status(options, None, READY, None).unwrap().name, "ready");
    assert_eq!(
        resolve_status(options, None, IN_PROGRESS, None).unwrap().name,
        "In progress",
        "the first case-insensitive match in the order of the field"
    );
    assert_eq!(
        resolve_status(options, Some("In Progress"), IN_PROGRESS, None).unwrap().name,
        "In Progress"
    );
    assert_eq!(resolve_status(options, Some("Todo"), READY, None).unwrap().name, "Todo");
    assert!(
        resolve_status(options, Some("Ready"), READY, None).is_none(),
        "no case-insensitive fallback"
    );
    assert!(resolve_status(options, Some("Gone"), READY, None).is_none(), "no default fallback");
    assert!(
        resolve_status(options, None, READY, Some("ready")).is_none(),
        "an unset name never takes the option the other status holds"
    );
}

/// GPP-FR-DEZO / GPP-FR-VXSB: a Project whose field says `In progress` is
/// valid with unset names, and with chosen names its own options are used.
#[test]
fn a_project_with_other_option_names_validates() {
    let lower = shape_of(&["Todo", "Ready", "In progress", "Done"]);
    let valid = validate_shape(&lower, &StatusNames::default()).unwrap();
    assert_eq!(valid.ready_option_id, "OPT_ready");
    assert_eq!(valid.in_progress_option_id, "OPT_in_progress");
    assert_eq!(valid.in_progress_name, "In progress");

    let custom = shape_of(&["Backlog", "Queued", "Doing", "Done"]);
    let valid = validate_shape(&custom, &names(Some("Queued"), Some("Doing"))).unwrap();
    assert_eq!(valid.ready_option_id, "OPT_queued");
    assert_eq!(valid.ready_name, "Queued");
    assert_eq!(valid.in_progress_option_id, "OPT_doing");
}

/// GPP-FR-DBPI / GPP-FR-VXSB: an unset status whose default is the option the
/// other status holds resolves to nothing, so the two never share an option.
#[test]
fn two_statuses_never_resolve_to_one_option() {
    let shape = shape_of(&["Ready", "In Progress"]);
    let failure = validate_shape(&shape, &names(None, Some("Ready"))).unwrap_err();
    assert_eq!(failure.code, ERR_READY_OPTION_MISSING);
    assert_eq!(failure.missing_status.as_deref(), Some(READY));
    let failure = validate_shape(&shape, &names(Some("In Progress"), None)).unwrap_err();
    assert_eq!(failure.code, ERR_IN_PROGRESS_OPTION_MISSING);
}

/// GPP-FR-EHRC / GPP-FR-YNOT: a stored name that no option holds, and an
/// unset name without a default match, each name the missing status; the
/// text names the status and the Project and asks for a choice.
#[test]
fn a_missing_status_is_named_with_the_project() {
    let renamed = shape_of(&["Todo", "Ready", "In Progress"]);
    let failure = validate_shape(&renamed, &names(Some("Queued"), None)).unwrap_err();
    assert_eq!(failure.code, ERR_READY_OPTION_MISSING);
    assert_eq!(failure.missing_status.as_deref(), Some("Queued"));
    assert_eq!(failure.project_title.as_deref(), Some("Roadmap"));

    let failure = validate_shape(&renamed, &names(None, Some("Doing"))).unwrap_err();
    assert_eq!(failure.code, ERR_IN_PROGRESS_OPTION_MISSING);
    assert_eq!(failure.missing_status.as_deref(), Some("Doing"));

    let no_default = shape_of(&["Todo", "Doing"]);
    let failure = validate_shape(&no_default, &StatusNames::default()).unwrap_err();
    assert_eq!(failure.code, ERR_READY_OPTION_MISSING);
    assert_eq!(
        failure.missing_status.as_deref(),
        Some(READY),
        "the default name where none is stored"
    );

    let configuration = GithubPollingConfiguration::invalid(
        ERR_READY_OPTION_MISSING,
        Some("Roadmap".into()),
        Some("Queued".into()),
    );
    let text = configuration.error.unwrap();
    assert!(text.contains("\"Queued\""), "{text}");
    assert!(text.contains("\"Roadmap\""), "{text}");
    assert!(text.contains("Choose an option"), "{text}");
    assert_eq!(configuration.missing_status.as_deref(), Some("Queued"));
}

/// GPP-FR-BOQX: an issue is eligible with the `Status` value that the Ready
/// status resolved to, and not with the default name.
#[test]
fn eligibility_follows_the_chosen_ready_status() {
    let items = vec![
        item(1, Some("Queued"), "OPEN", Some("Task"), "acme", "widgets"),
        item(2, Some("Ready"), "OPEN", Some("Task"), "acme", "widgets"),
        item(3, Some("queued"), "OPEN", Some("Task"), "acme", "widgets"),
    ];
    let kept: Vec<u64> =
        eligible_tasks(&items, &repo(), "Queued").iter().map(|t| t.issue_number).collect();
    assert_eq!(kept, vec![1]);
}

/// GPP-FR-ILGG / GPP-FR-BOQX: a claim re-fetches against the chosen Ready
/// status and moves the item to the chosen In Progress option.
#[test]
fn a_claim_moves_the_issue_to_the_chosen_in_progress_option() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let repository = repo();
    let fake = FakeProjects::new();
    *fake.shape.lock().unwrap() = Ok(shape_of(&["Queued", "Ready", "Doing", "In Progress"]));
    fake.hold_issue(4, "Queued", Some("Task"), "OPEN");
    fake.hold_issue(5, "Ready", Some("Task"), "OPEN");
    let statuses = names(Some("Queued"), Some("Doing"));
    let ctx = claims::ClaimContext {
        root: &root,
        secret: SECRET,
        repository: &repository,
        project_id: PROJECT,
        statuses: &statuses,
    };

    assert_eq!(
        claims::claim(&ctx, &fake, 5, "2026-10-02T09:30:00Z", &[]).unwrap_err().code,
        ERR_TASK_NOT_READY
    );
    claims::claim(&ctx, &fake, 4, "2026-10-02T09:30:00Z", &[]).expect("claimed");
    let updates: Vec<Call> =
        fake.calls().into_iter().filter(|c| matches!(c, Call::SetStatus { .. })).collect();
    assert_eq!(
        updates,
        vec![Call::SetStatus { item: "ITEM_4".into(), option: "OPT_doing".into() }]
    );
}

/// GPP-FR-IURX / GPP-FR-NLPG: a settings write persists the four values and
/// validates them; a cleared Project clears both names; an empty name is
/// stored as unset.
#[test]
fn a_settings_write_persists_and_validates_the_status_names() {
    let h = Harness::new();
    *h.fake.shape.lock().unwrap() = Ok(shape_of(&["Queued", "Doing"]));
    let view =
        set_settings_impl(&h.handle(), chosen(PROJECT, Some("Queued"), Some("Doing"))).unwrap();
    assert_eq!(view.configuration.state, ConfigurationState::Valid);
    assert_eq!(view.settings.ready_status.as_deref(), Some("Queued"));
    let stored = crate::project_settings::load_github_polling_settings_from(&h.root()).unwrap();
    assert_eq!(stored, chosen(PROJECT, Some("Queued"), Some("Doing")));

    let view = set_settings_impl(&h.handle(), chosen(PROJECT, Some(""), Some("Doing"))).unwrap();
    assert_eq!(view.settings.ready_status, None, "an empty name is unset");
    assert_eq!(view.configuration.error_code.as_deref(), Some(ERR_READY_OPTION_MISSING));
    assert_eq!(view.configuration.missing_status.as_deref(), Some(READY));

    let cleared = GithubPollingSettings {
        project_node_id: None,
        interval_minutes: Some(5),
        ready_status: Some("Queued".into()),
        in_progress_status: Some("Doing".into()),
    };
    let view = set_settings_impl(&h.handle(), cleared).unwrap();
    assert_eq!(view.configuration.state, ConfigurationState::Unset);
    assert_eq!(view.settings.ready_status, None);
    assert_eq!(view.settings.in_progress_status, None);
}

/// GPP-FR-DBPI / GPP-FR-SUFH: two equal status names are refused with
/// `duplicate_status_option`, and nothing is written.
#[test]
fn equal_status_names_are_refused_and_nothing_is_written() {
    let h = Harness::new();
    h.select(Some(PROJECT));
    let path = h.dir.path().join(".synthesis/project.toml");
    let before = std::fs::read(&path).unwrap();
    let refused = set_settings_impl(&h.handle(), chosen(PROJECT, Some("Ready"), Some("Ready")));
    assert_eq!(refused.unwrap_err(), ERR_DUPLICATE_STATUS_OPTION);
    assert_eq!(std::fs::read(&path).unwrap(), before, "project.toml is byte-identical");
}

/// GPP-FR-NNSJ: the status options of a Project, in the order of the field;
/// a Project without the field is refused; nothing is written.
#[test]
fn the_status_options_are_listed_in_field_order() {
    let h = Harness::new();
    *h.fake.shape.lock().unwrap() = Ok(shape_of(&["Todo", "In progress", "Ready"]));
    let options = list_statuses_impl(&h.handle(), PROJECT).unwrap();
    let listed: Vec<&str> = options.iter().map(|o| o.name.as_str()).collect();
    assert_eq!(listed, vec!["Todo", "In progress", "Ready"]);
    assert_eq!(options[1].id, "OPT_in_progress");
    assert!(!h.dir.path().join(".synthesis/project.toml").exists(), "a read writes nothing");

    let mut no_field = valid_shape();
    no_field.status_field = None;
    *h.fake.shape.lock().unwrap() = Ok(no_field);
    assert_eq!(list_statuses_impl(&h.handle(), PROJECT).unwrap_err(), ERR_STATUS_FIELD_MISSING);
    *h.fake.shape.lock().unwrap() = Err(ERR_PROJECT_UNAVAILABLE.into());
    assert_eq!(list_statuses_impl(&h.handle(), PROJECT).unwrap_err(), ERR_PROJECT_UNAVAILABLE);
}

/// GPP-FR-ANDE / GPP-FR-YNOT: a saved status that was renamed on GitHub makes
/// a poll find the configuration invalid; polls and claims then refuse until
/// the author chooses again.
#[test]
fn a_renamed_status_stops_polling_until_the_author_chooses_again() {
    let h = Harness::new();
    *h.fake.shape.lock().unwrap() = Ok(shape_of(&["Queued", "Doing"]));
    set_settings_impl(&h.handle(), chosen(PROJECT, Some("Queued"), Some("Doing"))).unwrap();

    *h.fake.shape.lock().unwrap() = Ok(shape_of(&["Up next", "Doing"]));
    let view = poll_impl(&h.handle()).unwrap();
    assert_eq!(view.configuration.state, ConfigurationState::Invalid);
    assert_eq!(view.configuration.error_code.as_deref(), Some(ERR_READY_OPTION_MISSING));
    assert_eq!(view.configuration.missing_status.as_deref(), Some("Queued"));
    assert_eq!(view.settings.ready_status.as_deref(), Some("Queued"), "nothing is replaced");

    assert_eq!(poll_impl(&h.handle()).unwrap_err(), ERR_CONFIGURATION_INVALID);
    assert_eq!(claim_impl(&h.handle(), 4).unwrap_err(), ERR_CONFIGURATION_INVALID);

    let view =
        set_settings_impl(&h.handle(), chosen(PROJECT, Some("Up next"), Some("Doing"))).unwrap();
    assert_eq!(view.configuration.state, ConfigurationState::Valid);
}

//! Remote enumeration, canonicalization, eligibility, and the persisted choice
//! (GHP-FR-WKDE, GHP-FR-BXTU, GHP-FR-MZPR, GHP-FR-LTAC, GHP-FR-HVQG,
//! GHP-FR-XAUP, GHP-FR-NDSB, GHP-FR-ZRFP, GHP-FR-PWXA).

use super::*;
use crate::project_settings::PublicationRemoteSelection;

/// GHP-FR-BXTU: every URL form of one repository canonicalizes to one value,
/// and the credentials, the port, the `.git`, and the trailing slash all come
/// off.
#[test]
fn canonicalizes_every_github_url_form_to_one_value() {
    let forms = [
        "https://github.com/acme/widgets.git",
        "https://token@github.com/acme/widgets",
        "https://user:secret@GitHub.com:443/acme/widgets/",
        "git@github.com:acme/widgets.git",
        "ssh://git@github.com/acme/widgets",
    ];
    for form in forms {
        assert_eq!(remotes::parse_github_remote(form), Some(("acme".into(), "widgets".into())));
        assert_eq!(remotes::canonical_url(form), "github.com/acme/widgets", "{form}");
    }
}

/// GHP-FR-BXTU / GHP-FR-DHXK: a non-GitHub remote is classified as such, and a
/// credential in its URL never survives into the canonical form written to
/// disk.
#[test]
fn a_non_github_url_is_not_a_github_remote_and_loses_its_credentials() {
    assert_eq!(remotes::parse_github_remote("https://git.internal/acme/widgets"), None);
    assert_eq!(
        remotes::canonical_url("https://user:secret@git.internal/acme/widgets/"),
        "https://git.internal/acme/widgets"
    );
    // Not a repository address: too much path.
    assert_eq!(remotes::parse_github_remote("https://github.com/acme"), None);
    assert_eq!(remotes::parse_github_remote("https://github.com/acme/widgets/pulls"), None);
}

/// GHP-FR-LTAC: a non-GitHub remote is reported with its reason rather than
/// omitted. GHP-FR-MZPR: a GitHub remote carries the check that failed.
#[test]
fn every_configured_remote_is_reported_with_its_own_reason() {
    let fake = FakeGithub::with_probe(ProbeOutcome::CreateForbidden);
    let classified = remotes::classify_with(
        &[
            configured("origin", "https://github.com/acme/widgets.git"),
            configured("mirror", "https://git.internal/acme/widgets"),
        ],
        Some("secret"),
        &fake,
     false);
    assert_eq!(classified.len(), 2);
    assert_eq!(classified[0].eligibility, RemoteEligibility::IssuesCreateForbidden);
    assert!(classified[0].reason.as_deref().unwrap().contains("create issues"));
    assert_eq!(classified[1].eligibility, RemoteEligibility::NotGithub);
    assert_eq!(classified[1].kind, RemoteKind::Other);
    // GHP-FR-WKDE: classification probes and mutates nothing.
    assert_eq!(fake.calls(), vec![Call::Probe("acme/widgets".into())]);
}

/// GHP-FR-TKBW: a repository that accepts no issue carries `issues_disabled`,
/// and its reason names the repository rather than the token. An author whose
/// token is sound must not be sent to their token settings.
#[test]
fn a_repository_that_accepts_no_issue_names_the_repository_in_its_reason() {
    let classified = remotes::classify_with(
        &[configured("origin", "https://github.com/acme/widgets.git")],
        Some("secret"),
        &FakeGithub::with_probe(ProbeOutcome::IssuesDisabled),
        false,
    );
    assert_eq!(classified[0].eligibility, RemoteEligibility::IssuesDisabled);
    let reason = classified[0].reason.as_deref().unwrap();
    assert!(reason.contains("repository"), "{reason}");
    assert!(!reason.contains("token"), "{reason}");
    // The refusal the same remote produces carries that one sentence too.
    assert_eq!(remotes::refusal_reason(ERR_ISSUES_DISABLED), reason);
}

/// GHP-FR-MZPR: with no token resolved, no request is made at all and every
/// GitHub remote reads `token_unavailable`.
#[test]
fn without_a_token_no_request_is_made_and_every_github_remote_is_ineligible() {
    let fake = FakeGithub::publishable();
    let classified =
        remotes::classify_with(&[configured("origin", "https://github.com/acme/widgets")], None, &fake, false);
    assert_eq!(classified[0].eligibility, RemoteEligibility::TokenUnavailable);
    assert!(fake.calls().is_empty());
}

/// GHP-FR-XAUP / GHP-FR-NDSB: one eligible remote is used automatically.
#[test]
fn one_eligible_remote_is_selected_automatically() {
    let fake = FakeGithub::publishable();
    let classified =
        remotes::classify_with(&[configured("origin", "https://github.com/acme/widgets")], Some("s"), &fake, false);
    let resolution = remotes::resolve(classified, None);
    assert_eq!(resolution.selection.as_deref(), Some("origin"));
    assert_eq!(resolution.origin, SelectionOrigin::Automatic);
}

/// GHP-FR-XAUP / GHP-FR-NDSB: two or more remotes leave the choice to the
/// caller, and the origin says the choice is this attempt's alone.
#[test]
fn two_remotes_leave_the_choice_to_the_caller() {
    let fake = FakeGithub::publishable();
    let classified = remotes::classify_with(
        &[
            configured("mirror", "https://git.internal/acme/widgets"),
            configured("origin", "https://github.com/acme/widgets"),
        ],
        Some("s"),
        &fake,
     false);
    let resolution = remotes::resolve(classified, None);
    assert_eq!(resolution.selection.as_deref(), Some("origin"));
    assert_eq!(resolution.origin, SelectionOrigin::AttemptOnly);
    assert_eq!(resolution.remotes.len(), 2);
}

/// GHP-FR-HVQG / GHP-FR-NDSB: a persisted choice applies while both the name
/// and the canonicalized URL still match.
#[test]
fn a_persisted_choice_is_reused_while_name_and_url_still_match() {
    let fake = FakeGithub::publishable();
    let classified = remotes::classify_with(
        &[
            configured("origin", "https://github.com/acme/widgets"),
            configured("upstream", "https://github.com/acme/upstream"),
        ],
        Some("s"),
        &fake,
     false);
    let persisted = PublicationRemoteSelection {
        name: "upstream".into(),
        url: "github.com/acme/upstream".into(),
    };
    let resolution = remotes::resolve(classified, Some(persisted));
    assert_eq!(resolution.selection.as_deref(), Some("upstream"));
    assert_eq!(resolution.origin, SelectionOrigin::Persisted);
}

/// GHP-FR-HVQG: a rename and a re-point each invalidate the persisted choice
/// for this attempt, and so does the remote becoming ineligible.
#[test]
fn a_renamed_repointed_or_ineligible_remote_invalidates_the_persisted_choice() {
    let eligible = || {
        remotes::classify_with(
            &[
                configured("origin", "https://github.com/acme/widgets"),
                configured("upstream", "https://github.com/acme/upstream"),
            ],
            Some("s"),
            &FakeGithub::publishable(),
         false)
    };
    // Renamed: no remote of that name is enumerated any more.
    let renamed = remotes::resolve(
        eligible(),
        Some(PublicationRemoteSelection { name: "fork".into(), url: "github.com/acme/upstream".into() }),
    );
    assert_eq!(renamed.origin, SelectionOrigin::AttemptOnly);
    // Re-pointed: the name matches and the canonicalized URL does not.
    let repointed = remotes::resolve(
        eligible(),
        Some(PublicationRemoteSelection { name: "upstream".into(), url: "github.com/acme/old".into() }),
    );
    assert_eq!(repointed.origin, SelectionOrigin::AttemptOnly);
    // The choice itself is still reported, so a surface can say what was stored.
    assert_eq!(repointed.persisted_choice.unwrap().name, "upstream");
    // Ineligible: the remote is there and the token cannot publish to it.
    let ineligible = remotes::classify_with(
        &[configured("upstream", "https://github.com/acme/upstream")],
        Some("s"),
        &FakeGithub::with_probe(ProbeOutcome::IssuesUnreadable),
     false);
    let refused = remotes::resolve(
        ineligible,
        Some(PublicationRemoteSelection { name: "upstream".into(), url: "github.com/acme/upstream".into() }),
    );
    assert_eq!(refused.origin, SelectionOrigin::None);
    assert_eq!(refused.selection, None);
}

/// GHP-FR-ZRFP: each of the six no-selection cases names itself.
#[test]
fn a_resolution_with_no_selection_names_which_case_holds() {
    let of = |configured_remotes: &[ConfiguredRemote], probe| {
        remotes::refusal_for(&remotes::classify_with(configured_remotes, Some("s"), &FakeGithub::with_probe(probe), false))
    };
    assert_eq!(remotes::refusal_for(&[]), ERR_NO_REMOTE);
    assert_eq!(
        of(&[configured("mirror", "https://git.internal/a/b")], ProbeOutcome::Publishable),
        ERR_NO_GITHUB_REMOTE
    );
    assert_eq!(
        of(&[configured("origin", "https://github.com/acme/widgets")], ProbeOutcome::IssuesUnreadable),
        ERR_ISSUES_INACCESSIBLE
    );
    assert_eq!(
        of(&[configured("origin", "https://github.com/acme/widgets")], ProbeOutcome::CreateForbidden),
        ERR_ISSUES_CREATE_FORBIDDEN
    );
    assert_eq!(
        of(&[configured("origin", "https://github.com/acme/widgets")], ProbeOutcome::IssuesDisabled),
        ERR_ISSUES_DISABLED
    );
    assert_eq!(
        remotes::refusal_for(&remotes::classify_with(
            &[configured("origin", "https://github.com/acme/widgets")],
            None,
            &FakeGithub::publishable(),
        false,
    )),
        ERR_TOKEN_UNAVAILABLE
    );
}

/// GHP-FR-ZRFP / GHP-FR-MZPR: every ineligible value carries its own typed
/// code and its own sentence, and the refusal for that code answers with the
/// same sentence. A mis-wired arm in either table compiles and ships, and the
/// publish-time refusal is where the author meets it.
#[test]
fn every_ineligible_value_maps_to_one_code_and_one_sentence() {
    let cases = [
        (RemoteEligibility::NotGithub, ERR_NO_GITHUB_REMOTE),
        (RemoteEligibility::IssuesInaccessible, ERR_ISSUES_INACCESSIBLE),
        (RemoteEligibility::IssuesDisabled, ERR_ISSUES_DISABLED),
        (RemoteEligibility::IssuesCreateForbidden, ERR_ISSUES_CREATE_FORBIDDEN),
        (RemoteEligibility::TokenUnavailable, ERR_TOKEN_UNAVAILABLE),
    ];
    for (eligibility, code) in cases {
        assert_eq!(eligibility.error_code(), code, "{eligibility:?}");
        let sentence = eligibility.reason().expect("an ineligible value states a reason");
        // `no_github_remote` is the exception: as a refusal it speaks about the
        // project holding no GitHub remote, and as an eligibility value about
        // one remote which is not a GitHub repository. Two statements, so two
        // sentences.
        if code != ERR_NO_GITHUB_REMOTE {
            assert_eq!(remotes::refusal_reason(code), sentence, "{eligibility:?}");
        }
    }
    assert_eq!(RemoteEligibility::Eligible.reason(), None);
    assert_eq!(RemoteEligibility::Eligible.error_code(), "");
    // The two refusals that speak about the project rather than about a remote.
    assert_eq!(remotes::refusal_reason(ERR_NO_REMOTE), "This project has no configured Git remote.");
    assert_eq!(remotes::refusal_reason(ERR_NO_GITHUB_REMOTE), "This project has no GitHub remote.");
    // A code no eligibility value stands for still answers with a sentence.
    assert_eq!(remotes::refusal_reason(ERR_GITHUB_UNREACHABLE), "Publication is not available.");
}

/// GHP-FR-CVYK: the publish-time refusal carries the remote's own typed code,
/// which is the path `error_code()` is actually read on.
#[test]
fn a_repository_that_accepts_no_issue_refuses_the_publish_with_its_own_code() {
    let classified = remotes::classify_with(
        &[configured("origin", "https://github.com/acme/widgets")],
        Some("s"),
        &FakeGithub::with_probe(ProbeOutcome::IssuesDisabled),
        false,
    );
    assert_eq!(
        flow::resolve_remote_for(&classified, "origin"),
        Err(ERR_ISSUES_DISABLED.to_string())
    );
}

/// GHP-FR-WNJC: where two GitHub remotes fail differently, the refusal names
/// the token condition before the one repository's condition. The author fixes
/// a token once; a repository setting stops that repository alone.
#[test]
fn a_token_condition_is_named_before_one_repository_that_takes_no_issue() {
    let mixed = |first: ProbeOutcome, second: ProbeOutcome| {
        let mut classified = remotes::classify_with(
            &[configured("origin", "https://github.com/acme/widgets")],
            Some("s"),
            &FakeGithub::with_probe(first),
            false,
        );
        classified.extend(remotes::classify_with(
            &[configured("upstream", "https://github.com/acme/upstream")],
            Some("s"),
            &FakeGithub::with_probe(second),
            false,
        ));
        remotes::refusal_for(&classified)
    };
    assert_eq!(
        mixed(ProbeOutcome::IssuesDisabled, ProbeOutcome::CreateForbidden),
        ERR_ISSUES_CREATE_FORBIDDEN
    );
    assert_eq!(
        mixed(ProbeOutcome::IssuesDisabled, ProbeOutcome::IssuesUnreadable),
        ERR_ISSUES_INACCESSIBLE
    );
    assert_eq!(
        mixed(ProbeOutcome::CreateForbidden, ProbeOutcome::IssuesUnreadable),
        ERR_ISSUES_INACCESSIBLE
    );
    // `token_unavailable` heads the order. It arises only where the project
    // resolves no secret, so the mixed case is one remote classified without a
    // token beside one that was probed.
    let mut no_token = remotes::classify_with(
        &[configured("origin", "https://github.com/acme/widgets")],
        None,
        &FakeGithub::publishable(),
        false,
    );
    no_token.extend(remotes::classify_with(
        &[configured("upstream", "https://github.com/acme/upstream")],
        Some("s"),
        &FakeGithub::with_probe(ProbeOutcome::IssuesDisabled),
        false,
    ));
    assert_eq!(remotes::refusal_for(&no_token), ERR_TOKEN_UNAVAILABLE);
}

/// GHP-FR-CVYK: a remote that is no longer eligible at the moment of the call
/// is refused with its own typed error.
#[test]
fn publishing_re_resolves_the_named_remote_and_refuses_an_ineligible_one() {
    let classified = remotes::classify_with(
        &[configured("origin", "https://github.com/acme/widgets")],
        Some("s"),
        &FakeGithub::with_probe(ProbeOutcome::CreateForbidden),
     false);
    assert_eq!(
        flow::resolve_remote_for(&classified, "origin"),
        Err(ERR_ISSUES_CREATE_FORBIDDEN.to_string())
    );
    assert_eq!(flow::resolve_remote_for(&classified, "gone"), Err(ERR_NO_REMOTE.to_string()));
}

/// GHP-FR-PWXA: the option persists the choice, and leaving it clear writes
/// nothing.
#[test]
fn the_persist_option_decides_whether_the_choice_reaches_project_local_settings() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    flow::persist_choice(&root, &origin(), false).unwrap();
    assert_eq!(crate::project_settings::load_publication_remote_selection_from(&root), None);
    flow::persist_choice(&root, &origin(), true).unwrap();
    let stored = crate::project_settings::load_publication_remote_selection_from(&root).unwrap();
    assert_eq!(stored.name, "origin");
    assert_eq!(stored.url, "github.com/acme/widgets");
}

/// PSS-FR-TQFB / PSS-FR-NKRE: the stored choice is cleared by `None`, and a
/// malformed or half-written value reads as no choice at all rather than as an
/// error.
#[test]
fn the_stored_choice_is_clearable_and_a_malformed_one_reads_as_none() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    flow::persist_choice(&root, &origin(), true).unwrap();
    assert!(crate::project_settings::load_publication_remote_selection_from(&root).is_some());

    crate::project_settings::save_publication_remote_selection_to(&root, None).unwrap();
    assert_eq!(crate::project_settings::load_publication_remote_selection_from(&root), None);

    // Half a record — a name with no URL — is no choice.
    crate::project_settings::save_publication_remote_selection_to(
        &root,
        Some(crate::project_settings::PublicationRemoteSelection {
            name: "origin".into(),
            url: String::new(),
        }),
    )
    .unwrap();
    assert_eq!(crate::project_settings::load_publication_remote_selection_from(&root), None);
}

/// The two cases below are the only ones that exercise the process-wide
/// eligibility cache, and each clears **all** of it. Cargo runs this file's
/// cases on a thread pool in one process, so without this lock one case's
/// `forget_probes` would drop the entry the other is counting requests
/// against — an intermittent failure with a window of microseconds.
static CACHE_CASES: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// The eligibility cache answers a repeated question without a second request,
/// and `forget_probes` drops it — which is what a settled publication does, so
/// a repository the author has just changed is asked about again.
#[test]
fn the_eligibility_cache_answers_a_repeated_question_without_a_second_request() {
    let _serialized = CACHE_CASES.lock().unwrap_or_else(|e| e.into_inner());
    remotes::forget_probes();
    let fake = FakeGithub::publishable();
    let remotes_of = || {
        remotes::classify_with(
            &[configured("origin", "https://github.com/cache-case/widgets")],
            Some("cache-case-token"),
            &fake,
            true,
        )
    };
    assert_eq!(remotes_of()[0].eligibility, RemoteEligibility::Eligible);
    assert_eq!(remotes_of()[0].eligibility, RemoteEligibility::Eligible);
    assert_eq!(fake.calls().len(), 1);

    remotes::forget_probes();
    assert_eq!(remotes_of()[0].eligibility, RemoteEligibility::Eligible);
    assert_eq!(fake.calls().len(), 2);
}

/// GHP-FR-MZPR: a cached answer stands for the whole TTL, so a repository the
/// author has just changed keeps its previous eligibility until the cache is
/// dropped. This is what an author meets after they turn Issues on: the action
/// stays disabled for a moment, and that is the cache rather than the fix
/// failing.
#[test]
fn a_repository_the_author_has_just_changed_keeps_its_answer_until_the_cache_drops() {
    let _serialized = CACHE_CASES.lock().unwrap_or_else(|e| e.into_inner());
    remotes::forget_probes();
    let fake = FakeGithub::with_probe(ProbeOutcome::IssuesDisabled);
    let remotes_of = || {
        remotes::classify_with(
            &[configured("origin", "https://github.com/stale-case/widgets")],
            Some("stale-case-token"),
            &fake,
            true,
        )
    };
    assert_eq!(remotes_of()[0].eligibility, RemoteEligibility::IssuesDisabled);
    // The author turns Issues on. The repository answers differently now.
    *fake.probe.lock().unwrap() = ProbeOutcome::Publishable;
    assert_eq!(remotes_of()[0].eligibility, RemoteEligibility::IssuesDisabled);
    // Dropping the cache is what a settled publication does, and it is what the
    // TTL does on its own.
    remotes::forget_probes();
    assert_eq!(remotes_of()[0].eligibility, RemoteEligibility::Eligible);
}


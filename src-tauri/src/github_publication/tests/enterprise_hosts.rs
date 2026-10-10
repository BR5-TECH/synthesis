//! GitHub Enterprise hosts in publication (GHP-FR-BXTU, GHP-FR-MZPR,
//! GHP-FR-ZRFP, GHP-FR-WNJC, GHP-FR-MJTB, GHP-FR-HSTB, GHP-FR-HSTC).

use super::*;

const GHE: &str = "https://company.ghe.com/acme/widgets.git";

fn classify_for(
    remotes_: &[ConfiguredRemote],
    token: Option<(&str, &str)>,
    known: &[String],
    fake: &FakeGithub,
) -> Vec<PublicationRemote> {
    remotes::classify_hosted(remotes_, token, known, fake, false)
}

// GHP-FR-BXTU
#[test]
fn every_url_form_of_an_enterprise_repository_canonicalizes_to_one_value() {
    for form in [
        "https://company.ghe.com/acme/widgets.git",
        "https://user:secret@Company.GHE.com:443/acme/widgets/",
        "git@company.ghe.com:acme/widgets.git",
        "ssh://git@company.ghe.com/acme/widgets",
    ] {
        assert_eq!(remotes::canonical_url(form), "company.ghe.com/acme/widgets", "{form}");
        let parsed = remotes::parse_remote_repository(form).unwrap();
        assert_eq!(parsed.host, "company.ghe.com");
        assert_eq!((parsed.owner.as_str(), parsed.repo.as_str()), ("acme", "widgets"));
    }
    // The legacy parser still answers for `github.com` alone.
    assert_eq!(remotes::parse_github_remote(GHE), None);
}

// GHP-FR-BXTU, GHP-FR-MZPR, GHP-FR-HSTA
#[test]
fn a_remote_on_the_host_of_the_token_is_classified_and_probed() {
    let fake = FakeGithub::with_probe(ProbeOutcome::Publishable);
    let classified = classify_for(
        &[configured("origin", GHE)],
        Some(("secret", "company.ghe.com")),
        &[],
        &fake,
    );
    assert_eq!(classified[0].kind, RemoteKind::Github);
    assert_eq!(classified[0].eligibility, RemoteEligibility::Eligible);
    assert_eq!(classified[0].repository_host.as_deref(), Some("company.ghe.com"));
    assert_eq!(classified[0].url, "company.ghe.com/acme/widgets");
    assert_eq!(fake.calls(), vec![Call::Probe("acme/widgets".into())]);
}

// GHP-FR-MZPR, GTS-FR-OBAS
#[test]
fn a_remote_on_another_host_than_the_token_is_a_host_mismatch_with_no_request() {
    let fake = FakeGithub::with_probe(ProbeOutcome::Publishable);
    let classified = classify_for(
        &[
            configured("origin", GHE),
            configured("upstream", "https://github.com/acme/widgets.git"),
        ],
        Some(("secret", "company.ghe.com")),
        &[],
        &fake,
    );
    assert_eq!(classified[0].eligibility, RemoteEligibility::Eligible);
    assert_eq!(classified[1].kind, RemoteKind::Github);
    assert_eq!(classified[1].eligibility, RemoteEligibility::HostMismatch);
    assert_eq!(classified[1].refusal_code(), "github_host_mismatch");
    assert!(classified[1].reason.as_deref().unwrap().contains("another GitHub host"));
    assert_eq!(fake.calls().len(), 1, "only the remote on the token host is probed");
}

// GHP-FR-ZRFP, GHP-FR-WNJC
#[test]
fn the_refusal_names_a_host_mismatch_before_a_missing_token() {
    let fake = FakeGithub::with_probe(ProbeOutcome::Publishable);
    let only_mismatch = classify_for(
        &[configured("origin", GHE)],
        Some(("secret", "github.com")),
        &[],
        &fake,
    );
    assert_eq!(remotes::refusal_for(&only_mismatch), "github_host_mismatch");
    assert!(fake.calls().is_empty());
    assert!(remotes::refusal_reason("github_host_mismatch").contains("another GitHub host"));

    let mixed = classify_for(
        &[configured("a", GHE), configured("b", "https://github.com/acme/widgets")],
        None,
        &[],
        &fake,
    );
    assert_eq!(remotes::refusal_for(&mixed), "token_unavailable");
}

// GHP-FR-BXTU, GHP-FR-LTAC
#[test]
fn a_host_is_github_when_a_stored_token_names_it_and_other_hosts_stay_other() {
    let fake = FakeGithub::with_probe(ProbeOutcome::Publishable);
    let corp = "https://git.corp.example/acme/widgets.git";
    let known = vec!["git.corp.example".to_string()];

    let github = classify_for(&[configured("origin", corp)], Some(("s", "git.corp.example")), &[], &fake);
    assert_eq!(github[0].eligibility, RemoteEligibility::Eligible, "the token host is a GitHub host");

    let by_registry = classify_for(&[configured("origin", corp)], Some(("s", "github.com")), &known, &fake);
    assert_eq!(by_registry[0].eligibility, RemoteEligibility::HostMismatch);

    let other = classify_for(&[configured("origin", corp)], Some(("s", "github.com")), &[], &fake);
    assert_eq!(other[0].kind, RemoteKind::Other);
    assert_eq!(other[0].eligibility, RemoteEligibility::NotGithub);
    assert_eq!(other[0].repository_host, None);
}

// GHP-FR-BXTU
#[test]
fn the_remote_wire_shape_carries_the_repository_host() {
    let fake = FakeGithub::with_probe(ProbeOutcome::Publishable);
    let classified = classify_for(&[configured("origin", GHE)], Some(("s", "company.ghe.com")), &[], &fake);
    let wire = serde_json::to_value(&classified[0]).unwrap();
    assert_eq!(wire["repositoryHost"], "company.ghe.com");
    assert_eq!(wire["eligibility"], "eligible");
    let mismatch = classify_for(&[configured("origin", GHE)], Some(("s", "github.com")), &[], &fake);
    assert_eq!(serde_json::to_value(&mismatch[0]).unwrap()["eligibility"], "host_mismatch");
}

// GHP-FR-MJTB
#[test]
fn an_issue_page_opens_only_on_the_repository_host_of_its_record() {
    assert!(flow::is_openable_issue_url("https://company.ghe.com/acme/widgets/issues/4", "company.ghe.com"));
    assert!(!flow::is_openable_issue_url("https://github.com/acme/widgets/issues/4", "company.ghe.com"));
    assert!(!flow::is_openable_issue_url("https://company.ghe.com.evil.example/acme/w/issues/4", "company.ghe.com"));
    assert!(!flow::is_openable_issue_url("https://user@company.ghe.com/acme/w/issues/4", "company.ghe.com"));
    assert!(!flow::is_openable_issue_url("file:///etc/passwd", "company.ghe.com"));
}

// GHP-FR-HSTB
#[test]
fn a_record_and_an_attempt_stored_without_a_host_read_as_github_com() {
    let record: PublicationRecord = serde_json::from_value(serde_json::json!({
        "provider": "github",
        "repositoryOwner": "acme",
        "repositoryName": "widgets",
        "issueNumber": 4,
        "issueUrl": "https://github.com/acme/widgets/issues/4",
        "publishedAt": "2026-01-01T00:00:00Z",
        "marker": "m",
    }))
    .unwrap();
    assert_eq!(record.repository_host, "github.com");

    let attempt: PublicationAttempt = serde_json::from_value(serde_json::json!({
        "marker": "m",
        "remoteName": "origin",
        "remoteUrl": "github.com/acme/widgets",
        "repositoryOwner": "acme",
        "repositoryName": "widgets",
        "state": "open",
        "startedAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-01T00:00:00Z",
    }))
    .unwrap();
    assert_eq!(attempt.repository_host, "github.com");
}

// GHP-FR-HSTB
#[test]
fn an_attempt_and_its_record_keep_the_host_of_the_repository() {
    let remote = PublicationRemote {
        repository_host: Some("company.ghe.com".into()),
        url: "company.ghe.com/acme/widgets".into(),
        ..origin()
    };
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "Hosted");
    let attempt = flow::open_attempt_with(&root, &id, &remote, "marker".into(), None).unwrap();
    assert_eq!(attempt.repository_host, "company.ghe.com");
    let issue = IssueRef {
        number: 4,
        title: "t".into(),
        body: String::new(),
        url: "https://company.ghe.com/acme/widgets/issues/4".into(),
        ..Default::default()
    };
    assert_eq!(flow::record_of(&attempt, &issue).repository_host, "company.ghe.com");
}

// GHP-FR-HSTC
#[test]
fn a_repository_resolves_with_its_host_for_the_polling_module() {
    let fake = FakeGithub::with_probe(ProbeOutcome::Publishable);
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let resolved = flow::resolve_repository_hosted(
        &root,
        &[configured("origin", GHE)],
        Some(("secret", "company.ghe.com")),
        &[],
        &fake,
    )
    .unwrap();
    assert_eq!(resolved.repository_host, "company.ghe.com");
    assert_eq!(resolved.repository_owner, "acme");

    let refused = flow::resolve_repository_hosted(
        &root,
        &[configured("origin", GHE)],
        Some(("secret", "github.com")),
        &[],
        &fake,
    );
    assert_eq!(refused, Err("github_host_mismatch".to_string()));
}

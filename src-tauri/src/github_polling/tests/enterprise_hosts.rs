//! GitHub Enterprise hosts in polling (GPP-FR-WOIM, GPP-FR-HSTD, GPP-FR-HSTE,
//! GPP-FR-WOLE, GPP-FR-RGNM).

use super::*;

fn ghe_repo() -> RepositoryRef {
    RepositoryRef { host: "company.ghe.com".into(), owner: "acme".into(), name: "widgets".into() }
}

// GPP-FR-HSTE, GPP-FR-XSKT
#[test]
fn a_ready_row_keeps_the_host_of_the_polling_repository() {
    let rows = eligible_tasks(&[ready(7)], &ghe_repo());
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].repository_host, "company.ghe.com");
    let wire = serde_json::to_value(&rows[0]).unwrap();
    assert_eq!(wire["repositoryHost"], "company.ghe.com");
}

// GPP-FR-WOLE, GPP-FR-HSTE
#[test]
fn an_issue_opens_only_on_the_host_of_the_polling_repository() {
    let repository = ghe_repo();
    assert!(is_repository_issue_url("https://company.ghe.com/acme/widgets/issues/7", &repository));
    assert!(!is_repository_issue_url("https://github.com/acme/widgets/issues/7", &repository));
    assert!(!is_repository_issue_url(
        "https://company.ghe.com.evil.example/acme/widgets/issues/7",
        &repository
    ));
    assert!(!is_repository_issue_url("https://company.ghe.com/acme/other/issues/7", &repository));
    assert!(is_repository_issue_url("https://github.com/acme/widgets/issues/7", &repo()));
}

// GPP-FR-WOLE
#[test]
fn a_listed_enterprise_issue_resolves_to_its_url_and_a_foreign_url_is_refused() {
    let repository = ghe_repo();
    let mut rows = eligible_tasks(&[ready(7)], &repository);
    rows[0].url = "https://company.ghe.com/acme/widgets/issues/7".into();
    assert_eq!(
        listed_issue_url(7, &repository, &rows, &[]).unwrap(),
        "https://company.ghe.com/acme/widgets/issues/7"
    );
    rows[0].url = "https://github.com/acme/widgets/issues/7".into();
    assert_eq!(
        listed_issue_url(7, &repository, &rows, &[]).unwrap_err(),
        ERR_ISSUE_NOT_LISTED
    );
}

// GPP-FR-HSTE, PSS-FR-TXJV
#[test]
fn a_claim_and_a_link_stored_without_a_host_read_as_github_com() {
    let claim: crate::project_settings::GithubPendingClaim = serde_json::from_value(serde_json::json!({
        "repositoryOwner": "acme",
        "repositoryName": "widgets",
        "issueNumber": 7,
        "issueUrl": "https://github.com/acme/widgets/issues/7",
        "projectNodeId": "PVT_1",
        "claimedAt": "2026-10-02T09:00:00Z",
    }))
    .unwrap();
    assert_eq!(claim.repository_host, "github.com");

    let link: crate::drafts::GithubIssueLink = serde_json::from_value(serde_json::json!({
        "repositoryOwner": "acme",
        "repositoryName": "widgets",
        "issueNumber": 7,
        "issueUrl": "https://github.com/acme/widgets/issues/7",
        "projectNodeId": "PVT_1",
        "claimState": "claimed",
    }))
    .unwrap();
    assert_eq!(link.repository_host, "github.com");
}

// GPP-FR-HSTE, GPP-FR-DHQM
#[test]
fn a_claim_on_an_enterprise_host_keeps_the_host_in_the_claim_and_the_shadow_link() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let repository = ghe_repo();
    let fake = FakeProjects::new();
    fake.hold_issue(7, "Ready", Some("Task"), "OPEN");
    let outcome = super::super::claims::claim(
        &context(&root, &repository),
        &fake,
        7,
        "2026-10-02T09:00:00Z",
        &[],
    )
    .unwrap();
    assert_eq!(outcome.claim.repository_host, "company.ghe.com");
    let link = crate::drafts::github_issue_link(&root, &outcome.result.draft_id).unwrap();
    assert_eq!(link.repository_host, "company.ghe.com");
}

fn ghe_task(number: u64) -> GithubReadyTask {
    eligible_tasks(&[ready(number)], &ghe_repo()).remove(0)
}

fn on_ghe<T>(mut link: crate::drafts::GithubIssueLink, then: impl FnOnce(&mut crate::drafts::GithubIssueLink) -> T) -> T {
    link.repository_host = "company.ghe.com".into();
    then(&mut link)
}

// GPP-FR-QCAM, GPP-FR-HSTE
#[test]
fn a_shadow_or_a_claim_on_one_host_does_not_hide_the_same_issue_of_another_host() {
    let github_task = eligible_tasks(&[ready(7)], &repo()).remove(0);
    let enterprise_task = ghe_task(7);

    let shadows = [link(7)];
    let claims = [pending(7, Some("d1"))];
    assert!(exclude_claimed(vec![github_task.clone()], &shadows, &claims).is_empty());
    assert_eq!(
        exclude_claimed(vec![enterprise_task.clone()], &shadows, &claims),
        vec![enterprise_task.clone()],
        "a github.com shadow and claim leave the enterprise task"
    );

    let enterprise_shadows = [on_ghe(link(7), |l| l.clone())];
    let enterprise_claims = [{
        let mut claim = pending(7, Some("d1"));
        claim.repository_host = "Company.GHE.com".into();
        claim
    }];
    assert!(exclude_claimed(vec![enterprise_task.clone()], &enterprise_shadows, &[]).is_empty());
    assert!(
        exclude_claimed(vec![enterprise_task], &[], &enterprise_claims).is_empty(),
        "the host match ignores case"
    );
    assert_eq!(
        exclude_claimed(vec![github_task.clone()], &enterprise_shadows, &enterprise_claims),
        vec![github_task],
        "an enterprise shadow and claim leave the github.com task"
    );
}

// GPP-FR-YROY, GPP-FR-HSTE
#[test]
fn the_same_issue_of_two_hosts_has_two_identities_and_two_claim_locks() {
    assert_ne!(IssueKey::of_task(&ghe_task(7)), IssueKey::of_task(&eligible_tasks(&[ready(7)], &repo())[0]));
    assert_eq!(IssueKey::new("Company.GHE.com", "ACME", "Widgets", 7), IssueKey::of_task(&ghe_task(7)));

    let guards = ClaimGuards::default();
    let held = guards.acquire(IssueKey::new("github.com", "acme", "widgets", 7)).unwrap();
    assert!(
        guards.acquire(IssueKey::new("company.ghe.com", "acme", "widgets", 7)).is_ok(),
        "the other host runs at the same time"
    );
    assert_eq!(
        guards.acquire(IssueKey::new("GitHub.com", "acme", "widgets", 7)).err(),
        Some(ERR_CLAIM_IN_PROGRESS.to_string())
    );
    drop(held);
}

// GPP-FR-XPUO, GPP-FR-CWGH, GPP-FR-HSTE
#[test]
fn a_claim_does_not_reuse_a_shadow_or_meet_a_pending_claim_of_another_host() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let github_shadow = crate::drafts::create_github_shadow_draft(&root, "t", "b", link(7))
        .expect("the github.com shadow draft is created");
    crate::project_settings::save_github_pending_claims_to(&root, &[pending(7, Some(github_shadow.draft.id.as_str()))]).unwrap();

    let repository = ghe_repo();
    let fake = FakeProjects::new();
    fake.hold_issue(7, "Ready", Some("Task"), "OPEN");
    let outcome = super::super::claims::claim(
        &context(&root, &repository),
        &fake,
        7,
        "2026-10-02T09:00:00Z",
        &[],
    )
    .expect("the github.com claim of the same number does not block the enterprise claim");
    assert!(outcome.created, "a new shadow draft is made");
    assert_ne!(outcome.result.draft_id, github_shadow.draft.id);
    let created = crate::drafts::github_issue_link(&root, &outcome.result.draft_id).unwrap();
    assert_eq!(created.repository_host, "company.ghe.com");
    assert_eq!(
        crate::drafts::find_github_shadow(&root, "github.com", "acme", "widgets", 7).unwrap().id,
        github_shadow.draft.id
    );
    assert_eq!(
        crate::drafts::find_github_shadow(&root, "company.ghe.com", "acme", "widgets", 7).unwrap().id,
        outcome.result.draft_id
    );

    // Acknowledging the enterprise claim leaves the github.com claim.
    super::super::claims::acknowledge(&root, &repository, 7).unwrap();
    let left = crate::project_settings::load_github_pending_claims_from(&root);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].repository_host, "github.com");
}

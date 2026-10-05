//! The decisions the HTTP client makes, taken apart from the requests it makes
//! (GHP-FR-MZPR, GHP-FR-FQIZ, GHP-FR-PVOA, GHP-FR-DHXK).

use super::super::client::{
    exact_match, issue_capability, issue_from_json, scopes_from_header, transport_code,
    urlencode, ProbeOutcome,
};
use super::*;

/// GHP-FR-TKBW: a repository accepts an issue only where Issues are on and the
/// repository is not archived, and that answer is about the repository alone.
#[test]
fn a_repository_that_takes_no_issue_answers_for_the_repository_not_the_token() {
    let on = serde_json::json!({ "has_issues": true });
    assert_eq!(issue_capability(&on, None), ProbeOutcome::Publishable);

    // Issues turned off. The strongest possible token changes nothing, so the
    // answer must not be the one that names the token.
    let off = serde_json::json!({
        "has_issues": false,
        "permissions": { "admin": true, "push": true },
    });
    assert_eq!(issue_capability(&off, None), ProbeOutcome::IssuesDisabled);
    let scoped = ["repo".to_string()];
    assert_eq!(issue_capability(&off, Some(&scoped)), ProbeOutcome::IssuesDisabled);

    // An archived repository keeps Issues on and still takes no new issue, and
    // so does one GitHub itself has disabled.
    let archived = serde_json::json!({ "has_issues": true, "archived": true });
    assert_eq!(issue_capability(&archived, Some(&scoped)), ProbeOutcome::IssuesDisabled);
    let disabled = serde_json::json!({ "has_issues": true, "disabled": true });
    assert_eq!(issue_capability(&disabled, Some(&scoped)), ProbeOutcome::IssuesDisabled);

    // A record with no `has_issues` at all says nothing about either party, and
    // neither does one whose `has_issues` is not a boolean.
    assert_eq!(issue_capability(&serde_json::json!({}), None), ProbeOutcome::IssuesUnreadable);
    let malformed = serde_json::json!({ "has_issues": "true" });
    assert_eq!(issue_capability(&malformed, None), ProbeOutcome::IssuesUnreadable);
}

/// GHP-FR-RQDV: issue-write capability comes from the token's granted scopes.
/// Code-write permission is never required, and an absent scope header is never
/// read as a refusal.
#[test]
fn a_token_may_write_issues_without_any_write_permission_on_the_repository() {
    // The regression this guards: an author who may only read a repository may
    // still open an issue in it, so `pull`-only access is publishable.
    let read_only = serde_json::json!({
        "has_issues": true,
        "permissions": { "pull": true, "push": false, "triage": false },
    });
    let scoped = ["repo".to_string()];
    assert_eq!(issue_capability(&read_only, Some(&scoped)), ProbeOutcome::Publishable);
    // A record carrying no `permissions` at all is publishable just the same.
    let bare = serde_json::json!({ "has_issues": true });
    assert_eq!(issue_capability(&bare, Some(&scoped)), ProbeOutcome::Publishable);

    // No scope header is a fine-grained token, which is not inferred against.
    assert_eq!(issue_capability(&read_only, None), ProbeOutcome::Publishable);

    // `public_repo` reaches a public repository and not a private one.
    let public = ["public_repo".to_string()];
    let private_repo = serde_json::json!({ "has_issues": true, "private": true });
    let public_repo = serde_json::json!({ "has_issues": true, "private": false });
    assert_eq!(issue_capability(&public_repo, Some(&public)), ProbeOutcome::Publishable);
    assert_eq!(issue_capability(&private_repo, Some(&public)), ProbeOutcome::CreateForbidden);
    assert_eq!(issue_capability(&private_repo, Some(&scoped)), ProbeOutcome::Publishable);

    // A scope list holding none that may write an issue. `repo:status` and
    // `repo_deployment` are their own scopes, so the match is exact and never a
    // prefix of `repo`. The same list with `repo` added writes again, so the
    // case proves the exact match rather than a blanket refusal.
    let near = ["repo:status".to_string(), "repo_deployment".to_string()];
    assert_eq!(issue_capability(&public_repo, Some(&near)), ProbeOutcome::CreateForbidden);
    let near_plus = ["repo:status".to_string(), "repo".to_string()];
    assert_eq!(issue_capability(&public_repo, Some(&near_plus)), ProbeOutcome::Publishable);

    // A record with no `private` at all is read as public. The permissive
    // direction is deliberate: this check never manufactures a refusal from a
    // field GitHub did not send.
    let no_visibility = serde_json::json!({ "has_issues": true });
    assert_eq!(issue_capability(&no_visibility, Some(&public)), ProbeOutcome::Publishable);
}

/// GHP-FR-RQDV: "reports no scopes at all" covers both an absent header and one
/// that arrives empty, so neither is ever read as a refusal.
#[test]
fn a_header_naming_no_scope_reports_none_however_it_arrives() {
    // A fine-grained token sends no header at all.
    assert_eq!(scopes_from_header(None), None);
    // A classic token that names no scope sends the header empty, or holding
    // nothing but separators. Answering `Some([])` here would refuse it, which
    // is the misattribution this whole check exists to remove.
    assert_eq!(scopes_from_header(Some("")), None);
    assert_eq!(scopes_from_header(Some("  ")), None);
    assert_eq!(scopes_from_header(Some(" , ,")), None);
    // A header that names scopes reports them, trimmed.
    assert_eq!(
        scopes_from_header(Some("repo, workflow")),
        Some(vec!["repo".to_string(), "workflow".to_string()])
    );
}

/// GHP-FR-FQIZ: the marker is confirmed by exact substring against a
/// candidate's own body, because GitHub's search answers approximately.
#[test]
fn a_candidate_is_accepted_only_where_its_body_holds_the_marker_exactly() {
    let issue = |number: u64, body: &str| {
        serde_json::json!({
            "number": number,
            "html_url": format!("https://github.com/acme/widgets/issues/{number}"),
            "title": "widget",
            "body": body,
        })
    };
    let items = vec![
        // A near miss the full-text index would happily return.
        issue(1, "<!-- synthesis-publication-marker: pub-0001 -->"),
        issue(2, "text\n<!-- synthesis-publication-marker: pub-0002 -->"),
    ];
    assert_eq!(exact_match(&items, "pub-0002").unwrap().number, 2);
    assert_eq!(exact_match(&items, "pub-9999"), None);
    // A payload missing what an issue is identified by contributes nothing
    // rather than a half-built record.
    assert_eq!(issue_from_json(&serde_json::json!({ "title": "x" })), None);
}

/// GHP-FR-PVOA / GHP-FR-DHXK: a transport failure becomes one of the module's
/// own codes rather than the client's own message, which echoes the request it
/// made and is the one place a credential could surface.
#[test]
fn a_transport_failure_becomes_a_typed_code_and_never_a_message() {
    assert_eq!(transport_code(&ureq::Error::StatusCode(401)), ERR_ISSUES_CREATE_FORBIDDEN);
    assert_eq!(transport_code(&ureq::Error::StatusCode(403)), ERR_ISSUES_CREATE_FORBIDDEN);
    assert_eq!(transport_code(&ureq::Error::StatusCode(404)), ERR_ISSUES_INACCESSIBLE);
    assert_eq!(transport_code(&ureq::Error::StatusCode(500)), ERR_GITHUB_UNREACHABLE);
    assert_eq!(transport_code(&ureq::Error::TooManyRedirects), ERR_GITHUB_UNREACHABLE);
}

/// The search query is escaped, so a marker and a repository address reach
/// GitHub as one term rather than as several.
#[test]
fn a_search_query_is_percent_encoded() {
    assert_eq!(
        urlencode("repo:acme/widgets in:body \"pub-1\""),
        "repo%3Aacme%2Fwidgets%20in%3Abody%20%22pub-1%22"
    );
    assert_eq!(urlencode("pub-1a_2.3~4"), "pub-1a_2.3~4");
}

// ---------------------------------------------------------------------------
// Parent, Type, and milestone reads and requests
// ---------------------------------------------------------------------------

use super::super::client::{
    is_parent_candidate, issue_payload, issue_type_names, milestone_from_json, parent_from_url,
    IssueFields,
};

/// GHP-FR-CMPR / GHP-FR-OHGY: an issue answer carries the id, the state, the
/// Type, the milestone, and the parent the comparison and the link rest on.
#[test]
fn an_issue_answer_is_read_for_its_publication_metadata() {
    let issue = issue_from_json(&serde_json::json!({
        "number": 12,
        "id": 9001,
        "html_url": "https://github.com/acme/widgets/issues/12",
        "title": "t",
        "body": "b",
        "state": "open",
        "type": { "name": "Task" },
        "milestone": { "number": 3, "title": "v1" },
        "parent_issue_url": "https://api.github.com/repos/acme/widgets/issues/4",
    }))
    .unwrap();
    assert_eq!(issue.id, 9001);
    assert!(issue.open && !issue.is_pull_request);
    assert_eq!(issue.issue_type.as_deref(), Some("Task"));
    assert_eq!(issue.milestone.unwrap().number, 3);
    let parent = issue.parent.unwrap();
    assert!(parent.names("ACME", "widgets", 4), "the repository match ignores case");
    assert!(!parent.names("acme", "widgets", 5));

    let bare = issue_from_json(&serde_json::json!({
        "number": 1, "html_url": "u", "state": "closed", "pull_request": {}, "type": null, "milestone": null,
    }))
    .unwrap();
    assert!(!bare.open && bare.is_pull_request);
    assert_eq!((bare.issue_type, bare.milestone, bare.parent), (None, None, None));
}

/// GHP-FR-CMPR: only a well-formed repository issue address names a parent.
#[test]
fn a_parent_address_is_parsed_strictly() {
    let parent = parent_from_url("https://api.github.com/repos/acme/widgets/issues/4").unwrap();
    assert_eq!((parent.owner.as_str(), parent.repo.as_str(), parent.number), ("acme", "widgets", 4));
    for bad in ["", "https://api.github.com/repos/acme/widgets/pulls/4", "https://api.github.com/repos/acme", "https://api.github.com/repos/acme/widgets/issues/x"] {
        assert!(parent_from_url(bad).is_none(), "{bad}");
    }
}

/// GHP-FR-FTMC: a parent candidate is open, not a pull request, and of a
/// configured Type, compared without regard to case.
#[test]
fn a_parent_candidate_is_open_typed_and_not_a_pull_request() {
    let mut issue = issue_from_json(&serde_json::json!({
        "number": 1, "html_url": "u", "state": "open", "type": { "name": "feature" },
    }))
    .unwrap();
    let types = vec!["Feature".to_string()];
    assert!(is_parent_candidate(&issue, &types));
    issue.is_pull_request = true;
    assert!(!is_parent_candidate(&issue, &types));
    issue.is_pull_request = false;
    issue.open = false;
    assert!(!is_parent_candidate(&issue, &types));
    issue.open = true;
    assert!(!is_parent_candidate(&issue, &["Bug".to_string()]));
    issue.issue_type = None;
    assert!(!is_parent_candidate(&issue, &types));
}

/// GHP-FR-YSPJ: the Type names of an owner's answer, a switched-off Type left
/// out; a milestone answer is read for its number and title.
#[test]
fn the_type_and_milestone_answers_are_read() {
    let types = issue_type_names(&serde_json::json!([
        { "name": "Task", "is_enabled": true },
        { "name": "Bug" },
        { "name": "Retired", "is_enabled": false },
        { "name": "  " },
    ]));
    assert_eq!(types, vec!["Task".to_string(), "Bug".to_string()]);
    assert!(issue_type_names(&serde_json::json!({ "message": "Not Found" })).is_empty());

    let milestone = milestone_from_json(&serde_json::json!({ "number": 7, "title": "v1.2", "state": "open" })).unwrap();
    assert_eq!((milestone.number, milestone.title.as_str()), (7, "v1.2"));
    assert!(milestone_from_json(&serde_json::json!({ "title": "x" })).is_none());
}

/// GHP-FR-PLQE / GHP-FR-OHGY: the request names a Type and a milestone only
/// when the choice does, so a plain root issue sends the title and body alone.
#[test]
fn the_request_body_names_only_what_the_choice_names() {
    assert_eq!(
        issue_payload("t", "b", &IssueFields::default()),
        serde_json::json!({ "title": "t", "body": "b" })
    );
    assert_eq!(
        issue_payload("t", "b", &IssueFields { issue_type: Some("Task".into()), milestone: Some(7) }),
        serde_json::json!({ "title": "t", "body": "b", "type": "Task", "milestone": 7 })
    );
    assert_eq!(urlencode("Feature request"), "Feature%20request");
}

/// GHP-FR-PVOA / GHP-FR-DZLB: the new metadata errors have fixed displayable
/// text that carries no transport detail.
#[test]
fn the_metadata_errors_have_fixed_text() {
    for code in [ERR_PARENT_ISSUES_UNREADABLE, ERR_ISSUE_TYPES_UNREADABLE, ERR_MILESTONES_UNREADABLE] {
        let list = MetadataList::<u8>::failed(code);
        assert_eq!(list.error_code.as_deref(), Some(code));
        let text = list.error.unwrap();
        assert!(!text.is_empty() && !text.contains("http"));
    }
}

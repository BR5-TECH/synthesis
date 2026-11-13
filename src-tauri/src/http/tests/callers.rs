//! Each family of callers of the one HTTP path against loopback servers: a
//! 4xx or 5xx answer keeps the classification of that caller (AAP-FR-UXUI),
//! and a credential goes to no other host on a redirect (AAP-FR-GMVE).

use super::*;

use crate::ai_shared::{
    AuthStyle, EndpointProber, HttpEndpointProber, ModelsFormat, ProbeError, ProbeRequest,
};

const SECRET: &str = "secret-token";

fn probe(
    base_url: &str,
    auth: AuthStyle,
) -> Result<Vec<crate::ai_shared::ModelOption>, ProbeError> {
    HttpEndpointProber.probe(&ProbeRequest {
        base_url,
        api_key: Some(SECRET),
        auth,
        models_path: "/models",
        models_format: ModelsFormat::Lenient,
    })
}

// AAP-FR-UXUI, AAP-FR-05: the endpoint prober reads a status as the status.
#[test]
fn the_endpoint_prober_classifies_a_4xx_or_5xx_answer() {
    for (status, expected) in [
        (401, ProbeError::Rejected),
        (403, ProbeError::Rejected),
        (404, ProbeError::NotExpectedKind),
        (503, crate::ai_shared::probe_failure_for_status(503)),
    ] {
        let server = Loopback::serve(vec![json_reply(status, r#"{"data":[{"id":"m"}]}"#)]);
        assert_eq!(probe(&server.url(""), AuthStyle::Bearer), Err(expected), "status {status}");
        server.requests();
    }
}

// AAP-FR-GMVE: the `x-api-key` of an Anthropic probe goes to no other host on
// a redirect.
#[test]
fn the_api_key_of_a_probe_goes_to_no_other_host_on_a_redirect() {
    let other = Loopback::serve(vec![json_reply(200, r#"{"data":[{"id":"m"}]}"#)]);
    let first = Loopback::serve(vec![reply(307, &[("location", &other.url("/models"))], b"")]);
    let models = probe(&format!("http://localhost:{}", first.port), AuthStyle::AnthropicApiKey)
        .expect("the redirect is followed");
    assert_eq!(models.len(), 1);
    let asked = first.requests();
    assert_eq!(header_of(&asked[0], "x-api-key").as_deref(), Some(SECRET));
    let landed = other.requests();
    assert_eq!(header_of(&landed[0], "x-api-key"), None, "{}", landed[0]);
    assert_eq!(header_of(&landed[0], "anthropic-version").as_deref(), Some("2023-06-01"));
    assert!(!landed[0].contains(SECRET), "{}", landed[0]);
}

// AAP-FR-UXUI, AAP-FR-TIZO, AAP-FR-QRAX: the model detector reads a status as
// the status, and a body that is too long as a body it cannot use.
#[test]
fn the_model_detector_classifies_a_status_and_a_body_that_is_too_long() {
    use crate::ai_api::detection::{DetectionFailure, HttpModelDetector, ModelDetector};
    let server = Loopback::serve(vec![json_reply(429, "{}")]);
    assert_eq!(
        HttpModelDetector::new().show(&server.url(""), Some(SECRET), "m"),
        Err(DetectionFailure::Status(429))
    );
    server.requests();

    let server = Loopback::serve(vec![reply(200, &[], &vec![b' '; MAX_BODY_BYTES + 1])]);
    assert_eq!(
        HttpModelDetector::new().show(&server.url(""), Some(SECRET), "m"),
        Err(DetectionFailure::BadBody)
    );
    server.requests();
}

// AAP-FR-UXUI, GSS-FR-HPWE: a relay that answers with a status is not a
// relay, and a body that is too long is not a relay's answer.
#[test]
fn the_relay_check_classifies_a_status_and_a_body_that_is_too_long() {
    use crate::relay_endpoint::{HttpRelayProbe, RelayProbe, ERR_NOT_A_RELAY, ERR_UNREACHABLE};
    for status in [404, 500] {
        let server = Loopback::serve(vec![json_reply(status, "{}")]);
        assert_eq!(
            HttpRelayProbe.health(&server.url("/v1/health")).unwrap_err(),
            ERR_NOT_A_RELAY,
            "status {status}"
        );
        server.requests();
    }
    let server = Loopback::serve(vec![reply(200, &[], &vec![b' '; MAX_BODY_BYTES + 1])]);
    assert_eq!(HttpRelayProbe.health(&server.url("/v1/health")).unwrap_err(), ERR_UNREACHABLE);
    server.requests();
}

// AAP-FR-UXUI, GTS-FR-04: the token verifier reads 401 and 403 as a refused
// token and another status as no useful answer. A success gives the login and
// the scopes.
#[test]
fn the_token_verifier_classifies_a_status_and_reads_a_success() {
    use crate::github_tokens::{verify_at, VerifyError};
    for (status, rejected) in [(401, true), (403, true), (404, false), (500, false)] {
        let server = Loopback::serve(vec![json_reply(status, "{}")]);
        let error = verify_at(&server.url("/user"), SECRET).unwrap_err();
        server.requests();
        match (rejected, error) {
            (true, VerifyError::Rejected) | (false, VerifyError::Unreachable(_)) => {}
            (_, other) => panic!("status {status}: {other:?}"),
        }
    }
    let server = Loopback::serve(vec![reply(
        200,
        &[("x-oauth-scopes", "repo, read:org")],
        br#"{"login":"octo","name":"Octo Cat"}"#,
    )]);
    let identity = verify_at(&server.url("/user"), SECRET).unwrap();
    let requests = server.requests();
    assert_eq!(identity.login, "octo");
    assert_eq!(identity.scopes, ["repo", "read:org"]);
    assert_eq!(
        header_of(&requests[0], "authorization").as_deref(),
        Some(format!("Bearer {SECRET}").as_str())
    );
}

// AAP-FR-UXUI, GPP-FR-WKZF: the Projects client reads a status as a refused
// request.
#[test]
fn the_projects_client_classifies_a_4xx_or_5xx_answer() {
    use crate::github_polling::client::{GithubProjects, HttpGithubProjects};
    use crate::github_polling::records::ERR_REQUEST_FAILED;
    for status in [401, 502] {
        let server = Loopback::serve(vec![json_reply(status, "{}")]);
        let error = HttpGithubProjects::at(&server.url("/graphql"))
            .project_shape(SECRET, "PVT_1")
            .unwrap_err();
        assert_eq!(error, ERR_REQUEST_FAILED, "status {status}");
        server.requests();
    }
}

// AAP-FR-UXUI, GHP-FR-PVOA: the Issues client keeps its classification of
// each status.
#[test]
fn the_issues_client_classifies_a_4xx_or_5xx_answer() {
    use crate::github_publication::client::{GithubIssues, HttpGithubIssues, ProbeOutcome};
    use crate::github_publication::records::{
        ERR_GITHUB_UNREACHABLE, ERR_ISSUES_CREATE_FORBIDDEN, ERR_ISSUES_INACCESSIBLE,
        ERR_ISSUE_CREATE_FAILED,
    };
    let server = Loopback::serve(vec![
        json_reply(404, "{}"),
        json_reply(410, "{}"),
        json_reply(500, "{}"),
        json_reply(403, "{}"),
        json_reply(404, "{}"),
        json_reply(500, "{}"),
        json_reply(404, "{}"),
        json_reply(401, "{}"),
    ]);
    let issues = HttpGithubIssues::at(&server.url(""));
    assert_eq!(issues.get_issue(SECRET, "acme", "web", 7), Ok(None));
    assert_eq!(issues.get_issue(SECRET, "acme", "web", 7), Ok(None));
    assert_eq!(issues.get_issue(SECRET, "acme", "web", 7), Err(ERR_GITHUB_UNREACHABLE.into()));
    assert_eq!(
        issues.find_by_marker(SECRET, "acme", "web", "pub-1"),
        Err(ERR_ISSUES_CREATE_FORBIDDEN.into())
    );
    assert_eq!(
        issues.find_by_marker(SECRET, "acme", "web", "pub-1"),
        Err(ERR_ISSUES_INACCESSIBLE.into())
    );
    let fields = Default::default();
    assert_eq!(
        issues.create_issue(SECRET, "acme", "web", "t", "b", &fields).map(|i| i.number),
        Err(ERR_ISSUE_CREATE_FAILED.into())
    );
    assert_eq!(issues.list_issue_types(SECRET, "octo"), Ok(Vec::new()));
    assert_eq!(issues.probe(SECRET, "acme", "web"), ProbeOutcome::IssuesUnreadable);
    assert_eq!(server.requests().len(), 8);
}

// AAP-FR-GMVE, GHP-FR-SVXP: a moved issue is read on the same API host with
// the token, and the token goes to no other host.
#[test]
fn the_issues_client_follows_a_moved_issue_with_the_token_on_its_own_host_only() {
    use crate::github_publication::client::{GithubIssues, HttpGithubIssues};
    let issue = r#"{"number":8,"html_url":"https://github.com/acme/web/issues/8","state":"open"}"#;
    let other = Loopback::serve(vec![json_reply(200, issue)]);
    let server = Loopback::serve(vec![
        reply(301, &[("location", "/repos/acme/web/issues/8")], b""),
        json_reply(200, issue),
        reply(301, &[("location", &other.url("/repos/acme/web/issues/8"))], b""),
    ]);
    let issues = HttpGithubIssues::at(&server.url(""));
    assert_eq!(issues.get_issue(SECRET, "acme", "web", 7).unwrap().unwrap().number, 8);
    assert_eq!(issues.get_issue(SECRET, "acme", "web", 7).unwrap().unwrap().number, 8);
    let requests = server.requests();
    assert!(requests[1].starts_with("GET /repos/acme/web/issues/8 "), "{}", requests[1]);
    assert!(requests[1].contains(SECRET), "{}", requests[1]);
    let landed = other.requests();
    assert_eq!(header_of(&landed[0], "authorization"), None, "{}", landed[0]);
}

// AAP-FR-UXUI, FTS-FR-ZLFX: a write of the flow tracker adapter answered with
// a status fails with `github_unreachable`.
#[test]
fn the_flow_tracker_adapter_classifies_a_4xx_or_5xx_answer() {
    use crate::flow_sync::adapter::TrackerAdapter;
    use crate::flow_sync::github::GithubAdapter;
    use crate::flow_sync::records::ERR_GITHUB_UNREACHABLE;
    for status in [404, 422, 500] {
        let server = Loopback::serve(vec![json_reply(status, "{}")]);
        let url = server.url("");
        let github = GithubAdapter::at(&url, &format!("{url}/graphql"), "acme", "payments");
        github.remember_id(4, 9004);
        assert_eq!(
            github.remove_blocked_by(3, 4).unwrap_err(),
            ERR_GITHUB_UNREACHABLE,
            "status {status}"
        );
        server.requests();
    }
}

// AAP-FR-UXUI, GTC-FR-DWVY: the pull request client reads a 5xx answer as
// unreachable, on a read and on a write.
#[test]
fn the_pull_request_client_classifies_a_5xx_answer() {
    use crate::git::pull_requests::client::{
        get_at, write_at, GithubFailure, GithubWriteFailure, WriteMethod,
    };
    let server = Loopback::serve(vec![json_reply(500, "{}"), json_reply(503, "{}")]);
    let api = server.url("");
    assert_eq!(
        get_at(&api, SECRET, "/repos/acme/web/pulls").unwrap_err(),
        GithubFailure::Unreachable
    );
    assert_eq!(
        write_at(&api, WriteMethod::Post, SECRET, "/repos/acme/web/pulls", &serde_json::json!({}))
            .unwrap_err(),
        GithubWriteFailure::Unreachable
    );
    server.requests();
}

// AAP-FR-GMVE: a pull request write does not follow a redirect, so its token
// goes nowhere else. A read follows one, and the token goes with it only to
// its own host.
#[test]
fn the_pull_request_client_sends_its_token_to_no_other_host() {
    use crate::git::pull_requests::client::{get_at, write_at, GithubWriteFailure, WriteMethod};
    let other = Loopback::serve(vec![json_reply(200, "[]")]);
    let target = other.url("/x");
    let server = Loopback::serve(vec![
        reply(307, &[("location", &target)], b""),
        reply(302, &[("location", &target)], b""),
    ]);
    let api = server.url("");
    assert_eq!(
        write_at(&api, WriteMethod::Post, SECRET, "/repos/acme/web/pulls", &serde_json::json!({}))
            .unwrap_err(),
        GithubWriteFailure::Unreachable
    );
    assert_eq!(get_at(&api, SECRET, "/repos/acme/web/pulls"), Ok(serde_json::json!([])));
    assert_eq!(server.requests().len(), 2);
    let landed = other.requests();
    assert_eq!(landed.len(), 1, "the write is not repeated at the other host");
    assert_eq!(header_of(&landed[0], "authorization"), None, "{}", landed[0]);
}

// FTS-FR-ZLFX, AAP-FR-TIZO: a REST answer of the flow tracker adapter whose
// body breaks off reads as no JSON, as before. A body that is too long fails
// the request.
#[test]
fn the_flow_tracker_adapter_reads_a_cut_body_as_no_json_and_a_long_body_as_a_failure() {
    use crate::flow_sync::adapter::TrackerAdapter;
    use crate::flow_sync::github::GithubAdapter;
    use crate::flow_sync::records::ERR_GITHUB_UNREACHABLE;
    let cut = b"HTTP/1.1 200 OK\r\ncontent-length: 100\r\nconnection: close\r\n\r\n0123456789";
    let long = reply(200, &[], &vec![b' '; MAX_BODY_BYTES + 1]);
    let server = Loopback::serve(vec![cut.to_vec(), long]);
    let url = server.url("");
    let github = GithubAdapter::at(&url, &format!("{url}/graphql"), "acme", "payments");
    github.remember_id(4, 9004);
    assert_eq!(github.remove_blocked_by(3, 4), Ok(()));
    assert_eq!(github.remove_blocked_by(3, 4).unwrap_err(), ERR_GITHUB_UNREACHABLE);
    assert_eq!(server.requests().len(), 2);
}

// AAP-FR-05, GSS-FR-HPWE: the time limit covers the body. A server that sends
// the head and then stops is a probe that timed out.
#[test]
fn the_relay_check_and_the_endpoint_prober_time_out_on_a_body_that_stops() {
    use crate::relay_endpoint::{HttpRelayProbe, ERR_TIMED_OUT};
    let limit = Duration::from_millis(300);
    let server = Stalled::serve(STALLED_HEAD);
    let url = format!("http://127.0.0.1:{}/v1/health", server.port);
    assert_eq!(HttpRelayProbe.health_within(&url, limit).unwrap_err(), ERR_TIMED_OUT);
    server.release();

    let server = Stalled::serve(STALLED_HEAD);
    let base_url = format!("http://127.0.0.1:{}", server.port);
    let request = ProbeRequest {
        base_url: &base_url,
        api_key: Some(SECRET),
        auth: AuthStyle::Bearer,
        models_path: "/models",
        models_format: ModelsFormat::Lenient,
    };
    assert_eq!(HttpEndpointProber.probe_within(&request, limit), Err(ProbeError::TimedOut));
    server.release();
}

// GHP-FR-RCNL: the removal of a sub-issue is a `DELETE` with a JSON body that
// names the sub-issue.
#[test]
fn the_issues_client_unlinks_a_sub_issue_with_a_delete_that_carries_a_body() {
    use crate::github_publication::client::{GithubIssues, HttpGithubIssues};
    let server = Loopback::serve(vec![json_reply(200, "{}")]);
    let issues = HttpGithubIssues::at(&server.url(""));
    assert_eq!(issues.unlink_sub_issue(SECRET, "acme", "web", 5, 42), Ok(()));
    let requests = server.requests();
    assert!(
        requests[0].starts_with("DELETE /repos/acme/web/issues/5/sub_issue "),
        "{}",
        requests[0]
    );
    assert_eq!(header_of(&requests[0], "content-type").as_deref(), Some("application/json"));
    let body = requests[0].split_once("\r\n\r\n").map(|(_, b)| b).unwrap_or_default();
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(body).unwrap(),
        serde_json::json!({ "sub_issue_id": 42 })
    );
}

// GTS-FR-04: a token with a line break is not sent at all, and the verifier
// does not read that as a token that GitHub refused.
#[test]
fn the_token_verifier_sends_no_token_that_is_not_valid_in_http() {
    use crate::github_tokens::{verify_at, VerifyError};
    let server = Loopback::serve(vec![json_reply(200, r#"{"login":"octo"}"#)]);
    let error = verify_at(&server.url("/user"), "ghp_abc\nInjected: yes").unwrap_err();
    assert!(matches!(error, VerifyError::Unreachable(_)), "{error:?}");
    assert!(server.requests().is_empty(), "no request reaches the server");
}

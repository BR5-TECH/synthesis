//! AAP-FR-JGYD: a request uses the proxy that the environment names, and a
//! direct request or a request to this computer uses none.
//!
//! These tests set the proxy variables of the process. That is safe only
//! because nextest runs each test in a process of its own: under
//! `cargo test`, the variables would reach the tests that run beside them.

use super::*;

/// A name that no resolver knows (RFC 2606). A request to it can succeed only
/// through a proxy.
const NOWHERE: &str = "http://example.invalid/x";

/// Name the server `proxy` as the proxy of every plain HTTP request of this
/// process, with no exception.
pub(crate) fn use_proxy(proxy: &Loopback) {
    let url = format!("http://127.0.0.1:{}", proxy.port);
    for name in ["HTTP_PROXY", "http_proxy", "ALL_PROXY", "all_proxy"] {
        std::env::set_var(name, &url);
    }
    for name in ["NO_PROXY", "no_proxy", "HTTPS_PROXY", "https_proxy"] {
        std::env::remove_var(name);
    }
}

// AAP-FR-JGYD: a request that is not direct goes through the proxy that the
// environment names, in the absolute form a proxy reads.
#[test]
fn a_request_goes_through_the_proxy_that_the_environment_names() {
    let proxy = Loopback::serve(vec![json_reply(200, r#"{"via":"proxy"}"#)]);
    use_proxy(&proxy);
    let response = Request::get(NOWHERE, LIMIT).send().expect("the proxy answers");
    assert_eq!(response.text().unwrap(), r#"{"via":"proxy"}"#);
    let requests = proxy.requests();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].starts_with("GET http://example.invalid/x "), "{}", requests[0]);
}

// AAP-FR-JGYD: a direct request uses no proxy, also when the environment
// names one. The name does not resolve, so the request fails.
#[test]
fn a_direct_request_uses_no_proxy_that_the_environment_names() {
    let proxy = Loopback::serve(vec![json_reply(200, "{}")]);
    use_proxy(&proxy);
    let result = Request::get(NOWHERE, LIMIT).direct(true).send();
    assert_eq!(result.unwrap_err(), HttpError::Transport);
    assert!(proxy.requests().is_empty(), "the proxy got a request");
}

// AAP-FR-JGYD: a request to this computer uses no proxy, so a credential to a
// loopback host never goes through a proxy.
#[test]
fn a_request_to_this_computer_uses_no_proxy() {
    let proxy = Loopback::serve(vec![json_reply(200, r#"{"via":"proxy"}"#)]);
    use_proxy(&proxy);
    let target = Loopback::serve(vec![json_reply(200, r#"{"via":"target"}"#)]);
    let response = Request::get(&target.url("/x"), LIMIT).bearer("secret").send().unwrap();
    assert_eq!(response.text().unwrap(), r#"{"via":"target"}"#);
    assert_eq!(target.requests().len(), 1);
    assert!(proxy.requests().is_empty(), "the proxy got a request");
}

// AAP-FR-JGYD, FTS-FR-ARMD: the flow tracker adapter of a test is direct, so
// a proxy that the environment names never sees its requests.
#[test]
fn the_direct_flow_tracker_adapter_uses_no_proxy() {
    use crate::flow_sync::adapter::TrackerAdapter;
    use crate::flow_sync::github::GithubAdapter;
    use crate::flow_sync::records::ERR_GITHUB_UNREACHABLE;
    let proxy = Loopback::serve(vec![json_reply(200, "{}")]);
    use_proxy(&proxy);
    let base = "http://example.invalid";
    let github = GithubAdapter::at(base, &format!("{base}/graphql"), "acme", "payments");
    github.remember_id(4, 9004);
    assert_eq!(github.remove_blocked_by(3, 4).unwrap_err(), ERR_GITHUB_UNREACHABLE);
    assert!(proxy.requests().is_empty(), "the proxy got a request");
}

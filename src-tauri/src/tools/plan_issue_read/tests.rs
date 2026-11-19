//! Tests for `PIR-plan-issue-read-tools.md`, against the fake GitHub tracker and
//! a mock application with a project open. No test reaches a network.

use rig::tool::{PortableTool, ToolErrorKind};
use serde_json::{json, Value};

use super::*;
use crate::delivery::github::tests::fake::{issue, link, Board, Mock, SECRET};
use crate::delivery::settings::{DeliverySettings, MappedKind, ReadEntry};
use crate::logging::{Domain, LogFilter, LogLevel};
use crate::tools::tests::plan_fixture::PlanFixture;

type List = ListPlanIssuesTool<Mock>;
type Search = SearchPlanIssuesTool<Mock>;
type Read = ReadPlanIssueTool<Mock>;

fn list(f: &PlanFixture) -> List {
    List::new(f.app(), f.origin())
}

fn search(f: &PlanFixture) -> Search {
    Search::new(f.app(), f.origin())
}

fn read(f: &PlanFixture) -> Read {
    Read::new(f.app(), f.origin())
}

/// One call of a tool, with the arguments as the model composed them. The
/// result is the data object as JSON.
fn run<T>(tool: &T, args: Value) -> Result<Value, PlanRefusal>
where
    T: PortableTool<Error = PlanRefusal>,
    T::Args: serde::de::DeserializeOwned,
    T::Output: serde::Serialize,
{
    let parsed = serde_json::from_value(args).expect("arguments");
    tauri::async_runtime::block_on(tool.call(parsed))
        .map(|output| serde_json::to_value(output).expect("output"))
}

fn numbers(output: &Value) -> Vec<u64> {
    output["issues"].as_array().unwrap().iter().map(|i| i["number"].as_u64().unwrap()).collect()
}

/// Put `count` more open Task issues on the board, numbered from 10, with the
/// title "Common work N" and the body "Common body N".
fn crowd(f: &PlanFixture, count: u64) {
    for n in 10..10 + count {
        let mut crowded = issue(n, Some("Task"));
        crowded.title = format!("Common work {n}");
        crowded.body = format!("Common body {n}");
        f.h.fake.board_issue(crowded, None);
    }
    f.refresh();
}

// PIR-FR-NBCT, TLC-FR-17: the three tools are named as the spec says, and a
// call writes nothing to GitHub, stores nothing, and appends no comment.
#[test]
fn the_tools_are_read_only() {
    let f = PlanFixture::new();
    assert_eq!(<List as PortableTool>::NAME, "list_plan_issues");
    assert_eq!(<Search as PortableTool>::NAME, "search_plan_issues");
    assert_eq!(<Read as PortableTool>::NAME, "read_plan_issue");
    run(&list(&f), json!({})).unwrap();
    let cached = f.h.cache();
    let comments = f.comments();
    run(&list(&f), json!({ "label": "docs" })).unwrap();
    run(&search(&f), json!({ "query": "Issue" })).unwrap();
    run(&read(&f), json!({ "number": 2 })).unwrap();
    assert!(f.h.fake.writes().is_empty(), "{:?}", f.h.fake.writes());
    assert_eq!(f.h.cache(), cached, "a read never updates the read cache");
    assert_eq!(f.comments(), comments);
}

// PIR-FR-VMSF, TLC-FR-05, TLC-FR-06: the description is the fixed text and the
// schema is the one of the contract surface, whatever the conversation.
#[test]
fn the_description_and_the_schema_are_fixed() {
    let f = PlanFixture::new();
    let other = PlanFixture::new();
    for (tool, other_tool, text) in [
        (
            list(&f).description(),
            List::new(other.app(), other.origin()).description(),
            LIST_DESCRIPTION,
        ),
        (
            search(&f).description(),
            Search::new(other.app(), other.origin()).description(),
            SEARCH_DESCRIPTION,
        ),
        (
            read(&f).description(),
            Read::new(other.app(), other.origin()).description(),
            READ_DESCRIPTION,
        ),
    ] {
        assert_eq!(tool, text);
        assert_eq!(other_tool, text);
    }
    assert!(LIST_DESCRIPTION.starts_with("List the issues of the delivery plan"));
    assert!(SEARCH_DESCRIPTION.starts_with("Search the issues of the delivery plan"));
    assert!(READ_DESCRIPTION.contains("It reads no issue that is not part of the plan."));
    let keys = |schema: Value| {
        let mut keys: Vec<String> =
            schema["properties"].as_object().unwrap().keys().cloned().collect();
        keys.sort();
        (keys, schema["required"].clone())
    };
    assert_eq!(
        keys(list(&f).parameters()),
        (
            ["label", "limit", "milestone", "offset", "parent", "state", "status", "type"]
                .map(String::from)
                .to_vec(),
            Value::Null
        )
    );
    assert_eq!(
        keys(search(&f).parameters()),
        (vec!["limit".into(), "query".into()], json!(["query"]))
    );
    assert_eq!(keys(read(&f).parameters()), (vec!["number".into()], json!(["number"])));
    assert_eq!(list(&f).parameters()["properties"]["state"]["enum"], json!(["open", "closed"]));
    assert_eq!(list(&f).parameters(), List::new(other.app(), other.origin()).parameters());
}

// PIR-FR-HRXN, PIR-FR-APMQ: a turn of any discussion, a file discussion
// here, reads the plan; a plan that is not available and no open project each
// have their refusal, and a refused call reads nothing.
#[test]
fn a_turn_of_any_discussion_with_a_plan_can_call_the_tools() {
    let f = PlanFixture::new();
    let elsewhere = ConversationOrigin::stub_artifact("t", "spec.md", false);
    let listed = run(&List::new(f.app(), elsewhere.clone()), json!({})).expect("a list");
    assert!(listed["total"].as_u64().unwrap() > 0, "{listed}");
    let found = run(&Search::new(f.app(), elsewhere.clone()), json!({ "query": "x" }));
    assert!(found.is_ok(), "{found:?}");
    let issue = run(&Read::new(f.app(), elsewhere), json!({ "number": 1 })).expect("a read");
    assert_eq!(issue["number"], 1);

    f.h.select_project(None);
    f.h.fake.lock().calls.clear();
    for refused in [
        run(&list(&f), json!({})).unwrap_err(),
        run(&search(&f), json!({ "query": "x" })).unwrap_err(),
        run(&read(&f), json!({ "number": 1 })).unwrap_err(),
    ] {
        assert_eq!(refused.kind, ToolErrorKind::NotFound);
        assert!(!refused.retryable);
        assert_eq!(refused.message, PLAN_UNAVAILABLE);
    }
    assert!(f.h.fake.calls().is_empty(), "a refused call reads nothing");

    f.close();
    let refused = run(&list(&f), json!({})).unwrap_err();
    assert_eq!(refused, PlanRefusal::from(ToolRefusal::NoProjectOpen));
    assert_eq!(refused.kind, ToolErrorKind::NotFound);
    assert!(!refused.retryable);
}

// PIR-FR-JEBI, DSR-FR-MKIG, DSR-FR-TZDZ: only the issues of the polling
// repository that the Type mapping keeps are plan issues. A pull request, an
// excluded issue, an unmapped issue, and an issue of another repository are
// never listed, found, or read.
#[test]
fn only_plan_issues_are_reachable() {
    let f = PlanFixture::new();
    f.h.save_delivery_settings(&DeliverySettings {
        read_entries: vec![ReadEntry { type_name: "Spike".into(), kind: MappedKind::Exclude }],
        ..DeliverySettings::default()
    });
    let mut excluded = issue(20, Some("Spike"));
    excluded.title = "Plan hidden excluded".into();
    f.h.fake.board_issue(excluded, None);
    let mut unmapped = issue(21, Some("Idea"));
    unmapped.title = "Plan hidden unmapped".into();
    f.h.fake.board_issue(unmapped, None);
    let mut foreign = issue(22, Some("Task"));
    foreign.owner = "other".into();
    foreign.title = "Plan hidden foreign".into();
    f.h.fake.board_issue(foreign, None);
    f.h.fake.lock().board.push(("PR_1".into(), None, Board::PullRequest));
    f.h.fake.lock().pull_requests.push(23);
    f.refresh();

    let listed = run(&list(&f), json!({})).unwrap();
    assert_eq!(numbers(&listed), vec![1, 2, 3]);
    assert_eq!(listed["total"], 3);
    let found = run(&search(&f), json!({ "query": "hidden" })).unwrap();
    assert_eq!(found["total"], 0);
    for number in [20, 21, 22, 23, 999] {
        let refused = run(&read(&f), json!({ "number": number })).unwrap_err();
        assert_eq!(refused.code, "not_a_plan_issue", "issue {number}");
    }
    assert_eq!(run(&read(&f), json!({ "number": 3 })).unwrap()["number"], 3);
}

// PIR-FR-JPMU: the filters, the page, the total, and the row.
#[test]
fn a_list_filters_and_pages_the_plan_issues() {
    let f = PlanFixture::new();
    let mut closed = issue(4, Some("Task"));
    closed.state = "closed".into();
    closed.state_reason = Some(crate::delivery::github::adapter::StateReason::NotPlanned);
    closed.parent = Some(link(2, "open"));
    f.h.fake.board_issue(closed, Some("Done"));
    f.h.fake.lock().issue_labels.insert(2, vec!["docs".into()]);
    f.refresh();

    let all = run(&list(&f), json!({})).unwrap();
    assert_eq!(numbers(&all), vec![1, 2, 3, 4]);
    assert_eq!((all["total"].clone(), all["offset"].clone()), (json!(4), json!(0)));
    assert_eq!(
        all["issues"][2],
        json!({
            "number": 3, "title": "Issue 3", "type": "Task", "state": "open", "closeReason": null,
            "milestone": "Milestone 1", "parent": 2, "status": "Ready"
        })
    );
    assert_eq!(all["issues"][3]["closeReason"], "not_planned");
    let cases = [
        (json!({ "state": "closed" }), vec![4]),
        (json!({ "state": "open" }), vec![1, 2, 3]),
        (json!({ "type": "feature" }), vec![2]),
        (json!({ "milestone": "Milestone 1" }), vec![3]),
        (json!({ "status": "Ready" }), vec![3]),
        (json!({ "parent": 2 }), vec![3, 4]),
        (json!({ "parent": -5 }), vec![]),
        (json!({ "label": "docs" }), vec![2]),
        (json!({ "state": "open", "parent": 2 }), vec![3]),
    ];
    for (args, expected) in cases {
        assert_eq!(numbers(&run(&list(&f), args.clone()).unwrap()), expected, "{args}");
    }
    let page = run(&list(&f), json!({ "limit": 2, "offset": 1 })).unwrap();
    assert_eq!(numbers(&page), vec![2, 3]);
    assert_eq!((page["total"].clone(), page["offset"].clone()), (json!(4), json!(1)));
}

// PIR-FR-JPMU: at most `limit` issues, 50 by default and 100 at most.
#[test]
fn a_list_page_has_a_default_and_a_largest_size() {
    let f = PlanFixture::new();
    crowd(&f, 120);
    let by_default = run(&list(&f), json!({})).unwrap();
    assert_eq!(by_default["issues"].as_array().unwrap().len(), LIST_DEFAULT);
    assert_eq!(by_default["total"], 123);
    let largest = run(&list(&f), json!({ "limit": 5000 })).unwrap();
    assert_eq!(largest["issues"].as_array().unwrap().len(), LIST_MAX);
    let smallest = run(&list(&f), json!({ "limit": 0 })).unwrap();
    assert_eq!(smallest["issues"].as_array().unwrap().len(), 1);
    let last = run(&list(&f), json!({ "limit": 100, "offset": 100 })).unwrap();
    assert_eq!(last["issues"].as_array().unwrap().len(), 23);
    assert_eq!(size_of(None, 7, 9), 7);
}

// PIR-FR-DFKG: the first page gives the catalog, and a later page does not.
#[test]
fn the_first_page_gives_the_catalog() {
    let f = PlanFixture::new();
    f.h.fake
        .lock()
        .milestones
        .push(crate::delivery::github::tests::fake::milestone(2, None, "closed"));
    f.refresh();
    let first = run(&list(&f), json!({})).unwrap();
    assert_eq!(
        first["catalog"],
        json!({
            "milestones": [
                { "number": 1, "title": "Milestone 1", "state": "open" },
                { "number": 2, "title": "Milestone 2", "state": "closed" },
            ],
            "types": ["Epic", "Feature", "Task", "Bug"],
            "statuses": ["Todo", "Ready", "In Progress", "Done"],
            "labels": ["bug", "docs", "Needs Review"],
        })
    );
    let later = run(&list(&f), json!({ "offset": 1 })).unwrap();
    assert!(later.get("catalog").is_none());
    let calls = f.h.fake.calls();
    assert_eq!(calls.iter().filter(|c| c.as_str() == "list_labels").count(), 1);
}

// PIR-FR-DFKG: where the labels cannot be read, the catalog holds `labels: null`
// and the call still succeeds.
#[test]
fn unreadable_labels_leave_the_catalog_without_labels() {
    let f = PlanFixture::new();
    run(&list(&f), json!({})).unwrap();
    f.h.fake.lock().read_error = Some("github_request_failed".into());
    let again = run(&list(&f), json!({})).unwrap();
    assert_eq!(again["catalog"]["labels"], Value::Null);
    assert_eq!(numbers(&again), vec![1, 2, 3]);
    assert_eq!(again["catalog"]["types"], json!(["Epic", "Feature", "Task"]));
}

// PIR-FR-APBD: a search finds the words of the title and the body, best first,
// with one short snippet.
#[test]
fn a_search_ranks_the_title_before_the_body() {
    let f = PlanFixture::new();
    let mut by_body = issue(5, Some("Task"));
    by_body.title = "Unrelated".into();
    by_body.body = format!("{} relay design {}", "filler ".repeat(60), "tail ".repeat(60));
    f.h.fake.board_issue(by_body, None);
    let mut by_title = issue(6, Some("Task"));
    by_title.title = "Relay auth".into();
    by_title.body = "Nothing".into();
    f.h.fake.board_issue(by_title, None);
    f.refresh();

    let found = run(&search(&f), json!({ "query": "relay" })).unwrap();
    assert_eq!(numbers(&found), vec![6, 5]);
    assert_eq!(found["total"], 2);
    assert_eq!(found["issues"][0]["title"], "Relay auth");
    assert_eq!(found["issues"][0]["type"], "Task");
    let snippet = found["issues"][1]["snippet"].as_str().unwrap();
    assert!(snippet.contains("relay design"));
    assert!(snippet.chars().count() <= 160);
    let both = run(&search(&f), json!({ "query": "relay design" })).unwrap();
    assert_eq!(numbers(&both), vec![5]);
    assert_eq!(run(&search(&f), json!({ "query": "absent" })).unwrap()["total"], 0);
}

// PIR-FR-APBD: at most `limit` issues, 20 by default and 50 at most, and a blank
// query is the refusal.
#[test]
fn a_search_has_a_size_and_refuses_a_blank_query() {
    let f = PlanFixture::new();
    crowd(&f, 60);
    let by_default = run(&search(&f), json!({ "query": "common" })).unwrap();
    assert_eq!(by_default["issues"].as_array().unwrap().len(), SEARCH_DEFAULT);
    assert_eq!(by_default["total"], 60);
    let largest = run(&search(&f), json!({ "query": "common", "limit": 900 })).unwrap();
    assert_eq!(largest["issues"].as_array().unwrap().len(), SEARCH_MAX);
    for query in ["", "   "] {
        let refused = run(&search(&f), json!({ "query": query })).unwrap_err();
        assert_eq!(refused.kind, ToolErrorKind::InvalidArgs);
        assert!(refused.retryable);
        assert_eq!(refused.message, BLANK_QUERY);
    }
    let missing = run(&search(&f), json!({})).unwrap_err();
    assert_eq!(missing.message, BLANK_QUERY);
}

// PIR-FR-GYHA: the whole issue, with its relations. The body is cut at 20000
// characters with a flag.
#[test]
fn a_read_returns_the_whole_issue() {
    let f = PlanFixture::new();
    {
        let mut state = f.h.fake.lock();
        state.issue_labels.insert(2, vec!["docs".into(), "bug".into()]);
        let blocker = link(1, "open");
        let two = state.issues.iter_mut().find(|i| i.number == 2).unwrap();
        two.blocked_by = vec![blocker];
        two.body = "Short body".into();
    }
    let out = run(&read(&f), json!({ "number": 2 })).unwrap();
    assert_eq!(
        out,
        json!({
            "number": 2, "title": "Issue 2", "body": "Short body", "bodyCut": false,
            "type": "Feature", "state": "open", "closeReason": null, "milestone": null,
            "parent": 1, "subIssues": [3], "labels": ["docs", "bug"], "status": "Todo",
            "blockedBy": [1], "updatedAt": "2026-01-01T00:00:02Z",
            "url": "https://github.com/acme/widgets/issues/2", "stale": false,
        })
    );
    let long = "é".repeat(BODY_MAX_CHARS + 5);
    f.h.fake.lock().issues.iter_mut().find(|i| i.number == 3).unwrap().body = long;
    let cut = run(&read(&f), json!({ "number": 3 })).unwrap();
    assert_eq!(cut["bodyCut"], true);
    assert_eq!(cut["body"].as_str().unwrap().chars().count(), BODY_MAX_CHARS);
    f.h.fake.lock().issues.iter_mut().find(|i| i.number == 3).unwrap().body =
        "x".repeat(BODY_MAX_CHARS);
    let exact = run(&read(&f), json!({ "number": 3 })).unwrap();
    assert_eq!(exact["bodyCut"], false);
}

// PIR-FR-GYHA, PIR-FR-APMQ: a number that is not a plan issue is the refusal
// with that number, retryable.
#[test]
fn a_number_that_is_not_a_plan_issue_is_refused() {
    let f = PlanFixture::new();
    for number in [99, 0, -4] {
        let refused = run(&read(&f), json!({ "number": number })).unwrap_err();
        assert_eq!(refused.kind, ToolErrorKind::NotFound);
        assert!(refused.retryable);
        assert_eq!(refused.message, not_a_plan_issue(number));
    }
    assert_eq!(
        not_a_plan_issue(99),
        "Issue number 99 is not part of the delivery plan. List the plan issues to find the right number."
    );
}

// PIR-FR-NRLP, GDP-FR-LKXA: a list and a search answer from the read cache and
// name its `read_at`; a read of one issue asks GitHub now.
#[test]
fn a_list_and_a_search_answer_from_the_cache_and_a_read_asks_github() {
    let f = PlanFixture::new();
    run(&list(&f), json!({})).unwrap();
    let cached = f.h.cache().expect("a read cache");
    f.h.fake.lock().calls.clear();
    f.h.fake.lock().issues.iter_mut().find(|i| i.number == 2).unwrap().title =
        "Changed on GitHub".into();

    let listed = run(&list(&f), json!({ "offset": 1 })).unwrap();
    let found = run(&search(&f), json!({ "query": "Issue" })).unwrap();
    assert_eq!(listed["readAt"], json!(cached.read_at));
    assert_eq!(found["readAt"], json!(cached.read_at));
    assert_eq!((listed["stale"].clone(), found["stale"].clone()), (json!(false), json!(false)));
    assert!(f.h.fake.calls().is_empty(), "no request: {:?}", f.h.fake.calls());
    assert_eq!(listed["issues"][0]["title"], "Issue 2");

    let current = run(&read(&f), json!({ "number": 2 })).unwrap();
    assert_eq!(current["title"], "Changed on GitHub");
    assert_eq!(current["stale"], false);
    assert!(f.h.fake.calls().iter().any(|c| c.starts_with("read_issue_detail 2")));
}

// PIR-FR-NRLP: a failed read falls back to the cache and says so.
#[test]
fn a_failed_read_falls_back_to_the_cache_and_says_so() {
    let f = PlanFixture::new();
    run(&list(&f), json!({})).unwrap();
    let cached = f.h.cache().expect("a read cache");
    f.h.fake.lock().issues.iter_mut().find(|i| i.number == 2).unwrap().title =
        "Changed on GitHub".into();
    f.h.fake.lock().issue_labels.insert(2, vec!["docs".into()]);
    f.h.fake.lock().read_error = Some("github_request_failed".into());
    let out = run(&read(&f), json!({ "number": 2 })).unwrap();
    assert_eq!(out["stale"], true);
    assert_eq!(out["readAt"], json!(cached.read_at));
    assert_eq!(out["title"], "Issue 2");
    assert_eq!(out["labels"], json!([]));
    assert_eq!(out["parent"], 1);
}

// PIR-FR-ZVSC, TLC-FR-MOWF, TLC-FR-18: the tools use the token of the delivery
// plan and carry no credential: no argument names one, and no result, schema,
// or record holds it.
#[test]
fn no_token_is_in_an_argument_a_result_or_a_record() {
    static BUFFER: crate::logging::LogBuffer = crate::logging::LogBuffer::new();
    let f = PlanFixture::new();
    let outputs = [
        run(&List::new(f.app(), f.origin()).with_buffer(&BUFFER), json!({})).unwrap(),
        run(&Search::new(f.app(), f.origin()).with_buffer(&BUFFER), json!({ "query": "Issue" }))
            .unwrap(),
        run(&Read::new(f.app(), f.origin()).with_buffer(&BUFFER), json!({ "number": 1 })).unwrap(),
    ];
    let schemas = [list(&f).parameters(), search(&f).parameters(), read(&f).parameters()];
    let descriptions = [LIST_DESCRIPTION, SEARCH_DESCRIPTION, READ_DESCRIPTION];
    let text = format!("{outputs:?}{schemas:?}{descriptions:?}");
    assert!(!text.contains(SECRET) && !text.to_lowercase().contains("token"));
    let records = BUFFER
        .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 1000)
        .unwrap()
        .records;
    assert!(!records.is_empty());
    assert!(!serde_json::to_string(&records).unwrap().contains(SECRET));
    assert_eq!(f.h.fake.lock().writes, 0, "the plan tools make no write");
}

// PIR-FR-APMQ: the refusals are those of the contract surface, each with its
// kind and its retry, and a refusal ends nothing: the next call succeeds.
#[test]
fn the_refusals_are_those_of_the_contract_surface() {
    let f = PlanFixture::new();
    let refused = [
        (run(&search(&f), json!({ "query": " " })).unwrap_err(), ToolErrorKind::InvalidArgs, true),
        (run(&read(&f), json!({ "number": 77 })).unwrap_err(), ToolErrorKind::NotFound, true),
    ];
    for (refusal, kind, retryable) in refused {
        let delivered = refusal.to_execution_error();
        assert_eq!(delivered.kind(), kind);
        assert_eq!(delivered.retryable(), Some(retryable));
        assert_eq!((refusal.kind, refusal.retryable), (kind, retryable));
    }
    assert_eq!(
        PLAN_UNAVAILABLE,
        "The project has no GitHub Project selected for its plan, or its configuration is not valid. Tell the author; asking again will not succeed."
    );
    assert_eq!(COULD_NOT_READ, "The plan could not be read.");
    let broken = read_refusal(&DeliveryError::new("github_request_failed", "x"));
    assert_eq!(
        (broken.kind, broken.retryable, broken.message.as_str()),
        (ToolErrorKind::Other, true, COULD_NOT_READ)
    );
    assert_eq!(run(&read(&f), json!({ "number": 1 })).unwrap()["number"], 1);
}

// PIR-FR-VNGK, TLC-FR-14: one DEBUG record names the tool and the count, and
// one WARN record names the tool and the reason, under `ai` and `backend`.
// No record carries a title, a body, a query, or a token.
#[test]
fn each_call_is_logged_without_content() {
    static BUFFER: crate::logging::LogBuffer = crate::logging::LogBuffer::new();
    let f = PlanFixture::new();
    {
        let mut state = f.h.fake.lock();
        let issue = state.issues.iter_mut().find(|i| i.number == 2).unwrap();
        issue.title = "ZEBRA-TITLE".into();
        issue.body = "ZEBRA-BODY".into();
    }
    f.refresh();
    run(&List::new(f.app(), f.origin()).with_buffer(&BUFFER), json!({})).unwrap();
    run(&Search::new(f.app(), f.origin()).with_buffer(&BUFFER), json!({ "query": "ZEBRA" }))
        .unwrap();
    run(&Search::new(f.app(), f.origin()).with_buffer(&BUFFER), json!({ "query": " " }))
        .unwrap_err();
    let records = |domain: Domain| {
        BUFFER
            .query(
                &LogFilter {
                    min_level: LogLevel::Debug,
                    domains: vec![domain],
                    ..LogFilter::default()
                },
                None,
                1000,
            )
            .unwrap()
            .records
    };
    let ai = records(Domain::Ai);
    assert_eq!(records(Domain::Backend).len(), ai.len());
    let debug: Vec<_> = ai.iter().filter(|r| r.level == LogLevel::Debug).collect();
    let warn: Vec<_> = ai.iter().filter(|r| r.level == LogLevel::Warn).collect();
    assert_eq!(debug.len(), 2);
    assert_eq!(warn.len(), 1);
    assert_eq!(debug[0].fields["tool"], "list_plan_issues");
    assert_eq!(debug[0].fields["count"], 3);
    assert_eq!(debug[1].fields["tool"], "search_plan_issues");
    assert_eq!(debug[1].fields["count"], 1);
    assert_eq!(warn[0].fields["tool"], "search_plan_issues");
    assert_eq!(warn[0].fields["reason"], "blank_query");
    let text = serde_json::to_string(&ai).unwrap();
    assert!(!text.contains("ZEBRA") && !text.contains("Issue 1") && !text.contains(SECRET));
}

/// One answer of the `read_github_plan_issue` command as the frontend gets it.
fn command_read(f: &PlanFixture, number: u64) -> Result<Value, DeliveryError> {
    let read = tauri::async_runtime::block_on(
        crate::delivery::github::plan_reads::read_plan_issue_detail(f.app(), number),
    )?;
    Ok(serde_json::to_value(read).unwrap())
}

// GDP-FR-WQEN, GDP-FR-QFQF: the command answers the full detail of one plan
// issue, read from GitHub now, in the camelCase shape of the contract surface.
#[test]
fn the_command_answers_the_detail_in_the_frontend_shape() {
    let f = PlanFixture::new();
    f.h.fake.lock().issue_labels.insert(2, vec!["docs".into()]);
    let out = command_read(&f, 2).unwrap();
    assert_eq!(out["stale"], false);
    assert_eq!(out["error"], Value::Null);
    assert_eq!(out["readAt"], Value::Null);
    let issue = &out["issue"];
    assert_eq!(issue["number"], 2);
    assert_eq!(issue["labels"], json!(["docs"]));
    assert_eq!(issue["subIssues"], json!([3]));
    for key in ["milestoneTitle", "issueType", "closeReason", "blockedBy", "updatedAt"] {
        assert!(issue.get(key).is_some(), "{key} is in {issue}");
    }
    assert!(issue.get("sub_issues").is_none(), "{issue}");
}

// GDP-FR-WQEN: a number that is not a plan issue refuses with
// `tracker_item_not_found` and sends no request.
#[test]
fn the_command_refuses_a_number_outside_the_plan() {
    let f = PlanFixture::new();
    run(&list(&f), json!({})).unwrap();
    f.h.fake.lock().calls.clear();
    let refused = command_read(&f, 99).unwrap_err();
    assert_eq!(refused.code, "tracker_item_not_found");
    assert!(f.h.fake.calls().is_empty(), "no request: {:?}", f.h.fake.calls());
}

// GDP-FR-KRMB: a failed GitHub read answers the cache, stale, with no labels,
// the typed error, and the read_at of the cache.
#[test]
fn the_command_falls_back_to_the_cache_when_github_fails() {
    let f = PlanFixture::new();
    run(&list(&f), json!({})).unwrap();
    let cached = f.h.cache().expect("a read cache");
    f.h.fake.lock().issue_labels.insert(2, vec!["docs".into()]);
    f.h.fake.lock().read_error = Some("github_request_failed".into());
    let out = command_read(&f, 2).unwrap();
    assert_eq!(out["stale"], true);
    assert_eq!(out["readAt"], json!(cached.read_at));
    assert_eq!(out["issue"]["labels"], json!([]));
    assert_eq!(out["error"]["code"], "github_request_failed");
}

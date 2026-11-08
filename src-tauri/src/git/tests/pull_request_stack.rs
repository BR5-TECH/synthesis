//! The stack operations against a scripted GitHub and a real stack record
//! (GTC-FR-TBQW, GTC-FR-JMVC, GTC-FR-RNXK, GTC-FR-LCYE, GTC-FR-OIMF,
//! GTC-FR-UYGD, GTC-FR-HEXN, GTC-FR-KQVA, GTC-FR-AEBW, GTC-FR-DPTN,
//! GTC-FR-GJFM; RMS-FR-QPLE).

use super::pull_requests::{
    list_path, page_path, repo_with_origin, token_missing, token_ready, FakeGithub, PLATFORM,
    SECRET,
};
use super::*;
use crate::git::pull_requests::client::{GithubFailure, GithubWriteFailure};
use crate::git::pull_requests::create::NewPullRequest;
use crate::git::pull_requests::stack::{
    add_branch_to_stack_reported, get_branch_stack_reported, submit_pull_request_to_stack_reported,
    ERR_STACKS_NOT_ENABLED, ERR_STACK_BUSY, ERR_STACK_PARENT_HAS_NO_PULL_REQUEST,
    ERR_STACK_PARENT_NOT_IN_STACK, ERR_STACK_PARENT_NOT_TOP, ERR_STACK_RECORD_MISSING,
    ERR_STACK_REJECTED, ERR_STACK_UNAVAILABLE,
};
use crate::git::pull_requests::stack_record::{self, StackRecordEntry, STACKS_FILE};
use serde_json::{json, Value};

const REPO: &str = "/repos/acme/platform";

fn open_row(number: u64, head: &str, base: &str) -> Value {
    json!({
        "number": number,
        "title": format!("Change {number}"),
        "state": "open",
        "head": { "ref": head, "repo": { "full_name": "acme/platform" } },
        "base": { "ref": base, "repo": { "full_name": "acme/platform" } },
    })
}

fn stack_json(number: u64, heads: &[&str]) -> Value {
    json!({
        "number": number,
        "base": { "ref": "main" },
        "pull_requests": heads
            .iter()
            .enumerate()
            .map(|(i, head)| json!({ "number": i + 1, "head": { "ref": head } }))
            .collect::<Vec<_>>(),
    })
}

/// A repository on GitHub with the local branches `names`.
fn fixture(names: &[&str]) -> WorktreeFixture {
    let f = repo_with_origin(PLATFORM);
    for name in names {
        f.branch(name);
    }
    f
}

fn open_pulls(github: &FakeGithub, rows: Vec<Value>) {
    github.answer(&page_path(&list_path("open"), 1), Ok(Value::Array(rows)));
}

struct Machine {
    dir: TempDir,
}

impl Machine {
    fn new() -> Machine {
        Machine { dir: TempDir::new().unwrap() }
    }

    fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(self.dir.path())
    }

    fn entries(&self) -> Vec<StackRecordEntry> {
        stack_record::read_entries(&self.root()).unwrap()
    }
}

fn access<'a>(
    root: &'a std::path::Path,
    tokens: &'a (GlobalSettingsStore, GithubTokens),
    github: &'a FakeGithub,
) -> PullRequestAccess<'a> {
    PullRequestAccess {
        project: GithubAccess {
            root,
            store: &tokens.0,
            tokens: &tokens.1,
            project_key: "/dev/acme",
        },
        client: github,
    }
}

fn read(
    f: &WorktreeFixture,
    github: &FakeGithub,
    tokens: &(GlobalSettingsStore, GithubTokens),
    machine: Option<&crate::fs::RootFs>,
    branch: &str,
) -> crate::git::BranchStack {
    let root = f.root();
    get_branch_stack_reported(
        &NullSink,
        own_buffer(),
        &access(&root, tokens, github),
        machine,
        branch,
    )
    .unwrap()
}

// GTC-FR-TBQW, GTC-FR-WHEO
#[test]
fn a_branch_with_an_open_pull_request_reads_the_github_stack_that_holds_it() {
    let f = fixture(&["a", "b"]);
    let github = FakeGithub::new();
    open_pulls(&github, vec![open_row(1, "a", "main"), open_row(2, "b", "a")]);
    github
        .answer(&format!("{REPO}/stacks?pull_request=2"), Ok(json!([stack_json(7, &["a", "b"])])));
    let machine = Machine::new();

    let stack = read(&f, &github, &token_ready(), Some(&machine.root()), "b");

    assert!(stack.available && stack.in_stack && stack.is_top);
    assert_eq!(stack.source.as_deref(), Some("github"));
    assert_eq!(stack.stack_number, Some(7));
    assert_eq!(stack.parent.as_deref(), Some("a"));
    assert!(github.requests().iter().all(|(secret, _)| secret == SECRET));
    let wire = serde_json::to_value(&stack).unwrap();
    assert_eq!(wire["inStack"], json!(true));
    assert_eq!(wire["pullRequest"], json!({ "number": 2, "title": "Change 2" }));
}

// GTC-FR-TBQW: a branch with no pull request probes the stacks of the repository.
#[test]
fn a_branch_without_a_pull_request_probes_the_stacks_and_reads_its_record() {
    let f = fixture(&["a", "c"]);
    let github = FakeGithub::new();
    open_pulls(&github, vec![open_row(1, "a", "main"), open_row(2, "b", "a")]);
    github.answer(&format!("{REPO}/stacks?per_page=1"), Ok(json!([])));
    let machine = Machine::new();
    stack_record::put_entry(
        &machine.root(),
        StackRecordEntry { branch: "c".into(), parent: "b".into(), stack: Some(7) },
    )
    .unwrap();

    let stack = read(&f, &github, &token_ready(), Some(&machine.root()), "c");

    assert!(github.paths().contains(&format!("{REPO}/stacks?per_page=1")));
    assert_eq!(stack.source.as_deref(), Some("local_record"));
    assert_eq!(stack.parent.as_deref(), Some("b"));
    assert_eq!(stack.stack_number, Some(7));
}

// GTC-FR-RNXK: an entry whose branch is not a local branch is ignored.
#[test]
fn a_record_of_a_branch_that_is_gone_is_ignored() {
    let f = fixture(&[]);
    let github = FakeGithub::new();
    open_pulls(&github, vec![]);
    github.answer(&format!("{REPO}/stacks?per_page=1"), Ok(json!([])));
    let machine = Machine::new();
    stack_record::put_entry(
        &machine.root(),
        StackRecordEntry { branch: "gone".into(), parent: "main".into(), stack: None },
    )
    .unwrap();

    let stack = read(&f, &github, &token_ready(), Some(&machine.root()), "gone");
    assert!(stack.available && !stack.in_stack);
}

// GTC-FR-JMVC, GTC-FR-DPTN: stacks GitHub does not serve are unavailable, and
// the open pull request is still named.
#[test]
fn stacks_that_github_does_not_serve_answer_unavailable_and_keep_the_pull_request() {
    let f = fixture(&["b"]);
    let github = FakeGithub::new();
    open_pulls(&github, vec![open_row(2, "b", "a")]);
    github.answer(&format!("{REPO}/stacks?pull_request=2"), Err(GithubFailure::NotFound));

    let stack = read(&f, &github, &token_ready(), None, "b");
    assert!(!stack.available && !stack.in_stack);
    assert_eq!(stack.unavailable_cause.as_deref(), Some(ERR_STACKS_NOT_ENABLED));
    assert_eq!(stack.pull_request.map(|p| p.number), Some(2));

    let github = FakeGithub::new();
    open_pulls(&github, vec![open_row(2, "b", "a")]);
    github.answer(&format!("{REPO}/stacks?pull_request=2"), Err(GithubFailure::Rejected));
    let stack = read(&f, &github, &token_ready(), None, "b");
    assert_eq!(stack.unavailable_cause.as_deref(), Some("github_token_rejected"));
}

// GTC-FR-JMVC: no token and no GitHub remote are no error, and send nothing.
#[test]
fn no_token_or_no_github_remote_is_unavailable_with_no_request() {
    let f = fixture(&["b"]);
    let github = FakeGithub::new();
    let stack = read(&f, &github, &token_missing(), None, "b");
    assert!(!stack.available);
    assert_eq!(stack.unavailable_cause.as_deref(), Some(github_tokens::ERR_TOKEN_MISSING));

    let other = repo_with_origin("https://example.com/acme/platform.git");
    let stack = read(&other, &github, &token_ready(), None, "main");
    assert_eq!(stack.unavailable_cause.as_deref(), Some(crate::git::ERR_NOT_A_GITHUB_REMOTE));
    assert!(github.requests().is_empty());
}

// GTC-FR-JMVC: the open pull requests that cannot be read leave no pull request.
#[test]
fn a_failed_read_of_the_open_pull_requests_is_unavailable() {
    let f = fixture(&["b"]);
    let github = FakeGithub::new();
    let stack = read(&f, &github, &token_ready(), None, "b");
    assert!(!stack.available && stack.pull_request.is_none());
}

// GTC-FR-JMVC: a list of open pull requests that stops at the page limit is
// partial, so no stack is resolved from it and no stack read is sent.
#[test]
fn a_truncated_list_of_open_pull_requests_is_unavailable() {
    use crate::git::pull_requests::client::{MAX_PAGES, PAGE_SIZE};
    let f = fixture(&["b"]);
    let github = FakeGithub::new();
    let full: Vec<Value> =
        (0..PAGE_SIZE as u64).map(|i| open_row(100 + i, &format!("x{i}"), "main")).collect();
    let mut pages = vec![full; MAX_PAGES];
    pages.push(vec![open_row(2, "b", "a")]);
    github.pages(&list_path("open"), pages);

    let stack = read(&f, &github, &token_ready(), None, "b");
    assert!(!stack.available && !stack.in_stack);
    assert_eq!(stack.unavailable_cause.as_deref(), Some(ERR_STACK_UNAVAILABLE));
    assert!(stack.pull_request.is_none());
    assert!(!github.paths().iter().any(|p| p.contains("/stacks")));
}

// GTC-FR-TBQW: a pull request from a fork is not a layer of a stack.
#[test]
fn a_pull_request_from_a_fork_is_left_out() {
    let f = fixture(&["b"]);
    let github = FakeGithub::new();
    let mut fork = open_row(2, "b", "main");
    fork["head"]["repo"]["full_name"] = json!("someone/platform");
    open_pulls(&github, vec![fork]);
    github.answer(&format!("{REPO}/stacks?per_page=1"), Ok(json!([])));
    let stack = read(&f, &github, &token_ready(), None, "b");
    assert!(stack.available && stack.pull_request.is_none());
}

// GTC-FR-TBQW: an empty branch is the one refusal.
#[test]
fn an_empty_branch_is_unknown() {
    let f = fixture(&[]);
    let github = FakeGithub::new();
    let root = f.root();
    let tokens = token_ready();
    let error = get_branch_stack_reported(
        &NullSink,
        own_buffer(),
        &access(&root, &tokens, &github),
        None,
        " ",
    )
    .unwrap_err();
    assert_eq!(error, ERR_UNKNOWN_BRANCH);
}

fn add(
    f: &WorktreeFixture,
    github: &FakeGithub,
    machine: Option<&crate::fs::RootFs>,
    branch: &str,
    parent: &str,
) -> Result<crate::git::BranchStack, String> {
    let root = f.root();
    let tokens = token_ready();
    add_branch_to_stack_reported(
        &NullSink,
        own_buffer(),
        &access(&root, &tokens, github),
        machine,
        branch,
        parent,
    )
}

/// GitHub holds stack 7 of `a` and `b`, with `b` the head of #2.
fn stacked_github() -> FakeGithub {
    let github = FakeGithub::new();
    open_pulls(&github, vec![open_row(1, "a", "main"), open_row(2, "b", "a")]);
    github
        .answer(&format!("{REPO}/stacks?pull_request=2"), Ok(json!([stack_json(7, &["a", "b"])])));
    github
        .answer(&format!("{REPO}/stacks?pull_request=1"), Ok(json!([stack_json(7, &["a", "b"])])));
    github
}

// GTC-FR-LCYE, GTC-FR-OIMF, RMS-FR-QPLE
#[test]
fn a_new_branch_on_the_top_of_a_stack_is_recorded_with_its_parent_and_stack() {
    let f = fixture(&["a", "b", "c"]);
    let github = stacked_github();
    let machine = Machine::new();

    let stack = add(&f, &github, Some(&machine.root()), "c", "b").unwrap();

    assert_eq!(
        machine.entries(),
        vec![StackRecordEntry { branch: "c".into(), parent: "b".into(), stack: Some(7) }]
    );
    assert!(machine.dir.path().join(STACKS_FILE).exists());
    assert_eq!(stack.source.as_deref(), Some("local_record"));
    assert_eq!(stack.top.as_deref(), Some("c"));
    assert!(stack.is_top && stack.in_stack);
    assert!(!stack.top_has_pull_request, "the recorded branch has no pull request");
    assert!(github.posts().is_empty(), "GitHub receives no write");

    // A second write of the same branch replaces its entry.
    add(&f, &github, Some(&machine.root()), "c", "b").unwrap();
    assert_eq!(machine.entries().len(), 1);
}

// GTC-FR-LCYE: every refusal writes nothing.
#[test]
fn a_parent_that_cannot_carry_the_branch_is_refused_and_nothing_is_written() {
    let f = fixture(&["a", "b", "c", "solo"]);
    let machine = Machine::new();

    let github = stacked_github();
    assert_eq!(
        add(&f, &github, Some(&machine.root()), "c", "a").unwrap_err(),
        ERR_STACK_PARENT_NOT_TOP
    );
    assert_eq!(
        add(&f, &github, Some(&machine.root()), "nope", "b").unwrap_err(),
        ERR_UNKNOWN_BRANCH
    );

    let github = FakeGithub::new();
    open_pulls(&github, vec![open_row(5, "solo", "main")]);
    github.answer(&format!("{REPO}/stacks?pull_request=5"), Ok(json!([])));
    assert_eq!(
        add(&f, &github, Some(&machine.root()), "c", "solo").unwrap_err(),
        ERR_STACK_PARENT_NOT_IN_STACK
    );

    let github = FakeGithub::new();
    assert_eq!(
        add(&f, &github, Some(&machine.root()), "c", "b").unwrap_err(),
        ERR_STACK_UNAVAILABLE
    );

    // A recorded parent with no pull request yet.
    let github = FakeGithub::new();
    open_pulls(&github, vec![]);
    github.answer(&format!("{REPO}/stacks?per_page=1"), Ok(json!([])));
    stack_record::put_entry(
        &machine.root(),
        StackRecordEntry { branch: "b".into(), parent: "a".into(), stack: None },
    )
    .unwrap();
    assert_eq!(
        add(&f, &github, Some(&machine.root()), "c", "b").unwrap_err(),
        ERR_STACK_PARENT_HAS_NO_PULL_REQUEST
    );
    assert_eq!(machine.entries().len(), 1, "only the entry the test wrote");
}

// GTC-FR-OIMF: no store, no record.
#[test]
fn a_store_that_cannot_be_written_returns_store_unavailable() {
    let f = fixture(&["a", "b", "c"]);
    let github = stacked_github();
    assert_eq!(add(&f, &github, None, "c", "b").unwrap_err(), stack_record::ERR_STORE_UNAVAILABLE);
}

fn created_json(number: u64) -> Value {
    json!({ "number": number, "html_url": format!("https://github.com/acme/platform/pull/{number}") })
}

fn submit(
    f: &WorktreeFixture,
    github: &FakeGithub,
    machine: &Machine,
    head: &str,
    base: &str,
    buffer: &'static LogBuffer,
) -> Result<crate::git::StackedPullRequest, String> {
    let root = f.root();
    let tokens = token_ready();
    submit_pull_request_to_stack_reported(
        &NullSink,
        buffer,
        &access(&root, &tokens, github),
        Some(&machine.root()),
        &NewPullRequest { title: "secret title", body: "secret body", base, head, draft: false },
    )
}

// GTC-FR-UYGD, GTC-FR-HEXN, GTC-FR-AEBW, GTC-FR-GJFM
#[test]
fn a_recorded_head_with_a_stack_number_is_created_then_added_to_that_stack() {
    let f = fixture(&["a", "b", "c"]);
    let github = stacked_github();
    let machine = Machine::new();
    stack_record::put_entry(
        &machine.root(),
        StackRecordEntry { branch: "c".into(), parent: "b".into(), stack: Some(7) },
    )
    .unwrap();
    github.queue_post(Ok(created_json(3)));
    github.queue_post(Ok(stack_json(7, &["a", "b", "c"])));
    let buffer = own_buffer();

    let created = submit(&f, &github, &machine, "c", "b", buffer).unwrap();

    let posts = github.posts();
    assert_eq!(posts[0].1, format!("{REPO}/pulls"));
    assert_eq!(posts[1].1, format!("{REPO}/stacks/7/add"));
    assert_eq!(posts[1].2, json!({ "pull_requests": [3] }));
    assert_eq!(
        serde_json::to_value(&created).unwrap(),
        json!({
            "number": 3,
            "url": "https://github.com/acme/platform/pull/3",
            "stack": { "state": "added", "stackNumber": 7 },
        })
    );
    assert!(machine.entries().is_empty(), "the GitHub stack now holds the branch");
    let added = record_for(buffer, "pull request added to stack");
    assert_eq!(added.fields.get("stackNumber"), Some(&json!(7)));
    let records = format!("{:?}", all_records(buffer));
    assert!(!records.contains("secret title") && !records.contains("secret body"));
    assert!(!records.contains(SECRET));
}

// GTC-FR-HEXN: a record with no number and a chain below creates a stack.
#[test]
fn a_recorded_head_on_a_chain_creates_a_stack_of_the_chain_and_the_new_pull_request() {
    let f = fixture(&["a", "b", "c"]);
    let github = FakeGithub::new();
    open_pulls(&github, vec![open_row(1, "a", "main"), open_row(2, "b", "a")]);
    github.answer(&format!("{REPO}/stacks?pull_request=2"), Ok(json!([])));
    let machine = Machine::new();
    stack_record::put_entry(
        &machine.root(),
        StackRecordEntry { branch: "c".into(), parent: "b".into(), stack: None },
    )
    .unwrap();
    github.queue_post(Ok(created_json(3)));
    github.queue_post(Ok(stack_json(11, &["a", "b", "c"])));

    let created = submit(&f, &github, &machine, "c", "b", own_buffer()).unwrap();

    let posts = github.posts();
    assert_eq!(posts[1].1, format!("{REPO}/stacks"));
    assert_eq!(posts[1].2, json!({ "pull_requests": [1, 2, 3] }));
    assert_eq!(created.stack.state, "created");
    assert_eq!(created.stack.stack_number, Some(11));
}

// GTC-FR-HEXN: a record with no number, but GitHub now holds a stack of base.
#[test]
fn a_recorded_head_whose_base_is_now_in_a_github_stack_is_added_to_it() {
    let f = fixture(&["a", "b", "c"]);
    let github = stacked_github();
    let machine = Machine::new();
    stack_record::put_entry(
        &machine.root(),
        StackRecordEntry { branch: "c".into(), parent: "b".into(), stack: None },
    )
    .unwrap();
    github.queue_post(Ok(created_json(3)));
    github.queue_post(Ok(stack_json(7, &["a", "b", "c"])));

    let created = submit(&f, &github, &machine, "c", "b", own_buffer()).unwrap();
    assert_eq!(github.posts()[1].1, format!("{REPO}/stacks/7/add"));
    assert_eq!(created.stack.state, "added");
}

// GTC-FR-KQVA, GTC-FR-AEBW: a failed stack step keeps the pull request and the
// record, and is no error.
#[test]
fn a_stack_step_that_fails_answers_not_added_and_keeps_the_record() {
    let f = fixture(&["a", "b", "c"]);
    let machine = Machine::new();
    stack_record::put_entry(
        &machine.root(),
        StackRecordEntry { branch: "c".into(), parent: "b".into(), stack: Some(7) },
    )
    .unwrap();
    for (failure, expected) in [
        (GithubWriteFailure::Conflict, ERR_STACK_BUSY.to_string()),
        (
            GithubWriteFailure::Invalid("cannot append".into()),
            format!("{ERR_STACK_REJECTED}: cannot append"),
        ),
        (GithubWriteFailure::Rejected, "github_token_rejected".to_string()),
        (GithubWriteFailure::Unreachable, github_tokens::ERR_GITHUB_UNREACHABLE.to_string()),
    ] {
        let github = stacked_github();
        github.queue_post(Ok(created_json(3)));
        github.queue_post(Err(failure));
        let created = submit(&f, &github, &machine, "c", "b", own_buffer()).unwrap();
        assert_eq!(created.number, 3);
        assert_eq!(created.stack.state, "not_added");
        assert_eq!(created.stack.error.as_deref(), Some(expected.as_str()));
    }
    assert_eq!(machine.entries().len(), 1);
}

// GTC-FR-KQVA: without a record that names the base, no stack request is sent.
#[test]
fn a_head_without_a_record_for_that_base_is_not_added() {
    let f = fixture(&["a", "b", "c"]);
    let github = stacked_github();
    let machine = Machine::new();
    github.queue_post(Ok(created_json(3)));
    let created = submit(&f, &github, &machine, "c", "b", own_buffer()).unwrap();
    assert_eq!(created.stack.error.as_deref(), Some(ERR_STACK_RECORD_MISSING));
    assert_eq!(github.posts().len(), 1);

    // A recorded parent with no open pull request.
    let github = FakeGithub::new();
    open_pulls(&github, vec![]);
    stack_record::put_entry(
        &machine.root(),
        StackRecordEntry { branch: "c".into(), parent: "b".into(), stack: None },
    )
    .unwrap();
    github.queue_post(Ok(created_json(3)));
    let created = submit(&f, &github, &machine, "c", "b", own_buffer()).unwrap();
    assert_eq!(created.stack.error.as_deref(), Some(ERR_STACK_PARENT_HAS_NO_PULL_REQUEST));
}

// GTC-FR-UYGD: a failed creation is the error of the call, with no stack step.
#[test]
fn a_failed_creation_returns_its_error_and_sends_no_stack_request() {
    let f = fixture(&["a", "b", "c"]);
    let github = stacked_github();
    let machine = Machine::new();
    stack_record::put_entry(
        &machine.root(),
        StackRecordEntry { branch: "c".into(), parent: "b".into(), stack: Some(7) },
    )
    .unwrap();
    github.queue_post(Err(GithubWriteFailure::Invalid("A pull request already exists".into())));
    let error = submit(&f, &github, &machine, "c", "b", own_buffer()).unwrap_err();
    assert_eq!(error, crate::git::ERR_PULL_REQUEST_EXISTS);
    assert_eq!(github.posts().len(), 1);
    assert_eq!(machine.entries().len(), 1);
}

/// A record of `c` on `b`, with no stack number.
fn record_c_on_b(machine: &Machine) {
    stack_record::put_entry(
        &machine.root(),
        StackRecordEntry { branch: "c".into(), parent: "b".into(), stack: None },
    )
    .unwrap();
}

// GTC-FR-HEXN, GTC-FR-AEBW: a stack that GitHub created is `created` also
// when its answer names no number, and the record entry goes away.
#[test]
fn a_created_stack_with_an_answer_that_names_no_number_is_created() {
    let f = fixture(&["a", "b", "c"]);
    let github = FakeGithub::new();
    open_pulls(&github, vec![open_row(1, "a", "main"), open_row(2, "b", "a")]);
    github.answer(&format!("{REPO}/stacks?pull_request=2"), Ok(json!([])));
    let machine = Machine::new();
    record_c_on_b(&machine);
    github.queue_post(Ok(created_json(3)));
    github.queue_post(Ok(json!({})));

    let created = submit(&f, &github, &machine, "c", "b", own_buffer()).unwrap();

    assert_eq!(created.stack.state, "created");
    assert_eq!(created.stack.stack_number, None);
    assert!(created.stack.error.is_none());
    assert!(machine.entries().is_empty(), "the GitHub stack now holds the branch");
}

// GTC-FR-KQVA, GTC-FR-GJFM: a stack read that fails in the stack step is
// `stack_unavailable`, and its own cause is logged with the repository.
#[test]
fn a_failed_stack_read_in_the_stack_step_is_unavailable_and_logs_its_cause() {
    let f = fixture(&["a", "b", "c"]);
    let github = FakeGithub::new();
    open_pulls(&github, vec![open_row(1, "a", "main"), open_row(2, "b", "a")]);
    github.answer(&format!("{REPO}/stacks?pull_request=2"), Err(GithubFailure::NotFound));
    let machine = Machine::new();
    record_c_on_b(&machine);
    github.queue_post(Ok(created_json(3)));
    let buffer = own_buffer();

    let created = submit(&f, &github, &machine, "c", "b", buffer).unwrap();

    assert_eq!(created.stack.state, "not_added");
    assert_eq!(created.stack.error.as_deref(), Some(ERR_STACK_UNAVAILABLE));
    assert_eq!(github.posts().len(), 1, "only the creation was sent");
    assert_eq!(machine.entries().len(), 1, "the record stays after not_added");
    let failed = record_for(buffer, "stack read for the stack step failed");
    assert_eq!(failed.fields.get("owner"), Some(&json!("acme")));
    assert_eq!(failed.fields.get("repo"), Some(&json!("platform")));
    assert_eq!(failed.fields.get("cause"), Some(&json!(ERR_STACKS_NOT_ENABLED)));
    let write = record_for(buffer, "stack write");
    assert_eq!(write.fields.get("write"), Some(&json!("read")));
    assert_eq!(write.fields.get("error"), Some(&json!(ERR_STACK_UNAVAILABLE)));
}

// GTC-FR-RNXK, GTC-FR-KQVA: a stack record that cannot be read is empty in
// the stack step, and the module logs it at WARN.
#[test]
fn a_stack_record_that_cannot_be_read_is_logged_and_is_no_record() {
    let f = fixture(&["a", "b", "c"]);
    let github = stacked_github();
    let machine = Machine::new();
    std::fs::write(machine.dir.path().join(STACKS_FILE), "this is [not toml").unwrap();
    github.queue_post(Ok(created_json(3)));
    let buffer = own_buffer();

    let created = submit(&f, &github, &machine, "c", "b", buffer).unwrap();

    assert_eq!(created.stack.error.as_deref(), Some(ERR_STACK_RECORD_MISSING));
    let failed = record_for(buffer, "stack record read failed");
    assert_eq!(failed.level, crate::logging::LogLevel::Warn);
}

// GTC-FR-SKDF, GTC-FR-GJFM: every request of the stack operations goes to the
// REST client of the repository, and each stack write is logged with the
// owner, the repository and its kind.
#[test]
fn the_stack_operations_reach_github_only_through_its_rest_endpoints() {
    let f = fixture(&["a", "b", "c"]);
    let github = stacked_github();
    let machine = Machine::new();
    add(&f, &github, Some(&machine.root()), "c", "b").unwrap();
    github.queue_post(Ok(created_json(3)));
    github.queue_post(Ok(stack_json(7, &["a", "b", "c"])));
    let buffer = own_buffer();

    submit(&f, &github, &machine, "c", "b", buffer).unwrap();

    let reached: Vec<String> = github
        .paths()
        .into_iter()
        .chain(github.posts().into_iter().map(|(_, path, _)| path))
        .collect();
    assert!(!reached.is_empty());
    assert!(reached.iter().all(|path| path.starts_with(&format!("{REPO}/"))), "{reached:?}");
    let write = record_for(buffer, "stack write");
    assert_eq!(write.fields.get("owner"), Some(&json!("acme")));
    assert_eq!(write.fields.get("repo"), Some(&json!("platform")));
    assert_eq!(write.fields.get("write"), Some(&json!("add")));
    assert_eq!(write.fields.get("state"), Some(&json!("added")));
}

//! Which stack a branch is in (GTC-FR-WHEO, GTC-FR-XCMQ, GTC-FR-ZPUA,
//! GTC-FR-HEXN). The resolution is pure, so these tests give it the open pull
//! requests, the GitHub stack, and the record directly.

use crate::git::pull_requests::stack_model::{
    chain_up_to, first_stack, resolve_branch_stack, GithubStack, OpenPullRequest, StackInputs,
};
use crate::git::pull_requests::stack_record::StackRecordEntry;
use serde_json::json;

fn pr(number: u64, head: &str, base: &str) -> OpenPullRequest {
    OpenPullRequest {
        number,
        title: format!("Change {number}"),
        head: head.into(),
        base: base.into(),
    }
}

fn entry(branch: &str, parent: &str, stack: Option<u64>) -> StackRecordEntry {
    StackRecordEntry { branch: branch.into(), parent: parent.into(), stack }
}

fn stack(number: u64, base: &str, heads: &[&str]) -> GithubStack {
    GithubStack { number, base: base.into(), heads: heads.iter().map(|h| h.to_string()).collect() }
}

// GTC-FR-WHEO, GTC-FR-XCMQ, GTC-FR-ZPUA
#[test]
fn a_branch_in_a_github_stack_has_the_branch_below_as_its_parent_and_the_last_as_its_top() {
    let open = [pr(1, "a", "main"), pr(2, "b", "a"), pr(3, "c", "b")];
    let github = stack(7, "main", &["a", "b", "c"]);
    let inputs = StackInputs { open: &open, github: Some(&github), records: &[] };

    let middle = resolve_branch_stack("b", &inputs);
    assert!(middle.available && middle.in_stack);
    assert_eq!(middle.source.as_deref(), Some("github"));
    assert_eq!(middle.stack_number, Some(7));
    assert_eq!(middle.parent.as_deref(), Some("a"));
    assert_eq!(middle.top.as_deref(), Some("c"));
    assert!(!middle.is_top);
    assert!(middle.top_has_pull_request, "the top c has an open pull request");
    assert_eq!(middle.pull_request.as_ref().map(|p| p.number), Some(2));

    let bottom = resolve_branch_stack("a", &inputs);
    assert_eq!(bottom.parent.as_deref(), Some("main"), "the bottom stands on the stack base");
    let top = resolve_branch_stack("c", &inputs);
    assert!(top.is_top);
}

// GTC-FR-WHEO, GTC-FR-XCMQ, GTC-FR-ZPUA
#[test]
fn a_recorded_branch_is_in_the_stack_of_its_record_and_raises_the_top_above_its_parent() {
    let open = [pr(1, "a", "main"), pr(2, "b", "a")];
    let github = stack(7, "main", &["a", "b"]);
    let records = [entry("c", "b", Some(7))];
    let for_b = StackInputs { open: &open, github: Some(&github), records: &records };

    let b = resolve_branch_stack("b", &for_b);
    assert_eq!(b.top.as_deref(), Some("c"), "the record above b is the top");
    assert!(!b.is_top);

    let for_c = StackInputs { open: &open, github: None, records: &records };
    let c = resolve_branch_stack("c", &for_c);
    assert!(c.in_stack);
    assert_eq!(c.source.as_deref(), Some("local_record"));
    assert_eq!(c.parent.as_deref(), Some("b"));
    assert_eq!(c.stack_number, Some(7));
    assert!(c.is_top);
    assert!(c.pull_request.is_none());
    assert!(!c.top_has_pull_request);
}

// GTC-FR-ZPUA
#[test]
fn a_stack_top_with_no_open_pull_request_is_reported_from_every_layer() {
    let open = [pr(1, "a", "main"), pr(2, "b", "a")];
    let github = stack(7, "main", &["a", "b"]);
    let records = [entry("c", "b", Some(7))];
    let inputs = StackInputs { open: &open, github: Some(&github), records: &records };

    for layer in ["a", "b"] {
        let answer = resolve_branch_stack(layer, &inputs);
        assert_eq!(answer.top.as_deref(), Some("c"), "layer {layer}");
        assert!(answer.pull_request.is_some(), "layer {layer} has a pull request");
        assert!(!answer.top_has_pull_request, "layer {layer}");
    }
    let none = resolve_branch_stack("zzz", &inputs);
    assert!(!none.in_stack && !none.top_has_pull_request);
}

// GTC-FR-WHEO, GTC-FR-XCMQ
#[test]
fn a_chain_of_open_pull_requests_is_a_stack_when_github_holds_none() {
    let open = [pr(1, "a", "main"), pr(2, "b", "a")];
    let inputs = StackInputs { open: &open, github: None, records: &[] };

    let b = resolve_branch_stack("b", &inputs);
    assert_eq!(b.source.as_deref(), Some("pull_request_chain"));
    assert_eq!(b.parent.as_deref(), Some("a"));
    assert!(b.is_top);
    assert!(b.stack_number.is_none());

    let a = resolve_branch_stack("a", &inputs);
    assert_eq!(a.source.as_deref(), Some("pull_request_chain"), "a pull request below is in it");
    assert_eq!(a.top.as_deref(), Some("b"));
}

// GTC-FR-WHEO
#[test]
fn a_lone_pull_request_or_a_branch_with_nothing_is_in_no_stack() {
    let open = [pr(1, "a", "main"), pr(5, "z", "main")];
    let inputs = StackInputs { open: &open, github: None, records: &[] };
    let a = resolve_branch_stack("a", &inputs);
    assert!(a.available && !a.in_stack);
    assert_eq!(a.pull_request.as_ref().map(|p| p.number), Some(1));
    assert!(a.parent.is_none() && a.top.is_none());
    assert!(!resolve_branch_stack("nothing", &inputs).in_stack);
}

// GTC-FR-WHEO: a GitHub stack wins over a record of the same branch.
#[test]
fn a_github_stack_wins_over_the_record_of_the_same_branch() {
    let open = [pr(1, "a", "main"), pr(2, "b", "a")];
    let github = stack(9, "main", &["a", "b"]);
    let records = [entry("b", "other", None)];
    let inputs = StackInputs { open: &open, github: Some(&github), records: &records };
    let b = resolve_branch_stack("b", &inputs);
    assert_eq!(b.source.as_deref(), Some("github"));
    assert_eq!(b.parent.as_deref(), Some("a"));
}

// GTC-FR-ZPUA: the lowest number, then the first name, and a loop ends the walk.
#[test]
fn the_walk_up_takes_the_lowest_number_then_the_first_name_and_ends_on_a_loop() {
    let open = [pr(1, "a", "main"), pr(4, "d", "a"), pr(3, "c", "a")];
    let inputs = StackInputs { open: &open, github: None, records: &[] };
    assert_eq!(resolve_branch_stack("a", &inputs).top.as_deref(), Some("c"));

    let records = [entry("y", "x", None), entry("w", "x", None), entry("x", "w", None)];
    let inputs = StackInputs { open: &[], github: None, records: &records };
    let x = resolve_branch_stack("x", &inputs);
    assert_eq!(x.top.as_deref(), Some("w"), "the walk stops where it would go round");
}

// GTC-FR-HEXN
#[test]
fn the_chain_below_a_base_is_listed_from_the_bottom_up() {
    let open = [pr(3, "c", "b"), pr(1, "a", "main"), pr(2, "b", "a"), pr(9, "x", "main")];
    assert_eq!(chain_up_to(&open, "c"), vec![1, 2, 3]);
    assert_eq!(chain_up_to(&open, "a"), vec![1]);
    assert!(chain_up_to(&open, "none").is_empty());
}

// GTC-FR-TBQW
#[test]
fn the_first_stack_of_an_answer_is_read_and_a_wrong_answer_is_an_error() {
    let answer = json!([{
        "number": 12,
        "base": { "ref": "main" },
        "pull_requests": [
            { "number": 1, "head": { "ref": "a" } },
            { "number": 2, "head": { "ref": "b" } },
        ],
    }]);
    assert_eq!(first_stack(&answer).unwrap(), Some(stack(12, "main", &["a", "b"])));
    assert_eq!(first_stack(&json!([])).unwrap(), None);
    assert!(first_stack(&json!({ "message": "x" })).is_err());
}

//! The pure parts of the job queue: which evaluation a change of the Git facts
//! needs, the pass fingerprint, and the job labels.

use super::*;
use crate::embeddings::layers::{Chunk, Chunks, Head};

fn repository(tip: Option<&str>, head: Head) -> GitFacts {
    GitFacts::Repository {
        prefix: String::new(),
        root_branch: Some("main".to_string()),
        root_tip: tip.map(str::to_string),
        head,
        branches: vec!["main".to_string()],
    }
}

fn on(branch: &str) -> Head {
    Head::Branch(branch.to_string())
}

// EMB-FR-VQSK, EMB-FR-NHQG, EMB-FR-HTIQ: a moved root tip needs a root update
// (which also evaluates the branch layer); a branch, a `HEAD`, or a repository
// change needs a branch update; nothing else needs a full evaluation.
#[test]
fn a_change_of_the_git_facts_chooses_its_evaluation() {
    let same = repository(Some("a"), on("main"));
    assert_eq!(moved(&same, &same.clone()), None);
    assert_eq!(moved(&same, &repository(Some("b"), on("main"))), Some(JobKind::RootUpdate));
    assert_eq!(moved(&same, &repository(Some("a"), on("feature"))), Some(JobKind::BranchUpdate));
    assert_eq!(moved(&same, &repository(Some("a"), Head::Detached)), Some(JobKind::BranchUpdate));
    assert_eq!(
        moved(&same, &repository(Some("b"), on("feature"))),
        Some(JobKind::RootUpdate),
        "a root update evaluates the branch layer too"
    );
    assert_eq!(moved(&GitFacts::NoRepository, &same), Some(JobKind::RootUpdate));
    assert_eq!(
        moved(&repository(None, Head::Detached), &GitFacts::NoRepository),
        Some(JobKind::BranchUpdate),
        "only the move out of the repository differs"
    );
    let mut more_branches = same.clone();
    if let GitFacts::Repository { branches, .. } = &mut more_branches {
        branches.push("feature".to_string());
    }
    assert_eq!(moved(&same, &more_branches), None, "a new branch elsewhere changes no layer");
}

// EMB-FR-BZZT, EMB-FR-JIJH: the fingerprint of a pass that did not read the
// documents differs from that of a pass that read no documents, so the first
// pass that reads them is never skipped as unchanged.
#[test]
fn documents_that_were_not_read_give_a_fingerprint_of_their_own() {
    let mut working = Chunks::new();
    working.insert("a.md#0".to_string(), Chunk { hash: "h".to_string(), text: "t".to_string() });
    let empty = Chunks::new();
    assert_ne!(fingerprint_of(&working, None), fingerprint_of(&working, Some(&empty)));
    assert_eq!(fingerprint_of(&working, Some(&empty)), fingerprint_of(&working, Some(&empty)));
}

// EMB-FR-WJCW: the label of each job kind.
#[test]
fn each_job_kind_has_its_label() {
    let labels = [
        (JobKind::FirstIndexing, "Embedding specifications…"),
        (JobKind::OpenCheck, "Checking embeddings…"),
        (JobKind::BranchUpdate, "Checking embeddings…"),
        (JobKind::RootUpdate, "Checking embeddings…"),
        (JobKind::Incremental, "Updating embeddings…"),
        (JobKind::ModelSwitch, "Switching embedding model…"),
        (JobKind::StaleRebuild, "Rebuilding embeddings…"),
        (JobKind::Reindex, "Reindexing embeddings…"),
    ];
    for (kind, label) in labels {
        assert_eq!(kind.label(), label, "{kind:?}");
    }
}

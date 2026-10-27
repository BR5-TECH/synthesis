//! Tests of one layer evaluation against a real store and real Git objects,
//! with the fake embedder (no model, no network).

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;

use git2::Repository;

use super::*;
use crate::embeddings::layers::test_support::{commit, init};
use crate::embeddings::layers::{git_facts, root_chunks, Chunk, Chunks, GitFacts};
use crate::embeddings::model::fake::vector_of;
use crate::embeddings::store::{LayerKind, Store};

struct Fixture {
    repo_dir: tempfile::TempDir,
    store_dir: tempfile::TempDir,
    repo: Repository,
}

impl Fixture {
    fn new(files: &[(&str, &str)]) -> Fixture {
        let repo_dir = tempfile::tempdir().unwrap();
        let repo = init(repo_dir.path());
        commit(&repo, files);
        Fixture { repo_dir, store_dir: tempfile::tempdir().unwrap(), repo }
    }

    fn store(&self) -> Store {
        Store::open(&crate::fs::RootFs::for_root(self.store_dir.path()), "fake").unwrap().0
    }

    fn facts(&self) -> GitFacts {
        git_facts(self.repo_dir.path())
    }

    fn branch(&self, name: &str) {
        let head = self.repo.head().unwrap().peel_to_commit().unwrap();
        self.repo.branch(name, &head, false).unwrap();
        self.repo.set_head(&format!("refs/heads/{name}")).unwrap();
    }
}

/// The working copy's chunks, as the BM25 snapshot would give them.
fn working(files: &[(&str, &str)]) -> Chunks {
    let mut out = Chunks::new();
    for (path, text) in files {
        for (ordinal, chunk) in
            crate::bm25_index::chunk_file_text(text, false).into_iter().enumerate()
        {
            let id = crate::bm25_index::chunk_id(path, ordinal as u32);
            out.insert(id, Chunk { hash: layers::hash_text(&chunk), text: chunk });
        }
    }
    out
}

/// Run one evaluation, counting embedded texts and the size of each batch.
struct Run {
    embedded: RefCell<Vec<String>>,
    batches: RefCell<Vec<usize>>,
    cancel: Cell<bool>,
    fail: Cell<bool>,
}

impl Run {
    fn new() -> Run {
        Run {
            embedded: RefCell::new(Vec::new()),
            batches: RefCell::new(Vec::new()),
            cancel: Cell::new(false),
            fail: Cell::new(false),
        }
    }

    fn eval(
        &self,
        store: &mut Store,
        fx: &Fixture,
        work: &Chunks,
        docs: &Chunks,
        skip_root: bool,
        drop: Option<ReindexScope>,
    ) -> Result<Outcome, String> {
        let root_dir = fx.repo_dir.path().to_path_buf();
        let reader = move |tip: &str, prefix: &str| {
            let repo = Repository::open(&root_dir).unwrap();
            root_chunks(&repo, tip, prefix)
        };
        let facts = fx.facts();
        let mut embed = |texts: &[String]| -> Result<Vec<Vec<f32>>, String> {
            if self.fail.get() {
                return Err("model failed".to_string());
            }
            self.batches.borrow_mut().push(texts.len());
            self.embedded.borrow_mut().extend(texts.iter().cloned());
            Ok(texts.iter().map(|t| vector_of(t)).collect())
        };
        let cancelled = || self.cancel.get();
        let mut progress = |_: u64, _: u64| {};
        evaluate(
            store,
            &Evaluation {
                facts: &facts,
                working: work,
                documents: docs,
                skip_root,
                drop,
                root_reader: &reader,
            },
            &mut Job { embed: &mut embed, cancelled: &cancelled, progress: &mut progress },
        )
    }

    fn take(&self) -> Vec<String> {
        std::mem::take(&mut *self.embedded.borrow_mut())
    }
}

const SPEC_A: &str =
    "# Worktrees\n\nHow a worktree is torn down.\n\n# Branches\n\nHow a branch is created.";
const SPEC_B: &str = "# Search\n\nThe search bar finds files.";

fn view_ids(store: &Store, outcome: &Outcome) -> BTreeMap<String, String> {
    store.view_entries(outcome.spec_view).unwrap()
}

// EMB-FR-LGLI, EMB-FR-OVPR, EMB-FR-DCXI: the first indexing reads the root
// layer from Git objects, and on the root branch with a clean working copy the
// branch layer is empty.
#[test]
fn first_indexing_builds_the_root_layer_and_an_empty_branch_layer() {
    let files = [("specifications/a.md", SPEC_A), ("specifications/b.md", SPEC_B)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    let outcome = run.eval(&mut store, &fx, &working(&files), &Chunks::new(), false, None).unwrap();
    assert_eq!(outcome.spec_chunks, 3);
    assert_eq!(run.take().len(), 3, "every root chunk is embedded once");
    let branch = outcome.spec_view.branch.unwrap();
    assert!(
        store.entries(branch).unwrap().is_empty(),
        "no chunk equal to the root is stored again"
    );
    let root = store.find_layer(LayerKind::Root, None).unwrap().unwrap();
    assert_eq!(root.tip.as_deref(), fx.facts().root_tip());
}

// EMB-FR-DCXI, EMB-FR-DSUG, EMB-FR-KNBG: a branch edit stores only the
// difference; the view replaces the edited chunk and drops the deleted file.
#[test]
fn a_branch_stores_only_its_difference_and_the_view_lays_it_over_the_root() {
    let files = [("specifications/a.md", SPEC_A), ("specifications/b.md", SPEC_B)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    run.eval(&mut store, &fx, &working(&files), &Chunks::new(), false, None).unwrap();
    run.take();

    fx.branch("feature");
    let edited = SPEC_A.replace("How a branch is created.", "How a branch is renamed.");
    let work = working(&[("specifications/a.md", edited.as_str())]);
    let outcome = run.eval(&mut store, &fx, &work, &Chunks::new(), false, None).unwrap();
    assert_eq!(
        run.take(),
        [work["specifications/a.md#1"].text.clone()],
        "only the changed chunk is embedded"
    );

    let branch = store.entries(outcome.spec_view.branch.unwrap()).unwrap();
    assert_eq!(branch.len(), 2);
    assert_eq!(branch["specifications/a.md#1"], Some(work["specifications/a.md#1"].hash.clone()));
    assert_eq!(branch["specifications/b.md#0"], None, "a deleted root chunk has a deletion marker");
    let view = view_ids(&store, &outcome);
    assert_eq!(
        view.keys().map(String::as_str).collect::<Vec<_>>(),
        ["specifications/a.md#0", "specifications/a.md#1"]
    );
    assert_eq!(view["specifications/a.md#1"], work["specifications/a.md#1"].hash);
}

// EMB-FR-MBJV, EMB-FR-BZZT: a chunk edited back to its root text leaves the
// branch layer, with nothing embedded.
#[test]
fn a_branch_chunk_equal_to_the_root_again_leaves_the_layer() {
    let files = [("specifications/a.md", SPEC_A)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    run.eval(&mut store, &fx, &working(&files), &Chunks::new(), false, None).unwrap();
    fx.branch("feature");
    let edited = SPEC_A.replace("torn down", "removed");
    let outcome = run
        .eval(
            &mut store,
            &fx,
            &working(&[("specifications/a.md", edited.as_str())]),
            &Chunks::new(),
            true,
            None,
        )
        .unwrap();
    assert_eq!(store.entries(outcome.spec_view.branch.unwrap()).unwrap().len(), 1);
    run.take();
    let outcome = run.eval(&mut store, &fx, &working(&files), &Chunks::new(), true, None).unwrap();
    assert!(store.entries(outcome.spec_view.branch.unwrap()).unwrap().is_empty());
    assert!(run.take().is_empty(), "the root vector is reused");
}

// EMB-FR-NTMA: a detached HEAD has no branch layer; the view is the root.
#[test]
fn a_detached_head_uses_the_root_layer_only() {
    let files = [("specifications/a.md", SPEC_A)];
    let fx = Fixture::new(&files);
    let tip = fx.repo.head().unwrap().peel_to_commit().unwrap().id();
    fx.repo.set_head_detached(tip).unwrap();
    let mut store = fx.store();
    let outcome = Run::new()
        .eval(
            &mut store,
            &fx,
            &working(&[("specifications/a.md", "# Other\n\ntext")]),
            &Chunks::new(),
            false,
            None,
        )
        .unwrap();
    assert_eq!(outcome.spec_view.branch, None);
    assert_eq!(outcome.spec_chunks, 2, "the root layer alone, whatever the working copy holds");
}

// EMB-FR-MGML: outside Git, one layer from the working copy.
#[test]
fn a_project_outside_git_has_one_working_copy_layer() {
    let outside = tempfile::tempdir().unwrap();
    let store_dir = tempfile::tempdir().unwrap();
    let mut store = Store::open(&crate::fs::RootFs::for_root(store_dir.path()), "fake").unwrap().0;
    let facts = git_facts(outside.path());
    assert_eq!(facts, GitFacts::NoRepository);
    let work = working(&[("specifications/a.md", SPEC_A)]);
    let reader = |_: &str, _: &str| -> Result<layers::RootRead, String> {
        panic!("no root layer outside Git")
    };
    let mut embed = |texts: &[String]| -> Result<Vec<Vec<f32>>, String> {
        Ok(texts.iter().map(|t| vector_of(t)).collect())
    };
    let mut progress = |_: u64, _: u64| {};
    let outcome = evaluate(
        &mut store,
        &Evaluation {
            facts: &facts,
            working: &work,
            documents: &Chunks::new(),
            skip_root: false,
            drop: None,
            root_reader: &reader,
        },
        &mut Job { embed: &mut embed, cancelled: &|| false, progress: &mut progress },
    )
    .unwrap();
    assert_eq!(outcome.spec_view.root, None);
    assert_eq!(outcome.spec_chunks, 2);
    assert_eq!(
        store.find_layer(LayerKind::Worktree, None).unwrap().unwrap().id,
        outcome.spec_view.branch.unwrap()
    );
}

// EMB-FR-DQVX, EMB-FR-USBA: when the root tip moves, the root layer is updated
// by content hash and records the new tip; only new chunks are embedded.
#[test]
fn a_moved_root_tip_updates_the_root_layer_by_hash() {
    let files = [("specifications/a.md", SPEC_A)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    run.eval(&mut store, &fx, &working(&files), &Chunks::new(), false, None).unwrap();
    run.take();
    let new_b = "# New\n\nA new specification on the root branch.";
    commit(&fx.repo, &[("specifications/b.md", new_b)]);
    let both = [("specifications/a.md", SPEC_A), ("specifications/b.md", new_b)];
    let outcome = run.eval(&mut store, &fx, &working(&both), &Chunks::new(), false, None).unwrap();
    assert_eq!(run.take(), crate::bm25_index::chunk_file_text(new_b, false));
    let root = store.find_layer(LayerKind::Root, None).unwrap().unwrap();
    assert_eq!(root.tip.as_deref(), fx.facts().root_tip());
    assert_eq!(outcome.spec_chunks, 3);
    let branch = store.find_layer(LayerKind::Branch, fx.facts().branch()).unwrap().unwrap();
    assert_eq!(
        branch.root_tip.as_deref(),
        fx.facts().root_tip(),
        "the branch layer records the root tip it was compared with"
    );
}

// EMB-FR-BZZT: an incremental pass leaves the root layer alone even when the
// tip moved; the worker escalates that case to a full evaluation.
#[test]
fn an_incremental_pass_does_not_read_the_root_again() {
    let files = [("specifications/a.md", SPEC_A)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    run.eval(&mut store, &fx, &working(&files), &Chunks::new(), false, None).unwrap();
    let old_tip = store.find_layer(LayerKind::Root, None).unwrap().unwrap().tip;
    commit(&fx.repo, &[("specifications/b.md", SPEC_B)]);
    run.eval(&mut store, &fx, &working(&files), &Chunks::new(), true, None).unwrap();
    assert_eq!(store.find_layer(LayerKind::Root, None).unwrap().unwrap().tip, old_tip);
}

// EMB-FR-PYCS, EMB-FR-IBXK: documents have one layer, compared by hash; a
// removed document loses its entries.
#[test]
fn documents_have_one_layer_compared_by_hash() {
    let files = [("specifications/a.md", SPEC_A)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    let docs = working(&[("doc-1", "# Manual\n\nRate limits."), ("doc-2", "# Other\n\nText.")]);
    let outcome = run.eval(&mut store, &fx, &working(&files), &docs, false, None).unwrap();
    assert_eq!(outcome.document_chunks, 2);
    run.take();
    let docs = working(&[("doc-1", "# Manual\n\nRate limits.")]);
    let outcome = run.eval(&mut store, &fx, &working(&files), &docs, true, None).unwrap();
    assert_eq!(outcome.document_chunks, 1);
    assert!(run.take().is_empty());
}

// EMB-FR-HRYV, EMB-FR-RXCL: Current branch embeds the branch and documents
// layers again and keeps the root; Whole trace embeds all of them again.
#[test]
fn reindex_scopes_embed_their_layers_again() {
    let files = [("specifications/a.md", SPEC_A)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    let docs = working(&[("doc-1", "# Manual\n\nRate limits.")]);
    run.eval(&mut store, &fx, &working(&files), &docs, false, None).unwrap();
    fx.branch("feature");
    let edited = SPEC_A.replace("torn down", "removed");
    let work = working(&[("specifications/a.md", edited.as_str())]);
    run.eval(&mut store, &fx, &work, &docs, false, None).unwrap();
    let root_before = store.find_layer(LayerKind::Root, None).unwrap().unwrap().id;
    run.take();

    run.eval(&mut store, &fx, &work, &docs, false, Some(ReindexScope::CurrentBranch)).unwrap();
    let embedded = run.take();
    assert_eq!(embedded.len(), 2, "the branch chunk and the document chunk: {embedded:?}");
    assert_eq!(
        store.find_layer(LayerKind::Root, None).unwrap().unwrap().id,
        root_before,
        "the root layer stays"
    );

    run.eval(&mut store, &fx, &work, &docs, false, Some(ReindexScope::WholeTrace)).unwrap();
    assert_eq!(run.take().len(), 4, "two root chunks, the branch chunk, and the document chunk");
}

// EMB-FR-QNWF: on a detached HEAD, Current branch embeds the documents only.
#[test]
fn reindex_current_branch_on_a_detached_head_embeds_documents_only() {
    let files = [("specifications/a.md", SPEC_A)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    let docs = working(&[("doc-1", "# Manual\n\nRate limits.")]);
    run.eval(&mut store, &fx, &working(&files), &docs, false, None).unwrap();
    let tip = fx.repo.head().unwrap().peel_to_commit().unwrap().id();
    fx.repo.set_head_detached(tip).unwrap();
    run.take();
    run.eval(&mut store, &fx, &working(&files), &docs, false, Some(ReindexScope::CurrentBranch))
        .unwrap();
    assert_eq!(run.take(), [docs["doc-1#0"].text.clone()]);
}

// EMB-FR-USBA: the layer of a branch that no longer exists is deleted.
#[test]
fn the_layer_of_a_deleted_branch_is_removed() {
    let files = [("specifications/a.md", SPEC_A)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    run.eval(&mut store, &fx, &working(&files), &Chunks::new(), false, None).unwrap();
    let main = fx.facts().branch().unwrap().to_string();
    fx.branch("gone");
    run.eval(&mut store, &fx, &working(&files), &Chunks::new(), false, None).unwrap();
    assert!(store.find_layer(LayerKind::Branch, Some("gone")).unwrap().is_some());
    fx.repo.set_head(&format!("refs/heads/{main}")).unwrap();
    fx.repo.find_branch("gone", git2::BranchType::Local).unwrap().delete().unwrap();
    run.eval(&mut store, &fx, &working(&files), &Chunks::new(), false, None).unwrap();
    assert!(store.find_layer(LayerKind::Branch, Some("gone")).unwrap().is_none());
}

// EMB-FR-EAEA: a pass that fails leaves the store as it was.
#[test]
fn a_failed_pass_leaves_the_store_unchanged() {
    let files = [("specifications/a.md", SPEC_A)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    let first = run.eval(&mut store, &fx, &working(&files), &Chunks::new(), false, None).unwrap();
    let before = view_ids(&store, &first);
    run.fail.set(true);
    let edited = SPEC_A.replace("torn down", "removed");
    let err = run.eval(
        &mut store,
        &fx,
        &working(&[("specifications/a.md", edited.as_str())]),
        &Chunks::new(),
        true,
        None,
    );
    assert_eq!(err.unwrap_err(), "model failed");
    assert_eq!(view_ids(&store, &first), before);
}

// EMB-FR-SWRV: a cancelled job stops and reports `cancelled`.
#[test]
fn a_cancelled_job_stops() {
    let files = [("specifications/a.md", SPEC_A)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    run.cancel.set(true);
    let err = run.eval(&mut store, &fx, &working(&files), &Chunks::new(), false, None).unwrap_err();
    assert_eq!(err, CANCELLED);
    assert!(run.take().is_empty());
}

// EMB-FR-USBA, EMB-FR-DCXI: the active branch keeps its layer while it is
// unborn — it is not yet in the list of branches.
#[test]
fn an_unborn_active_branch_keeps_its_layer() {
    let dir = tempfile::tempdir().unwrap();
    let repo = init(dir.path());
    // No commit: the branch HEAD names does not exist yet.
    let fx = Fixture { repo_dir: dir, store_dir: tempfile::tempdir().unwrap(), repo };
    let mut store = fx.store();
    let work = working(&[("specifications/a.md", SPEC_A)]);
    let run = Run::new();
    let outcome = run.eval(&mut store, &fx, &work, &Chunks::new(), false, None).unwrap();
    let branch = outcome.spec_view.branch.expect("the unborn branch has a layer");
    assert_eq!(store.entries(branch).unwrap().len(), 2);
    run.take();
    run.eval(&mut store, &fx, &work, &Chunks::new(), true, None).unwrap();
    assert!(run.take().is_empty(), "the layer survived, so nothing is embedded again");
}

// EMB-FR-EAEA: the whole pass is one transaction — a pass that fails on a
// later layer leaves the earlier layers as they were too, and a Reindex that
// fails keeps the layers it would have dropped.
#[test]
fn a_pass_that_fails_late_changes_no_layer() {
    let files = [("specifications/a.md", SPEC_A), ("specifications/b.md", SPEC_B)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    let docs = working(&[("doc-1", "# Manual\n\nRate limits.")]);
    let first = run.eval(&mut store, &fx, &working(&files), &docs, false, None).unwrap();
    let branch_before = store.entries(first.spec_view.branch.unwrap()).unwrap();
    let docs_before = store.entries(first.documents_layer.unwrap()).unwrap();

    // The branch change needs no embedding (a deletion only); the new
    // document does, and its embedding fails.
    run.fail.set(true);
    let docs2 = working(&[("doc-1", "# Manual\n\nRate limits."), ("doc-2", "# New\n\nText.")]);
    let only_a = [("specifications/a.md", SPEC_A)];
    assert!(run.eval(&mut store, &fx, &working(&only_a), &docs2, true, None).is_err());
    assert_eq!(store.entries(first.spec_view.branch.unwrap()).unwrap(), branch_before);
    assert_eq!(store.entries(first.documents_layer.unwrap()).unwrap(), docs_before);

    let layers = store.layers().unwrap();
    let reindex = Some(ReindexScope::WholeTrace);
    assert!(run.eval(&mut store, &fx, &working(&files), &docs, false, reindex).is_err());
    assert_eq!(store.layers().unwrap(), layers);
}

// EMB-FR-DQVX, EMB-FR-HBLQ: a moved root tip that deletes a file and changes a
// chunk removes their entries and the vectors no layer needs, and embeds only
// the new hash.
#[test]
fn a_moved_root_tip_removes_what_the_tip_no_longer_has() {
    let files = [("specifications/a.md", SPEC_A), ("specifications/b.md", SPEC_B)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    run.eval(&mut store, &fx, &working(&files), &Chunks::new(), false, None).unwrap();
    assert_eq!(store.vector_count().unwrap(), 3);
    run.take();

    let edited = SPEC_A.replace("torn down", "removed");
    let mut index = fx.repo.index().unwrap();
    index.remove_path(std::path::Path::new("specifications/b.md")).unwrap();
    index.write().unwrap();
    std::fs::remove_file(fx.repo_dir.path().join("specifications/b.md")).unwrap();
    commit(&fx.repo, &[("specifications/a.md", edited.as_str())]);
    let now = [("specifications/a.md", edited.as_str())];
    let outcome = run.eval(&mut store, &fx, &working(&now), &Chunks::new(), false, None).unwrap();
    assert_eq!(run.take(), [working(&now)["specifications/a.md#0"].text.clone()]);
    assert!(outcome.removed >= 1);
    assert_eq!(outcome.spec_chunks, 2);
    assert_eq!(store.vector_count().unwrap(), 2, "the old vectors are gone");
}

// EMB-FR-USBA: an active branch that has no commit of its own keeps its layer
// while other branches exist.
#[test]
fn an_orphan_active_branch_keeps_its_layer_beside_other_branches() {
    let files = [("specifications/a.md", SPEC_A)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    run.eval(&mut store, &fx, &working(&files), &Chunks::new(), false, None).unwrap();
    // HEAD names a branch that does not exist yet; `main` does.
    fx.repo.set_head("refs/heads/orphan").unwrap();
    let edited = SPEC_A.replace("torn down", "removed");
    let work = working(&[("specifications/a.md", edited.as_str())]);
    let outcome = run.eval(&mut store, &fx, &work, &Chunks::new(), true, None).unwrap();
    let layer = store.find_layer(LayerKind::Branch, Some("orphan")).unwrap();
    assert_eq!(layer.map(|l| l.id), outcome.spec_view.branch);
    assert_eq!(store.entries(outcome.spec_view.branch.unwrap()).unwrap().len(), 1);
}

// EMB-FR-JLRR: chunks are embedded in batches of at most sixteen.
#[test]
fn chunks_are_embedded_in_small_batches() {
    let texts: Vec<(String, String)> = (0..40)
        .map(|n| (format!("specifications/s{n:02}.md"), format!("# Topic {n}\n\nwords {n}")))
        .collect();
    let files: Vec<(&str, &str)> = texts.iter().map(|(p, t)| (p.as_str(), t.as_str())).collect();
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let run = Run::new();
    run.eval(&mut store, &fx, &working(&files), &Chunks::new(), false, None).unwrap();
    assert_eq!(*run.batches.borrow(), [16, 16, 8]);
}

// EMB-FR-EAEA: a model that returns a wrong number of vectors fails the pass,
// and the store does not change.
#[test]
fn a_wrong_number_of_vectors_fails_the_pass() {
    let files = [("specifications/a.md", SPEC_A)];
    let fx = Fixture::new(&files);
    let mut store = fx.store();
    let facts = fx.facts();
    let root_dir = fx.repo_dir.path().to_path_buf();
    let reader = move |tip: &str, prefix: &str| {
        root_chunks(&Repository::open(&root_dir).unwrap(), tip, prefix)
    };
    let mut embed = |_: &[String]| -> Result<Vec<Vec<f32>>, String> { Ok(vec![vec![1.0]]) };
    let mut progress = |_: u64, _: u64| {};
    let work = working(&files);
    let err = evaluate(
        &mut store,
        &Evaluation {
            facts: &facts,
            working: &work,
            documents: &Chunks::new(),
            skip_root: false,
            drop: None,
            root_reader: &reader,
        },
        &mut Job { embed: &mut embed, cancelled: &|| false, progress: &mut progress },
    )
    .unwrap_err();
    assert!(err.contains("wrong number"));
    assert!(store.layers().unwrap().is_empty());
}

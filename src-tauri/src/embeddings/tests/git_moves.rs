//! Changes of the Git facts that reach the service with no file change, or
//! together with one: a branch switch, a detach, a pull, and the branch layers
//! that a model switch removes (EMB-FR-HTIQ, EMB-FR-NHQG, EMB-FR-VQSK,
//! EMB-FR-DQVX, EMB-FR-OAJQ, EMB-FR-TGCL, EMB-FR-USBA).

use super::*;

impl Harness {
    fn create_branch(&self, name: &str) {
        let head = self.repo.head().unwrap().peel_to_commit().unwrap();
        self.repo.branch(name, &head, false).unwrap();
    }

    /// The layers of the store, read through a search connection.
    fn stored_branch_layers(&self) -> Vec<String> {
        let store = super::super::store::Store::open_existing(&self.store_file()).unwrap();
        store
            .layers()
            .unwrap()
            .into_iter()
            .filter(|layer| layer.kind == super::super::store::LayerKind::Branch)
            .filter_map(|layer| layer.branch)
            .collect()
    }

    /// Check out `branch` as `git switch` does: the files, then `HEAD`.
    fn checkout(&self, branch: &str) {
        let target = self.repo.revparse_single(&format!("refs/heads/{branch}")).unwrap();
        let mut options = git2::build::CheckoutBuilder::new();
        options.force();
        self.repo.checkout_tree(&target, Some(&mut options)).unwrap();
        self.repo.set_head(&format!("refs/heads/{branch}")).unwrap();
    }

    /// The chunk IDs and hashes of the current vector view equal those of the
    /// BM25 snapshot of the specifications.
    fn assert_view_matches_snapshot(&self) {
        let view =
            self.service().lock().project.as_ref().unwrap().outcome.as_ref().unwrap().spec_view;
        let store = super::super::store::Store::open_existing(&self.store_file()).unwrap();
        let snapshot = self.app.state::<Bm25Indexer>().snapshot();
        let working: std::collections::BTreeMap<String, String> =
            super::super::layers::chunks_from(snapshot.chunks(crate::bm25_index::IndexId::Spec))
                .into_iter()
                .map(|(id, chunk)| (id, chunk.hash))
                .collect();
        assert_eq!(store.view_entries(view).unwrap(), working);
    }

    /// The chunk IDs of the current vector view.
    fn view_ids(&self) -> Vec<String> {
        let view =
            self.service().lock().project.as_ref().unwrap().outcome.as_ref().unwrap().spec_view;
        let store = super::super::store::Store::open_existing(&self.store_file()).unwrap();
        store.view_entries(view).unwrap().into_keys().collect()
    }

    /// The entries of the layer of `branch`.
    fn branch_entries(&self, branch: &str) -> super::super::store::Entries {
        use super::super::store::{LayerKind, Store};
        let store = Store::open_existing(&self.store_file()).unwrap();
        let layer = store.find_layer(LayerKind::Branch, Some(branch)).unwrap().unwrap();
        store.entries(layer.id).unwrap()
    }

    /// The root tip the status shows, the tip the root layer holds, and the
    /// root tip the active branch layer was compared with are one commit.
    fn assert_layers_match_facts(&self) {
        use super::super::store::{LayerKind, Store};
        let status = self.service().status().project.unwrap();
        let store = Store::open_existing(&self.store_file()).unwrap();
        let root = store.find_layer(LayerKind::Root, None).unwrap().unwrap();
        assert_eq!(root.tip, status.root_tip);
        if let Some(branch) = status.branch.as_deref() {
            let layer = store.find_layer(LayerKind::Branch, Some(branch)).unwrap().unwrap();
            assert_eq!(layer.root_tip, status.root_tip);
        }
    }
}

// EMB-FR-HTIQ, EMB-FR-NHQG: a branch switch that changes no file — `git
// switch` in a terminal — is found by the root check of the Git-metadata
// watch and evaluates the layers for the new branch.
#[test]
fn a_branch_switch_with_no_file_change_updates_the_branch_layer() {
    let h = Harness::new();
    h.ready();
    let jobs = h.jobs().len();
    h.create_branch("feature");
    h.repo.set_head("refs/heads/feature").unwrap();
    h.git_change();
    h.settle(Instant::now());
    assert_eq!(h.jobs()[jobs..], [JobKind::BranchUpdate]);
    let project = h.service().status().project.unwrap();
    assert_eq!(project.branch.as_deref(), Some("feature"));
    assert_eq!(project.state, Phase::Ready);
    h.assert_layers_match_facts();
}

// EMB-FR-HTIQ, EMB-FR-NTMA: a detach from a terminal evaluates the layers
// again; the view is then the root layer only, and search stays ready.
#[test]
fn a_detached_head_from_a_terminal_is_found_by_the_root_check() {
    let h = Harness::new();
    h.ready();
    let jobs = h.jobs().len();
    let tip = h.repo.head().unwrap().peel_to_commit().unwrap().id();
    h.repo.set_head_detached(tip).unwrap();
    h.git_change();
    h.settle(Instant::now());
    assert_eq!(h.jobs()[jobs..], [JobKind::BranchUpdate]);
    let project = h.service().status().project.unwrap();
    assert!(project.detached);
    assert_eq!(project.branch, None);
    assert!(h.ready_for_spec());

    h.repo.set_head("refs/heads/main").unwrap();
    h.git_change();
    h.settle(Instant::now());
    assert_eq!(h.jobs()[jobs..], [JobKind::BranchUpdate, JobKind::BranchUpdate]);
    assert_eq!(h.service().status().project.unwrap().branch.as_deref(), Some("main"));
}

// EMB-FR-VQSK: a root check while the root tip and the branch both moved runs
// one root update, which also evaluates the branch layer.
#[test]
fn a_root_check_after_a_tip_move_and_a_switch_runs_one_root_update() {
    let h = Harness::new();
    h.ready();
    let jobs = h.jobs().len();
    commit(&h.repo, &[("specifications/c.md", "# Third\n\nA committed specification.")]);
    h.create_branch("feature");
    h.repo.set_head("refs/heads/feature").unwrap();
    h.git_change();
    h.settle(Instant::now());
    assert_eq!(h.jobs()[jobs..], [JobKind::RootUpdate]);
    assert_eq!(h.service().status().project.unwrap().branch.as_deref(), Some("feature"));
    h.assert_layers_match_facts();
}

// EMB-FR-OAJQ, EMB-FR-DQVX, EMB-FR-MBJV, EMB-FR-KPWZ: a pull moves the root
// tip and the working copy together. The root check finds the move, and the
// pass after it gives one root update, which embeds only the new chunks and
// leaves the branch layer empty.
#[test]
fn a_pull_updates_the_root_layer_in_one_pass() {
    let h = Harness::new();
    h.ready();
    let jobs = h.jobs().len();
    let embedded = h.embedded();
    let new_c = "# Third\n\nA pulled specification.";
    commit(&h.repo, &[("specifications/c.md", new_c)]);
    h.git_change();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs()[jobs..], [JobKind::RootUpdate]);
    assert_eq!(h.embedded() - embedded, 1, "the pulled chunk only");
    let project = h.service().status().project.unwrap();
    assert_eq!(project.spec_chunks, 4);
    h.assert_layers_match_facts();
    let store = super::super::store::Store::open_existing(&h.store_file()).unwrap();
    let branch =
        store.find_layer(super::super::store::LayerKind::Branch, Some("main")).unwrap().unwrap();
    assert!(store.entries(branch.id).unwrap().is_empty(), "the working copy equals the root");
}

// EMB-FR-DQVX, EMB-FR-EAEA: a root update that fails records no new tip, so
// the next root check runs the root update again.
#[test]
fn a_failed_root_update_is_tried_again_by_the_next_root_check() {
    let h = Harness::new();
    h.ready();
    let old_tip = h.service().status().project.unwrap().root_tip;
    commit(&h.repo, &[("specifications/c.md", "# Third\n\nA committed specification.")]);
    h.log.fail_embed.store(true, Ordering::SeqCst);
    h.git_change();
    h.settle(Instant::now());
    let project = h.service().status().project.unwrap();
    assert!(project.last_error.is_some());
    assert_eq!(project.root_tip, old_tip);

    h.log.fail_embed.store(false, Ordering::SeqCst);
    let jobs = h.jobs().len();
    h.git_change();
    h.settle(Instant::now());
    assert_eq!(h.jobs()[jobs..], [JobKind::RootUpdate]);
    let project = h.service().status().project.unwrap();
    assert_ne!(project.root_tip, old_tip);
    assert_eq!(project.last_error, None);
    h.assert_layers_match_facts();
}

// EMB-FR-TGCL, EMB-FR-USBA: a branch keeps its layer while main is active; a
// model switch removes it.
#[test]
fn a_model_switch_removes_the_layers_of_inactive_branches() {
    let h = Harness::new();
    h.ready();
    h.create_branch("feature");
    h.repo.set_head("refs/heads/feature").unwrap();
    h.write("specifications/b.md", "# Search\n\nFeature text.");
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    assert!(h.stored_branch_layers().contains(&"feature".to_string()));

    h.repo.set_head("refs/heads/main").unwrap();
    h.write("specifications/b.md", SPEC_B);
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    assert!(h.stored_branch_layers().contains(&"feature".to_string()));

    h.service()
        .apply_settings(EmbeddingsRecord { enabled: true, model: Some("AllMiniLML6V2".into()) });
    h.settle(Instant::now());
    assert_eq!(h.jobs().last(), Some(&JobKind::ModelSwitch));
    assert_eq!(h.stored_branch_layers(), ["main".to_string()]);
}

// EMB-FR-DQVX, EMB-FR-OAJQ, EMB-FR-KPWZ: an incremental pass whose Git facts
// moved after the last job — a pull that reaches the service before any root
// check — runs no job, and the root update waits for a fresh BM25 pass. The
// facts it records then match the layers it wrote, and a root check after it
// finds no move.
#[test]
fn an_incremental_pass_after_a_tip_move_becomes_a_root_update() {
    let h = Harness::new();
    h.ready();
    let jobs = h.jobs().len();
    let asked = h.passes_asked();
    commit(&h.repo, &[("specifications/c.md", "# Third\n\nA pulled specification.")]);
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs().len(), jobs, "the evaluation waits for a fresh pass");
    assert!(h.awaiting());
    assert_eq!(h.passes_asked(), asked + 1);
    h.requested_pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs()[jobs..], [JobKind::RootUpdate]);
    assert_eq!(h.service().status().project.unwrap().spec_chunks, 4);
    h.assert_layers_match_facts();
    h.service().request_root_check();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs()[jobs..], [JobKind::RootUpdate], "the facts did not move again");
}

// EMB-FR-DQVX, EMB-FR-NHQG, EMB-FR-KPWZ: a pass after a branch switch, with
// no root check before it, runs no job; the branch update for the new branch
// runs after the fresh BM25 pass.
#[test]
fn an_incremental_pass_after_a_branch_switch_becomes_a_branch_update() {
    let h = Harness::new();
    h.ready();
    let jobs = h.jobs().len();
    h.create_branch("feature");
    h.repo.set_head("refs/heads/feature").unwrap();
    h.write("specifications/b.md", "# Search\n\nFeature text.");
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs().len(), jobs, "the evaluation waits for a fresh pass");
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs()[jobs..], [JobKind::BranchUpdate]);
    assert_eq!(h.service().status().project.unwrap().branch.as_deref(), Some("feature"));
    h.assert_layers_match_facts();
}

// EMB-FR-USBA: back on main, a full evaluation keeps the layer of a branch
// that no worktree has checked out, and removes it once the branch is deleted.
#[test]
fn a_branch_with_no_worktree_keeps_its_layer_until_it_is_deleted() {
    let h = Harness::new();
    h.ready();
    h.create_branch("feature");
    h.repo.set_head("refs/heads/feature").unwrap();
    h.write("specifications/b.md", "# Search\n\nFeature text.");
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    assert!(h.stored_branch_layers().contains(&"feature".to_string()));

    h.repo.set_head("refs/heads/main").unwrap();
    h.write("specifications/b.md", SPEC_B);
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    let mut layers = h.stored_branch_layers();
    layers.sort();
    assert_eq!(layers, ["feature".to_string(), "main".to_string()]);

    h.repo.find_branch("feature", git2::BranchType::Local).unwrap().delete().unwrap();
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    assert_eq!(h.stored_branch_layers(), ["main".to_string()]);
}

// EMB-FR-KPWZ, EMB-FR-HTIQ, EMB-FR-NHQG: a `git switch` in a terminal. The
// root check finds the move before the BM25 pass of the new files. The branch
// update waits for a pass of the artifact indexes that started after the
// finding: a pass that started before it, a drafts pass, and a documents pass
// do not let it run, no incremental pass runs, and search stays ready. The
// switch then embeds nothing, because the new checkout holds only chunks that
// have vectors.
#[test]
fn a_branch_switch_waits_for_a_pass_of_the_new_checkout() {
    let h = Harness::new();
    h.ready();
    h.create_branch("feature");
    h.checkout("feature");
    commit(
        &h.repo,
        &[
            ("specifications/b.md", "# Search\n\nFeature text on the branch."),
            ("specifications/c.md", "# Third\n\nOnly on the branch."),
        ],
    );
    h.git_change();
    h.settle(Instant::now() + LATER);
    h.assert_view_matches_snapshot();
    assert!(h.view_ids().contains(&"specifications/c.md#0".to_string()), "the new file");
    let embedded = h.embedded();
    let jobs = h.jobs().len();
    let asked = h.passes_asked();

    h.checkout("main");
    let started_before = h.app.state::<Bm25Indexer>().passes_started();
    h.service().request_root_check();
    h.settle(Instant::now());
    assert!(h.awaiting());
    assert_eq!(h.passes_asked(), asked + 1, "the service asks for a pass");
    assert_eq!(h.jobs().len(), jobs, "no evaluation of the old snapshot");
    assert!(h.ready_for_spec(), "the wait is not a job");

    let generation = h.app.state::<Bm25Indexer>().generation();
    h.service().index_published(
        Published { touched: true, artifacts_pass: Some(started_before) },
        generation,
        Instant::now(),
    );
    assert!(h.awaiting(), "a pass that started before the finding does not count");
    h.pass_scope(PassScope::DRAFTS);
    h.pass_scope(PassScope::DOCUMENTS);
    h.settle(Instant::now() + LATER);
    assert!(h.awaiting(), "a pass of another part does not count");
    assert!(h.incremental_due(), "the documents pass left an incremental pass due");
    assert_eq!(h.jobs().len(), jobs, "no incremental pass runs while it waits");
    assert_eq!(h.passes_asked(), asked + 1, "and no pass found the move again");

    h.requested_pass();
    assert!(!h.awaiting());
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs()[jobs..], [JobKind::BranchUpdate]);
    assert_eq!(h.embedded(), embedded, "the switch embeds nothing");
    assert_eq!(h.service().status().project.unwrap().branch.as_deref(), Some("main"));
    h.assert_view_matches_snapshot();
    assert!(
        !h.view_ids().contains(&"specifications/c.md#0".to_string()),
        "the requested pass reads the file list of the new checkout"
    );
    h.assert_layers_match_facts();
}

// EMB-FR-USBA: a switch back to a branch that kept its layer
// embeds nothing again, and its view equals the new checkout.
#[test]
fn a_switch_back_to_a_branch_embeds_nothing_again() {
    let h = Harness::new();
    h.ready();
    h.create_branch("feature");
    h.checkout("feature");
    commit(&h.repo, &[("specifications/b.md", "# Search\n\nFeature text on the branch.")]);
    h.git_change();
    h.settle(Instant::now() + LATER);
    h.checkout("main");
    h.git_change();
    h.settle(Instant::now() + LATER);
    let embedded = h.embedded();
    let jobs = h.jobs().len();

    h.checkout("feature");
    h.git_change();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs()[jobs..], [JobKind::BranchUpdate]);
    assert_eq!(h.embedded(), embedded, "the kept layer has every vector");
    h.assert_view_matches_snapshot();
    let mut layers = h.stored_branch_layers();
    layers.sort();
    assert_eq!(layers, ["feature".to_string(), "main".to_string()]);
}

// EMB-FR-KPWZ, EMB-FR-VQSK: a second Git move while the service waits. The
// root tip moved after the branch, so the wait becomes a root update, and only
// a pass that started after the second finding lets it run.
#[test]
fn a_second_move_while_waiting_needs_a_pass_after_it() {
    let h = Harness::new();
    h.ready();
    let jobs = h.jobs().len();
    h.create_branch("feature");
    h.repo.set_head("refs/heads/feature").unwrap();
    h.service().request_root_check();
    h.settle(Instant::now());
    assert!(h.awaiting());

    // A pass starts between the two findings.
    h.pass_scope(PassScope::DRAFTS);
    let between = h.app.state::<Bm25Indexer>().passes_started();
    commit(&h.repo, &[("specifications/c.md", "# Third\n\nOn main and on the branch.")]);
    let tip = h.repo.head().unwrap().peel_to_commit().unwrap();
    h.repo.branch("main", &tip, true).unwrap();
    h.service().request_root_check();
    h.settle(Instant::now());
    let kind = h.service().lock().project.as_ref().unwrap().pending.awaiting.unwrap().kind;
    assert_eq!(kind, JobKind::RootUpdate);

    let generation = h.app.state::<Bm25Indexer>().generation();
    h.service().index_published(
        Published { touched: true, artifacts_pass: Some(between) },
        generation,
        Instant::now(),
    );
    assert!(h.awaiting(), "a pass that started before the second finding does not count");

    h.requested_pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs()[jobs..], [JobKind::RootUpdate]);
    assert_eq!(h.service().status().project.unwrap().branch.as_deref(), Some("feature"));
    h.assert_layers_match_facts();
    h.assert_view_matches_snapshot();
}

// EMB-FR-KPWZ, EMB-FR-EAEA: a pass that publishes nothing, here one of an old
// generation, does not end the wait. The next root check finds the move again
// and asks for a new pass, which lets the evaluation run.
#[test]
fn a_lost_publish_is_recovered_by_the_next_root_check() {
    let h = Harness::new();
    h.ready();
    let jobs = h.jobs().len();
    h.create_branch("feature");
    h.repo.set_head("refs/heads/feature").unwrap();
    h.service().request_root_check();
    h.settle(Instant::now());
    let asked = h.passes_asked();

    let generation = h.app.state::<Bm25Indexer>().generation();
    h.service().index_published(
        Published { touched: true, artifacts_pass: Some(u64::MAX) },
        generation + 1,
        Instant::now(),
    );
    h.settle(Instant::now() + LATER);
    assert!(h.awaiting(), "a publish of another generation does not count");
    assert_eq!(h.jobs().len(), jobs);

    h.git_change();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.passes_asked(), asked + 1);
    assert_eq!(h.jobs()[jobs..], [JobKind::BranchUpdate]);
}

// EMB-FR-KPWZ, EMB-FR-VQSK: a root update that waits takes the place of a
// branch update that is already queued, so the job has the right name.
#[test]
fn a_waiting_root_update_takes_the_place_of_a_queued_branch_update() {
    let h = Harness::new();
    h.ready();
    let after = h.app.state::<Bm25Indexer>().passes_started();
    {
        let service = h.service();
        let mut st = service.lock();
        let pending = &mut st.project.as_mut().unwrap().pending;
        pending.full = Some(JobKind::BranchUpdate);
        pending.awaiting =
            Some(super::super::service::Awaiting { kind: JobKind::RootUpdate, after });
    }
    h.requested_pass();
    let full = h.service().lock().project.as_ref().unwrap().pending.full;
    assert_eq!(full, Some(JobKind::RootUpdate));
}

// EMB-FR-USBA, EMB-FR-MBJV, EMB-FR-DQVX: a branch keeps its layer while the
// root tip moves. A switch back to it rebuilds its difference against the new
// root tip: a chunk that now equals the root chunk leaves the layer, a root
// chunk that the branch does not have is marked deleted, and the layer
// records the new tip.
#[test]
fn a_kept_branch_layer_is_compared_again_after_the_root_tip_moved() {
    let h = Harness::new();
    h.ready();
    h.create_branch("feature");
    h.checkout("feature");
    let feature_b = "# Search\n\nFeature text on the branch.";
    commit(&h.repo, &[("specifications/b.md", feature_b)]);
    h.git_change();
    h.settle(Instant::now() + LATER);
    assert!(h.branch_entries("feature").contains_key("specifications/b.md#0"));

    h.checkout("main");
    h.git_change();
    h.settle(Instant::now() + LATER);
    commit(
        &h.repo,
        &[("specifications/b.md", feature_b), ("specifications/c.md", "# Third\n\nOnly on main.")],
    );
    h.git_change();
    h.settle(Instant::now() + LATER);
    let embedded = h.embedded();

    h.checkout("feature");
    h.git_change();
    h.settle(Instant::now() + LATER);
    let entries = h.branch_entries("feature");
    assert!(!entries.contains_key("specifications/b.md#0"), "b.md equals the root now");
    assert_eq!(entries.get("specifications/c.md#0"), Some(&None), "c.md is marked deleted");
    assert_eq!(h.embedded(), embedded, "the switch back embeds nothing");
    h.assert_layers_match_facts();
    h.assert_view_matches_snapshot();
}

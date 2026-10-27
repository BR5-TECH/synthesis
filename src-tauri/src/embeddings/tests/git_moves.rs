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
    h.service().request_root_check();
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
    h.service().request_root_check();
    h.settle(Instant::now());
    assert_eq!(h.jobs()[jobs..], [JobKind::BranchUpdate]);
    let project = h.service().status().project.unwrap();
    assert!(project.detached);
    assert_eq!(project.branch, None);
    assert!(h.ready_for_spec());

    h.repo.set_head("refs/heads/main").unwrap();
    h.service().request_root_check();
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
    h.service().request_root_check();
    h.settle(Instant::now());
    assert_eq!(h.jobs()[jobs..], [JobKind::RootUpdate]);
    assert_eq!(h.service().status().project.unwrap().branch.as_deref(), Some("feature"));
    h.assert_layers_match_facts();
}

// EMB-FR-OAJQ, EMB-FR-DQVX, EMB-FR-MBJV: a pull moves the root tip and the
// working copy together. The root check and the pass that the watcher starts
// give one root update, which embeds only the new chunks and leaves the branch
// layer empty.
#[test]
fn a_pull_updates_the_root_layer_in_one_pass() {
    let h = Harness::new();
    h.ready();
    let jobs = h.jobs().len();
    let embedded = h.embedded();
    let new_c = "# Third\n\nA pulled specification.";
    commit(&h.repo, &[("specifications/c.md", new_c)]);
    h.service().request_root_check();
    h.pass();
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
    h.service().request_root_check();
    h.settle(Instant::now());
    let project = h.service().status().project.unwrap();
    assert!(project.last_error.is_some());
    assert_eq!(project.root_tip, old_tip);

    h.log.fail_embed.store(false, Ordering::SeqCst);
    let jobs = h.jobs().len();
    h.service().request_root_check();
    h.settle(Instant::now());
    assert_eq!(h.jobs()[jobs..], [JobKind::RootUpdate]);
    let project = h.service().status().project.unwrap();
    assert_ne!(project.root_tip, old_tip);
    assert_eq!(project.last_error, None);
    h.assert_layers_match_facts();
}

// EMB-FR-TGCL, EMB-FR-USBA: a branch that a linked worktree has checked out
// keeps its layer while main is active; a model switch removes it.
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
    let linked = tempfile::tempdir().unwrap();
    let reference = h.repo.find_reference("refs/heads/feature").unwrap();
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    h.repo.worktree("feature", &linked.path().join("wt"), Some(&options)).unwrap();
    h.mount(ProjectEmbeddingsMode::UseGlobal);
    h.pass();
    h.settle(Instant::now());
    assert!(
        h.stored_branch_layers().contains(&"feature".to_string()),
        "the linked worktree keeps the layer of its branch"
    );

    h.service()
        .apply_settings(EmbeddingsRecord { enabled: true, model: Some("AllMiniLML6V2".into()) });
    h.settle(Instant::now());
    assert_eq!(h.jobs().last(), Some(&JobKind::ModelSwitch));
    assert_eq!(h.stored_branch_layers(), ["main".to_string()]);
}

// EMB-FR-DQVX, EMB-FR-OAJQ: a pass whose Git facts moved after the last job —
// a pull that reaches the service before any root check — becomes one root
// update, so the facts it records match the layers it wrote. A root check
// after it then finds no move.
#[test]
fn an_incremental_pass_after_a_tip_move_becomes_a_root_update() {
    let h = Harness::new();
    h.ready();
    let jobs = h.jobs().len();
    commit(&h.repo, &[("specifications/c.md", "# Third\n\nA pulled specification.")]);
    h.pass();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs()[jobs..], [JobKind::RootUpdate]);
    assert_eq!(h.service().status().project.unwrap().spec_chunks, 4);
    h.assert_layers_match_facts();
    h.service().request_root_check();
    h.settle(Instant::now() + LATER);
    assert_eq!(h.jobs()[jobs..], [JobKind::RootUpdate], "the facts did not move again");
}

// EMB-FR-DQVX, EMB-FR-NHQG: a pass after a branch switch, with no root check
// before it, becomes a branch update for the new branch.
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
    assert_eq!(h.jobs()[jobs..], [JobKind::BranchUpdate]);
    assert_eq!(h.service().status().project.unwrap().branch.as_deref(), Some("feature"));
    h.assert_layers_match_facts();
}

// EMB-FR-USBA: back on main, a full evaluation removes the layer of a branch
// that no worktree has checked out, while the branch itself stays.
#[test]
fn a_full_evaluation_removes_the_layer_of_a_branch_with_no_worktree() {
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
    assert!(h.repo.find_branch("feature", git2::BranchType::Local).is_ok());
    assert_eq!(h.stored_branch_layers(), ["main".to_string()]);
}

//! The checkout a push of a stream's branch runs from (`GTC-git.md`
//! GTC-FR-RZNQ).

use super::*;

// GTC-FR-RZNQ
#[test]
fn a_stream_branch_is_pushed_from_the_stream_working_copy() {
    let fx = Fixture::new();
    let stream = fx.create("editor-work", None).expect("created");

    let checkout = crate::git::push_checkout(&fx.app, &fx.root(), &stream.branch);

    assert_eq!(canonical(&checkout), canonical(Path::new(&stream.worktree_path)));
}

// GTC-FR-RZNQ
#[test]
fn a_stream_whose_working_copy_is_gone_is_pushed_from_the_primary_worktree() {
    let fx = Fixture::new();
    let stream = fx.create("editor-work", None).expect("created");
    std::fs::remove_dir_all(&stream.worktree_path).unwrap();

    let checkout = crate::git::push_checkout(&fx.app, &fx.root(), &stream.branch);

    assert_eq!(canonical(&checkout), fx.root());
}

//! A project root inside a larger repository.
//!
//! One part of `../tests/mod.rs`, which holds the fixture these run against.

use super::*;

// -----------------------------------------------------------------------
// Co-located mode: project root inside a larger repository
// -----------------------------------------------------------------------

#[test]
fn a_project_below_the_repo_root_reports_project_relative_paths() {
    // CHC-FR-02 / CHC-FR-11: in co-located mode the repository is the host
    // code repository, but ids must still match the project tree's.
    let f = Fixture::new();
    f.write("proj/a.md", "a\n");
    f.write("outside.md", "o\n");
    f.commit("A");
    f.write("proj/a.md", "a\nb\n");
    f.write("outside.md", "o\np\n");

    let project_root = f.root().join("proj");
    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(project_root)).unwrap();
    let got = paths(&set);
    assert_eq!(
        got,
        vec!["a.md".to_string()],
        "paths are relative to the project root, and the host repo's other \
         changes stay out of the project's tree"
    );
}

#[test]
fn a_project_below_the_repo_root_compares_branches_within_the_project() {
    let f = Fixture::new();
    f.write("proj/a.md", "base\n");
    f.write("outside.md", "o\n");
    f.commit("A");
    let main = f.current_branch();
    f.branch("feature");
    f.checkout("feature");
    f.write("proj/a.md", "base\nfeature\n");
    f.write("outside.md", "o\nalso feature\n");
    f.commit("E");

    let set = branch_change_set(&crate::fs::RootFs::for_root(f.root().join("proj")), &main).unwrap();
    assert_eq!(
        paths(&set),
        vec!["a.md".to_string()],
        "the branch comparison is scoped to the project, with project-relative paths"
    );
}

#[test]
fn a_sibling_directory_sharing_the_prefix_does_not_leak_into_the_change_set() {
    // `proj` must not match `project-notes`. The pathspec is the first line
    // of defence and `to_project_rel` the second; this exercises both
    // together against a real repository.
    let f = Fixture::new();
    f.write("proj/a.md", "a\n");
    f.write("project-notes/b.md", "b\n");
    f.commit("A");
    f.write("proj/a.md", "a\nedited\n");
    f.write("project-notes/b.md", "b\nedited\n");

    let set = uncommitted_change_set(&crate::fs::RootFs::for_root(f.root().join("proj"))).unwrap();
    assert_eq!(paths(&set), vec!["a.md".to_string()]);
}

#[test]
fn project_prefix_is_empty_when_the_project_is_the_repository_root() {
    let f = Fixture::new();
    f.write("a.md", "a\n");
    f.commit("A");
    assert_eq!(project_prefix(&f.repo(), &f.root()), "");
    // An existing nested project root — the co-located shape.
    std::fs::create_dir_all(f.root().join("apps/web")).unwrap();
    assert_eq!(project_prefix(&f.repo(), &f.root().join("apps/web")), "apps/web");
    // And one that does not resolve: the prefix must still be derived, not
    // silently collapse to "" and widen the change set to the whole repo.
    assert_eq!(project_prefix(&f.repo(), &f.root().join("gone")), "gone");
    // A root genuinely outside the repository yields no prefix.
    let elsewhere = TempDir::new().unwrap();
    assert_eq!(project_prefix(&f.repo(), elsewhere.path()), "");
}

#[test]
fn canonicalize_lenient_resolves_the_existing_ancestor_of_a_missing_path() {
    let dir = TempDir::new().unwrap();
    let real = canonicalize_lenient(dir.path());
    assert_eq!(
        canonicalize_lenient(&dir.path().join("a/b/c")),
        real.join("a/b/c"),
        "the missing remainder is re-appended to the resolved ancestor"
    );
    assert_eq!(canonicalize_lenient(dir.path()), real);
}

#[test]
fn to_project_rel_strips_the_prefix_and_rejects_outsiders() {
    assert_eq!(to_project_rel("", "a/b.md"), Some("a/b.md".to_string()));
    assert_eq!(to_project_rel("proj", "proj/a.md"), Some("a.md".to_string()));
    assert_eq!(to_project_rel("proj", "other/a.md"), None);
    // A sibling directory sharing the prefix as a substring is not inside it.
    assert_eq!(to_project_rel("proj", "project/a.md"), None);
}

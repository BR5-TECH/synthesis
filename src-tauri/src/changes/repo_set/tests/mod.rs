//! The repository set (`GSM-git-submodules.md`) on real submodules.

pub mod fixture;

use super::*;
use fixture::{commit_all, write, Superproject};

// GSM-FR-VPAS, GSM-FR-SFEI: the main repository and an initialised submodule
// are members, named by their project-relative roots, main first.
#[test]
fn the_set_holds_the_main_repository_and_its_submodules() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let set = RepoSet::open(&p.root()).unwrap();
    let paths: Vec<&str> = set.members.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(paths, ["", "libs/sub"]);
    assert_eq!(set.members[1].name, "sub");
    assert_eq!(set.members[1].parent.as_deref(), Some(""));
    assert_eq!(set.members[1].path_in_parent.as_deref(), Some("libs/sub"));
    assert!(set.unopened.is_empty());
}

// GSM-FR-VPAS: a submodule of a submodule is a member, with its own parent.
#[test]
fn a_nested_submodule_is_a_member() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    p.add_submodule("libs/sub", "inner", &[("i.md", "i\n")]);
    let set = RepoSet::open(&p.root()).unwrap();
    let inner = set.index_of("libs/sub/inner").expect("the nested submodule is a member");
    assert_eq!(set.members[inner].parent.as_deref(), Some("libs/sub"));
    assert_eq!(set.members[inner].depth, 2);
    assert_eq!(set.members[inner].path_in_parent.as_deref(), Some("inner"));
    // GSM-FR-KWWZ: the deepest first and the main repository last.
    let order: Vec<&str> =
        set.deepest_first().iter().map(|i| set.members[*i].path.as_str()).collect();
    assert_eq!(order, ["libs/sub/inner", "libs/sub", ""]);
}

// GSM-FR-FRLZ: a submodule with no checkout is not a member and is not
// reported as unopened.
#[test]
fn an_uninitialised_submodule_is_not_a_member() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let clone_dir = p.tmp.path().join("clone");
    git2::Repository::clone(&p.root().to_string_lossy(), &clone_dir).unwrap();
    let set = RepoSet::open(&clone_dir).unwrap();
    assert_eq!(set.members.len(), 1);
    assert!(set.unopened.is_empty());
}

// GSM-FR-SIWR, GSM-FR-MUTH: a path goes to the nearest member root above it,
// and the root path of a submodule goes to its parent.
#[test]
fn a_path_goes_to_the_nearest_member() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    p.add_submodule("libs/sub", "inner", &[("i.md", "i\n")]);
    let set = RepoSet::open(&p.root()).unwrap();
    assert_eq!(set.owner("top.md").path, "");
    assert_eq!(set.owner("libs/other.md").path, "");
    assert_eq!(set.owner("libs/sub/a.md").path, "libs/sub");
    assert_eq!(set.owner("libs/sub/inner/i.md").path, "libs/sub/inner");
    assert_eq!(set.owner("libs/sub").path, "", "a submodule root is its parent's pointer");
    assert_eq!(set.owner("libs/sub/inner").path, "libs/sub");
    assert_eq!(set.owner("libs/subway.md").path, "", "a sibling with a common prefix");
    let sub = set.owner("libs/sub/a.md");
    assert_eq!(sub.to_repo("libs/sub/a.md"), "a.md");
    assert_eq!(sub.to_project("a.md").as_deref(), Some("libs/sub/a.md"));
}

// GSM-FR-MUTH, GSM-FR-VPAS: in a project that is a folder of the main
// repository, paths are project-relative and a submodule outside the project
// is not a member.
#[test]
fn a_project_below_the_repository_root_maps_its_paths() {
    let p = Superproject::new(&[("app/readme.md", "r\n"), ("other.md", "o\n")]);
    p.add_submodule("", "app/libs/sub", &[("a.md", "a\n")]);
    p.add_submodule("", "vendor/out", &[("o.md", "o\n")]);
    let project = p.root().join("app");
    let set = RepoSet::open(&project).unwrap();
    let paths: Vec<&str> = set.members.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(paths, ["", "libs/sub"]);
    let main = set.main();
    assert_eq!(main.prefix, "app");
    assert_eq!(main.to_repo("readme.md"), "app/readme.md");
    assert_eq!(set.members[1].path_in_parent.as_deref(), Some("app/libs/sub"));
    assert_eq!(set.owner("libs/sub/a.md").to_repo("libs/sub/a.md"), "a.md");
}

// GSM-FR-FCOF, CHC-FR-AIZV: the parent base records the pointer, and in
// Branch mode the submodule base is the commit recorded at the main merge-base.
#[test]
fn the_base_of_a_submodule_is_the_pointer_its_parent_records() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    let sub = p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    let recorded = sub.head().unwrap().peel_to_commit().unwrap().id();
    let main = p.main();
    let head = main.head().unwrap().peel_to_commit().unwrap();
    main.branch("base", &head, false).unwrap();
    write(&sub, "a.md", "a\nmore\n");
    let moved = commit_all(&sub, "sub work");

    let set = RepoSet::open(&p.root()).unwrap();
    let bases = set.bases(Base::Head).unwrap();
    assert_eq!(set.recorded_pointer(&bases, 1), Some(recorded));
    assert_eq!(set.members[1].head_id(), Some(moved));

    let bases = set.bases(Base::MergeBase("base")).unwrap();
    let sub_base = bases.of[1].tree().expect("the recorded commit is the base");
    let recorded_tree = sub.find_commit(recorded).unwrap().tree().unwrap().id();
    assert_eq!(sub_base.id(), recorded_tree);
    assert!(bases.merge_base.is_some());
}

// GSM-FR-FRLZ: a checkout whose `.git` file names no Git directory is not a
// member, and it is reported with its cause.
#[test]
fn a_submodule_that_does_not_open_is_reported() {
    let p = Superproject::new(&[("top.md", "top\n")]);
    p.add_submodule("", "libs/sub", &[("a.md", "a\n")]);
    std::fs::write(p.root().join("libs/sub/.git"), "gitdir: ../../.git/modules/missing\n").unwrap();
    let set = RepoSet::open(&p.root()).unwrap();
    assert_eq!(set.members.len(), 1);
    assert_eq!(set.unopened.len(), 1, "{:?}", set.unopened);
    assert_eq!(set.unopened[0].0, "libs/sub");
    assert!(!set.unopened[0].1.is_empty());
}

//! The throwaway checkouts of a run, against a real repository and a real store
//! (`../ai/GRL-graduation-loop.md` GRL-FR-BIQX, GRL-FR-PQLM, GRL-FR-LYXG;
//! `GRD-graduation.md` GRD-FR-XKGC).

use super::*;
use tempfile::TempDir;

const RUN: &str = "g12";

/// A repository with one commit, and a store with the home of the run `RUN`.
struct Bed {
    _project: TempDir,
    _store: TempDir,
    main: git2::Repository,
    fs: fsa::FsAccess,
    base: StoreBase,
    head: git2::Oid,
}

impl Bed {
    fn new() -> Self {
        let project = TempDir::new().unwrap();
        let store = TempDir::new().unwrap();
        let root = std::fs::canonicalize(project.path()).unwrap();
        let store_root = std::fs::canonicalize(store.path()).unwrap();
        let main = git2::Repository::init(&root).unwrap();
        std::fs::write(root.join("README.md"), "seed\n").unwrap();
        let head = {
            let mut index = main.index().unwrap();
            index.add_path(Path::new("README.md")).unwrap();
            index.write().unwrap();
            let tree = main.find_tree(index.write_tree().unwrap()).unwrap();
            let sig = git2::Signature::now("Test", "test@example.com").unwrap();
            main.commit(Some("HEAD"), &sig, &sig, "seed", &tree, &[]).unwrap()
        };
        let base = StoreBase::new(store_root.clone());
        std::fs::create_dir_all(base.home(RUN)).unwrap();
        std::fs::write(base.home(RUN).join("run.toml"), "id = \"g12\"\n").unwrap();
        let fs = fsa::FsAccess::builder().owned_root(&store_root).build().unwrap();
        Self { _project: project, _store: store, main, fs, base, head }
    }

    fn home(&self) -> PathBuf {
        self.base.home(RUN)
    }

    fn review(&self) -> Throwaway<'static> {
        Throwaway::review(self.base.clone(), RUN)
    }

    fn registrations(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .main
            .worktrees()
            .unwrap()
            .iter()
            .filter_map(|n| n.ok().flatten().map(str::to_string))
            .collect();
        names.sort();
        names
    }

    fn has_branch(&self, name: &str) -> bool {
        self.main.find_branch(name, git2::BranchType::Local).is_ok()
    }

    /// A registration named `name` whose recorded path is `target`, made at a
    /// scratch path and then pointed at `target` the way Git records it.
    fn register_at(&self, name: &str, target: &Path, scratch: &Path) {
        self.main.worktree(name, &scratch.join(name), None).unwrap();
        let record = self.main.path().join("worktrees").join(name).join("gitdir");
        std::fs::write(record, format!("{}\n", target.join(".git").display())).unwrap();
        assert_eq!(self.main.find_worktree(name).unwrap().path(), target);
    }

    /// A checkout made the way an earlier version made it: on the scratch
    /// branch, with its HEAD still on that branch.
    fn attached_checkout(&self, name: &str, path: &Path) {
        let commit = self.main.find_commit(self.head).unwrap();
        self.main.branch(&format!("synthesis/review/{RUN}"), &commit, true).unwrap();
        let reference = self
            .main
            .find_branch(&format!("synthesis/review/{RUN}"), git2::BranchType::Local)
            .unwrap()
            .into_reference();
        let mut opts = git2::WorktreeAddOptions::new();
        opts.reference(Some(&reference));
        self.main.worktree(name, path, Some(&opts)).unwrap();
    }
}

/// A directory that refuses the removal of its entries until it is dropped.
#[cfg(unix)]
struct ReadOnly(PathBuf);

#[cfg(unix)]
impl ReadOnly {
    fn new(path: &Path) -> Self {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o555)).unwrap();
        ReadOnly(path.to_path_buf())
    }
}

#[cfg(unix)]
impl Drop for ReadOnly {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

// GRL-FR-BIQX, GRL-FR-PQLM: each checkout stands at a new nonced path, detached
// at the commit, and the reclaim takes it with its registration and branch.
#[test]
fn each_checkout_gets_a_new_path_and_the_reclaim_takes_it() {
    let bed = Bed::new();
    let review = bed.review();

    let (checkout, first) = review.make(&bed.fs, &bed.main, bed.head).unwrap();
    assert!(checkout.head_detached().unwrap(), "detached");
    assert_eq!(checkout.head().unwrap().target(), Some(bed.head));
    drop(checkout);
    assert_eq!(first.parent(), Some(bed.home().as_path()));
    let name = first.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("rv-") && is_checkout_name("rv", &name), "{name}");
    assert_eq!(bed.registrations(), vec![format!("{RUN}-{name}")]);
    assert!(review.stands(&bed.fs, &bed.main));

    review.reclaim(&bed.fs, &bed.main).unwrap();
    assert!(!first.exists());
    assert!(bed.registrations().is_empty());
    assert!(!bed.has_branch(&format!("synthesis/review/{RUN}")));
    assert!(!review.stands(&bed.fs, &bed.main));
    assert!(bed.home().join("run.toml").is_file(), "the run record stays");

    let (_, second) = review.make(&bed.fs, &bed.main, bed.head).unwrap();
    assert_ne!(first, second, "a later checkout never reuses a path");
}

// GRD-FR-XKGC: a PR summary checkout is `ps-<nonce>` on its own scratch branch,
// and a review reclaim leaves it alone.
#[test]
fn a_pr_summary_checkout_has_its_own_names() {
    let bed = Bed::new();
    let summary = Throwaway::pr_summary(bed.base.clone(), RUN);
    let (_, path) = summary.make(&bed.fs, &bed.main, bed.head).unwrap();
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("ps-") && is_checkout_name("ps", &name), "{name}");
    assert_eq!(bed.registrations(), vec![format!("{RUN}-{name}")]);
    assert!(bed.has_branch(&format!("synthesis/pr-summary/{RUN}")));

    bed.review().reclaim(&bed.fs, &bed.main).unwrap();
    assert!(path.is_dir(), "a review reclaim leaves the PR summary checkout");

    summary.reclaim(&bed.fs, &bed.main).unwrap();
    assert!(!path.exists());
    assert!(bed.registrations().is_empty());
    assert!(!bed.has_branch(&format!("synthesis/pr-summary/{RUN}")));
}

// GRL-FR-PQLM: the reclaim takes the name without a nonce that earlier versions
// used, nonced checkouts, and a checkout directory with no registration.
#[test]
fn the_reclaim_takes_the_legacy_name_and_every_nonced_checkout() {
    let bed = Bed::new();
    // Detached once made, as the earlier versions left it.
    bed.attached_checkout(&format!("{RUN}-rv"), &bed.home().join("rv"));
    git2::Repository::open(bed.home().join("rv")).unwrap().set_head_detached(bed.head).unwrap();
    std::fs::create_dir_all(bed.home().join("rv-0123abcd")).unwrap();
    std::fs::write(bed.home().join("rv-0123abcd/f.txt"), "x\n").unwrap();
    let (_, nonced) = bed.review().make(&bed.fs, &bed.main, bed.head).unwrap();
    assert_eq!(bed.registrations().len(), 2);
    assert!(bed.review().stands(&bed.fs, &bed.main));

    bed.review().reclaim(&bed.fs, &bed.main).unwrap();

    for gone in [bed.home().join("rv"), bed.home().join("rv-0123abcd"), nonced] {
        assert!(!gone.exists(), "{} is gone", gone.display());
    }
    assert!(bed.registrations().is_empty());
    assert!(!bed.has_branch(&format!("synthesis/review/{RUN}")));
}

/// The recorded path of one row, and a file under it that must stay, from the
/// bed and a directory outside the store.
type TargetOf = Box<dyn Fn(&Bed, &Path) -> (PathBuf, PathBuf)>;

// GRL-FR-PQLM: a registration whose recorded path is not a checkout directly
// under the run home is pruned, and what stands at that path stays.
#[test]
fn a_registration_that_records_another_path_is_pruned_and_the_path_stays() {
    let rows: Vec<(&str, TargetOf)> = vec![
        (
            "outside the store",
            Box::new(|_, outside| (outside.to_path_buf(), outside.join("sentinel"))),
        ),
        ("the run home", Box::new(|bed, _| (bed.home(), bed.home().join("run.toml")))),
        ("the run's logs", Box::new(|bed, _| (bed.home().join("logs"), bed.home().join("logs/a")))),
        ("the merge worktree", Box::new(|bed, _| (bed.home().join("mw"), bed.home().join("mw/a")))),
        (
            "a checkout name not directly under the home",
            Box::new(|bed, _| {
                let path = bed.home().join("nested/rv-0123abcd");
                (path.clone(), path.join("a"))
            }),
        ),
        (
            "a checkout of another run",
            Box::new(|bed, _| {
                let path = bed.base.home("g13").join("rv-0123abcd");
                (path.clone(), path.join("a"))
            }),
        ),
    ];
    for (row, target_of) in rows {
        let bed = Bed::new();
        let outside = TempDir::new().unwrap();
        let scratch = TempDir::new().unwrap();
        let (target, sentinel) = target_of(&bed, &std::fs::canonicalize(outside.path()).unwrap());
        std::fs::create_dir_all(sentinel.parent().unwrap()).unwrap();
        if !sentinel.exists() {
            std::fs::write(&sentinel, "keep\n").unwrap();
        }
        bed.register_at(&format!("{RUN}-rv-0123abcd"), &target, scratch.path());

        bed.review().reclaim(&bed.fs, &bed.main).unwrap_or_else(|e| panic!("{row}: {e}"));

        assert!(bed.registrations().is_empty(), "{row}: the registration is pruned");
        assert!(sentinel.is_file(), "{row}: what stands at the path stays");
        assert!(target.is_dir(), "{row}: the directory stays");
    }
}

// GRL-FR-PQLM: a link named like a checkout is not followed. Its registration
// is pruned, and the directory it points at keeps what it holds.
#[cfg(unix)]
#[test]
fn a_link_named_like_a_checkout_does_not_lead_out_of_the_run_home() {
    let bed = Bed::new();
    let outside = TempDir::new().unwrap();
    let scratch = TempDir::new().unwrap();
    let target = std::fs::canonicalize(outside.path()).unwrap();
    std::fs::write(target.join("sentinel"), "keep\n").unwrap();
    let link = bed.home().join("rv-0123abcd");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    bed.register_at(&format!("{RUN}-rv-0123abcd"), &link, scratch.path());

    bed.review().reclaim(&bed.fs, &bed.main).unwrap();

    assert!(bed.registrations().is_empty());
    assert!(target.join("sentinel").is_file(), "the link target keeps its contents");
}

// GRL-FR-PQLM: names that are not checkout names of this run and kind are left
// alone, directories and registrations both, and nothing of them "stands".
#[test]
fn names_that_are_not_checkouts_of_the_run_are_left_alone() {
    let bed = Bed::new();
    let decoys = [
        "rvx",
        "rv-XYZ",
        "rv-0123ABCD",
        "rv-0123abc",
        "rv-0123abcde",
        "rv_0123abcd",
        "ps-0123abcd",
        "mw",
        "logs",
    ];
    for dir in decoys {
        std::fs::create_dir_all(bed.home().join(dir)).unwrap();
        std::fs::write(bed.home().join(dir).join("a"), "keep\n").unwrap();
    }
    std::fs::write(bed.home().join("rv-fedcba98"), "a file\n").unwrap();
    let scratch = TempDir::new().unwrap();
    let names =
        ["g12-rvx", "g12-rv-XYZ", "g12-ps-0123abcd", "g12-mw", "g123-rv", "g13-rv-0123abcd"];
    for name in names {
        bed.main.worktree(name, &scratch.path().join(name), None).unwrap();
    }
    assert!(!bed.review().stands(&bed.fs, &bed.main));

    bed.review().reclaim(&bed.fs, &bed.main).unwrap();

    for dir in decoys {
        assert!(bed.home().join(dir).join("a").is_file(), "{dir} stays");
    }
    assert!(bed.home().join("rv-fedcba98").is_file(), "a file of a checkout name stays");
    let mut expected: Vec<String> = names.iter().map(|n| n.to_string()).collect();
    expected.sort();
    assert_eq!(bed.registrations(), expected);
}

// GRL-FR-PQLM: the registration matcher takes the run id whole, so a run whose
// id starts with another run's id is not taken for it.
#[test]
fn the_registration_matcher_takes_the_run_id_whole() {
    let bed = Bed::new();
    let review = bed.review();
    for (name, expected) in [
        ("g12-rv", true),
        ("g12-rv-0123abcd", true),
        ("g123-rv", false),
        ("g12rv", false),
        ("g12-rvx", false),
        ("g12-ps-0123abcd", false),
        ("g1-rv", false),
    ] {
        assert_eq!(review.is_registration(name), expected, "{name}");
    }
}

// GRL-FR-LYXG, GRL-FR-PQLM: a checkout whose directory cannot go is reported,
// loses its registration all the same, and does not stop the next checkout,
// also where its HEAD still held the scratch branch. Every other step is tried.
#[cfg(unix)]
#[test]
fn a_checkout_that_cannot_go_does_not_stop_the_next_one() {
    if crate::fs::permission_probe::skip_without_enforcement(
        "a_checkout_that_cannot_go_does_not_stop_the_next_one",
        crate::fs::permission_probe::Injection::Write,
    ) {
        return;
    }
    let bed = Bed::new();
    let stuck = bed.home().join("rv-0123abcd");
    bed.attached_checkout(&format!("{RUN}-rv-0123abcd"), &stuck);
    std::fs::create_dir_all(stuck.join("locked")).unwrap();
    std::fs::write(stuck.join("locked/f.txt"), "x\n").unwrap();
    std::fs::create_dir_all(bed.home().join("rv-89abcdef")).unwrap();
    std::fs::write(bed.home().join("rv-89abcdef/f.txt"), "x\n").unwrap();
    let guard = ReadOnly::new(&stuck.join("locked"));

    let error = bed.review().reclaim(&bed.fs, &bed.main).expect_err("the stuck checkout stays");
    assert!(error.contains("rv-0123abcd"), "{error}");
    assert!(!error.contains("rv-89abcdef"), "{error}");
    assert!(stuck.join("locked/f.txt").is_file(), "the stuck directory stays");
    assert!(!bed.home().join("rv-89abcdef").exists(), "the other checkout went");
    assert!(bed.registrations().is_empty(), "the registration is pruned all the same");

    let (_, next) = bed.review().make(&bed.fs, &bed.main, bed.head).expect("the next checkout");
    assert_ne!(next, stuck);

    drop(guard);
    bed.review().reclaim(&bed.fs, &bed.main).unwrap();
    assert!(!stuck.exists(), "a later reclaim takes it");
    assert!(!next.exists());
    assert!(bed.registrations().is_empty());
}

// GRL-FR-BIQX: a nonce whose directory stands already, or whose registration
// stands already, is drawn again.
#[test]
fn a_nonce_that_is_taken_is_drawn_again() {
    let bed = Bed::new();
    let review = bed.review();
    std::fs::create_dir_all(bed.home().join("rv-00000001")).unwrap();
    let scratch = TempDir::new().unwrap();
    bed.main.worktree("g12-rv-00000002", &scratch.path().join("w"), None).unwrap();

    let mut draws = ["00000001", "00000002", "00000003"].into_iter().map(str::to_string);
    let nonce = review.free_nonce(&bed.fs, &bed.main, || draws.next().unwrap()).unwrap();

    assert_eq!(nonce, "00000003", "a directory and a registration are both taken");
}

// GRL-FR-BIQX: where every draw is taken, no checkout is made and the error
// says so.
#[test]
fn no_checkout_is_made_where_every_draw_is_taken() {
    let bed = Bed::new();
    std::fs::create_dir_all(bed.home().join("rv-00000001")).unwrap();
    let mut drawn = 0;
    let error = bed
        .review()
        .free_nonce(&bed.fs, &bed.main, || {
            drawn += 1;
            "00000001".to_string()
        })
        .expect_err("no free name");
    assert_eq!(error, "no free name for a new checkout");
    assert_eq!(drawn, NONCE_DRAWS, "every draw was tried");
    assert!(bed.registrations().is_empty());
}

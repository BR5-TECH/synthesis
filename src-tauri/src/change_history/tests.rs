//! Tests for `CVH-change-version-history.md`, against a real repository.
//!
//! Fixtures build files with `std::fs`, which the FSA guard allows in a test
//! file.

use super::*;

const CHANGE: &str = "add-auth";

struct Repo {
    dir: tempfile::TempDir,
    repo: git2::Repository,
}

impl Repo {
    fn new() -> Repo {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let repo = git2::Repository::init(dir.path()).expect("init");
        let mut config = repo.config().expect("config");
        config.set_str("user.name", "Ada").expect("name");
        config.set_str("user.email", "ada@example.com").expect("email");
        drop(config);
        Repo { dir, repo }
    }

    fn write(&self, rel: &str, text: &[u8]) {
        let path = self.dir.path().join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, text).expect("write");
    }

    fn remove(&self, rel: &str) {
        let path = self.dir.path().join(rel);
        if path.is_dir() {
            std::fs::remove_dir_all(path).expect("remove dir");
        } else {
            std::fs::remove_file(path).expect("remove");
        }
    }

    /// Commit the whole working tree, deletions included.
    fn commit(&self, message: &str) -> String {
        let mut index = self.repo.index().expect("index");
        index.add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None).expect("add");
        index.update_all(["*"].iter(), None).expect("update");
        index.write().expect("write");
        let tree = self.repo.find_tree(index.write_tree().expect("tree")).expect("tree");
        let sig = self.repo.signature().expect("sig");
        let parent = self.repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        self.repo
            .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .expect("commit")
            .to_string()
    }

    fn path(&self) -> &std::path::Path {
        self.dir.path()
    }
}

fn base(r: &Repo) -> (String, String, String, String) {
    let dir = format!("openspec/changes/{CHANGE}");
    r.write("README.md", b"readme\n");
    r.write(&format!("{dir}/proposal.md"), b"## Why\n");
    r.write(&format!("{dir}/tasks.md"), b"- [ ] 1.1\n");
    let created = r.commit("Propose change add-auth");
    r.write("README.md", b"other\n");
    let unrelated = r.commit("Edit the readme");
    r.write(&format!("{dir}/tasks.md"), b"- [x] 1.1\n");
    let edited = r.commit("Accept edits to OpenSpec change add-auth\n\nbody");
    r.remove(&format!("{dir}/proposal.md"));
    r.write(&format!("{dir}/why.md"), b"## Why\n");
    let renamed = r.commit("Rename the proposal");
    let _ = unrelated;
    (created, edited, renamed, dir)
}

// CVH-FR-HDTA / CVH-FR-VZGR / CVH-FR-URCF: a version is a commit that changes
// the change folder, newest first, with its message line, author, time, and
// paths; a rename is a delete and an add; a commit elsewhere is no version.
#[test]
fn the_versions_are_the_commits_that_change_the_folder() {
    let r = Repo::new();
    let (created, edited, renamed, _) = base(&r);
    let listed = list_versions_in(r.path(), CHANGE).expect("versions");
    let ids: Vec<&str> = listed.versions.iter().map(|v| v.commit_id.as_str()).collect();
    assert_eq!(ids, [renamed.as_str(), edited.as_str(), created.as_str()]);
    assert!(!listed.truncated);
    let [rename, edit, create] = &listed.versions[..] else { panic!("three versions") };
    assert_eq!(edit.summary, "Accept edits to OpenSpec change add-auth");
    assert_eq!(edit.author_name, "Ada");
    assert!(edit.committed_at.ends_with('Z'));
    assert_eq!(edit.modified, vec!["tasks.md".to_string()]);
    assert_eq!(create.added, vec!["proposal.md".to_string(), "tasks.md".to_string()]);
    assert_eq!(rename.added, vec!["why.md".to_string()]);
    assert_eq!(rename.deleted, vec!["proposal.md".to_string()]);
}

/// Move the change folder below `openspec/changes/archive/` as `openspec
/// archive` does, and commit it.
fn archive(r: &Repo, dir: &str, folder: &str) -> String {
    let source = r.path().join(dir);
    let target = r.path().join(format!("openspec/changes/archive/{folder}"));
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::rename(source, target).unwrap();
    r.commit(&format!("Archive OpenSpec change {CHANGE}"))
}

// CVH-FR-SJIF, CVH-FR-HXVV, CVH-FR-URCF: the archive version moves the folder
// to `openspec/changes/archive/`, and the list reads only the versions newer
// than it, so an archived change with no later version is refused.
#[test]
fn an_archived_change_lists_no_version() {
    let r = Repo::new();
    let (created, _, _, dir) = base(&r);
    let archived = archive(&r, &dir, &format!("2026-11-11-{CHANGE}"));
    assert_eq!(list_versions_in(r.path(), CHANGE), Err(ERR_CHANGE_NOT_FOUND.to_string()));
    // CVH-FR-VSQD: neither the archive version nor an older one reads.
    assert_eq!(read_version_in(r.path(), CHANGE, &archived), Err(ERR_UNKNOWN_VERSION.to_string()));
    assert_eq!(read_version_in(r.path(), CHANGE, &created), Err(ERR_UNKNOWN_VERSION.to_string()));
}

// CVH-FR-SJIF: a later change of the same name starts with no version, and
// lists only its own versions, newer than the newest archive version.
#[test]
fn a_later_change_of_the_same_name_lists_its_own_versions() {
    let r = Repo::new();
    let (_, _, _, dir) = base(&r);
    archive(&r, &dir, &format!("2026-11-10-{CHANGE}"));
    r.write(&format!("{dir}/proposal.md"), b"## Why again\n");
    let first = r.commit("Propose add-auth again");
    archive(&r, &dir, &format!("2026-11-11-{CHANGE}"));
    r.write(&format!("{dir}/proposal.md"), b"## Why a third time\n");
    let third = r.commit("Propose add-auth a third time");
    r.write(&format!("{dir}/tasks.md"), b"- [ ] 1.1\n");
    let edit = r.commit("Add the tasks");
    let listed = list_versions_in(r.path(), CHANGE).expect("versions");
    let ids: Vec<&str> = listed.versions.iter().map(|v| v.commit_id.as_str()).collect();
    assert_eq!(ids, [edit.as_str(), third.as_str()]);
    assert_eq!(read_version_in(r.path(), CHANGE, &first), Err(ERR_UNKNOWN_VERSION.to_string()));
    assert_eq!(read_version_in(r.path(), CHANGE, &third).expect("read").files.len(), 1);
}

// CVH-FR-SJIF: a commit that deletes the folder with no archive folder is no
// archive version, so the older versions stay listed (CVH-FR-HXVV).
#[test]
fn a_deleted_folder_with_no_archive_keeps_its_versions() {
    let r = Repo::new();
    let (created, _, _, dir) = base(&r);
    r.remove(&dir);
    let deleted = r.commit("Remove add-auth");
    let listed = list_versions_in(r.path(), CHANGE).expect("versions");
    assert_eq!(listed.versions[0].commit_id, deleted);
    assert_eq!(listed.versions[0].deleted, vec!["tasks.md".to_string(), "why.md".to_string()]);
    assert_eq!(listed.versions.last().unwrap().commit_id, created);
    assert_eq!(read_version_in(r.path(), CHANGE, &deleted).expect("read").files, Vec::new());
}

// CVH-FR-ITHO / CVH-FR-HCOJ / CVH-FR-VSQD: an uncommitted edit and a commit
// on another branch are not versions.
#[test]
fn only_commits_of_the_checked_out_branch_count() {
    let r = Repo::new();
    let (_, _, renamed, dir) = base(&r);
    let home = r.repo.head().unwrap().name().expect("branch name").to_string();
    let head = r.repo.head().unwrap().peel_to_commit().unwrap();
    r.repo.branch("other", &head, false).expect("branch");
    r.repo.set_head("refs/heads/other").expect("head");
    r.write(&format!("{dir}/tasks.md"), b"- [x] 1.1\n- [ ] 1.2\n");
    let elsewhere = r.commit("On another branch");
    r.repo.set_head(&home).expect("back");
    r.repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force())).expect("checkout");
    r.write(&format!("{dir}/tasks.md"), b"uncommitted\n");
    let listed = list_versions_in(r.path(), CHANGE).expect("versions");
    assert_eq!(listed.versions[0].commit_id, renamed);
    assert_eq!(listed.versions.len(), 3);
    // CVH-FR-VSQD: the commit of the other branch is not a version to read.
    assert_eq!(read_version_in(r.path(), CHANGE, &elsewhere), Err(ERR_UNKNOWN_VERSION.to_string()));
}

// CVH-FR-VDZZ: a version reads the files of the folder at that commit, by
// path, with text for UTF-8 and no content for binary bytes.
#[test]
fn a_version_reads_its_files_as_the_commit_holds_them() {
    let r = Repo::new();
    let (created, edited, _, dir) = base(&r);
    r.write(&format!("{dir}/logo.bin"), &[0xff, 0xfe, 0x00]);
    let with_binary = r.commit("Add a logo");
    let old = read_version_in(r.path(), CHANGE, &created).expect("read");
    assert_eq!(
        old.files.iter().map(|f| (f.path.as_str(), f.text.as_deref())).collect::<Vec<_>>(),
        [("proposal.md", Some("## Why\n")), ("tasks.md", Some("- [ ] 1.1\n"))]
    );
    let mid = read_version_in(r.path(), CHANGE, &edited).expect("read");
    assert_eq!(mid.files[1].text.as_deref(), Some("- [x] 1.1\n"));
    let bin = read_version_in(r.path(), CHANGE, &with_binary).expect("read");
    let logo = bin.files.iter().find(|f| f.path == "logo.bin").expect("logo");
    assert!(logo.binary && logo.text.is_none() && logo.bytes == 3);
}

// CVH-FR-VSQD / CVH-FR-HXVV: a commit that is not a version is unknown, and a
// change the branch never held is not found.
#[test]
fn a_commit_that_is_not_a_version_is_refused() {
    let r = Repo::new();
    base(&r);
    let readme_only = {
        r.write("README.md", b"third\n");
        r.commit("Readme again")
    };
    assert_eq!(
        read_version_in(r.path(), CHANGE, &readme_only),
        Err(ERR_UNKNOWN_VERSION.to_string())
    );
    assert_eq!(
        read_version_in(r.path(), CHANGE, "not-an-id"),
        Err(ERR_UNKNOWN_VERSION.to_string())
    );
    assert_eq!(list_versions_in(r.path(), "never-here"), Err(ERR_CHANGE_NOT_FOUND.to_string()));
}

// CVH-FR-DAAQ: the list stops at its bound and says it is truncated.
#[test]
fn a_long_history_is_truncated_at_the_bound() {
    let r = Repo::new();
    base(&r);
    let listed = list_versions_limited(r.path(), CHANGE, 2).expect("versions");
    assert_eq!(listed.versions.len(), 2);
    assert!(listed.truncated);
}

// CVH-FR-DAAQ, CVH-FR-SJIF: where the versions newer than the archive version
// fill the bound exactly, the list is not truncated, because the archive
// version and the versions older than it are no versions of the change; one
// more version newer than the archive version truncates it.
#[test]
fn the_archive_version_at_the_bound_does_not_truncate_the_list() {
    let r = Repo::new();
    let (_, _, _, dir) = base(&r);
    archive(&r, &dir, &format!("2026-11-11-{CHANGE}"));
    r.write(&format!("{dir}/proposal.md"), b"## Why again\n");
    r.commit("Propose add-auth again");
    r.write(&format!("{dir}/tasks.md"), b"- [ ] 1.1\n");
    r.commit("Add the tasks");
    let listed = list_versions_limited(r.path(), CHANGE, 2).expect("versions");
    assert_eq!(listed.versions.len(), 2);
    assert!(!listed.truncated);
    r.write(&format!("{dir}/tasks.md"), b"- [x] 1.1\n");
    r.commit("Do the task");
    let listed = list_versions_limited(r.path(), CHANGE, 2).expect("versions");
    assert_eq!(listed.versions.len(), 2);
    assert!(listed.truncated);
}

// CVH-FR-DVTL: a read changes nothing in the repository or the working copy.
#[test]
fn a_read_writes_nothing() {
    let r = Repo::new();
    let (created, ..) = base(&r);
    let before = crate::tools::tests::tree_snapshot(r.path());
    list_versions_in(r.path(), CHANGE).expect("versions");
    read_version_in(r.path(), CHANGE, &created).expect("read");
    assert_eq!(crate::tools::tests::tree_snapshot(r.path()), before);
}

// CVH-FR-YDVP: a bad name and an unknown stream are refused by the count the
// statistics use.
#[test]
fn the_count_refuses_a_bad_name_and_an_unknown_stream() {
    assert_eq!(
        count_change_versions("cvh-none", "Bad Name"),
        Err(ERR_INVALID_CHANGE_NAME.to_string())
    );
    assert_eq!(count_change_versions("cvh-none", CHANGE), Err(ERR_UNKNOWN_STREAM.to_string()));
    let r = Repo::new();
    base(&r);
    crate::change_proposals::set_test_stream("cvh-s", "S", crate::fs::RootFs::for_root(r.path()));
    assert_eq!(count_change_versions("cvh-s", CHANGE), Ok(3));
}

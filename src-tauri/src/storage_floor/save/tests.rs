//! The tests of the save commit a stream checkout waits for.

use super::*;
use std::sync::Arc;
use tempfile::TempDir;

const ID: &str = "1a2b3c4d5e6-0001-deadbeef";

/// A repository with one commit and an authoring identity, holding a draft
/// committed under `prefix` (empty for a project at the repository root).
struct Fixture {
    dir: TempDir,
    prefix: String,
}

impl Fixture {
    fn new(prefix: &str) -> Fixture {
        let dir = TempDir::new().expect("tempdir");
        let repo = git2::Repository::init(dir.path()).expect("repo");
        let mut config = repo.config().expect("config");
        config.set_str("user.name", "An Author").expect("name");
        config.set_str("user.email", "author@example.com").expect("email");
        let f = Fixture { dir, prefix: prefix.to_string() };
        f.write(
            &format!(".synthesis/drafts/UI/{ID}/draft.toml"),
            &format!(
                "id = \"{ID}\"\nname = \"Push button\"\nstatus = \"active\"\n\
                 promptPath = \"P.md\"\ncreatedAt = \"2026-01-01T00:00:00Z\"\n\
                 updatedAt = \"2026-01-01T00:00:00Z\"\n"
            ),
        );
        f.write(&format!(".synthesis/drafts/UI/{ID}/files/P.md"), "committed\n");
        f.write(&format!(".synthesis/drafts/UI/{ID}/proposals/p.toml"), "state = \"pending\"\n");
        f.write("README.md", "the project\n");
        commit_all(&f.repo(), "the first commit");
        f
    }

    fn project_rel(&self, rel: &str) -> String {
        if self.prefix.is_empty() {
            rel.to_string()
        } else {
            format!("{}/{rel}", self.prefix)
        }
    }

    fn write(&self, rel: &str, contents: &str) {
        let path = self.dir.path().join(self.project_rel(rel));
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the folder");
        std::fs::write(path, contents).expect("the file");
    }

    fn repo(&self) -> git2::Repository {
        git2::Repository::open(self.dir.path()).expect("repo")
    }

    fn access(&self) -> Arc<FsAccess> {
        access_for(self.dir.path())
    }

    fn head_message(&self) -> String {
        let repo = self.repo();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        head.message().unwrap_or_default().to_string()
    }

    fn head_blob(&self, rel: &str) -> String {
        let repo = self.repo();
        let tree = repo.head().unwrap().peel_to_commit().unwrap().tree().unwrap();
        let entry = tree.get_path(std::path::Path::new(&self.project_rel(rel))).expect("in HEAD");
        let blob = entry.to_object(&repo).unwrap();
        String::from_utf8(blob.as_blob().unwrap().content().to_vec()).unwrap()
    }
}

fn access_for(root: &std::path::Path) -> Arc<FsAccess> {
    Arc::new(FsAccess::builder().allow_root(root).build().expect("an instance"))
}

fn commit_all(repo: &git2::Repository, message: &str) {
    let mut index = repo.index().expect("index");
    index.add_all(["*"], git2::IndexAddOption::DEFAULT, None).expect("add");
    index.write().expect("write");
    let tree = repo.find_tree(index.write_tree().expect("tree")).expect("tree");
    let signature = repo.signature().expect("signature");
    let parents: Vec<git2::Commit> = repo
        .head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok())
        .into_iter()
        .collect();
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &parent_refs)
        .expect("commit");
}

// PST-FR-RONA / PST-FR-YWXF: a changed committed draft is saved under its own
// `draft: save` message before a checkout; its private storage and an untracked
// new draft are not named.
#[test]
fn a_changed_draft_is_saved_and_private_storage_is_not() {
    let f = Fixture::new("");
    f.write(&format!(".synthesis/drafts/UI/{ID}/files/P.md"), "saved, never committed\n");
    f.write(&format!(".synthesis/drafts/UI/{ID}/proposals/p.toml"), "state = \"accepted\"\n");
    f.write(".synthesis/drafts/1a2b3c4d5e6-0002-cafebabe/draft.toml", "id = \"new\"\n");

    let saved = save_drafts_before_checkout(&f.access(), f.dir.path()).expect("saved");

    assert_eq!(saved, 1);
    assert_eq!(f.head_message(), "draft: save \"Push button\"");
    assert_eq!(f.head_blob(&format!(".synthesis/drafts/UI/{ID}/files/P.md")), "saved, never committed\n");
    assert_eq!(
        f.head_blob(&format!(".synthesis/drafts/UI/{ID}/proposals/p.toml")),
        "state = \"pending\"\n",
        "private storage an earlier build committed is not named (DRS-FR-PIWL)",
    );
    let status = f.repo().status_file(std::path::Path::new(".synthesis/drafts/1a2b3c4d5e6-0002-cafebabe/draft.toml")).unwrap();
    assert!(status.is_wt_new(), "the untracked draft is left untracked");
}

// PST-FR-RONA: a project below the repository root is saved at its own paths.
#[test]
fn a_colocated_project_saves_its_drafts_below_the_prefix() {
    let f = Fixture::new("apps/web");
    f.write(&format!(".synthesis/drafts/UI/{ID}/files/P.md"), "edited\n");

    let saved = save_drafts_before_checkout(&f.access(), f.dir.path()).expect("saved");

    assert_eq!(saved, 1);
    assert_eq!(f.head_blob(&format!(".synthesis/drafts/UI/{ID}/files/P.md")), "edited\n");
}

// PST-FR-RONA: nothing changed is no save and no refusal; a save that cannot
// be taken refuses with `draft_save_failed`.
#[test]
fn a_save_that_cannot_be_taken_refuses() {
    let f = Fixture::new("");
    assert_eq!(save_drafts_before_checkout(&f.access(), f.dir.path()), Ok(0));

    f.write(&format!(".synthesis/drafts/UI/{ID}/files/P.md"), "edited\n");
    std::fs::write(f.repo().path().join("MERGE_HEAD"), "0".repeat(40)).expect("a merge");
    assert_eq!(
        save_drafts_before_checkout(&f.access(), f.dir.path()),
        Err(ERR_DRAFT_SAVE_FAILED.to_string()),
    );

    let plain = TempDir::new().expect("tempdir");
    let access = access_for(plain.path());
    assert_eq!(save_drafts_before_checkout(&access, plain.path()), Ok(0), "no repository, no save");
}

// PST-FR-RONA / GTC-FR-19: a worktree the shared filesystem instance does not
// cover is refused rather than saved, and nothing of its drafts is committed as
// a deletion.
#[test]
fn a_worktree_outside_the_shared_instance_is_refused_and_nothing_is_deleted() {
    let f = Fixture::new("");
    f.write(&format!(".synthesis/drafts/UI/{ID}/files/P.md"), "edited\n");
    let elsewhere = TempDir::new().expect("tempdir");
    let before = f.repo().head().unwrap().peel_to_commit().unwrap().id();

    assert_eq!(
        save_drafts_before_checkout(&access_for(elsewhere.path()), f.dir.path()),
        Err(ERR_DRAFT_SAVE_FAILED.to_string()),
    );
    assert_eq!(f.repo().head().unwrap().peel_to_commit().unwrap().id(), before, "no commit");
    assert_eq!(f.head_blob(&format!(".synthesis/drafts/UI/{ID}/files/P.md")), "committed\n");

    // GTC-FR-19: a named path the gate refuses is reported, not recorded as a
    // deletion.
    let outside = RootFs::new(f.dir.path().to_path_buf(), access_for(elsewhere.path()));
    let refused = crate::git::commit_exact_paths(
        &outside,
        "draft: save \"Push button\"",
        &[format!(".synthesis/drafts/UI/{ID}/files/P.md")],
    );
    assert!(refused.is_err(), "{refused:?}");
    assert_eq!(f.repo().head().unwrap().peel_to_commit().unwrap().id(), before);
}


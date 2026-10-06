use super::*;

// DRS-FR-ZIVL / DRS-FR-VECL / DRS-FR-BKFG: an event names the committed draft
// storage of its draft — the record, the prompt, and the images — wherever the
// draft is filed, and the drafts root's two Git files.
#[test]
fn an_event_names_the_committed_layout_of_its_draft() {
    // The shape `new_draft_id` generates: `<11 hex>-<4 hex>-<8 hex>`.
    let id = "1a2b3c4d5e6-0001-deadbeef";
    for named in [
        ".gitattributes".to_string(),
        ".gitignore".to_string(),
        format!("{id}/draft.toml"),
        format!("{id}/files/A prompt.md"),
        format!("{id}/assets/abc.png"),
        format!("{id}/assets/filenames.toml"),
        // A leading dot is a legal name for a draft's own file, so an asset the
        // author called `.logo.png` is committed like any other.
        format!("{id}/assets/.logo.png"),
        // Filed inside the author's folders, at every depth.
        format!("UI/Components/{id}/draft.toml"),
        format!("UI/Components/{id}/files/p.md"),
    ] {
        assert!(is_draft_event_rel(&named, id, false), "{named} is named");
        assert!(is_draft_event_rel(&named, id, true), "{named} is named by a deletion");
    }
}

// DRS-FR-ZIVL / DRS-FR-WYIN / DRS-FR-XPQI / DRS-FR-NXWO: private draft
// storage, a transient file, and a file the author put under the drafts root
// are named by no event; a deletion names every path under its own draft's
// folder but a transient one.
#[test]
fn private_transient_and_authored_files_are_not_named() {
    let id = "1a2b3c4d5e6-0001-deadbeef";
    for private in [
        format!("{id}/conversation.jsonl"),
        format!("{id}/proposals/p1.toml"),
        format!("{id}/history/e1.snapshot"),
        format!("{id}/publication.toml"),
        format!("{id}/notes.md"),
        // A draft's review is no longer inside its folder (CMS-FR-37).
        format!("{id}/comments/discussion.jsonl"),
    ] {
        assert!(!is_draft_event_rel(&private, id, false), "{private} is not named");
        assert!(is_draft_event_rel(&private, id, true), "{private} goes with a deletion");
    }
    for outsider in [
        String::new(),
        ".".to_string(),
        "notes.md".to_string(),
        "UI/my-log.jsonl".to_string(),
        id.to_string(),
        "Research/history/2024-notes.md".to_string(),
        "Research/files/an outline.md".to_string(),
        "1a2b3c4d5e6-0002-cafebabe/draft.toml".to_string(),
        format!("{id}/proposals/.abc.toml.tmp.1234.5678/candidate"),
        format!("{id}/files/.A prompt.md.synthesis-rename"),
        format!("{id}/assets/.abc.png.tmp.1234.5678"),
        format!("{id}/files/.A prompt.md.tmp.42.99"),
        format!("{id}/../../escape.md"),
    ] {
        assert!(!is_draft_event_rel(&outsider, id, false), "{outsider} is not named");
        assert!(!is_draft_event_rel(&outsider, id, true), "{outsider} is not named by a deletion");
    }
}

// ---------------------------------------------------------------------------
// What reaches the repository (DRS-FR-DGMI, DRS-FR-JDRY, DRS-FR-QHHY,
// DRS-FR-VECL, DRS-FR-WYIN, DRS-FR-HRIB, DRS-FR-GWNI, DRS-FR-NRQQ)
// ---------------------------------------------------------------------------

use crate::fs::RootFs;
use tempfile::TempDir;

/// A project whose worktree is a repository with one commit and a configured
/// authoring identity — a checkout as an author leaves one.
struct Fixture {
    _dir: TempDir,
    root: RootFs,
}

impl Fixture {
    /// A project in a repository.
    fn git() -> Fixture {
        let fixture = Fixture::bare();
        let repo = git2::Repository::init(fixture.root.path()).expect("a repository");
        let mut config = repo.config().expect("config");
        config.set_str("user.name", "An Author").expect("a name");
        config
            .set_str("user.email", "author@example.com")
            .expect("an address");
        std::fs::write(fixture.root.path().join("README.md"), "the project\n").expect("a file");
        commit_everything(&repo, "the first commit");
        fixture
    }

    /// A project in no repository at all, which is every project outside Git.
    fn bare() -> Fixture {
        let dir = TempDir::new().expect("a temporary folder");
        std::fs::create_dir_all(dir.path().join(".synthesis")).expect("the scaffold");
        let root = RootFs::for_root(dir.path());
        Fixture { _dir: dir, root }
    }

    fn repo(&self) -> git2::Repository {
        git2::Repository::open(self.root.path()).expect("a repository")
    }

    /// Every path the current `HEAD` commit holds.
    fn committed_paths(&self) -> Vec<String> {
        // A project in no repository has committed nothing, which is what
        // DRS-FR-HRIB asks this to answer rather than to panic over.
        let Ok(repo) = git2::Repository::open(self.root.path()) else {
            return Vec::new();
        };
        let Ok(head) = repo.head() else {
            return Vec::new();
        };
        let tree = head.peel_to_commit().expect("a commit").tree().expect("a tree");
        let mut paths = Vec::new();
        tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
            if entry.kind() == Some(git2::ObjectType::Blob) {
                paths.push(format!("{dir}{}", entry.name().unwrap_or_default()));
            }
            git2::TreeWalkResult::Ok
        })
        .expect("a walk");
        paths
    }

    /// Take the commits of every draft event raised so far, here and now.
    fn commit_now(&self) {
        crate::storage_floor::commit::wait_for_committer(&self.root);
    }
}

fn commit_everything(repo: &git2::Repository, message: &str) {
    let mut index = repo.index().expect("an index");
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .expect("an add");
    index.write().expect("a written index");
    let tree = repo
        .find_tree(index.write_tree().expect("a tree"))
        .expect("a tree");
    let signature = repo.signature().expect("a signature");
    let parents: Vec<git2::Commit> = repo
        .head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok())
        .into_iter()
        .collect();
    let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
    repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &parent_refs)
        .expect("a commit");
}

/// Whether Git ignores a project-relative path in the fixture's repository.
fn ignored(f: &Fixture, rel: &str) -> bool {
    f.repo()
        .status_should_ignore(std::path::Path::new(rel))
        .expect("an ignore answer")
}

use crate::storage_floor::commit::DraftEvent;

// DRS-FR-DGMI / DRS-FR-JDRY / DRS-FR-BKFG: the attributes file and the ignore
// file exist before the first draft is created, carry their entries, and
// preserve an entry the author added to either.
#[test]
fn the_drafts_root_git_files_exist_before_the_first_draft() {
    let f = Fixture::bare();
    let root = &f.root;
    let attributes = root.path().join(".synthesis/drafts/.gitattributes");
    let ignore = root.path().join(".synthesis/drafts/.gitignore");

    crate::drafts::create_draft_impl(root, Some("a prompt"), None).expect("a draft");
    assert_eq!(
        std::fs::read_to_string(&attributes).expect("the attributes file"),
        "*.jsonl merge=union\n",
        "the pattern carries no slash, so it reaches every depth (DRS-FR-PEAY)",
    );
    let entries = std::fs::read_to_string(&ignore).expect("the ignore file");
    for item in ["history/", "proposals/", "publication.toml", "conversation.jsonl"] {
        assert!(
            entries.lines().any(|line| line.starts_with("**/[0-9a-fA-F]") && line.ends_with(item)),
            "{item} is ignored inside a draft: {entries}",
        );
    }

    // DRS-FR-BKFG: an entry the author added survives every later ensure, and
    // the module's own entry is not written a second time.
    std::fs::write(&attributes, "*.png binary\n*.jsonl merge=union\n").expect("the author's file");
    std::fs::write(&ignore, format!("scratch/\n{entries}")).expect("the author's entry");
    super::ensure_git_files(root).expect("an ensure");
    assert_eq!(
        std::fs::read_to_string(&attributes).expect("the attributes file"),
        "*.png binary\n*.jsonl merge=union\n",
    );
    assert_eq!(
        std::fs::read_to_string(&ignore).expect("the ignore file"),
        format!("scratch/\n{entries}"),
    );

    // DRS-FR-DGMI: a file removed behind the module's back — what a branch
    // switch, a merge, or a `git clean` does — is written again by the next
    // ensure.
    std::fs::remove_file(&attributes).expect("the file goes");
    std::fs::remove_file(&ignore).expect("the file goes");
    super::ensure_git_files(root).expect("an ensure");
    assert_eq!(
        std::fs::read_to_string(&attributes).expect("the attributes file"),
        "*.jsonl merge=union\n",
    );
    assert_eq!(std::fs::read_to_string(&ignore).expect("the ignore file"), entries);
}

// DRS-FR-JDRY / DRS-FR-WYIN / DRS-FR-XPQI: Git ignores the private storage of
// a draft at every depth, and nothing of a drafts folder the author named
// `history` or `proposals`.
#[test]
fn private_draft_storage_is_ignored_and_the_authors_folders_are_not() {
    let f = Fixture::git();
    let root = &f.root;
    super::ensure_git_files(root).expect("an ensure");
    let id = "1a2b3c4d5e6-0001-deadbeef";
    for private in [
        format!(".synthesis/drafts/{id}/proposals/p1.toml"),
        format!(".synthesis/drafts/{id}/history/e1.snapshot"),
        format!(".synthesis/drafts/{id}/history/journal.toml"),
        format!(".synthesis/drafts/{id}/publication.toml"),
        format!(".synthesis/drafts/{id}/conversation.jsonl"),
        format!(".synthesis/drafts/UI/Components/{id}/proposals/p1.toml"),
    ] {
        assert!(ignored(&f, &private), "{private} is ignored");
    }
    for public in [
        format!(".synthesis/drafts/{id}/draft.toml"),
        format!(".synthesis/drafts/{id}/files/A prompt.md"),
        format!(".synthesis/drafts/{id}/assets/shot.png"),
        ".synthesis/drafts/Research/history/2024-notes.md".to_string(),
        ".synthesis/drafts/history/proposals/plan.md".to_string(),
        ".synthesis/drafts/Research/publication.toml".to_string(),
        ".synthesis/proposals-of-my-own.md".to_string(),
    ] {
        assert!(!ignored(&f, &public), "{public} is not ignored");
    }
}

// PST-FR-DQZT / DRS-FR-QHHY / DRS-FR-VECL / DRS-FR-XPQI: creating a draft
// commits that draft's committed storage and nothing else — not the author's
// staged file, not their own file under the drafts root, not a transient one,
// and not another draft.
#[test]
fn a_creation_commits_its_draft_and_nothing_else() {
    let f = Fixture::git();
    let root = &f.root;

    // A draft already on disk and never committed.
    let existing = root.path().join(".synthesis/drafts/1000000000a-0009-0badcafe/files");
    std::fs::create_dir_all(&existing).expect("the older draft");
    std::fs::write(existing.join("p.md"), "an older prompt").expect("its prompt");

    // The author's own work: one path they staged, and one file they put under
    // the drafts root themselves.
    std::fs::write(root.path().join("mine.md"), "my work\n").expect("the author's file");
    let repo = f.repo();
    let mut index = repo.index().expect("an index");
    index.add_path(std::path::Path::new("mine.md")).expect("a stage");
    index.write().expect("a written index");
    std::fs::write(
        root.path().join(".synthesis/drafts/notes-of-my-own.md"),
        "not a draft\n",
    )
    .expect("the author's note");

    let created = crate::drafts::create_draft_impl(root, Some("a prompt"), None).expect("a draft");
    let id = created.draft.id.clone();
    // A transient file, as an interrupted atomic write leaves one.
    std::fs::write(
        root.path().join(format!(".synthesis/drafts/{id}/.draft.toml.tmp.42.99")),
        "half written\n",
    )
    .expect("the temporary file");

    f.commit_now();
    let committed = f.committed_paths();
    for named in [
        ".synthesis/drafts/.gitattributes".to_string(),
        ".synthesis/drafts/.gitignore".to_string(),
        format!(".synthesis/drafts/{id}/draft.toml"),
        format!(".synthesis/drafts/{id}/files/a prompt.md"),
    ] {
        assert!(committed.contains(&named), "{named} is committed: {committed:?}");
    }
    for unnamed in [
        "mine.md".to_string(),
        ".synthesis/drafts/notes-of-my-own.md".to_string(),
        format!(".synthesis/drafts/{id}/.draft.toml.tmp.42.99"),
        ".synthesis/drafts/1000000000a-0009-0badcafe/files/p.md".to_string(),
    ] {
        assert!(!committed.contains(&unnamed), "{unnamed} is not committed: {committed:?}");
    }
    let repo = f.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.message().unwrap_or_default(), "draft: create \"a prompt\"");
    // GTC-FR-19: the path the author staged stays staged and uncommitted.
    let status = f.repo().status_file(std::path::Path::new("mine.md")).expect("a status");
    assert!(
        status.contains(git2::Status::INDEX_NEW),
        "the author's staged path is still staged: {status:?}",
    );

    // DRS-FR-21 / DRS-FR-VECL: a deletion reaches the repository through its
    // own event, and it names that draft's paths alone.
    crate::drafts::delete_draft_impl(root, root, &id).expect("the deletion");
    f.commit_now();
    let after = f.committed_paths();
    assert!(
        !after.iter().any(|path| path.contains(&id)),
        "the deleted draft's paths are gone: {after:?}",
    );
    let repo = f.repo();
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.message().unwrap_or_default(), "draft: delete \"a prompt\"");
}

// DRS-FR-23: a draft is created with no `conversation.jsonl`.
#[test]
fn a_draft_is_created_with_no_conversation_log() {
    let f = Fixture::bare();
    let created = crate::drafts::create_draft_impl(&f.root, Some("a prompt"), None).expect("a draft");
    assert!(
        !f.root
            .path()
            .join(format!(".synthesis/drafts/{}/conversation.jsonl", created.draft.id))
            .exists(),
        "no conversation log is scaffolded",
    );
}

/// DRS-FR-HRIB: a project in no repository writes drafts exactly as one in a
/// repository does — no draft operation waits for the commit or fails with it.
#[test]
fn drs_ts_hrib_a_project_outside_git_writes_its_drafts_all_the_same() {
    let f = Fixture::bare();
    let root = &f.root;

    let created = crate::drafts::create_draft_impl(root, Some("a prompt"), None).expect("a draft");
    f.commit_now();

    let prompt = root
        .path()
        .join(format!(".synthesis/drafts/{}/files/a prompt.md", created.draft.id));
    assert!(prompt.is_file(), "the draft is complete on disk for a later attempt");
    assert!(f.committed_paths().is_empty(), "and there is no repository to commit to");
}

/// DRS-FR-GWNI / DRS-FR-JPVB: the commit is not attempted while a merge stands,
/// and the draft files stay on disk for the attempt after it.
#[test]
fn drs_ts_gwni_no_commit_is_taken_while_a_merge_stands() {
    let f = Fixture::git();
    let root = &f.root;
    let head = f.repo().head().unwrap().peel_to_commit().unwrap().id();
    // What Git leaves in a repository holding a merge: the state every
    // `repository_unready` check reads.
    std::fs::write(root.path().join(".git/MERGE_HEAD"), format!("{head}\n")).expect("a merge");

    let created = crate::drafts::create_draft_impl(root, Some("a prompt"), None).expect("a draft");
    f.commit_now();

    assert_eq!(
        f.repo().head().unwrap().peel_to_commit().unwrap().id(),
        head,
        "no commit lands inside an operation the author is in the middle of",
    );
    assert!(
        root.path()
            .join(format!(".synthesis/drafts/{}/draft.toml", created.draft.id))
            .is_file(),
        "and the draft is complete on disk for the next attempt (DRS-FR-HRIB)",
    );
}

/// DRS-FR-AFSU: the commit runs against the active worktree, **which may be a
/// linked one**, so no Git operation reads or writes a sibling worktree.
///
/// A linked worktree is where this could go wrong without being noticed: its
/// `.git` is a file rather than a directory and its `HEAD` is its own, so a
/// commit taken against the repository rather than against the root it was
/// handed would move the primary worktree's branch instead.
#[test]
fn drs_ts_afsu_a_linked_worktree_commits_its_own_drafts_and_moves_no_sibling() {
    let primary = Fixture::git();
    let repo = primary.repo();
    // A branch for the linked worktree to check out, which is what
    // `git worktree add` needs.
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    repo.branch("linked", &head, false).expect("a branch");
    let reference = repo
        .find_branch("linked", git2::BranchType::Local)
        .unwrap()
        .into_reference();
    let mut options = git2::WorktreeAddOptions::new();
    options.reference(Some(&reference));
    // Outside the primary worktree, so it is a second checkout rather than a
    // directory inside the one under test.
    let holder = TempDir::new().expect("a temporary folder");
    let linked_path = holder.path().join("linked");
    repo.worktree("linked", &linked_path, Some(&options))
        .expect("a linked worktree");
    let linked = RootFs::for_root(&linked_path);
    let primary_head = repo.head().unwrap().peel_to_commit().unwrap().id();

    crate::drafts::create_draft_impl(&linked, Some("a prompt"), None).expect("a draft");
    crate::storage_floor::commit::wait_for_committer(&linked);

    // The linked worktree's own `HEAD` carries the draft…
    let linked_repo = git2::Repository::open(&linked_path).expect("the linked worktree");
    let committed = linked_repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .expect("a commit")
        .tree()
        .expect("a tree");
    assert!(
        committed
            .get_path(std::path::Path::new(".synthesis/drafts"))
            .is_ok(),
        "the worktree the draft was written in carries it",
    );
    // …and the primary worktree's branch has not moved, nor has its checkout
    // gained a drafts folder.
    assert_eq!(
        repo.head().unwrap().peel_to_commit().unwrap().id(),
        primary_head,
        "no Git operation reads or writes a sibling worktree",
    );
    assert!(
        !primary.root.path().join(".synthesis/drafts").exists(),
        "and nothing was written into the sibling checkout",
    );
}

// DRS-FR-ISPI / DRS-FR-NRQQ / DRS-FR-WYIN: an existing project whose ignore
// file still excludes the drafts is migrated by removing that entry, and a
// draft already on disk reaches the repository through the next draft event
// of that draft — its committed storage, and not its proposals.
#[test]
fn drs_ts_ispi_a_migrated_project_commits_the_drafts_it_already_had() {
    let f = Fixture::git();
    let root = &f.root;

    // A project as an earlier version of this application left one: the ignore
    // file excludes the drafts, and a draft is already on disk.
    let id = "1000000000a-0009-0badcafe";
    let files = root.path().join(format!(".synthesis/drafts/{id}/files"));
    std::fs::create_dir_all(&files).expect("the draft");
    std::fs::write(files.join("p.md"), "an older prompt").expect("its prompt");
    std::fs::create_dir_all(root.path().join(format!(".synthesis/drafts/{id}/proposals")))
        .expect("its proposals folder");
    std::fs::write(
        root.path().join(format!(".synthesis/drafts/{id}/proposals/p1.toml")),
        "id = \"p1\"\n",
    )
    .expect("a proposal");
    std::fs::write(
        root.path().join(format!(".synthesis/drafts/{id}/draft.toml")),
        "id = \"1000000000a-0009-0badcafe\"\nname = \"older\"\nstatus = \"active\"\n\
         prompt_path = \"p.md\"\ncreated_at = \"2026-01-01T00:00:00Z\"\n\
         updated_at = \"2026-01-01T00:00:00Z\"\n",
    )
    .expect("its record");
    std::fs::create_dir_all(root.path().join(".synthesis")).expect("the scaffold");
    std::fs::write(
        root.path().join(".synthesis/.gitignore"),
        "cache/\ndrafts/\nlocal.toml\nproposals/\n",
    )
    .expect("the old ignore file");

    // While the entry stands, the commit sees nothing at all.
    super::commit_event(root, id, "older", DraftEvent::GraduationStarted);
    f.commit_now();
    assert!(
        !f.committed_paths()
            .iter()
            .any(|path| path.starts_with(".synthesis/drafts/")),
        "an ignored path is invisible to a working-tree status",
    );

    // FSA-FR-08: the migration, which is the whole of what an existing project
    // needs — and which writes the ignore file and no other file.
    root.ensure_gitignored(root.path().join(".synthesis"))
        .expect("the migration");
    assert_eq!(
        std::fs::read_to_string(files.join("p.md")).expect("the prompt"),
        "an older prompt",
        "DRS-FR-ISPI: every draft file stays exactly where it was",
    );
    assert!(
        !f.committed_paths()
            .iter()
            .any(|path| path.starts_with(".synthesis/drafts/")),
        "DRS-FR-NRQQ: and the migration itself commits nothing",
    );

    // DRS-FR-NRQQ: the next draft event of that draft carries its committed
    // storage. Its proposal is private draft storage and stays out of Git.
    super::ensure_private_ignored(root);
    super::commit_event(root, id, "older", DraftEvent::GraduationStarted);
    f.commit_now();

    let committed = f.committed_paths();
    for named in [
        ".synthesis/drafts/.gitattributes".to_string(),
        ".synthesis/drafts/.gitignore".to_string(),
        format!(".synthesis/drafts/{id}/draft.toml"),
        format!(".synthesis/drafts/{id}/files/p.md"),
    ] {
        assert!(committed.contains(&named), "{named} is committed: {committed:?}");
    }
    let proposal = format!(".synthesis/drafts/{id}/proposals/p1.toml");
    assert!(!committed.contains(&proposal), "the proposal is not committed: {committed:?}");
    assert!(ignored(&f, &proposal), "and Git ignores it");
}

// PST-FR-DQZT / DRS-FR-WYIN: the writers of a draft's images, proposals, and
// history raise no commit. The image reaches the repository through the next
// draft event; the proposal and the history never do.
#[test]
fn private_writers_commit_nothing_and_an_image_waits_for_the_next_event() {
    let f = Fixture::git();
    let root = &f.root;
    let created = crate::drafts::create_draft_impl(root, Some("a prompt"), None).expect("a draft");
    let id = created.draft.id.clone();
    f.commit_now();
    let after_creation = f.repo().head().unwrap().peel_to_commit().unwrap().id();

    // An image the prompt embeds (`DAS-draft-assets.md` DAS-FR-01).
    let png = {
        use base64::Engine as _;
        let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        bytes.extend_from_slice(b"one");
        base64::engine::general_purpose::STANDARD.encode(&bytes)
    };
    let asset = crate::draft_assets::store_image_impl(
        root,
        &id,
        "image/png",
        Some("shot.png"),
        &png,
        &|_: &str| {},
    )
    .expect("an asset");
    // A proposal an agent recorded (`DCP-draft-change-proposals.md` DCP-FR-01)
    // and a version an acceptance settled (`DHS-draft-history.md` DHS-FR-01).
    let proposals = crate::drafts::draft_proposals_dir(root, &id).expect("the folder");
    std::fs::write(proposals.join("p1.content"), "a proposed prompt\n").expect("a candidate");
    let history = crate::drafts::draft_history_dir(root, &id).expect("the folder");
    std::fs::write(history.join("e1.snapshot"), "the prompt as it was\n").expect("a snapshot");
    f.commit_now();
    assert_eq!(
        f.repo().head().unwrap().peel_to_commit().unwrap().id(),
        after_creation,
        "no write of these raised a commit",
    );

    super::commit_event(root, &id, "a prompt", DraftEvent::GraduationStarted);
    f.commit_now();
    let committed = f.committed_paths();
    let asset_path = format!(".synthesis/drafts/{id}/{}", asset.path);
    assert!(committed.contains(&asset_path), "the image is committed: {committed:?}");
    for private in [
        format!(".synthesis/drafts/{id}/proposals/p1.content"),
        format!(".synthesis/drafts/{id}/history/e1.snapshot"),
    ] {
        assert!(!committed.contains(&private), "{private} is not committed: {committed:?}");
        assert!(ignored(&f, &private), "{private} is ignored");
    }
}

// PST-FR-DQZT / DRS-FR-12: a saved prompt makes no commit. It reaches the
// repository through the next draft event, with the bytes the save wrote.
#[test]
fn a_saved_prompt_waits_for_the_next_draft_event() {
    let f = Fixture::git();
    let root = &f.root;
    let created = crate::drafts::create_draft_impl(root, Some("a prompt"), None).expect("a draft");
    let id = created.draft.id.clone();
    f.commit_now();
    let after_creation = f.repo().head().unwrap().peel_to_commit().unwrap().id();

    crate::drafts::save_draft_file_impl(root, &id, &created.file, "# the second draft\n")
        .expect("a save");
    f.commit_now();
    assert_eq!(
        f.repo().head().unwrap().peel_to_commit().unwrap().id(),
        after_creation,
        "the save raised no commit",
    );

    super::commit_event(root, &id, "a prompt", DraftEvent::GraduationStarted);
    f.commit_now();
    let repo = f.repo();
    let head = repo.head().and_then(|head| head.peel_to_commit()).expect("a commit");
    assert_eq!(head.message().unwrap_or_default(), "draft: graduate \"a prompt\"");
    let object = head
        .tree()
        .expect("a tree")
        .get_path(std::path::Path::new(&format!(
            ".synthesis/drafts/{id}/files/{}",
            created.file
        )))
        .expect("the prompt is committed")
        .to_object(&repo)
        .expect("an object");
    assert_eq!(
        std::str::from_utf8(object.as_blob().expect("a blob").content()).expect("text"),
        "# the second draft\n",
        "the committed bytes are the bytes the save wrote",
    );
}

// DRS-FR-JDRY / DRS-FR-WYIN: a writer of private draft storage ensures the
// ignore file again, so a removed `.gitignore` comes back with the next
// proposal, history entry, or publication write, and the write itself never
// fails because of it.
#[test]
fn a_private_writer_restores_the_ignore_file() {
    let f = Fixture::git();
    let root = &f.root;
    let created = crate::drafts::create_draft_impl(root, Some("a prompt"), None).expect("a draft");
    let id = created.draft.id.clone();
    let ignore = root.path().join(".synthesis/drafts/.gitignore");

    std::fs::remove_file(&ignore).expect("the file goes");
    crate::github_publication::store::write_store(root, &id, &Default::default()).expect("a store");
    assert!(ignore.is_file(), "the publication write restored it");
    assert!(ignored(&f, &format!(".synthesis/drafts/{id}/publication.toml")));

    // A `.gitignore` that cannot be written fails no private write.
    std::fs::remove_file(&ignore).expect("the file goes");
    std::fs::create_dir_all(&ignore).expect("a folder in its place");
    super::ensure_private_ignored(root);
    crate::github_publication::store::write_store(root, &id, &Default::default())
        .expect("the write does not fail because of the ignore file");
}


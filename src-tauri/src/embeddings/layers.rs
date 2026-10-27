//! What each layer must hold (EMB-FR-LGLI, EMB-FR-FALJ, EMB-FR-DCXI,
//! EMB-FR-MBJV, EMB-FR-NTMA, EMB-FR-MGML, EMB-FR-HDFV).
//!
//! The working copy's chunks come from the published BM25 snapshot, so they
//! are cut by the same rules and named by the same chunk IDs. The root layer's
//! chunks are read from the Git objects of the root tip and cut by the same
//! function (`bm25_index::chunk_file_text`).

use std::collections::BTreeMap;
use std::path::Path;

use git2::Repository;

use super::store::Entries;
use crate::bm25_index::{self, ChunkKey};
use crate::scanning::{self, ArtifactType, ContentFacts, LibraryAssignments};

/// One chunk with its content hash (EMB-FR-KNBG).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub hash: String,
    pub text: String,
}

/// Chunks by chunk ID.
pub type Chunks = BTreeMap<String, Chunk>;

/// What a layer entry must be after a pass.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Want {
    Chunk(Chunk),
    /// EMB-FR-DCXI: a root chunk the working copy does not have.
    Deleted,
}

/// The entries a layer must hold, by chunk ID.
pub type Desired = BTreeMap<String, Want>;

/// EMB-FR-KNBG: the content hash of a chunk text.
pub fn hash_text(text: &str) -> String {
    crate::fs::sha256_bytes(text.as_bytes())
}

/// The chunks of a BM25 index, by chunk ID (EMB-FR-IRQA).
pub fn chunks_from(index_chunks: Vec<(ChunkKey, String)>) -> Chunks {
    index_chunks
        .into_iter()
        .map(|(key, text)| (key.chunk_id(), Chunk { hash: hash_text(&text), text }))
        .collect()
}

/// A layer that holds every chunk of `chunks`: the root layer, the documents
/// layer, and the one layer of a project outside Git (EMB-FR-MGML).
pub fn whole(chunks: &Chunks) -> Desired {
    chunks.iter().map(|(id, chunk)| (id.clone(), Want::Chunk(chunk.clone()))).collect()
}

/// EMB-FR-DCXI, EMB-FR-MBJV: the branch layer — each working chunk that is new
/// or differs from the root chunk with the same ID, and a deletion marker for
/// each root chunk the working copy does not have. A chunk equal to its root
/// chunk is never part of it.
pub fn branch_difference(working: &Chunks, root: &Entries) -> Desired {
    let mut out = Desired::new();
    for (id, chunk) in working {
        if root.get(id).and_then(|h| h.as_ref()) != Some(&chunk.hash) {
            out.insert(id.clone(), Want::Chunk(chunk.clone()));
        }
    }
    for (id, hash) in root {
        if hash.is_some() && !working.contains_key(id) {
            out.insert(id.clone(), Want::Deleted);
        }
    }
    out
}

/// The `HEAD` of the active worktree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Head {
    /// A named branch, born or not.
    Branch(String),
    /// EMB-FR-NTMA: a detached `HEAD` has no branch layer.
    Detached,
}

/// What the layers depend on in Git.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GitFacts {
    /// EMB-FR-MGML: the project is not inside a Git repository.
    NoRepository,
    Repository {
        /// The project folder in the repository, `""` at its top.
        prefix: String,
        /// EMB-FR-HDFV: the default branch and its tip, when it resolves.
        root_branch: Option<String>,
        root_tip: Option<String>,
        head: Head,
        /// EMB-FR-USBA: the local branches that exist now.
        branches: Vec<String>,
    },
}

impl GitFacts {
    /// The branch of the active worktree, when it is on one.
    pub fn branch(&self) -> Option<&str> {
        match self {
            GitFacts::Repository { head: Head::Branch(name), .. } => Some(name),
            _ => None,
        }
    }

    pub fn root_tip(&self) -> Option<&str> {
        match self {
            GitFacts::Repository { root_tip, .. } => root_tip.as_deref(),
            GitFacts::NoRepository => None,
        }
    }
}

/// The Git facts of the content root `root`.
pub fn git_facts(root: &Path) -> GitFacts {
    let Ok(repo) = crate::changes::open_repo(root) else {
        return GitFacts::NoRepository;
    };
    let prefix = crate::changes::project_prefix(&repo, root);
    let root_branch = crate::changes::default_branch(&repo).ok();
    let root_tip = root_branch
        .as_deref()
        .and_then(|name| crate::git::objects::branch_tip(&repo, name).ok())
        .map(|oid| oid.to_string());
    let head = if repo.head_detached().unwrap_or(false) {
        Head::Detached
    } else {
        match repo.head() {
            Ok(head) => {
                head.shorthand().map(|s| Head::Branch(s.to_string())).unwrap_or(Head::Detached)
            }
            // An unborn branch still has a name, so the working copy has a layer.
            Err(_) => repo
                .find_reference("HEAD")
                .ok()
                .and_then(|r| r.symbolic_target().ok().flatten().map(str::to_string))
                .and_then(|t| t.strip_prefix("refs/heads/").map(|b| Head::Branch(b.to_string())))
                .unwrap_or(Head::Detached),
        }
    };
    let branches = repo
        .branches(Some(git2::BranchType::Local))
        .map(|iter| {
            iter.filter_map(|b| {
                b.ok().and_then(|(b, _)| b.name().ok().flatten().map(str::to_string))
            })
            .collect()
        })
        .unwrap_or_default();
    GitFacts::Repository { prefix, root_branch, root_tip, head, branches }
}

/// What the root reader found: the chunks, and each specification file that
/// gave none, with the reason (EMB-FR-OLDK).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RootRead {
    pub chunks: Chunks,
    pub skipped: Vec<(String, &'static str)>,
}

/// EMB-FR-LGLI, EMB-FR-FALJ: the specification chunks of the commit `tip`,
/// read from the Git objects with no checkout. Only the files below the
/// project folder `prefix` count, under their project-relative paths, and each
/// is classified by the rules of ASC-FR-06 with the assignments of that tree.
pub fn root_chunks(repo: &Repository, tip: &str, prefix: &str) -> Result<RootRead, String> {
    let commit = git2::Oid::from_str(tip)
        .map_err(|_| crate::git::objects::ERR_OBJECT_MISSING.to_string())?;
    let files = crate::git::objects::list_files_at(repo, commit, prefix)?;
    let mut by_rel: BTreeMap<String, git2::Oid> = BTreeMap::new();
    for file in files {
        if let Some(rel) = crate::changes::to_project_rel(prefix, &file.path) {
            by_rel.insert(rel, file.blob);
        }
    }
    let assignments: LibraryAssignments = by_rel
        .get(scanning::LIBRARY_TOML_REL)
        .and_then(|blob| crate::git::objects::read_blob(repo, *blob).ok())
        .and_then(|bytes| String::from_utf8(bytes).ok())
        .and_then(|text| toml::from_str(&text).ok())
        .unwrap_or_default();
    let read_text = |blob: &git2::Oid| -> Option<String> {
        let bytes = crate::git::objects::read_blob(repo, *blob).ok()?;
        if bytes.len() as u64 > bm25_index::MAX_FILE_BYTES {
            return None;
        }
        String::from_utf8(bytes).ok()
    };
    let content = |rel: &str| -> ContentFacts {
        if !rel.to_ascii_lowercase().ends_with(".md") {
            return ContentFacts::default();
        }
        by_rel
            .get(rel)
            .and_then(read_text)
            .map(|text| scanning::read_content_facts(&text))
            .unwrap_or_default()
    };

    let mut out = RootRead::default();
    for (rel, blob) in &by_rel {
        // ASC-FR-09: the same scope rules the scan applies to the working copy.
        if rel == scanning::LIBRARY_TOML_REL || in_pruned_dir(rel) {
            continue;
        }
        if scanning::classify_file(rel, &assignments, &content).0 != Some(ArtifactType::Spec) {
            continue;
        }
        // BMI-FR-09: an oversize or non-UTF-8 file has no chunks.
        let Some(text) = read_text(blob) else {
            out.skipped.push((rel.clone(), "too large or not UTF-8"));
            continue;
        };
        for (ordinal, chunk) in bm25_index::chunk_file_text(&text, false).into_iter().enumerate() {
            let id = bm25_index::chunk_id(rel, ordinal as u32);
            out.chunks.insert(id, Chunk { hash: hash_text(&chunk), text: chunk });
        }
    }
    Ok(out)
}

fn in_pruned_dir(rel: &str) -> bool {
    let mut dir = rel;
    while let Some((parent, _)) = dir.rsplit_once('/') {
        if scanning::is_pruned_dir(parent) {
            return true;
        }
        dir = parent;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunks(pairs: &[(&str, &str)]) -> Chunks {
        pairs
            .iter()
            .map(|(id, text)| {
                (id.to_string(), Chunk { hash: hash_text(text), text: text.to_string() })
            })
            .collect()
    }

    fn root_entries(pairs: &[(&str, &str)]) -> Entries {
        pairs.iter().map(|(id, text)| (id.to_string(), Some(hash_text(text)))).collect()
    }

    // EMB-FR-DCXI, EMB-FR-MBJV: the branch layer holds new and changed chunks
    // and deletion markers, and never a chunk equal to its root chunk.
    #[test]
    fn the_branch_layer_is_the_difference_from_the_root() {
        let root = root_entries(&[("a#0", "same"), ("a#1", "old"), ("b#0", "gone")]);
        let working = chunks(&[("a#0", "same"), ("a#1", "new"), ("c#0", "added")]);
        let diff = branch_difference(&working, &root);
        assert_eq!(diff.len(), 3);
        assert!(!diff.contains_key("a#0"), "an unchanged chunk is not stored again");
        assert!(matches!(diff.get("a#1"), Some(Want::Chunk(c)) if c.text == "new"));
        assert!(matches!(diff.get("c#0"), Some(Want::Chunk(_))));
        assert_eq!(diff.get("b#0"), Some(&Want::Deleted));
    }

    // EMB-FR-MBJV: a chunk edited back to the root text leaves the layer.
    #[test]
    fn a_chunk_equal_to_the_root_again_leaves_the_branch_layer() {
        let root = root_entries(&[("a#0", "text")]);
        assert!(branch_difference(&chunks(&[("a#0", "edited")]), &root).contains_key("a#0"));
        assert!(branch_difference(&chunks(&[("a#0", "text")]), &root).is_empty());
    }

    // EMB-FR-MGML, EMB-FR-PYCS: a whole layer holds every chunk.
    #[test]
    fn a_whole_layer_holds_every_chunk() {
        let working = chunks(&[("a#0", "x"), ("b#0", "y")]);
        assert_eq!(whole(&working).len(), 2);
    }

    // EMB-FR-HDFV, EMB-FR-NTMA, EMB-FR-MGML: the facts of a folder outside Git,
    // a branch, and a detached HEAD.
    #[test]
    fn git_facts_name_the_head_and_the_root_tip() {
        let outside = tempfile::tempdir().unwrap();
        assert_eq!(git_facts(outside.path()), GitFacts::NoRepository);

        let dir = tempfile::tempdir().unwrap();
        let repo = test_support::init(dir.path());
        let tip = test_support::commit(&repo, &[("specifications/a.md", "# A\n\nalpha")]);
        let facts = git_facts(dir.path());
        let head = repo.head().unwrap().shorthand().unwrap().to_string();
        assert_eq!(facts.branch(), Some(head.as_str()));
        assert_eq!(facts.root_tip(), Some(tip.to_string().as_str()));

        repo.set_head_detached(tip).unwrap();
        assert_eq!(git_facts(dir.path()).branch(), None);
    }

    // EMB-FR-LGLI, EMB-FR-FALJ: the root layer reads the specifications of the
    // tip commit from Git objects, classified with the tree's own
    // assignments, under project-relative paths.
    #[test]
    fn root_chunks_come_from_the_tip_commit_without_a_checkout() {
        let dir = tempfile::tempdir().unwrap();
        let repo = test_support::init(dir.path());
        let tip = test_support::commit(
            &repo,
            &[
                ("proj/specifications/a.md", "# A\n\nalpha\n\n# B\n\nbeta"),
                ("proj/docs/plain.md", "# Not a spec"),
                ("proj/notes/assigned.md", "# Assigned"),
                (
                    "proj/.synthesis/library.toml",
                    "[assignments.\"notes/assigned.md\"]\ntype = \"spec\"\nscope = \"file\"\n",
                ),
                ("proj/.synthesis/drafts/x/files/specifications/d.md", "# Draft"),
                ("other/specifications/z.md", "# Outside the project"),
            ],
        );
        // The working copy changes; the root layer does not see it.
        std::fs::write(dir.path().join("proj/specifications/a.md"), "# Changed").unwrap();

        let chunks = root_chunks(&repo, &tip.to_string(), "proj").unwrap().chunks;
        let ids: Vec<&str> = chunks.keys().map(String::as_str).collect();
        assert_eq!(ids, ["notes/assigned.md#0", "specifications/a.md#0", "specifications/a.md#1"]);
        assert_eq!(
            chunks["specifications/a.md#1"].text,
            bm25_index::chunk_text("# A\n\nalpha\n\n# B\n\nbeta")[1]
        );
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::path::Path;

    use git2::{Oid, Repository};

    /// A new repository whose first branch is `main`, whatever the host's
    /// `init.defaultBranch` says.
    pub fn init(path: &Path) -> Repository {
        let mut options = git2::RepositoryInitOptions::new();
        options.initial_head("main");
        Repository::init_opts(path, &options).unwrap()
    }

    /// Write `files` into the working copy of `repo` and commit them on `HEAD`.
    pub fn commit(repo: &Repository, files: &[(&str, &str)]) -> Oid {
        let workdir = repo.workdir().unwrap().to_path_buf();
        let mut index = repo.index().unwrap();
        for (path, text) in files {
            let full = workdir.join(path);
            std::fs::create_dir_all(full.parent().unwrap()).unwrap();
            std::fs::write(&full, text).unwrap();
            index.add_path(Path::new(path)).unwrap();
        }
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        let parents: Vec<git2::Commit<'_>> =
            repo.head().ok().and_then(|h| h.peel_to_commit().ok()).into_iter().collect();
        let parents: Vec<&git2::Commit<'_>> = parents.iter().collect();
        repo.commit(Some("HEAD"), &sig, &sig, "commit", &tree, &parents).unwrap()
    }
}

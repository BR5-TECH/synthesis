//! The repository set of the active worktree (`GSM-git-submodules.md`).
//!
//! The set holds the main repository and every initialised submodule, at any
//! depth, inside the project content root (GSM-FR-VPAS). Each member is named
//! by its project-relative root path; the main repository has the path `""`
//! (GSM-FR-SFEI). A path goes to the member whose root is the nearest folder
//! above it (GSM-FR-SIWR), and the operations give Git the path relative to the
//! root of that member (GSM-FR-MUTH).
//!
//! The set is read again for each operation (GSM-FR-VFJO). It reads the Git
//! configuration and the `.git` entries only, and makes no network request.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use git2::{Oid, Repository, Tree};
use serde::Serialize;

use super::{open_repo, project_prefix, resolve_branch_commit, to_forward, to_project_rel};
use crate::scanning::repos::{nearest_root, relative_to};

/// The mode of a submodule entry (a gitlink) in a tree or an index.
pub const GITLINK_MODE: u32 = 0o160000;

/// One repository of the set (GSM-FR-VPAS).
pub struct Member {
    /// Project-relative root; `""` for the main repository.
    pub path: String,
    /// The last segment of `path`; the content root folder name for the main
    /// repository.
    pub name: String,
    /// The path of the parent member; `None` for the main repository.
    pub parent: Option<String>,
    /// The root of this member relative to the working directory of its
    /// parent, which is where the parent records the pointer.
    pub path_in_parent: Option<String>,
    /// How many members are above this one: 0 for the main repository.
    pub depth: usize,
    pub repo: Repository,
    /// Where the project root sits inside this repository's working directory.
    /// Only the main repository of a co-located project has one.
    pub prefix: String,
}

/// The wire form of a [`Member`] (`CHC-changes.md` `RepositoryInfo`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepositoryInfo {
    pub path: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head_id: Option<String>,
}

fn join(parent: &str, child: &str) -> String {
    match (parent.is_empty(), child.is_empty()) {
        (true, _) => child.to_string(),
        (_, true) => parent.to_string(),
        _ => format!("{parent}/{child}"),
    }
}

impl Member {
    pub fn is_main(&self) -> bool {
        self.parent.is_none()
    }

    /// The repository-relative path of the project-relative `path`
    /// (GSM-FR-MUTH).
    pub fn to_repo(&self, path: &str) -> String {
        join(&self.prefix, relative_to(path, &self.path))
    }

    /// The project-relative path of the repository-relative `spec`, or `None`
    /// when it is outside the project.
    pub fn to_project(&self, spec: &str) -> Option<String> {
        let inner = to_project_rel(&self.prefix, spec)?;
        Some(join(&self.path, &inner))
    }

    /// The current branch, or `None` when `HEAD` is detached or unborn.
    pub fn branch(&self) -> Option<String> {
        if self.repo.head_detached().unwrap_or(false) {
            return None;
        }
        self.repo.head().ok()?.shorthand().ok().map(str::to_string)
    }

    /// The commit at `HEAD`, or `None` when the branch has no commit.
    pub fn head_id(&self) -> Option<Oid> {
        self.repo.head().ok()?.peel_to_commit().ok().map(|c| c.id())
    }

    pub fn info(&self) -> RepositoryInfo {
        RepositoryInfo {
            path: self.path.clone(),
            name: self.name.clone(),
            parent: self.parent.clone(),
            branch: self.branch(),
            head_id: self.head_id().map(|id| id.to_string()),
        }
    }
}

/// The comparison base of a change set or a diff.
#[derive(Clone, Copy, Debug)]
pub enum Base<'a> {
    /// Each member against its own `HEAD` (`CHC-changes.md` CHC-FR-JFNC).
    Head,
    /// The main repository against the merge-base with this branch, and each
    /// submodule against the commit its parent records there (CHC-FR-AIZV).
    MergeBase(&'a str),
}

/// The base of one member.
pub enum MemberBase<'r> {
    /// The tree to compare against. `None` is an unborn branch, against which
    /// the whole working tree is new.
    Tree(Option<Tree<'r>>),
    /// No base: the parent base records no commit for the submodule, or the
    /// submodule does not hold that commit.
    Missing,
}

impl MemberBase<'_> {
    pub fn tree(&self) -> Option<&Tree<'_>> {
        match self {
            MemberBase::Tree(t) => t.as_ref(),
            MemberBase::Missing => None,
        }
    }
}

/// The bases of every member, and the merge-base commit of the main
/// repository in Branch mode.
pub struct Bases<'r> {
    pub of: Vec<MemberBase<'r>>,
    pub merge_base: Option<Oid>,
}

/// GSM-FR-VPAS: the main repository and its initialised submodules.
pub struct RepoSet {
    /// The main repository first, then the submodules in path order. A parent
    /// is always before its children.
    pub members: Vec<Member>,
    /// GSM-FR-FRLZ: initialised submodules that could not be opened, with the
    /// cause. The callers log them.
    pub unopened: Vec<(String, String)>,
}

impl RepoSet {
    /// GSM-FR-VPAS / GSM-FR-VFJO: read the set of the content root `root`.
    pub fn open(root: &Path) -> Result<RepoSet, String> {
        let repo = open_repo(root)?;
        let prefix = project_prefix(&repo, root);
        let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut members = vec![Member {
            path: String::new(),
            name,
            parent: None,
            path_in_parent: None,
            depth: 0,
            repo,
            prefix,
        }];
        let mut unopened = Vec::new();
        let mut next = 0;
        while next < members.len() {
            let found = children_of(&members[next], &mut unopened);
            let (parent, depth) = (members[next].path.clone(), members[next].depth + 1);
            for (path, path_in_parent, repo) in found {
                let name = path.rsplit('/').next().unwrap_or(&path).to_string();
                members.push(Member {
                    path,
                    name,
                    parent: Some(parent.clone()),
                    path_in_parent: Some(path_in_parent),
                    depth,
                    repo,
                    prefix: String::new(),
                });
            }
            next += 1;
        }
        // The main repository has the empty path, so it sorts first, and a
        // parent path sorts before every path below it.
        members.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(RepoSet { members, unopened })
    }

    pub fn main(&self) -> &Member {
        &self.members[0]
    }

    pub fn has_submodules(&self) -> bool {
        self.members.len() > 1
    }

    fn roots(&self) -> BTreeSet<String> {
        self.members.iter().filter(|m| !m.is_main()).map(|m| m.path.clone()).collect()
    }

    /// The index of the member with the root `path`, if there is one.
    pub fn index_of(&self, path: &str) -> Option<usize> {
        self.members.iter().position(|m| m.path == path)
    }

    /// GSM-FR-SIWR: the index of the member that owns `path`. The root path of
    /// a submodule is owned by its parent.
    pub fn owner_index(&self, path: &str) -> usize {
        let root = nearest_root(path, &self.roots());
        self.index_of(&root).unwrap_or(0)
    }

    pub fn owner(&self, path: &str) -> &Member {
        &self.members[self.owner_index(path)]
    }

    /// The submodule whose root is `path`, when `path` is a pointer path.
    pub fn submodule_at(&self, path: &str) -> Option<&Member> {
        self.members.iter().find(|m| !m.is_main() && m.path == path)
    }

    pub fn parent_of(&self, member: &Member) -> Option<&Member> {
        member.parent.as_deref().and_then(|p| self.index_of(p)).map(|i| &self.members[i])
    }

    pub fn infos(&self) -> Vec<RepositoryInfo> {
        self.members.iter().map(Member::info).collect()
    }

    /// The bases of every member for `base`, in member order.
    pub fn bases(&self, base: Base<'_>) -> Result<Bases<'_>, String> {
        let mut of: Vec<MemberBase<'_>> = Vec::with_capacity(self.members.len());
        let mut merge_base = None;
        let mut position: BTreeMap<&str, usize> = BTreeMap::new();
        for (i, member) in self.members.iter().enumerate() {
            position.insert(member.path.as_str(), i);
            let resolved = match (base, member.is_main()) {
                (Base::Head, _) => MemberBase::Tree(head_tree(&member.repo)),
                (Base::MergeBase(target), true) => {
                    let (oid, tree) = main_merge_base(&member.repo, target)?;
                    merge_base = Some(oid);
                    MemberBase::Tree(Some(tree))
                }
                (Base::MergeBase(_), false) => {
                    let parent = member.parent.as_deref().and_then(|p| position.get(p)).copied();
                    let recorded = parent.and_then(|p| {
                        let tree = of[p].tree()?;
                        recorded_commit(tree, member.path_in_parent.as_deref()?)
                    });
                    match recorded.and_then(|oid| member.repo.find_commit(oid).ok()) {
                        Some(commit) => match commit.tree() {
                            Ok(tree) => MemberBase::Tree(Some(tree)),
                            Err(_) => MemberBase::Missing,
                        },
                        None => MemberBase::Missing,
                    }
                }
            };
            of.push(resolved);
        }
        Ok(Bases { of, merge_base })
    }

    /// GSM-FR-FCOF: the commit that the parent base records for the submodule
    /// at index `i`, from the bases of [`RepoSet::bases`].
    pub fn recorded_pointer(&self, bases: &Bases<'_>, i: usize) -> Option<Oid> {
        let member = &self.members[i];
        let parent = self.index_of(member.parent.as_deref()?)?;
        recorded_commit(bases.of[parent].tree()?, member.path_in_parent.as_deref()?)
    }

    /// The indexes of the members, deepest first and the main repository last
    /// (GSM-FR-KWWZ, GSM-FR-REIT).
    pub fn deepest_first(&self) -> Vec<usize> {
        let mut order: Vec<usize> = (0..self.members.len()).collect();
        order.sort_by(|a, b| {
            let (ma, mb) = (&self.members[*a], &self.members[*b]);
            mb.depth.cmp(&ma.depth).then_with(|| ma.path.cmp(&mb.path))
        });
        order
    }
}

/// The initialised submodules of `member` inside the project, as
/// `(project path, path in the parent, repository)`.
fn children_of(
    member: &Member,
    unopened: &mut Vec<(String, String)>,
) -> Vec<(String, String, Repository)> {
    let Ok(submodules) = member.repo.submodules() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for submodule in submodules {
        let in_parent = to_forward(submodule.path());
        let Some(path) = member.to_project(&in_parent) else {
            continue;
        };
        if path.is_empty() {
            continue;
        }
        match submodule.open() {
            Ok(repo) => out.push((path, in_parent, repo)),
            // GSM-FR-FRLZ: a submodule with no checkout is not a member, and
            // says nothing. One with a checkout that does not open is reported.
            Err(e) => {
                let name = submodule.name().unwrap_or_default();
                let status = member.repo.submodule_status(name, git2::SubmoduleIgnore::None);
                let checked_out = status.is_ok_and(|s| {
                    s.contains(git2::SubmoduleStatus::IN_WD)
                        && !s.contains(git2::SubmoduleStatus::WD_UNINITIALIZED)
                });
                if checked_out {
                    unopened.push((path, e.message().to_string()));
                }
            }
        }
    }
    out
}

/// The tree at `HEAD`, or `None` on an unborn branch.
pub fn head_tree(repo: &Repository) -> Option<Tree<'_>> {
    repo.head().ok()?.peel_to_commit().ok()?.tree().ok()
}

/// The merge-base of `HEAD` and `target`, and its tree (CHC-FR-04, CHC-FR-05,
/// CHC-FR-15).
fn main_merge_base<'r>(repo: &'r Repository, target: &str) -> Result<(Oid, Tree<'r>), String> {
    let target = resolve_branch_commit(repo, target)?;
    let head = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .map(|c| c.id())
        .ok_or_else(|| super::ERR_NO_MERGE_BASE.to_string())?;
    let base = repo.merge_base(head, target).map_err(|_| super::ERR_NO_MERGE_BASE.to_string())?;
    let tree = repo
        .find_commit(base)
        .and_then(|c| c.tree())
        .map_err(|_| super::ERR_NO_MERGE_BASE.to_string())?;
    Ok((base, tree))
}

/// The commit that `tree` records at `spec` as a submodule entry.
pub fn recorded_commit(tree: &Tree<'_>, spec: &str) -> Option<Oid> {
    let entry = tree.get_path(Path::new(spec)).ok()?;
    (entry.filemode() as u32 == GITLINK_MODE).then(|| entry.id())
}

#[cfg(test)]
pub mod tests;

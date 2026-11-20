//! The mounted project index of one content root (`ASC-artifact-scanning.md`
//! ASC-FR-HPUY, ASC-FR-TPJO, ASC-FR-17) and the candidate list it serves.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::{ArtifactType, NodeKind, ProjectIndex, TreeNode};

/// The ten built-in types, in the order of ASC-FR-02.
pub const EVERY_TYPE: [ArtifactType; 10] = [
    ArtifactType::Skill,
    ArtifactType::Agent,
    ArtifactType::Prompt,
    ArtifactType::Spec,
    ArtifactType::Flow,
    ArtifactType::Architecture,
    ArtifactType::Instructions,
    ArtifactType::Scenario,
    ArtifactType::Scratchpad,
    ArtifactType::Doc,
];

/// One entry of the enumerable candidate list (ASC-FR-17): a `file` node of the
/// scan, carrying its stable id, project-relative path, and resolved
/// classification.
///
/// This is the set `../core/SCC-search.md` matches against (SCC-FR-03), which is
/// why search inherits the exclusions of ASC-FR-09 without restating any of
/// them: a path that never became a `file` node is never a candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// The node id (ASC-FR-13) — its project-relative path.
    pub id: String,
    /// Basename.
    pub name: String,
    /// Project-relative path (forward-slash separated).
    pub path: String,
    /// Position in the enumeration, from 0. The ordering key of SCC-FR-10.
    pub ordinal: u64,
    /// The scan's resolved type, or `None` for an unclassified file.
    pub artifact_type: Option<ArtifactType>,
}

/// ASC-FR-17: enumerate a scanned tree's `file` nodes in a deterministic order.
///
/// The order is the tree's own — folders before files, each alphabetical (the
/// `BTreeMap`/`BTreeSet` ordering `convert_dir` builds on) — so an unchanged
/// tree enumerates identically every time, which is what makes SCC-FR-10's
/// ordinals stable across two runs of the same query.
pub fn candidate_files(tree: &TreeNode) -> Vec<Candidate> {
    let mut out = Vec::new();
    collect_candidates(tree, &mut out);
    out
}

fn collect_candidates(node: &TreeNode, out: &mut Vec<Candidate>) {
    if node.node_kind == NodeKind::File {
        out.push(Candidate {
            id: node.id.clone(),
            name: node.name.clone(),
            path: node.path.clone(),
            ordinal: out.len() as u64,
            artifact_type: node.artifact_type,
        });
    }
    if let Some(children) = &node.children {
        for child in children {
            collect_candidates(child, out);
        }
    }
}

/// The mounted candidate list (ASC-FR-17 / SCC-FR-04).
///
/// It belongs to the project's content root and lives exactly as long as it: a
/// project close (ASC-FR-14) and a worktree change (ASC-FR-16) both [`clear`] it,
/// and the watcher [`invalidate`]s it on a structural change so the next
/// enumeration sees the current tree (ASC-FR-10 / ASC-FR-15). Between those, the
/// list is served from memory — which is what keeps a burst of searches from
/// re-walking the tree once per keystroke.
///
/// [`clear`]: CandidateStore::clear
/// [`invalidate`]: CandidateStore::invalidate
///
/// ASC-FR-HPUY: what is mounted is the whole project index, and the candidate
/// list is one view of it. `build` serialises the builds, so a command that
/// arrives while the index builds waits for that build and then reads its
/// result (ASC-FR-TPJO).
#[derive(Default)]
pub struct CandidateStore {
    inner: Mutex<Option<Mounted>>,
    build: Mutex<()>,
    /// Bumped by every teardown, so a build that was walking when the store
    /// was cleared is handed to its caller but never mounted: the clear is
    /// not undone by a walk of the tree from before it.
    generation: AtomicU64,
}

struct Mounted {
    root: PathBuf,
    /// Shared so a running search holds the index it started from even if the
    /// store is invalidated or torn down under it — a search never observes the
    /// list changing halfway through, and a mid-search project close cannot
    /// leave a worker reading freed state. `None` while an update changes it.
    index: Option<Arc<ProjectIndex>>,
}

impl CandidateStore {
    /// The candidate list for `root`, building it from a scan when nothing is
    /// mounted, when the mounted list belongs to a different root, or when the
    /// watcher has invalidated it. Never fails: an unreadable root scans to an
    /// empty tree (ASC-FR-11), which enumerates to no candidates.
    pub fn candidates(&self, root: &crate::fs::RootFs) -> Arc<Vec<Candidate>> {
        self.index(root).candidates()
    }

    /// ASC-FR-HPUY: the project index of `root`, built when nothing is
    /// mounted, when the mounted index belongs to a different root, or when
    /// the watcher has invalidated it.
    pub fn index(&self, root: &crate::fs::RootFs) -> Arc<ProjectIndex> {
        if let Some(index) = self.mounted(root) {
            return index;
        }
        // One build at a time. The `inner` lock is not held across the build,
        // so a reader of another root, and the teardown, never wait for it.
        let _build = self.build.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(index) = self.mounted(root) {
            return index;
        }
        let started = self.generation.load(Ordering::SeqCst);
        let index = Arc::new(ProjectIndex::build(root));
        if let Ok(mut guard) = self.inner.lock() {
            // A teardown during the walk wins: the next read builds again.
            if self.generation.load(Ordering::SeqCst) == started {
                *guard = Some(Mounted {
                    root: root.path().to_path_buf(),
                    index: Some(Arc::clone(&index)),
                });
            }
        }
        index
    }

    /// The mounted index when it is mounted for `root`.
    pub fn mounted(&self, root: &crate::fs::RootFs) -> Option<Arc<ProjectIndex>> {
        let guard = self.inner.lock().ok()?;
        let mounted = guard.as_ref()?;
        if mounted.root.as_path() != root.path() {
            return None;
        }
        mounted.index.clone()
    }

    /// ASC-FR-UWMI, ASC-FR-JYXO: change the mounted index of `root` with `f`
    /// and return what `f` returns. `None` when no index is mounted for
    /// `root`, which leaves the next read to build one.
    pub fn update<T>(
        &self,
        root: &crate::fs::RootFs,
        f: impl FnOnce(&mut ProjectIndex) -> T,
    ) -> Option<T> {
        let _build = self.build.lock().unwrap_or_else(|e| e.into_inner());
        // Taken out of the store, so a read during the update waits on the
        // build lock and then reads the changed index. The index is copied
        // only when a running search still holds it.
        let mut index = {
            let mut guard = self.inner.lock().ok()?;
            let mounted = guard.as_mut()?;
            if mounted.root.as_path() != root.path() {
                return None;
            }
            mounted.index.take()?
        };
        let out = f(Arc::make_mut(&mut index));
        let mut guard = self.inner.lock().ok()?;
        match guard.as_mut() {
            Some(mounted) if mounted.root.as_path() == root.path() => {
                mounted.index = Some(index);
                Some(out)
            }
            _ => None,
        }
    }

    /// The tree changed in a way the caller cannot apply, so the mounted index
    /// no longer describes it. The next read rebuilds; searches already running
    /// keep the index they started from.
    pub fn invalidate(&self) {
        self.clear();
    }

    /// ASC-FR-14 / ASC-FR-16: tear the list down with the scan, on project close
    /// and on a change of active worktree.
    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            self.generation.fetch_add(1, Ordering::SeqCst);
            *guard = None;
        }
    }

    /// Whether a list is currently mounted. Test-facing: the teardown and
    /// invalidation claims of ASC-FR-17 have no other observable.
    pub fn is_mounted(&self) -> bool {
        self.inner.lock().map(|guard| guard.is_some()).unwrap_or(false)
    }
}

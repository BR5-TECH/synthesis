//! One evaluation of the layers against the store (EMB-FR-BZZT, EMB-FR-EAEA,
//! EMB-FR-DQVX, EMB-FR-USBA, EMB-FR-HRYV, EMB-FR-QNWF, EMB-FR-RXCL).
//!
//! Pure over its inputs and the store: the worker hands it the chunks of the
//! published BM25 snapshot and the Git facts, and it brings each layer in line
//! by content hash. It embeds only the hashes the store has no vector for, in
//! small batches, and writes the whole pass in one transaction.

use super::layers::{self, Chunks, Desired, GitFacts, Want};
use super::store::{Entries, LayerChange, LayerKind, Store, View};

/// EMB-FR-JLRR: how many chunks one batch embeds. A query embedding waits for
/// at most one batch.
pub const BATCH_SIZE: usize = 16;

/// EMB-FR-HRYV: what a Reindex removes before it embeds again.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReindexScope {
    CurrentBranch,
    WholeTrace,
}

/// The input of one evaluation.
pub struct Evaluation<'a> {
    pub facts: &'a GitFacts,
    /// The specification chunks of the working copy (EMB-FR-DCXI).
    pub working: &'a Chunks,
    /// The document chunks of the collection (EMB-FR-PYCS).
    pub documents: &'a Chunks,
    /// EMB-FR-BZZT: an incremental pass leaves the root layer alone.
    pub skip_root: bool,
    pub drop: Option<ReindexScope>,
    /// Reads the root layer's chunks from Git (EMB-FR-LGLI).
    pub root_reader: &'a dyn Fn(&str, &str) -> Result<layers::RootRead, String>,
}

/// What an evaluation leaves for search.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    pub spec_view: View,
    /// EMB-FR-LGLI: the tip commit id the root layer records.
    pub root_tip: Option<String>,
    /// EMB-FR-OLDK: the root files that gave no chunks, with the reason.
    pub skipped: Vec<(String, &'static str)>,
    /// The content hashes of the working copy and the documents this pass
    /// saw, so a later pass that sees the same can skip (EMB-FR-BZZT).
    pub fingerprint: String,
    pub documents_layer: Option<i64>,
    pub spec_chunks: usize,
    pub document_chunks: usize,
    pub compared: usize,
    pub embedded: usize,
    pub removed: usize,
}

/// Embeds a batch of texts, one vector per text.
pub type EmbedFn<'a> = dyn FnMut(&[String]) -> Result<Vec<Vec<f32>>, String> + 'a;

/// The callbacks of a running job.
pub struct Job<'a> {
    pub embed: &'a mut EmbedFn<'a>,
    /// True when the job must stop (EMB-FR-SWRV).
    pub cancelled: &'a dyn Fn() -> bool,
    /// Called with (embedded so far, total to embed).
    pub progress: &'a mut dyn FnMut(u64, u64),
}

/// Where a layer of the view comes from: a stored layer this pass leaves
/// alone, or the change at this index of the commit.
#[derive(Clone, Copy)]
enum LayerRef {
    Stored(i64),
    Planned(usize),
}

/// One layer change of the commit, with the entries it must end with.
struct Plan {
    layer_id: Option<i64>,
    kind: LayerKind,
    branch: Option<String>,
    tip: Option<String>,
    root_tip: Option<String>,
    desired: Desired,
}

impl Plan {
    fn entries(&self) -> Entries {
        self.desired
            .iter()
            .map(|(id, want)| {
                let hash = match want {
                    Want::Chunk(chunk) => Some(chunk.hash.clone()),
                    Want::Deleted => None,
                };
                (id.clone(), hash)
            })
            .collect()
    }
}

/// Bring the layers in line with the evaluation input.
///
/// EMB-FR-EAEA: the pass plans every layer first, embeds what the plan needs,
/// and writes the whole plan — the Reindex drop included — in one
/// transaction, so a pass that fails or is cancelled leaves the store as it
/// was.
pub fn evaluate(
    store: &mut Store,
    input: &Evaluation<'_>,
    job: &mut Job<'_>,
) -> Result<Outcome, String> {
    let mut outcome = Outcome::default();
    let stored_layers = store.layers()?;
    // EMB-FR-HRYV, EMB-FR-QNWF: the layers a Reindex embeds again.
    let dropped: Vec<i64> = match input.drop {
        Some(scope) => reindex_ids(&stored_layers, input.facts, scope),
        None => Vec::new(),
    };
    let find = |kind: LayerKind, branch: Option<&str>| {
        stored_layers
            .iter()
            .find(|layer| {
                !dropped.contains(&layer.id)
                    && layer.kind == kind
                    && (kind != LayerKind::Branch || layer.branch.as_deref() == branch)
            })
            .cloned()
    };
    let mut plans: Vec<Plan> = Vec::new();

    // EMB-FR-LGLI / EMB-FR-DQVX: the root layer, read again when the tip moved.
    let mut root_ref: Option<LayerRef> = None;
    let mut root_entries = Entries::new();
    if let GitFacts::Repository {
        prefix,
        root_branch: Some(root_branch),
        root_tip: Some(tip),
        ..
    } = input.facts
    {
        let stored = find(LayerKind::Root, None);
        let current = stored.as_ref().is_some_and(|layer| {
            layer.tip.as_deref() == Some(tip) && layer.branch.as_deref() == Some(root_branch)
        });
        match stored {
            Some(layer) if current || input.skip_root => {
                root_entries = store.entries(layer.id)?;
                root_ref = Some(LayerRef::Stored(layer.id));
            }
            stored => {
                let read = (input.root_reader)(tip, prefix)?;
                outcome.skipped.extend(read.skipped);
                let plan = Plan {
                    layer_id: stored.map(|layer| layer.id),
                    kind: LayerKind::Root,
                    branch: Some(root_branch.clone()),
                    tip: Some(tip.clone()),
                    root_tip: None,
                    desired: layers::whole(&read.chunks),
                };
                root_entries = plan.entries();
                root_ref = Some(LayerRef::Planned(plans.len()));
                plans.push(plan);
            }
        }
        outcome.root_tip = Some(match &root_ref {
            Some(LayerRef::Stored(id)) => stored_layers
                .iter()
                .find(|layer| layer.id == *id)
                .and_then(|layer| layer.tip.clone())
                .unwrap_or_else(|| tip.clone()),
            _ => tip.clone(),
        });
    }
    check(job)?;

    // EMB-FR-DCXI / EMB-FR-NTMA / EMB-FR-MGML: the layer of the working copy.
    let working_plan = match input.facts {
        GitFacts::NoRepository => Some(Plan {
            layer_id: find(LayerKind::Worktree, None).map(|layer| layer.id),
            kind: LayerKind::Worktree,
            branch: None,
            tip: None,
            root_tip: None,
            desired: layers::whole(input.working),
        }),
        GitFacts::Repository { head: layers::Head::Detached, .. } => None,
        GitFacts::Repository { head: layers::Head::Branch(name), root_tip, .. } => Some(Plan {
            layer_id: find(LayerKind::Branch, Some(name)).map(|layer| layer.id),
            kind: LayerKind::Branch,
            branch: Some(name.clone()),
            tip: None,
            root_tip: root_tip.clone(),
            desired: layers::branch_difference(input.working, &root_entries),
        }),
    };
    let branch_ref = working_plan.map(|plan| {
        plans.push(plan);
        LayerRef::Planned(plans.len() - 1)
    });

    // EMB-FR-PYCS: the one documents layer.
    let documents_ref = LayerRef::Planned(plans.len());
    plans.push(Plan {
        layer_id: find(LayerKind::Documents, None).map(|layer| layer.id),
        kind: LayerKind::Documents,
        branch: None,
        tip: None,
        root_tip: None,
        desired: layers::whole(input.documents),
    });

    // EMB-FR-USBA: a branch that no longer exists loses its layer. The active
    // branch keeps its layer even while it is unborn, and so is not yet in the
    // list; an empty list means the list could not be read, and deletes nothing.
    let mut deleted = dropped.clone();
    if let GitFacts::Repository { branches, .. } = input.facts {
        let active = input.facts.branch();
        deleted.extend(
            stored_layers
                .iter()
                .filter(|layer| {
                    layer.kind == LayerKind::Branch
                        && !dropped.contains(&layer.id)
                        && !branches.is_empty()
                        && layer.branch.as_deref() != active
                        && layer.branch.as_ref().is_some_and(|name| !branches.contains(name))
                })
                .map(|layer| layer.id),
        );
    }

    // EMB-FR-KNBG / EMB-FR-RXCL: embed each hash no kept layer has a vector
    // for. A vector that only the dropped layers refer to is embedded again.
    let available = store.hashes_outside(&deleted)?;
    let mut texts_by_hash: std::collections::BTreeMap<&String, &String> = Default::default();
    for plan in &plans {
        outcome.compared += plan.desired.len();
        for want in plan.desired.values() {
            if let Want::Chunk(chunk) = want {
                if !available.contains(&chunk.hash) {
                    texts_by_hash.entry(&chunk.hash).or_insert(&chunk.text);
                }
            }
        }
    }
    let missing: Vec<&String> = texts_by_hash.keys().copied().collect();
    let goal = missing.len() as u64;
    let mut vectors: Vec<(String, Vec<f32>)> = Vec::with_capacity(missing.len());
    for batch in missing.chunks(BATCH_SIZE) {
        check(job)?;
        let texts: Vec<String> = batch.iter().map(|hash| texts_by_hash[*hash].clone()).collect();
        let embedded = (job.embed)(&texts)?;
        if embedded.len() != texts.len() {
            return Err("the model returned a wrong number of vectors".to_string());
        }
        vectors.extend(batch.iter().map(|hash| (*hash).clone()).zip(embedded));
        (job.progress)(vectors.len() as u64, goal);
    }
    check(job)?;

    let entries: Vec<Entries> = plans.iter().map(Plan::entries).collect();
    let changes: Vec<LayerChange<'_>> = plans
        .iter()
        .zip(&entries)
        .map(|(plan, entries)| LayerChange {
            layer_id: plan.layer_id,
            kind: plan.kind,
            branch: plan.branch.as_deref(),
            tip: plan.tip.as_deref(),
            root_tip: plan.root_tip.as_deref(),
            entries,
        })
        .collect();
    let (ids, removed) = store.commit(&deleted, &changes, &vectors)?;
    let resolve = |layer: LayerRef| match layer {
        LayerRef::Stored(id) => id,
        LayerRef::Planned(index) => ids[index],
    };
    outcome.embedded = vectors.len();
    outcome.removed = removed;
    outcome.spec_view = View { root: root_ref.map(resolve), branch: branch_ref.map(resolve) };
    let documents_layer = resolve(documents_ref);
    outcome.documents_layer = Some(documents_layer);
    outcome.spec_chunks = store.view_entries(outcome.spec_view)?.len();
    outcome.document_chunks = store.entries(documents_layer)?.len();
    Ok(outcome)
}

/// EMB-FR-HRYV, EMB-FR-QNWF: the layers a Reindex embeds again — the active
/// branch layer (or the one layer outside Git) and the documents layer, and
/// the root layer too for the whole trace.
fn reindex_ids(
    layers: &[super::store::LayerRow],
    facts: &GitFacts,
    scope: ReindexScope,
) -> Vec<i64> {
    let branch = facts.branch();
    layers
        .iter()
        .filter(|layer| match layer.kind {
            LayerKind::Documents | LayerKind::Worktree => true,
            LayerKind::Branch => branch.is_some() && layer.branch.as_deref() == branch,
            LayerKind::Root => scope == ReindexScope::WholeTrace,
        })
        .map(|layer| layer.id)
        .collect()
}

fn check(job: &Job<'_>) -> Result<(), String> {
    if (job.cancelled)() {
        Err(CANCELLED.to_string())
    } else {
        Ok(())
    }
}

/// The error of a job that was cancelled (EMB-FR-SWRV). It is not a failure.
pub const CANCELLED: &str = "cancelled";

#[cfg(test)]
mod tests;

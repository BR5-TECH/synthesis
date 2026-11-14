//! The typed form of a valid Flow document (`FGV-flow-graph-validation.md`
//! FGV-FR-01), and the "waits for" graph the plan's `blocks` edges make.
//!
//! The validator in the parent module reads raw JSON, so that it can name every
//! violation. The modules that read a plan after it is valid — the delivery
//! state (`FDL-flow-delivery.md`), the overlap analysis
//! (`FOA-flow-overlap-analysis.md`), and the delivery modules that read and
//! write a plan (`DSR-delivery-source.md`, `GDP-github-delivery-plan.md`) — read
//! this typed form instead.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

/// The five kinds of a plan item (FGV-FR-01).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Milestone,
    Epic,
    Feature,
    Task,
    Defect,
}

impl Kind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "milestone" => Self::Milestone,
            "epic" => Self::Epic,
            "feature" => Self::Feature,
            "task" => Self::Task,
            "defect" => Self::Defect,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Milestone => "milestone",
            Self::Epic => "epic",
            Self::Feature => "feature",
            Self::Task => "task",
            Self::Defect => "defect",
        }
    }

    /// A container kind holds other items; a leaf kind does not (FGV-FR-01).
    pub fn is_container(self) -> bool {
        !matches!(self, Self::Task | Self::Defect)
    }

    /// FGV-FR-YHFX: the rank of the kind. `milestone` is the highest, then
    /// `epic`, then `feature`, and then `task` and `defect` together.
    pub fn rank(self) -> u8 {
        match self {
            Self::Milestone => 3,
            Self::Epic => 2,
            Self::Feature => 1,
            Self::Task | Self::Defect => 0,
        }
    }

    /// FGV-FR-YHFX: whether this kind may sit in `container`. `None` is the top
    /// level. A `milestone` sits at the top level alone. Every other kind sits
    /// at the top level or in a container of a higher rank.
    pub fn may_sit_in(self, container: Option<Kind>) -> bool {
        match (self, container) {
            (Self::Milestone, None) => true,
            (Self::Milestone, Some(_)) => false,
            (_, None) => true,
            (_, Some(c)) => c.is_container() && c.rank() > self.rank(),
        }
    }
}

/// One plan item. Field order is the order the document writes them in.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub kind: Kind,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub draft_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footprint: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EdgeType {
    Blocks,
    Relates,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub from: String,
    pub to: String,
    #[serde(rename = "type")]
    pub edge_type: EdgeType,
}

/// A valid Flow document, version 3. It holds the plan alone: no position, no
/// size, and no tracker section (FGV-FR-01).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct FlowDocument {
    #[serde(default)]
    pub version: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub references: Option<Vec<String>>,
    #[serde(default)]
    pub items: Vec<Item>,
    #[serde(default)]
    pub edges: Vec<Edge>,
}

/// Validate `body` and return its typed form, or the report that refused it.
/// An empty body is the empty plan (FGV-FR-05).
pub fn parse_valid(body: &str) -> Result<FlowDocument, super::ValidationReport> {
    let report = super::validate(body);
    if !report.valid {
        return Err(report);
    }
    if body.trim().is_empty() {
        return Ok(FlowDocument { version: super::FLOW_VERSION, ..FlowDocument::default() });
    }
    serde_json::from_str(body).map_err(|e| {
        // The validator accepted a body serde cannot read: a defect of this
        // module rather than of the document, reported in the same shape.
        super::ValidationReport::single(super::FlowViolation::doc(
            "shape",
            format!("the document could not be read as a Flow: {e}"),
        ))
    })
}

/// The hierarchy of a document, by item id.
pub struct Hierarchy<'a> {
    parent: HashMap<&'a str, &'a str>,
    children: HashMap<&'a str, Vec<&'a str>>,
    kind: HashMap<&'a str, Kind>,
    order: Vec<&'a str>,
}

impl<'a> Hierarchy<'a> {
    pub fn new(items: &'a [Item]) -> Self {
        Self::from_parts(
            items.iter().map(|i| (i.id.as_str(), i.kind, i.parent_id.as_deref())).collect(),
        )
    }

    /// Build from `(id, kind, parent)` in document order. A parent that names
    /// no listed item is left out, and the first of two items sharing an id wins.
    pub fn from_parts(parts: Vec<(&'a str, Kind, Option<&'a str>)>) -> Self {
        let mut parent = HashMap::new();
        let mut children: HashMap<&str, Vec<&str>> = HashMap::new();
        let mut kind = HashMap::new();
        let mut order = Vec::new();
        for (id, k, _) in &parts {
            if !kind.contains_key(id) {
                kind.insert(*id, *k);
                order.push(*id);
            }
        }
        let mut placed = HashSet::new();
        for (id, _, p) in &parts {
            if let Some(p) = p {
                if kind.contains_key(p) && placed.insert(*id) {
                    parent.insert(*id, *p);
                    children.entry(*p).or_default().push(*id);
                }
            }
        }
        Self { parent, children, kind, order }
    }

    pub fn kind(&self, id: &str) -> Option<Kind> {
        self.kind.get(id).copied()
    }

    pub fn parent(&self, id: &str) -> Option<&'a str> {
        self.parent.get(id).copied()
    }

    pub fn children(&self, id: &str) -> &[&'a str] {
        self.children.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Ancestors from the nearest outwards. Bounded by the item count, so a
    /// containment cycle cannot make it run for ever.
    pub fn ancestors(&self, id: &str) -> Vec<&'a str> {
        let mut out = Vec::new();
        let mut at = self.parent(id);
        while let Some(p) = at {
            if out.len() > self.order.len() || out.contains(&p) || p == id {
                break;
            }
            out.push(p);
            at = self.parent(p);
        }
        out
    }

    pub fn is_ancestor(&self, ancestor: &str, of: &str) -> bool {
        self.ancestors(of).contains(&ancestor)
    }

    /// Every leaf at or under `id`, in document order.
    pub fn leaves_under(&self, id: &str) -> Vec<&'a str> {
        let Some(start) = self.order.iter().copied().find(|o| *o == id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let mut stack = vec![start];
        while let Some(at) = stack.pop() {
            if !seen.insert(at) {
                continue;
            }
            match self.kind(at) {
                Some(k) if !k.is_container() => out.push(at),
                Some(_) => stack.extend(self.children(at).iter().copied()),
                None => {}
            }
        }
        let rank: HashMap<&str, usize> =
            self.order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        out.sort_by_key(|id| rank.get(id).copied().unwrap_or(usize::MAX));
        out
    }

    /// Item ids in document order.
    pub fn order(&self) -> &[&'a str] {
        &self.order
    }
}

/// The "waits for" graph of FGV-FR-PMTY. Each item is two vertices: its
/// **waiter**, which waits for what blocks the item or any of its ancestors,
/// and its **completion**, which an item reaches when its waits are over and,
/// for a container, when every member has. A `blocks` edge `F → T` is the arc
/// `waiter(T) → completion(F)`. A cycle in this graph is a leaf that waits,
/// directly or through other leaves, for itself.
pub struct WaitGraph<'a> {
    index: HashMap<&'a str, usize>,
    /// Arcs, each tagged with the `blocks` edge id that made it, if any.
    arcs: Vec<Vec<(usize, Option<&'a str>)>>,
}

impl<'a> WaitGraph<'a> {
    /// `blocks` lists the `(edge id, from, to)` of every `blocks` edge whose
    /// endpoints are items of `hierarchy`.
    pub fn new(hierarchy: &Hierarchy<'a>, blocks: &[(&'a str, &'a str, &'a str)]) -> Self {
        let mut index = HashMap::new();
        for (i, id) in hierarchy.order.iter().enumerate() {
            index.entry(*id).or_insert(i);
        }
        let n = hierarchy.order.len();
        let waiter = |i: usize| i;
        let completion = |i: usize| n + i;
        let mut arcs: Vec<Vec<(usize, Option<&str>)>> = vec![Vec::new(); 2 * n];
        for (id, &i) in &index {
            if let Some(p) = hierarchy.parent(id).and_then(|p| index.get(p)) {
                arcs[waiter(i)].push((waiter(*p), None));
            }
            // Every item completes only once its own waits are over. For a
            // container with leaves this is already implied through them; for an
            // empty container it is what makes a ring of them a deadlock, since
            // an empty container is never done (FDL-FR-JYKC).
            arcs[completion(i)].push((waiter(i), None));
            if hierarchy.kind(id).is_some_and(Kind::is_container) {
                for c in hierarchy.children(id) {
                    if let Some(&ci) = index.get(c) {
                        arcs[completion(i)].push((completion(ci), None));
                    }
                }
            }
        }
        for (edge, from, to) in blocks {
            if let (Some(&f), Some(&t)) = (index.get(from), index.get(to)) {
                arcs[waiter(t)].push((completion(f), Some(*edge)));
            }
        }
        Self { index, arcs }
    }

    /// FGV-FR-PMTY: the ids of the `blocks` edges that lie on a cycle, as a set.
    pub fn cycle_edges(&self) -> HashSet<&'a str> {
        let comp = strongly_connected(&self.arcs);
        let mut out = HashSet::new();
        for (v, out_arcs) in self.arcs.iter().enumerate() {
            for (w, edge) in out_arcs {
                if let Some(edge) = edge {
                    if comp[v] == comp[*w] {
                        out.insert(*edge);
                    }
                }
            }
        }
        out
    }

    /// FOA-FR-XOAR: whether `later` waits, directly or transitively, for
    /// `earlier`.
    pub fn waits_for(&self, later: &str, earlier: &str) -> bool {
        let (Some(&l), Some(&e)) = (self.index.get(later), self.index.get(earlier)) else {
            return false;
        };
        let n = self.arcs.len() / 2;
        let target = n + e;
        let mut seen = vec![false; self.arcs.len()];
        let mut stack = vec![l];
        while let Some(v) = stack.pop() {
            if v == target {
                return true;
            }
            if std::mem::replace(&mut seen[v], true) {
                continue;
            }
            stack.extend(self.arcs[v].iter().map(|(w, _)| *w));
        }
        false
    }
}

/// Kosaraju's algorithm, iterative so a deep plan cannot overflow the stack.
/// Returns the component number of every vertex.
fn strongly_connected(arcs: &[Vec<(usize, Option<&str>)>]) -> Vec<usize> {
    let n = arcs.len();
    let mut order = Vec::with_capacity(n);
    let mut seen = vec![false; n];
    for start in 0..n {
        if seen[start] {
            continue;
        }
        seen[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((v, next)) = stack.last_mut() {
            if let Some((w, _)) = arcs[*v].get(*next) {
                *next += 1;
                let w = *w;
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                order.push(*v);
                stack.pop();
            }
        }
    }
    let mut reverse = vec![Vec::new(); n];
    for (v, out) in arcs.iter().enumerate() {
        for (w, _) in out {
            reverse[*w].push(v);
        }
    }
    let mut comp = vec![usize::MAX; n];
    let mut count = 0;
    for &start in order.iter().rev() {
        if comp[start] != usize::MAX {
            continue;
        }
        let mut stack = vec![start];
        comp[start] = count;
        while let Some(v) = stack.pop() {
            for &w in &reverse[v] {
                if comp[w] == usize::MAX {
                    comp[w] = count;
                    stack.push(w);
                }
            }
        }
        count += 1;
    }
    comp
}

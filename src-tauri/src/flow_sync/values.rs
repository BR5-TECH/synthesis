//! Pure helpers of the sync: the values an item holds on the Flow side
//! (FTS-FR-HLVJ), how two values compare (FTS-FR-VRRJ), the kind an issue
//! Type gives (FTS-FR-CJVS), where an item goes (FTS-FR-EGJL, FTS-FR-LKZO),
//! and whether a changed plan is still one (FGV-flow-graph-validation.md).

use std::collections::{HashMap, HashSet};

use crate::flow_validation::model::{
    EdgeType, FlowDocument, Hierarchy, Item, Kind, Position, RefKind, Size, SyncLink, TrackerRef,
    WaitGraph,
};
use crate::project_settings::FlowTrackerSettings;

use super::adapter::IssueKey;

/// The extent of a leaf card, the room a container keeps around its members,
/// and the room its header takes (`src/state/flowDocument/types.ts`).
pub const CARD_W: f64 = 170.0;
pub const CARD_H: f64 = 96.0;
pub const CONTAINER_PADDING: f64 = 16.0;
pub const CONTAINER_HEADER_H: f64 = 40.0;

/// The size a new container of each kind takes (`DEFAULT_CONTAINER_SIZES`).
pub fn default_size(kind: Kind) -> Option<Size> {
    let (width, height) = match kind {
        Kind::Deliverable => (760.0, 480.0),
        Kind::Milestone => (640.0, 380.0),
        Kind::Epic => (520.0, 300.0),
        Kind::Feature => (400.0, 220.0),
        Kind::Task | Kind::Defect => return None,
    };
    Some(Size { width, height })
}

/// The id prefix of each kind (`ITEM_ID_PREFIX`).
fn id_prefix(kind: Kind) -> &'static str {
    match kind {
        Kind::Deliverable => "d",
        Kind::Milestone => "m",
        Kind::Epic => "ep",
        Kind::Feature => "f",
        Kind::Task => "t",
        Kind::Defect => "x",
    }
}

/// A new item id for `kind`, as the Flow tab makes one (`mintId`). An id that
/// a link still names is taken too: that link is the record of an item gone
/// from the Flow (FGV-FR-15), and a second link with its id is not valid.
pub fn mint_item_id(doc: &FlowDocument, kind: Kind) -> String {
    let mut taken: HashSet<&str> = doc.items.iter().map(|i| i.id.as_str()).collect();
    taken.extend(doc.sync.iter().flat_map(|s| s.links.iter()).map(|l| l.item_id.as_str()));
    let same = doc.items.iter().filter(|i| i.kind == kind).count();
    mint(id_prefix(kind), &taken, same + 1)
}

/// A new edge id, as the Flow tab makes one.
pub fn mint_edge_id(doc: &FlowDocument) -> String {
    let taken: HashSet<&str> = doc.edges.iter().map(|e| e.id.as_str()).collect();
    mint("e", &taken, doc.edges.len() + 1)
}

fn mint(prefix: &str, taken: &HashSet<&str>, from: usize) -> String {
    let mut n = from.max(1);
    while taken.contains(format!("{prefix}{n}").as_str()) {
        n += 1;
    }
    format!("{prefix}{n}")
}

/// FTS-FR-LMBE: the issue Type of an issue kind. A milestone has none.
pub fn type_for(settings: &FlowTrackerSettings, kind: Kind) -> Option<&str> {
    Some(match kind {
        Kind::Deliverable => &settings.deliverable_type,
        Kind::Epic => &settings.epic_type,
        Kind::Feature => &settings.feature_type,
        Kind::Task => &settings.task_type,
        Kind::Defect => &settings.defect_type,
        Kind::Milestone => return None,
    })
}

/// FTS-FR-CJVS: the order in which the kinds are tried.
pub const PULL_ORDER: [Kind; 5] =
    [Kind::Deliverable, Kind::Epic, Kind::Feature, Kind::Task, Kind::Defect];

/// The kinds whose Type is `issue_type`, in the order of FTS-FR-CJVS.
pub fn kinds_of_type(settings: &FlowTrackerSettings, issue_type: Option<&str>) -> Vec<Kind> {
    let Some(wanted) = issue_type else { return Vec::new() };
    PULL_ORDER
        .into_iter()
        .filter(|k| type_for(settings, *k).is_some_and(|t| t.eq_ignore_ascii_case(wanted)))
        .collect()
}

/// FTS-FR-CJVS: the first kind with the Type that `container` permits.
pub fn kind_for(
    settings: &FlowTrackerSettings,
    issue_type: Option<&str>,
    container: Option<Kind>,
) -> Option<Kind> {
    kinds_of_type(settings, issue_type)
        .into_iter()
        .find(|k| k.permitted_parents().contains(&container))
}

/// FTS-FR-VRRJ: a description as two descriptions are compared: LF line
/// endings, and no whitespace at the end of the text.
pub fn normalize_description(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n").trim_end().to_string()
}

/// An issue Type as two Types are compared.
pub fn normalize_type(value: Option<&str>) -> Option<String> {
    value.map(|t| t.trim().to_ascii_lowercase())
}

/// The part of a reference that names the tracker item. The address is left
/// out, so two references to the same item compare equal.
pub type RefId = (String, String, u64);

pub fn ref_id(r: &TrackerRef) -> RefId {
    (r.owner.to_ascii_lowercase(), r.repo.to_ascii_lowercase(), r.number)
}

pub fn key_id(k: &IssueKey) -> RefId {
    (k.owner.to_ascii_lowercase(), k.repo.to_ascii_lowercase(), k.number)
}

/// The publication repository a sync is bound to.
#[derive(Clone, Debug)]
pub struct Repo {
    pub host: String,
    pub owner: String,
    pub repo: String,
}

impl Repo {
    /// FTS-FR-WANV: whether a reference names this repository.
    pub fn holds(&self, r: &TrackerRef) -> bool {
        r.host.eq_ignore_ascii_case(&self.host)
            && r.owner.eq_ignore_ascii_case(&self.owner)
            && r.repo.eq_ignore_ascii_case(&self.repo)
    }

    pub fn holds_key(&self, k: &IssueKey) -> bool {
        k.owner.eq_ignore_ascii_case(&self.owner) && k.repo.eq_ignore_ascii_case(&self.repo)
    }

    /// The reference the Flow stores for an issue key of this host.
    pub fn issue_ref(&self, k: &IssueKey) -> TrackerRef {
        TrackerRef {
            kind: RefKind::Issue,
            host: self.host.clone(),
            owner: k.owner.clone(),
            repo: k.repo.clone(),
            number: k.number,
            url: k.url.clone(),
        }
    }
}

/// The links of a document that this repository holds, by item id.
pub fn own_links<'a>(doc: &'a FlowDocument, repo: &Repo) -> HashMap<&'a str, &'a SyncLink> {
    doc.sync
        .iter()
        .flat_map(|s| s.links.iter())
        .filter(|l| repo.holds(&l.reference))
        .map(|l| (l.item_id.as_str(), l))
        .collect()
}

/// FTS-FR-HLVJ: the structural values of one item on the Flow side.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Derived {
    pub parent: Option<TrackerRef>,
    pub milestone: Option<u64>,
    pub blocked_by: Vec<TrackerRef>,
}

/// FTS-FR-HLVJ: `parent` and `milestone` of an item that sits in
/// `container`: the nearest linked ancestor that is not a milestone, and the
/// nearest linked milestone ancestor.
pub fn derive_place(
    hierarchy: &Hierarchy<'_>,
    links: &HashMap<&str, &SyncLink>,
    container: Option<&str>,
) -> (Option<TrackerRef>, Option<u64>) {
    let mut chain: Vec<&str> = Vec::new();
    if let Some(c) = container {
        chain.push(c);
        chain.extend(hierarchy.ancestors(c));
    }
    let mut parent = None;
    let mut milestone = None;
    for id in chain {
        let Some(link) = links.get(id) else { continue };
        let is_milestone = hierarchy.kind(id) == Some(Kind::Milestone);
        if is_milestone && milestone.is_none() && link.reference.kind == RefKind::Milestone {
            milestone = Some(link.reference.number);
        }
        if !is_milestone && parent.is_none() && link.reference.kind == RefKind::Issue {
            parent = Some(link.reference.clone());
        }
    }
    (parent, milestone)
}

/// FTS-FR-HLVJ: the values of every item of `doc`.
pub fn derive_all(doc: &FlowDocument, repo: &Repo) -> HashMap<String, Derived> {
    let hierarchy = Hierarchy::new(&doc.items);
    let links = own_links(doc, repo);
    let mut out: HashMap<String, Derived> = HashMap::new();
    for item in &doc.items {
        let (parent, milestone) = derive_place(&hierarchy, &links, item.parent_id.as_deref());
        out.insert(item.id.clone(), Derived { parent, milestone, blocked_by: Vec::new() });
    }
    for edge in doc.edges.iter().filter(|e| e.edge_type == EdgeType::Blocks) {
        let Some(link) = links.get(edge.from.as_str()) else { continue };
        if link.reference.kind != RefKind::Issue {
            continue;
        }
        if let Some(d) = out.get_mut(&edge.to) {
            if !d.blocked_by.iter().any(|r| ref_id(r) == ref_id(&link.reference)) {
                d.blocked_by.push(link.reference.clone());
            }
        }
    }
    out
}

/// FTS-FR-EGJL: the container of an item whose tracker parent is the item
/// `parent` and whose milestone is the milestone item `milestone`: the
/// milestone item where it is under the parent's item, and the parent's item
/// otherwise.
pub fn place(
    hierarchy: &Hierarchy<'_>,
    parent: Option<&str>,
    milestone: Option<&str>,
) -> Option<String> {
    match (parent, milestone) {
        (None, Some(m)) => Some(m.to_string()),
        (Some(p), Some(m)) if hierarchy.is_ancestor(p, m) => Some(m.to_string()),
        (Some(p), _) => Some(p.to_string()),
        (None, None) => None,
    }
}

/// The extent of an item on the canvas.
pub fn extent(item: &Item) -> Size {
    item.size.unwrap_or(Size { width: CARD_W, height: CARD_H })
}

/// FTS-FR-LKZO: the position of a new member of `container`: below its last
/// member, or at the first place of an empty container. A new top-level item
/// goes below the lowest top-level item.
pub fn position_below_last(doc: &FlowDocument, container: Option<&str>) -> Position {
    let bottom = doc
        .items
        .iter()
        .filter(|i| i.parent_id.as_deref() == container)
        .map(|i| i.position.y + extent(i).height)
        .fold(None, |acc: Option<f64>, b| Some(acc.map_or(b, |a| a.max(b))));
    match (bottom, container) {
        (Some(b), _) => Position { x: CONTAINER_PADDING, y: b + CONTAINER_PADDING },
        (None, Some(_)) => {
            Position { x: CONTAINER_PADDING, y: CONTAINER_HEADER_H + CONTAINER_PADDING }
        }
        (None, None) => Position { x: 40.0, y: 40.0 },
    }
}

/// FLO-FR-44: make each container from `id` outwards large enough to hold its
/// members with its padding.
pub fn grow_to_fit(doc: &mut FlowDocument, id: Option<&str>) {
    let mut at = id.map(str::to_string);
    let mut guard = 0;
    while let Some(container) = at {
        guard += 1;
        if guard > doc.items.len() {
            return;
        }
        let (mut width, mut height) = (0.0_f64, 0.0_f64);
        for m in doc.items.iter().filter(|i| i.parent_id.as_deref() == Some(container.as_str())) {
            let e = extent(m);
            width = width.max(m.position.x + e.width + CONTAINER_PADDING);
            height = height.max(m.position.y + e.height + CONTAINER_PADDING);
        }
        let Some(item) = doc.items.iter_mut().find(|i| i.id == container) else { return };
        if let Some(size) = item.size.as_mut() {
            size.width = size.width.max(width);
            size.height = size.height.max(height);
        }
        at = item.parent_id.clone();
    }
}

/// Whether `doc` still obeys the structural rules a sync can break: the
/// containers each kind permits (FGV-FR-YHFX), no containment cycle
/// (FGV-FR-13), the fields each kind carries (FGV-FR-MZIA), no edge within a
/// lineage (FGV-FR-KBLT), one edge between two items (FGV-FR-11), and no
/// dependency cycle (FGV-FR-PMTY).
pub fn structure_ok(doc: &FlowDocument) -> bool {
    let hierarchy = Hierarchy::new(&doc.items);
    let kinds: HashMap<&str, Kind> = doc.items.iter().map(|i| (i.id.as_str(), i.kind)).collect();
    for item in &doc.items {
        let parent_kind = match item.parent_id.as_deref() {
            None => None,
            Some(p) => match kinds.get(p) {
                Some(k) => Some(*k),
                None => return false,
            },
        };
        if !item.kind.permitted_parents().contains(&parent_kind) {
            return false;
        }
        let container = item.kind.is_container();
        if container != item.size.is_some()
            || (container && (item.draft_id.is_some() || item.footprint.is_some()))
            || (item.kind != Kind::Milestone && item.due.is_some())
        {
            return false;
        }
    }
    // A chain that ends in a cycle never reaches the top level.
    let parents: HashMap<&str, Option<&str>> =
        doc.items.iter().map(|i| (i.id.as_str(), i.parent_id.as_deref())).collect();
    for item in &doc.items {
        let mut at = item.parent_id.as_deref();
        let mut steps = 0;
        while let Some(p) = at {
            steps += 1;
            if p == item.id || steps > doc.items.len() {
                return false;
            }
            at = parents.get(p).copied().flatten();
        }
    }
    let mut pairs: HashSet<(&str, &str)> = HashSet::new();
    let mut blocks = Vec::new();
    for edge in &doc.edges {
        let (a, b) = (edge.from.as_str(), edge.to.as_str());
        if a == b || !kinds.contains_key(a) || !kinds.contains_key(b) {
            return false;
        }
        if hierarchy.is_ancestor(a, b) || hierarchy.is_ancestor(b, a) {
            return false;
        }
        let pair = if a < b { (a, b) } else { (b, a) };
        if !pairs.insert(pair) {
            return false;
        }
        if edge.edge_type == EdgeType::Blocks {
            blocks.push((edge.id.as_str(), a, b));
        }
    }
    WaitGraph::new(&hierarchy, &blocks).cycle_edges().is_empty()
}

/// Every item at or under `id`.
pub fn subtree(doc: &FlowDocument, id: &str) -> HashSet<String> {
    let mut out: HashSet<String> = HashSet::new();
    out.insert(id.to_string());
    loop {
        let before = out.len();
        for item in &doc.items {
            if item.parent_id.as_ref().is_some_and(|p| out.contains(p)) {
                out.insert(item.id.clone());
            }
        }
        if out.len() == before {
            return out;
        }
    }
}

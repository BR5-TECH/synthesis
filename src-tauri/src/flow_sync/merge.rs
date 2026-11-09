//! FTS-FR-AIJM to FTS-FR-CYJJ, FTS-FR-VEEG to FTS-FR-SZGF, FTS-FR-PBQC: the
//! three-way merge of each link whose item is on both sides.

use serde_json::Value;

use crate::flow_validation::model::{
    Edge, EdgeType, Hierarchy, Kind, RefKind, SyncLink, TrackerRef,
};

use super::adapter::{IssueChange, MilestoneChange, TrackerIssue, TrackerMilestone};
use super::engine::{flow_description, Sync, Write};
use super::records::*;
use super::removed::ref_value;
use super::values::{self, Derived, RefId};

/// Which side a field takes after the comparison.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Side {
    /// Neither side changed it.
    Same,
    /// Only the Flow changed it: it is written to the tracker.
    Flow,
    /// Only the tracker changed it: it is written to the Flow.
    Tracker,
    /// Both changed it to the same value: only the base changes.
    Both,
    /// Both changed it to different values.
    Conflict,
}

/// FTS-FR-AIJM: compare the normalized base, Flow, and tracker values.
pub(super) fn decide<T: PartialEq>(base: &T, flow: &T, tracker: &T) -> Side {
    match (flow != base, tracker != base) {
        (false, false) => Side::Same,
        (true, false) => Side::Flow,
        (false, true) => Side::Tracker,
        (true, true) if flow == tracker => Side::Both,
        (true, true) => Side::Conflict,
    }
}

/// The structural fields, which a tracker change applies to the Flow as a
/// move or a change of kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Structural {
    Type,
    Parent,
    Milestone,
}

impl Structural {
    fn name(self) -> &'static str {
        match self {
            Self::Type => "type",
            Self::Parent => "parent",
            Self::Milestone => "milestone",
        }
    }
}

/// One structural field of one link, with its three values.
struct StructuralField {
    field: Structural,
    side: Side,
    base: Value,
    flow: Value,
    tracker: Value,
    flow_key: String,
    tracker_key: String,
}

fn opt_key(value: Option<String>) -> String {
    value.unwrap_or_else(|| "null".to_string())
}

fn ref_key(id: Option<RefId>) -> String {
    opt_key(id.map(|(o, r, n)| format!("{o}/{r}#{n}")))
}

impl Sync<'_> {
    /// Merge every link whose item is in the Flow and on the tracker.
    pub(super) fn merge_pass(&mut self) {
        let derived = values::derive_all(&self.doc, &self.repo);
        let links: Vec<SyncLink> = self.links().to_vec();
        for link in links {
            if !self.repo.holds(&link.reference) || self.item(&link.item_id).is_none() {
                continue;
            }
            let number = link.reference.number;
            match link.reference.kind {
                RefKind::Issue => {
                    let Some(issue) = self.issues.get(&number).cloned() else { continue };
                    let d = derived.get(&link.item_id).cloned().unwrap_or_default();
                    self.merge_issue(&link, &d, &issue);
                }
                RefKind::Milestone => {
                    let Some(m) = self.milestones.get(&number).cloned() else { continue };
                    self.merge_milestone(&link, &m);
                }
            }
        }
    }

    /// A field conflict, or `None` where the author's choice settles it. The
    /// choice is returned as the side the field takes.
    #[allow(clippy::too_many_arguments)]
    fn conflict(
        &mut self,
        link: &SyncLink,
        kind: ConflictKind,
        field: &'static str,
        keys: (&str, &str),
        base: Value,
        flow: Value,
        tracker: Value,
        url: &str,
    ) -> Result<Side, Choice> {
        let item_name = self.item(&link.item_id).map(|i| i.name.clone()).unwrap_or_default();
        let conflict = SyncConflict {
            id: self.conflict_id(&link.item_id, kind, field, keys.0, keys.1),
            kind,
            item_id: link.item_id.clone(),
            item_name,
            field: Some(field),
            base,
            flow,
            tracker,
            choices: kind.choices(),
            url: Some(url.to_string()),
        };
        match self.choice_for(&conflict) {
            Some(Choice::KeepFlow) => Ok(Side::Flow),
            Some(Choice::TakeTracker) => Ok(Side::Tracker),
            Some(other) => Err(other),
            None => {
                self.result.conflicts.push(conflict);
                Ok(Side::Conflict)
            }
        }
    }

    /// A text field: `name` or `description`.
    fn text_side(
        &mut self,
        link: &SyncLink,
        field: &'static str,
        values: (&str, &str, &str),
        url: &str,
    ) -> Result<Side, Choice> {
        let norm = |t: &str| {
            if field == "description" {
                values::normalize_description(t)
            } else {
                t.to_string()
            }
        };
        let (b, f, t) = (norm(values.0), norm(values.1), norm(values.2));
        match decide(&b, &f, &t) {
            Side::Conflict => self.conflict(
                link,
                ConflictKind::Field,
                field,
                (&f, &t),
                Value::String(values.0.to_string()),
                Value::String(values.1.to_string()),
                Value::String(values.2.to_string()),
                url,
            ),
            side => Ok(side),
        }
    }

    fn merge_issue(&mut self, link: &SyncLink, derived: &Derived, issue: &TrackerIssue) {
        let item_id = link.item_id.clone();
        let Some(item) = self.item(&item_id).cloned() else { return };
        let base = link.base.clone();
        let url = issue.url.clone();
        let flow_desc = item.description.clone().unwrap_or_default();

        let name = self.text_side(link, "name", (&base.name, &item.name, &issue.title), &url);
        let desc =
            self.text_side(link, "description", (&base.description, &flow_desc, &issue.body), &url);
        let (Ok(name), Ok(desc)) = (name, desc) else { return };

        let flow_type = values::type_for(self.settings, item.kind).map(str::to_string);
        let base_type = base.r#type.clone().flatten();
        let base_parent = base.parent.clone().flatten();
        let base_milestone = base.milestone.flatten();
        // A parent or a milestone that is gone on one side waits for the
        // author to decide its removal (FTS-FR-BMWI, FTS-FR-VXMN). Until then
        // the tracker value reads as the base, so no item moves.
        let parent_waits =
            base_parent.as_ref().is_some_and(|p| self.pending.contains(&values::ref_id(p)));
        let milestone_waits = base_milestone.is_some_and(|m| {
            self.missing_milestones.contains(&m)
                || self.links().iter().any(|l| {
                    l.reference.kind == RefKind::Milestone
                        && l.reference.number == m
                        && self.repo.holds(&l.reference)
                        && self.item(&l.item_id).is_none()
                })
        });
        let tracker_parent_key: Option<RefId> = if parent_waits {
            base_parent.as_ref().map(values::ref_id)
        } else {
            issue.parent.as_ref().map(values::key_id)
        };
        let tracker_milestone = if milestone_waits { base_milestone } else { issue.milestone };
        let tracker_parent = if parent_waits {
            base_parent.clone()
        } else {
            issue.parent.as_ref().map(|k| self.repo.issue_ref(k))
        };
        let mut fields = vec![
            StructuralField {
                field: Structural::Type,
                side: decide(
                    &values::normalize_type(base_type.as_deref()),
                    &values::normalize_type(flow_type.as_deref()),
                    &values::normalize_type(issue.issue_type.as_deref()),
                ),
                base: base_type.clone().map_or(Value::Null, Value::String),
                flow: flow_type.clone().map_or(Value::Null, Value::String),
                tracker: issue.issue_type.clone().map_or(Value::Null, Value::String),
                flow_key: opt_key(values::normalize_type(flow_type.as_deref())),
                tracker_key: opt_key(values::normalize_type(issue.issue_type.as_deref())),
            },
            StructuralField {
                field: Structural::Parent,
                side: decide(
                    &base_parent.as_ref().map(values::ref_id),
                    &derived.parent.as_ref().map(values::ref_id),
                    &tracker_parent_key,
                ),
                base: ref_value(base_parent.as_ref()),
                flow: ref_value(derived.parent.as_ref()),
                tracker: ref_value(tracker_parent.as_ref()),
                flow_key: ref_key(derived.parent.as_ref().map(values::ref_id)),
                tracker_key: ref_key(tracker_parent_key.clone()),
            },
            StructuralField {
                field: Structural::Milestone,
                side: decide(&base_milestone, &derived.milestone, &tracker_milestone),
                base: base_milestone.map_or(Value::Null, Value::from),
                flow: derived.milestone.map_or(Value::Null, Value::from),
                tracker: tracker_milestone.map_or(Value::Null, Value::from),
                flow_key: opt_key(derived.milestone.map(|m| m.to_string())),
                tracker_key: opt_key(tracker_milestone.map(|m| m.to_string())),
            },
        ];

        // FTS-FR-SZGF: a conflict whose tracker value the plan cannot hold is
        // `invalid_on_tracker`, so `take_tracker` is never offered for it.
        for f in fields.iter_mut().filter(|f| f.side == Side::Conflict) {
            let kind = if self.take_structural(&item_id, issue, derived, &[f.field]).is_some() {
                ConflictKind::Field
            } else {
                ConflictKind::InvalidOnTracker
            };
            match self.structural_conflict(link, f, kind, &url) {
                Ok(side) => f.side = side,
                Err(Choice::Unlink) => return self.unlink(&item_id),
                Err(_) => f.side = Side::Conflict,
            }
        }
        // Apply the tracker changes together. Where the plan refuses them, each
        // becomes `invalid_on_tracker`, and the ones the author keeps from the
        // Flow drop out before the next try.
        for _ in 0..=fields.len() {
            let taken: Vec<Structural> =
                fields.iter().filter(|f| f.side == Side::Tracker).map(|f| f.field).collect();
            if taken.is_empty() {
                break;
            }
            if let Some(doc) = self.take_structural(&item_id, issue, derived, &taken) {
                self.doc = doc;
                self.mark_changed(&item_id);
                break;
            }
            for f in fields.iter_mut().filter(|f| f.side == Side::Tracker) {
                let kind = ConflictKind::InvalidOnTracker;
                match self.structural_conflict(link, f, kind, &url) {
                    Ok(Side::Tracker) | Ok(Side::Conflict) | Err(_) => f.side = Side::Conflict,
                    Ok(side) => f.side = side,
                }
            }
            if self.used_unlink(link, &fields, &url) {
                return self.unlink(&item_id);
            }
        }

        // Text fields taken from the tracker.
        if name == Side::Tracker || desc == Side::Tracker {
            if let Some(i) = self.item_mut(&item_id) {
                if name == Side::Tracker {
                    i.name = issue.title.clone();
                }
                if desc == Side::Tracker {
                    i.description = flow_description(&issue.body);
                }
            }
            self.mark_changed(&item_id);
        }

        // The base: the tracker value where the tracker side holds, the old
        // base where the field is in conflict or waits for a write.
        let side_of = |f: Structural| fields.iter().find(|x| x.field == f).map(|x| x.side);
        let mut change = IssueChange::default();
        if let Some(l) = self.link_mut(&item_id) {
            if matches!(name, Side::Tracker | Side::Both) {
                l.base.name = issue.title.clone();
            }
            if matches!(desc, Side::Tracker | Side::Both) {
                l.base.description = issue.body.clone();
            }
            if matches!(side_of(Structural::Type), Some(Side::Tracker | Side::Both)) {
                l.base.r#type = Some(issue.issue_type.clone());
            }
            if matches!(side_of(Structural::Parent), Some(Side::Tracker | Side::Both)) {
                l.base.parent = Some(tracker_parent.clone());
            }
            if matches!(side_of(Structural::Milestone), Some(Side::Tracker | Side::Both)) {
                l.base.milestone = Some(tracker_milestone);
            }
            // FTS-FR-CYJJ: the state is read, never written.
            l.base.state = issue.state.clone();
            l.base.r#type.get_or_insert(None);
            l.base.parent.get_or_insert(None);
            l.base.milestone.get_or_insert(None);
            l.base.blocked_by.get_or_insert_with(Vec::new);
        }
        if name == Side::Flow {
            change.title = Some(item.name.clone());
        }
        if desc == Side::Flow {
            change.body = Some(flow_desc.clone());
        }
        if side_of(Structural::Type) == Some(Side::Flow) {
            change.issue_type = Some(flow_type.clone());
        }
        let number = issue.number;
        if !change.is_empty() {
            self.writes.push(Write::Issue { item_id: item_id.clone(), number, change });
        }
        if side_of(Structural::Parent) == Some(Side::Flow) {
            let current =
                issue.parent.as_ref().filter(|p| self.repo.holds_key(p)).map(|p| p.number);
            self.writes.push(Write::Parent {
                item_id: item_id.clone(),
                number,
                current,
                parent: derived.parent.clone(),
            });
        }
        if side_of(Structural::Milestone) == Some(Side::Flow) {
            self.writes.push(Write::Milestone {
                item_id: item_id.clone(),
                number,
                milestone: derived.milestone,
            });
        }
        self.merge_blocked_by(&item_id, number, derived, issue);
    }

    fn structural_conflict(
        &mut self,
        link: &SyncLink,
        f: &StructuralField,
        kind: ConflictKind,
        url: &str,
    ) -> Result<Side, Choice> {
        self.conflict(
            link,
            kind,
            f.field.name(),
            (&f.flow_key, &f.tracker_key),
            f.base.clone(),
            f.flow.clone(),
            f.tracker.clone(),
            url,
        )
    }

    /// Whether the author chose `unlink` for an `invalid_on_tracker` conflict
    /// of this link in this call.
    fn used_unlink(&self, link: &SyncLink, fields: &[StructuralField], _url: &str) -> bool {
        fields.iter().any(|f| {
            let id = self.conflict_id(
                &link.item_id,
                ConflictKind::InvalidOnTracker,
                f.field.name(),
                &f.flow_key,
                &f.tracker_key,
            );
            self.used.contains(&id) && self.resolutions.get(&id) == Some(&Choice::Unlink)
        })
    }

    /// The document with the tracker values of `taken` applied to the item:
    /// a change of kind, a move, or both. `None` where the plan cannot hold
    /// them (FTS-FR-SZGF).
    fn take_structural(
        &self,
        item_id: &str,
        issue: &TrackerIssue,
        derived: &Derived,
        taken: &[Structural],
    ) -> Option<crate::flow_validation::model::FlowDocument> {
        let item = self.item(item_id)?;
        let take = |f| taken.contains(&f);
        let want_parent: Option<RefId> = if take(Structural::Parent) {
            issue.parent.as_ref().map(values::key_id)
        } else {
            derived.parent.as_ref().map(values::ref_id)
        };
        let want_milestone =
            if take(Structural::Milestone) { issue.milestone } else { derived.milestone };
        let moving = take(Structural::Parent) || take(Structural::Milestone);
        let container = if moving {
            let parent_item = match &want_parent {
                Some((owner, repo, number)) => {
                    if !owner.eq_ignore_ascii_case(&self.repo.owner)
                        || !repo.eq_ignore_ascii_case(&self.repo.repo)
                    {
                        return None;
                    }
                    Some(self.item_linked_to(RefKind::Issue, *number)?)
                }
                None => None,
            };
            let milestone_item = match want_milestone {
                Some(m) => Some(self.item_linked_to(RefKind::Milestone, m)?),
                None => None,
            };
            let hierarchy = Hierarchy::new(&self.doc.items);
            values::place(&hierarchy, parent_item.as_deref(), milestone_item.as_deref())
        } else {
            item.parent_id.clone()
        };
        let kinds: Vec<Kind> = if take(Structural::Type) {
            values::kinds_of_type(self.settings, issue.issue_type.as_deref())
        } else {
            vec![item.kind]
        };
        for kind in kinds {
            let mut doc = self.doc.clone();
            let moved_from = item.parent_id.clone();
            let position = if moving && container != moved_from {
                Some(values::position_below_last(&doc, container.as_deref()))
            } else {
                None
            };
            let Some(target) = doc.items.iter_mut().find(|i| i.id == item_id) else { continue };
            if target.kind != kind {
                target.kind = kind;
                target.size = if kind.is_container() {
                    target.size.or_else(|| values::default_size(kind))
                } else {
                    None
                };
            }
            if let Some(p) = position {
                target.parent_id = container.clone();
                target.position = p;
            }
            if !values::structure_ok(&doc) {
                continue;
            }
            let hierarchy = Hierarchy::new(&doc.items);
            let links = values::own_links(&doc, &self.repo);
            let (p, m) = values::derive_place(&hierarchy, &links, container.as_deref());
            if p.as_ref().map(values::ref_id) != want_parent || m != want_milestone {
                continue;
            }
            drop(hierarchy);
            values::grow_to_fit(&mut doc, container.as_deref());
            return Some(doc);
        }
        None
    }

    /// FTS-FR-VYOV: `blockedBy`, merged per entry. It never yields a conflict.
    fn merge_blocked_by(
        &mut self,
        item_id: &str,
        number: u64,
        derived: &Derived,
        issue: &TrackerIssue,
    ) {
        let base: Vec<TrackerRef> =
            self.link(item_id).and_then(|l| l.base.blocked_by.clone()).unwrap_or_default();
        let flow: Vec<(RefId, TrackerRef)> =
            derived.blocked_by.iter().map(|r| (values::ref_id(r), r.clone())).collect();
        let tracker: Vec<RefId> = issue.blocked_by.iter().map(values::key_id).collect();
        let in_base = |id: &RefId| base.iter().any(|r| &values::ref_id(r) == id);
        let in_flow = |id: &RefId| flow.iter().any(|(f, _)| f == id);
        let in_tracker = |id: &RefId| tracker.contains(id);

        let waits = |id: &RefId| self.pending.contains(id);

        let mut new_base: Vec<TrackerRef> = Vec::new();
        let mut writes = Vec::new();
        let mut gone_on_tracker = Vec::new();
        for r in &base {
            let id = values::ref_id(r);
            // A blocker whose removal waits for the author keeps its entry,
            // and no relation or edge of it changes meanwhile.
            if waits(&id) {
                new_base.push(r.clone());
                continue;
            }
            match (in_flow(&id), in_tracker(&id)) {
                (true, true) => new_base.push(r.clone()),
                // Removed in the Flow: the base keeps the entry until the
                // removal on the tracker is made.
                (false, true) => {
                    new_base.push(r.clone());
                    writes.push(Write::RemoveBlocker {
                        item_id: item_id.to_string(),
                        number,
                        blocker: r.clone(),
                    });
                }
                // Removed on the tracker: the edge goes from the Flow.
                (true, false) => gone_on_tracker.push(id),
                (false, false) => {}
            }
        }
        for (id, r) in &flow {
            if in_base(id) || waits(id) {
                continue;
            }
            if in_tracker(id) {
                new_base.push(r.clone());
            } else {
                writes.push(Write::AddBlocker {
                    item_id: item_id.to_string(),
                    number,
                    blocker: r.clone(),
                });
            }
        }
        let added_on_tracker: Vec<RefId> = tracker
            .iter()
            .filter(|id| !in_base(id) && !in_flow(id) && !waits(id))
            .cloned()
            .collect();
        for id in gone_on_tracker {
            self.remove_blocks_edge(&id, item_id);
        }
        // Added on the tracker: an edge where the plan can hold one. An entry
        // the plan cannot hold stays out of the base, so it reads the same in
        // the next sync and nothing is removed from the tracker.
        for id in added_on_tracker {
            if let Some(link_ref) = self.add_blocks_edge(&id, item_id) {
                new_base.push(link_ref);
            }
        }
        self.writes.extend(writes);
        if let Some(l) = self.link_mut(item_id) {
            l.base.blocked_by = Some(new_base);
        }
    }

    fn remove_blocks_edge(&mut self, blocker: &RefId, item_id: &str) {
        let Some(from) = self.item_linked_to(RefKind::Issue, blocker.2) else { return };
        let before = self.doc.edges.len();
        self.doc
            .edges
            .retain(|e| !(e.edge_type == EdgeType::Blocks && e.from == from && e.to == item_id));
        if self.doc.edges.len() != before {
            self.mark_changed(item_id);
        }
    }

    /// Add the edge "`blocker` blocks `item_id`" where the plan holds it, and
    /// return the reference of the blocker's link.
    fn add_blocks_edge(&mut self, blocker: &RefId, item_id: &str) -> Option<TrackerRef> {
        if !blocker.0.eq_ignore_ascii_case(&self.repo.owner)
            || !blocker.1.eq_ignore_ascii_case(&self.repo.repo)
        {
            return None;
        }
        let from = self.item_linked_to(RefKind::Issue, blocker.2)?;
        let reference = self.link(&from)?.reference.clone();
        let mut doc = self.doc.clone();
        let id = values::mint_edge_id(&doc);
        doc.edges.push(Edge { id, from, to: item_id.to_string(), edge_type: EdgeType::Blocks });
        if !values::structure_ok(&doc) {
            return None;
        }
        self.doc = doc;
        self.mark_changed(item_id);
        Some(reference)
    }

    fn merge_milestone(&mut self, link: &SyncLink, m: &TrackerMilestone) {
        let item_id = link.item_id.clone();
        let Some(item) = self.item(&item_id).cloned() else { return };
        let base = link.base.clone();
        let flow_desc = item.description.clone().unwrap_or_default();
        let name = self.text_side(link, "name", (&base.name, &item.name, &m.title), &m.url);
        let desc = self.text_side(
            link,
            "description",
            (&base.description, &flow_desc, &m.description),
            &m.url,
        );
        let base_due = base.due.clone().flatten();
        let due = match decide(&base_due, &item.due, &m.due) {
            Side::Conflict => {
                let as_value = |d: &Option<String>| d.clone().map_or(Value::Null, Value::String);
                let fk = opt_key(item.due.clone());
                let tk = opt_key(m.due.clone());
                self.conflict(
                    link,
                    ConflictKind::Field,
                    "due",
                    (&fk, &tk),
                    as_value(&base_due),
                    as_value(&item.due),
                    as_value(&m.due),
                    &m.url,
                )
            }
            side => Ok(side),
        };
        let (Ok(name), Ok(desc), Ok(due)) = (name, desc, due) else { return };
        if [name, desc, due].contains(&Side::Tracker) {
            if let Some(i) = self.item_mut(&item_id) {
                if name == Side::Tracker {
                    i.name = m.title.clone();
                }
                if desc == Side::Tracker {
                    i.description = flow_description(&m.description);
                }
                if due == Side::Tracker {
                    i.due = m.due.clone();
                }
            }
            self.mark_changed(&item_id);
        }
        let mut change = MilestoneChange::default();
        if let Some(l) = self.link_mut(&item_id) {
            if matches!(name, Side::Tracker | Side::Both) {
                l.base.name = m.title.clone();
            }
            if matches!(desc, Side::Tracker | Side::Both) {
                l.base.description = m.description.clone();
            }
            if matches!(due, Side::Tracker | Side::Both) {
                l.base.due = Some(m.due.clone());
            }
            l.base.state = m.state.clone();
            l.base.due.get_or_insert(None);
        }
        if name == Side::Flow {
            change.title = Some(item.name.clone());
        }
        if desc == Side::Flow {
            change.description = Some(flow_desc);
        }
        if due == Side::Flow {
            change.due = Some(item.due.clone());
        }
        if !change.is_empty() {
            self.writes.push(Write::MilestoneFields { item_id, number: m.number, change });
        }
    }
}

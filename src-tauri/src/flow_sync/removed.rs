//! The links whose item is gone on one side (FTS-FR-BMWI, FTS-FR-VXMN), the
//! links of another repository (FTS-FR-WANV), and the choices that answer a
//! removal: `unlink`, `restore`, and `remove_item`.

use serde_json::Value;

use crate::flow_validation::model::{
    Hierarchy, Item, Kind, RefKind, SyncBase, SyncLink, TrackerRef,
};

use super::engine::{flow_description, Sync};
use super::records::*;
use super::values;

/// The tracker values of one link, as a restore and a conflict read them.
struct Found {
    title: String,
    url: String,
}

impl Sync<'_> {
    /// Every link whose item is gone from the Flow or from the tracker. A link
    /// whose item is gone from both sides holds nothing to decide and is
    /// removed.
    pub(super) fn removed_pass(&mut self) {
        let links: Vec<SyncLink> = self.links().to_vec();
        let mut restores: Vec<(SyncLink, SyncConflict)> = Vec::new();
        for link in links {
            let number = link.reference.number;
            if !self.repo.holds(&link.reference) {
                // FTS-FR-WANV.
                self.skip(Some(number), &link.base.name, SKIP_FOREIGN_REPOSITORY, None);
                continue;
            }
            let in_flow = self.original.items.iter().any(|i| i.id == link.item_id);
            let found = self.found(&link);
            match (in_flow, found) {
                (true, Some(_)) => {}
                (false, None) => self.remove_link(&link.item_id),
                (false, Some(found)) => {
                    if let Some(conflict) = self.removed_in_flow(&link, found) {
                        restores.push((link, conflict));
                    }
                }
                (true, None) => self.removed_on_tracker(&link),
            }
        }
        // A restore needs the item of its parent, which can be a restore of
        // this call too: try again while a try adds an item.
        loop {
            let before = restores.len();
            let mut left = Vec::new();
            for (link, conflict) in restores {
                if !self.restore(&link) {
                    left.push((link, conflict));
                }
            }
            restores = left;
            if restores.is_empty() || restores.len() == before {
                break;
            }
        }
        // A restore that the plan cannot hold applies nothing, and the
        // conflict stands.
        self.result.conflicts.extend(restores.into_iter().map(|(_, c)| c));
    }

    fn found(&self, link: &SyncLink) -> Option<Found> {
        let number = link.reference.number;
        match link.reference.kind {
            RefKind::Issue => self
                .issues
                .get(&number)
                .map(|i| Found { title: i.title.clone(), url: i.url.clone() }),
            RefKind::Milestone => self
                .milestones
                .get(&number)
                .map(|m| Found { title: m.title.clone(), url: m.url.clone() }),
        }
    }

    /// FTS-FR-BMWI. Returns the conflict where the author chose `restore`, so
    /// the caller restores it once the other restores are made.
    fn removed_in_flow(&mut self, link: &SyncLink, found: Found) -> Option<SyncConflict> {
        let kind = ConflictKind::RemovedInFlow;
        let conflict = SyncConflict {
            id: self.conflict_id(&link.item_id, kind, "", "", ""),
            kind,
            item_id: link.item_id.clone(),
            item_name: found.title.clone(),
            field: None,
            base: Value::String(link.base.name.clone()),
            flow: Value::Null,
            tracker: Value::String(found.title),
            choices: kind.choices(),
            url: Some(found.url),
        };
        match self.choice_for(&conflict) {
            Some(Choice::Unlink) => self.remove_link(&link.item_id),
            Some(Choice::Restore) => return Some(conflict),
            _ => self.result.conflicts.push(conflict),
        }
        None
    }

    /// FTS-FR-VXMN.
    fn removed_on_tracker(&mut self, link: &SyncLink) {
        let kind = ConflictKind::RemovedOnTracker;
        let name = self.item(&link.item_id).map(|i| i.name.clone()).unwrap_or_default();
        let conflict = SyncConflict {
            id: self.conflict_id(&link.item_id, kind, "", "", ""),
            kind,
            item_id: link.item_id.clone(),
            item_name: name.clone(),
            field: None,
            base: Value::String(link.base.name.clone()),
            flow: Value::String(name),
            tracker: Value::Null,
            choices: kind.choices(),
            url: None,
        };
        match self.choice_for(&conflict) {
            Some(Choice::RemoveItem) => self.remove_item(&link.item_id),
            Some(Choice::Unlink) => self.unlink(&link.item_id),
            _ => self.result.conflicts.push(conflict),
        }
    }

    /// `unlink`: the link goes, and the item and the tracker item stay as they
    /// are. No issue is created for the item in the same call.
    pub(super) fn unlink(&mut self, item_id: &str) {
        // The conflicts of the link go with it.
        self.result.conflicts.retain(|c| c.item_id != item_id);
        self.remove_link(item_id);
        self.no_create.insert(item_id.to_string());
        self.mark_changed(item_id);
    }

    /// `remove_item`: the item, every item under it, and every edge of each go,
    /// with the link of the item. The links of the items under it stay, so the
    /// next sync reports them as removed in the Flow.
    fn remove_item(&mut self, item_id: &str) {
        let gone = values::subtree(&self.doc, item_id);
        self.doc.items.retain(|i| !gone.contains(&i.id));
        self.doc.edges.retain(|e| !gone.contains(&e.from) && !gone.contains(&e.to));
        self.remove_link(item_id);
        self.mark_changed(item_id);
    }

    /// `restore`: add the item back from the tracker values, in the container
    /// its parent and milestone give. False where the plan cannot hold it.
    fn restore(&mut self, link: &SyncLink) -> bool {
        let number = link.reference.number;
        let (kind, container, base, name, description, due) = match link.reference.kind {
            RefKind::Issue => {
                let Some(issue) = self.issues.get(&number).cloned() else { return false };
                let parent = match &issue.parent {
                    Some(p) if self.repo.holds_key(p) => {
                        match self.item_linked_to(RefKind::Issue, p.number) {
                            Some(id) => Some(id),
                            None => return false,
                        }
                    }
                    Some(_) => return false,
                    None => None,
                };
                let milestone = match issue.milestone {
                    Some(m) => match self.item_linked_to(RefKind::Milestone, m) {
                        Some(id) => Some(id),
                        None => return false,
                    },
                    None => None,
                };
                let hierarchy = Hierarchy::new(&self.doc.items);
                let container = values::place(&hierarchy, parent.as_deref(), milestone.as_deref());
                let container_kind = container.as_deref().and_then(|c| hierarchy.kind(c));
                let Some(kind) =
                    values::kind_for(self.settings, issue.issue_type.as_deref(), container_kind)
                else {
                    return false;
                };
                let links = values::own_links(&self.doc, &self.repo);
                let (d_parent, d_milestone) =
                    values::derive_place(&hierarchy, &links, container.as_deref());
                let wanted_parent = issue.parent.as_ref().map(values::key_id);
                if d_parent.as_ref().map(values::ref_id) != wanted_parent
                    || d_milestone != issue.milestone
                {
                    return false;
                }
                let base = SyncBase {
                    name: issue.title.clone(),
                    description: issue.body.clone(),
                    state: issue.state.clone(),
                    r#type: Some(issue.issue_type.clone()),
                    parent: Some(d_parent),
                    milestone: Some(d_milestone),
                    blocked_by: Some(Vec::new()),
                    due: None,
                };
                (kind, container, base, issue.title, issue.body, None)
            }
            RefKind::Milestone => {
                let Some(m) = self.milestones.get(&number).cloned() else { return false };
                // A milestone sits in a deliverable, and the tracker names
                // none: the first deliverable of the plan holds it.
                let Some(container) = self
                    .doc
                    .items
                    .iter()
                    .find(|i| i.kind == Kind::Deliverable)
                    .map(|i| i.id.clone())
                else {
                    return false;
                };
                let base = SyncBase {
                    name: m.title.clone(),
                    description: m.description.clone(),
                    state: m.state.clone(),
                    r#type: None,
                    parent: None,
                    milestone: None,
                    blocked_by: None,
                    due: Some(m.due.clone()),
                };
                (Kind::Milestone, Some(container), base, m.title, m.description, m.due)
            }
        };
        let id = if self.item(&link.item_id).is_none() {
            link.item_id.clone()
        } else {
            values::mint_item_id(&self.doc, kind)
        };
        let item = Item {
            id: id.clone(),
            kind,
            name,
            description: flow_description(&description),
            artifact_ids: None,
            parent_id: container.clone(),
            local_only: None,
            draft_id: None,
            footprint: None,
            due,
            position: values::position_below_last(&self.doc, container.as_deref()),
            size: values::default_size(kind),
        };
        self.doc.items.push(item);
        values::grow_to_fit(&mut self.doc, container.as_deref());
        if let Some(l) = self.link_mut(&link.item_id) {
            l.item_id = id.clone();
            l.base = base;
        }
        self.added_items.insert(id);
        true
    }
}

/// A reference of a link, as a conflict value.
pub(super) fn ref_value(r: Option<&TrackerRef>) -> Value {
    r.map(|r| serde_json::to_value(r).unwrap_or(Value::Null)).unwrap_or(Value::Null)
}

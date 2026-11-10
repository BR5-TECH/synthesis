//! FTS-FR-VFNM, FTS-FR-HDRR, FTS-FR-DNBY, FTS-FR-XITX: the tracker writes of
//! one call, made before the Flow is written. Each write brings its base field
//! up to date only once it is made, so a call that stops part way leaves the
//! next sync to complete it without a conflict.

use crate::flow_validation::model::{
    EdgeType, Hierarchy, Kind, RefKind, SyncBase, SyncLink, TrackerRef,
};

use super::adapter::{NewIssue, NewMilestone};
use super::engine::{Sync, Write};
use super::records::*;
use super::values;

impl Sync<'_> {
    /// Make every planned write, then the creations, then the blocked-by
    /// relations of the created items. The first failure stops the writes
    /// and is returned.
    pub(super) fn write_all(&mut self) -> Result<(), String> {
        // A parent or a milestone is written after the creations: an item the
        // Flow moved into a new container names that container's new issue.
        let (place, other): (Vec<Write>, Vec<Write>) = std::mem::take(&mut self.writes)
            .into_iter()
            .partition(|w| matches!(w, Write::Parent { .. } | Write::Milestone { .. }));
        for write in other {
            self.apply(write)?;
        }
        self.create_all()?;
        for write in place {
            let write = self.with_current_place(write);
            self.apply(write)?;
        }
        self.link_created_blockers()
    }

    /// The parent or the milestone the item's place gives now (FTS-FR-HLVJ),
    /// with the links of the creations of this call.
    fn with_current_place(&self, write: Write) -> Write {
        let place = |item_id: &str| {
            let hierarchy = Hierarchy::new(&self.doc.items);
            let links = values::own_links(&self.doc, &self.repo);
            let container = self.item(item_id).and_then(|i| i.parent_id.clone());
            values::derive_place(&hierarchy, &links, container.as_deref())
        };
        match write {
            Write::Parent { item_id, number, current, .. } => {
                let parent = place(&item_id).0;
                Write::Parent { item_id, number, current, parent }
            }
            Write::Milestone { item_id, number, .. } => {
                let milestone = place(&item_id).1;
                Write::Milestone { item_id, number, milestone }
            }
            other => other,
        }
    }

    fn apply(&mut self, write: Write) -> Result<(), String> {
        match write {
            Write::Issue { item_id, number, mut change } => {
                // FTS-FR-XITX: a Type the repository does not offer is not
                // written, and the issue is reported.
                if let Some(Some(name)) = change.issue_type.clone() {
                    match self.offered_type(&name) {
                        Some(spelling) => change.issue_type = Some(Some(spelling)),
                        None => {
                            change.issue_type = None;
                            let title = self
                                .issues
                                .get(&number)
                                .map(|i| i.title.clone())
                                .unwrap_or_default();
                            let url = self.issues.get(&number).map(|i| i.url.clone());
                            self.skip(
                                Some(number),
                                &title,
                                url.as_deref(),
                                SKIP_TYPE_UNAVAILABLE,
                                Some(&item_id),
                            );
                        }
                    }
                }
                if change.is_empty() {
                    return Ok(());
                }
                self.adapter.update_issue(number, &change)?;
                if let Some(l) = self.link_mut(&item_id) {
                    if let Some(title) = &change.title {
                        l.base.name = title.clone();
                    }
                    if let Some(body) = &change.body {
                        l.base.description = body.clone();
                    }
                    if let Some(t) = &change.issue_type {
                        l.base.r#type = Some(t.clone());
                    }
                }
                self.updated_items.insert(item_id);
            }
            Write::Parent { item_id, number, current, parent } => {
                let on_tracker =
                    self.issues.get(&number).and_then(|i| i.parent.as_ref()).map(values::key_id);
                if parent.as_ref().map(values::ref_id) == on_tracker {
                    if let Some(l) = self.link_mut(&item_id) {
                        l.base.parent = Some(parent);
                    }
                    return Ok(());
                }
                // A parent that is gone from the tracker waits for the author
                // to decide its conflict (FTS-FR-VXMN).
                if parent.as_ref().is_some_and(|p| self.missing_issues.contains(&p.number)) {
                    return Ok(());
                }
                // A parent in another repository is removed there.
                self.adapter.set_parent(
                    number,
                    current.as_ref(),
                    parent.as_ref().map(|p| p.number),
                )?;
                if let Some(l) = self.link_mut(&item_id) {
                    l.base.parent = Some(parent);
                }
                self.updated_items.insert(item_id);
            }
            Write::Milestone { item_id, number, milestone } => {
                if self.issues.get(&number).map(|i| i.milestone) == Some(milestone) {
                    if let Some(l) = self.link_mut(&item_id) {
                        l.base.milestone = Some(milestone);
                    }
                    return Ok(());
                }
                if milestone.is_some_and(|m| self.missing_milestones.contains(&m)) {
                    return Ok(());
                }
                self.adapter.set_milestone(number, milestone)?;
                if let Some(l) = self.link_mut(&item_id) {
                    l.base.milestone = Some(milestone);
                }
                self.updated_items.insert(item_id);
            }
            Write::AddBlocker { item_id, number, blocker } => {
                if self.missing_issues.contains(&blocker.number) {
                    return Ok(());
                }
                self.adapter.add_blocked_by(number, blocker.number)?;
                self.push_blocker(&item_id, blocker);
                self.updated_items.insert(item_id);
            }
            Write::RemoveBlocker { item_id, number, blocker } => {
                self.adapter.remove_blocked_by(number, blocker.number)?;
                if let Some(l) = self.link_mut(&item_id) {
                    if let Some(list) = l.base.blocked_by.as_mut() {
                        list.retain(|r| values::ref_id(r) != values::ref_id(&blocker));
                    }
                }
                self.updated_items.insert(item_id);
            }
            Write::MilestoneFields { item_id, number, change } => {
                self.adapter.update_milestone(number, &change)?;
                if let Some(l) = self.link_mut(&item_id) {
                    if let Some(title) = &change.title {
                        l.base.name = title.clone();
                    }
                    if let Some(description) = &change.description {
                        l.base.description = description.clone();
                    }
                    if let Some(due) = &change.due {
                        l.base.due = Some(due.clone());
                    }
                }
                self.updated_items.insert(item_id);
            }
        }
        Ok(())
    }

    fn push_blocker(&mut self, item_id: &str, blocker: TrackerRef) {
        if let Some(l) = self.link_mut(item_id) {
            let list = l.base.blocked_by.get_or_insert_with(Vec::new);
            if !list.iter().any(|r| values::ref_id(r) == values::ref_id(&blocker)) {
                list.push(blocker);
            }
        }
    }

    /// FTS-FR-HDRR: the items that get a tracker item, parents first: every
    /// item with no link that is not `localOnly` and was not unlinked in this
    /// call.
    pub(super) fn creation_order(&self) -> Vec<String> {
        let hierarchy = Hierarchy::new(&self.doc.items);
        let mut order: Vec<(usize, usize, String)> = self
            .doc
            .items
            .iter()
            .enumerate()
            .filter(|(_, i)| {
                i.local_only != Some(true)
                    && !self.no_create.contains(&i.id)
                    && self.link(&i.id).is_none()
            })
            .map(|(n, i)| (hierarchy.ancestors(&i.id).len(), n, i.id.clone()))
            .collect();
        order.sort();
        order.into_iter().map(|(_, _, id)| id).collect()
    }

    fn create_all(&mut self) -> Result<(), String> {
        for item_id in self.creation_order() {
            let Some(item) = self.item(&item_id).cloned() else { continue };
            let description = item.description.clone().unwrap_or_default();
            if item.kind == Kind::Milestone {
                let created = self.adapter.create_milestone(&NewMilestone {
                    title: item.name.clone(),
                    description: description.clone(),
                    due: item.due.clone(),
                })?;
                let base = SyncBase {
                    name: item.name.clone(),
                    description,
                    state: "open".into(),
                    r#type: None,
                    parent: None,
                    milestone: None,
                    blocked_by: None,
                    due: Some(item.due.clone()),
                };
                self.add_created_link(&item_id, RefKind::Milestone, created, base);
                continue;
            }
            // FTS-FR-XITX: a repository with no Types gets a plain issue. In
            // one with Types, an issue whose Type it does not offer is not
            // created.
            let issue_type = if self.plain_issues() {
                None
            } else {
                let wanted = values::type_for(self.settings, item.kind).unwrap_or_default();
                let Some(spelling) = self.offered_type(wanted) else {
                    self.skip(None, &item.name, None, SKIP_TYPE_UNAVAILABLE, Some(&item_id));
                    continue;
                };
                Some(spelling)
            };
            let hierarchy = Hierarchy::new(&self.doc.items);
            let links = values::own_links(&self.doc, &self.repo);
            let (parent, milestone) =
                values::derive_place(&hierarchy, &links, item.parent_id.as_deref());
            // A parent or a milestone that is gone from the tracker waits for
            // the author to decide its conflict (FTS-FR-VXMN).
            let parent = parent.filter(|p| !self.missing_issues.contains(&p.number));
            let milestone = milestone.filter(|m| !self.missing_milestones.contains(m));
            let created = self.adapter.create_issue(&NewIssue {
                title: item.name.clone(),
                body: description.clone(),
                issue_type: issue_type.clone(),
                milestone,
            })?;
            let number = created.number;
            let base = SyncBase {
                name: item.name.clone(),
                description,
                state: "open".into(),
                r#type: Some(issue_type),
                parent: Some(None),
                milestone: Some(milestone),
                blocked_by: Some(Vec::new()),
                due: None,
            };
            self.add_created_link(&item_id, RefKind::Issue, created, base);
            if let Some(parent) = parent {
                self.adapter.set_parent(number, None, Some(parent.number))?;
                if let Some(l) = self.link_mut(&item_id) {
                    l.base.parent = Some(Some(parent));
                }
            }
        }
        Ok(())
    }

    fn add_created_link(
        &mut self,
        item_id: &str,
        kind: RefKind,
        created: super::adapter::Created,
        base: SyncBase,
    ) {
        let reference = TrackerRef {
            kind,
            host: self.repo.host.clone(),
            owner: self.repo.owner.clone(),
            repo: self.repo.repo.clone(),
            number: created.number,
            url: created.url.clone(),
        };
        self.sync_mut().links.push(SyncLink { item_id: item_id.to_string(), reference, base });
        self.created_items.push(item_id.to_string());
        self.created_urls.push(created.url);
    }

    /// FTS-FR-DNBY: the blocked-by relations of every `blocks` edge that has
    /// a created item at one end and a linked issue at the other.
    fn link_created_blockers(&mut self) -> Result<(), String> {
        let created: Vec<String> = self.created_items.clone();
        let edges: Vec<(String, String)> = self
            .doc
            .edges
            .iter()
            .filter(|e| e.edge_type == EdgeType::Blocks)
            .filter(|e| created.contains(&e.from) || created.contains(&e.to))
            .map(|e| (e.from.clone(), e.to.clone()))
            .collect();
        for (from, to) in edges {
            let issue_link = |id: &str| {
                self.link(id)
                    .filter(|l| l.reference.kind == RefKind::Issue && self.repo.holds(&l.reference))
                    .map(|l| l.reference.clone())
            };
            let (Some(blocker), Some(blocked)) = (issue_link(&from), issue_link(&to)) else {
                continue;
            };
            if self.missing_issues.contains(&blocker.number)
                || self.missing_issues.contains(&blocked.number)
            {
                continue;
            }
            self.adapter.add_blocked_by(blocked.number, blocker.number)?;
            self.push_blocker(&to, blocker);
            self.updated_items.insert(to);
        }
        Ok(())
    }
}

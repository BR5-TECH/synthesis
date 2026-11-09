//! FTS-FR-EGJL, FTS-FR-CJVS, FTS-FR-LKZO: the sub-issues of linked issues that
//! the Flow does not hold come into the Flow.

use std::collections::HashSet;

use crate::flow_validation::model::{Hierarchy, Item, RefKind, SyncBase, SyncLink, TrackerRef};

use super::adapter::TrackerIssue;
use super::engine::{flow_description, Sync};
use super::records::*;
use super::values;

/// The most parents one sub-issue read asks for.
const SUB_ISSUE_BATCH: usize = 10;

impl Sync<'_> {
    /// Pull every sub-issue that no link names, level by level, so the
    /// sub-issues of a pulled container are pulled in the same sync.
    pub(super) fn pull_all(&mut self) -> Result<(), String> {
        let mut named: HashSet<u64> = self
            .links()
            .iter()
            .filter(|l| l.reference.kind == RefKind::Issue && self.repo.holds(&l.reference))
            .map(|l| l.reference.number)
            .chain(self.released.iter().copied())
            .collect();
        // Every linked issue, leaves too: a sub-issue of a leaf has no kind
        // that fits, and is reported (FTS-FR-CJVS).
        let mut parents: Vec<u64> = self
            .links()
            .iter()
            .filter(|l| {
                l.reference.kind == RefKind::Issue
                    && self.repo.holds(&l.reference)
                    && self.issues.contains_key(&l.reference.number)
                    && self.item(&l.item_id).is_some()
            })
            .map(|l| l.reference.number)
            .collect();
        let mut guard = 0;
        while !parents.is_empty() && guard <= self.doc.items.len() + 1 {
            guard += 1;
            let mut next = Vec::new();
            for chunk in parents.chunks(SUB_ISSUE_BATCH) {
                for (parent, children) in self.adapter.read_sub_issues(chunk)? {
                    for child in children {
                        if let Some(number) = self.pull_one(parent, child, &mut named) {
                            next.push(number);
                        }
                    }
                }
            }
            parents = next;
        }
        Ok(())
    }

    /// Pull one sub-issue of `parent`. Returns its number where it is pulled,
    /// so its own sub-issues are read next.
    fn pull_one(
        &mut self,
        parent: u64,
        child: TrackerIssue,
        named: &mut HashSet<u64>,
    ) -> Option<u64> {
        if !child.owner.eq_ignore_ascii_case(&self.repo.owner)
            || !child.repo.eq_ignore_ascii_case(&self.repo.repo)
        {
            self.skip(Some(child.number), &child.title, SKIP_FOREIGN_REPOSITORY, None);
            return None;
        }
        if !named.insert(child.number) {
            return None;
        }
        let parent_item = self.item_linked_to(RefKind::Issue, parent)?;
        let milestone_item =
            child.milestone.and_then(|m| self.item_linked_to(RefKind::Milestone, m));
        let hierarchy = Hierarchy::new(&self.doc.items);
        let container =
            values::place(&hierarchy, Some(parent_item.as_str()), milestone_item.as_deref());
        let container_kind = container.as_deref().and_then(|c| hierarchy.kind(c));
        let Some(kind) =
            values::kind_for(self.settings, child.issue_type.as_deref(), container_kind)
        else {
            self.skip(Some(child.number), &child.title, SKIP_NO_KIND_FITS, None);
            return None;
        };
        let links = values::own_links(&self.doc, &self.repo);
        let (d_parent, d_milestone) =
            values::derive_place(&hierarchy, &links, container.as_deref());
        drop(hierarchy);
        let id = values::mint_item_id(&self.doc, kind);
        let item = Item {
            id: id.clone(),
            kind,
            name: child.title.clone(),
            description: flow_description(&child.body),
            artifact_ids: None,
            parent_id: container.clone(),
            local_only: None,
            draft_id: None,
            footprint: None,
            due: None,
            position: values::position_below_last(&self.doc, container.as_deref()),
            size: values::default_size(kind),
        };
        self.doc.items.push(item);
        values::grow_to_fit(&mut self.doc, container.as_deref());
        // The base holds the tracker values, except the parent and the
        // milestone, which hold the values of the place the item now has. A
        // milestone the plan cannot give the item then reads as a tracker
        // change that the plan refuses (FTS-FR-SZGF), and nothing is written.
        let base = SyncBase {
            name: child.title.clone(),
            description: child.body.clone(),
            state: child.state.clone(),
            r#type: Some(child.issue_type.clone()),
            parent: Some(d_parent),
            milestone: Some(d_milestone),
            blocked_by: Some(Vec::new()),
            due: None,
        };
        let reference = TrackerRef {
            kind: RefKind::Issue,
            host: self.repo.host.clone(),
            owner: self.repo.owner.clone(),
            repo: self.repo.repo.clone(),
            number: child.number,
            url: child.url.clone(),
        };
        self.sync_mut().links.push(SyncLink { item_id: id.clone(), reference, base });
        self.added_items.insert(id);
        let number = child.number;
        self.issues.insert(number, child);
        Some(number)
    }
}

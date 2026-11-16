//! The tracker edits `set_labels`, `close_item`, `set_status`, `set_parent`,
//! and `set_milestone`, and the extra fields of `create_item`
//! (`GDP-github-delivery-plan.md` GDP-FR-ICDW to GDP-FR-RMNF).
//!
//! Each edit follows the rules of the other tracker edits: it reads the item
//! and refuses a changed one (GDP-FR-GBDF), writes through the tracker adapter,
//! reads the item again, and stops at the first failed write (GDP-FR-MSVS).

use super::*;

fn label_unknown(name: &str) -> DeliveryError {
    refusal(ERR_LABEL_UNKNOWN, &format!("The repository has no label named \"{name}\"."))
}

fn status_unknown(name: &str) -> DeliveryError {
    refusal(
        ERR_STATUS_UNKNOWN,
        &format!("The Status field of the Project has no option named \"{name}\"."),
    )
}

impl TrackerEdit<'_> {
    /// GDP-FR-ICDW: the repository spelling of each label name in `names`,
    /// without a repeat. A name that no label has refuses with `label_unknown`.
    /// No label is created. With no name, no request is made.
    pub(super) fn canonical_labels(&self, names: &[String]) -> Result<Vec<String>, DeliveryError> {
        if names.is_empty() {
            return Ok(Vec::new());
        }
        let known = self.adapter.list_labels().map_err(|c| github_error(&c))?;
        let mut found: Vec<String> = Vec::new();
        for name in names {
            let label = known
                .iter()
                .find(|k| k.eq_ignore_ascii_case(name.trim()))
                .ok_or_else(|| label_unknown(name))?;
            if !found.contains(label) {
                found.push(label.clone());
            }
        }
        Ok(found)
    }

    /// GDP-FR-RMNF: the parent and the milestone of a new issue. A given
    /// `parent_id` or `milestone_id` replaces the value of the container rule
    /// (GDP-FR-NJJZ); one that is not given keeps it.
    pub(super) fn places(
        &self,
        container_id: Option<&str>,
        parent_id: Option<&str>,
        milestone_id: Option<&str>,
    ) -> Result<(Option<u64>, Option<u64>), DeliveryError> {
        let (mut parent, mut milestone) = containers(self.plan, container_id)?;
        if let Some(id) = parent_id {
            parent = Some(self.issue_number(id)?);
        }
        if let Some(id) = milestone_id {
            milestone = Some(self.milestone_number(id)?);
        }
        Ok((parent, milestone))
    }

    /// The number of the issue item `id`, or a refusal where it is no issue.
    fn issue_number(&self, id: &str) -> Result<u64, DeliveryError> {
        match self.item(id)? {
            PlanRef::Issue(n) => Ok(n),
            PlanRef::Milestone(_) => Err(kind_unsupported()),
        }
    }

    /// The number of the milestone item `id`, or a refusal where it is no
    /// milestone.
    fn milestone_number(&self, id: &str) -> Result<u64, DeliveryError> {
        match self.item(id)? {
            PlanRef::Milestone(m) => Ok(m),
            PlanRef::Issue(_) => Err(kind_unsupported()),
        }
    }

    /// GDP-FR-ICDW: add and remove labels of an issue. A label that the issue
    /// has already is not added again, and a label it lacks is not removed.
    pub(super) fn set_labels(
        &mut self,
        item_id: &str,
        add: &[String],
        remove: &[String],
        updated_at: &str,
    ) -> Result<(), DeliveryError> {
        let n = self.issue_number(item_id)?;
        let issue = PlanRef::Issue(n);
        self.check_fresh(issue, updated_at)?;
        let add = self.canonical_labels(add)?;
        let remove = self.canonical_labels(remove)?;
        if add.is_empty() && remove.is_empty() {
            return Ok(());
        }
        let current = self
            .adapter
            .read_issue_detail(n, self.project_id)
            .map_err(|c| github_error(&c))?
            .ok_or_else(not_found)?
            .labels;
        let has = |name: &String| current.iter().any(|c| c.eq_ignore_ascii_case(name));
        let additions: Vec<String> = add.iter().filter(|l| !has(l)).cloned().collect();
        let removals: Vec<String> =
            remove.iter().filter(|l| has(l) && !add.contains(l)).cloned().collect();
        self.with_refresh(&[issue], |e, w| {
            for label in &removals {
                w.run(e.adapter.remove_label(n, label))?;
            }
            if !additions.is_empty() {
                w.run(e.adapter.add_labels(n, &additions))?;
            }
            Ok(())
        })
    }

    /// GDP-FR-MVAA: close an issue with a reason. It stays on the Project.
    pub(super) fn close_item(
        &mut self,
        item_id: &str,
        reason: CloseReason,
        updated_at: &str,
    ) -> Result<(), DeliveryError> {
        let n = self.issue_number(item_id)?;
        let issue = PlanRef::Issue(n);
        self.check_fresh(issue, updated_at)?;
        if self.read.issue(n).is_some_and(|i| i.state == "closed") {
            return Err(refusal(ERR_NO_CHANGE, "The issue is closed already."));
        }
        let with = match reason {
            CloseReason::Completed => CloseWith::Completed,
            CloseReason::NotPlanned => CloseWith::NotPlanned,
        };
        self.with_refresh(&[issue], |e, w| w.run(e.adapter.close_issue(n, with)))
    }

    /// GDP-FR-KOWL: set the `Status` of an issue on the selected Project.
    pub(super) fn set_status(
        &mut self,
        item_id: &str,
        status: &str,
        updated_at: &str,
    ) -> Result<(), DeliveryError> {
        let n = self.issue_number(item_id)?;
        let issue = PlanRef::Issue(n);
        self.check_fresh(issue, updated_at)?;
        let field = self
            .adapter
            .read_status_field(self.project_id)
            .map_err(|c| github_error(&c))?
            .ok_or_else(|| status_unknown(status))?;
        let option = field
            .options
            .iter()
            .find(|o| o.name.eq_ignore_ascii_case(status.trim()))
            .ok_or_else(|| status_unknown(status))?
            .clone();
        let project_item = self.read.item_id_of(n).map(str::to_string).ok_or_else(not_found)?;
        let mut writes = Writes::default();
        writes.run(self.adapter.set_project_status(
            self.project_id,
            &project_item,
            &field.field_id,
            &option.id,
        ))?;
        self.read.set_status(n, Some(option.name));
        self.refresh(&[issue]);
        Ok(())
    }

    /// GDP-FR-RMNF: set or remove the parent issue of an issue, whatever its
    /// container or milestone.
    pub(super) fn set_parent_of(
        &mut self,
        item_id: &str,
        parent_id: Option<&str>,
        updated_at: &str,
    ) -> Result<(), DeliveryError> {
        let n = self.issue_number(item_id)?;
        let parent = match parent_id {
            Some(id) => {
                let parent = self.issue_number(id)?;
                let hierarchy = Hierarchy::new(&self.plan.document.items);
                if parent == n || hierarchy.is_ancestor(item_id, id) {
                    // An issue cannot be its own parent or the parent of its
                    // own ancestor.
                    return Err(kind_unsupported());
                }
                Some(parent)
            }
            None => None,
        };
        self.check_fresh(PlanRef::Issue(n), updated_at)?;
        let issue = self.read.issue(n).cloned().ok_or_else(not_found)?;
        let own_parent = issue
            .parent
            .as_ref()
            .filter(|p| self.read.repository.is(&p.owner, &p.repo))
            .map(|p| p.number);
        if own_parent == parent && (parent.is_some() || issue.parent.is_none()) {
            return Ok(());
        }
        let mut changed = vec![PlanRef::Issue(n)];
        changed.extend(parent.into_iter().chain(own_parent).map(PlanRef::Issue));
        self.with_refresh(&changed, |e, w| {
            w.run(e.adapter.set_parent(n, issue.parent.as_ref(), parent))
        })
    }

    /// GDP-FR-RMNF: set or remove the milestone of an issue, whatever its
    /// container or parent.
    pub(super) fn set_milestone_of(
        &mut self,
        item_id: &str,
        milestone_id: Option<&str>,
        updated_at: &str,
    ) -> Result<(), DeliveryError> {
        let n = self.issue_number(item_id)?;
        let milestone = milestone_id.map(|id| self.milestone_number(id)).transpose()?;
        self.check_fresh(PlanRef::Issue(n), updated_at)?;
        if self.read.issue(n).ok_or_else(not_found)?.milestone == milestone {
            return Ok(());
        }
        self.with_refresh(&[PlanRef::Issue(n)], |e, w| w.run(e.adapter.set_milestone(n, milestone)))
    }
}

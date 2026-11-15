//! `publish_delivery_to_github()` (`DSR-delivery-source.md` DSR-FR-RCLA to
//! DSR-FR-WEVP): move the delivery plan of a Flow file to the selected GitHub
//! Project.
//!
//! The items are created through the `create_item` edit of
//! `GDP-github-delivery-plan.md`, containers before their members, and then
//! the `blocks` edges through the `add_edge` edit. Each created item and
//! relation goes into the publish journal before the next request, so a
//! failed publish is resumed and never makes a second copy of the plan.

use std::collections::HashMap;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Manager, Runtime};

use crate::delivery::github::edits::{apply_for_target, GithubDeliveryEdit};
use crate::delivery::github::plan::{parse_item_id, PlanRef};
use crate::delivery::github::{self, GithubDeliveryPlan, GithubDeliveryState, GithubTarget};
use crate::delivery::machine_state::{
    load_layout, read_delivery_overlay, read_publish_journal, remove_publish_journal, save_layout,
    write_delivery_overlay, write_publish_journal, CreatedItem, CreatedItemKind, CreatedRelation,
    PublishJournal, RelatesEdge,
};
use crate::delivery::settings::DeliverySourceChoice;
use crate::delivery::DeliveryError;
use crate::flow_validation::model::{parse_valid, EdgeType, FlowDocument, Hierarchy, Item, Kind};
use crate::logging::LogLevel;

use super::{
    log, project_root, refuse, resolve_local, Resolved, ERR_FLOW_INVALID, ERR_NOT_A_FLOW,
    ERR_NOT_FILE_SOURCE,
};

/// DSR contract surface `PublishResult`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishResult {
    pub milestones: usize,
    pub issues: usize,
    pub relations: usize,
    pub created_urls: Vec<String>,
}

/// DSR-FR-GXSE: the items of `doc`, containers before their members, and in
/// document order within one depth. Pure.
pub fn containers_first(doc: &FlowDocument) -> Vec<&Item> {
    let hierarchy = Hierarchy::new(&doc.items);
    let mut items: Vec<(usize, usize, &Item)> = doc
        .items
        .iter()
        .enumerate()
        .map(|(i, item)| (hierarchy.ancestors(&item.id).len(), i, item))
        .collect();
    items.sort_by_key(|(depth, i, _)| (*depth, *i));
    items.into_iter().map(|(_, _, item)| item).collect()
}

/// The body of the Flow file `flow_id`.
fn read_flow(root: &crate::fs::RootFs, flow_id: &str) -> Result<String, DeliveryError> {
    let missing = || DeliveryError::new(ERR_NOT_A_FLOW, "The delivery file cannot be read.");
    let path = crate::fs::resolve_under(root.path(), flow_id).map_err(|_| missing())?;
    root.read_text(&path).map_err(|_| missing())
}

/// DSR-FR-GXSE: a valid Flow, or `flow_invalid` with its report.
pub(crate) fn read_valid_flow(
    root: &crate::fs::RootFs,
    flow_id: &str,
) -> Result<FlowDocument, DeliveryError> {
    let body = read_flow(root, flow_id)?;
    parse_valid(&body).map_err(|report| {
        DeliveryError::new(ERR_FLOW_INVALID, "The Flow is not a valid delivery plan.")
            .with_report(report)
    })
}

/// The plan id a journal entry names.
fn plan_id(created: &CreatedItem) -> String {
    match created.kind {
        CreatedItemKind::Issue => github::plan::issue_id(created.number),
        CreatedItemKind::Milestone => github::plan::milestone_id(created.number),
    }
}

/// The steps of a publish after its checks.
struct Run<'a, R: Runtime> {
    app: &'a AppHandle<R>,
    target: &'a GithubTarget,
    store: crate::fs::RootFs,
    flow_id: &'a str,
    journal: PublishJournal,
    result: PublishResult,
    plan: Option<GithubDeliveryPlan>,
    /// The partial creates of an earlier publish that this one finished.
    completed: usize,
}

impl<R: Runtime> Run<'_, R> {
    fn edit(&mut self, edit: GithubDeliveryEdit) -> Result<Option<String>, DeliveryError> {
        let (plan, id) = apply_for_target(self.app, self.target, &edit)?;
        self.plan = Some(plan);
        Ok(id)
    }

    /// The tracker facts of `item_id` in the last plan.
    fn facts(&self, item_id: &str) -> Option<&github::plan::PlanTrackerFacts> {
        self.plan.as_ref()?.tracker.iter().find(|t| t.item_id == item_id)
    }

    fn mapped(&self, flow_item: &str) -> Option<String> {
        self.journal.items.get(flow_item).map(plan_id)
    }

    /// DSR-FR-GXSE, DSR-FR-ULON: create each item that the journal does not
    /// hold, containers first.
    fn create_items(&mut self, doc: &FlowDocument) -> Result<(), DeliveryError> {
        for item in containers_first(doc) {
            if let Some(entry) = self.journal.items.get(&item.id).cloned() {
                if entry.complete || self.complete_item(item, &entry)? {
                    continue;
                }
                // The partly created issue is gone from GitHub, and its entry
                // is gone from the journal: the item is created again.
            }
            let container_id = item.parent_id.as_deref().and_then(|p| self.mapped(p));
            let created = self.edit(GithubDeliveryEdit::CreateItem {
                item_kind: item.kind,
                name: item.name.clone(),
                container_id,
                description: item.description.clone(),
                due: item.due.clone(),
                parent_id: None,
                milestone_id: None,
                labels: Vec::new(),
            });
            let new_id = match created {
                Ok(id) => id,
                Err(error) => {
                    // DSR-FR-ULON: an issue that a partial create made is
                    // journalled, so a retry does not make it again. It is
                    // not complete: a retry finishes its missing steps.
                    let url = error.created_urls.as_ref().and_then(|u| u.first()).cloned();
                    let number = url
                        .as_deref()
                        .and_then(|u| u.rsplit('/').next())
                        .and_then(|n| n.parse().ok());
                    if let (Some(url), Some(number)) = (url, number) {
                        let entry = CreatedItem {
                            kind: CreatedItemKind::Issue,
                            number,
                            url: url.clone(),
                            node_id: None,
                            complete: false,
                        };
                        self.journal.items.insert(item.id.clone(), entry);
                        write_publish_journal(&self.store, self.flow_id, &self.journal)?;
                        self.result.issues += 1;
                        self.result.created_urls.push(url);
                    }
                    return Err(error);
                }
            };
            let Some(new_id) = new_id else { continue };
            let (kind, number) = match parse_item_id(&new_id) {
                Some(PlanRef::Issue(n)) => (CreatedItemKind::Issue, n),
                Some(PlanRef::Milestone(n)) => (CreatedItemKind::Milestone, n),
                None => continue,
            };
            let url = self.facts(&new_id).map(|f| f.reference.url.clone()).unwrap_or_default();
            self.journal.items.insert(
                item.id.clone(),
                CreatedItem { kind, number, url: url.clone(), node_id: None, complete: true },
            );
            write_publish_journal(&self.store, self.flow_id, &self.journal)?;
            match kind {
                CreatedItemKind::Milestone => self.result.milestones += 1,
                CreatedItemKind::Issue => self.result.issues += 1,
            }
            self.result.created_urls.push(url);
        }
        Ok(())
    }

    /// DSR-FR-ULON, DSR-FR-GXSE: finish an issue that a partial create made.
    /// An issue that is not on the Project is added to it and placed in its
    /// containers through the `add_existing_issue` edit; an issue that is on
    /// the Project is placed through the `move` edit. The entry is complete
    /// after that, so its members can name it as their container.
    ///
    /// The publish reads the Project before this step, so the `move` edit
    /// names the `updated_at` of that read. A change on GitHub between two
    /// publishes thus does not refuse the completion; GDP-FR-GBDF refuses
    /// only a change between that read and the write.
    ///
    /// An issue that is not on GitHub now (deleted after the partial create)
    /// cannot be finished: its entry goes from the journal, and the answer is
    /// `false`, so the caller creates the item again.
    fn complete_item(&mut self, item: &Item, entry: &CreatedItem) -> Result<bool, DeliveryError> {
        if entry.kind == CreatedItemKind::Issue {
            let id = plan_id(entry);
            let container_id = item.parent_id.as_deref().and_then(|p| self.mapped(p));
            if self.plan.is_none() {
                self.plan = Some(github::load_target(self.app, self.target, false)?);
            }
            let edit = match self.facts(&id).map(|f| f.updated_at.clone()) {
                Some(updated_at) => {
                    GithubDeliveryEdit::Move { item_id: id, container_id, updated_at }
                }
                None if !self.issue_exists(entry.number)? => {
                    self.journal.items.remove(&item.id);
                    write_publish_journal(&self.store, self.flow_id, &self.journal)?;
                    log(
                        self.app,
                        LogLevel::Warn,
                        true,
                        "delivery publish item gone from tracker",
                        crate::log_fields! { "number" => entry.number },
                    );
                    return Ok(false);
                }
                None => GithubDeliveryEdit::AddExistingIssue { number: entry.number, container_id },
            };
            let kind = edit.name();
            self.edit(edit)?;
            log(
                self.app,
                LogLevel::Info,
                true,
                "delivery publish item completed",
                crate::log_fields! { "edit" => kind },
            );
        }
        if let Some(stored) = self.journal.items.get_mut(&item.id) {
            stored.complete = true;
        }
        write_publish_journal(&self.store, self.flow_id, &self.journal)?;
        self.completed += 1;
        Ok(true)
    }

    /// Whether the issue `number` is on GitHub now.
    fn issue_exists(&self, number: u64) -> Result<bool, DeliveryError> {
        let adapter = github::connect(self.app)?;
        let issue = adapter.read_issue(number).map_err(|c| github::github_error(&c))?;
        Ok(issue.is_some())
    }

    /// DSR-FR-ULON: the URL of each item that the publish created, in this
    /// run or in an earlier one. The items of the Flow come in the order of
    /// their creation, and then each entry of an item that the Flow no longer
    /// holds.
    fn journal_urls(&self, doc: &FlowDocument) -> Vec<String> {
        let ordered = containers_first(doc);
        let in_flow = ordered.iter().filter_map(|i| self.journal.items.get(&i.id));
        let others = self
            .journal
            .items
            .iter()
            .filter(|(id, _)| !doc.items.iter().any(|i| &i.id == *id))
            .map(|(_, entry)| entry);
        in_flow.chain(others).map(|e| e.url.clone()).filter(|u| !u.is_empty()).collect()
    }

    /// DSR-FR-GXSE, DSR-FR-ULON: add each `blocks` edge that the journal does
    /// not hold.
    fn create_relations(&mut self, doc: &FlowDocument) -> Result<(), DeliveryError> {
        for edge in doc.edges.iter().filter(|e| e.edge_type == EdgeType::Blocks) {
            if self.journal.edges.contains_key(&edge.id) {
                continue;
            }
            let (Some(from), Some(to)) = (self.mapped(&edge.from), self.mapped(&edge.to)) else {
                continue;
            };
            if self.plan.is_none() {
                self.plan = Some(github::load_target(self.app, self.target, false)?);
            }
            let updated_at = self.facts(&to).map(|f| f.updated_at.clone()).unwrap_or_default();
            self.edit(GithubDeliveryEdit::AddEdge {
                from: from.clone(),
                to: to.clone(),
                edge_type: EdgeType::Blocks,
                updated_at,
            })?;
            let number = |id: &str| match parse_item_id(id) {
                Some(PlanRef::Issue(n)) => n,
                _ => 0,
            };
            self.journal.edges.insert(
                edge.id.clone(),
                CreatedRelation { blocked: number(&to), blocking: number(&from) },
            );
            write_publish_journal(&self.store, self.flow_id, &self.journal)?;
            self.result.relations += 1;
        }
        Ok(())
    }

    /// DSR-FR-PTIT: the references, footprints, draft links, `relates` edges,
    /// milestone order, and layout of the Flow go into the per-machine state,
    /// under the ids of the created items.
    fn carry_machine_state(&self, doc: &FlowDocument) -> Result<(), DeliveryError> {
        let node = &self.target.project_node_id;
        let mut overlay =
            read_delivery_overlay(self.app, github::buffer_of(self.app), &self.store, node)?;
        for item in &doc.items {
            let Some(id) = self.mapped(&item.id) else { continue };
            if item.artifact_ids.is_none() && item.footprint.is_none() && item.draft_id.is_none() {
                continue;
            }
            let entry = overlay.items.entry(id).or_default();
            entry.artifact_ids = item.artifact_ids.clone();
            entry.footprint = item.footprint.clone();
            entry.draft_id = item.draft_id.clone();
        }
        for edge in doc.edges.iter().filter(|e| e.edge_type == EdgeType::Relates) {
            if let (Some(from), Some(to)) = (self.mapped(&edge.from), self.mapped(&edge.to)) {
                if !overlay.relates.iter().any(|r| r.from == from && r.to == to) {
                    overlay.relates.push(RelatesEdge { from, to });
                }
            }
        }
        if let Some(references) = &doc.references {
            overlay.references = references.clone();
        }
        // DSR-FR-PTIT, DMS-FR-WAUS: the file's milestone order is the document
        // order of its milestone items, under the ids of the created items.
        overlay.milestone_order = doc
            .items
            .iter()
            .filter(|i| i.kind == Kind::Milestone)
            .filter_map(|i| self.mapped(&i.id))
            .collect();
        write_delivery_overlay(self.app, github::buffer_of(self.app), &self.store, node, &overlay)?;

        let source = load_layout(
            self.app,
            github::buffer_of(self.app),
            &self.store,
            &format!("flow:{}", self.flow_id),
        )?;
        if !source.items.is_empty() {
            let scope = format!("github:{node}");
            let mut layout =
                load_layout(self.app, github::buffer_of(self.app), &self.store, &scope)?;
            for (id, entry) in source.items {
                if let Some(mapped) = self.mapped(&id) {
                    layout.items.insert(mapped, entry);
                }
            }
            save_layout(&self.store, &scope, &layout)?;
        }
        Ok(())
    }
}

/// DSR-FR-RCLA to DSR-FR-WEVP: the publish, on the calling thread.
pub fn publish_impl<R: Runtime>(app: &AppHandle<R>) -> Result<PublishResult, DeliveryError> {
    let started = Instant::now();
    let op = "publish_delivery_to_github";
    log(app, LogLevel::Info, true, "delivery publish started", crate::log_fields! {});
    let fail = |e| refuse(app, op, started, e);
    let root = project_root(app).map_err(fail)?;
    let (mut settings, resolved) = resolve_local(&root).map_err(fail)?;
    let Resolved::File { flow_id } = resolved else {
        return Err(fail(DeliveryError::new(
            ERR_NOT_FILE_SOURCE,
            "The delivery source of the project is not a Flow file.",
        )));
    };
    let target = github::target_of_selected(app).map_err(fail)?;
    let doc = read_valid_flow(&root, &flow_id).map_err(fail)?;
    // GDP-FR-YGXA refuses a `blocks` edge with a milestone at either end, so
    // such a plan is refused before anything is created.
    let kinds: HashMap<&str, Kind> = doc.items.iter().map(|i| (i.id.as_str(), i.kind)).collect();
    let milestone_edge = doc.edges.iter().filter(|e| e.edge_type == EdgeType::Blocks).any(|e| {
        kinds.get(e.from.as_str()) == Some(&Kind::Milestone)
            || kinds.get(e.to.as_str()) == Some(&Kind::Milestone)
    });
    if milestone_edge {
        return Err(fail(DeliveryError::new(
            github::ERR_MILESTONE_EDGE_UNSUPPORTED,
            "A blocks edge of the Flow has a milestone at one end.",
        )));
    }
    let store = github::machine_store(app).map_err(fail)?;
    let state = app.state::<GithubDeliveryState>();
    let _turn = state.edits.enter();
    // A read first: it checks the configuration (GPP-FR-DEZO) before anything
    // is written, and it gives the edits a read cache.
    github::shared_read(app, &target).map_err(fail)?;
    let journal =
        read_publish_journal(app, github::buffer_of(app), &store, &flow_id).map_err(fail)?;
    let resumed = journal.items.len() + journal.edges.len();
    let mut run = Run {
        app,
        target: &target,
        store,
        flow_id: &flow_id,
        journal,
        result: PublishResult::default(),
        plan: None,
        completed: 0,
    };
    let steps = run
        .create_items(&doc)
        .and_then(|_| run.create_relations(&doc))
        .and_then(|_| run.carry_machine_state(&doc));
    if let Err(mut error) = steps {
        // DSR-FR-ULON: the journal stays, and the error names each item that
        // the publish created, also those of an earlier run.
        error.created_urls = Some(run.journal_urls(&doc));
        log(
            app,
            LogLevel::Error,
            true,
            "delivery publish failed",
            crate::log_fields! {
                "code" => &error.code,
                "milestones" => run.result.milestones,
                "issues" => run.result.issues,
                "relations" => run.result.relations,
                "ms" => started.elapsed().as_millis() as u64,
            },
        );
        return Err(error);
    }
    // DSR-FR-WEVP: the source is GitHub now, the journal goes, and the file
    // stays on disk unchanged.
    settings.source = DeliverySourceChoice::Github;
    super::store_settings(&root, &settings).map_err(fail)?;
    remove_publish_journal(&run.store, &flow_id).map_err(fail)?;
    log(
        app,
        LogLevel::Info,
        true,
        "delivery published",
        crate::log_fields! {
            "milestones" => run.result.milestones,
            "issues" => run.result.issues,
            "relations" => run.result.relations,
            "resumed" => resumed,
            "completed" => run.completed,
            "ms" => started.elapsed().as_millis() as u64,
        },
    );
    super::announce(app);
    Ok(run.result)
}

/// DSR contract surface: `"publish delivery to github"`.
#[tauri::command]
pub async fn publish_delivery_to_github(app: AppHandle) -> Result<PublishResult, DeliveryError> {
    super::off_thread(move || publish_impl(&app)).await
}

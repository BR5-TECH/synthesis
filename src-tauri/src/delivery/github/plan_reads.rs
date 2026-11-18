//! In-process reads of the GitHub delivery plan for other modules
//! (`GDP-github-delivery-plan.md` GDP-FR-QFQF, GDP-FR-LKXA). The plan issue
//! read tools of the Map discussion call these helpers. None of them is a
//! Tauri command, and none of them changes GitHub.
//!
//! A **plan issue** is an issue that the plan holds as an item: an issue of the
//! polling repository on the selected Project with a mapped kind. A pull
//! request, an excluded issue, an unmapped issue, and an issue of another
//! repository are never plan issues.
//!
//! How to call them:
//!
//! - [`list_plan_issues`] and [`search_plan_issues`] are synchronous. They
//!   answer from the read cache and make no request. Where no cache exists,
//!   they make one GitHub read of the Project, so call them from a blocking
//!   context (for example `tauri::async_runtime::spawn_blocking`).
//! - [`plan_catalog`] and [`read_plan_issue_detail`] are asynchronous. They
//!   run their blocking work on a worker thread.
//!
//! Every function refuses with `no_project_open`, `not_github_source`, or the
//! configuration or request error of the plan, as a load of the plan does. No
//! log record of this module carries a title, a body, a label name, or a query
//! (GDP-FR-QHQR).

use std::collections::HashSet;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Runtime};

use crate::delivery::source;
use crate::delivery::DeliveryError;
use crate::logging::LogLevel;

use super::adapter::{IssueDetail, ItemContent, StateReason, TrackerIssue};
use super::cache::TrackerRead;
use super::plan::{build_plan, issue_id, BuiltPlan};
use super::*;

/// The longest snippet of a search hit, in characters.
const SNIPPET_CHARS: usize = 160;

/// One milestone of the plan catalog.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CatalogMilestone {
    pub number: u64,
    pub title: String,
    /// `open` or `closed`.
    pub state: String,
}

/// What a plan can be changed to: the names that an edit accepts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanCatalog {
    /// Every milestone of the repository, open and closed.
    pub milestones: Vec<CatalogMilestone>,
    /// The issue Types the repository offers. Where that read fails, the Types
    /// that the plan issues carry.
    pub types: Vec<String>,
    /// The options of the Project `Status` field, in the order of the field.
    pub statuses: Vec<String>,
    /// The label names of the repository, or `None` where they cannot be read.
    pub labels: Option<Vec<String>>,
}

/// One plan issue as a list shows it. `labels` is empty: the read cache holds
/// no labels, and [`read_plan_issue_detail`] fills them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanIssueRow {
    pub number: u64,
    pub title: String,
    pub issue_type: Option<String>,
    /// `open` or `closed`.
    pub state: String,
    /// `completed` or `not_planned` for a closed issue, else `None`.
    pub close_reason: Option<String>,
    /// The title of the milestone.
    pub milestone: Option<String>,
    /// The number of the parent issue, where the parent is in the repository.
    pub parent: Option<u64>,
    pub status: Option<String>,
    pub labels: Vec<String>,
}

/// The filters of [`list_plan_issues`]. A `None` field filters nothing. Text
/// filters compare without case.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlanIssueFilter {
    /// `open` or `closed`.
    pub state: Option<String>,
    pub issue_type: Option<String>,
    /// A milestone title.
    pub milestone: Option<String>,
    pub status: Option<String>,
    /// A label name. A row holds no label, so a label filter matches no row.
    pub label: Option<String>,
    /// The number of the parent issue.
    pub parent: Option<u64>,
    /// At most this many rows. `None` is no limit.
    pub limit: Option<usize>,
    pub offset: usize,
}

/// One page of [`list_plan_issues`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanIssuePage {
    /// The number of plan issues that match the filters, before the page.
    pub total: usize,
    pub offset: usize,
    /// The instant of the GitHub read that the cache holds.
    pub read_at: String,
    pub rows: Vec<PlanIssueRow>,
}

/// One hit of [`search_plan_issues`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanIssueHit {
    pub row: PlanIssueRow,
    /// A short part of the body around the first match, or the start of the
    /// body.
    pub snippet: String,
}

/// One plan issue in full, from [`read_plan_issue_detail`]. The serde form is
/// the frontend's `GithubPlanIssueDetail` of `src/types/delivery.ts`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanIssueDetail {
    pub number: u64,
    pub url: String,
    pub title: String,
    /// The whole body. The caller cuts it where it needs to.
    pub body: String,
    pub issue_type: Option<String>,
    pub state: String,
    pub close_reason: Option<String>,
    pub milestone: Option<u64>,
    pub milestone_title: Option<String>,
    pub parent: Option<u64>,
    pub sub_issues: Vec<u64>,
    pub labels: Vec<String>,
    pub status: Option<String>,
    pub blocked_by: Vec<u64>,
    pub updated_at: String,
}

/// The answer of [`read_plan_issue_detail`]. The serde form is the
/// frontend's `GithubPlanIssueRead` of `src/types/delivery.ts`.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanIssueRead {
    pub issue: PlanIssueDetail,
    /// True where the GitHub read failed and the detail comes from the read
    /// cache. Then `labels` is empty and `error` names the failure.
    pub stale: bool,
    pub error: Option<DeliveryError>,
    /// The instant of the cached read, where `stale`.
    pub read_at: Option<String>,
}

fn close_reason_of(state: &str, reason: Option<StateReason>) -> Option<String> {
    (state != "open").then(|| {
        match reason {
            Some(StateReason::NotPlanned) | Some(StateReason::Duplicate) => "not_planned",
            _ => "completed",
        }
        .to_string()
    })
}

/// The cache of the target, or one GitHub read where none exists.
fn cache_or_read<R: Runtime>(
    app: &AppHandle<R>,
    target: &GithubTarget,
) -> Result<TrackerRead, DeliveryError> {
    match cached(app, target)? {
        Some(read) => Ok(read),
        None => shared_read(app, target),
    }
}

/// The read cache of the delivery plan and the plan built from it.
fn plan_of<R: Runtime>(
    app: &AppHandle<R>,
) -> Result<(GithubTarget, TrackerRead, BuiltPlan), DeliveryError> {
    let target = delivery_target(app)?;
    let read = cache_or_read(app, &target)?;
    let store = machine_store(app)?;
    let overlay = read_delivery_overlay(app, buffer_of(app), &store, &target.project_node_id)?;
    let built = build_plan(&read, &target.settings, &overlay, &target.statuses);
    Ok((target, read, built))
}

/// The plan issues of `read` in Project order, each with its Status.
fn plan_issues<'a>(
    read: &'a TrackerRead,
    built: &BuiltPlan,
) -> Vec<(&'a TrackerIssue, Option<&'a str>)> {
    let ids: HashSet<&str> = built
        .tracker
        .iter()
        .filter(|f| f.reference.kind == "issue")
        .map(|f| f.item_id.as_str())
        .collect();
    let mut seen = HashSet::new();
    read.project
        .items
        .iter()
        .filter_map(|item| match &item.content {
            ItemContent::Issue(i)
                if read.own(i)
                    && ids.contains(issue_id(i.number).as_str())
                    && seen.insert(i.number) =>
            {
                Some((i, item.status.as_deref()))
            }
            _ => None,
        })
        .collect()
}

fn row_of(read: &TrackerRead, issue: &TrackerIssue, status: Option<&str>) -> PlanIssueRow {
    PlanIssueRow {
        number: issue.number,
        title: issue.title.clone(),
        issue_type: issue.issue_type.clone(),
        state: issue.state.clone(),
        close_reason: close_reason_of(&issue.state, issue.state_reason),
        milestone: issue.milestone.and_then(|m| read.milestone(m)).map(|m| m.title.clone()),
        parent: own_parent(read, issue),
        status: status.map(str::to_string),
        labels: Vec::new(),
    }
}

fn own_parent(read: &TrackerRead, issue: &TrackerIssue) -> Option<u64> {
    issue.parent.as_ref().filter(|p| read.repository.is(&p.owner, &p.repo)).map(|p| p.number)
}

fn same(wanted: &Option<String>, found: Option<&str>) -> bool {
    wanted.as_deref().is_none_or(|w| found.is_some_and(|f| f.trim().eq_ignore_ascii_case(w.trim())))
}

/// Whether a row passes the filters, without the page.
fn passes(filter: &PlanIssueFilter, row: &PlanIssueRow) -> bool {
    same(&filter.state, Some(&row.state))
        && same(&filter.issue_type, row.issue_type.as_deref())
        && same(&filter.milestone, row.milestone.as_deref())
        && same(&filter.status, row.status.as_deref())
        && filter.parent.is_none_or(|p| row.parent == Some(p))
        && filter
            .label
            .as_deref()
            .is_none_or(|l| row.labels.iter().any(|x| x.trim().eq_ignore_ascii_case(l.trim())))
}

/// The rows of the plan issues that pass the filter, as one page. Pure.
pub fn page_of(read: &TrackerRead, built: &BuiltPlan, filter: &PlanIssueFilter) -> PlanIssuePage {
    let matching: Vec<PlanIssueRow> = plan_issues(read, built)
        .into_iter()
        .map(|(issue, status)| row_of(read, issue, status))
        .filter(|row| passes(filter, row))
        .collect();
    let total = matching.len();
    PlanIssuePage {
        total,
        offset: filter.offset,
        read_at: read.read_at.clone(),
        rows: matching
            .into_iter()
            .skip(filter.offset)
            .take(filter.limit.unwrap_or(usize::MAX))
            .collect(),
    }
}

/// The part of `body` around the first of `terms`, in at most `SNIPPET_CHARS`
/// characters.
fn snippet_of(body: &str, terms: &[String]) -> String {
    let chars: Vec<char> = body.chars().collect();
    let lower: Vec<char> = body.to_lowercase().chars().collect();
    // Lowercasing can change the length of a text; a body that changes length
    // gets the snippet from its start.
    let at = (lower.len() == chars.len())
        .then(|| {
            terms
                .iter()
                .filter_map(|t| {
                    let needle: Vec<char> = t.chars().collect();
                    lower.windows(needle.len().max(1)).position(|w| w == needle.as_slice())
                })
                .min()
        })
        .flatten()
        .unwrap_or(0);
    let start = at.saturating_sub(SNIPPET_CHARS / 4);
    let text: String = chars.iter().skip(start).take(SNIPPET_CHARS).collect();
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The plan issues whose title or body holds every word of `query`, best
/// first: a word in the title counts more than a word in the body, and equal
/// scores keep the Project order. At most `limit` hits. A query with no word
/// finds nothing. Pure.
pub fn hits_of(
    read: &TrackerRead,
    built: &BuiltPlan,
    query: &str,
    limit: usize,
) -> Vec<PlanIssueHit> {
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if terms.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(usize, PlanIssueHit)> = Vec::new();
    for (issue, status) in plan_issues(read, built) {
        let (title, body) = (issue.title.to_lowercase(), issue.body.to_lowercase());
        let mut score = 0;
        let mut all = true;
        for term in &terms {
            let (in_title, in_body) = (title.contains(term.as_str()), body.contains(term.as_str()));
            all &= in_title || in_body;
            score += 3 * usize::from(in_title) + usize::from(in_body);
        }
        if all {
            let hit = PlanIssueHit {
                row: row_of(read, issue, status),
                snippet: snippet_of(&issue.body, &terms),
            };
            scored.push((score, hit));
        }
    }
    scored.sort_by_key(|(score, _)| std::cmp::Reverse(*score));
    scored.into_iter().take(limit).map(|(_, hit)| hit).collect()
}

/// PIR-FR-JPMU: the plan issues that pass `filter`, in Project order, from the
/// read cache. Blocking where no cache exists (see the module text).
pub fn list_plan_issues<R: Runtime>(
    app: &AppHandle<R>,
    filter: &PlanIssueFilter,
) -> Result<PlanIssuePage, DeliveryError> {
    let started = Instant::now();
    let result = plan_of(app).and_then(|(target, read, built)| match filter.label {
        None => Ok(page_of(&read, &built, filter)),
        Some(_) => labelled_page(app, &target, &read, &built, filter),
    });
    log_read(app, "plan issues listed", &result.as_ref().map(|p| p.rows.len()), started);
    result
}

/// The most issues that a label filter examines. The read cache holds no
/// labels, so the filter reads one issue at a time from GitHub.
const LABEL_SCAN_MAX: usize = 50;

/// PIR-FR-JPMU: a page for a label filter. The first `LABEL_SCAN_MAX` issues
/// that pass the other filters are read from GitHub, and the page holds those
/// that carry the label. The rows of the page hold their labels.
fn labelled_page<R: Runtime>(
    app: &AppHandle<R>,
    target: &GithubTarget,
    read: &TrackerRead,
    built: &BuiltPlan,
    filter: &PlanIssueFilter,
) -> Result<PlanIssuePage, DeliveryError> {
    let others = PlanIssueFilter { label: None, limit: None, offset: 0, ..filter.clone() };
    let candidates = page_of(read, built, &others).rows;
    let adapter = connect(app)?;
    let mut matching = Vec::new();
    for mut row in candidates.into_iter().take(LABEL_SCAN_MAX) {
        let detail = adapter
            .read_issue_detail(row.number, &target.project_node_id)
            .map_err(|code| github_error(&code))?;
        let Some(detail) = detail else { continue };
        row.labels = detail.labels;
        if passes(filter, &row) {
            matching.push(row);
        }
    }
    Ok(PlanIssuePage {
        total: matching.len(),
        offset: filter.offset,
        read_at: read.read_at.clone(),
        rows: matching
            .into_iter()
            .skip(filter.offset)
            .take(filter.limit.unwrap_or(usize::MAX))
            .collect(),
    })
}

/// PIR-FR-APBD: the answer of [`search_plan_issues`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct PlanIssueSearch {
    /// The number of plan issues that match, before the limit.
    pub total: usize,
    /// The instant of the GitHub read that the cache holds.
    pub read_at: String,
    /// The best `limit` hits, best first.
    pub hits: Vec<PlanIssueHit>,
}

/// PIR-FR-APBD: the plan issues whose title or body holds every word of
/// `query`, best first, from the read cache. Blocking where no cache exists
/// (see the module text).
pub fn search_plan_issues<R: Runtime>(
    app: &AppHandle<R>,
    query: &str,
    limit: usize,
) -> Result<PlanIssueSearch, DeliveryError> {
    let started = Instant::now();
    let result = plan_of(app).map(|(_, read, built)| {
        let mut hits = hits_of(&read, &built, query, usize::MAX);
        let total = hits.len();
        hits.truncate(limit);
        PlanIssueSearch { total, read_at: read.read_at.clone(), hits }
    });
    log_read(app, "plan issues searched", &result.as_ref().map(|s| s.hits.len()), started);
    result
}

fn log_read<R: Runtime>(
    app: &AppHandle<R>,
    message: &str,
    outcome: &Result<usize, &DeliveryError>,
    started: Instant,
) {
    let ms = started.elapsed().as_millis() as u64;
    match outcome {
        Ok(count) => {
            log(app, LogLevel::Debug, message, crate::log_fields! { "count" => *count, "ms" => ms })
        }
        Err(e) => log(
            app,
            LogLevel::Warn,
            "plan read refused",
            crate::log_fields! { "code" => &e.code, "ms" => ms },
        ),
    }
}

/// PIR-FR-DFKG: the catalog of the plan. The milestones and the Status options
/// come from the read cache. The Types and the labels come from one GitHub
/// read each. A label read that fails gives `labels: None`; a Type read that
/// fails gives the Types of the plan issues. It refuses where the plan itself
/// cannot be read.
pub async fn plan_catalog<R: Runtime>(app: AppHandle<R>) -> Result<PlanCatalog, DeliveryError> {
    source::off_thread(move || plan_catalog_blocking(&app)).await
}

fn plan_catalog_blocking<R: Runtime>(app: &AppHandle<R>) -> Result<PlanCatalog, DeliveryError> {
    let started = Instant::now();
    let (_, read, built) = plan_of(app)?;
    let adapter = connect(app);
    let labels = match adapter.as_ref().map(|a| a.list_labels()) {
        Ok(Ok(labels)) => Some(labels),
        Ok(Err(code)) => {
            log(app, LogLevel::Warn, "plan labels not read", crate::log_fields! { "code" => code });
            None
        }
        Err(e) => {
            log(
                app,
                LogLevel::Warn,
                "plan labels not read",
                crate::log_fields! { "code" => &e.code },
            );
            None
        }
    };
    let offered = adapter.as_ref().ok().and_then(|a| a.list_issue_types().ok());
    let types = offered.unwrap_or_else(|| {
        let mut used: Vec<String> = Vec::new();
        for (issue, _) in plan_issues(&read, &built) {
            if let Some(t) = &issue.issue_type {
                if !used.contains(t) {
                    used.push(t.clone());
                }
            }
        }
        used
    });
    let catalog = PlanCatalog {
        milestones: read
            .milestones
            .iter()
            .map(|m| CatalogMilestone {
                number: m.number,
                title: m.title.clone(),
                state: m.state.clone(),
            })
            .collect(),
        types,
        statuses: read
            .project
            .status_field
            .as_ref()
            .map(|f| f.options.iter().map(|o| o.name.clone()).collect())
            .unwrap_or_default(),
        labels,
    };
    log(
        app,
        LogLevel::Debug,
        "plan catalog read",
        crate::log_fields! {
            "milestones" => catalog.milestones.len(),
            "types" => catalog.types.len(),
            "statuses" => catalog.statuses.len(),
            "labels_read" => catalog.labels.is_some(),
            "ms" => started.elapsed().as_millis() as u64,
        },
    );
    Ok(catalog)
}

fn detail_from_github(detail: IssueDetail) -> PlanIssueDetail {
    PlanIssueDetail {
        number: detail.number,
        url: detail.url,
        title: detail.title,
        body: detail.body,
        issue_type: detail.issue_type,
        close_reason: close_reason_of(&detail.state, detail.state_reason),
        state: detail.state,
        milestone: detail.milestone,
        milestone_title: detail.milestone_title,
        parent: detail.parent.map(|p| p.number),
        sub_issues: detail.sub_issues.iter().map(|s| s.number).collect(),
        labels: detail.labels,
        status: detail.status,
        blocked_by: detail.blocked_by.iter().map(|b| b.number).collect(),
        updated_at: detail.updated_at,
    }
}

/// The detail of a plan issue as the read cache holds it. It holds no labels.
fn detail_from_cache(
    read: &TrackerRead,
    issue: &TrackerIssue,
    status: Option<&str>,
) -> PlanIssueDetail {
    let milestone = issue.milestone.and_then(|m| read.milestone(m));
    PlanIssueDetail {
        number: issue.number,
        url: issue.url.clone(),
        title: issue.title.clone(),
        body: issue.body.clone(),
        issue_type: issue.issue_type.clone(),
        state: issue.state.clone(),
        close_reason: close_reason_of(&issue.state, issue.state_reason),
        milestone: issue.milestone,
        milestone_title: milestone.map(|m| m.title.clone()),
        parent: own_parent(read, issue),
        sub_issues: read
            .project
            .items
            .iter()
            .filter_map(|item| match &item.content {
                ItemContent::Issue(i)
                    if read.own(i) && own_parent(read, i) == Some(issue.number) =>
                {
                    Some(i.number)
                }
                _ => None,
            })
            .collect(),
        labels: Vec::new(),
        status: status.map(str::to_string),
        blocked_by: issue.blocked_by.iter().map(|b| b.number).collect(),
        updated_at: issue.updated_at.clone(),
    }
}

/// PIR-FR-GYHA, PIR-FR-NRLP, GDP-FR-QFQF: one plan issue in full, read from
/// GitHub now. It refuses with `tracker_item_not_found` for a number that is
/// not a plan issue, and it makes no request then. Where the GitHub read
/// fails, the answer is the detail of the read cache with `stale` true, no
/// labels, and the error. The read changes nothing and updates no cache.
pub async fn read_plan_issue_detail<R: Runtime>(
    app: AppHandle<R>,
    number: u64,
) -> Result<PlanIssueRead, DeliveryError> {
    source::off_thread(move || read_plan_issue_blocking(&app, number)).await
}

fn read_plan_issue_blocking<R: Runtime>(
    app: &AppHandle<R>,
    number: u64,
) -> Result<PlanIssueRead, DeliveryError> {
    let started = Instant::now();
    let (target, read, built) = plan_of(app)?;
    let cached_detail = plan_issues(&read, &built)
        .into_iter()
        .find(|(i, _)| i.number == number)
        .map(|(issue, status)| detail_from_cache(&read, issue, status))
        .ok_or_else(|| {
            let error = DeliveryError::new(
                ERR_TRACKER_ITEM_NOT_FOUND,
                "The issue is not part of the delivery plan.",
            );
            log(
                app,
                LogLevel::Warn,
                "plan issue read refused",
                crate::log_fields! { "code" => &error.code },
            );
            error
        })?;
    let fetched = connect(app).and_then(|adapter| {
        adapter
            .read_issue_detail(number, &target.project_node_id)
            .map_err(|code| github_error(&code))
    });
    let ms = started.elapsed().as_millis() as u64;
    match fetched {
        Ok(Some(detail)) => {
            log(app, LogLevel::Debug, "plan issue read", crate::log_fields! { "ms" => ms });
            Ok(PlanIssueRead {
                issue: detail_from_github(detail),
                stale: false,
                error: None,
                read_at: None,
            })
        }
        other => {
            let error = other.err().unwrap_or_else(|| {
                DeliveryError::new(ERR_TRACKER_ITEM_NOT_FOUND, "The tracker has no such issue.")
            });
            log(
                app,
                LogLevel::Warn,
                "plan issue read fell back to the cache",
                crate::log_fields! { "code" => &error.code, "ms" => ms },
            );
            Ok(PlanIssueRead {
                issue: cached_detail,
                stale: true,
                error: Some(error),
                read_at: Some(read.read_at.clone()),
            })
        }
    }
}

/// What a proposal of plan changes asks the plan about.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlanFactsQuery {
    /// Read the Types that the repository offers.
    pub need_types: bool,
    /// Read the label names of the repository.
    pub need_labels: bool,
    /// The plan issues that a change names. GitHub is read for each of them
    /// (GDP-FR-PBSD).
    pub numbers: Vec<u64>,
}

/// The facts that a proposal of plan changes is checked against
/// (PPT-FR-XMZF).
#[derive(Clone, Debug, PartialEq)]
pub struct PlanFacts {
    pub milestones: Vec<CatalogMilestone>,
    pub statuses: Vec<String>,
    /// The Types of the repository, where the query asked for them.
    pub types: Option<Vec<String>>,
    /// The label names of the repository, where the query asked for them.
    pub labels: Option<Vec<String>>,
    /// The numbers of all plan issues.
    pub plan_numbers: HashSet<u64>,
    /// The detail of each named plan issue, read from GitHub now with its
    /// labels (PPT-FR-UZEK).
    pub issues: std::collections::HashMap<u64, PlanIssueDetail>,
}

/// PPT-FR-XMZF, PPT-FR-UZEK, GDP-FR-PBSD: the facts for a proposal. It makes at
/// most one request for the Types, one for the labels, and two for each named
/// plan issue: its detail, and the issue as the read cache holds it. The
/// milestones and the Status options come from the read cache. It changes
/// nothing on GitHub. It puts each issue that it read into the read cache.
pub async fn plan_facts<R: Runtime>(
    app: AppHandle<R>,
    query: PlanFactsQuery,
) -> Result<PlanFacts, DeliveryError> {
    source::off_thread(move || plan_facts_blocking(&app, &query)).await
}

fn plan_facts_blocking<R: Runtime>(
    app: &AppHandle<R>,
    query: &PlanFactsQuery,
) -> Result<PlanFacts, DeliveryError> {
    let started = Instant::now();
    let (target, read, built) = plan_of(app)?;
    let plan = plan_issues(&read, &built);
    let mut issues = std::collections::HashMap::new();
    for number in &query.numbers {
        if let Some((issue, status)) = plan.iter().find(|(i, _)| i.number == *number) {
            issues.insert(*number, detail_from_cache(&read, issue, *status));
        }
    }
    let plan_numbers: HashSet<u64> = plan.iter().map(|(i, _)| i.number).collect();
    // PPT-FR-UZEK: each named plan issue is read from GitHub once.
    let mut fresh_numbers: Vec<u64> = Vec::new();
    for number in query.numbers.iter().filter(|n| plan_numbers.contains(n)) {
        if !fresh_numbers.contains(number) {
            fresh_numbers.push(*number);
        }
    }
    let reads_github = query.need_types || query.need_labels || !fresh_numbers.is_empty();
    let adapter = if reads_github { Some(connect(app)?) } else { None };
    let mut types = None;
    let mut labels = None;
    if let Some(adapter) = &adapter {
        if query.need_types {
            types = Some(adapter.list_issue_types().map_err(|code| github_error(&code))?);
        }
        if query.need_labels {
            labels = Some(adapter.list_labels().map_err(|code| github_error(&code))?);
        }
        let mut statuses = Vec::new();
        for number in &fresh_numbers {
            let detail = adapter
                .read_issue_detail(*number, &target.project_node_id)
                .map_err(|code| github_error(&code))?
                .ok_or_else(|| {
                    DeliveryError::new(ERR_TRACKER_ITEM_NOT_FOUND, "The tracker has no such issue.")
                })?;
            statuses.push((*number, detail.status.clone()));
            issues.insert(*number, detail_from_github(detail));
        }
        cache_fresh_issues(app, &target, adapter.as_ref(), &statuses);
    }
    let facts = PlanFacts {
        milestones: read
            .milestones
            .iter()
            .map(|m| CatalogMilestone {
                number: m.number,
                title: m.title.clone(),
                state: m.state.clone(),
            })
            .collect(),
        statuses: read
            .project
            .status_field
            .as_ref()
            .map(|f| f.options.iter().map(|o| o.name.clone()).collect())
            .unwrap_or_default(),
        types,
        labels,
        plan_numbers,
        issues,
    };
    log(
        app,
        LogLevel::Debug,
        "plan facts read",
        crate::log_fields! {
            "issues" => facts.issues.len(),
            "types_read" => facts.types.is_some(),
            "labels_read" => facts.labels.is_some(),
            "ms" => started.elapsed().as_millis() as u64,
        },
    );
    Ok(facts)
}

/// GDP-FR-PBSD: put each issue that a proposal read from GitHub into the read
/// cache, and announce the change. The detail read holds no node ID and no
/// assignees, which the cache needs, so each issue is read once more as the
/// cache holds it. The write takes the turn of the edit queue (GDP-FR-IRDD).
/// A failure here leaves the cache as it was and never refuses the proposal:
/// it is logged.
fn cache_fresh_issues<R: Runtime>(
    app: &AppHandle<R>,
    target: &GithubTarget,
    adapter: &dyn super::adapter::TrackerAdapter,
    statuses: &[(u64, Option<String>)],
) {
    let mut fresh: Vec<(TrackerIssue, Option<String>)> = Vec::new();
    for (number, status) in statuses {
        match adapter.read_issue(*number) {
            Ok(Some(issue)) => fresh.push((issue, status.clone())),
            Ok(None) => {}
            Err(code) => log(
                app,
                LogLevel::Warn,
                "plan issue cache update skipped",
                crate::log_fields! { "code" => &code },
            ),
        }
    }
    if fresh.is_empty() {
        return;
    }
    let state = app.project_store::<GithubDeliveryState>();
    let _turn = state.edits.enter();
    let stored = cached(app, target).and_then(|read| {
        let Some(mut read) = read else { return Ok(false) };
        let before = read.clone();
        for (issue, status) in fresh {
            let number = issue.number;
            read.replace_issue(issue);
            read.set_status(number, status);
        }
        if read == before {
            return Ok(false);
        }
        store_edit(app, target, &read).map(|_| true)
    });
    match stored {
        Ok(changed) => log(
            app,
            LogLevel::Debug,
            "plan issues cached after a proposal read",
            crate::log_fields! { "issues" => statuses.len(), "changed" => changed },
        ),
        Err(error) => log(
            app,
            LogLevel::Warn,
            "plan issue cache update failed",
            crate::log_fields! { "code" => &error.code },
        ),
    }
}

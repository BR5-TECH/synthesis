//! `list_plan_issues`, `search_plan_issues`, and `read_plan_issue` — the three
//! tools that read the issues of the delivery plan in a Map discussion
//! (`PIR-plan-issue-read-tools.md`).
//!
//! They read the plan issues of the selected GitHub Project and no other issue
//! of the repository (PIR-FR-JEBI). They write nothing (PIR-FR-NBCT). The list
//! and the search answer from the read cache, and the read of one issue asks
//! GitHub (PIR-FR-NRLP). The token is the one of the delivery plan, and it
//! never reaches an argument, a result, or a record (PIR-FR-ZVSC).

use std::future::Future;

use rig::tool::ToolErrorKind;
use serde::{Deserialize, Serialize};

use super::plan_refusals::{
    gate, log_read_done, log_refused, plan_is_unavailable, GateWords, PlanRefusal,
};
use super::ToolRefusal;
use crate::agent_conversations::ConversationOrigin;
use crate::delivery::github::plan_reads::{
    self, PlanCatalog, PlanIssueFilter, PlanIssueHit, PlanIssueRow,
};
use crate::delivery::DeliveryError;
use crate::logging::{LogBuffer, LogSink, BUFFER};

#[cfg(test)]
mod tests;

/// PIR-FR-NBCT.
pub const LIST_NAME: &str = "list_plan_issues";
pub const SEARCH_NAME: &str = "search_plan_issues";
pub const READ_NAME: &str = "read_plan_issue";

/// PIR-FR-VMSF: the fixed descriptions of the contract surface.
pub const LIST_DESCRIPTION: &str = "List the issues of the delivery plan: the issues of the selected GitHub Project. Filter by state, Type, milestone, Status, label, or parent. The first page also gives the catalog: the milestones, the Types, the Status options, and the labels that exist. Use it to see what the plan holds before you propose a change.";
pub const SEARCH_DESCRIPTION: &str = "Search the issues of the delivery plan by words in the title or the body. Use it to find out whether the plan already holds an issue for a part of a design.";
pub const READ_DESCRIPTION: &str = "Read one issue of the delivery plan in full by its number: body, labels, milestone, parent, sub-issues, Status, and relations. It reads no issue that is not part of the plan.";

/// PIR contract surface, "Refusals".
pub const NOT_A_MAP: &str = "This conversation is not a Map discussion, so it has no plan issues. Asking again will not succeed.";
pub const PLAN_UNAVAILABLE: &str = "The project has no GitHub Project selected for its plan, or its configuration is not valid. Tell the author; asking again will not succeed.";
pub const BLANK_QUERY: &str = "Give words to search for in `query`.";
pub const COULD_NOT_READ: &str = "The plan could not be read.";

/// PIR contract surface: "Issue number N is not part of the delivery plan."
pub fn not_a_plan_issue(number: i64) -> String {
    format!(
        "Issue number {number} is not part of the delivery plan. List the plan issues to find the right number."
    )
}

/// PIR-FR-JPMU, PIR-FR-APBD: the page sizes.
const LIST_DEFAULT: usize = 50;
const LIST_MAX: usize = 100;
const SEARCH_DEFAULT: usize = 20;
const SEARCH_MAX: usize = 50;
/// PIR-FR-GYHA: the body of an issue is cut at this many characters.
const BODY_MAX_CHARS: usize = 20_000;

const WORDS: GateWords = GateWords { not_a_map: NOT_A_MAP, plan_unavailable: PLAN_UNAVAILABLE };

fn read_refusal(error: &DeliveryError) -> PlanRefusal {
    if error.code == crate::delivery::ERR_NO_PROJECT_OPEN {
        return ToolRefusal::NoProjectOpen.into();
    }
    if plan_is_unavailable(error) {
        return PlanRefusal::new(
            ToolErrorKind::NotFound,
            "plan_unavailable",
            false,
            PLAN_UNAVAILABLE,
        );
    }
    PlanRefusal::new(ToolErrorKind::Other, "could_not_read", true, COULD_NOT_READ)
}

// ---------------------------------------------------------------------------
// The shapes (PIR contract surface, "Output")
// ---------------------------------------------------------------------------

/// A row of a list or a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanIssueRowOut {
    pub number: u64,
    pub title: String,
    #[serde(rename = "type")]
    pub issue_type: Option<String>,
    pub state: String,
    pub close_reason: Option<String>,
    pub milestone: Option<String>,
    pub parent: Option<u64>,
    pub status: Option<String>,
}

impl From<PlanIssueRow> for PlanIssueRowOut {
    fn from(row: PlanIssueRow) -> Self {
        PlanIssueRowOut {
            number: row.number,
            title: row.title,
            issue_type: row.issue_type,
            state: row.state,
            close_reason: row.close_reason,
            milestone: row.milestone,
            parent: row.parent,
            status: row.status,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogMilestoneOut {
    pub number: u64,
    pub title: String,
    pub state: String,
}

/// PIR-FR-DFKG: the catalog of the plan. `labels` is `null` where the labels
/// could not be read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CatalogOut {
    pub milestones: Vec<CatalogMilestoneOut>,
    pub types: Vec<String>,
    pub statuses: Vec<String>,
    pub labels: Option<Vec<String>>,
}

impl From<PlanCatalog> for CatalogOut {
    fn from(catalog: PlanCatalog) -> Self {
        CatalogOut {
            milestones: catalog
                .milestones
                .into_iter()
                .map(|m| CatalogMilestoneOut { number: m.number, title: m.title, state: m.state })
                .collect(),
            types: catalog.types,
            statuses: catalog.statuses,
            labels: catalog.labels,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListPlanIssuesOutput {
    pub total: usize,
    pub offset: usize,
    pub read_at: String,
    pub stale: bool,
    pub issues: Vec<PlanIssueRowOut>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog: Option<CatalogOut>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SearchHitOut {
    #[serde(flatten)]
    pub row: PlanIssueRowOut,
    pub snippet: String,
}

impl From<PlanIssueHit> for SearchHitOut {
    fn from(hit: PlanIssueHit) -> Self {
        SearchHitOut { row: hit.row.into(), snippet: hit.snippet }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPlanIssuesOutput {
    pub total: usize,
    pub read_at: String,
    pub stale: bool,
    pub issues: Vec<SearchHitOut>,
}

/// PIR-FR-GYHA, PIR-FR-NRLP. `stale` and `read_at` say that GitHub could not
/// be read and the answer comes from the read cache, which holds no labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadPlanIssueOutput {
    pub number: u64,
    pub title: String,
    pub body: String,
    pub body_cut: bool,
    #[serde(rename = "type")]
    pub issue_type: Option<String>,
    pub state: String,
    pub close_reason: Option<String>,
    pub milestone: Option<String>,
    pub parent: Option<u64>,
    pub sub_issues: Vec<u64>,
    pub labels: Vec<String>,
    pub status: Option<String>,
    pub blocked_by: Vec<u64>,
    pub updated_at: String,
    pub url: String,
    pub stale: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_at: Option<String>,
}

// ---------------------------------------------------------------------------
// The arguments and the schemas (PIR contract surface, "Arguments")
// ---------------------------------------------------------------------------

/// The arguments, as the model composed them. Unknown fields are ignored
/// (TLC-FR-07).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ListPlanIssuesArgs {
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default, rename = "type")]
    pub issue_type: Option<String>,
    #[serde(default)]
    pub milestone: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub parent: Option<i64>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SearchPlanIssuesArgs {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReadPlanIssueArgs {
    pub number: i64,
}

/// PIR-FR-VMSF, TLC-FR-06.
pub fn list_parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "state": { "type": "string", "enum": ["open", "closed"], "description": "Only issues in this state." },
            "type": { "type": "string", "description": "Only issues of this Type, by name." },
            "milestone": { "type": "string", "description": "Only issues in the milestone with this exact title." },
            "status": { "type": "string", "description": "Only issues with this Status option of the Project." },
            "label": { "type": "string", "description": "Only issues that carry this label. At most 50 issues are checked for it." },
            "parent": { "type": "integer", "description": "Only the sub-issues of this plan issue, by number." },
            "limit": { "type": "integer", "description": "The most issues to return. 50 by default, 100 at most." },
            "offset": { "type": "integer", "description": "The number of matching issues to skip. 0 by default. The catalog is on the first page only." },
        },
    })
}

/// PIR-FR-VMSF, TLC-FR-06.
pub fn search_parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "query": { "type": "string", "description": "Words to find in the title or the body of a plan issue. Every word must be present." },
            "limit": { "type": "integer", "description": "The most issues to return. 20 by default, 50 at most." },
        },
        "required": ["query"],
    })
}

/// PIR-FR-VMSF, TLC-FR-06.
pub fn read_parameters() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "number": { "type": "integer", "description": "The number of a plan issue." },
        },
        "required": ["number"],
    })
}

// ---------------------------------------------------------------------------
// The tools
// ---------------------------------------------------------------------------

/// The part of a tool that the three share: the handle of the project and the
/// origin of the turn.
struct Turn<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    origin: ConversationOrigin,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> Turn<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    fn new(app: tauri::AppHandle<R>, origin: ConversationOrigin) -> Self {
        Turn { app, origin, buffer: &BUFFER }
    }
}

/// PIR-FR-VNGK: the log record of the end of a call. It holds the tool and a
/// count or a reason, and never a title, a body, a query, or a token.
fn log_outcome<S: LogSink + Clone + Send + 'static, T>(
    sink: &S,
    buffer: &'static LogBuffer,
    tool: &'static str,
    outcome: &Result<T, PlanRefusal>,
    count: impl FnOnce(&T) -> usize,
) {
    match outcome {
        Ok(value) => log_read_done(sink, buffer, tool, count(value)),
        Err(refusal) => log_refused(sink, buffer, tool, refusal, crate::logging::Fields::new()),
    }
}

/// `list_plan_issues`.
pub struct ListPlanIssuesTool<R: tauri::Runtime>(Turn<R>);

/// `search_plan_issues`.
pub struct SearchPlanIssuesTool<R: tauri::Runtime>(Turn<R>);

/// `read_plan_issue`.
pub struct ReadPlanIssueTool<R: tauri::Runtime>(Turn<R>);

macro_rules! constructors {
    ($tool:ident) => {
        impl<R: tauri::Runtime> $tool<R>
        where
            tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
        {
            /// TLC-FR-15: bound to the project handle and the turn's origin. It
            /// names no issue and no repository.
            pub fn new(app: tauri::AppHandle<R>, origin: ConversationOrigin) -> Self {
                $tool(Turn::new(app, origin))
            }

            /// The same tool, reporting into `buffer`.
            #[cfg(test)]
            pub fn with_buffer(mut self, buffer: &'static LogBuffer) -> Self {
                self.0.buffer = buffer;
                self
            }
        }
    };
}

constructors!(ListPlanIssuesTool);
constructors!(SearchPlanIssuesTool);
constructors!(ReadPlanIssueTool);

/// A positive size from the model, inside `1..=max`; `default` where absent.
fn size_of(wanted: Option<i64>, default: usize, max: usize) -> usize {
    match wanted {
        None => default,
        Some(n) => usize::try_from(n).unwrap_or(0).clamp(1, max),
    }
}

/// A filter text, or `None` where it is absent or blank.
fn text_of(value: &Option<String>) -> Option<String> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty()).map(str::to_string)
}

impl<R: tauri::Runtime> ListPlanIssuesTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    async fn run(
        app: tauri::AppHandle<R>,
        origin: ConversationOrigin,
        args: ListPlanIssuesArgs,
    ) -> Result<ListPlanIssuesOutput, PlanRefusal> {
        gate(&app, &origin, &WORDS)?;
        let limit = size_of(args.limit, LIST_DEFAULT, LIST_MAX);
        let offset = args.offset.map_or(0, |o| usize::try_from(o).unwrap_or(0));
        let filter = PlanIssueFilter {
            state: text_of(&args.state).map(|s| s.to_lowercase()),
            issue_type: text_of(&args.issue_type),
            milestone: text_of(&args.milestone),
            status: text_of(&args.status),
            label: text_of(&args.label),
            parent: args.parent.and_then(|p| u64::try_from(p).ok()),
            limit: Some(limit),
            offset,
        };
        // A parent that is not a positive number matches no issue.
        let impossible_parent = args.parent.is_some() && filter.parent.is_none();
        let worker = app.clone();
        let page = crate::delivery::source::off_thread(move || {
            plan_reads::list_plan_issues(&worker, &filter)
        })
        .await
        .map_err(|e| read_refusal(&e))?;
        let catalog = if offset == 0 {
            Some(CatalogOut::from(
                plan_reads::plan_catalog(app.clone()).await.map_err(|e| read_refusal(&e))?,
            ))
        } else {
            None
        };
        let (total, rows) =
            if impossible_parent { (0, Vec::new()) } else { (page.total, page.rows) };
        Ok(ListPlanIssuesOutput {
            total,
            offset: page.offset,
            read_at: page.read_at,
            stale: false,
            issues: rows.into_iter().map(PlanIssueRowOut::from).collect(),
            catalog,
        })
    }
}

impl<R: tauri::Runtime> SearchPlanIssuesTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    async fn run(
        app: tauri::AppHandle<R>,
        origin: ConversationOrigin,
        args: SearchPlanIssuesArgs,
    ) -> Result<SearchPlanIssuesOutput, PlanRefusal> {
        gate(&app, &origin, &WORDS)?;
        // PIR-FR-APBD.
        let query = args.query.trim().to_string();
        if query.is_empty() {
            return Err(PlanRefusal::invalid("blank_query", BLANK_QUERY));
        }
        let limit = size_of(args.limit, SEARCH_DEFAULT, SEARCH_MAX);
        let worker = app.clone();
        let found = crate::delivery::source::off_thread(move || {
            plan_reads::search_plan_issues(&worker, &query, limit)
        })
        .await
        .map_err(|e| read_refusal(&e))?;
        Ok(SearchPlanIssuesOutput {
            total: found.total,
            read_at: found.read_at,
            stale: false,
            issues: found.hits.into_iter().map(SearchHitOut::from).collect(),
        })
    }
}

impl<R: tauri::Runtime> ReadPlanIssueTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    async fn run(
        app: tauri::AppHandle<R>,
        origin: ConversationOrigin,
        args: ReadPlanIssueArgs,
    ) -> Result<ReadPlanIssueOutput, PlanRefusal> {
        gate(&app, &origin, &WORDS)?;
        let not_a_plan_issue = || {
            PlanRefusal::new(
                ToolErrorKind::NotFound,
                "not_a_plan_issue",
                true,
                not_a_plan_issue(args.number),
            )
        };
        let number = u64::try_from(args.number).map_err(|_| not_a_plan_issue())?;
        let read = plan_reads::read_plan_issue_detail(app, number).await.map_err(|e| {
            if e.code == crate::delivery::github::ERR_TRACKER_ITEM_NOT_FOUND {
                not_a_plan_issue()
            } else {
                read_refusal(&e)
            }
        })?;
        let issue = read.issue;
        let body_cut = issue.body.chars().count() > BODY_MAX_CHARS;
        let body =
            if body_cut { issue.body.chars().take(BODY_MAX_CHARS).collect() } else { issue.body };
        Ok(ReadPlanIssueOutput {
            number: issue.number,
            title: issue.title,
            body,
            body_cut,
            issue_type: issue.issue_type,
            state: issue.state,
            close_reason: issue.close_reason,
            milestone: issue.milestone_title,
            parent: issue.parent,
            sub_issues: issue.sub_issues,
            labels: issue.labels,
            status: issue.status,
            blocked_by: issue.blocked_by,
            updated_at: issue.updated_at,
            url: issue.url,
            stale: read.stale,
            read_at: read.read_at,
        })
    }
}

macro_rules! portable {
    ($tool:ident, $name:expr, $args:ty, $output:ty, $description:expr, $parameters:expr, $count:expr) => {
        impl<R: tauri::Runtime> rig::tool::PortableTool for $tool<R>
        where
            tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
        {
            const NAME: &'static str = $name;

            type Args = $args;
            type Output = $output;
            type Error = PlanRefusal;

            fn description(&self) -> String {
                $description.to_string()
            }

            fn parameters(&self) -> serde_json::Value {
                $parameters()
            }

            fn map_error(&self, error: Self::Error) -> rig::tool::ToolExecutionError {
                error.to_execution_error()
            }

            fn call(
                &self,
                arguments: Self::Args,
            ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send {
                let app = self.0.app.clone();
                let origin = self.0.origin.clone();
                let buffer = self.0.buffer;
                async move {
                    let outcome = Self::run(app.clone(), origin, arguments).await;
                    log_outcome(&app, buffer, $name, &outcome, $count);
                    outcome
                }
            }
        }
    };
}

portable!(
    ListPlanIssuesTool,
    LIST_NAME,
    ListPlanIssuesArgs,
    ListPlanIssuesOutput,
    LIST_DESCRIPTION,
    list_parameters,
    |o: &ListPlanIssuesOutput| o.issues.len()
);
portable!(
    SearchPlanIssuesTool,
    SEARCH_NAME,
    SearchPlanIssuesArgs,
    SearchPlanIssuesOutput,
    SEARCH_DESCRIPTION,
    search_parameters,
    |o: &SearchPlanIssuesOutput| o.issues.len()
);
portable!(
    ReadPlanIssueTool,
    READ_NAME,
    ReadPlanIssueArgs,
    ReadPlanIssueOutput,
    READ_DESCRIPTION,
    read_parameters,
    |_: &ReadPlanIssueOutput| 1
);

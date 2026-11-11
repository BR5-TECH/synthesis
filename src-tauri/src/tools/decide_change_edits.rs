//! Decide change edits tool (`DCE-decide-change-edits-tool.md`).
//!
//! The twin of `decide_prompt_changes` for an OpenSpec change: the same
//! arguments, the same authority rules, and the same refusals (DCE-FR-XVLE),
//! with the change's proposal store (`CEP-change-edit-proposals.md`) in place
//! of a file's. Each entry runs the function behind the chip's command
//! (CEP-FR-KLWZ), so a decision by tool is the author's decision.

use tauri::Manager;

use super::decide_changes::{
    self, Accepted, CallRefusal, DecideChangesArgs, DecideChangesOutput, HunkStateView,
    ProposalBackend, ProposalStateView, ProposalView,
};
use super::require_open_project;

use crate::agent_conversations::{ConversationOrigin, OwnedRoots};
use crate::change_action_requests::{self as car, AppStarter, RunStarter};
use crate::change_proposals::{
    self as cep, AppSites, ChangeEditProposal, ChangeProposalState, SiteResolver,
};
use crate::comments::{self, DiscussionTarget, Participant};
use crate::logging::{LogBuffer, LogSink, BUFFER};
use crate::prompt_proposals::HunkState;

/// DCE-FR-QIKC.
pub const NAME: &str = "decide_change_edits";

/// DCE-FR-ILCP: the fixed text of the contract surface.
pub const DESCRIPTION: &str = "Decide hunks of the open proposal of the OpenSpec change under discussion, the proposed new change of its split, or the pending request to apply or rework the change, on the author's explicit instruction in the latest comment. Use this only when the author tells you which hunks or which new change to accept, reject, or discuss, to confirm the split, or to confirm or reject the request, and copy their words into `instruction`. Name a hunk by its id, or by its place counted from 1; name the new change, the split, and the request by the ids your input lists. Confirming a request starts its run. Do not read the decision back and wait for a confirmation. This never creates or changes a hunk, a proposed change, or a request.";

/// The parameter text of `action`.
const ACTION_DESCRIPTION: &str = "`accept` to write the hunk into its file of the change or to create the proposed new change, `reject` to decline it or to decline the request, `discuss` to open a new discussion about it, or `confirm` with the id of the split to end it or with the id of the request to start its run.";

/// DCE-FR-JCHG / DPC-FR-JEXJ: the typed errors worth a retry.
const RETRYABLE_ERRORS: [&str; 3] =
    [cep::ERR_WRITE_FAILED, cep::ERR_ACCEPTANCE_IN_PROGRESS, cep::ERR_STREAM_BUSY];

/// The `id` of an entry: a hunk, the proposed new change, the split, or the
/// pending action request.
const ID_DESCRIPTION: &str = "The id of a hunk of the open proposal, of the proposed new change that waits on the author, of the split, or of the request to apply or rework the change that waits on the author, as your input lists it.";

/// DCE-FR-PHSU: the actions of an entry.
const ACTIONS: [&str; 4] = ["accept", "reject", "discuss", "confirm"];

/// TLC-FR-06: the JSON Schema of the arguments (DCE-FR-ILCP, DCE-FR-PHSU).
pub fn parameters() -> serde_json::Value {
    decide_changes::parameters_with(ACTION_DESCRIPTION, ID_DESCRIPTION, &ACTIONS)
}

/// DCE-FR-QIKC: `decide_change_edits` as a `rig` portable tool.
pub struct DecideChangeEditsTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: OwnedRoots,
    origin: ConversationOrigin,
    agent: Participant,
    turn_id: String,
    decider: Option<Participant>,
    sites: Option<std::sync::Arc<dyn SiteResolver + Send + Sync>>,
    /// DCE-FR-IZTA: the starts a confirmation calls, where a test fixes them.
    starter: Option<std::sync::Arc<dyn RunStarter + Send + Sync>>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> DecideChangeEditsTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    /// DCE-FR-PQAP: bound to the change of the turn's target, the turn's
    /// conversation, the turn's agent participant, and the turn.
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: OwnedRoots,
        origin: ConversationOrigin,
        agent: Participant,
        turn_id: &str,
    ) -> Self {
        DecideChangeEditsTool {
            app,
            roots,
            origin,
            agent,
            turn_id: turn_id.to_string(),
            decider: None,
            sites: None,
            starter: None,
            buffer: &BUFFER,
        }
    }

    /// The same tool, starting runs through `starter`.
    #[cfg(test)]
    pub fn with_starter(mut self, starter: Box<dyn RunStarter + Send + Sync>) -> Self {
        self.starter = Some(std::sync::Arc::from(starter));
        self
    }

    /// The same tool with the author, the sites, and the log buffer fixed.
    #[cfg(test)]
    pub fn for_test(
        mut self,
        decider: Participant,
        sites: Box<dyn SiteResolver + Send + Sync>,
        buffer: &'static LogBuffer,
    ) -> Self {
        self.decider = Some(decider);
        self.sites = Some(std::sync::Arc::from(sites));
        self.buffer = buffer;
        self
    }

    fn author(&self) -> Result<Participant, String> {
        if let Some(decider) = &self.decider {
            return Ok(decider.clone());
        }
        let store = self.app.try_state::<crate::global_settings::GlobalSettingsStore>();
        let project = self.app.try_state::<crate::project::ProjectState>();
        match (store, project) {
            (Some(store), Some(project)) => comments::acting_participant(&store, &project),
            _ => Err(cep::ERR_NO_PROJECT_OPEN.to_string()),
        }
    }

    fn change(&self) -> Option<(&str, &str)> {
        self.origin.target.change_owner()
    }

    /// The whole call, with its logging (DCE-FR-GAHP).
    fn run(&self, args: DecideChangesArgs) -> Result<DecideChangesOutput, CallRefusal> {
        let outcome = self.decide(&args);
        let (stream_id, change) = self.change().unwrap_or(("", ""));
        decide_changes::log_call_with(
            &self.app,
            self.buffer,
            NAME,
            &[("streamId", stream_id), ("change", change)],
            &outcome,
        );
        outcome
    }

    fn decide(&self, args: &DecideChangesArgs) -> Result<DecideChangesOutput, CallRefusal> {
        require_open_project(&self.app).map_err(CallRefusal::from)?;
        let Some((stream_id, change)) = self.change() else {
            return Err(CallRefusal::WrongConversation(decide_changes::NOT_A_CHANGE_CONVERSATION));
        };
        let instruction = decide_changes::check_arguments(args)?;
        let grant = crate::agent_conversations::authorize_decision(
            &self.app,
            self.roots.borrowed(),
            &self.origin,
            &self.turn_id,
            instruction,
        )
        .map_err(CallRefusal::Authority)?;
        let app_sites = AppSites { app: &self.app };
        let sites: &dyn SiteResolver = match &self.sites {
            Some(sites) => sites.as_ref(),
            None => &app_sites,
        };
        let app_starter = AppStarter { app: &self.app };
        let starter: &dyn RunStarter = match &self.starter {
            Some(starter) => starter.as_ref(),
            None => &app_starter,
        };
        let backend = ChangeBackend {
            tool: self,
            stream_id,
            change,
            sites,
            starter,
            author: self.author(),
            line: decide_changes::instruction_line(&self.agent, instruction),
            split: split::split_of(&self.roots.store, stream_id, change),
            request: car::pending_request(&self.roots.store, stream_id, change).map(|r| r.id),
        };
        let mut output = decide_changes::run_entries(&backend, args)?;
        // DCE contract surface: each row names its hunk's change-relative path.
        if let Some(open) = cep::list_proposals_impl(&self.roots.store, stream_id, change)
            .into_iter()
            .find(|p| p.ledger.iter().any(|row| output.decisions.iter().any(|d| d.id == row.id)))
        {
            for row in &mut output.decisions {
                row.path = open.row(&row.id).map(|r| r.path.clone());
            }
        }
        if decide_changes::decided_any(&output) {
            grant.commit();
        }
        Ok(output)
    }
}

/// The change proposal store, as the entry loop reaches it (CEP-FR-KLWZ).
struct ChangeBackend<'a, R: tauri::Runtime> {
    tool: &'a DecideChangeEditsTool<R>,
    stream_id: &'a str,
    change: &'a str,
    sites: &'a dyn SiteResolver,
    /// DCE-FR-IZTA: the starts of a confirmed action request.
    starter: &'a dyn RunStarter,
    author: Result<Participant, String>,
    line: String,
    /// DCE-FR-UZDN: the split as the call found it when it began.
    split: Option<split::SplitView>,
    /// DCE-FR-UZDN: the pending action request when the call began.
    request: Option<String>,
}

impl<R: tauri::Runtime> ChangeBackend<'_, R> {
    fn target(&self) -> DiscussionTarget {
        DiscussionTarget::Change {
            stream_id: self.stream_id.to_string(),
            change: self.change.to_string(),
        }
    }

    fn store(&self) -> &crate::fs::RootFs {
        &self.tool.roots.store
    }
}

fn hunk_view(state: HunkState) -> HunkStateView {
    match state {
        HunkState::Pending => HunkStateView::Pending,
        HunkState::Accepted => HunkStateView::Accepted,
        HunkState::Rejected => HunkStateView::Rejected,
        HunkState::Discussing => HunkStateView::Discussing,
        HunkState::Withdrawn => HunkStateView::Withdrawn,
    }
}

fn view_of(record: ChangeEditProposal) -> ProposalView {
    ProposalView {
        state: match record.state {
            ChangeProposalState::Pending => ProposalStateView::Pending,
            ChangeProposalState::Accepted => ProposalStateView::Accepted,
            ChangeProposalState::Rejected => ProposalStateView::Rejected,
        },
        hunks: record.ledger.iter().map(|row| (row.id.clone(), hunk_view(row.state))).collect(),
        id: record.id,
    }
}

impl<R: tauri::Runtime> ProposalBackend for ChangeBackend<'_, R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    fn file_word(&self) -> &'static str {
        "the change"
    }

    fn no_open_message(&self) -> &'static str {
        decide_changes::NO_OPEN_CHANGE_PROPOSAL
    }

    fn open_proposal(&self) -> Option<ProposalView> {
        cep::pending_change_proposal(self.store(), self.stream_id, self.change).map(view_of)
    }

    fn proposal(&self, proposal_id: &str) -> Option<ProposalView> {
        cep::list_proposals_impl(self.store(), self.stream_id, self.change)
            .into_iter()
            .find(|p| p.id == proposal_id)
            .map(view_of)
    }

    fn is_lost(&self, proposal_id: &str, hunk_id: &str) -> bool {
        cep::change_hunk_is_lost(self.store(), self.sites, proposal_id, hunk_id)
    }

    fn accept(&self, proposal_id: &str, hunk_id: &str) -> Result<Accepted, String> {
        let by = self.author.clone()?;
        let accepted = cep::accept_change_hunk_in_process(
            &self.tool.app,
            self.store(),
            self.sites,
            cep::ChangeHunkByTool {
                proposal_id,
                hunk_id,
                by: &by,
                instruction_line: Some(&self.line),
            },
        );
        match accepted {
            Ok(outcome) => Ok(Accepted { comment_owed: Some(outcome.proposal.comment_owed) }),
            Err(error) if error == cep::ERR_ACCEPTANCE_INCOMPLETE => {
                let owed =
                    cep::complete_in_process(&self.tool.app, self.store(), self.sites, proposal_id)
                        .map(|o| o.proposal.comment_owed)
                        .unwrap_or(true);
                Ok(Accepted { comment_owed: Some(owed) })
            }
            Err(error) => Err(error),
        }
    }

    fn reject(&self, proposal_id: &str, hunk_id: &str) -> Result<(), String> {
        let by = self.author.clone()?;
        cep::reject_change_hunk_in_process(
            &self.tool.app,
            self.store(),
            self.sites,
            cep::ChangeHunkByTool {
                proposal_id,
                hunk_id,
                by: &by,
                instruction_line: Some(&self.line),
            },
        )
        .map(|_| ())
    }

    fn discussions(&self) -> Vec<comments::Discussion> {
        comments::list_discussions_in(self.store(), &self.target())
    }

    fn open_discussion(&self, proposal_id: &str, hunk_id: &str) -> Result<String, String> {
        // DCE-FR-CXZG (as DPC-FR-WCBF): a new discussion of the change, opened
        // as the turn's agent, whose opening comment names the hunk.
        let path = cep::list_proposals_impl(self.store(), self.stream_id, self.change)
            .into_iter()
            .find(|p| p.id == proposal_id)
            .and_then(|p| p.row(hunk_id).map(|r| r.path.clone()))
            .unwrap_or_default();
        let target = self.target();
        let attachment = decide_changes::hunk_attachment(proposal_id, hunk_id, &target, &path);
        let opened = comments::open_discussion_in(
            self.store(),
            &self.tool.roots.worktree,
            &target,
            None,
            decide_changes::DISCUSSION_BODY.to_string(),
            vec![attachment],
            comments::Stamp { by: &self.tool.agent, at: &crate::notes::now_rfc3339() },
        )?;
        comments::emit_discussion_changed(&self.tool.app, &opened);
        Ok(opened.id)
    }

    fn hold(&self, proposal_id: &str, hunk_id: &str) -> Result<(), String> {
        cep::hold_in_process(&self.tool.app, self.store(), proposal_id, hunk_id, true).map(|_| ())
    }

    fn retryable_error(&self, error: &str) -> bool {
        RETRYABLE_ERRORS.contains(&error)
    }

    /// DCE-FR-PHSU: a pending new-change proposal, a split that can be
    /// confirmed, or a pending action request lets a call run.
    fn side_open(&self) -> bool {
        self.request.is_some()
            || self.split.as_ref().is_some_and(|s| s.pending.is_some() || s.confirmable)
    }

    fn side_entry(
        &self,
        id: &str,
        action: &str,
        place: usize,
        total: usize,
    ) -> Option<decide_changes::DecisionRow> {
        if self.request.as_deref() == Some(id) {
            return Some(action_request::entry(self, id, action, place, total));
        }
        // DCE-FR-WRSI: a `confirm` that names neither the split nor the
        // pending request. With a split, the split's entry refuses it.
        if action == "confirm" && self.split.is_none() && self.request.is_some() {
            let mut row = decide_changes::DecisionRow::new(place, id, action);
            let text =
                "the id you gave is not a change of the open proposal. Use an id from your input.";
            row.refuse(decide_changes::RefusalCode::UnknownHunk, total, text, None);
            return Some(row);
        }
        split::side_entry(self, id, action, place, total)
    }
}

mod action_request;
mod split;

impl<R: tauri::Runtime> rig::tool::PortableTool for DecideChangeEditsTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    const NAME: &'static str = NAME;

    type Args = DecideChangesArgs;
    type Output = DecideChangesOutput;
    type Error = CallRefusal;

    fn description(&self) -> String {
        DESCRIPTION.to_string()
    }

    fn parameters(&self) -> serde_json::Value {
        parameters()
    }

    fn map_error(&self, error: Self::Error) -> rig::tool::ToolExecutionError {
        error.to_execution_error()
    }

    fn call(
        &self,
        arguments: Self::Args,
    ) -> impl std::future::Future<Output = Result<Self::Output, Self::Error>> + Send {
        // CAR-FR-AQHX: a confirmation takes the graduation write guard and can
        // reach the network, so the decisions run on a blocking thread.
        let this = self.clone();
        async move {
            match tauri::async_runtime::spawn_blocking(move || this.run(arguments)).await {
                Ok(result) => result,
                // A panic in the decisions stays a panic of the turn, as it
                // was when the decisions ran on the turn's own thread.
                Err(error) => panic!("the decisions of decide_change_edits stopped: {error}"),
            }
        }
    }
}

impl<R: tauri::Runtime> Clone for DecideChangeEditsTool<R> {
    fn clone(&self) -> Self {
        Self {
            app: self.app.clone(),
            roots: self.roots.clone(),
            origin: self.origin.clone(),
            agent: self.agent.clone(),
            turn_id: self.turn_id.clone(),
            decider: self.decider.clone(),
            sites: self.sites.clone(),
            starter: self.starter.clone(),
            buffer: self.buffer,
        }
    }
}

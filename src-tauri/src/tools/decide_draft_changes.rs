//! Decide draft changes tool (`DDC-decide-draft-changes-tool.md`).
//!
//! The tool an agent uses when the author tells it to decide a change of the
//! open proposal of the draft under discussion: "accept the second change",
//! "reject the last one", "let us discuss the first". Each entry runs the
//! operation of the matching review chip, through the same functions the
//! chip's command calls (DCP-FR-LAWR). So a decision by tool is the author's
//! decision and not the agent's (DCP-FR-CIPW).
//!
//! ## Why it is constructed per turn
//!
//! The draft, the conversation, and the agent participant are bound by the
//! constructor (DDC-FR-QRIY, TLC-FR-15). No argument names a draft, a
//! proposal, a conversation, or a participant, so no call can reach a draft
//! that the agent was not asked about.
//!
//! ## Why a call does not end the turn
//!
//! The agent tells the author what it did in its reply (DDC-FR-PWTE), so the
//! loop continues after the result. A close that the tool causes dispatches
//! no agent turn: the calling turn is running and reads the close in the
//! result (DDC-FR-KQZN).

use tauri::Manager;

use super::decide_changes::{
    self, Accepted, CallRefusal, DecideChangesArgs, DecideChangesOutput, DecisionRow,
    HunkStateView, ProposalBackend, ProposalStateView, ProposalView,
};
use super::require_open_project;

use crate::agent_conversations::{ConversationOrigin, OwnedRoots};
use crate::comments::{self, DiscussionTarget, Participant};
use crate::draft_proposals::{self, hunks::HunkState, Decision, ProposalState, Verdict};
use crate::logging::{LogBuffer, LogSink, BUFFER};

/// DDC-FR-SQYQ.
pub const NAME: &str = "decide_draft_changes";

/// DDC-FR-VAYO / DDC-FR-ROLG / DDC-FR-MDCX: the fixed text the model reads.
pub const DESCRIPTION: &str = "Decide changes of the open proposal of the draft under discussion, when the author tells you to. Call it only when the author's latest comment tells you plainly to accept, reject, or discuss a change, for example \"accept the second change\". Never call it on your own judgement, on an earlier comment, or to tidy up a proposal. Do not read the decision back and wait for a confirmation: the author's instruction is the confirmation. Pass `decisions`, one entry for each change the author named, in the order the author named them. Each entry gives the `id` of a change and an `action`. `accept` writes the change into the prompt as it stands, with any edit the author made to it. `reject` writes nothing. `discuss` opens a new discussion about that change, with a first comment from you that says the author asked to discuss it; it does not decide the change. A rejected change can still be accepted while the proposal is open. Your input lists each change of the open proposal with its id, its place, its state, and its text. When the author names a change by its place, count the changes in their order and skip the withdrawn ones, as the review does. Each entry runs on its own: a refused entry stops no other, and the result tells you what happened to each. Tell the author in your reply what you did. The same entries decide the proposed new draft that waits on the author during a decomposition: give its id with `accept`, `reject`, or `discuss`. When the author tells you to confirm the decomposition, give the id of the decomposition with `confirm`. Copy into `instruction` the words of the author's latest comment that ask for these decisions, exactly as the author wrote or said them. A call is refused when the latest comment is not the author's, when the words are not in it, or when that comment already gave a decision. While the author is still speaking, wait until the comment of the speech arrives. This tool never makes, changes, or withdraws a change or a proposed draft; use `propose_draft_changes` or `propose_new_draft` for that.";

/// DDC-FR-VAYO: the parameter text of `action`.
const ACTION_DESCRIPTION: &str = "`accept` to write the change into the prompt, `reject` to decline it, `discuss` to open a new discussion about it, or `confirm` with the id of the decomposition to end it.";

/// DDC-FR-LYTA: the typed errors worth a retry.
const RETRYABLE_ERRORS: [&str; 3] = [
    crate::draft_history::ERR_WRITE_FAILED,
    crate::draft_history::ERR_ACCEPTANCE_IN_PROGRESS,
    crate::draft_history::ERR_HISTORY_RECOVERY_FAILED,
];

/// TLC-FR-06: the JSON Schema of [`DecideChangesArgs`] for this tool.
/// DDC contract surface: the `id` description, which names a proposed draft
/// too (DDC-FR-GGWO).
const ID_DESCRIPTION: &str = "The id of a change of the open proposal, of the proposed new draft that waits on the author, or of the decomposition, as your input lists it.";

/// DDC contract surface: a draft takes `confirm` beside the three hunk actions.
const ACTIONS: [&str; 4] = ["accept", "reject", "discuss", "confirm"];

pub fn parameters() -> serde_json::Value {
    decide_changes::parameters_with(ACTION_DESCRIPTION, ID_DESCRIPTION, &ACTIONS)
}

/// DDC-FR-SQYQ: `decide_draft_changes` as a `rig` portable tool.
pub struct DecideDraftChangesTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: OwnedRoots,
    origin: ConversationOrigin,
    agent: Participant,
    /// AGC-FR-DTCL: the turn that calls the tool, whose received comments
    /// carry the author's authority (DDC-FR-ELAE).
    turn_id: String,
    /// The author a decision is recorded as. `None` resolves the author from
    /// the open project at call time, as the chip's command does.
    decider: Option<Participant>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> DecideDraftChangesTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    /// DDC-FR-QRIY: bound to the turn's draft (through its origin), the
    /// origin of the turn's conversation, the turn's agent participant, and
    /// the turn itself (DDC-FR-ELAE).
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: OwnedRoots,
        origin: ConversationOrigin,
        agent: Participant,
        turn_id: &str,
    ) -> Self {
        DecideDraftChangesTool {
            app,
            roots,
            origin,
            agent,
            turn_id: turn_id.to_string(),
            decider: None,
            buffer: &BUFFER,
        }
    }

    /// The same tool, with the author fixed and reporting into `buffer`. A
    /// test app manages no settings store to resolve the author from.
    #[cfg(test)]
    pub fn for_test(mut self, decider: Participant, buffer: &'static LogBuffer) -> Self {
        self.decider = Some(decider);
        self.buffer = buffer;
        self
    }

    /// CMS-FR-11 / DCP-FR-LAWR: the author, resolved as the chip's command
    /// resolves it.
    fn author(&self) -> Result<Participant, String> {
        if let Some(decider) = &self.decider {
            return Ok(decider.clone());
        }
        let missing = || crate::prompt_proposals::ERR_NO_PROJECT_OPEN.to_string();
        let store = self.app.try_state::<crate::global_settings::GlobalSettingsStore>();
        let project = self.app.try_state::<crate::project::ProjectState>();
        match (store, project) {
            (Some(store), Some(project)) => comments::acting_participant(&store, &project),
            _ => Err(missing()),
        }
    }

    /// The whole call, with its logging (DDC-FR-TBVA).
    fn run(&self, args: DecideChangesArgs) -> Result<DecideChangesOutput, CallRefusal> {
        let outcome = self.decide(&args);
        let draft = self.origin.draft_id().unwrap_or("");
        decide_changes::log_call(&self.app, self.buffer, NAME, ("draftId", draft), &outcome);
        outcome
    }

    fn decide(&self, args: &DecideChangesArgs) -> Result<DecideChangesOutput, CallRefusal> {
        // DDC-FR-OEJB / TLC-FR-13: checked per call, because a turn can
        // outlive the project being open.
        require_open_project(&self.app).map_err(CallRefusal::from)?;
        // DDC-FR-OEJB: unreachable while CVL-FR-08 attaches this tool to draft
        // origins alone, and checked anyway.
        let Some(draft_id) = self.origin.draft_id() else {
            return Err(CallRefusal::WrongConversation(decide_changes::NOT_A_DRAFT_CONVERSATION));
        };
        // DDC-FR-VYLJ: the malformed arguments, then the author's authority,
        // then the no-open-proposal refusal of the entry loop.
        let instruction = decide_changes::check_arguments(args)?;
        let grant = crate::agent_conversations::authorize_decision(
            &self.app,
            self.roots.borrowed(),
            &self.origin,
            &self.turn_id,
            instruction,
        )
        .map_err(CallRefusal::Authority)?;
        // DDC-FR-GGWO / DDC-FR-ZWKS: the new-draft proposal pending when the call
        // began. Only its id is a new-draft entry of this call.
        let new_draft_open =
            crate::new_draft_proposals::pending_for(&self.roots.worktree, draft_id).map(|p| p.id);
        let backend = DraftBackend {
            tool: self,
            draft_id,
            author: self.author(),
            new_draft_open,
            decomposition: new_drafts::decomposition_of(&self.roots.worktree, draft_id),
            line: decide_changes::instruction_line(&self.agent, instruction),
        };
        let output = decide_changes::run_entries(&backend, args)?;
        // DDC-FR-VONX: a call that decided an entry spends the comment. A call
        // that decided nothing gives it back as the grant drops.
        if decide_changes::decided_any(&output) {
            grant.commit();
        }
        Ok(output)
    }
}

/// The draft store, as the entry loop reaches it (DCP-FR-LAWR).
struct DraftBackend<'a, R: tauri::Runtime> {
    tool: &'a DecideDraftChangesTool<R>,
    draft_id: &'a str,
    /// Resolved once per call. A failure refuses each accept and reject with
    /// `decision_failed`, as the chip's command refuses.
    author: Result<Participant, String>,
    /// DDC-FR-GGWO: the pending new-draft proposal when the call began.
    new_draft_open: Option<String>,
    /// DDC-FR-VEKP / DDC-FR-ZGXF: the decomposition of the draft, if it has
    /// one, when the call began.
    decomposition: Option<new_drafts::Decomposition>,
    /// DDC-FR-JJHZ: the instruction line that ends each decision comment and
    /// confirmation comment that this call causes.
    line: String,
}

impl<R: tauri::Runtime> DraftBackend<'_, R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    fn target(&self) -> DiscussionTarget {
        DiscussionTarget::Draft { draft_id: self.draft_id.to_string() }
    }

    /// DCP-FR-LAWR: one hunk decision, through the chip's own function.
    fn decide(&self, proposal_id: &str, hunk_id: &str, decision: Decision) -> Result<(), String> {
        let by = self.author.clone()?;
        // DDC-FR-JJHZ / DCP-FR-HMUG: no feedback. The instruction line takes
        // the place feedback takes in the decision comment, after one empty
        // line, so the body ends with it and holds nothing else of the call.
        draft_proposals::decide_hunk_in_process(
            &self.tool.app,
            &self.tool.roots.worktree,
            &self.tool.roots.store,
            proposal_id,
            Verdict { decision, hunk: Some(hunk_id), feedback: Some(&self.line) },
            &by,
        )
        .map(|_| ())
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

fn view_of(record: draft_proposals::DraftChangeProposal) -> ProposalView {
    ProposalView {
        state: match record.state {
            ProposalState::Pending => ProposalStateView::Pending,
            ProposalState::Accepted => ProposalStateView::Accepted,
            ProposalState::Rejected => ProposalStateView::Rejected,
        },
        hunks: record.ledger.iter().map(|row| (row.id.clone(), hunk_view(row.state))).collect(),
        id: record.id,
    }
}

impl<R: tauri::Runtime> ProposalBackend for DraftBackend<'_, R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    fn file_word(&self) -> &'static str {
        "the prompt"
    }

    fn no_open_message(&self) -> &'static str {
        decide_changes::NO_OPEN_DRAFT_PROPOSAL
    }

    fn open_proposal(&self) -> Option<ProposalView> {
        // DDC-FR-ZGXF: the draft's open proposal (DCP-FR-PWSF).
        let open = draft_proposals::pending_for(&self.tool.roots.worktree, self.draft_id)?;
        self.proposal(&open.id)
    }

    fn proposal(&self, proposal_id: &str) -> Option<ProposalView> {
        draft_proposals::proposal_as_it_stands(&self.tool.roots.worktree, proposal_id)
            .filter(|record| record.draft_id == self.draft_id)
            .map(view_of)
    }

    fn is_lost(&self, proposal_id: &str, hunk_id: &str) -> bool {
        draft_proposals::hunk_is_lost(&self.tool.roots.worktree, proposal_id, hunk_id)
    }

    fn accept(&self, proposal_id: &str, hunk_id: &str) -> Result<Accepted, String> {
        // DDC-FR-VUJQ / DDC-FR-YKPP: the chip's Accept, with no feedback.
        self.decide(proposal_id, hunk_id, Decision::Accept)?;
        Ok(Accepted { comment_owed: None })
    }

    fn reject(&self, proposal_id: &str, hunk_id: &str) -> Result<(), String> {
        // DDC-FR-QFNQ: the chip's Reject, with no feedback.
        self.decide(proposal_id, hunk_id, Decision::Decline)
    }

    fn discussions(&self) -> Vec<comments::Discussion> {
        comments::list_discussions_in(&self.tool.roots.store, &self.target())
    }

    fn open_discussion(&self, proposal_id: &str, hunk_id: &str) -> Result<String, String> {
        // DDC-FR-EEAZ / DDC-FR-ILCB: a new discussion of the draft, opened as
        // the turn's agent, whose opening comment names the hunk.
        let path = draft_proposals::proposal_as_it_stands(&self.tool.roots.worktree, proposal_id)
            .map(|record| record.path)
            .unwrap_or_default();
        let target = self.target();
        let attachment = decide_changes::hunk_attachment(proposal_id, hunk_id, &target, &path);
        let opened = comments::open_discussion_in(
            &self.tool.roots.store,
            &self.tool.roots.worktree,
            &target,
            None,
            decide_changes::DISCUSSION_BODY.to_string(),
            vec![attachment],
            comments::Stamp { by: &self.tool.agent, at: &crate::notes::now_rfc3339() },
        )?;
        // DCR-FR-KHEE: the discussion column shows it as any new discussion.
        comments::emit_discussion_changed(&self.tool.app, &opened);
        // DDC-FR-NLDN: no agent turn is dispatched into it.
        Ok(opened.id)
    }

    fn hold(&self, proposal_id: &str, hunk_id: &str) -> Result<(), String> {
        // DDC-FR-NKIA: "hold draft change hunk", as DCR-FR-JTXE does.
        draft_proposals::hold_hunk_in_process(
            &self.tool.app,
            &self.tool.roots.worktree,
            proposal_id,
            hunk_id,
            true,
        )
        .map(|_| ())
    }

    fn retryable_error(&self, error: &str) -> bool {
        RETRYABLE_ERRORS.contains(&error)
    }

    fn side_open(&self) -> bool {
        new_drafts::side_open(self)
    }

    fn side_entry(
        &self,
        id: &str,
        action: &str,
        place: usize,
        total: usize,
    ) -> Option<DecisionRow> {
        new_drafts::side_entry(self, id, action, place, total)
    }
}

impl<R: tauri::Runtime> rig::tool::PortableTool for DecideDraftChangesTool<R>
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
        // TLC-FR-16: every operation it runs is synchronous, so the call
        // resolves before the future is polled.
        std::future::ready(self.run(arguments))
    }
}

mod new_drafts;

#[cfg(test)]
mod tests;

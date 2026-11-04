//! Decide prompt changes tool (`DPC-decide-prompt-changes-tool.md`).
//!
//! The twin of `decide_draft_changes` for a file the project already holds: a
//! published prompt or a project document. Each entry runs the operation of
//! the matching chip of the file's review, through the same functions the
//! chip's command calls (PCP-FR-MXSB). So a decision by tool is the author's
//! decision and not the agent's (PCP-FR-VCBX).
//!
//! ## Why an incomplete acceptance is an accepted entry
//!
//! The file is written at the commit point of the acceptance (PCP-FR-14). An
//! `acceptance_incomplete` answer says the change has landed and only the
//! decision comment is still owed. So the entry is accepted, the tool asks for
//! the completion once, and the row says where the comment is still owed
//! (DPC-FR-WNHU). The review then offers its own retry (PCR-FR-KCVC).

use tauri::Manager;

use super::decide_changes::{
    self, Accepted, CallRefusal, DecideChangesArgs, DecideChangesOutput, HunkStateView,
    ProposalBackend, ProposalStateView, ProposalView,
};
use super::require_open_project;

use crate::agent_conversations::{ConversationOrigin, OwnedRoots};
use crate::artifacts::ContentTracker;
use crate::comments::{self, DiscussionTarget, Participant};
use crate::logging::{LogBuffer, LogSink, BUFFER};
use crate::prompt_proposals::{self, HunkState, PromptProposalState};

/// DPC-FR-AFSM.
pub const NAME: &str = "decide_prompt_changes";

/// DPC-FR-RJBJ / DPC-FR-HWOT: the fixed text the model reads.
pub const DESCRIPTION: &str = "Decide changes of the open proposal of the prompt or project document under discussion, when the author tells you to. Call it only when the author's latest comment tells you plainly to accept, reject, or discuss a change, for example \"accept the second change\". Never call it on your own judgement, on an earlier comment, or to tidy up a proposal. Do not read the decision back and wait for a confirmation: the author's instruction is the confirmation. Pass `decisions`, one entry for each change the author named, in the order the author named them. Each entry gives the `id` of a change and an `action`. `accept` writes the change into the file as it stands, with any edit the author made to it. `reject` writes nothing. `discuss` opens a new discussion about that change, with a first comment from you that says the author asked to discuss it; it does not decide the change. A rejected change can still be accepted while the proposal is open. Your input lists each change of the open proposal with its id, its place, its state, and its text. When the author names a change by its place, count the changes in their order and skip the withdrawn ones, as the review does. Each entry runs on its own: a refused entry stops no other, and the result tells you what happened to each. Tell the author in your reply what you did. Copy into `instruction` the words of the author's latest comment that ask for these decisions, exactly as the author wrote or said them. A call is refused when the latest comment is not the author's, when the words are not in it, or when that comment already gave a decision. While the author is still speaking, wait until the comment of the speech arrives. This tool never makes, changes, or withdraws a change; use `propose_prompt_changes` for that.";

/// DPC-FR-RJBJ: the parameter text of `action`.
const ACTION_DESCRIPTION: &str = "`accept` to write the change into the file, `reject` to decline it, or `discuss` to open a new discussion about it.";

/// DPC-FR-JEXJ: the typed errors worth a retry. `artifact_not_found`,
/// `not_a_prompt_artifact`, and every other error are not.
const RETRYABLE_ERRORS: [&str; 2] =
    [prompt_proposals::ERR_WRITE_FAILED, prompt_proposals::ERR_ACCEPTANCE_IN_PROGRESS];

/// TLC-FR-06: the JSON Schema of [`DecideChangesArgs`] for this tool.
pub fn parameters() -> serde_json::Value {
    decide_changes::parameters(ACTION_DESCRIPTION)
}

/// DPC-FR-AFSM: `decide_prompt_changes` as a `rig` portable tool.
pub struct DecidePromptChangesTool<R: tauri::Runtime> {
    app: tauri::AppHandle<R>,
    roots: OwnedRoots,
    origin: ConversationOrigin,
    agent: Participant,
    /// AGC-FR-DTCL: the turn that calls the tool (DPC-FR-VJIG).
    turn_id: String,
    /// The author a decision is recorded as. `None` resolves the author from
    /// the open project at call time, as the chip's command does.
    decider: Option<Participant>,
    buffer: &'static LogBuffer,
}

impl<R: tauri::Runtime> DecidePromptChangesTool<R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    /// DPC-FR-QMZE: bound to the artifact of the turn's target (through its
    /// origin), the origin of the turn's conversation, and the turn's agent
    /// participant, and the turn itself (DPC-FR-VJIG).
    pub fn new(
        app: tauri::AppHandle<R>,
        roots: OwnedRoots,
        origin: ConversationOrigin,
        agent: Participant,
        turn_id: &str,
    ) -> Self {
        DecidePromptChangesTool {
            app,
            roots,
            origin,
            agent,
            turn_id: turn_id.to_string(),
            decider: None,
            buffer: &BUFFER,
        }
    }

    /// The same tool, with the author fixed and reporting into `buffer`.
    #[cfg(test)]
    pub fn for_test(mut self, decider: Participant, buffer: &'static LogBuffer) -> Self {
        self.decider = Some(decider);
        self.buffer = buffer;
        self
    }

    /// PCP-FR-16 / PCP-FR-MXSB: the author, resolved as the chip's command
    /// resolves it.
    fn author(&self) -> Result<Participant, String> {
        if let Some(decider) = &self.decider {
            return Ok(decider.clone());
        }
        let store = self.app.try_state::<crate::global_settings::GlobalSettingsStore>();
        let project = self.app.try_state::<crate::project::ProjectState>();
        match (store, project) {
            (Some(store), Some(project)) => comments::acting_participant(&store, &project),
            _ => Err(prompt_proposals::ERR_NO_PROJECT_OPEN.to_string()),
        }
    }

    fn artifact_id(&self) -> Option<&str> {
        match &self.origin.target {
            DiscussionTarget::Artifact { artifact_id } => Some(artifact_id),
            _ => None,
        }
    }

    /// The whole call, with its logging (DPC-FR-CIRL).
    fn run(&self, args: DecideChangesArgs) -> Result<DecideChangesOutput, CallRefusal> {
        let outcome = self.decide(&args);
        let path = self.artifact_id().unwrap_or("");
        decide_changes::log_call(&self.app, self.buffer, NAME, ("path", path), &outcome);
        outcome
    }

    fn decide(&self, args: &DecideChangesArgs) -> Result<DecideChangesOutput, CallRefusal> {
        // DPC-FR-JJGY / TLC-FR-13.
        require_open_project(&self.app).map_err(CallRefusal::from)?;
        let Some(artifact_id) = self.artifact_id() else {
            return Err(CallRefusal::WrongConversation(
                decide_changes::NOT_AN_ARTIFACT_CONVERSATION,
            ));
        };
        // DPC-FR-VJIG: the authority rules of DDC, in the order of DDC-FR-VYLJ.
        let instruction = decide_changes::check_arguments(args)?;
        let grant = crate::agent_conversations::authorize_decision(
            &self.app,
            self.roots.borrowed(),
            &self.origin,
            &self.turn_id,
            instruction,
        )
        .map_err(CallRefusal::Authority)?;
        let backend = PromptBackend {
            tool: self,
            artifact_id,
            author: self.author(),
            line: decide_changes::instruction_line(&self.agent, instruction),
        };
        let output = decide_changes::run_entries(&backend, args)?;
        // DDC-FR-VONX: a call that decided an entry spends the comment.
        if decide_changes::decided_any(&output) {
            grant.commit();
        }
        Ok(output)
    }
}

/// The prompt proposal store, as the entry loop reaches it (PCP-FR-MXSB).
struct PromptBackend<'a, R: tauri::Runtime> {
    tool: &'a DecidePromptChangesTool<R>,
    artifact_id: &'a str,
    author: Result<Participant, String>,
    /// PCP-FR-QKCT: the instruction line of the decision comment.
    line: String,
}

impl<R: tauri::Runtime> PromptBackend<'_, R> {
    fn target(&self) -> DiscussionTarget {
        DiscussionTarget::Artifact { artifact_id: self.artifact_id.to_string() }
    }

    fn tracker(&self) -> Result<tauri::State<'_, ContentTracker>, String> {
        self.tool
            .app
            .try_state::<ContentTracker>()
            .ok_or_else(|| prompt_proposals::ERR_NO_PROJECT_OPEN.to_string())
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

fn view_of(record: prompt_proposals::PromptChangeProposal) -> ProposalView {
    ProposalView {
        state: match record.state {
            PromptProposalState::Pending => ProposalStateView::Pending,
            PromptProposalState::Accepted => ProposalStateView::Accepted,
            PromptProposalState::Rejected => ProposalStateView::Rejected,
        },
        hunks: record.ledger.iter().map(|row| (row.id.clone(), hunk_view(row.state))).collect(),
        id: record.id,
    }
}

impl<R: tauri::Runtime> ProposalBackend for PromptBackend<'_, R>
where
    tauri::AppHandle<R>: LogSink + Clone + Send + 'static,
{
    fn file_word(&self) -> &'static str {
        "the file"
    }

    fn no_open_message(&self) -> &'static str {
        decide_changes::NO_OPEN_FILE_PROPOSAL
    }

    fn open_proposal(&self) -> Option<ProposalView> {
        // DPC-FR-RGGP: the artifact's `pending` proposal (PCP-FR-04).
        prompt_proposals::pending_for(&self.tool.roots.worktree, self.artifact_id).map(view_of)
    }

    fn proposal(&self, proposal_id: &str) -> Option<ProposalView> {
        prompt_proposals::read_proposal(&self.tool.roots.worktree, proposal_id)
            .ok()
            .filter(|record| record.artifact_id == self.artifact_id)
            .map(view_of)
    }

    fn is_lost(&self, proposal_id: &str, hunk_id: &str) -> bool {
        prompt_proposals::hunk_is_lost(&self.tool.roots.worktree, proposal_id, hunk_id)
    }

    fn accept(&self, proposal_id: &str, hunk_id: &str) -> Result<Accepted, String> {
        // DPC-FR-DQQN: the chip's Accept, with no feedback, through the file's
        // own write path. DPC-FR-TVKX: it does not wait for an Editor tab.
        let by = self.author.clone()?;
        let tracker = self.tracker()?;
        let (worktree, store) = (&self.tool.roots.worktree, &self.tool.roots.store);
        let accepted = prompt_proposals::accept_hunk_in_process(
            &self.tool.app,
            worktree,
            store,
            prompt_proposals::HunkByTool {
                proposal_id,
                hunk_id,
                by: &by,
                instruction_line: Some(&self.line),
            },
            &tracker,
        );
        match accepted {
            Ok(outcome) => Ok(Accepted { comment_owed: Some(outcome.proposal.comment_owed) }),
            // DPC-FR-WNHU: the change has landed and the comment is owed. Ask
            // for the completion once.
            Err(error) if error == prompt_proposals::ERR_ACCEPTANCE_INCOMPLETE => {
                let owed = match prompt_proposals::complete_in_process(
                    &self.tool.app,
                    worktree,
                    store,
                    proposal_id,
                ) {
                    Ok(outcome) => outcome.proposal.comment_owed,
                    Err(_) => true,
                };
                Ok(Accepted { comment_owed: Some(owed) })
            }
            Err(error) => Err(error),
        }
    }

    fn reject(&self, proposal_id: &str, hunk_id: &str) -> Result<(), String> {
        // DPC-FR-QVKH: the chip's Reject, with no feedback.
        let by = self.author.clone()?;
        let tracker = self.tracker()?;
        prompt_proposals::reject_hunk_in_process(
            &self.tool.app,
            &self.tool.roots.worktree,
            &self.tool.roots.store,
            prompt_proposals::HunkByTool {
                proposal_id,
                hunk_id,
                by: &by,
                instruction_line: Some(&self.line),
            },
            &tracker,
        )
        .map(|_| ())
    }

    fn discussions(&self) -> Vec<comments::Discussion> {
        comments::list_discussions_in(&self.tool.roots.store, &self.target())
    }

    fn open_discussion(&self, proposal_id: &str, hunk_id: &str) -> Result<String, String> {
        // DPC-FR-WCBF / DPC-FR-AFVA: a new discussion of the artifact, opened
        // as the turn's agent, whose opening comment names the hunk.
        let path = prompt_proposals::read_proposal(&self.tool.roots.worktree, proposal_id)
            .map(|record| record.path)
            .unwrap_or_else(|_| self.artifact_id.to_string());
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
        // PCR-FR-PFOW: the discussion column shows it as any new discussion.
        comments::emit_discussion_changed(&self.tool.app, &opened);
        // DPC-FR-QPNB: no agent turn is dispatched into it.
        Ok(opened.id)
    }

    fn hold(&self, proposal_id: &str, hunk_id: &str) -> Result<(), String> {
        // DPC-FR-QPNB: "hold prompt change hunk" (PCP-FR-HLDQ).
        prompt_proposals::hold_in_process(
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
}

impl<R: tauri::Runtime> rig::tool::PortableTool for DecidePromptChangesTool<R>
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
        // TLC-FR-16: every operation it runs is synchronous.
        std::future::ready(self.run(arguments))
    }
}

#[cfg(test)]
mod tests;

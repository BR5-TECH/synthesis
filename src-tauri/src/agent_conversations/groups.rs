//! Agent groups in the engine: the dispatch of a group, the wait of a Meta turn,
//! the classifier call, and the synthesis turn (AGC-FR-KOHJ).
//!
//! The pure parts — the mode, the join, the shared state — are in
//! `crate::agent_groups`. This module joins them to the turn registry.

use super::*;
use crate::agent_groups::{
    self, parse_advice, parse_driver, plan_mode, ready_nicknames, GroupMode, GroupState,
    JoinOutcome, MemberStatus, PlannedMode,
};
use crate::agents::Agent;

/// CVL-FR-VUDT: the fixed prompt of the classifier.
const CLASSIFIER_PROMPT: &str = include_str!("../../../resources/prompts/group-classifier.md");
/// CVL-FR-OGXZ: the advisor prompt.
pub(super) const ADVISOR_PROMPT: &str = include_str!("../../../resources/prompts/group-advisor.md");
/// CVL-FR-XIBC: the synthesis prompt.
const SYNTHESIS_PROMPT: &str = include_str!("../../../resources/prompts/group-synthesis.md");
/// CVL-FR-RBQV: the note a Meta turn receives about the advisors tool.
const META_PROMPT: &str = include_str!("../../../resources/prompts/group-meta.md");

/// AGC-FR-WPCZ: the limit of the classifier call.
const CLASSIFIER_LIMIT: Duration = Duration::from_secs(30);
/// AGC-FR-WPCZ: the most characters of one persona's instructions that the
/// classifier reads.
const CLASSIFIER_INSTRUCTIONS_CHARS: usize = 2_000;

/// CVL-FR-HNGB: the tools an advisor turn does not carry — the tools that end a
/// turn on a contribution, and the tools that decide a proposal.
pub(super) const ADVISOR_WITHHELD_TOOLS: [&str; 12] = [
    crate::tools::ask_user_comment::NAME,
    crate::tools::request_document_export::NAME,
    crate::tools::ask_discussion_questions::NAME,
    crate::tools::propose_draft_changes::NAME,
    crate::tools::propose_new_draft::NAME,
    crate::tools::propose_prompt_changes::NAME,
    crate::tools::decide_draft_changes::NAME,
    crate::tools::decide_prompt_changes::NAME,
    crate::tools::propose_change_edits::NAME,
    crate::tools::decide_change_edits::NAME,
    crate::tools::propose_new_change::NAME,
    // CVL-FR-HNGB / AGC-FR-UFAY: a change action request ends a turn on a
    // contribution.
    crate::tools::request_change_action::NAME,
];

static NEXT_GROUP: AtomicU64 = AtomicU64::new(1);

/// The group a running turn belongs to.
pub(super) struct GroupBinding {
    pub(super) state: Arc<GroupState>,
    pub(super) role: GroupRole,
    /// AGC-FR-VDOO: the calls that a Meta turn made to the advisors tool.
    pub(super) advisor_calls: Option<Arc<AdvisorCalls>>,
}

/// What a dispatch needs to register a turn in a group.
pub(super) struct GroupSetup {
    pub(super) state: Arc<GroupState>,
    pub(super) role: GroupRole,
    /// AGC-FR-BIPE: the Meta turn that an advisor reports its tool calls to.
    pub(super) meta_turn_id: Option<String>,
    pub(super) extra_sections: Vec<InputSection>,
    pub(super) instructions_suffix: Option<String>,
    /// AGC-FR-HVQD: the advisors tool of a Meta turn.
    pub(super) advisors: Option<AdvisorsSetup>,
}

/// What `dispatch_agent_group` answers (AGC-FR-KOHJ).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentGroup {
    /// `regular`, `advisor`, or `synthesis`.
    pub mode: String,
    pub group_id: Option<String>,
    pub driver: Option<String>,
    pub turns: Vec<AgentTurn>,
    /// The addressed agents that got no turn while other turns were registered.
    pub refusals: Vec<GroupRefusal>,
}

/// AGC-FR-KOHJ: an addressed agent that the dispatch refused, with the typed
/// refusal of a regular dispatch.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupRefusal {
    pub nickname: String,
    pub failure: String,
}

fn section(tag: &str, body: String) -> InputSection {
    InputSection {
        tag: tag.to_string(),
        attributes: Vec::new(),
        body,
        truncated: false,
        parts: Vec::new(),
    }
}

pub(super) fn new_group_id() -> String {
    format!("group-{}", NEXT_GROUP.fetch_add(1, Ordering::Relaxed))
}

fn distinct(nicknames: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for nickname in nicknames {
        if !out.iter().any(|seen| seen.eq_ignore_ascii_case(nickname)) {
            out.push(nickname.clone());
        }
    }
    out
}

pub(super) fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// AGC-FR-KOHJ: dispatch the agents one message addresses.
pub fn dispatch_group_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    project_key: &str,
    nicknames: &[String],
    origin: ConversationOrigin,
    trigger_comment_id: &str,
) -> Result<AgentGroup, String> {
    use tauri::Manager;
    let store = app.state::<GlobalSettingsStore>();
    let turns = app.state::<TurnRegistry>();
    let nicknames = distinct(nicknames);
    let settings = store.load_agent_group_settings(project_key).unwrap_or_default();
    if !settings.enabled {
        return dispatch_regular(app, roots, project_key, &nicknames, origin, trigger_comment_id);
    }
    let roster =
        agents::list_project_agents_impl(&store, project_key, roots.worktree).unwrap_or_default();
    let ready = ready_nicknames(&roster);
    let addressed_ready: Vec<String> =
        nicknames.iter().filter(|n| ready.iter().any(|r| same(r, n))).cloned().collect();
    let other_ready = ready.iter().filter(|r| !nicknames.iter().any(|n| same(r, n))).count();
    let mode = plan_mode(true, nicknames.len(), addressed_ready.len(), other_ready);
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai],
        "agent group planned",
        log_fields! {
            "mode" => format!("{mode:?}"),
            "addressed" => nicknames.len(),
            "addressedReady" => addressed_ready.len(),
            "otherReady" => other_ready,
            "discussionId" => origin.discussion_id(),
        },
    );
    let timeout = Duration::from_secs(u64::from(settings.join_timeout_secs));
    match mode {
        PlannedMode::Regular => {
            dispatch_regular(app, roots, project_key, &nicknames, origin, trigger_comment_id)
        }
        PlannedMode::Advisor => dispatch_advisor(
            app,
            roots,
            project_key,
            &addressed_ready[0],
            origin,
            trigger_comment_id,
            timeout,
        ),
        PlannedMode::Synthesis => {
            let candidates: Vec<Agent> = roster
                .iter()
                .filter(|a| addressed_ready.iter().any(|n| same(n, &a.agent.nickname)))
                .filter(|a| agent_groups::is_ready(a))
                .map(|a| a.agent.clone())
                .collect();
            dispatch_synthesis(
                app,
                roots,
                project_key,
                &nicknames,
                &candidates,
                origin,
                trigger_comment_id,
                timeout,
            )
        }
    }
}

/// AGC-FR-MRAR, AGC-FR-LJVM: one dispatch per distinct nickname, as AGC-FR-04
/// states. A nickname that the dispatch refuses is logged by the dispatch and
/// listed in `refusals`; the call refuses as a whole only where it registered no
/// turn.
fn dispatch_regular<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    project_key: &str,
    nicknames: &[String],
    origin: ConversationOrigin,
    trigger_comment_id: &str,
) -> Result<AgentGroup, String> {
    let mut registered = Vec::new();
    let mut refusals = Vec::new();
    for nickname in nicknames {
        match dispatch_impl(app, roots, project_key, nickname, origin.clone(), trigger_comment_id) {
            Ok(turn) => registered.push(turn),
            Err(failure) => refusals.push(GroupRefusal { nickname: nickname.clone(), failure }),
        }
    }
    if let (true, Some(first)) = (registered.is_empty(), refusals.first()) {
        return Err(first.failure.clone());
    }
    Ok(AgentGroup {
        mode: "regular".into(),
        group_id: None,
        driver: None,
        turns: registered,
        refusals,
    })
}

/// AGC-FR-MWYP: the setup of a Meta turn. It starts no advisor. It has the
/// advisors tool (AGC-FR-HVQD) and the Meta prompt (CVL-FR-RBQV).
pub(super) fn meta_setup(timeout: Duration) -> GroupSetup {
    GroupSetup {
        state: Arc::new(GroupState::new(new_group_id(), GroupMode::Advisor)),
        role: GroupRole::Meta,
        meta_turn_id: None,
        extra_sections: Vec::new(),
        instructions_suffix: Some(META_PROMPT.to_string()),
        advisors: Some(AdvisorsSetup { timeout }),
    }
}

/// AGC-FR-DLXE: the setup of the Meta turn that a speech registers for the one
/// agent `nickname`, or `None` where the turn is a regular one.
pub(super) fn speech_meta_setup<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    project_key: &str,
    nickname: &str,
) -> Option<GroupSetup> {
    use tauri::Manager;
    let store = app.state::<GlobalSettingsStore>();
    let settings = store.load_agent_group_settings(project_key).unwrap_or_default();
    if !settings.enabled {
        return None;
    }
    let roster =
        agents::list_project_agents_impl(&store, project_key, roots.worktree).unwrap_or_default();
    let ready = ready_nicknames(&roster);
    let addressed_ready = usize::from(ready.iter().any(|r| same(r, nickname)));
    let other_ready = ready.iter().filter(|r| !same(r, nickname)).count();
    match plan_mode(true, 1, addressed_ready, other_ready) {
        PlannedMode::Advisor => {
            Some(meta_setup(Duration::from_secs(u64::from(settings.join_timeout_secs))))
        }
        _ => None,
    }
}

/// AGC-FR-MWYP: the addressed agent becomes Meta. No advisor starts here: Meta
/// asks for advice with the advisors tool (AGC-FR-HVQD).
fn dispatch_advisor<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    project_key: &str,
    meta: &str,
    origin: ConversationOrigin,
    trigger_comment_id: &str,
    timeout: Duration,
) -> Result<AgentGroup, String> {
    use tauri::Manager;
    let turns = app.state::<TurnRegistry>();
    let setup = meta_setup(timeout);
    let state = Arc::clone(&setup.state);
    let meta_turn = dispatch_grouped(
        app,
        roots,
        project_key,
        meta,
        origin,
        Trigger::Comment(trigger_comment_id),
        Some(setup),
    )?;
    // AGC-FR-ISKZ: a turn that took the comment as a steering message is a
    // running turn of a conversation that is already under way. It has no group.
    if meta_turn.group.is_none() {
        return Ok(AgentGroup {
            mode: "regular".into(),
            group_id: None,
            driver: None,
            turns: vec![meta_turn],
            refusals: Vec::new(),
        });
    }
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai],
        "meta turn dispatched",
        log_fields! { "groupId" => &state.id },
    );
    Ok(AgentGroup {
        mode: "advisor".into(),
        group_id: Some(state.id.clone()),
        driver: Some(meta.to_string()),
        turns: vec![meta_turn],
        refusals: Vec::new(),
    })
}

/// AGC-FR-WPCZ, AGC-FR-CZCU: the classifier picks the driver, the addressed ready
/// agents answer in parallel, and the driver writes the Synthesis.
#[allow(clippy::too_many_arguments)]
fn dispatch_synthesis<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    project_key: &str,
    nicknames: &[String],
    candidates: &[Agent],
    origin: ConversationOrigin,
    trigger_comment_id: &str,
    timeout: Duration,
) -> Result<AgentGroup, String> {
    use tauri::Manager;
    let turns = app.state::<TurnRegistry>();
    let driver =
        match classify_driver(app, roots, project_key, &origin, trigger_comment_id, candidates) {
            Ok(driver) => driver,
            Err(code) => {
                // AGC-FR-RMUQ: no driver, so the parallel flow of today.
                logging::log_warn(
                    app,
                    turns.buffer(),
                    &[Domain::Ai],
                    "driver classifier did not pick a driver",
                    log_fields! { "code" => code, "candidates" => candidates.len() },
                );
                return dispatch_regular(
                    app,
                    roots,
                    project_key,
                    nicknames,
                    origin,
                    trigger_comment_id,
                );
            }
        };
    let state = Arc::new(GroupState::new(new_group_id(), GroupMode::Synthesis));
    let mut registered = Vec::new();
    for candidate in candidates {
        match dispatch_grouped(
            app,
            roots,
            project_key,
            &candidate.nickname,
            origin.clone(),
            Trigger::Comment(trigger_comment_id),
            Some(GroupSetup {
                state: Arc::clone(&state),
                role: GroupRole::Participant,
                meta_turn_id: None,
                extra_sections: Vec::new(),
                instructions_suffix: None,
                advisors: None,
            }),
        ) {
            Ok(turn) => registered.push(turn),
            Err(refusal) => logging::log_warn(
                app,
                turns.buffer(),
                &[Domain::Ai],
                "a group member could not be dispatched",
                log_fields! { "refusal" => refusal, "groupId" => &state.id },
            ),
        }
    }
    // AGC-FR-KOHJ: an addressed agent that is not ready takes no part in the group.
    // It is dispatched on its own, so it gets the refusal that a regular dispatch
    // gives it.
    let group_members = registered.len();
    let mut refusals = Vec::new();
    for nickname in nicknames.iter().filter(|n| !candidates.iter().any(|c| same(&c.nickname, n))) {
        match dispatch_impl(app, roots, project_key, nickname, origin.clone(), trigger_comment_id) {
            Ok(turn) => registered.push(turn),
            Err(failure) => refusals.push(GroupRefusal { nickname: nickname.clone(), failure }),
        }
    }
    if registered.is_empty() {
        let first = refusals.first().map(|r| r.failure.clone());
        return Err(first.unwrap_or_else(|| agents::ERR_AGENT_UNAVAILABLE.to_string()));
    }
    if group_members == 0 {
        return Ok(AgentGroup {
            mode: "regular".into(),
            group_id: None,
            driver: None,
            turns: registered,
            refusals,
        });
    }
    state.seal(timeout, std::time::Instant::now());
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai],
        "synthesis group dispatched",
        log_fields! { "groupId" => &state.id, "members" => state.member_count() },
    );
    let monitor = {
        let app = app.clone();
        let state = Arc::clone(&state);
        let owned = roots.owned();
        let project_key = project_key.to_string();
        let origin = origin.clone();
        let trigger = trigger_comment_id.to_string();
        let driver = driver.clone();
        std::thread::spawn(move || {
            run_synthesis_monitor(&app, owned, &project_key, state, &driver, origin, &trigger)
        })
    };
    turns.keep_turn_thread(monitor);
    Ok(AgentGroup {
        mode: "synthesis".into(),
        group_id: Some(state.id.clone()),
        driver: Some(driver),
        turns: registered,
        refusals,
    })
}

/// AGC-FR-ANSK: cancel the members the join ended while they still ran, and mark
/// each skipped first so its terminal event carries the reason.
pub(super) fn skip_pending<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    pending: &[agent_groups::PendingMember],
    reason: SkipReason,
) {
    use tauri::Manager;
    let turns = app.state::<TurnRegistry>();
    let progress_registry = app.state::<ProgressRegistry>();
    for member in pending {
        turns.mark_skipped(&member.turn_id, reason);
        if let Err(error) = cancel_impl(app, &turns, &progress_registry, &member.turn_id) {
            logging::log_warn(
                app,
                turns.buffer(),
                &[Domain::Ai],
                "a skipped group member could not be cancelled",
                log_fields! { "turnId" => &member.turn_id, "error" => error },
            );
        }
    }
}

/// AGC-FR-CZCU: wait for the join of a synthesis group, skip what did not
/// finish, and start the synthesis turn of the driver.
fn run_synthesis_monitor<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: OwnedRoots,
    project_key: &str,
    state: Arc<GroupState>,
    driver: &str,
    origin: ConversationOrigin,
    trigger_comment_id: &str,
) {
    use tauri::Manager;
    let turns = app.state::<TurnRegistry>();
    let never = AtomicBool::new(false);
    let timed_out = match state.wait_join(&never) {
        JoinOutcome::Over { pending, timed_out } => {
            skip_pending(app, &pending, SkipReason::TimedOut);
            timed_out
        }
        JoinOutcome::Cancelled => return,
    };
    let finished = state.finished_count();
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai],
        "synthesis group joined",
        log_fields! {
            "groupId" => &state.id,
            "finished" => finished,
            "members" => state.member_count(),
            "timedOut" => timed_out,
        },
    );
    // AGC-FR-WZUL: no turn finished, so there is nothing to combine.
    if finished == 0 {
        return;
    }
    let setup = GroupSetup {
        state: Arc::clone(&state),
        role: GroupRole::Synthesis,
        meta_turn_id: None,
        extra_sections: synthesis_sections(&state, &origin),
        instructions_suffix: Some(SYNTHESIS_PROMPT.to_string()),
        advisors: None,
    };
    if let Err(refusal) = dispatch_grouped(
        app,
        roots.borrowed(),
        project_key,
        driver,
        origin,
        Trigger::Comment(trigger_comment_id),
        Some(setup),
    ) {
        logging::log_warn(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "the synthesis turn could not be dispatched",
            log_fields! { "refusal" => refusal, "groupId" => &state.id },
        );
    }
}

/// AGC-FR-DIZM / AGC-FR-WPAP: the proposal tool a synthesis turn on `target`
/// has, which its `group_proposals` section names.
pub(super) fn proposal_tool_of(target: &DiscussionTarget) -> &'static str {
    match target {
        DiscussionTarget::Artifact { .. } => crate::tools::propose_prompt_changes::NAME,
        DiscussionTarget::Change { .. } => crate::tools::propose_change_edits::NAME,
        _ => crate::tools::propose_draft_changes::NAME,
    }
}

/// AGC-FR-CZCU, AGC-FR-DIZM: the sections of a synthesis turn.
fn synthesis_sections(state: &GroupState, origin: &ConversationOrigin) -> Vec<InputSection> {
    let replies = state.replies();
    let mut body = String::new();
    for (nickname, status) in state.statuses() {
        match status {
            MemberStatus::Finished => {
                let reply = replies.iter().find(|r| r.nickname == nickname);
                match reply {
                    Some(reply) => {
                        body.push_str(&format!("@{nickname} replied:\n{}\n\n", reply.text))
                    }
                    None => body.push_str(&format!("@{nickname} replied without a comment\n\n")),
                }
            }
            MemberStatus::Skipped(reason) => {
                body.push_str(&format!("@{nickname}: skipped ({})\n\n", reason.label()))
            }
            MemberStatus::Pending => {}
        }
    }
    let mut sections = vec![section("synthesis", body.trim_end().to_string())];
    let held_questions = state.held_questions();
    if !held_questions.is_empty() {
        let body = held_questions
            .iter()
            .map(|h| format!("@{}\n{}", h.nickname, h.text))
            .collect::<Vec<_>>()
            .join("\n\n");
        sections.push(section("group_questions", body));
    }
    let held_proposals = state.held_proposals();
    if !held_proposals.is_empty() {
        let offers = held_proposals
            .iter()
            .map(|h| format!("@{}\n{}", h.nickname, h.text))
            .collect::<Vec<_>>()
            .join("\n\n");
        // CVL-FR-08: a draft turn has `propose_draft_changes` and an artifact
        // turn has `propose_prompt_changes`. The turn is told the one it has.
        // AGC-FR-WPAP: a change turn has `propose_change_edits`.
        let tool = proposal_tool_of(&origin.target);
        let body = format!("Propose the merged change with `{tool}`.\n\n{offers}");
        sections.push(section("group_proposals", body));
    }
    sections
}

/// AGC-FR-VDOO: the attachments of the reply of a turn. A Meta turn adds the
/// `advisors` attachment of its calls of the advisors tool.
pub(super) fn reply_attachments(plan: &TurnPlan) -> Vec<comments::Attachment> {
    let mut attachments = plan.attachments.clone();
    let advisors = plan
        .group
        .as_ref()
        .and_then(|binding| binding.advisor_calls.as_ref())
        .and_then(|calls| calls.attachment());
    attachments.extend(advisors);
    attachments
}

/// AGC-FR-VDOO: the marks of a Meta turn end with the turn.
pub(super) fn clear_marks(plan: &TurnPlan) {
    if role_of(plan) == Some(GroupRole::Meta) {
        comments::clear_turn_marks(plan.origin.discussion_id(), &plan.author);
    }
}

/// AGC-FR-WZUL: a reply of a synthesis turn that holds a call which ends the turn
/// holds the text of the Synthesis too. Without that text the call is not
/// carried out, and this is the result the model reads in its place.
pub(super) const SYNTHESIS_NEEDS_TEXT: &str = "Nothing was recorded. Write the Synthesis as the text of the same reply that holds this call, then make the call again.";

/// AGC-FR-WZUL: whether the call of a synthesis turn waits for the text of the
/// Synthesis.
pub(super) fn needs_synthesis_text(plan: &TurnPlan, reply_text: &str) -> bool {
    role_of(plan) == Some(GroupRole::Synthesis) && reply_text.trim().is_empty()
}

/// AGC-FR-WZUL: a synthesis turn that ended on a question, a question set, or a
/// proposal still appends its Synthesis comment, with the text of the reply that
/// held the call. A failure is logged, and the turn keeps the end it reached.
pub(super) fn post_synthesis_after_contribution<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    turns: &TurnRegistry,
    plan: &TurnPlan,
    reply_text: &str,
) {
    if role_of(plan) != Some(GroupRole::Synthesis) {
        return;
    }
    // AGC-FR-YBJC: the turn ended on a question, a question set, or a proposal,
    // so the Synthesis comment carries no tool list.
    let content = comments::AnswerContent {
        body: reply_text.trim().to_string(),
        attachments: plan.attachments.clone(),
        tool_calls: Vec::new(),
    };
    let posted = deliver_with(plan.roots.borrowed(), &plan.origin, &plan.author, content);
    match posted {
        Ok(thread) => comments::emit_discussion_changed(app, &thread),
        Err(error) => logging::log_warn(
            app,
            turns.buffer(),
            &[Domain::Ai],
            "the synthesis comment could not be appended",
            log_fields! { "turnId" => &plan.turn_id, "reason" => error },
        ),
    }
}

pub(super) fn advisor_entry(
    _state: &GroupState,
    entries: &[agent_groups::AdviceEntry],
    handle: String,
    status: MemberStatus,
) -> comments::AdvisorEntry {
    match status {
        MemberStatus::Finished => {
            let advice = entries.iter().find(|e| e.nickname == handle).map(|e| e.advice.clone());
            let advice = advice.unwrap_or_default();
            comments::AdvisorEntry {
                handle,
                state: "finished".into(),
                reason: None,
                points: advice.points,
                questions: advice.questions,
            }
        }
        MemberStatus::Skipped(reason) => comments::AdvisorEntry {
            handle,
            state: "skipped".into(),
            reason: Some(reason.label().into()),
            points: Vec::new(),
            questions: Vec::new(),
        },
        // The join ended, so no member is pending; read as skipped for safety.
        MemberStatus::Pending => comments::AdvisorEntry {
            handle,
            state: "skipped".into(),
            reason: Some(SkipReason::TimedOut.label().into()),
            points: Vec::new(),
            questions: Vec::new(),
        },
    }
}

/// AGC-FR-GNMP: an advisor's reply is recorded as advice in its group.
pub(super) fn record_advice(plan: &TurnPlan, answer: &str) {
    if let Some(binding) = plan.group.as_ref() {
        // The `<spoken>` tag every reply carries is no part of the advice.
        let advice = parse_advice(&crate::comments::without_spoken_tags(answer));
        binding.state.record_advice(&plan.agent.nickname, advice);
    }
}

/// AGC-FR-CZCU: an addressed agent's delivered reply is kept in its group.
pub(super) fn record_reply(plan: &TurnPlan, answer: &str) {
    if let Some(binding) = plan.group.as_ref() {
        let text = crate::comments::without_spoken_tags(answer).trim().to_string();
        binding.state.record_reply(&plan.agent.nickname, text);
    }
}

/// AGC-FR-GNMP: the role of the plan's group, if any.
pub(super) fn role_of(plan: &TurnPlan) -> Option<GroupRole> {
    plan.group.as_ref().map(|g| g.role)
}

/// AGC-FR-DIZM: a tool that records a question set or a proposal, as an addressed
/// agent of a synthesis group sees it. The call is held by the group instead of
/// recorded, and it ends the turn on the flag that a recorded call sets
/// (CVL-FR-15). Every other tool stays as it is.
pub(super) fn capture_contributions(
    tools: Vec<rig::tool::DynamicTool>,
    state: &Arc<GroupState>,
    nickname: &str,
    asked_questions: &Arc<AtomicBool>,
    proposed: &Arc<AtomicBool>,
) -> Vec<rig::tool::DynamicTool> {
    use rig::tool::IntoToolOutput;
    tools
        .into_iter()
        .map(|tool| {
            let definition = tool.definition();
            let name = definition.name.to_string();
            let (flag, questions) = if name == crate::tools::ask_discussion_questions::NAME {
                (Arc::clone(asked_questions), true)
            } else if name == crate::tools::propose_draft_changes::NAME
                || name == crate::tools::propose_prompt_changes::NAME
                || name == crate::tools::propose_change_edits::NAME
                || name == crate::tools::propose_new_change::NAME
            {
                (Arc::clone(proposed), false)
            } else {
                return tool;
            };
            let state = Arc::clone(state);
            let nickname = nickname.to_string();
            rig::tool::DynamicTool::new(
                definition.name,
                definition.description,
                definition.parameters,
                move |arguments: serde_json::Value| {
                    let offered = arguments.to_string();
                    if questions {
                        state.hold_questions(&nickname, offered);
                    } else {
                        state.hold_proposal(&nickname, offered);
                    }
                    flag.store(true, Ordering::SeqCst);
                    Box::pin(
                        async move { serde_json::json!({ "recorded": true }).into_tool_output() },
                    )
                },
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------
// The classifier
// ---------------------------------------------------------------------------

/// AGC-FR-WPCZ: the request of the classifier.
pub(super) fn classifier_request(
    material: Vec<InputSection>,
    candidates: &[Agent],
) -> AgentRequest {
    let body = candidates
        .iter()
        .map(|agent| {
            let instructions: String =
                agent.instructions.chars().take(CLASSIFIER_INSTRUCTIONS_CHARS).collect();
            format!(
                "nickname: {}\ntitle: {}\ninstructions:\n{}",
                agent.nickname, agent.title, instructions
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n");
    let mut input = vec![section("candidates", body)];
    input.extend(material);
    AgentRequest {
        instructions: crate::prompts::instruction_text(CLASSIFIER_PROMPT),
        input,
        ..Default::default()
    }
}

/// AGC-FR-WPCZ, AGC-FR-RMUQ: one call without tools; the nickname it names, or
/// the typed reason it names none.
pub(super) fn classify_with(
    seam: &dyn CompletionSeam,
    request: &AgentRequest,
    endpoint: &AiApiCall,
    candidates: &[Agent],
) -> Result<String, &'static str> {
    let nicknames: Vec<String> = candidates.iter().map(|a| a.nickname.clone()).collect();
    match seam.complete(request, &[], endpoint, CLASSIFIER_LIMIT) {
        Ok(reply) if !reply.refused && !reply.text.trim().is_empty() => {
            parse_driver(&reply.text, &nicknames).ok_or("no_candidate_named")
        }
        Ok(_) => Err("empty_reply"),
        Err(failure) => Err(failure.class),
    }
}

/// AGC-FR-WPCZ: the driver of a synthesis group. The call uses the model and the
/// reasoning stored for the AI API (AAP-FR-EDBQ).
fn classify_driver<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    project_key: &str,
    origin: &ConversationOrigin,
    trigger_comment_id: &str,
    candidates: &[Agent],
) -> Result<String, &'static str> {
    use tauri::Manager;
    let store = app.state::<GlobalSettingsStore>();
    let ai = app.state::<AiApiIntegrations>();
    let turns = app.state::<TurnRegistry>();
    let endpoint = ai_api::resolve_ai_api_call(&store, &ai, project_key)
        .map_err(|_| "provider_unconfigured")?;
    let budget = DocumentBudget::resolve(
        &store,
        &endpoint.provider,
        endpoint.model_id.as_deref().unwrap_or_default(),
    );
    let material = build_input_within(roots, origin, trigger_comment_id, &budget).sections;
    let request = classifier_request(material, candidates);
    let started = std::time::Instant::now();
    let outcome = classify_with(turns.seam.as_ref(), &request, &endpoint, candidates);
    // Counts, the provider, and the duration: never the document or a persona.
    logging::log_info(
        app,
        turns.buffer(),
        &[Domain::Ai, Domain::Remote],
        "driver classifier called",
        log_fields! {
            "provider" => &endpoint.provider,
            "model" => endpoint.model_id.as_deref().unwrap_or("<provider default>"),
            "candidates" => candidates.len(),
            "picked" => outcome.is_ok(),
            "durationMs" => started.elapsed().as_millis() as u64,
        },
    );
    outcome
}

/// `"dispatch agent group"` (AGC-FR-KOHJ).
#[tauri::command]
pub async fn dispatch_agent_group<R: tauri::Runtime>(
    nicknames: Vec<String>,
    origin: ConversationOrigin,
    trigger_comment_id: String,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
) -> Result<AgentGroup, String> {
    let key = project.anchor().ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    let store = project.require_store()?;
    // The classifier is a model call, so the command runs off the main thread
    // and returns once the turns are registered.
    tauri::async_runtime::spawn_blocking(move || {
        let roots = Roots { worktree: &root, store: &store };
        dispatch_group_impl(&app, roots, &key, &nicknames, origin, &trigger_comment_id)
    })
    .await
    .map_err(|_| FAIL_UNREACHABLE.to_string())?
}

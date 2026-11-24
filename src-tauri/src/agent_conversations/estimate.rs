//! The context figures of a discussion before any turn (AGC-FR-JFBZ,
//! AGC-FR-OBUA, AGC-FR-QCVC).
//!
//! What the indicator of the discussion column shows until a turn reports its
//! own figures. The input of a turn is assembled dry: no model call, no key, no
//! turn, no event, and no record.

use super::*;
use crate::project_scope::ProjectStores;

/// AGC-FR-OBUA: the agent `agent_id` names in the open project's roster, where
/// it is one the roster does not shadow.
fn agent_of(
    store: &GlobalSettingsStore,
    project_key: &str,
    worktree: &crate::fs::RootFs,
    agent_id: &str,
) -> Option<Agent> {
    agents::list_project_agents_impl(store, project_key, worktree)
        .ok()?
        .into_iter()
        .filter(|p| p.availability != agents::AgentAvailability::Shadowed)
        .find(|p| p.agent.id == agent_id)
        .map(|p| p.agent)
}

/// AGC-FR-JFBZ: the figures a turn for `target` and `discussion_id` would
/// carry, with an empty `current_comment` and the window of `agent_id`'s model.
pub fn estimate_impl<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    roots: Roots<'_>,
    project_key: &str,
    target: DiscussionTarget,
    discussion_id: Option<&str>,
    agent_id: Option<&str>,
) -> DiscussionContext {
    use tauri::Manager;
    let store = app.state::<GlobalSettingsStore>();
    // AGC-FR-OBUA: an absent or unknown discussion gives an empty history.
    let discussion = discussion_id
        .and_then(|id| comments::locate_discussion_of(roots.store, &target, id))
        .filter(|d| d.target == target);
    let origin = match &discussion {
        Some(d) => ConversationOrigin::of(d),
        None => ConversationOrigin {
            discussion_id: String::new(),
            target: target.clone(),
            fragment_target: None,
        },
    };
    // AGC-FR-OBUA: no agent, or no provider, gives the default window.
    let agent = agent_id.and_then(|id| agent_of(&store, project_key, roots.worktree, id));
    let provider = ai_api::resolve_agent_provider(&store, project_key).ok();
    // With no agent or no provider there is no model, so no override of a
    // provider applies: the window is the default, with the stored percent.
    let budget = match (provider.as_deref(), agent.as_ref()) {
        (Some(provider), Some(agent)) => DocumentBudget::resolve(&store, provider, &agent.model_id),
        _ => DocumentBudget {
            percent: store
                .load_document_limit_percent()
                .unwrap_or(crate::global_settings::DOCUMENT_LIMIT_PERCENT_DEFAULT),
            history_percent: store
                .load_history_limit_percent()
                .unwrap_or(crate::global_settings::HISTORY_LIMIT_PERCENT_DEFAULT),
            ..DocumentBudget::default_window()
        },
    };

    let mut sections = Vec::new();
    let document = match &origin.target {
        DiscussionTarget::Note { note_id } => {
            sections.extend(note_section(roots, note_id));
            None
        }
        // AGC-FR-UOPJ: the dry run holds no trigger, so every comment counts.
        DiscussionTarget::Map => {
            if let Some(discussion) = &discussion {
                sections.push(map_context::map_context_section(discussion));
                map_context::extend_map_material(app, roots, &budget, &mut sections);
                // AGC-FR-UOPJ: the estimate carries the plan references as a turn
                // does, and records no fingerprint.
                plan_references::extend_plan_references(
                    app,
                    roots,
                    &budget,
                    &discussion.id,
                    &mut sections,
                );
            }
            None
        }
        target => {
            let material = material_of(roots, target, origin.fragment_target.as_ref(), &budget);
            sections.extend(material.section);
            material.document
        }
    };
    if let Some(discussion) = &discussion {
        // The trigger is no comment of the discussion, so every comment stands
        // in the history.
        // AGC-FR-HOND: the estimate bounds the history as a turn bounds it.
        let parts =
            discussion_history_with_status(roots, discussion, "", budget.history_bound_chars());
        sections.push(parts.history);
        // AGC-FR-06: the state follows where the current comment would stand.
        sections.extend(parts.state);
        if matches!(discussion.target, DiscussionTarget::Map) {
            let focus = map_context::map_state_text(roots, discussion, "");
            prepend_state_text(&mut sections, &format!("{focus}\n"));
        } else if let Some(rows) = map_context::proposal_state_text(roots, discussion) {
            // MPR-FR-VYPE: the estimate counts the rows as a turn carries them.
            append_state_text(&mut sections, &rows);
        }
    }

    let (instructions, title, handle, agent_key, model) = match &agent {
        Some(a) => (
            a.instructions.as_str(),
            a.title.as_str(),
            a.nickname.clone(),
            a.id.clone(),
            Some(a.model_id.clone()),
        ),
        None => ("", "", String::new(), String::new(), None),
    };
    // CVL-FR-FQMM: the same language rules as the turn.
    let language = document_language::document_language(
        roots,
        &origin.target,
        origin.fragment_target.as_ref(),
    );
    document_language::log_document_language(
        app,
        app.project_store::<TurnRegistry>().buffer(),
        "estimate",
        &origin.target,
        language,
    );
    let prompt = compile_prompt_spoken(
        origin.kind(),
        instructions,
        title,
        &language_rules(&store, language.name),
    );
    let author =
        Participant::Agent { agent_id: agent_key, handle, model, title: Some(title.to_string()) };
    // CVL-FR-AAPA: the definitions the turn would be offered. Constructing a
    // tool reads and writes nothing.
    let unset = Arc::new(AtomicBool::new(false));
    let tools = conversation_tools(
        app,
        "context-estimate",
        roots,
        &origin,
        &author,
        ContributionFlags {
            asked: &unset,
            asked_questions: &unset,
            proposed: &unset,
            plan_card: &Arc::default(),
        },
        budget,
        // CVL-FR-VZZR: the filter changes no definition, so the estimate holds
        // for every selection.
        &crate::skills::SkillFilter::for_agent(
            agent.as_ref().map(|a| a.skills.clone()).unwrap_or_default(),
            agent.as_ref().map(|a| a.disabled_skills.clone()).unwrap_or_default(),
        ),
    );
    let definitions: Vec<_> = tools.iter().map(|tool| tool.definition()).collect();
    // ERP-FR-GDLV: the replay a turn of this agent would carry, selected on
    // the same terms and changing no entry.
    let instructions = instruction_tokens(&prompt, &definitions);
    let replay = match (&discussion, &agent) {
        (Some(discussion), Some(agent)) => {
            let memory = app.try_project_store::<ReplayMemory>();
            let inputs = ReplayInputs {
                roots,
                thread: discussion,
                trigger_comment_id: "",
                key: ReplayKey { discussion_id: discussion.id.clone(), agent_id: agent.id.clone() },
                offered: definitions.iter().map(|d| d.name.to_string()).collect(),
                provider: provider.as_deref().unwrap_or_default(),
                model_id: &agent.model_id,
                window: budget.window,
                history_bound: budget.history_bound_chars(),
                instruction_tokens: instructions,
                update_memory: false,
            };
            memory.map(|m| plan_replay(&m, &inputs, &mut sections)).unwrap_or_default()
        }
        _ => ReplayPlan::default(),
    };
    // AGC-FR-OBUA: tool results are 0 before any turn, and the current comment
    // is empty (AGC-FR-JFBZ).
    let mut split = split_of_sections(instructions, &sections);
    let replayed = AgentRequest {
        replay: replay.blocks,
        stable_head_sections: replay.head,
        ..AgentRequest::default()
    };
    add_replay(&mut split, &replayed);
    let limit_tokens = budget.limit_tokens();
    DiscussionContext {
        window: budget.window,
        estimated_tokens: split.total(),
        split,
        document,
        limit_tokens,
        above_limit: above_limit(document.as_ref(), limit_tokens),
    }
}

/// AGC contract: `"estimate discussion context"`.
#[project_scope_macros::project_scoped]
#[project_scope_macros::blocking_command]
pub fn estimate_discussion_context<R: tauri::Runtime>(
    target: DiscussionTarget,
    discussion_id: Option<String>,
    agent_id: Option<String>,
    app: tauri::AppHandle<R>,
    project: crate::project_scope::Scoped<'_, ProjectState>,
) -> Result<DiscussionContext, String> {
    // AGC-FR-QCVC: the one refusal.
    let key = project.anchor().ok_or_else(|| ERR_NO_PROJECT_OPEN.to_string())?;
    let root = project.require_root().map_err(|_| ERR_NO_PROJECT_OPEN.to_string())?;
    let store = project.require_store()?;
    let context = estimate_impl(
        &app,
        Roots { worktree: &root, store: &store },
        &key,
        target,
        discussion_id.as_deref(),
        agent_id.as_deref(),
    );
    // Numbers only: no path, no text, and no comment of the discussion.
    logging::log_debug(
        &app,
        &logging::BUFFER,
        &[Domain::Ai],
        "discussion context estimated",
        log_fields! {
            "window" => context.window,
            "estimatedTokens" => context.estimated_tokens,
            "limitTokens" => context.limit_tokens,
            "aboveLimit" => context.above_limit,
        },
    );
    Ok(context)
}

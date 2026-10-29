//! The context figures of a discussion before any turn (AGC-FR-JFBZ,
//! AGC-FR-OBUA, AGC-FR-QCVC).
//!
//! What the indicator of the discussion column shows until a turn reports its
//! own figures. The input of a turn is assembled dry: no model call, no key, no
//! turn, no event, and no record.

use super::*;

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
    // AGC-FR-OBUA: no agent, or no provider, gives a null window.
    let agent = agent_id.and_then(|id| agent_of(&store, project_key, roots.worktree, id));
    let provider = ai_api::resolve_agent_provider(&store, project_key).ok();
    // An empty provider or model resolves no window.
    let budget = DocumentBudget::resolve(
        &store,
        provider.as_deref().unwrap_or(""),
        agent.as_ref().map(|a| a.model_id.as_str()).unwrap_or(""),
    );

    let mut sections = Vec::new();
    let document = match &origin.target {
        DiscussionTarget::Note { note_id } => {
            sections.extend(note_section(roots, note_id));
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
        sections.push(discussion_history_section(roots, discussion, ""));
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
    let prompt = compile_prompt(origin.kind(), instructions, title);
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
        ContributionFlags { asked: &unset, asked_questions: &unset, proposed: &unset },
        budget,
    );
    let definitions: Vec<_> = tools.iter().map(|tool| tool.definition()).collect();
    // AGC-FR-OBUA: tool results are 0 before any turn, and the current comment
    // is empty (AGC-FR-JFBZ).
    let split = split_of_sections(instruction_tokens(&prompt, &definitions), &sections);
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
#[tauri::command]
pub fn estimate_discussion_context<R: tauri::Runtime>(
    target: DiscussionTarget,
    discussion_id: Option<String>,
    agent_id: Option<String>,
    app: tauri::AppHandle<R>,
    project: State<'_, ProjectState>,
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
            "window" => context.window.map(|w| w.to_string()).unwrap_or_default(),
            "estimatedTokens" => context.estimated_tokens,
            "limitTokens" => context.limit_tokens.map(|l| l.to_string()).unwrap_or_default(),
            "aboveLimit" => context.above_limit,
        },
    );
    Ok(context)
}

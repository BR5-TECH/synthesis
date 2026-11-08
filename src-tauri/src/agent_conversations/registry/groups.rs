//! The parts of the turn registry that belong to agent groups: the binding of a
//! turn to its group, the skip marks, the tool calls of an advisor that its Meta
//! turn mirrors, and the report of a member's end to its group (AGC-FR-KOHJ).

use super::*;

impl TurnRegistry {
    /// AGC-FR-AIQM: bind a registered turn to its group and role. Answers the
    /// turn as it now stands, which carries the `group` field.
    pub(in crate::agent_conversations) fn attach_group(
        &self,
        turn_id: &str,
        state: Arc<crate::agent_groups::GroupState>,
        role: GroupRole,
        mirror: Option<(String, String)>,
    ) -> Option<AgentTurn> {
        let mut inner = self.lock();
        let live = inner.live.get_mut(turn_id)?;
        live.turn.group = Some(TurnGroup { group_id: state.id.clone(), role, skipped: None });
        live.group = Some(state);
        live.mirror = mirror;
        Some(live.turn.clone())
    }

    /// AGC-FR-ANSK: mark a live turn skipped before it is cancelled, so the
    /// terminal event carries the reason.
    pub(in crate::agent_conversations) fn mark_skipped(&self, turn_id: &str, reason: SkipReason) {
        let mut inner = self.lock();
        if let Some(group) = inner.live.get_mut(turn_id).and_then(|live| live.turn.group.as_mut()) {
            group.skipped = Some(reason);
        }
    }

    /// AGC-FR-BIPE: end every call of an advisor turn that its Meta turn still
    /// shows as running, as failed, and stop the mirroring. Called before the
    /// advisor terminates from any path, because a call of a terminated turn
    /// reports nothing more and its mirror would stay active on the Meta turn.
    pub(in crate::agent_conversations) fn end_mirrored_calls<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        turn_id: &str,
    ) {
        let mut inner = self.lock();
        let Some(live) = inner.live.get_mut(turn_id) else { return };
        let mirrored: Vec<String> = live.mirrored.drain().map(|(_, meta_call)| meta_call).collect();
        let Some((meta_id, _)) = live.mirror.take() else { return };
        if mirrored.is_empty() {
            return;
        }
        if let Some(meta) = inner.live.get_mut(&meta_id) {
            for meta_call in &mirrored {
                finish_mirror(meta, meta_call, ToolCallState::Failed);
            }
            publish(app, &meta.turn.clone());
        }
    }
}

/// AGC-FR-BIPE: record a call of an advisor turn on its Meta turn, under the
/// advisor's name, and remember the id it has there. Called under the registry
/// lock, with the advisor's own call already recorded.
pub(super) fn mirror_call_begin<R: tauri::Runtime>(
    inner: &mut Inner,
    app: &tauri::AppHandle<R>,
    turn_id: &str,
    own_call: &str,
    mirror: (String, String),
    tool: &str,
    key_input: Option<String>,
) {
    let (meta_id, advisor) = mirror;
    let Some(meta) = inner.live.get_mut(&meta_id) else { return };
    let meta_seq = meta.next_seq;
    meta.next_seq += 1;
    let meta_call = format!("call-{meta_seq}");
    meta.turn.active_tool_calls.push(ActiveToolCall {
        id: meta_call.clone(),
        tool: tool.to_string(),
        activation_seq: meta_seq,
        advisor: Some(advisor.clone()),
    });
    meta.turn.tool_calls.push(TurnToolCall {
        id: meta_call.clone(),
        tool: tool.to_string(),
        activation_seq: meta_seq,
        state: ToolCallState::Running,
        key_input,
        advisor: Some(advisor),
    });
    publish(app, &meta.turn.clone());
    if let Some(own) = inner.live.get_mut(turn_id) {
        own.mirrored.insert(own_call.to_string(), meta_call);
    }
}

/// AGC-FR-BIPE: the call of an advisor turn ended, so its mirror on the Meta
/// turn ends the same way. Called under the registry lock.
pub(super) fn mirror_call_end<R: tauri::Runtime>(
    inner: &mut Inner,
    app: &tauri::AppHandle<R>,
    meta_id: &str,
    meta_call: &str,
    state: ToolCallState,
) {
    if let Some(meta) = inner.live.get_mut(meta_id) {
        finish_mirror(meta, meta_call, state);
        publish(app, &meta.turn.clone());
    }
}

fn finish_mirror(meta: &mut Live, meta_call: &str, state: ToolCallState) {
    meta.turn.active_tool_calls.retain(|call| call.id != meta_call);
    if let Some(call) = meta.turn.tool_calls.iter_mut().find(|c| c.id == meta_call) {
        call.state = state;
    }
}

/// AGC-FR-ZOCI, AGC-FR-ANSK: the group learns how its member ended, in the same
/// lock that stamps the terminal state, so the join never reads a state the turn
/// has not reached. A turn that is no member is ignored there. A member that
/// failed or was cancelled is skipped, and a reason the join already set stands.
pub(super) fn note_member_end(live: &mut Live, turn_id: &str, state: AgentTurnState) {
    let Some(group) = live.group.as_ref() else { return };
    let end = match state {
        AgentTurnState::Delivered | AgentTurnState::AwaitingReply => {
            crate::agent_groups::MemberEnd::Finished
        }
        AgentTurnState::Cancelled => crate::agent_groups::MemberEnd::Cancelled,
        _ => crate::agent_groups::MemberEnd::Failed,
    };
    group.member_ended(turn_id, end, std::time::Instant::now());
    if let Some(turn_group) = live.turn.group.as_mut() {
        let member = matches!(turn_group.role, GroupRole::Participant | GroupRole::Advisor);
        if member && turn_group.skipped.is_none() {
            turn_group.skipped = match end {
                crate::agent_groups::MemberEnd::Failed => Some(SkipReason::Failed),
                crate::agent_groups::MemberEnd::Cancelled => Some(SkipReason::Cancelled),
                crate::agent_groups::MemberEnd::Finished => None,
            };
        }
    }
}

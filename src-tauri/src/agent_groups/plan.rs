//! Which mode a message takes (AGC-FR-NWDA) and who is ready (AGC-FR-YVNC).

use crate::agents::{AgentAvailability, ProjectAgent};

/// What `dispatch_agent_group` does with one message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlannedMode {
    /// One dispatch per addressed nickname, as before (AGC-FR-MRAR, AGC-FR-LJVM).
    Regular,
    /// The addressed agent is Meta and the other ready agents advise.
    Advisor,
    /// The addressed ready agents answer together and a driver combines them.
    Synthesis,
}

/// AGC-FR-YVNC: an agent is ready when it is not shadowed, it has a model, and it
/// can run. The roster's availability already says all three.
pub fn is_ready(agent: &ProjectAgent) -> bool {
    agent.availability == AgentAvailability::Ready && !agent.agent.model_id.trim().is_empty()
}

/// AGC-FR-YVNC: the nicknames of the ready agents, in roster order.
pub fn ready_nicknames(roster: &[ProjectAgent]) -> Vec<String> {
    roster.iter().filter(|a| is_ready(a)).map(|a| a.agent.nickname.clone()).collect()
}

/// AGC-FR-NWDA, AGC-FR-LJVM, AGC-FR-MRAR: the mode of a message.
///
/// `addressed` is the number of distinct nicknames the message addresses,
/// `addressed_ready` how many of them are ready, and `other_ready` how many
/// ready agents the message does not address.
pub fn plan_mode(
    enabled: bool,
    addressed: usize,
    addressed_ready: usize,
    other_ready: usize,
) -> PlannedMode {
    if !enabled {
        return PlannedMode::Regular;
    }
    if addressed_ready >= 2 {
        return PlannedMode::Synthesis;
    }
    if addressed == 1 && addressed_ready == 1 && other_ready >= 1 {
        return PlannedMode::Advisor;
    }
    PlannedMode::Regular
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agents::{Agent, AgentScope};

    fn agent(nick: &str, model: &str, availability: AgentAvailability) -> ProjectAgent {
        ProjectAgent {
            agent: Agent { nickname: nick.into(), model_id: model.into(), ..Default::default() },
            scope: AgentScope::Project,
            availability,
        }
    }

    // AGC-FR-YVNC: shadowed, modelless, and unavailable agents are not ready.
    #[test]
    fn only_a_ready_agent_with_a_model_is_ready() {
        let roster = vec![
            agent("a", "m", AgentAvailability::Ready),
            agent("b", "m", AgentAvailability::Shadowed),
            agent("c", "", AgentAvailability::Ready),
            agent("d", "m", AgentAvailability::ModelUnavailable),
            agent("e", "m", AgentAvailability::ProviderUnverified),
        ];
        assert_eq!(ready_nicknames(&roster), vec!["a".to_string()]);
    }

    // AGC-FR-MRAR: with the switch off every message is regular.
    #[test]
    fn the_switch_off_is_always_regular() {
        assert_eq!(plan_mode(false, 3, 3, 2), PlannedMode::Regular);
        assert_eq!(plan_mode(false, 1, 1, 2), PlannedMode::Regular);
    }

    // AGC-FR-NWDA: one tag with another ready agent is advisor mode.
    #[test]
    fn one_tag_with_another_ready_agent_is_advisor_mode() {
        assert_eq!(plan_mode(true, 1, 1, 1), PlannedMode::Advisor);
        assert_eq!(plan_mode(true, 1, 1, 4), PlannedMode::Advisor);
    }

    // AGC-FR-LJVM: one dispatched agent is a regular turn.
    #[test]
    fn one_dispatched_agent_is_regular() {
        // One tag and no other ready agent, or `@all` with one ready agent.
        assert_eq!(plan_mode(true, 1, 1, 0), PlannedMode::Regular);
        // Several tags, only one of them ready.
        assert_eq!(plan_mode(true, 3, 1, 2), PlannedMode::Regular);
        // Nobody ready.
        assert_eq!(plan_mode(true, 2, 0, 3), PlannedMode::Regular);
        assert_eq!(plan_mode(true, 1, 0, 3), PlannedMode::Regular);
    }

    // AGC-FR-NWDA: two ready addressed agents are synthesis mode.
    #[test]
    fn two_ready_addressed_agents_are_synthesis_mode() {
        assert_eq!(plan_mode(true, 2, 2, 0), PlannedMode::Synthesis);
        assert_eq!(plan_mode(true, 3, 2, 1), PlannedMode::Synthesis);
    }
}

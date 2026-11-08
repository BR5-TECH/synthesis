//! Agent groups: several agents answering one message together
//! (`AGC-agent-conversations.md` AGC-FR-KOHJ).
//!
//! This module holds the parts of a group that know nothing of a turn: the
//! stored settings, the fork-join arithmetic, the state the members of a group
//! share, and the advice an advisor returns. How a turn joins a group is the
//! engine's, in `crate::agent_conversations`.

mod advice;
mod driver;
mod join;
mod plan;
mod settings;
mod state;

pub use advice::{parse_advice, render_advisors_section, Advice, AdviceEntry};
pub use driver::parse_driver;
pub use join::Join;
pub use plan::{is_ready, plan_mode, ready_nicknames, PlannedMode};
pub use settings::*;
pub use state::{GroupMode, GroupState, Held, JoinOutcome, MemberEnd, MemberStatus, PendingMember};

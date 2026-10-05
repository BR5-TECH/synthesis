//! Tests for `LSK-load-skill-tool.md`.
//!
//! This is the one tool in the group that reads a file, so these run against a
//! real tree rather than a stub registry: the claims worth pinning — a body
//! returned byte-for-byte, a header stripped, a deleted file refusing, a
//! symlink refused rather than followed — are exactly the ones a stub would
//! answer by construction.

use rig::tool::{PortableTool, ToolErrorKind};
use tauri::Manager;

use super::*;
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::skills::Ecosystem;
use crate::tools::tests::block_on;
use crate::tools::tests::{closed_project, collision_project, demo_project, mounted, write_skill, Fixture};
use crate::tools::{ToolRefusal, NO_PROJECT_OPEN, SKILL_NOT_FOUND, SKILL_UNREADABLE};

mod body;
mod currency;
mod definition;
mod failures;
mod log_records;
mod resolution;


fn call(fixture: &Fixture, name: &str, ecosystem: Option<&str>) -> Result<String, ToolRefusal> {
    block_on(SkillLoadTool::new(fixture.handle()).call(LoadSkillArgs {
        name: name.to_string(),
        ecosystem: ecosystem.map(str::to_string),
    }))
}

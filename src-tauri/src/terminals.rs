//! Terminal sessions: the author's own interactive terminals into a worktree or
//! a work stream (`TRS-terminal-sessions.md`).
//!
//! A session is a process on a pseudo-terminal. It outlives the tab that shows
//! it, keeps its output history in memory, and ends when the author closes it,
//! when its target is gone, or when the project closes.

pub mod commands;
pub mod flow;
pub mod gather;
pub mod history;
pub mod host;
pub mod model;
pub mod process;
pub mod registry;
pub mod service;
pub mod session;
pub mod targets;

#[cfg(test)]
mod tests;

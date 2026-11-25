//! Terminal sessions: the author's own interactive terminals into a worktree or
//! a work stream (`TRS-terminal-sessions.md`).
//!
//! A session is a process on a pseudo-terminal. It outlives the tab that shows
//! it and keeps its output history in memory. A persistent session runs in the
//! application's tmux server and outlives the application too; every other
//! session ends when the author closes it, when its target is gone, or when
//! the project closes.

pub mod commands;
pub mod flow;
pub mod gather;
pub mod history;
pub mod host;
pub mod input;
pub mod model;
pub mod persistent;
pub mod process;
pub mod registry;
pub mod service;
pub mod session;
pub mod targets;
pub mod tmux;

#[cfg(test)]
mod tests;

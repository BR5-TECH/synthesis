//! The tests of the Dashboard widget loaders, of the refresh slots, and of
//! the timer behind them (PST-FR-31 … PST-FR-37 / DSH-dashboard.md).
//!
//! This head holds the scope and the fixture the topic files share.

use super::*;

fn at(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

mod active_drafts;
mod events;
mod loaders;
mod recent;
mod refresh;
mod runs;

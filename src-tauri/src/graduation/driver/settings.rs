//! The bounds one turn is dispatched under (GRL-FR-REPL, GRL-FR-KWNP).
//!
//! The pass budget, the stage time limit and the total time limit are the
//! project's own graduation settings (`PSS-project-settings-storage.md`
//! PSS-FR-QBNE, PSS-FR-HVSM, PSS-FR-RKTW). They are read **before every
//! dispatch** rather than once per run, so an author who corrects a bound
//! corrects the run they are waiting on rather than the next one.
//!
//! GRL-FR-NUDC: the shared execution timeout and the shared retry budget
//! (PSS-FR-TQMV, PSS-FR-WPKS) are not read here. They belong to the
//! conversation loop alone, as does the shared provider-call deadline
//! (PSS-FR-HDBN): no phase of this loop reaches a model.

use super::*;

/// GRL-FR-REPL: what one turn is bounded by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopSettings {
    /// How long one turn may run (PSS-FR-HVSM).
    pub stage_time_limit_ms: u64,
    /// How much working and reviewing time the run may use (PSS-FR-RKTW).
    pub total_time_limit_ms: u64,
    /// How many passes the run may take from its budget window's floor.
    pub pass_budget: u32,
}

impl Default for LoopSettings {
    /// GRL-FR-REPL: the loop's own defaults, which stand wherever the project
    /// configures nothing.
    fn default() -> Self {
        Self {
            stage_time_limit_ms: phases::DEFAULT_STAGE_TIME_LIMIT_MS,
            total_time_limit_ms: phases::DEFAULT_TOTAL_TIME_LIMIT_MS,
            pass_budget: phases::DEFAULT_PASS_BUDGET,
        }
    }
}

impl LoopSettings {
    /// GRL-FR-KWNP: read the settings this dispatch runs under.
    ///
    /// Best-effort, on the terms `PSS-FR-JXOU` reads any bound: a project that
    /// is not open, and a store that cannot be read, both leave the loop at its
    /// own defaults rather than stopping a turn that would otherwise have run.
    pub fn read<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Self {
        Self::try_read(app).unwrap_or_default()
    }

    /// The same, saying why the project's own bounds were not used.
    ///
    /// A caller that persists something derived from these — the pass budget
    /// window of GXD-FR-PWYD — reports the failure, because the window it then
    /// writes is narrower than the one the project configured and nothing else
    /// would say why.
    pub fn try_read<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Result<Self, String> {
        let root = project_root(app).ok_or_else(|| "no project is open".to_string())?;
        let stored =
            crate::project_settings::graduation_loop::GraduationLoopSettings::try_read(&root)?;
        Ok(Self {
            stage_time_limit_ms: stored.stage_time_limit_ms,
            total_time_limit_ms: stored.total_time_limit_ms,
            pass_budget: stored.pass_budget,
        })
    }

    /// GRL-FR-EFTM: the remaining total time of a run that used `used_ms`.
    pub fn remaining_total_ms(&self, used_ms: u64) -> u64 {
        self.total_time_limit_ms.saturating_sub(used_ms)
    }

    /// GRL-FR-EFTM: the timeout one turn of a run that used `used_ms` is
    /// dispatched under.
    pub fn turn_bound(&self, used_ms: u64) -> TurnBound {
        let remaining = self.remaining_total_ms(used_ms);
        TurnBound {
            limit_ms: self.stage_time_limit_ms.min(remaining),
            // GRL-FR-WZTE: where the remaining total time is not larger than
            // the stage time limit, it is the total time that ends the turn.
            total_governs: remaining <= self.stage_time_limit_ms,
            total_limit_ms: self.total_time_limit_ms,
        }
    }

    /// GXD-FR-PWYD: the highest pass the run's current budget window allows.
    ///
    /// Recomputed at every dispatch from the settings then held, so a budget
    /// the author raised while the run rested reaches the run itself.
    pub fn pass_limit(&self, pass_floor: u32) -> u32 {
        pass_floor.max(1).saturating_add(self.pass_budget.max(1) - 1)
    }
}

impl LoopSettings {
    /// GRL-FR-CZBT: the highest pass the window of one **run** allows.
    ///
    /// A merge run has a budget of its own, a constant of the module, so what the
    /// project configured for ordinary runs does not reach it.
    pub fn pass_limit_for(&self, run: &GraduationRun) -> u32 {
        self.pass_limit_from(run, run.pass_floor())
    }

    /// The same, for a window that starts at `floor`.
    pub fn pass_limit_from(&self, run: &GraduationRun, floor: u32) -> u32 {
        if run.is_merge() {
            floor.max(1).saturating_add(crate::graduation::MERGE_PASS_BUDGET.max(1) - 1)
        } else {
            self.pass_limit(floor)
        }
    }
}

/// GRL-FR-EFTM: the timeout one turn is dispatched under, and which bound it is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TurnBound {
    /// The effective timeout: the smaller of the stage time limit and the
    /// remaining total time.
    pub limit_ms: u64,
    /// GRL-FR-WZTE: whether the remaining total time is what ends the turn.
    pub total_governs: bool,
    /// The total time limit the turn was dispatched under.
    pub total_limit_ms: u64,
}

/// The open project's root, read through the filesystem instance the
/// application already holds (`FSA-filesystem-access.md`).
fn project_root<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<crate::fs::RootFs> {
    let state = app.try_state::<crate::project::ProjectState>()?;
    let root = state.require_root().ok()?;
    Some(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// GRL-FR-REPL: the loop's own bounds are the ones the specification
    /// states, so a project that configures nothing runs under three hours a
    /// turn, five hours a run and two passes.
    #[test]
    fn the_loops_own_bounds_are_the_ones_the_specification_states() {
        let settings = LoopSettings::default();
        assert_eq!(settings.stage_time_limit_ms, 3 * 60 * 60 * 1000);
        assert_eq!(settings.total_time_limit_ms, 5 * 60 * 60 * 1000);
        assert_eq!(settings.pass_budget, 2);
    }

    /// GRL-FR-EFTM / GRL-FR-WZTE: a turn runs under the smaller of the stage
    /// time limit and the remaining total time, and the bound says which one
    /// ends it.
    #[test]
    fn a_turn_runs_under_the_smaller_of_the_stage_limit_and_the_remaining_total() {
        let hour = 60 * 60 * 1000;
        let settings = LoopSettings {
            stage_time_limit_ms: 3 * hour,
            total_time_limit_ms: 5 * hour,
            ..LoopSettings::default()
        };
        let fresh = settings.turn_bound(0);
        assert_eq!(fresh.limit_ms, 3 * hour);
        assert!(!fresh.total_governs);

        let late = settings.turn_bound(4 * hour);
        assert_eq!(late.limit_ms, hour);
        assert!(late.total_governs);

        // Equal bounds end the run on the total, so Continue does not rest it
        // again at once on a stage timeout.
        assert!(settings.turn_bound(2 * hour).total_governs);

        // GRL-FR-WOBK: time used past the limit leaves nothing, never less.
        assert_eq!(settings.remaining_total_ms(6 * hour), 0);
        assert_eq!(settings.turn_bound(6 * hour).limit_ms, 0);
    }

    /// GXD-FR-PWYD: the window's highest pass is its floor plus the budget,
    /// counting the floor itself as the first pass of the window.
    #[test]
    fn the_window_bounds_the_budget_from_its_own_floor() {
        let two = LoopSettings::default();
        assert_eq!(two.pass_limit(1), 2);
        // Continue after exhaustion moves the floor, so the next window grants
        // a whole budget again without repeating a completed pass.
        assert_eq!(two.pass_limit(3), 4);

        let one = LoopSettings { pass_budget: 1, ..LoopSettings::default() };
        assert_eq!(one.pass_limit(1), 1);
        assert_eq!(one.pass_limit(5), 5);

        // A budget nobody could have meant is read as one pass rather than as
        // none, so a run always makes progress.
        let none = LoopSettings { pass_budget: 0, ..LoopSettings::default() };
        assert_eq!(none.pass_limit(1), 1);
    }
}

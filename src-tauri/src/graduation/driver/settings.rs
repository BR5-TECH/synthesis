//! The bounds one turn is dispatched under (GRL-FR-REPL, GRL-FR-KWNP).
//!
//! The execution timeout and the pass budget are the project's own settings
//! (`PSS-project-settings-storage.md` PSS-FR-TQMV, PSS-FR-WPKS). They are read
//! **before every dispatch** rather than once per run, so an author who
//! corrects a bound corrects the run they are waiting on rather than the next
//! one.
//!
//! The shared provider-call deadline (PSS-FR-HDBN) is not among them. No phase
//! of this loop reaches a model, so that bound constrains nothing here and is
//! the conversation loop's alone (`CVL-conversation-loop.md` CVL-FR-17).

use super::*;

/// GRL-FR-REPL: what one turn is bounded by.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LoopSettings {
    /// How long this turn may run.
    pub execution_timeout_ms: u64,
    /// How many passes the run may take from its budget window's floor.
    pub pass_budget: u32,
}

impl Default for LoopSettings {
    /// GRL-FR-REPL: the loop's own defaults, which stand wherever the project
    /// configures nothing.
    fn default() -> Self {
        Self {
            execution_timeout_ms: phases::DEFAULT_EXECUTION_TIMEOUT_MS,
            pass_budget: phases::DEFAULT_PASS_BUDGET,
        }
    }
}

impl LoopSettings {
    /// GRL-FR-KWNP: read the settings this dispatch runs under.
    ///
    /// Best-effort, on the terms `PSS-FR-JXOU` reads any shared bound: a
    /// project that is not open, and a store that cannot be read, both leave
    /// the loop at its own defaults rather than stopping a turn that would
    /// otherwise have run.
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
        let stored = crate::project_settings::loop_settings::SharedLoopSettings::try_read(&root)?;
        Ok(Self {
            execution_timeout_ms: stored
                .execution_timeout_ms_or(phases::DEFAULT_EXECUTION_TIMEOUT_MS),
            pass_budget: stored.retry_budget_or(phases::DEFAULT_PASS_BUDGET),
        })
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
            floor
                .max(1)
                .saturating_add(crate::graduation::MERGE_PASS_BUDGET.max(1) - 1)
        } else {
            self.pass_limit(floor)
        }
    }
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
    /// states, so a project that configures nothing runs under two hours and
    /// two passes.
    #[test]
    fn the_loops_own_bounds_are_the_ones_the_specification_states() {
        let settings = LoopSettings::default();
        assert_eq!(settings.execution_timeout_ms, 2 * 60 * 60 * 1000);
        assert_eq!(settings.pass_budget, 2);
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

        let one = LoopSettings {
            pass_budget: 1,
            ..LoopSettings::default()
        };
        assert_eq!(one.pass_limit(1), 1);
        assert_eq!(one.pass_limit(5), 5);

        // A budget nobody could have meant is read as one pass rather than as
        // none, so a run always makes progress.
        let none = LoopSettings {
            pass_budget: 0,
            ..LoopSettings::default()
        };
        assert_eq!(none.pass_limit(1), 1);
    }
}

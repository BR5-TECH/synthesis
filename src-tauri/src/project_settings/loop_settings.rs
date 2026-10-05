//! The three shared loop settings (PSS-FR-TQMV, PSS-FR-HDBN, PSS-FR-WPKS).
//!
//! One bound each for how long a turn may run, how long one call to a model may
//! run, and how much a bounded loop may spend before it stops for the author.
//! Both agent loops read them: `../../../specifications/ai/GRL-graduation-loop.md`
//! counts passes against the budget and
//! `../../../specifications/ai/CVL-conversation-loop.md` counts the physical
//! attempts of one provider call, each keeping its own meaning for the one
//! number.
//!
//! Split out of the module root, which had reached a thousand lines. What the
//! store holds and what a read returns are unchanged.

use super::{load_project_config_from, ProjectConfig};

/// PSS-FR-TQMV: the values an execution timeout may take, in milliseconds.
pub const EXECUTION_TIMEOUT_BOUNDS: std::ops::RangeInclusive<u64> = 1_000..=21_600_000;
/// PSS-FR-HDBN: the values a provider-call deadline may take, in milliseconds.
pub const PROVIDER_CALL_DEADLINE_BOUNDS: std::ops::RangeInclusive<u64> = 1_000..=3_600_000;
/// PSS-FR-WPKS: the values a retry budget may take.
pub const RETRY_BUDGET_BOUNDS: std::ops::RangeInclusive<u64> = 1..=10;

/// PSS-FR-TQMV: the shared bound on one agent turn's execution.
const EXECUTION_TIMEOUT_KEY: &str = "executionTimeoutMs";
/// PSS-FR-HDBN: the shared bound on one call to a model.
const PROVIDER_CALL_DEADLINE_KEY: &str = "providerCallDeadlineMs";
/// PSS-FR-WPKS: the shared bound on what a loop may spend before it stops.
const RETRY_BUDGET_KEY: &str = "retryBudget";

/// PSS-FR-ZLCF: one stored integer, or nothing where the store holds no valid
/// one.
///
/// A key that is absent, that holds something which is not an integer, or that
/// holds one outside the bounds all read as unset. A malformed **file** is the
/// typed error of PSS-FR-10 and is refused before this is reached; one odd
/// value is not a damaged store.
fn bounded_u64(
    table: &toml::value::Table,
    key: &str,
    bounds: std::ops::RangeInclusive<u64>,
) -> Option<u64> {
    let stored = table.get(key)?.as_integer()?;
    let value = u64::try_from(stored).ok()?;
    bounds.contains(&value).then_some(value)
}

/// PSS-FR-ZLCF: record one shared setting, or clear it, or say nothing of it.
fn write_bounded(
    table: &mut toml::value::Table,
    key: &str,
    value: Option<Option<u64>>,
    bounds: std::ops::RangeInclusive<u64>,
) {
    match value {
        // The key was absent from the payload: this write says nothing about
        // the bound and carries whatever is stored through unchanged.
        None => {}
        Some(Some(value)) if bounds.contains(&value) => {
            table.insert(key.to_string(), toml::Value::Integer(value as i64));
        }
        // `null`, and a value nobody could have meant, both return the project
        // to the loop's own default.
        Some(_) => {
            table.remove(key);
        }
    }
}

/// PSS-FR-JXOU: the three shared bounds, read from the committed project.
///
/// Best-effort by design, exactly as [`line_endings_for`] is: a damaged
/// `project.toml` is surfaced by `load_project_config` where the author can act
/// on it, and it must never stop a turn that would otherwise have run. Each
/// unset value leaves the caller at the default its own module states.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SharedLoopSettings {
    pub execution_timeout_ms: Option<u64>,
    pub provider_call_deadline_ms: Option<u64>,
    pub retry_budget: Option<u32>,
}

impl SharedLoopSettings {
    /// PSS-FR-JXOU: read from the store rather than from a cache, so every read
    /// reports what the project holds at that moment.
    pub fn read(root: &crate::fs::RootFs) -> Self {
        Self::try_read(root).unwrap_or_default()
    }

    /// The same, saying why the store could not be read.
    ///
    /// A caller that only wants a bound takes [`Self::read`]; one that persists
    /// something derived from these reports the failure instead of writing a
    /// narrower bound with nothing to explain it.
    pub fn try_read(root: &crate::fs::RootFs) -> Result<Self, String> {
        let config = load_project_config_from(root)?;
        Ok(Self {
            execution_timeout_ms: config.execution_timeout_ms.flatten(),
            provider_call_deadline_ms: config.provider_call_deadline_ms.flatten(),
            retry_budget: config.retry_budget.flatten(),
        })
    }

    pub fn execution_timeout_ms_or(&self, default: u64) -> u64 {
        self.execution_timeout_ms.unwrap_or(default)
    }

    pub fn provider_call_deadline_ms_or(&self, default: u64) -> u64 {
        self.provider_call_deadline_ms.unwrap_or(default)
    }

    pub fn retry_budget_or(&self, default: u32) -> u32 {
        self.retry_budget.unwrap_or(default)
    }
}

/// PSS-FR-TQMV: the stored execution timeout, or nothing where the store holds
/// no valid one.
pub(super) fn read_execution_timeout(table: &toml::value::Table) -> Option<Option<u64>> {
    Some(bounded_u64(table, EXECUTION_TIMEOUT_KEY, EXECUTION_TIMEOUT_BOUNDS))
}

/// PSS-FR-HDBN: the stored provider-call deadline, on the same terms.
pub(super) fn read_provider_call_deadline(table: &toml::value::Table) -> Option<Option<u64>> {
    Some(bounded_u64(
        table,
        PROVIDER_CALL_DEADLINE_KEY,
        PROVIDER_CALL_DEADLINE_BOUNDS,
    ))
}

/// PSS-FR-WPKS: the stored retry budget, on the same terms.
pub(super) fn read_retry_budget(table: &toml::value::Table) -> Option<Option<u32>> {
    Some(bounded_u64(table, RETRY_BUDGET_KEY, RETRY_BUDGET_BOUNDS).and_then(|n| u32::try_from(n).ok()))
}

/// PSS-FR-ZLCF, on the three ways a write can speak about a shared bound.
///
/// `None` says nothing and carries the stored value through, which is what lets
/// any other section be saved without disturbing a configured bound. A value
/// outside its bounds removes the key, which is how a section returns the
/// project to each loop's own default.
pub(super) fn write_into(table: &mut toml::value::Table, config: &ProjectConfig) {
    write_bounded(
        table,
        EXECUTION_TIMEOUT_KEY,
        config.execution_timeout_ms,
        EXECUTION_TIMEOUT_BOUNDS,
    );
    write_bounded(
        table,
        PROVIDER_CALL_DEADLINE_KEY,
        config.provider_call_deadline_ms,
        PROVIDER_CALL_DEADLINE_BOUNDS,
    );
    write_bounded(
        table,
        RETRY_BUDGET_KEY,
        config.retry_budget.map(|value| value.map(u64::from)),
        RETRY_BUDGET_BOUNDS,
    );
}

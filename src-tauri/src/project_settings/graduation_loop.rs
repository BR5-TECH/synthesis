//! The three graduation loop settings (PSS-FR-QBNE, PSS-FR-HVSM, PSS-FR-RKTW).
//!
//! How many passes a graduation run may take, how long one of its turns may
//! run, and how much working and reviewing time the whole run may use. Only the
//! graduation loop reads them (`../../../specifications/ai/GRL-graduation-loop.md`
//! GRL-FR-REPL). The shared execution timeout and retry budget of
//! [`super::loop_settings`] stay the conversation loop's alone.
//!
//! Unlike the shared settings, a write with a value that is not valid is
//! refused whole and writes nothing (PSS-FR-LFUZ). The Graduation section shows
//! the refusal and keeps the previous value.

use super::{load_project_config_from, ProjectConfig};

/// One half hour, the step of both time limits (PSS-FR-HVSM, PSS-FR-RKTW).
pub const TIME_LIMIT_STEP_MS: i64 = 30 * 60 * 1000;

/// One graduation loop setting: its key, its bounds, its step and its refusal.
#[derive(Clone, Copy, Debug)]
pub struct GraduationSetting {
    pub key: &'static str,
    pub min: i64,
    pub max: i64,
    pub step: i64,
    pub default: i64,
    pub refusal: &'static str,
}

impl GraduationSetting {
    /// PSS-FR-YDCA: whether a value is inside the bounds and on a step.
    pub fn accepts(&self, value: i64) -> bool {
        (self.min..=self.max).contains(&value) && (value - self.min) % self.step == 0
    }
}

/// PSS-FR-QBNE: the graduation pass budget.
pub const PASS_BUDGET: GraduationSetting = GraduationSetting {
    key: "graduationPassBudget",
    min: 1,
    max: 10,
    step: 1,
    default: 2,
    refusal: "graduation_pass_budget_invalid",
};

/// PSS-FR-HVSM: the graduation stage time limit, in milliseconds.
pub const STAGE_TIME_LIMIT: GraduationSetting = GraduationSetting {
    key: "graduationStageTimeLimitMs",
    min: TIME_LIMIT_STEP_MS,
    max: 48 * 60 * 60 * 1000,
    step: TIME_LIMIT_STEP_MS,
    default: 3 * 60 * 60 * 1000,
    refusal: "graduation_stage_time_limit_invalid",
};

/// PSS-FR-RKTW: the graduation total time limit, in milliseconds.
pub const TOTAL_TIME_LIMIT: GraduationSetting = GraduationSetting {
    key: "graduationTotalTimeLimitMs",
    min: TIME_LIMIT_STEP_MS,
    max: 96 * 60 * 60 * 1000,
    step: TIME_LIMIT_STEP_MS,
    default: 5 * 60 * 60 * 1000,
    refusal: "graduation_total_time_limit_invalid",
};

/// PSS-FR-YDCA: the stored value, or unset where the store holds no valid one.
///
/// Always `Some` on read: `Some(None)` is unset. A value that is not an
/// integer, is outside the bounds, or is not on a step reads as unset.
pub(super) fn read(table: &toml::value::Table, setting: GraduationSetting) -> Option<Option<i64>> {
    Some(table.get(setting.key).and_then(|v| v.as_integer()).filter(|v| setting.accepts(*v)))
}

/// The three settings of a payload, each with the setting it belongs to.
fn named(config: &ProjectConfig) -> [(GraduationSetting, Option<Option<i64>>); 3] {
    [
        (PASS_BUDGET, config.graduation_pass_budget),
        (STAGE_TIME_LIMIT, config.graduation_stage_time_limit_ms),
        (TOTAL_TIME_LIMIT, config.graduation_total_time_limit_ms),
    ]
}

/// PSS-FR-LFUZ: refuse a payload that names a value that is not valid.
///
/// Called before anything is written, so a refused save writes nothing.
pub(super) fn validate(config: &ProjectConfig) -> Result<(), String> {
    for (setting, value) in named(config) {
        if let Some(Some(value)) = value {
            if !setting.accepts(value) {
                return Err(setting.refusal.to_string());
            }
        }
    }
    Ok(())
}

/// PSS-FR-OWGI: an absent key says nothing, `null` clears, a value stores.
pub(super) fn write_into(table: &mut toml::value::Table, config: &ProjectConfig) {
    for (setting, value) in named(config) {
        match value {
            None => {}
            Some(None) => {
                table.remove(setting.key);
            }
            Some(Some(value)) => {
                table.insert(setting.key.to_string(), toml::Value::Integer(value));
            }
        }
    }
}

/// GRL-FR-REPL / GRL-FR-KWNP: the three settings as the graduation loop reads
/// them, each unset value replaced by its default.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraduationLoopSettings {
    pub pass_budget: u32,
    pub stage_time_limit_ms: u64,
    pub total_time_limit_ms: u64,
}

impl Default for GraduationLoopSettings {
    fn default() -> Self {
        Self {
            pass_budget: PASS_BUDGET.default as u32,
            stage_time_limit_ms: STAGE_TIME_LIMIT.default as u64,
            total_time_limit_ms: TOTAL_TIME_LIMIT.default as u64,
        }
    }
}

impl GraduationLoopSettings {
    /// PSS-FR-JXOU: read from the committed project and no cache.
    pub fn try_read(root: &crate::fs::RootFs) -> Result<Self, String> {
        let config = load_project_config_from(root)?;
        let defaults = Self::default();
        Ok(Self {
            pass_budget: config
                .graduation_pass_budget
                .flatten()
                .map_or(defaults.pass_budget, |v| v as u32),
            stage_time_limit_ms: config
                .graduation_stage_time_limit_ms
                .flatten()
                .map_or(defaults.stage_time_limit_ms, |v| v as u64),
            total_time_limit_ms: config
                .graduation_total_time_limit_ms
                .flatten()
                .map_or(defaults.total_time_limit_ms, |v| v as u64),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // PSS-FR-QBNE, PSS-FR-HVSM, PSS-FR-RKTW: the bounds, the steps and the
    // defaults are the ones the specification states.
    #[test]
    fn each_setting_holds_the_bounds_steps_and_default_of_the_specification() {
        assert!(PASS_BUDGET.accepts(1) && PASS_BUDGET.accepts(10));
        assert!(!PASS_BUDGET.accepts(0) && !PASS_BUDGET.accepts(11) && !PASS_BUDGET.accepts(-1));
        assert_eq!(PASS_BUDGET.default, 2);

        let half_hour = 1_800_000;
        assert!(STAGE_TIME_LIMIT.accepts(half_hour) && STAGE_TIME_LIMIT.accepts(172_800_000));
        assert!(!STAGE_TIME_LIMIT.accepts(172_800_000 + half_hour));
        assert!(!STAGE_TIME_LIMIT.accepts(half_hour + 60_000), "a value off the step");
        assert!(!STAGE_TIME_LIMIT.accepts(0));
        assert_eq!(STAGE_TIME_LIMIT.default, 10_800_000);

        assert!(TOTAL_TIME_LIMIT.accepts(half_hour) && TOTAL_TIME_LIMIT.accepts(345_600_000));
        assert!(!TOTAL_TIME_LIMIT.accepts(345_600_000 + half_hour));
        assert_eq!(TOTAL_TIME_LIMIT.default, 18_000_000);
    }

    // PSS-FR-YDCA: a stored value that is not valid reads as unset.
    #[test]
    fn a_stored_value_that_is_not_valid_reads_as_unset() {
        let table: toml::value::Table = toml::from_str(
            "graduationPassBudget = 0\ngraduationStageTimeLimitMs = 60000\ngraduationTotalTimeLimitMs = \"5h\"\n",
        )
        .unwrap();
        assert_eq!(read(&table, PASS_BUDGET), Some(None));
        assert_eq!(read(&table, STAGE_TIME_LIMIT), Some(None));
        assert_eq!(read(&table, TOTAL_TIME_LIMIT), Some(None));
        let table: toml::value::Table =
            toml::from_str("graduationPassBudget = 4\ngraduationStageTimeLimitMs = 3600000\n")
                .unwrap();
        assert_eq!(read(&table, PASS_BUDGET), Some(Some(4)));
        assert_eq!(read(&table, STAGE_TIME_LIMIT), Some(Some(3_600_000)));
        assert_eq!(read(&table, TOTAL_TIME_LIMIT), Some(None));
    }

    // PSS-FR-LFUZ: a payload that names a value that is not valid is refused
    // with the typed error of that setting.
    #[test]
    fn a_payload_with_a_value_that_is_not_valid_is_refused_by_name() {
        let refused = |config: ProjectConfig| validate(&config).unwrap_err();
        assert_eq!(
            refused(ProjectConfig { graduation_pass_budget: Some(Some(0)), ..Default::default() }),
            "graduation_pass_budget_invalid"
        );
        assert_eq!(
            refused(ProjectConfig {
                graduation_stage_time_limit_ms: Some(Some(1_000)),
                ..Default::default()
            }),
            "graduation_stage_time_limit_invalid"
        );
        assert_eq!(
            refused(ProjectConfig {
                graduation_total_time_limit_ms: Some(Some(-1_800_000)),
                ..Default::default()
            }),
            "graduation_total_time_limit_invalid"
        );
        // `null` and an absent key are never refused.
        assert!(validate(&ProjectConfig {
            graduation_pass_budget: Some(None),
            ..Default::default()
        })
        .is_ok());
        assert!(validate(&ProjectConfig::default()).is_ok());
    }

    // PSS-FR-OWGI: an absent key leaves the stored value, `null` clears it, and
    // a valid value stores it.
    #[test]
    fn a_write_says_nothing_clears_or_stores() {
        let mut table: toml::value::Table =
            toml::from_str("graduationPassBudget = 4\ngraduationStageTimeLimitMs = 3600000\n")
                .unwrap();
        write_into(
            &mut table,
            &ProjectConfig {
                graduation_stage_time_limit_ms: Some(None),
                graduation_total_time_limit_ms: Some(Some(7_200_000)),
                ..Default::default()
            },
        );
        assert_eq!(table.get("graduationPassBudget").and_then(|v| v.as_integer()), Some(4));
        assert!(table.get("graduationStageTimeLimitMs").is_none());
        assert_eq!(
            table.get("graduationTotalTimeLimitMs").and_then(|v| v.as_integer()),
            Some(7_200_000)
        );
    }
}

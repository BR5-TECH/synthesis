//! Why a run rests for the author without an escalation, and the check gate a
//! work turn passes before its review (`GRD-graduation.md` GRD-FR-SKLO,
//! GRD-FR-PYJN; `../ai/GRL-graduation-loop.md` GRL-FR-CPXI, GRL-FR-BVTS;
//! `GXD-graduation-execution.md` GXD-FR-UREF).

use super::*;

/// GRD-FR-SKLO: the reason a run rests on its total time limit.
pub const REST_TOTAL_TIME_LIMIT: &str = "total_time_limit";

/// GRD-FR-PYJN: the reason a run rests when its check gate is spent.
pub const REST_CHECKS_NOT_PASSED: &str = "checks_not_passed";

/// GRD-FR-SKLO / GRD-FR-PYJN: a run at rest for the author, without an
/// escalation and without a blocker.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationRest {
    /// `total_time_limit` or `checks_not_passed`.
    pub reason: String,
    /// RFC 3339 UTC.
    pub at: String,
    /// `total_time_limit`: the total time limit the run rested under.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_ms: Option<u64>,
    /// `total_time_limit`: the used total time when it rested.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub used_ms: Option<u64>,
    /// `checks_not_passed`: the cause of the last refusal of the check gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cause: Option<GraduationCheckCause>,
    /// `checks_not_passed`: each check of the last record that did not pass.
    /// Empty where the answer held no record.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checks: Option<Vec<GraduationRestCheck>>,
}

impl GraduationRest {
    /// GRD-FR-SKLO: the rest on the total time limit.
    pub fn total_time_limit(at: String, limit_ms: u64, used_ms: u64) -> Self {
        Self {
            reason: REST_TOTAL_TIME_LIMIT.to_string(),
            at,
            limit_ms: Some(limit_ms),
            used_ms: Some(used_ms),
            cause: None,
            checks: None,
        }
    }

    /// GRD-FR-PYJN: the rest of a run whose check gate is spent.
    pub fn checks_not_passed(
        at: String,
        cause: GraduationCheckCause,
        checks: Vec<GraduationRestCheck>,
    ) -> Self {
        Self {
            reason: REST_CHECKS_NOT_PASSED.to_string(),
            at,
            limit_ms: None,
            used_ms: None,
            cause: Some(cause),
            checks: Some(checks),
        }
    }

    pub fn is_checks_not_passed(&self) -> bool {
        self.reason == REST_CHECKS_NOT_PASSED
    }
}

/// GRD-FR-PYJN: one check that did not pass, as the rest names it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationRestCheck {
    pub name: String,
    pub finished: bool,
    /// Null where the check did not end.
    pub exit_status: Option<i64>,
}

/// GRL-FR-CPXI / GRL-FR-TYRV: why the check gate refused an answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraduationCheckCause {
    /// The result is not a valid `WorkResult`, or holds no check and no reason.
    RecordMissing,
    /// A check did not finish.
    ChecksUnfinished,
    /// A check has an exit status other than 0 and no valid waiver.
    ChecksRed,
    /// The turn answered `failure`.
    WorkFailed,
}

impl GraduationCheckCause {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RecordMissing => "record_missing",
            Self::ChecksUnfinished => "checks_unfinished",
            Self::ChecksRed => "checks_red",
            Self::WorkFailed => "work_failed",
        }
    }

    /// GRL-FR-CHDT: the `refusals.md` section of the check correction for this
    /// cause.
    pub fn correction_section(self) -> &'static str {
        match self {
            Self::ChecksUnfinished | Self::ChecksRed => "checks_not_passing",
            Self::RecordMissing => "checks_record_missing",
            Self::WorkFailed => "work_failed",
        }
    }
}

/// GRL-FR-BVTS: the step of an open check gate that is dispatched next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraduationCheckGateStep {
    /// GRL-FR-CHDT: the check correction turn.
    Correct,
    /// GRL-FR-SUBP: the escalate correction turn.
    Escalate,
}

impl GraduationCheckGateStep {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Correct => "correct",
            Self::Escalate => "escalate",
        }
    }
}

/// GRL-FR-BVTS / GXD-FR-UREF: an open check gate, with the cause of the last
/// refusal, the step dispatched next, and the session that step resumes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraduationCheckGate {
    pub cause: GraduationCheckCause,
    pub step: GraduationCheckGateStep,
    /// GRL-FR-SMQK: the session the refused turn reported, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resume: Option<GraduationResumeRef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // GRD-FR-PYJN, GRD-FR-SKLO: each rest serializes the fields of its reason
    // and no other, and a record written before the optional fields existed
    // still reads.
    #[test]
    fn each_rest_carries_the_fields_of_its_reason() {
        let checks = GraduationRest::checks_not_passed(
            "2026-11-08T09:00:00Z".into(),
            GraduationCheckCause::ChecksRed,
            vec![GraduationRestCheck {
                name: "tests".into(),
                finished: true,
                exit_status: Some(101),
            }],
        );
        let value = serde_json::to_value(&checks).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "reason": "checks_not_passed",
                "at": "2026-11-08T09:00:00Z",
                "cause": "checks_red",
                "checks": [{ "name": "tests", "finished": true, "exitStatus": 101 }],
            })
        );
        let unfinished =
            GraduationRestCheck { name: "build".into(), finished: false, exit_status: None };
        assert_eq!(
            serde_json::to_value(&unfinished).unwrap(),
            serde_json::json!({ "name": "build", "finished": false, "exitStatus": null })
        );

        let time = GraduationRest::total_time_limit("2026-11-08T09:00:00Z".into(), 60, 61);
        let value = serde_json::to_value(&time).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "reason": "total_time_limit",
                "at": "2026-11-08T09:00:00Z",
                "limitMs": 60,
                "usedMs": 61,
            })
        );
        let old: GraduationRest =
            serde_json::from_value(serde_json::json!({ "reason": "total_time_limit", "at": "t0", "limitMs": 1, "usedMs": 2 }))
                .unwrap();
        assert_eq!(old, GraduationRest::total_time_limit("t0".into(), 1, 2));
    }

    // GRL-FR-BVTS, GXD-FR-UREF: the open check gate round-trips through the
    // checkpoint with its cause, its step and its session, and a checkpoint
    // written before it existed reads as no gate.
    #[test]
    fn the_check_gate_round_trips_through_the_checkpoint() {
        let mut checkpoint = GraduationCheckpoint::default();
        assert!(!serde_json::to_value(&checkpoint)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("checkGate"));
        checkpoint.check_gate = Some(GraduationCheckGate {
            cause: GraduationCheckCause::RecordMissing,
            step: GraduationCheckGateStep::Escalate,
            resume: Some(GraduationResumeRef {
                session_id: Some("s1".into()),
                continuation_token: None,
            }),
        });
        checkpoint.continue_check_correction = Some(GraduationCheckCause::WorkFailed);
        let value = serde_json::to_value(&checkpoint).unwrap();
        assert_eq!(
            value["checkGate"],
            serde_json::json!({ "cause": "record_missing", "step": "escalate", "resume": { "sessionId": "s1" } })
        );
        assert_eq!(value["continueCheckCorrection"], "work_failed");
        let back: GraduationCheckpoint = serde_json::from_value(value).unwrap();
        assert_eq!(back, checkpoint);
    }

    /// The TOML text of `text` without the tables whose header starts with one
    /// of `tables`, and without the lines that start with one of `keys`.
    fn without(text: &str, tables: &[&str], keys: &[&str]) -> String {
        let mut dropping = false;
        let mut kept = Vec::new();
        for line in text.lines() {
            if line.starts_with('[') {
                dropping = tables.iter().any(|table| line.starts_with(table));
            }
            if dropping || keys.iter().any(|key| line.starts_with(key)) {
                continue;
            }
            kept.push(line);
        }
        kept.join("\n")
    }

    // GRL-FR-BVTS, GXD-FR-UREF, GRD-FR-OERK: the run store writes TOML. A run
    // with an open gate and a continued correction round-trips through it, and
    // a run record written before either field existed reads as no gate and no
    // continued correction.
    #[test]
    fn a_run_record_in_toml_keeps_the_gate_and_an_old_one_reads_as_none() {
        let mut run = GraduationRun::new_for_test("g1", "d1", "2026-11-08T09:00:00Z");
        run.checkpoint.check_gate = Some(GraduationCheckGate {
            cause: GraduationCheckCause::ChecksRed,
            step: GraduationCheckGateStep::Escalate,
            resume: Some(GraduationResumeRef {
                session_id: Some("s1".into()),
                continuation_token: None,
            }),
        });
        run.checkpoint.continue_check_correction = Some(GraduationCheckCause::ChecksUnfinished);
        let text = toml::to_string(&run).expect("the record serializes");
        assert!(text.contains("checkGate") && text.contains("continueCheckCorrection"), "{text}");
        let back: GraduationRun = toml::from_str(&text).expect("the record reads back");
        assert_eq!(back.checkpoint.check_gate, run.checkpoint.check_gate);
        assert_eq!(
            back.checkpoint.continue_check_correction,
            run.checkpoint.continue_check_correction
        );

        let old = without(&text, &["[checkpoint.checkGate"], &["continueCheckCorrection"]);
        assert!(!old.contains("checkGate") && !old.contains("continueCheckCorrection"), "{old}");
        let read: GraduationRun = toml::from_str(&old).expect("an old record reads");
        assert_eq!(read.checkpoint.check_gate, None);
        assert_eq!(read.checkpoint.continue_check_correction, None);
        assert_eq!(read.id, run.id);
    }

    // GRD-FR-PYJN: a `checks_not_passed` rest round-trips through the TOML
    // store, a check that did not end included. TOML has no null, so the
    // missing exit status must read back as none.
    #[test]
    fn a_checks_rest_with_an_unfinished_check_round_trips_through_toml() {
        let mut run = GraduationRun::new_for_test("g1", "d1", "2026-11-08T09:00:00Z");
        run.rest = Some(GraduationRest::checks_not_passed(
            "2026-11-08T09:30:00Z".into(),
            GraduationCheckCause::ChecksUnfinished,
            vec![
                GraduationRestCheck { name: "build".into(), finished: false, exit_status: None },
                GraduationRestCheck { name: "tests".into(), finished: true, exit_status: Some(-9) },
            ],
        ));
        let text = toml::to_string(&run).expect("the record serializes");
        let back: GraduationRun = toml::from_str(&text).expect("the record reads back");
        assert_eq!(back.rest, run.rest, "{text}");

        // A rest with no check record holds an empty list, not a missing one.
        run.rest = Some(GraduationRest::checks_not_passed(
            "2026-11-08T09:30:00Z".into(),
            GraduationCheckCause::RecordMissing,
            Vec::new(),
        ));
        let text = toml::to_string(&run).expect("the record serializes");
        let back: GraduationRun = toml::from_str(&text).expect("the record reads back");
        assert_eq!(back.rest, run.rest, "{text}");
    }

    // GRD-FR-SKLO: a `total_time_limit` rest written before the check gate
    // existed, with no cause and no checks, still reads as the time-limit rest.
    #[test]
    fn an_old_total_time_limit_rest_in_toml_still_reads() {
        let mut run = GraduationRun::new_for_test("g1", "d1", "2026-11-08T09:00:00Z");
        run.rest = Some(GraduationRest::total_time_limit("2026-11-08T09:30:00Z".into(), 60, 61));
        let text = toml::to_string(&run).expect("the record serializes");
        assert!(text.contains("[rest]"), "{text}");
        assert!(!text.contains("cause") && !text.contains("checks"), "{text}");
        let back: GraduationRun = toml::from_str(&text).expect("the record reads back");
        assert_eq!(back.rest, run.rest);

        let old = "reason = \"total_time_limit\"\nat = \"t0\"\nlimitMs = 1\nusedMs = 2\n";
        let rest: GraduationRest = toml::from_str(old).expect("an old rest reads");
        assert_eq!(rest, GraduationRest::total_time_limit("t0".into(), 1, 2));
        assert!(!rest.is_checks_not_passed());
    }
}

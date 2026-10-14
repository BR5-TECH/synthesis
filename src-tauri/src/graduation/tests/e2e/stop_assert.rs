//! The assertions that read the `run interrupted` record a stopped turn
//! writes into the run's structured log (GLG-FR-RLQZ).

use crate::graduation::logs::GraduationLogStream;

use super::outcome::Outcome;

impl Outcome {
    /// GLG-FR-RLQZ: the run's own log holds exactly one `run interrupted`
    /// record, from the stage-transition producer, naming `reason` at `level`
    /// and carrying each of `fields` with the value given.
    pub(crate) fn expect_interrupted_record(
        self,
        reason: &str,
        level: &str,
        fields: &[(&str, serde_json::Value)],
    ) -> Self {
        let records =
            super::super::lines_of(&self.fx, &self.run.id, GraduationLogStream::Structured);
        let stops: Vec<&serde_json::Value> = records
            .iter()
            .filter(|record| {
                record.get("event").and_then(|v| v.as_str()) == Some("run interrupted")
            })
            .collect();
        assert_eq!(stops.len(), 1, "[{}] the run's log states the stop exactly once", self.name);
        let stop = stops[0];
        assert_eq!(
            stop.get("producer").and_then(|v| v.as_str()),
            Some("stage_transition"),
            "[{}] the stop is a stage transition",
            self.name
        );
        assert_eq!(
            stop.get("level").and_then(|v| v.as_str()),
            Some(level),
            "[{}] the level of the stop",
            self.name
        );
        let held = stop.get("fields").cloned().unwrap_or_default();
        assert_eq!(
            held.get("reason").and_then(|v| v.as_str()),
            Some(reason),
            "[{}] the reason the log states",
            self.name
        );
        for (key, value) in fields {
            assert_eq!(held.get(*key), Some(value), "[{}] the stop's {key:?}", self.name);
        }
        self
    }

    /// GLG-FR-RLQZ: a field the stop record must not carry.
    pub(crate) fn expect_interrupted_record_lacks(self, key: &str) -> Self {
        let records =
            super::super::lines_of(&self.fx, &self.run.id, GraduationLogStream::Structured);
        let stop = records
            .iter()
            .find(|record| record.get("event").and_then(|v| v.as_str()) == Some("run interrupted"))
            .unwrap_or_else(|| panic!("[{}] the run's log states the stop", self.name));
        assert!(
            stop.get("fields").and_then(|fields| fields.get(key)).is_none(),
            "[{}] the stop carries no {key:?}",
            self.name
        );
        self
    }

    /// GLG-FR-RLQZ: the `run interrupted` records of the run's log, in order,
    /// as `(reason, level)` pairs. For a run that stopped more than once.
    pub(crate) fn expect_interrupted_records(self, expected: &[(&str, &str)]) -> Self {
        let records =
            super::super::lines_of(&self.fx, &self.run.id, GraduationLogStream::Structured);
        let held: Vec<(String, String)> = records
            .iter()
            .filter(|record| {
                record.get("event").and_then(|v| v.as_str()) == Some("run interrupted")
            })
            .map(|record| {
                let reason = record
                    .get("fields")
                    .and_then(|fields| fields.get("reason"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let level =
                    record.get("level").and_then(|v| v.as_str()).unwrap_or_default().to_string();
                (reason, level)
            })
            .collect();
        let expected: Vec<(String, String)> = expected
            .iter()
            .map(|(reason, level)| (reason.to_string(), level.to_string()))
            .collect();
        assert_eq!(held, expected, "[{}] the stops the run's log states, in order", self.name);
        self
    }

    /// GLG-FR-RLQZ / GRD-FR-IKVE: a stop whose record could not be stored
    /// leaves no `run interrupted` record behind.
    pub(crate) fn expect_no_interrupted_record(self) -> Self {
        let records =
            super::super::lines_of(&self.fx, &self.run.id, GraduationLogStream::Structured);
        assert!(
            !records
                .iter()
                .any(|record| record.get("event").and_then(|v| v.as_str())
                    == Some("run interrupted")),
            "[{}] the run's log states no stop",
            self.name
        );
        self
    }

    /// GXD-FR-WJOW: the whole of what the author is told about the stop.
    pub(crate) fn expect_interruption_detail(self, detail: &str) -> Self {
        let held = self.run.interruption.as_ref().map(|i| i.detail.clone()).unwrap_or_default();
        assert_eq!(held, detail, "[{}] what the author is told about the stop", self.name);
        self
    }

    /// GRD-FR-SKLO: the run rests on its total time limit, under `limit_ms`,
    /// and the rest names the time the run had used.
    pub(crate) fn expect_total_time_rest(self, limit_ms: u64) -> Self {
        let rest = self
            .run
            .rest
            .clone()
            .unwrap_or_else(|| panic!("[{}] the run rests on its total time limit", self.name));
        assert_eq!(rest.reason, "total_time_limit", "[{}] the rest reason", self.name);
        assert_eq!(rest.limit_ms, limit_ms, "[{}] the limit the run rested under", self.name);
        assert_eq!(rest.used_ms, self.run.turn_time_used_ms, "[{}] the time used", self.name);
        self
    }

    /// GRD-FR-SKLO: the run holds no rest.
    pub(crate) fn expect_no_rest(self) -> Self {
        assert!(self.run.rest.is_none(), "[{}] no rest is recorded", self.name);
        self
    }

    /// GRD-FR-UTTX: the used total time is at least `at_least` and below
    /// `below`.
    pub(crate) fn expect_time_used(self, at_least: u64, below: u64) -> Self {
        let used = self.run.turn_time_used_ms;
        assert!(
            used >= at_least && used < below,
            "[{}] the used total time {used} is in {at_least}..{below}",
            self.name
        );
        self
    }
}

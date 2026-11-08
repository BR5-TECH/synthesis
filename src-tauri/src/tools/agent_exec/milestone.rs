//! The milestone marker an agent writes in its text messages
//! (`EAC-execute-agent-cli.md` EAC-FR-HRTK, EAC-FR-PQLW, EAC-FR-XDGT,
//! EAC-FR-VCBJ, EAC-FR-KRFS, EAC-FR-QLMD, EAC-FR-BQNV).
//!
//! A marker is a description for the author and nothing else. Every function
//! here is total: a line that is not a valid marker is not a marker, and no
//! input makes the scan fail.

use serde::Deserialize;

/// EAC-FR-PQLW: what starts a marker line.
const PREFIX: &str = "SYNTHESIS_MILESTONE ";
/// EAC-FR-VCBJ: the most titles a plan holds.
pub(super) const MAX_TITLES: usize = 5;
/// EAC-FR-VCBJ: the most characters a title holds.
pub(super) const MAX_TITLE_CHARS: usize = 50;

/// EAC-FR-XDGT: the closed field set of each event.
#[derive(Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
enum RawMarker {
    Plan { milestones: Vec<String> },
    Start { index: u64 },
    Done { index: u64 },
}

/// One valid marker, with its titles as the executor trimmed them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Marker {
    Plan(Vec<String>),
    Start(usize),
    Done(usize),
}

impl Marker {
    /// EAC-FR-ZNBD: the marker as one compact JSON line, `event` first.
    pub(super) fn summary(&self) -> String {
        match self {
            Marker::Plan(titles) => {
                let list = serde_json::to_string(titles).unwrap_or_else(|_| "[]".to_string());
                format!("{{\"event\":\"plan\",\"milestones\":{list}}}")
            }
            Marker::Start(index) => format!("{{\"event\":\"start\",\"index\":{index}}}"),
            Marker::Done(index) => format!("{{\"event\":\"done\",\"index\":{index}}}"),
        }
    }
}

/// EAC-FR-VCBJ: a title, trimmed and with each run of white space collapsed,
/// or `None` where it is not a valid title.
fn normalized_title(raw: &str) -> Option<String> {
    let title = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let valid = !title.is_empty()
        && title.chars().count() <= MAX_TITLE_CHARS
        && !title.chars().any(|c| c == '"' || c == '\\' || c.is_control());
    valid.then_some(title)
}

/// EAC-FR-PQLW, EAC-FR-XDGT, EAC-FR-VCBJ, EAC-FR-KRFS: one line read as a
/// marker, or `None` where it is not one.
pub(super) fn parse_line(line: &str) -> Option<Marker> {
    let rest = line.trim_end().strip_prefix(PREFIX)?;
    if !rest.starts_with('{') {
        return None;
    }
    match serde_json::from_str::<RawMarker>(rest).ok()? {
        RawMarker::Plan { milestones } => {
            if milestones.is_empty() || milestones.len() > MAX_TITLES {
                return None;
            }
            let titles = milestones
                .iter()
                .map(|title| normalized_title(title))
                .collect::<Option<Vec<_>>>()?;
            Some(Marker::Plan(titles))
        }
        RawMarker::Start { index } => {
            usize::try_from(index).ok().filter(|i| *i >= 1).map(Marker::Start)
        }
        RawMarker::Done { index } => {
            usize::try_from(index).ok().filter(|i| *i >= 1).map(Marker::Done)
        }
    }
}

/// EAC-FR-QLMD, EAC-FR-BQNV: the milestone state of one call.
#[derive(Debug, Default)]
pub(super) struct MilestoneState {
    /// Which milestones of the counted plan are done. Empty until a plan counts.
    done: Vec<bool>,
}

impl MilestoneState {
    /// Whether a valid marker is delivered. An ignored marker changes nothing.
    fn accept(&mut self, marker: &Marker) -> bool {
        match marker {
            Marker::Plan(titles) => {
                if !self.done.is_empty() {
                    return false;
                }
                self.done = vec![false; titles.len()];
                true
            }
            Marker::Start(index) | Marker::Done(index) => {
                let Some(done) = self.done.get_mut(index - 1) else {
                    return false;
                };
                if *done {
                    return false;
                }
                if matches!(marker, Marker::Done(_)) {
                    *done = true;
                }
                true
            }
        }
    }

    /// EAC-FR-HRTK: scan the lines of one text block.
    ///
    /// Returns the markers that became activities, in line order, and the text
    /// that stays in the `message` activity (EAC-FR-HTSN).
    pub(super) fn scan(&mut self, text: &str) -> (Vec<Marker>, String) {
        let mut markers = Vec::new();
        let mut kept: Vec<&str> = Vec::new();
        for line in text.lines() {
            match parse_line(line).filter(|marker| self.accept(marker)) {
                Some(marker) => markers.push(marker),
                None => kept.push(line),
            }
        }
        (markers, kept.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // EAC-FR-PQLW, EAC-FR-XDGT, EAC-FR-VCBJ, EAC-FR-KRFS: valid markers.
    #[test]
    fn a_valid_marker_of_each_event_is_read() {
        let plan = parse_line(
            "SYNTHESIS_MILESTONE {\"event\":\"plan\",\"milestones\":[\"  Read   the spec \",\"Write\"]}\r",
        );
        assert_eq!(plan, Some(Marker::Plan(vec!["Read the spec".into(), "Write".into()])));
        assert_eq!(
            parse_line("SYNTHESIS_MILESTONE {\"event\":\"start\",\"index\":2}"),
            Some(Marker::Start(2))
        );
        assert_eq!(
            parse_line("SYNTHESIS_MILESTONE {\"event\":\"done\",\"index\":1}  "),
            Some(Marker::Done(1))
        );
    }

    // EAC-FR-PQLW, EAC-FR-XDGT, EAC-FR-VCBJ, EAC-FR-KRFS: malformed markers.
    #[test]
    fn a_malformed_marker_is_not_a_marker() {
        let long = "x".repeat(MAX_TITLE_CHARS + 1);
        let too_long =
            format!("SYNTHESIS_MILESTONE {{\"event\":\"plan\",\"milestones\":[\"{long}\"]}}");
        let bad = [
            "SYNTHESIS_MILESTONE not json",
            "  SYNTHESIS_MILESTONE {\"event\":\"start\",\"index\":1}",
            "SYNTHESIS_MILESTONE  {\"event\":\"start\",\"index\":1}",
            "SYNTHESIS_MILESTONE{\"event\":\"start\",\"index\":1}",
            "SYNTHESIS_MILESTONE {\"event\":\"start\",\"index\":1} trailing",
            "SYNTHESIS_MILESTONE {\"event\":\"start\",\"index\":1,\"extra\":true}",
            "SYNTHESIS_MILESTONE {\"event\":\"start\",\"index\":\"1\"}",
            "SYNTHESIS_MILESTONE {\"event\":\"start\",\"index\":1.5}",
            "SYNTHESIS_MILESTONE {\"event\":\"start\",\"index\":0}",
            "SYNTHESIS_MILESTONE {\"event\":\"start\",\"index\":-1}",
            "SYNTHESIS_MILESTONE {\"event\":\"start\"}",
            "SYNTHESIS_MILESTONE {\"event\":\"stop\",\"index\":1}",
            "SYNTHESIS_MILESTONE {\"event\":\"plan\",\"milestones\":[]}",
            "SYNTHESIS_MILESTONE {\"event\":\"plan\",\"milestones\":[\"a\",\"b\",\"c\",\"d\",\"e\",\"f\"]}",
            "SYNTHESIS_MILESTONE {\"event\":\"plan\",\"milestones\":[\"  \"]}",
            "SYNTHESIS_MILESTONE {\"event\":\"plan\",\"milestones\":[\"say \\\"hi\\\"\"]}",
            "SYNTHESIS_MILESTONE {\"event\":\"plan\",\"milestones\":[\"back\\\\slash\"]}",
            "SYNTHESIS_MILESTONE {\"event\":\"plan\",\"milestones\":[\"bell\\u0007\"]}",
            "SYNTHESIS_MILESTONE {\"event\":\"plan\",\"milestones\":[1]}",
            too_long.as_str(),
        ];
        for line in bad {
            assert_eq!(parse_line(line), None, "{line}");
        }
    }

    // EAC-FR-VCBJ: a plan of five longest titles fits the summary bound whole.
    #[test]
    fn a_full_plan_fits_the_summary_bound() {
        let title = "é".repeat(MAX_TITLE_CHARS);
        let plan = Marker::Plan(vec![title; MAX_TITLES]);
        let summary = plan.summary();
        assert!(summary.chars().count() <= super::super::descriptor::LIMIT_SUMMARY_LINE);
        assert_eq!(super::super::descriptor::summary_line(&summary), summary);
    }

    // EAC-FR-ZNBD: the summary is compact JSON with `event` first.
    #[test]
    fn the_summary_is_one_compact_json_line() {
        assert_eq!(
            Marker::Plan(vec!["A".into(), "B".into()]).summary(),
            "{\"event\":\"plan\",\"milestones\":[\"A\",\"B\"]}"
        );
        assert_eq!(Marker::Start(3).summary(), "{\"event\":\"start\",\"index\":3}");
        assert_eq!(Marker::Done(3).summary(), "{\"event\":\"done\",\"index\":3}");
    }

    // EAC-FR-QLMD, EAC-FR-BQNV, EAC-FR-HTSN: the sequence rules of one call.
    #[test]
    fn the_scan_applies_the_sequence_rules() {
        let mut state = MilestoneState::default();
        let early = "SYNTHESIS_MILESTONE {\"event\":\"start\",\"index\":1}";
        let (markers, rest) = state.scan(early);
        assert!(markers.is_empty(), "a start before a plan is ignored");
        assert_eq!(rest, early, "an ignored marker stays in the message");

        let plan = "SYNTHESIS_MILESTONE {\"event\":\"plan\",\"milestones\":[\"A\",\"B\"]}";
        let text = format!(
            "Hello\n{plan}\nSYNTHESIS_MILESTONE {{\"event\":\"plan\",\"milestones\":[\"C\"]}}\n\
             SYNTHESIS_MILESTONE {{\"event\":\"start\",\"index\":3}}\n\
             SYNTHESIS_MILESTONE {{\"event\":\"done\",\"index\":2}}\n\
             SYNTHESIS_MILESTONE {{\"event\":\"done\",\"index\":2}}\nBye"
        );
        let (markers, rest) = state.scan(&text);
        assert_eq!(markers, vec![Marker::Plan(vec!["A".into(), "B".into()]), Marker::Done(2)]);
        assert!(rest.starts_with("Hello\n") && rest.ends_with("\nBye"));
        assert!(!rest.contains("\"index\":1}"));
        assert!(rest.contains("\"milestones\":[\"C\"]"), "a later plan stays in the message");
    }

    // GRL-FR-DJPE, EAC-FR-PQLW: the example lines of the four instructions are valid markers.
    #[test]
    fn the_example_of_each_instruction_is_valid() {
        for source in [
            include_str!("../../../../resources/prompts/graduation/work.md"),
            include_str!("../../../../resources/prompts/graduation/review.md"),
            include_str!("../../../../resources/prompts/graduation/merge-work.md"),
            include_str!("../../../../resources/prompts/graduation/merge-review.md"),
        ] {
            let markers: Vec<Marker> = source.lines().filter_map(parse_line).collect();
            assert_eq!(markers.len(), 3);
            assert!(matches!(markers[0], Marker::Plan(ref titles) if titles.len() == 3));
            assert_eq!(markers[1], Marker::Start(1));
            assert_eq!(markers[2], Marker::Done(1));
        }
    }
}

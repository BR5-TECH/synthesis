//! The check of a new diagram source (`PDM-propose-diagram-changes-tool.md`
//! PDM-FR-HOQI, PDM-FR-CEZB).
//!
//! A source is valid when `merman`, the Rust Mermaid parser of the backend,
//! reads it as a diagram. The Editor renders with the JavaScript `mermaid`
//! library, so the two parsers can disagree about one source. The Editor
//! render decides what the author sees (PDM-FR-HOQI, EDT-FR-XTEB). The parser
//! runs in the backend, works offline, and runs no script from the source. A
//! parse has a size bound and a deadline, so a call never holds the turn.

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::Duration;

use merman::{OperationControl, Renderer};

/// PDM-FR-HOQI: the largest source the tool parses, in bytes.
pub const MAX_SOURCE_BYTES: usize = 256 * 1024;

/// PDM-FR-HOQI: the time one parse may take.
pub const PARSE_DEADLINE: Duration = Duration::from_secs(5);

/// PDM-FR-HOQI: the most characters of a parse error the model reads.
pub const ERROR_CHARS: usize = 500;

/// Why a new source is refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceFault {
    /// PDM-FR-HOQI: the source does not parse as Mermaid. Carries the parse
    /// error, cut to `ERROR_CHARS`.
    NotMermaid(String),
    /// PDM-FR-CEZB: a line of the source would end the code fence.
    FenceInSource,
}

/// PDM-FR-CEZB: whether a line starts with three or more backticks or tildes.
fn closes_a_fence(line: &str) -> bool {
    let trimmed = line.trim_start();
    ['`', '~'].iter().any(|mark| trimmed.chars().take_while(|c| c == mark).count() >= 3)
}

/// PDM-FR-HOQI, PDM-FR-CEZB: `Ok` for a source that is valid Mermaid and safe
/// to put in a fence, else the fault.
pub fn check_source(source: &str) -> Result<(), SourceFault> {
    check_source_within(source, PARSE_DEADLINE)
}

/// `check_source` with the deadline `limit`.
pub fn check_source_within(source: &str, limit: Duration) -> Result<(), SourceFault> {
    if source.trim().is_empty() {
        return Err(SourceFault::NotMermaid("The source is empty.".to_string()));
    }
    if source.len() > MAX_SOURCE_BYTES {
        return Err(SourceFault::NotMermaid("The source is too large.".to_string()));
    }
    if source.lines().any(closes_a_fence) {
        return Err(SourceFault::FenceInSource);
    }
    let control = OperationControl::new().with_deadline(limit);
    let outcome =
        catch_unwind(AssertUnwindSafe(|| Renderer::new().parse_semantic(source, control)));
    match outcome {
        Ok(Ok(Some(_))) => Ok(()),
        Ok(Ok(None)) => {
            Err(SourceFault::NotMermaid("The source does not hold a Mermaid diagram.".to_string()))
        }
        Ok(Err(error)) => Err(SourceFault::NotMermaid(bounded(&error.to_string()))),
        Err(_) => Err(SourceFault::NotMermaid("The parser stopped on this source.".to_string())),
    }
}

/// The parse error, on one line and cut to `ERROR_CHARS` by character.
fn bounded(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= ERROR_CHARS {
        return flat;
    }
    let cut: String = flat.chars().take(ERROR_CHARS).collect();
    format!("{cut}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    // PDM-FR-HOQI: a source that the parser reads is valid.
    #[test]
    fn a_valid_flowchart_passes() {
        assert_eq!(check_source("flowchart TD\n  A[Start] --> B[Done]"), Ok(()));
    }

    // PDM-FR-HOQI: sources that the Editor renders as diagrams are not refused.
    // They cover the chart types of the Editor hint sets, with labels, quoted
    // text, a comment line, and a front matter block.
    #[test]
    fn sources_that_the_editor_renders_are_not_refused() {
        let sources = [
            "flowchart LR\n  A[\"Quoted: text\"] -->|yes| B(Round)\n  B --> C{Choice}\n  %% comment\n  C -.-> D((End))",
            "---\ntitle: Flow\n---\nflowchart TD\n  subgraph one [Group]\n    A --> B\n  end",
            "sequenceDiagram\n  participant A as Alice\n  A->>B: Hello\n  B-->>A: Reply\n  Note over A,B: Done",
            "classDiagram\n  class Animal {\n    +String name\n    +eat()\n  }\n  Animal <|-- Dog",
            "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : start\n  Busy --> [*]",
            "erDiagram\n  CUSTOMER ||--o{ ORDER : places",
            "gantt\n  title Plan\n  dateFormat YYYY-MM-DD\n  section One\n  Task :a1, 2026-01-01, 3d",
            "pie title Share\n  \"A\" : 40\n  \"B\" : 60",
            "mindmap\n  root((Idea))\n    Branch\n      Leaf",
        ];
        for source in sources {
            assert_eq!(
                check_source(source),
                Ok(()),
                "refused a source that the Editor renders: {source}"
            );
        }
    }

    // PDM-FR-HOQI: an empty source and a source that is not a diagram are refused.
    #[test]
    fn an_empty_source_and_plain_text_are_refused() {
        assert!(matches!(check_source("  \n"), Err(SourceFault::NotMermaid(_))));
        assert!(matches!(check_source("this is not a diagram"), Err(SourceFault::NotMermaid(_))));
    }

    // PDM-FR-HOQI: a broken diagram is refused with a bounded parse error.
    #[test]
    fn a_broken_diagram_is_refused_with_a_bounded_error() {
        match check_source("sequenceDiagram\n  A->>: oops ->>") {
            Err(SourceFault::NotMermaid(message)) => {
                assert!(!message.is_empty());
                assert!(message.chars().count() <= ERROR_CHARS + 1);
            }
            other => panic!("expected a parse error, got {other:?}"),
        }
    }

    // PDM-FR-CEZB: a line that would end the fence is refused.
    #[test]
    fn a_fence_line_in_the_source_is_refused() {
        let source = "flowchart TD\n  A --> B\n```\n";
        assert_eq!(check_source(source), Err(SourceFault::FenceInSource));
        assert_eq!(check_source("flowchart TD\n~~~"), Err(SourceFault::FenceInSource));
    }

    // PDM-FR-HOQI: a source over the size bound is refused before it is parsed.
    #[test]
    fn a_source_over_the_bound_is_refused() {
        let big = format!("flowchart TD\n{}", "  A --> B\n".repeat(MAX_SOURCE_BYTES / 8));
        assert!(matches!(check_source(&big), Err(SourceFault::NotMermaid(_))));
    }
}

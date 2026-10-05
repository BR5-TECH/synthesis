//! Tests for `SPS-specification-search-tool.md`.
//!
//! The group-wide conventions this tool shares with the other four are checked
//! once in `tools/tests.rs`; what follows is what is true of this tool alone.

use rig::tool::{PortableTool, ToolErrorKind};

use tempfile::TempDir;
use tauri::Manager;

use super::*;
use crate::bm25_index::Bm25Indexer;
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::tests::{block_on, closed_project, mounted, sorted_keys, Fixture};

mod contract;
mod delegation;
mod freshness;
mod indexes;
mod limits;
mod log_records;
mod match_shape;
mod refusals;


// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Write a Markdown file under `root`, creating parents.
fn write(root: &std::path::Path, rel: &str, body: &str) {
    let path = root.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

/// Every record `buffer` holds under `domain`.
fn records_of(buffer: &LogBuffer, domain: Domain) -> Vec<crate::logging::LogRecord> {
    buffer
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![domain],
                ..LogFilter::default()
            },
            None,
            1000,
        )
        .unwrap()
        .records
}

/// Everything `buffer` holds, rendered, for the leak scan.
fn rendered_records(buffer: &LogBuffer) -> String {
    serde_json::to_string(
        &buffer
            .query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 1000)
            .unwrap()
            .records,
    )
    .unwrap()
}

/// A spec-shaped document: several heading sections, so a file yields several
/// chunks exactly as BMI-FR-05 cuts one.
fn spec_body(topic: &str, sections: usize) -> String {
    let mut out = format!("# {topic}\n\npreamble about {topic}\n");
    for i in 0..sections {
        out.push_str(&format!(
            "\n## Section {i} of {topic}\n\nThis section discusses {topic} in detail, covering {topic} thoroughly.\n"
        ));
    }
    out
}

/// A section that mentions `teardown` `weight` times, padded so the files
/// differ in density rather than only in length.
fn section(index: usize, weight: usize) -> String {
    let mentions = std::iter::repeat("teardown")
        .take(weight)
        .collect::<Vec<_>>()
        .join(" ");
    format!("\n## Section {index}\n\nThis section covers {mentions} and related matters.\n")
}

/// A project whose `specifications/` tree holds several multi-section specs,
/// every one of them matching `teardown` and to differing degrees — which is
/// what lets an ordering assertion mean anything.
fn spec_project() -> Fixture {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let mut heavy = String::from("# teardown\n\npreamble about teardown\n");
    for i in 0..4 {
        heavy.push_str(&section(i, 6));
    }
    write(root, "specifications/core/teardown.md", &heavy);

    let mut middling = String::from("# palette\n\npreamble about palette\n");
    for i in 0..2 {
        middling.push_str(&section(i, 2));
    }
    write(root, "specifications/ui/palette.md", &middling);

    let mut light = String::from("# indexing\n\npreamble about indexing\n");
    for i in 0..2 {
        light.push_str(&section(i, 1));
    }
    write(root, "specifications/core/indexing.md", &light);
    mounted(dir)
}

fn query(fixture: &Fixture, text: &str, limit: Option<i64>) -> SpecificationSearchOutput {
    let indexer = fixture.app.state::<Bm25Indexer>();
    search(
        &indexer,
        &SpecificationSearchArgs {
            query: text.to_string(),
            limit,
        },
    )
    .expect("the query succeeds")
}

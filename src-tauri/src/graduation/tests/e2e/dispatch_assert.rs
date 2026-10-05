//! The assertions one recorded dispatch reads (GTE-FR-XKGB).
//!
//! Split out of `scenario.rs`, which had reached a thousand lines. What a
//! scenario chain says and what it asserts are unchanged; the language's own
//! entry point stays [`super::scenario::Scenario`].

use std::path::{Path, PathBuf};

use super::super::Seen;
use crate::tools::agent_exec::TurnKind;

/// One recorded dispatch, with the assertions that read it (GTE-FR-XKGB).
pub(crate) struct DispatchAssert {
    pub(super) scenario: &'static str,
    pub(super) index: usize,
    pub(super) seen: Seen,
}

impl DispatchAssert {
    pub(crate) fn part(self, part: &str) -> Self {
        assert_eq!(
            self.seen.part, part,
            "[{}] dispatch {} is the {part} turn",
            self.scenario, self.index
        );
        self
    }

    pub(crate) fn purpose(self, purpose: &str) -> Self {
        assert_eq!(
            self.seen.purpose, purpose,
            "[{}] dispatch {} purpose",
            self.scenario, self.index
        );
        self
    }

    pub(crate) fn pass(self, pass: u32) -> Self {
        assert_eq!(
            self.seen.pass, pass,
            "[{}] dispatch {} pass",
            self.scenario, self.index
        );
        self
    }

    pub(crate) fn turn_kind(self, kind: TurnKind) -> Self {
        assert_eq!(
            self.seen.turn_kind, kind,
            "[{}] dispatch {} turn kind",
            self.scenario, self.index
        );
        self
    }

    pub(crate) fn directory(self, directory: &PathBuf) -> Self {
        assert_eq!(
            &self.seen.directory, directory,
            "[{}] dispatch {} execution directory",
            self.scenario, self.index
        );
        self
    }

    /// GRL-FR-DXLU / GRL-FR-OTRH: the instruction the turn was handed, whole.
    pub(crate) fn instruction(self, expected: &str) -> Self {
        assert_eq!(
            self.seen.instruction, expected,
            "[{}] dispatch {} instruction",
            self.scenario, self.index
        );
        self
    }

    /// GRL-FR-YKRI: the turn stood somewhere other than the stream's working
    /// copy — which is the whole of what makes a review a review.
    pub(crate) fn directory_is_not(self, directory: &Path) -> Self {
        assert_ne!(
            self.seen.directory.as_path(),
            directory,
            "[{}] dispatch {} stood outside the stream",
            self.scenario,
            self.index
        );
        self
    }

    /// GXD-FR-ODGX: one field of the structured task input.
    pub(crate) fn field(self, key: &str, value: &str) -> Self {
        assert_eq!(
            self.seen.instruction_field(key).as_deref(),
            Some(value),
            "[{}] dispatch {} carries {key}",
            self.scenario,
            self.index
        );
        self
    }

    pub(crate) fn field_contains(self, key: &str, fragment: &str) -> Self {
        let held = self.seen.instruction_field(key).unwrap_or_default();
        assert!(
            held.contains(fragment),
            "[{}] dispatch {} carries {key} holding {fragment:?}, but it holds {held:?}",
            self.scenario,
            self.index
        );
        self
    }

    pub(crate) fn has_no_field(self, key: &str) -> Self {
        assert!(
            self.seen.instruction_field(key).is_none(),
            "[{}] dispatch {} carries no {key}",
            self.scenario,
            self.index
        );
        self
    }

    /// GRB-FR-KMXT: the answers the author's decisions carry into a semantic
    /// turn, in order.
    pub(crate) fn decision_answers(self, answers: &[&str]) -> Self {
        let held: Vec<String> = self
            .seen
            .input
            .get("author_decisions")
            .and_then(|v| v.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|entry| entry.get("answer").and_then(|v| v.as_str()).map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            held,
            answers.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            "[{}] dispatch {} carries the author's decisions in order",
            self.scenario, self.index
        );
        self
    }

    /// How many entries one list field of the task input holds.
    pub(crate) fn list_len(self, key: &str, count: usize) -> Self {
        assert_eq!(
            self.seen.list_len(key),
            count,
            "[{}] dispatch {} carries {count} entries in {key}",
            self.scenario, self.index
        );
        self
    }

    /// GXD-FR-BJYT: the answers delivered into this turn, in order.
    pub(crate) fn escalation_answers(self, answers: &[&str]) -> Self {
        let held: Vec<String> = self
            .seen
            .input
            .get("escalation_answers")
            .and_then(|v| v.as_array())
            .map(|list| {
                list.iter()
                    .filter_map(|entry| {
                        entry
                            .get("answer")
                            .and_then(|v| v.as_str())
                            .map(str::to_string)
                    })
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(
            held,
            answers.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
            "[{}] dispatch {} carries the author's answers in order",
            self.scenario,
            self.index
        );
        self
    }

    /// GXD-FR-MTVR: the execution timeout this dispatch was composed under.
    pub(crate) fn execution_timeout_ms(self, expected: u64) -> Self {
        assert_eq!(
            self.seen.timeout_ms, expected,
            "[{}] dispatch {} execution timeout",
            self.scenario, self.index
        );
        self
    }

    /// GXD-FR-XPUR: the vendor session this dispatch asked to resume.
    pub(crate) fn resumes_session(self, session_id: &str) -> Self {
        assert_eq!(
            self.seen.resume_session.as_deref(),
            Some(session_id),
            "[{}] dispatch {} resumes the session",
            self.scenario, self.index
        );
        self
    }

    /// GXD-FR-XPUR / GRL-FR-OTRH: this dispatch resumes no earlier session.
    pub(crate) fn resumes_no_session(self) -> Self {
        assert_eq!(
            self.seen.resume_session, None,
            "[{}] dispatch {} resumes no session",
            self.scenario, self.index
        );
        self
    }

    /// GRL-FR-CKBL: what the ignore rules kept out, as this turn was told.
    pub(crate) fn hidden_paths(self, paths: &[&str]) -> Self {
        let mut held = self.seen.path_list("hidden_paths");
        held.sort();
        let mut expected: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        expected.sort();
        assert_eq!(
            held, expected,
            "[{}] dispatch {} hidden paths",
            self.scenario, self.index
        );
        self
    }

    /// GXD-FR-XQJR / GRB-FR-TOOU: a semantic turn, with its artifact mounted.
    pub(crate) fn semantic_turn(self) -> Self {
        assert_eq!(
            self.seen.turn_kind,
            TurnKind::SemanticRebase,
            "[{}] dispatch {} is a semantic turn",
            self.scenario, self.index
        );
        assert!(
            self.seen.semantic_mount,
            "[{}] dispatch {} carries the semantic-rebase mount",
            self.scenario, self.index
        );
        self
    }

    /// The paths the working copy held against the base commit when this turn
    /// started (GRL-FR-NVBZ: read from the copy, never from an envelope).
    pub(crate) fn changed_paths(self, paths: &[&str]) -> Self {
        let mut held = self.seen.path_list("changed_paths");
        held.sort();
        let mut expected: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        expected.sort();
        assert_eq!(
            held, expected,
            "[{}] dispatch {} changed paths",
            self.scenario, self.index
        );
        self
    }

    // -- the merge context of a merge run's turns (GTE-FR-RDPE) -------------------

    fn merge_value(&self, key: &str) -> Option<serde_json::Value> {
        self.seen.input.get("merge").and_then(|merge| merge.get(key)).cloned()
    }

    /// GRL-FR-TXEB: one scalar of the task's `merge` context.
    pub(crate) fn merge_field(self, key: &str, value: &str) -> Self {
        assert_eq!(
            self.merge_value(key).as_ref().and_then(|v| v.as_str()),
            Some(value),
            "[{}] dispatch {} carries merge.{key}",
            self.scenario,
            self.index
        );
        self
    }

    /// GRL-FR-TXEB: one list of paths of the task's `merge` context.
    pub(crate) fn merge_paths(self, key: &str, paths: &[&str]) -> Self {
        let mut held: Vec<String> = self
            .merge_value(key)
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect();
        held.sort();
        let mut expected: Vec<String> = paths.iter().map(|p| p.to_string()).collect();
        expected.sort();
        assert_eq!(
            held, expected,
            "[{}] dispatch {} carries merge.{key}",
            self.scenario, self.index
        );
        self
    }

    /// GRL-FR-TXEB: what each side did to one unresolved path.
    pub(crate) fn merge_conflict(self, path: &str, base_change: &str, stream_change: &str) -> Self {
        let held = self
            .merge_value("conflicts")
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default();
        let found = held
            .iter()
            .find(|entry| entry.get("path").and_then(|v| v.as_str()) == Some(path))
            .unwrap_or_else(|| {
                panic!(
                    "[{}] dispatch {} carries a conflict for {path}",
                    self.scenario, self.index
                )
            });
        assert_eq!(
            (
                found.get("base_change").and_then(|v| v.as_str()),
                found.get("stream_change").and_then(|v| v.as_str())
            ),
            (Some(base_change), Some(stream_change)),
            "[{}] dispatch {} carries what each side did to {path}",
            self.scenario,
            self.index
        );
        self
    }

    /// GRL-FR-TXEB: a merge work turn carries no `reconciled_paths`, and an
    /// ordinary turn carries no `merge` context at all.
    pub(crate) fn merge_has_no(self, key: &str) -> Self {
        assert!(
            self.merge_value(key).is_none(),
            "[{}] dispatch {} carries no merge.{key}",
            self.scenario,
            self.index
        );
        self
    }

    pub(crate) fn has_no_merge(self) -> Self {
        assert!(
            self.seen.input.get("merge").is_none(),
            "[{}] dispatch {} carries no merge context",
            self.scenario,
            self.index
        );
        self
    }

    /// GRL-FR-GADT: the instruction is the merge work instruction, whole, and is
    /// none of the other three.
    pub(crate) fn merge_work_instruction(self) -> Self {
        let expected = crate::graduation::driver::prompts::MERGE_WORK_PROMPT.as_str();
        self.merge_instruction(expected)
    }

    /// GRL-FR-GADT: the instruction is the merge review instruction, whole, and
    /// is none of the other three.
    pub(crate) fn merge_review_instruction(self) -> Self {
        let expected = crate::graduation::driver::prompts::MERGE_REVIEW_PROMPT.as_str();
        self.merge_instruction(expected)
    }

    fn merge_instruction(self, expected: &str) -> Self {
        use crate::graduation::driver::prompts as p;
        assert_eq!(
            self.seen.instruction, expected,
            "[{}] dispatch {} instruction",
            self.scenario, self.index
        );
        for other in [p::WORK_PROMPT.as_str(), p::REVIEW_PROMPT.as_str(), p::REBASE_PROMPT.as_str()] {
            assert_ne!(
                self.seen.instruction, other,
                "[{}] dispatch {} carries an instruction a merge run never compiles",
                self.scenario, self.index
            );
        }
        self
    }

    /// GXD-FR-MKTZ: neither merge turn carries a supplementary mount.
    pub(crate) fn no_supplementary_mount(self) -> Self {
        assert!(
            !self.seen.semantic_mount,
            "[{}] dispatch {} carries no supplementary mount",
            self.scenario, self.index
        );
        self
    }

    /// GTE-FR-RDPE: what the execution directory held at one path when this turn
    /// started, so a resumed merge worktree is read to hold its earlier edits.
    pub(crate) fn directory_held(self, path: &str, content: &str) -> Self {
        assert_eq!(
            self.seen.tree.get(path).map(String::as_str),
            Some(content),
            "[{}] dispatch {} found {path} in its execution directory",
            self.scenario,
            self.index
        );
        self
    }
}

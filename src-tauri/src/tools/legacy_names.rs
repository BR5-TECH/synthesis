//! The legacy names of the agent tools (`PVC-product-vocabulary.md`
//! PVC-FR-LGYT, PVC-FR-PXOE, and its **Legacy names** table).
//!
//! The agent tools carry the names of the product vocabulary (PVC-FR-RWDM). A
//! call that an earlier build stored carries the name that the tool had then.
//! Where such a call is read again, it is read as a call of the tool's current
//! name, so that a replay offers it to the model as a call of a tool the turn
//! has, and not as text.
//!
//! Three earlier OpenSpec names are now the current names of the change
//! tools. So the name alone does not tell which tool an old call used: the
//! target kind of its conversation tells it (PVC-FR-PXOE). A conversation on an
//! OpenSpec change never has the change tools of PVC-FR-HMJD, so on that
//! target kind the old OpenSpec names map to the `openspec` names, and on every
//! other target kind they keep their names.
//!
//! The function here is pure. It reads no state and writes no log record.

use crate::comments::DiscussionTarget;

/// The target kind of the conversation that holds a stored call, as the
/// **Legacy names** table tells them apart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallTarget {
    /// A conversation whose target is an OpenSpec change
    /// (`DiscussionTarget::Change`, CVL-FR-EZKH).
    OpenSpecChange,
    /// A conversation on any other target.
    Other,
}

impl CallTarget {
    /// The target kind of a conversation on `target`.
    pub fn of_discussion(target: &DiscussionTarget) -> Self {
        match target {
            DiscussionTarget::Change { .. } => CallTarget::OpenSpecChange,
            _ => CallTarget::Other,
        }
    }
}

/// PVC-FR-LGYT: the rows of the **Legacy names** table that map on every
/// target kind, as `(legacy name, current name)`. The five draft-tool names
/// are these rows. `src/state/legacyToolNames.ts` holds the same rows.
pub const EVERY_KIND: &[(&str, &str)] = &[
    ("search_drafts", super::draft_search::NAME),
    ("read_draft", super::draft_read::NAME),
    ("propose_draft_changes", super::propose_draft_changes::NAME),
    ("decide_draft_changes", super::decide_draft_changes::NAME),
    ("propose_new_draft", super::propose_new_draft::NAME),
];

/// PVC-FR-PXOE: the rows of the **Legacy names** table that map only on the
/// target kind `change`, as `(legacy name, current name)`.
/// `src/state/legacyToolNames.ts` holds the same rows.
pub const OPENSPEC_CHANGE: &[(&str, &str)] = &[
    ("propose_change_edits", super::propose_change_edits::NAME),
    ("decide_change_edits", super::decide_change_edits::NAME),
    ("propose_new_change", super::propose_new_change::NAME),
    ("request_change_action", super::request_change_action::NAME),
];

/// PVC-FR-LGYT, PVC-FR-PXOE: the current name of a stored call of `name` in a
/// conversation of `target`. A name that is not a legacy name for that target
/// kind is returned as it is.
pub fn current_name(name: &str, target: CallTarget) -> &str {
    let row = |rows: &[(&'static str, &'static str)]| {
        rows.iter().find(|(legacy, _)| *legacy == name).map(|(_, current)| *current)
    };
    if let Some(current) = row(EVERY_KIND) {
        return current;
    }
    match target {
        CallTarget::OpenSpecChange => row(OPENSPEC_CHANGE).unwrap_or(name),
        CallTarget::Other => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every target kind of the table.
    const ALL: [CallTarget; 2] = [CallTarget::OpenSpecChange, CallTarget::Other];

    // PVC-FR-LGYT: each of the five draft-tool rows maps on every target kind.
    #[test]
    fn each_draft_tool_row_maps_on_every_target_kind() {
        let rows = [
            ("search_drafts", "search_changes"),
            ("read_draft", "read_change"),
            ("propose_draft_changes", "propose_change_edits"),
            ("decide_draft_changes", "decide_change_edits"),
            ("propose_new_draft", "propose_new_change"),
        ];
        for target in ALL {
            for (legacy, current) in rows {
                assert_eq!(current_name(legacy, target), current, "{legacy} on {target:?}");
            }
        }
    }

    // PVC-FR-LGYT, PVC-FR-PXOE: the four old OpenSpec names map on an OpenSpec
    // change target.
    #[test]
    fn each_openspec_row_maps_on_an_openspec_change() {
        let rows = [
            ("propose_change_edits", "propose_openspec_change_edits"),
            ("decide_change_edits", "decide_openspec_change_edits"),
            ("propose_new_change", "propose_new_openspec_change"),
            ("request_change_action", "request_openspec_change_action"),
        ];
        for (legacy, current) in rows {
            assert_eq!(current_name(legacy, CallTarget::OpenSpecChange), current, "{legacy}");
        }
    }

    // PVC-FR-PXOE: on any other target kind, an old OpenSpec name keeps its
    // name, which is now the current name of a change tool.
    #[test]
    fn an_old_openspec_name_on_another_target_keeps_its_name() {
        let names = [
            "propose_change_edits",
            "decide_change_edits",
            "propose_new_change",
            "request_change_action",
        ];
        for target in [CallTarget::Other] {
            for name in names {
                assert_eq!(current_name(name, target), name, "{name} on {target:?}");
            }
        }
    }

    // PVC-FR-PXOE: a name that is not in the table, and a current name, keep
    // their names on every target kind.
    #[test]
    fn an_unknown_name_and_a_current_name_keep_their_names() {
        let names = [
            "read_file",
            "search_notes",
            "mcp__server__tool",
            "",
            "Read_Draft",
            "search_changes",
            "read_change",
            "read_implementation_file",
            "read_graduation_file",
            "propose_openspec_change_edits",
            "decide_openspec_change_edits",
            "propose_new_openspec_change",
            "request_openspec_change_action",
        ];
        for target in ALL {
            for name in names {
                assert_eq!(current_name(name, target), name, "{name} on {target:?}");
            }
        }
    }

    // PVC-FR-PXOE: an OpenSpec change discussion is the `change` target kind,
    // and every other discussion target is not.
    #[test]
    fn the_target_kind_of_a_discussion_comes_from_its_target() {
        let change = DiscussionTarget::Change { stream_id: "s".into(), change: "c".into() };
        assert_eq!(CallTarget::of_discussion(&change), CallTarget::OpenSpecChange);
        let others = [
            DiscussionTarget::Draft { draft_id: "d".into() },
            DiscussionTarget::Artifact { artifact_id: "a".into() },
            DiscussionTarget::Note { note_id: "n".into() },
            DiscussionTarget::Map,
        ];
        for target in others {
            assert_eq!(CallTarget::of_discussion(&target), CallTarget::Other, "{target:?}");
        }
    }

    /// The rows of the map `constant` of `src/state/legacyToolNames.ts`, in
    /// their order. Each row is `["legacy name", "current name"],`.
    fn frontend_rows(source: &str, constant: &str) -> Vec<(String, String)> {
        let head = format!("const {constant}: ReadonlyMap<string, string> = new Map([");
        let start = source.find(&head).unwrap_or_else(|| panic!("no map {constant}"));
        let body = &source[start + head.len()..];
        let body = &body[..body.find("]);").unwrap_or_else(|| panic!("{constant} is not closed"))];
        body.lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(|line| {
                let names: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
                assert_eq!(names.len(), 2, "a row of {constant} is not two names: {line}");
                (names[0].to_string(), names[1].to_string())
            })
            .collect()
    }

    fn owned(rows: &[(&str, &str)]) -> Vec<(String, String)> {
        rows.iter().map(|(legacy, current)| (legacy.to_string(), current.to_string())).collect()
    }

    // PVC-FR-LGYT, PVC-FR-PXOE: the backend and the frontend read a stored call
    // by the same table. The rows of each map of the frontend are the rows of
    // this module, and each row maps here on exactly the target kinds that the
    // frontend gives it.
    #[test]
    fn the_frontend_table_holds_the_same_rows() {
        const SOURCE: &str = include_str!("../../../src/state/legacyToolNames.ts");
        let every_kind = frontend_rows(SOURCE, "EVERY_KIND");
        let openspec_change = frontend_rows(SOURCE, "OPENSPEC_CHANGE");
        assert_eq!(every_kind, owned(EVERY_KIND));
        assert_eq!(openspec_change, owned(OPENSPEC_CHANGE));
        for (legacy, current) in &every_kind {
            for target in ALL {
                assert_eq!(
                    current_name(legacy, target),
                    current.as_str(),
                    "{legacy} on {target:?}"
                );
            }
        }
        for (legacy, current) in &openspec_change {
            assert_eq!(
                current_name(legacy, CallTarget::OpenSpecChange),
                current.as_str(),
                "{legacy}"
            );
            assert_eq!(current_name(legacy, CallTarget::Other), legacy.as_str(), "{legacy}");
        }
    }
}

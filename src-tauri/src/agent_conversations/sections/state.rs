//! The `discussion_state` section (`AGC-agent-conversations.md` AGC-FR-06,
//! AGC-FR-YQVB).
//!
//! The section follows `current_comment` and holds what changes from one turn
//! to the next: the decomposition, the action requests, the split, the open
//! proposal, the question set, and on a Map or a Flow their volatile parts
//! (AGC-FR-UOPJ, AGC-FR-KQNF, MPR-FR-EHCG). The material and the history before
//! it then stay the same from one turn to the next, which is the prefix a
//! provider cache reuses (CVL-FR-23, ERP-FR-IVGL).

use super::*;

/// AGC-FR-06: a `discussion_state` section that holds `body`. A builder adds no
/// section where the state is empty, so this is never called with an empty body
/// by a builder.
pub(in crate::agent_conversations) fn state_section(body: String, truncated: bool) -> InputSection {
    InputSection {
        tag: TAG_DISCUSSION_STATE.to_string(),
        attributes: Vec::new(),
        parts: Vec::new(),
        body,
        truncated,
    }
}

/// The place of a new `discussion_state` section: directly after
/// `current_comment`, else at the end.
fn state_position(sections: &[InputSection]) -> usize {
    sections.iter().position(|s| s.tag == TAG_CURRENT_COMMENT).map_or(sections.len(), |at| at + 1)
}

/// AGC-FR-06: add `text` to the end of the `discussion_state` section, and add
/// the section after `current_comment` where it is not there. Empty text adds
/// nothing.
pub(in crate::agent_conversations) fn append_state_text(
    sections: &mut Vec<InputSection>,
    text: &str,
) {
    if text.is_empty() {
        return;
    }
    if let Some(state) = sections.iter_mut().find(|s| s.tag == TAG_DISCUSSION_STATE) {
        state.body.push_str(text);
        return;
    }
    let at = state_position(sections);
    sections.insert(at, state_section(text.to_string(), false));
}

/// AGC-FR-06: add `text` to the start of the `discussion_state` section, and
/// add the section where it is not there. Empty text adds nothing.
pub(in crate::agent_conversations) fn prepend_state_text(
    sections: &mut Vec<InputSection>,
    text: &str,
) {
    if text.is_empty() {
        return;
    }
    if let Some(state) = sections.iter_mut().find(|s| s.tag == TAG_DISCUSSION_STATE) {
        state.body.insert_str(0, text);
        return;
    }
    let at = state_position(sections);
    sections.insert(at, state_section(text.to_string(), false));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tagged(tag: &str) -> InputSection {
        InputSection { tag: tag.to_string(), ..InputSection::default() }
    }

    /// AGC-FR-06: the state section goes after `current_comment` and before
    /// the sections that follow it, and text is added at its two ends.
    #[test]
    fn the_state_goes_after_the_current_comment() {
        let mut sections = vec![
            tagged(TAG_ARTIFACT),
            tagged(TAG_DISCUSSION_HISTORY),
            tagged(TAG_CURRENT_COMMENT),
            tagged("synthesis"),
        ];
        append_state_text(&mut sections, "middle\n");
        append_state_text(&mut sections, "end\n");
        prepend_state_text(&mut sections, "start\n");
        let tags: Vec<&str> = sections.iter().map(|s| s.tag.as_str()).collect();
        assert_eq!(
            tags,
            [
                TAG_ARTIFACT,
                TAG_DISCUSSION_HISTORY,
                TAG_CURRENT_COMMENT,
                TAG_DISCUSSION_STATE,
                "synthesis"
            ]
        );
        assert_eq!(sections[3].body, "start\nmiddle\nend\n");
    }

    /// AGC-FR-06: empty text adds no section.
    #[test]
    fn empty_text_adds_no_state() {
        let mut sections = vec![tagged(TAG_CURRENT_COMMENT)];
        append_state_text(&mut sections, "");
        prepend_state_text(&mut sections, "");
        assert_eq!(sections.len(), 1);
    }
}

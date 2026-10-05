//! Tests for containment, errors, and the two proposal modules (PCP-FR-25 … PCP-FR-29).

use super::*;

/// The whole module's source: the root file and the parts it was split into.
fn module_source() -> String {
    [
        include_str!("../../prompt_proposals.rs"),
        include_str!("../model.rs"),
        include_str!("../reconcile.rs"),
        include_str!("../commands.rs"),
    ]
    .join("\n")
}


// ---------------------------------------------------------------------------
// Containment, errors, and the two proposal modules (PCP-FR-25 … PCP-FR-29)
// ---------------------------------------------------------------------------

#[test]
fn a_crafted_proposal_id_reaches_nothing_outside_the_folder() {
    // PCP-FR-25, FSA-FR-10.
    let f = Fixture::new();
    for crafted in ["../library", "a/b", "..", "with space", ""] {
        assert!(
            !is_valid_proposal_id(crafted),
            "{crafted:?} is not a proposal id",
        );
        assert_eq!(
            read_proposal(&f.root, crafted).unwrap_err(),
            ERR_PROPOSAL_NOT_FOUND,
        );
        assert_eq!(
            load_content_impl(&f.root, crafted).unwrap_err(),
            ERR_PROPOSAL_NOT_FOUND,
        );
        assert_eq!(
            save_candidate_impl(&f.root, crafted, "x", "y").unwrap_err(),
            ERR_PROPOSAL_NOT_FOUND,
        );
    }
    // The module writes nothing outside its own folder, the artifact bytes an
    // acceptance lands excepted.
    let module = module_source();
    let module_text = module.as_str();
    for raw in ["fs::write", "fs::read", "fs::remove", "fs::rename"] {
        assert!(
            !module_text.contains(&format!("std::{raw}")),
            "this module performs no raw {raw} call",
        );
    }
}

#[test]
fn a_command_names_a_proposal_the_worktree_does_not_hold() {
    // PCP-FR-26.
    let f = Fixture::new();
    assert_eq!(
        read_proposal(&f.root, "nosuchproposal").unwrap_err(),
        ERR_PROPOSAL_NOT_FOUND,
    );
    assert!(
        list_proposals_impl(&f.root, "prompts/nothing.md").is_empty(),
        "an artifact with no proposals lists none",
    );
}

#[test]
fn the_two_proposal_modules_share_nothing() {
    // PCP-FR-04 / PCP-FR-29.
    assert_ne!(PROMPT_PROPOSALS_CHANGED, crate::draft_proposals::PROPOSALS_CHANGED);
    // Neither module *calls* into the other: a mention in a comment is how each
    // says so, and a path expression is what would make it false.
    let module = module_source();
    let module_text = module.as_str();
    for reached in ["draft_proposals::", "draft_history::", "drafts::"] {
        assert!(
            !module_text.contains(reached),
            "no operation of this module reaches {reached}",
        );
    }
    const OTHER: &str = include_str!("../../draft_proposals.rs");
    assert!(
        !OTHER.contains("prompt_proposals"),
        "and the reverse holds too",
    );
    // They hold their records in different folders and use different attachment
    // kinds.
    assert_ne!(PROPOSALS_REL, ".synthesis/drafts");
    assert!(module_text.contains("Attachment::PromptProposal"));
    assert!(!module_text.contains("Attachment::Proposal {"));
}


//! Active drafts (PST-FR-32).
//!
//! One part of `../tests/mod.rs`.

use super::*;

// ------------------------------------------------------------------
// Active drafts (PST-FR-32)
// ------------------------------------------------------------------

fn draft(id: &str, name: &str, status: DraftStatus, activity: Option<&str>) -> DraftSummary {
    DraftSummary {
        id: id.to_string(),
        name: name.to_string(),
        status,
        folder: String::new(),
        inconsistent: false,
        updated_at: "2026-01-01T00:00:00Z".to_string(),
        prompt_activity_at: activity.map(|s| s.to_string()),
        build: crate::drafts::BuildState::Idle,
        has_pending_proposal: false,
        graduation: None,
        github_issue: None,
    }
}

#[test]
fn active_drafts_keeps_only_active_ones_ordered_by_prompt_activity() {
    // PST-FR-32: seven active, two archived, one graduated -> five active
    // items, prompt-activity descending.
    let mut drafts: Vec<DraftSummary> = (0..7)
        .map(|i| {
            draft(
                &format!("d{i}"),
                &format!("draft {i}"),
                DraftStatus::Active,
                Some(&format!("2026-01-0{}T00:00:00.000Z", i + 1)),
            )
        })
        .collect();
    drafts.push(draft("z1", "archived", DraftStatus::Archived, Some("2026-02-01T00:00:00.000Z")));
    drafts.push(draft("z2", "archived 2", DraftStatus::Archived, Some("2026-02-01T00:00:00.000Z")));
    drafts.push(draft("z3", "graduated", DraftStatus::Graduated, Some("2026-02-01T00:00:00.000Z")));

    let items = active_drafts_from(drafts);
    assert_eq!(items.len(), DASHBOARD_ITEM_CAP);
    assert!(
        items.iter().all(|i| i.status == DraftStatus::Active),
        "an archived or graduated draft never appears"
    );
    assert_eq!(
        items.iter().map(|i| i.draft_id.as_str()).collect::<Vec<_>>(),
        vec!["d6", "d5", "d4", "d3", "d2"],
        "most recent prompt activity first"
    );
}

#[test]
fn active_drafts_omits_a_draft_with_no_prompt_to_stat() {
    // PST-FR-32 / DRS-FR-15: an inconsistent draft is omitted and the rest
    // are still returned.
    let mut broken = draft("d2", "broken", DraftStatus::Active, None);
    broken.inconsistent = true;
    let items = active_drafts_from(vec![
        draft("d1", "good", DraftStatus::Active, Some("2026-01-01T00:00:00.000Z")),
        broken,
    ]);
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].draft_id, "d1");
}

#[test]
fn active_drafts_break_a_tie_on_the_draft_id_ascending() {
    let items = active_drafts_from(vec![
        draft("b", "b", DraftStatus::Active, Some("2026-01-01T00:00:00.000Z")),
        draft("a", "a", DraftStatus::Active, Some("2026-01-01T00:00:00.000Z")),
    ]);
    assert_eq!(
        items.iter().map(|i| i.draft_id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b"]
    );
}

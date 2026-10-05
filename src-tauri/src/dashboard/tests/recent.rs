//! Recently edited (PST-FR-31).
//!
//! One part of `../tests/mod.rs`.

use super::*;

// ------------------------------------------------------------------
// Recently edited (PST-FR-31)
// ------------------------------------------------------------------

#[test]
fn recently_edited_orders_by_modification_time_and_caps_at_five() {
    // PST-FR-31: eight artifacts in, exactly five out, most-recent first.
    let readings: Vec<_> = (0..8)
        .map(|i| {
            (
                format!("a{i}.md"),
                format!("a{i}.md"),
                ArtifactKind::Markdown,
                at(1_000 + i as u64),
            )
        })
        .collect();
    let items = recently_edited_from(readings);
    assert_eq!(items.len(), DASHBOARD_ITEM_CAP, "the loader applies the cap");
    let ids: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
    assert_eq!(ids, vec!["a7.md", "a6.md", "a5.md", "a4.md", "a3.md"]);
}

#[test]
fn recently_edited_breaks_a_tie_on_the_stable_id_ascending() {
    // PST-FR-31: two files carrying the identical instant order the same way
    // on every call, which is what makes the widget stable.
    let readings = vec![
        ("b.md".into(), "b.md".into(), ArtifactKind::Markdown, at(10)),
        ("a.md".into(), "a.md".into(), ArtifactKind::Flow, at(10)),
    ];
    let first = recently_edited_from(readings.clone());
    let second = recently_edited_from(readings);
    assert_eq!(first, second, "the order is total, so it repeats");
    assert_eq!(
        first.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(),
        vec!["a.md", "b.md"],
        "the tie-break is the stable id ascending"
    );
}

#[test]
fn recently_edited_reports_the_instant_it_read() {
    let items = recently_edited_from(vec![(
        "spec.md".into(),
        "spec.md".into(),
        ArtifactKind::Markdown,
        at(0),
    )]);
    assert_eq!(items[0].modified_at, "1970-01-01T00:00:00.000Z");
}

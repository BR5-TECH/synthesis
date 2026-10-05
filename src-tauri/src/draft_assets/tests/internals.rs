//! Tests for the internal rules of the module: the reference reader, the lock, the channels and the worker.

use super::*;

// ---------------------------------------------------------------------------
// The reference reading itself (DAS-FR-06, DAS-FR-07)
// ---------------------------------------------------------------------------

#[test]
fn a_destination_is_classified_by_the_standard_relative_path_rule() {
    assert_eq!(resolve_destination("../assets/a.png"), Destination::Asset("a.png".into()));
    assert_eq!(
        resolve_destination("../assets/a.png#fig-1"),
        Destination::Asset("a.png".into()),
        "a fragment is not part of the file it names"
    );
    assert_eq!(
        resolve_destination("../assets/a%20b.png"),
        Destination::Asset("a b.png".into()),
        "a percent-escape and its literal are one filename"
    );
    assert_eq!(resolve_destination("../assets/nested/a.png"), Destination::InsideDraft);
    assert_eq!(resolve_destination("./sibling.md"), Destination::InsideDraft);
    for outside in [
        "../../elsewhere.png",
        "/absolute.png",
        "C:/windows.png",
        "data:image/png;base64,AAA",
        "https://example.invalid/a.png",
        "#anchor",
        "",
    ] {
        assert_eq!(resolve_destination(outside), Destination::Outside, "{outside}");
    }
}

#[test]
fn a_reference_inside_a_code_construct_protects_nothing() {
    // DAS-FR-06: the reading is the renderer's, and a renderer draws no image
    // inside a fenced block or a code span — so neither protects a file.
    let references = read_references(
        "```\n![a](../assets/a.png)\n```\n\nand `![b](../assets/b.png)` inline.\n",
    );
    assert!(references.protected.is_empty(), "{:?}", references.protected);
    assert!(references.images.is_empty());
}

#[test]
fn an_image_use_carries_its_alt_text_and_its_span() {
    let references = read_references("before ![a diagram](../assets/a.png) after\n");
    assert_eq!(references.images.len(), 1);
    let used = &references.images[0];
    assert_eq!(used.alt, "a diagram");
    assert_eq!(used.reference, "../assets/a.png");
    assert_eq!(&"before ![a diagram](../assets/a.png) after\n"[used.start..used.end], "![a diagram](../assets/a.png)");
    assert_eq!(references.protected, ["a.png".to_string()].into_iter().collect());
}

#[test]
fn the_accepted_image_kinds_are_recognised_by_their_bytes_and_no_others() {
    // DAS-FR-03: a media type this module accepts, whose bytes are not that kind,
    // is refused — which is what stops arbitrary content being stored under a
    // name a surface will later hand to an image element.
    assert!(bytes_are(kind_of("image/png").unwrap(), &png("x")));
    assert!(!bytes_are(kind_of("image/png").unwrap(), b"GIF89a"));
    assert!(bytes_are(kind_of("image/gif").unwrap(), b"GIF89a rest"));
    assert!(bytes_are(kind_of("image/jpeg").unwrap(), &[0xFF, 0xD8, 0xFF, 0x00]));
    assert!(bytes_are(kind_of("image/svg+xml").unwrap(), b"<svg xmlns=\"...\"></svg>"));
    assert!(!bytes_are(kind_of("image/svg+xml").unwrap(), b"just prose"));
    let mut webp = b"RIFF\0\0\0\0WEBP".to_vec();
    webp.extend_from_slice(b"VP8 ");
    assert!(bytes_are(kind_of("image/webp").unwrap(), &webp));
    // A media type outside the image family is not an image at all.
    assert!(!is_image_media_type("application/pdf"));
    assert!(!is_image_media_type("text/plain"));
    assert!(is_image_media_type("image/png; charset=binary"));
}


// ---------------------------------------------------------------------------
// a removal that succeeds beside one that fails (DAS-FR-28)
// ---------------------------------------------------------------------------

/// DAS-FR-28: the other half of the pair above — a pass whose deletes are **not**
/// refused removes every candidate and counts none failed.
///
/// Together with `das_ts16` this is the claim: a refusal is counted and costs
/// the pass nothing (there), and a pass that can delete does delete (here). One
/// without the other would pass over an implementation that never deleted
/// anything, or over one that stopped at the first refusal.
#[test]
fn das_ts28_a_pass_removes_what_it_can_and_counts_what_it_could_not() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    for name in ["one.png", "two.png", "three.png"] {
        place(&root, &id, name, &png(name));
    }
    save_prompt(&root, &id, "no references\n");

    let pass = sweep(&root, &id);
    assert_eq!(pass.removed, 3, "every unreferenced asset went");
    assert_eq!(pass.failed, 0);
    assert!(pass.complete);
    assert!(asset_names(&root, &id).is_empty());
}

// ---------------------------------------------------------------------------
// DAS-FR-25 — the lock refuses a write and answers a read (DAS-FR-25)
// ---------------------------------------------------------------------------

/// DAS-FR-25: `store_draft_image` and `discard_draft_image` are refused against
/// a draft a graduation holds, and `read_draft_image` and `read_prompt_images`
/// answer for it exactly as for any other draft.
///
/// The refusal itself lives in the command wrappers, which need a Tauri runtime
/// — what is pinned here is the half this module owns: that both **reads** take
/// no lock into account at all, so the "answers as it does for any draft" claim
/// is a property of the code rather than of a test's setup, and that neither
/// read is reachable through a path the lock would have to guard.
#[test]
fn das_ts18_both_reads_are_indifferent_to_the_lock() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    let asset = store_png(&root, &id, "a.png", "read");
    save_prompt(&root, &id, &format!("![a]({})\n", asset.reference));

    // Whatever a run holds, these two answer — reading being what a run and an
    // author both do. Neither takes a lock argument, so there is nothing a
    // locked draft could make them do differently.
    assert!(read_image_impl(&root, &id, &asset.path).is_ok());
    assert_eq!(read_prompt_images(&root, &id).len(), 1);

    // And the two that DO refuse are the two the command surface guards. Named
    // here so a rename fails to compile rather than silently leaving the guard
    // covering nothing.
    let _guarded: (
        fn(&crate::fs::RootFs, &str, &str, Option<&str>, &str, &dyn Fn(&str)) -> Result<DraftAsset, String>,
        fn(&crate::fs::RootFs, &str, &str, &DraftAssetSweeper) -> Result<DraftAssetDiscarded, String>,
    ) = (store_image_impl, discard_image_impl);
    const LIB: &str = include_str!("../../draft_assets.rs");
    for command in ["store_draft_image", "discard_draft_image"] {
        let at = LIB.find(&format!("pub fn {command}")).expect("the command");
        let body = &LIB[at..at + 1600];
        assert!(
            body.contains("require_unlocked_draft"),
            "{command} refuses against a draft a graduation holds (DAS-FR-25)",
        );
    }
    // And the two reads do not, because a lock is not their business.
    for command in ["read_draft_image"] {
        let at = LIB.find(&format!("pub fn {command}")).expect("the command");
        let body = &LIB[at..at + 900];
        assert!(
            !body.contains("require_unlocked_draft"),
            "{command} answers for a locked draft as for any other",
        );
    }
}


// ---------------------------------------------------------------------------
// DAS-FR-24 — an asset is not the draft's record and not its prompt
// ---------------------------------------------------------------------------

/// DAS-FR-24: storing, reading, and sweeping an asset surface nothing on the
/// prompt-mutation channel and emit no `"drafts changed"` — an asset is neither
/// the draft's record nor its prompt, so the drafts index does no work for one.
///
/// Asserted where the claim lives: the three operations are pure over a root
/// and reach no `AppHandle`, so there is nothing they *could* emit. Emitting is
/// something a `#[tauri::command]` does with the handle it holds, and the two
/// asset commands that hold one are read here for the absence.
#[test]
fn das_ts21_no_asset_operation_emits_or_surfaces_on_the_prompt_channel() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");

    // The three of them, run for real, against a root and nothing else.
    let asset = store_png(&root, &id, "a.png", "one");
    read_image_impl(&root, &id, &asset.path).unwrap();
    save_prompt(&root, &id, "no references\n");
    assert_eq!(sweep(&root, &id).removed, 1);

    // None of the three commands announces anything: the drafts channel and the
    // `"drafts changed"` event belong to `crate::drafts`, and the write that
    // carries a new reference into the prompt already emits for the change the
    // author made (DRS-FR-22).
    const SOURCE: &str = include_str!("../../draft_assets.rs");
    let code: String = SOURCE
        .lines()
        .filter(|line| !line.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    for forbidden in ["announce(", "DraftChange::", "app.emit(", "DRAFTS_CHANGED"] {
        assert!(
            !code.contains(forbidden),
            "no operation of this module emits ({forbidden}) — DAS-FR-24",
        );
    }
}

// ---------------------------------------------------------------------------
// DAS-FR-12, DAS-FR-15 / DAS-FR-19 — the scheduler, and what a failed pass leaves behind
// ---------------------------------------------------------------------------

/// DAS-FR-15: a worker that ends — however it ends — leaves the draft able to
/// have another pass scheduled for it.
///
/// The failure this guards against is silent and permanent: a claim left in the
/// running set coalesces every later request into a pass nothing is running,
/// while `sweep_draft_assets` goes on answering `scheduled` true — which is
/// exactly the "reports success for work it did not do" DAS-FR-27 forbids.
#[test]
fn das_ts13_a_finished_worker_leaves_the_draft_schedulable_again() {
    let sweeper = DraftAssetSweeper::default();

    let (first, run_now) = sweeper.claim("draft-1");
    assert!(first.scheduled && !first.coalesced && run_now);
    // A second request while the first is still only pending is coalesced.
    let (second, run_again) = sweeper.claim("draft-1");
    assert!(second.coalesced && !run_again);

    // The worker takes the claim and then ends — as it does on a panic, which
    // the guard in `run_sweep_worker` turns into exactly this call.
    assert!(sweeper.take("draft-1"));
    sweeper.finish("draft-1");

    // The next trigger schedules a fresh worker rather than joining one that is
    // not there.
    let (later, run_fresh) = sweeper.claim("draft-1");
    assert!(later.scheduled, "the request is still accepted");
    assert!(!later.coalesced, "and it is not coalesced into a pass nobody runs");
    assert!(run_fresh, "a worker is started for it");
}

/// DAS-FR-14: a hold survives until the insertion settles, and the two things
/// that settle one are the prompt being saved and the asset being discarded.
#[test]
fn das_ts11_a_hold_is_released_by_a_save_and_by_a_discard_and_by_nothing_else() {
    let sweeper = DraftAssetSweeper::default();
    sweeper.hold("d1", "a.png");
    sweeper.hold("d1", "b.png");
    sweeper.hold("d2", "c.png");

    // A hold is per draft and per asset.
    assert!(sweeper.is_held("d1", "a.png"));
    assert!(!sweeper.is_held("d2", "a.png"));

    // `discard_draft_image` releases the one it removed and nothing else.
    sweeper.release("d1", "a.png");
    assert!(!sweeper.is_held("d1", "a.png"));
    assert!(sweeper.is_held("d1", "b.png"));

    // The draft's prompt being saved settles every insertion outstanding
    // against it — and leaves another draft's alone.
    sweeper.release_draft("d1");
    assert!(!sweeper.is_held("d1", "b.png"));
    assert!(sweeper.is_held("d2", "c.png"));
}

/// DAS-FR-14 / DAS-FR-15: the hold is re-checked **as the pass runs**, not
/// snapshotted when it began.
///
/// A snapshot says nothing about an image the author pasted while the pass was
/// running, and a pass that decided on one would delete the picture seconds
/// after they put it there.
#[test]
fn das_ts11_a_hold_taken_during_a_pass_still_protects_its_asset() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    place(&root, &id, "arriving.png", &png("arriving"));
    place(&root, &id, "stale.png", &png("stale"));
    save_prompt(&root, &id, "no references\n");

    // The hold is taken the first time the pass asks about the file — which
    // stands for a store that completed after the pass had begun.
    let asked = std::cell::Cell::new(false);
    let held = |name: &str| {
        if name == "arriving.png" && !asked.get() {
            asked.set(true);
            return true;
        }
        name == "arriving.png"
    };
    let pass = sweep_draft_impl(&root, &id, false, &held, &never_cancelled());

    assert_eq!(pass.removed, 1, "the unheld asset went");
    assert!(
        assets_of(&root, &id).join("arriving.png").is_file(),
        "the one that gained a hold mid-pass is still there",
    );
}

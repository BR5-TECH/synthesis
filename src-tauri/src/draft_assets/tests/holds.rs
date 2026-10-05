//! Tests for the hold, the discard, and the scheduling of a sweep.

use super::*;

// ---------------------------------------------------------------------------
// DAS-FR-09 — the hold, and the ordering of an insertion (DAS-FR-10, DAS-FR-14)
// ---------------------------------------------------------------------------

#[test]
fn das_ts11_a_held_asset_survives_a_pass_that_runs_before_its_reference_lands() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    save_prompt(&root, &id, "the prompt, so far without a picture\n");

    let sweeper = DraftAssetSweeper::default();
    let asset = store_png(&root, &id, "a.png", "held");
    let name = asset.path.strip_prefix("assets/").unwrap().to_string();
    sweeper.hold(&id, &name);
    assert!(sweeper.is_held(&id, &name));

    // A pass in the instant between the store and the write carrying its
    // reference deletes nothing.
    let pass = sweep_holding(&root, &id, &[name.as_str()]);
    assert_eq!(pass.removed, 0);
    // The held asset, and the filename record beside it — both retained, for
    // two different reasons (DAS-FR-14, DAS-FR-16).
    assert_eq!(pass.retained, 2);
    assert!(assets_of(&root, &id).join(&name).is_file());

    // Once the write lands the saved prompt is what protects it, and the hold
    // has done its work.
    save_prompt(&root, &id, &format!("![a]({})\n", asset.reference));
    sweeper.release_draft(&id);
    assert!(!sweeper.is_held(&id, &name));
    assert_eq!(sweep(&root, &id).removed, 0, "retained as referenced now");

    // DAS-FR-10: where the save fails and no earlier reference names the asset,
    // the asset is discarded, so a failed insertion leaves neither a reference
    // nor a file.
    let second = store_png(&root, &id, "b.png", "discarded");
    let outcome = discard_image_impl(&root, &id, &second.path, &sweeper).unwrap();
    assert!(outcome.discarded && !outcome.retained);
    assert!(!assets_of(&root, &id).join(second.path.strip_prefix("assets/").unwrap()).exists());
    assert_eq!(sweep(&root, &id).removed, 0);
}

// ---------------------------------------------------------------------------
// DAS-FR-09 — discard is idempotent and refuses to take a referenced asset
// ---------------------------------------------------------------------------

#[test]
fn das_ts12_discard_retains_a_referenced_asset_and_is_idempotent() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    let sweeper = DraftAssetSweeper::default();
    let asset = store_png(&root, &id, "a.png", "kept");
    save_prompt(&root, &id, &format!("![a]({})\n", asset.reference));

    let outcome = discard_image_impl(&root, &id, &asset.path, &sweeper).unwrap();
    assert!(outcome.retained && !outcome.discarded);
    assert!(read_image_impl(&root, &id, &asset.path).is_ok(), "still readable");

    for _ in 0..2 {
        let outcome =
            discard_image_impl(&root, &id, "assets/nothing-here.png", &sweeper).unwrap();
        assert!(outcome.discarded, "a path naming nothing is reported discarded");
        assert!(!outcome.retained);
    }
    // DAS-FR-09: a `path` that leaves the draft is `path_escape` and removes
    // nothing.
    assert_eq!(
        discard_image_impl(&root, &id, "../../elsewhere.png", &sweeper).unwrap_err(),
        ERR_PATH_ESCAPE
    );
}

// ---------------------------------------------------------------------------
// DAS-FR-12 — scheduling, coalescing, and the answer's shape (DAS-FR-15)
// ---------------------------------------------------------------------------

#[test]
fn das_ts13_ten_requests_in_one_window_claim_one_pass_and_carry_no_tally() {
    let sweeper = DraftAssetSweeper::default();
    let (first, run_now) = sweeper.claim("draft-1");
    assert!(first.scheduled && !first.coalesced);
    assert!(run_now, "the first request is the one that starts the worker");
    for _ in 0..9 {
        let (later, run_now) = sweeper.claim("draft-1");
        assert!(later.scheduled && later.coalesced);
        assert!(!run_now, "the nine that followed joined the pass already pending");
    }
    // The answer is an acknowledgement and not a result: no scanned, removed,
    // retained, or failed count of any kind appears in it.
    let wire = serde_json::to_string(&first).unwrap();
    for absent in ["scanned", "removed", "retained", "failed", "complete"] {
        assert!(!wire.contains(absent), "the answer carries no {absent}: {wire}");
    }
    assert_eq!(wire, r#"{"scheduled":true,"coalesced":false}"#);

    // A different draft is a pass of its own.
    let (other, run_now) = sweeper.claim("draft-2");
    assert!(other.scheduled && !other.coalesced && run_now);
}

#[test]
fn das_ts13_running_a_pass_twice_over_an_unchanged_draft_removes_what_one_removed() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    let kept = store_png(&root, &id, "kept.png", "kept");
    place(&root, &id, "gone.png", &png("gone"));
    save_prompt(&root, &id, &format!("![kept]({})\n", kept.reference));

    let first = sweep(&root, &id);
    let second = sweep(&root, &id);
    assert_eq!(first.removed, 1);
    assert_eq!(second.removed, 0);
    assert_eq!(asset_names(&root, &id).len(), 1);
}

// ---------------------------------------------------------------------------
// DAS-FR-09, DAS-FR-21, DAS-FR-27 — a refused delete is per file and stops nothing (DAS-FR-28)
// ---------------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn das_ts16_a_refused_delete_costs_the_pass_nothing_but_that_file() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    // Three unreferenced assets, the second inside a folder the process may not
    // write — which is what makes its delete refused while the others succeed.
    place(&root, &id, "one.png", &png("one"));
    place(&root, &id, "three.png", &png("three"));
    save_prompt(&root, &id, "no references\n");

    let assets = assets_of(&root, &id);
    let blocked = assets.join("two.png");
    std::fs::write(&blocked, png("two")).unwrap();
    // A `unlink` is refused by the **containing** directory's write bit, which
    // is the one refusal a portable test can arrange — so every delete in this
    // pass is refused rather than just the second. What DAS-FR-28 claims is
    // still what is asserted: each refusal is counted, each file is untouched,
    // and the pass **did not stop at the first** — a pass that stopped would
    // have scanned one and failed one. That a pass which *can* delete does so
    // is `das_ts28` below, and the two together are the requirement.
    let original = std::fs::metadata(&assets).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_mode(0o500);
    std::fs::set_permissions(&assets, readonly).unwrap();

    let pass = sweep(&root, &id);

    std::fs::set_permissions(&assets, original).unwrap();

    assert_eq!(pass.scanned, 3, "every candidate was examined");
    assert_eq!(pass.removed, 0, "every delete was refused here");
    assert_eq!(
        pass.failed, 3,
        "each is counted, and the pass did not stop at the first — a pass that \
         stopped would have failed one and examined one",
    );
    assert!(!pass.complete);
    assert_eq!(std::fs::read(&blocked).unwrap(), png("two"), "byte-for-byte what it was");
    assert_eq!(asset_names(&root, &id).len(), 3, "all three are still there");

    // DAS-FR-09 / DAS-FR-28: the same condition reported to a caller that asked
    // for one removal is the typed `asset_cleanup_failed`.
    let mut readonly = std::fs::metadata(&assets).unwrap().permissions();
    readonly.set_mode(0o500);
    std::fs::set_permissions(&assets, readonly).unwrap();
    let error =
        discard_image_impl(&root, &id, "assets/two.png", &DraftAssetSweeper::default()).unwrap_err();
    let mut writable = std::fs::metadata(&assets).unwrap().permissions();
    writable.set_mode(0o700);
    std::fs::set_permissions(&assets, writable).unwrap();

    assert!(is_typed(&error, ERR_ASSET_CLEANUP_FAILED), "{error}");
    assert_eq!(category_of(&error), Some(category::DELETE_REFUSED));
    assert!(!error.contains('/'), "a safe diagnostic category and no path: {error}");
    assert_eq!(std::fs::read(&blocked).unwrap(), png("two"));
}

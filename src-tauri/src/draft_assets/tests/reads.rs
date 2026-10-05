//! Tests for what a read of the assets answers, and for the life of an asset with its draft.

use super::*;


// ---------------------------------------------------------------------------
// DAS-FR-15, DAS-FR-20 / DAS-FR-23, GSU-FR-IJEE — an abandoned pass reports nothing completed
// ---------------------------------------------------------------------------

#[test]
fn das_ts15_a_pass_abandoned_at_shutdown_reports_itself_incomplete() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    place(&root, &id, "a.png", &png("a"));
    place(&root, &id, "b.png", &png("b"));
    save_prompt(&root, &id, "no references\n");

    let cancelled = AtomicBool::new(true);
    let pass = sweep_draft_impl(&root, &id, false, &|_: &str| false, &cancelled);
    assert!(!pass.complete, "no route reports that pass completed");
    assert_eq!(pass.removed, 0);
    assert_eq!(asset_names(&root, &id).len(), 2, "the assets it had not decided are still there");

    // And the next trigger simply runs the pass again, which completes normally.
    let pass = sweep(&root, &id);
    assert!(pass.complete);
    assert_eq!(pass.removed, 2);
}

// ---------------------------------------------------------------------------
// DAS-FR-07 — what a conversation is handed (DAS-FR-22)
// ---------------------------------------------------------------------------

#[test]
fn das_ts17_read_prompt_images_returns_the_bytes_in_order_with_their_context() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    let first = store_png(&root, &id, "flow.png", "first");
    let second = store_png(&root, &id, "second.png", "second");
    save_prompt(
        &root,
        &id,
        &format!(
            "Some text.\n\n![flow diagram]({})\n\nMore text.\n\n![the second]({})\n",
            first.reference, second.reference
        ),
    );

    let images = read_prompt_images(&root, &id);
    assert_eq!(images.len(), 2);
    for (image, stored, alt) in [
        (&images[0], &first, "flow diagram"),
        (&images[1], &second, "the second"),
    ] {
        assert!(image.resolved);
        assert_eq!(image.asset_path.as_deref(), Some(stored.path.as_str()));
        assert_eq!(image.media_type.as_deref(), Some("image/png"));
        assert_eq!(image.alt, alt);
        assert_eq!(image.reference, stored.reference);
        // DAS-FR-22: the context is the Markdown reference together with the alt
        // text, so a model reads a picture with what the prompt called it.
        assert!(image.context.contains(alt), "{}", image.context);
        assert!(image.context.contains(&stored.reference), "{}", image.context);
        // The bytes themselves, decoding to exactly what a direct read returns.
        let direct = read_image_impl(&root, &id, &stored.path).unwrap();
        assert_eq!(image.data.as_deref(), Some(direct.data.as_str()));
    }
    // Ordered by where they occur in the prompt.
    assert!(images[0].start < images[1].start);

    // DAS-FR-22: no handle, no token, and no path a caller would have to fetch
    // by — the bytes are in the returned value.
    assert!(images.iter().all(|i| i.data.is_some()));
}

#[test]
fn das_ts17_an_unresolved_image_keeps_its_position_and_its_context() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    let stored = store_png(&root, &id, "a.png", "one");
    save_prompt(
        &root,
        &id,
        &format!(
            "![a]({})\n\n![remote](https://example.invalid/x.png)\n\n\
             ![undefined][no-such-label]\n\n![missing](../assets/not-there.png)\n",
            stored.reference
        ),
    );

    let images = read_prompt_images(&root, &id);
    assert_eq!(images.len(), 4, "each is returned in its own position");
    assert!(images[0].resolved);
    for image in &images[1..] {
        assert!(!image.resolved);
        assert!(image.asset_path.is_none() && image.media_type.is_none() && image.data.is_none());
        assert!(!image.context.is_empty());
    }
    assert!(images[1].reference.contains("example.invalid"));
    assert!(images[2].alt.contains("undefined"), "{:?}", images[2]);
    assert!(images[3].reference.contains("not-there.png"));
}

#[test]
fn das_ts17_one_unreadable_asset_costs_the_caller_nothing_but_that_image() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    let good = store_png(&root, &id, "good.png", "good");
    let bad = store_png(&root, &id, "bad.png", "bad");
    save_prompt(
        &root,
        &id,
        &format!("![good]({})\n\n![bad]({})\n", good.reference, bad.reference),
    );
    // Replaced by a directory of the same name: the entry is there and its bytes
    // cannot be read.
    let bad_name = bad.path.strip_prefix("assets/").unwrap();
    std::fs::remove_file(assets_of(&root, &id).join(bad_name)).unwrap();
    std::fs::create_dir(assets_of(&root, &id).join(bad_name)).unwrap();

    let images = read_prompt_images(&root, &id);
    assert_eq!(images.len(), 2);
    assert!(images[0].resolved && images[0].data.is_some());
    assert!(!images[1].resolved, "that entry alone comes back unresolved");
    assert!(!images[1].context.is_empty());
}

// ---------------------------------------------------------------------------
// DAS-FR-25 — the graduation lock (DAS-FR-25)
// ---------------------------------------------------------------------------

#[test]
fn das_ts18_a_sweep_against_a_locked_or_graduated_draft_deletes_nothing() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    place(&root, &id, "unreferenced.png", &png("unreferenced"));
    save_prompt(&root, &id, "no references at all\n");

    let pass = sweep_draft_impl(&root, &id, true, &|_: &str| false, &never_cancelled());
    assert_eq!(pass.removed, 0, "the prompt cannot change, so there is nothing to collect");
    assert_eq!(pass.retained, 1);
    assert!(pass.complete);
    assert!(assets_of(&root, &id).join("unreferenced.png").is_file());

    // Reading answers for a locked draft exactly as for any other, reading being
    // what a run and an author both do.
    place(&root, &id, "read-me.png", &png("read"));
    save_prompt(&root, &id, "![r](../assets/read-me.png)\n");
    assert!(read_image_impl(&root, &id, "assets/read-me.png").is_ok());
    assert_eq!(read_prompt_images(&root, &id).len(), 1);
}

// ---------------------------------------------------------------------------
// DRS-FR-21, DRS-FR-32 — assets travel with the draft, and go with it (DAS-FR-01)
// ---------------------------------------------------------------------------

#[test]
fn das_ts19_moving_a_draft_carries_its_assets_and_deleting_it_takes_them() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    drafts::create_drafts_folder_impl(&root, "", "UI").unwrap();
    drafts::create_drafts_folder_impl(&root, "UI", "Components").unwrap();
    drafts::create_drafts_folder_impl(&root, "", "backend").unwrap();
    let id = drafts::create_draft_impl(&root, Some("filed"), Some("UI/Components"))
        .unwrap()
        .draft
        .id;
    let one = store_png(&root, &id, "one.png", "one");
    let two = store_png(&root, &id, "two.png", "two");

    drafts::move_draft_to_folder_impl(&root, &id, "backend").unwrap();

    // DRS-FR-40: resolved by the walk, so the folder moved with the draft.
    let moved = assets_of(&root, &id);
    assert!(moved.to_string_lossy().contains("backend"), "{}", moved.display());
    for stored in [&one, &two] {
        assert_eq!(
            read_image_impl(&root, &id, &stored.path).unwrap().data,
            b64(&png(stored.filename.as_deref().unwrap().strip_suffix(".png").unwrap()))
        );
    }

    drafts::delete_draft_impl(&root, &root, &id).unwrap();
    assert!(!moved.exists(), "the assets went with the folder");
    // Nothing outside `.synthesis/drafts/` was written.
    let stray: Vec<_> = tree_bytes(dir.path())
        .into_iter()
        .filter(|(p, _)| !p.to_string_lossy().contains(".synthesis"))
        .collect();
    assert!(stray.is_empty(), "{stray:?}");
}

// ---------------------------------------------------------------------------
// DAS-FR-27 — the fixed error vocabulary (DAS-FR-27)
// ---------------------------------------------------------------------------

#[test]
fn das_ts22_every_typed_error_is_safe_and_leaves_the_draft_as_it_was() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    let sweeper = DraftAssetSweeper::default();
    let asset = store_png(&root, &id, "a.png", "one");
    save_prompt(&root, &id, "no reference to it\n");
    let before = tree_bytes(dir.path());

    let provoked = [
        store_image_impl(&root, "no-such-draft", "image/png", None, &b64(&png("x")), &no_hold()).unwrap_err(),
        store_image_impl(&root, &id, "text/plain", None, &b64(b"hello"), &no_hold()).unwrap_err(),
        store_image_impl(
            &root,
            &id,
            "image/png",
            None,
            &b64(&vec![0u8; MAX_IMAGE_BYTES + 1]),
            &no_hold(),
        )
        .unwrap_err(),
        store_image_impl(&root, &id, "image/png", None, "%%%", &no_hold()).unwrap_err(),
        read_image_impl(&root, &id, "assets/nope.png").unwrap_err(),
        read_image_impl(&root, &id, "/etc/passwd").unwrap_err(),
    ];
    assert_eq!(
        provoked,
        [
            ERR_DRAFT_NOT_FOUND,
            ERR_UNSUPPORTED_MEDIA_TYPE,
            ERR_IMAGE_TOO_LARGE,
            ERR_MALFORMED_IMAGE,
            ERR_ASSET_NOT_FOUND,
            ERR_PATH_ESCAPE,
        ]
    );
    for error in &provoked {
        // Every value is a token from the fixed vocabulary: no path, no address,
        // no operating-system message.
        assert!(!error.contains('/'), "{error}");
        assert!(!error.contains(' '), "{error}");
        assert!(!error.contains(dir.path().to_str().unwrap()), "{error}");
    }
    assert_eq!(before, tree_bytes(dir.path()), "nothing changed");

    // And the one removal a successful discard performs, and nothing else.
    discard_image_impl(&root, &id, &asset.path, &sweeper).unwrap();
    let after = tree_bytes(dir.path());
    assert_eq!(before.len(), after.len() + 1);
}


//! Tests for the references that protect an asset from a sweep.

use super::*;

// ---------------------------------------------------------------------------
// DAS-FR-08, DAS-FR-22, DAS-FR-27 — the two answers to an unsafe destination (DAS-FR-07)
// ---------------------------------------------------------------------------

#[test]
fn das_ts04_a_direct_read_is_refused_and_a_resolution_refuses_nothing() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    let asset = store_png(&root, &id, "a.png", "valid");
    let name = asset.path.strip_prefix("assets/").unwrap().to_string();
    save_prompt(&root, &id, &format!("![diagram]({})\n", asset.reference));

    // The prompt's own destination and the draft-relative path are one file.
    for path in [asset.path.as_str(), asset.reference.as_str()] {
        let content = read_image_impl(&root, &id, path).unwrap();
        assert_eq!(content.media_type, "image/png");
        assert_eq!(content.data, b64(&png("valid")));
    }
    assert_eq!(
        read_image_impl(&root, &id, "assets/missing.png").unwrap_err(),
        ERR_ASSET_NOT_FOUND
    );
    // DAS-FR-07: a destination that leaves the draft, an absolute path, and a
    // data destination are each refused, and nothing outside the draft was
    // opened, followed, or requested.
    let secret = dir.path().join("secret.txt");
    std::fs::write(&secret, b"nobody may read this").unwrap();
    for escape in [
        "../../etc/passwd",
        "/etc/passwd",
        "data:image/png;base64,AAA",
        "https://example.invalid/a.png",
        secret.to_str().unwrap(),
    ] {
        assert_eq!(
            read_image_impl(&root, &id, escape).unwrap_err(),
            ERR_PATH_ESCAPE,
            "{escape} is not the draft's to reach"
        );
    }
    // A symbolic link out of the draft is an escape the syntactic walk cannot
    // see, so the read refuses it before the link is followed.
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&secret, assets_of(&root, &id).join("link.png")).unwrap();
        assert_eq!(
            read_image_impl(&root, &id, "assets/link.png").unwrap_err(),
            ERR_PATH_ESCAPE
        );
    }

    // DAS-FR-22 / DAS-FR-12: resolution refuses nothing. One unreferenced asset
    // is still collected, and every unsafe destination comes back unresolved
    // with its context intact.
    place(&root, &id, "orphan.png", &png("orphan"));
    save_prompt(
        &root,
        &id,
        &format!(
            "![diagram]({})\n\n![a](../../etc/passwd)\n\n![b](/etc/passwd)\n\n\
             ![c](data:image/png;base64,AAA)\n\n![d](https://example.invalid/a.png)\n",
            asset.reference
        ),
    );
    let images = read_prompt_images(&root, &id);
    assert_eq!(images.len(), 5, "every image use is returned");
    assert!(images[0].resolved);
    assert_eq!(images[0].data.as_deref(), Some(b64(&png("valid")).as_str()));
    for unresolved in &images[1..] {
        assert!(!unresolved.resolved);
        assert!(unresolved.asset_path.is_none());
        assert!(unresolved.media_type.is_none());
        assert!(unresolved.data.is_none());
        assert!(!unresolved.context.is_empty(), "its context is intact");
    }

    let pass = sweep(&root, &id);
    assert_eq!(pass.removed, 1, "the unreferenced asset was still collected");
    assert!(asset_names(&root, &id).contains(&name));
    assert!(!asset_names(&root, &id).contains(&"orphan.png".to_string()));
    assert_eq!(
        std::fs::read(&secret).unwrap(),
        b"nobody may read this",
        "nothing outside the draft was touched"
    );
}

// ---------------------------------------------------------------------------
// DAS-FR-12 — reference-style images, defined below and respelled (DAS-FR-06)
// ---------------------------------------------------------------------------

#[test]
fn das_ts05_reference_style_images_protect_their_assets_however_they_are_spelled() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    for name in ["a.png", "b.png", "c.png", "d.png"] {
        place(&root, &id, name, &png(name));
    }
    save_prompt(
        &root,
        &id,
        "![a][one]\n\n![two][]\n\n![three]\n\n![d](../assets/d.png)\n\n\
         [one]: ../assets/a.png\n[two]: ../assets/b.png\n[three]: ../assets/c.png\n",
    );

    let pass = sweep(&root, &id);
    assert_eq!(pass.removed, 0, "a definition below its use resolves");
    assert_eq!(pass.retained, 4);
    assert_eq!(asset_names(&root, &id).len(), 4);

    // DAS-FR-06: labels are matched by the renderer's own normalization, so two
    // spellings of one label resolve to one destination.
    save_prompt(
        &root,
        &id,
        "![a][One]\n\n![TWO][]\n\n![Three]\n\n![d](../assets/d.png)\n\n\
         [ONE]: ../assets/a.png\n[two   ]: ../assets/b.png\n[tHrEe]: ../assets/c.png\n",
    );
    let pass = sweep(&root, &id);
    assert_eq!(pass.removed, 0, "still no asset was removed");
    assert_eq!(asset_names(&root, &id).len(), 4);
}

// ---------------------------------------------------------------------------
// DAS-FR-12 — every reference protects, not the first (DAS-FR-11)
// ---------------------------------------------------------------------------

#[test]
fn das_ts06_an_asset_is_protected_by_every_reference_to_it() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    place(&root, &id, "x.png", &png("x"));

    save_prompt(
        &root,
        &id,
        "![one](../assets/x.png)\n\n![two](../assets/x.png)\n\n![three](../assets/x.png)\n",
    );
    assert_eq!(sweep(&root, &id).removed, 0);

    save_prompt(&root, &id, "![three](../assets/x.png)\n");
    assert_eq!(sweep(&root, &id).removed, 0, "one of three still protects it");
    assert!(assets_of(&root, &id).join("x.png").is_file());

    save_prompt(&root, &id, "nothing here now\n");
    let pass = sweep(&root, &id);
    assert_eq!(pass.removed, 1, "the last reference is gone");
    assert!(!assets_of(&root, &id).join("x.png").exists());
}

// ---------------------------------------------------------------------------
// DAS-FR-12 — a definition and a link protect too (DAS-FR-17)
// ---------------------------------------------------------------------------

#[test]
fn das_ts07_a_reference_definition_and_a_link_destination_protect_an_asset() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    for name in ["y.png", "z.png", "w.png"] {
        place(&root, &id, name, &png(name));
    }
    save_prompt(
        &root,
        &id,
        "The prompt links [the sketch](../assets/w.png) and defines a label it no longer uses.\n\n\
         [unused]: ../assets/z.png\n",
    );

    let pass = sweep(&root, &id);
    assert_eq!(pass.removed, 1);
    assert_eq!(asset_names(&root, &id), vec!["w.png".to_string(), "z.png".to_string()]);
}

// ---------------------------------------------------------------------------
// DAS-FR-12 — everything uncertain is left alone (DAS-FR-16, DAS-FR-18)
// ---------------------------------------------------------------------------

#[test]
fn das_ts08_a_link_a_directory_an_unknown_extension_and_a_fake_png_are_all_retained() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    let assets = assets_of(&root, &id);

    let outside = dir.path().join("outside.png");
    std::fs::write(&outside, png("outside")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside, assets.join("link.png")).unwrap();
    std::fs::create_dir(assets.join("nested")).unwrap();
    place(&root, &id, "notes.txt", b"plain text");
    place(&root, &id, "fake.png", b"this is not a png at all");
    save_prompt(&root, &id, "the prompt references none of them\n");

    let pass = sweep(&root, &id);
    #[cfg(unix)]
    let expected = 4;
    #[cfg(not(unix))]
    let expected = 3;
    assert_eq!(pass.scanned, expected);
    assert_eq!(pass.removed, 0);
    assert_eq!(pass.retained, expected);
    assert!(pass.complete);
    // The link's target was neither read nor removed.
    assert_eq!(std::fs::read(&outside).unwrap(), png("outside"));
    assert!(assets.join("nested").is_dir());
    assert_eq!(std::fs::read(assets.join("fake.png")).unwrap(), b"this is not a png at all");
}

#[test]
fn das_ts08_a_pass_removes_files_alone_and_never_a_directory() {
    // DAS-FR-18: it removes no directory, the `assets/` folder itself included,
    // so a draft whose every image has gone still holds an empty `assets/`.
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    place(&root, &id, "gone.png", &png("gone"));
    save_prompt(&root, &id, "nothing\n");

    assert_eq!(sweep(&root, &id).removed, 1);
    assert!(assets_of(&root, &id).is_dir(), "the folder itself stands");
    assert!(asset_names(&root, &id).is_empty());
    // And nothing under the draft's siblings was touched.
    let draft_dir = drafts::draft_dir(&root, &id).unwrap();
    for sibling in ["files", "proposals", "history"] {
        assert!(draft_dir.join(sibling).is_dir(), "{sibling} stands");
    }
    assert_eq!(std::fs::read_dir(draft_dir.join("files")).unwrap().count(), 1);
}

// ---------------------------------------------------------------------------
// DAS-FR-16 — an unsafe destination costs the pass nothing (DAS-FR-07/16)
// ---------------------------------------------------------------------------

#[test]
fn das_ts09_malformed_external_and_escaping_destinations_neither_protect_nor_condemn() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    place(&root, &id, "unreferenced.png", &png("unreferenced"));
    let secret = dir.path().join("secrets.png");
    std::fs::write(&secret, png("secret")).unwrap();

    save_prompt(
        &root,
        &id,
        "![broken(../assets/unreferenced.png\n\n![missing][no-such-label]\n\n\
         ![d](data:image/png;base64,AAA)\n\n![e](https://example.invalid/a.png)\n\n\
         ![x](../../../secrets.png)\n",
    );

    let pass = sweep(&root, &id);
    assert_eq!(pass.removed, 1, "the unreferenced asset is collected");
    assert!(pass.complete);
    assert!(asset_names(&root, &id).is_empty());
    assert_eq!(
        std::fs::read(&secret).unwrap(),
        png("secret"),
        "nothing outside the draft was read, resolved, fetched, or deleted"
    );
}

// ---------------------------------------------------------------------------
// DAS-FR-21 — an unreadable prompt protects everything (DAS-FR-13, DAS-FR-16)
// ---------------------------------------------------------------------------

#[test]
fn das_ts10_a_prompt_that_cannot_be_read_deletes_nothing_at_all() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    place(&root, &id, "a.png", &png("a"));
    place(&root, &id, "b.png", &png("b"));

    // A prompt whose bytes do not decode as UTF-8.
    let prompt = drafts::draft_file_abs_path(&root, &id, &drafts::require_prompt(&root, &id).unwrap())
        .unwrap();
    std::fs::write(&prompt, [0xF0, 0x9F, 0x92]).unwrap();

    let pass = sweep(&root, &id);
    assert_eq!(pass.removed, 0);
    assert_eq!(pass.retained, 2, "every file retained");
    assert!(!pass.complete, "the pass reports itself incomplete");
    assert_eq!(asset_names(&root, &id).len(), 2);
}

#[test]
fn das_ts10_an_inconsistent_draft_deletes_nothing_at_all() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    place(&root, &id, "a.png", &png("a"));
    // A second file under `files/` makes the draft inconsistent (DRS-FR-15).
    let files = drafts::draft_dir(&root, &id).unwrap().join("files");
    std::fs::write(files.join("second.md"), "another").unwrap();

    let pass = sweep(&root, &id);
    assert_eq!(pass.removed, 0);
    assert_eq!(pass.retained, 1);
    assert!(!pass.complete);
    assert!(assets_of(&root, &id).join("a.png").is_file());
}


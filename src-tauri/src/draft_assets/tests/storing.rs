//! Tests for a stored asset, its reference, and the refusals a store answers.

use super::*;

// ---------------------------------------------------------------------------
// DAS-FR-01, DAS-FR-02, DAS-FR-05, DAS-FR-26 — a stored asset, its path, and its reference
// ---------------------------------------------------------------------------

#[test]
fn das_ts01_a_store_writes_an_opaquely_named_file_and_returns_its_reference() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");

    let asset = store_png(&root, &id, "My Screenshot.png", "one");

    // DAS-FR-01 / DAS-FR-02: the file is inside that draft's own directory,
    // named by an opaque identifier with the extension the media type belongs
    // to.
    assert!(asset.path.starts_with("assets/"), "{}", asset.path);
    assert!(asset.path.ends_with(".png"), "{}", asset.path);
    let name = asset.path.strip_prefix("assets/").unwrap();
    assert!(assets_of(&root, &id).join(name).is_file());
    // DAS-FR-05: the destination that resolves to it from the prompt, which sits
    // at the root of `files/`.
    assert_eq!(asset.reference, format!("../{}", asset.path));
    assert_eq!(asset.media_type, "image/png");
    assert_eq!(asset.filename.as_deref(), Some("My Screenshot.png"));
    assert_eq!(asset.bytes, png("one").len() as u64);
    // DAS-FR-02: the supplied filename never reaches the path.
    assert!(
        !asset.path.contains("My Screenshot") && !asset.path.contains(' '),
        "the supplied filename appears nowhere in the path: {}",
        asset.path
    );

    // DAS-FR-02: two stores of byte-identical images produce two assets with two
    // identifiers, this module holding no content-addressed store.
    let again = store_image_impl(
        &root,
        &id,
        "image/png",
        Some("My Screenshot.png"),
        &b64(&png("one")),
        &no_hold(),
    )
    .unwrap();
    assert_ne!(again.path, asset.path);
    assert_eq!(asset_names(&root, &id).len(), 2);
    for stored in [&asset, &again] {
        let content = read_image_impl(&root, &id, &stored.path).unwrap();
        assert_eq!(content.media_type, "image/png");
        assert_eq!(content.data, b64(&png("one")));
        // DAS-FR-08: the read serves the **recorded filename** beside the bytes
        // — the name the image was supplied under, which DAS-FR-02 keeps out of
        // the path and records as metadata alone.
        assert_eq!(content.filename.as_deref(), Some("My Screenshot.png"));
    }

    // DAS-FR-02: and the record it is kept in is a file of the folder's own,
    // carrying no accepted image extension — so a sweep retains it rather than
    // collecting it as an unreferenced picture (DAS-FR-12, DAS-FR-16).
    save_prompt(&root, &id, "no references at all\n");
    let pass = sweep(&root, &id);
    assert_eq!(pass.removed, 2, "both pictures went");
    assert_eq!(pass.retained, 1, "the filename record stayed");
    assert_eq!(everything_in_assets(&root, &id), vec![FILENAMES_FILE.to_string()]);
}

/// DAS-FR-08 / DAS-FR-22: a store with no supplied filename records none, and
/// both reads say so by leaving the field absent rather than inventing one.
#[test]
fn das_ts01_an_image_supplied_under_no_name_carries_none() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    let asset =
        store_image_impl(&root, &id, "image/png", None, &b64(&png("anon")), &no_hold()).unwrap();
    assert_eq!(asset.filename, None);
    assert_eq!(read_image_impl(&root, &id, &asset.path).unwrap().filename, None);
    // A whitespace-only name is no name at all.
    let blank =
        store_image_impl(&root, &id, "image/png", Some("   "), &b64(&png("b")), &no_hold()).unwrap();
    assert_eq!(blank.filename, None);

    // DAS-FR-22: and a prompt image carries the recorded name where there is
    // one, so an agent reads a picture with what the author called it.
    let named = store_png(&root, &id, "flow diagram.png", "named");
    save_prompt(&root, &id, &format!("![f]({})\n", named.reference));
    let images = read_prompt_images(&root, &id);
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].filename.as_deref(), Some("flow diagram.png"));
}

#[test]
fn das_ts01_every_command_of_the_contract_surface_answers_its_documented_shape() {
    // DAS-FR-26: every command exists with a typed payload, and each still
    // refuses the media types, sizes, and paths above. Exercised through the
    // implementations the commands are thin wrappers over, so the shapes are
    // pinned without a Tauri runtime.
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");

    let asset: DraftAsset = store_png(&root, &id, "a.png", "one");
    let content: DraftImageContent = read_image_impl(&root, &id, &asset.path).unwrap();
    assert!(!content.data.is_empty());
    let discarded: DraftAssetDiscarded =
        discard_image_impl(&root, &id, &asset.path, &DraftAssetSweeper::default()).unwrap();
    assert!(discarded.discarded && !discarded.retained);

    assert_eq!(
        store_image_impl(&root, &id, "application/pdf", None, &b64(b"%PDF"), &no_hold()).unwrap_err(),
        ERR_UNSUPPORTED_MEDIA_TYPE
    );
    assert_eq!(
        read_image_impl(&root, &id, "../../escape.png").unwrap_err(),
        ERR_PATH_ESCAPE
    );
}

// ---------------------------------------------------------------------------
// DAS-FR-03 — the three validation refusals (DAS-FR-03)
// ---------------------------------------------------------------------------

#[test]
fn das_ts02_a_refused_store_writes_nothing_at_all() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    let before = tree_bytes(dir.path());

    assert_eq!(
        store_image_impl(&root, &id, "application/pdf", Some("a.pdf"), &b64(b"%PDF-1.7"), &no_hold())
            .unwrap_err(),
        ERR_UNSUPPORTED_MEDIA_TYPE,
    );
    assert_eq!(
        store_image_impl(
            &root,
            &id,
            "image/png",
            Some("huge.png"),
            &b64(&vec![0u8; MAX_IMAGE_BYTES + 1]),
            &no_hold(),
        )
        .unwrap_err(),
        ERR_IMAGE_TOO_LARGE,
    );
    assert_eq!(
        store_image_impl(&root, &id, "image/png", Some("a.png"), "not base64!!", &no_hold()).unwrap_err(),
        ERR_MALFORMED_IMAGE,
    );
    assert_eq!(
        store_image_impl(&root, &id, "image/png", Some("a.png"), &b64(b"GIF89a not a png"), &no_hold())
            .unwrap_err(),
        ERR_MALFORMED_IMAGE,
    );

    assert!(asset_names(&root, &id).is_empty(), "no file was written");
    assert_eq!(before, tree_bytes(dir.path()), "the draft is byte-for-byte what it was");
}

#[test]
fn das_ts02_an_image_type_this_module_cannot_name_a_file_for_is_malformed() {
    // DAS-FR-03: "data … that carries no filename extension it can derive" is
    // `malformed_image`, distinct from the `unsupported_media_type` a
    // non-image gets. The distinction matters to the author: one says bring a
    // different picture, the other says this picture is not readable.
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    assert_eq!(
        store_image_impl(&root, &id, "image/x-unheard-of", None, &b64(&png("one")), &no_hold()).unwrap_err(),
        ERR_MALFORMED_IMAGE,
    );
    assert!(asset_names(&root, &id).is_empty());
}

// ---------------------------------------------------------------------------
// DAS-FR-16, DAS-FR-27 — a store that could not be completed (DAS-FR-04)
// ---------------------------------------------------------------------------

#[test]
fn das_ts03_a_store_that_cannot_complete_leaves_no_entry_and_names_no_path() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");

    // A plain file stands where `assets/` should be, which is how a real "the
    // filesystem refuses" reproduces without a fault-injecting filesystem. The
    // folder is there as far as a stat goes, so what fails is the **write**.
    let assets = assets_of(&root, &id);
    std::fs::remove_dir_all(&assets).unwrap();
    std::fs::write(&assets, b"not a directory").unwrap();

    let error = store_image_impl(&root, &id, "image/png", Some("a.png"), &b64(&png("one")), &no_hold())
        .unwrap_err();
    assert!(is_typed(&error, ERR_ASSET_STORE_FAILED), "{error}");
    // DAS-FR-04 / DAS-FR-27: carrying a **safe diagnostic category** rather than
    // the underlying error's own text — which is what tells a reader of the log
    // which half of the store was the problem.
    assert_eq!(category_of(&error), Some(category::WRITE_REFUSED));
    // DAS-FR-04 / DAS-FR-27: a safe diagnostic category and never the underlying
    // error's own text — no path, no operating-system message.
    assert!(!error.contains('/'), "no path in the error: {error}");
    assert!(
        !error.to_lowercase().contains("os error") && !error.contains(dir.path().to_str().unwrap()),
        "no operating-system message in the error: {error}"
    );
    // And nothing partial was left behind.
    assert!(assets.is_file(), "the obstruction is untouched");
    assert_eq!(std::fs::read(&assets).unwrap(), b"not a directory");
}

/// DAS-FR-04, DAS-FR-16, DAS-FR-27: a store into a draft whose `assets/` folder is missing **and
/// cannot be created** is `asset_store_failed` on the same terms, and says so
/// with the category that names the folder rather than the write.
///
/// The other half of DAS-FR-04's "one whose folder cannot be created": the test
/// above obstructs the folder with a file, which a stat accepts, so it never
/// reaches the creation branch at all.
#[cfg(unix)]
#[test]
fn das_ts03_a_folder_that_cannot_be_created_names_the_folder() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    let draft_dir = drafts::draft_dir(&root, &id).unwrap();
    std::fs::remove_dir_all(assets_of(&root, &id)).unwrap();

    let original = std::fs::metadata(&draft_dir).unwrap().permissions();
    let mut readonly = original.clone();
    readonly.set_mode(0o500);
    std::fs::set_permissions(&draft_dir, readonly).unwrap();

    let error = store_image_impl(&root, &id, "image/png", Some("a.png"), &b64(&png("one")), &no_hold())
        .unwrap_err();

    std::fs::set_permissions(&draft_dir, original).unwrap();

    assert!(is_typed(&error, ERR_ASSET_STORE_FAILED), "{error}");
    assert_eq!(category_of(&error), Some(category::FOLDER_UNAVAILABLE));
    assert!(!error.contains('/'), "no path in the error: {error}");
    assert!(!assets_of(&root, &id).exists(), "nothing was created");
}

/// DAS-FR-03: a truncated container is refused rather than crashing the
/// command.
///
/// An `ftyp` box whose compatible-brand list is cut off is a damaged image, and
/// a damaged image is `malformed_image` — the one thing it must never be is a
/// panic, because the same signature check runs inside the housekeeping worker
/// (DAS-FR-12), where a panic would take the pass down mid-sweep and leave the
/// draft's housekeeping wedged (DAS-FR-15).
#[test]
fn das_ts03_a_truncated_container_is_refused_rather_than_panicking() {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");

    for media_type in ["image/avif", "image/heic", "image/heif"] {
        for length in 12..=16usize {
            let mut bytes = b"\x00\x00\x00\x0Cftypmp42".to_vec();
            bytes.truncate(length);
            assert_eq!(
                store_image_impl(&root, &id, media_type, None, &b64(&bytes), &no_hold())
                    .unwrap_err(),
                ERR_MALFORMED_IMAGE,
                "{media_type} at {length} bytes",
            );
        }
        // And the brand that really is one is still accepted, so the guard
        // above did not simply refuse the whole family.
        let mut real = b"\x00\x00\x00\x18ftypmif1".to_vec();
        real.extend_from_slice(b"\x00\x00\x00\x00");
        real.extend_from_slice(if media_type == "image/avif" { b"avif" } else { b"heic" });
        assert!(
            store_image_impl(&root, &id, media_type, None, &b64(&real), &no_hold()).is_ok(),
            "{media_type} with a real compatible brand",
        );
    }

    // And the same content under `assets/` does not take a sweep down with it.
    place(&root, &id, "truncated.avif", b"\x00\x00\x00\x0Cftypmp42");
    save_prompt(&root, &id, "no references\n");
    let pass = sweep(&root, &id);
    assert!(
        asset_names(&root, &id).contains(&"truncated.avif".to_string()),
        "a file whose content this module cannot recognise is retained",
    );
    assert!(pass.complete, "and the pass decided every file it scanned");
}

#[test]
fn das_ts03_a_temporary_name_an_interrupted_store_left_behind_is_retained() {
    // DAS-FR-04 / DAS-FR-16: a temporary name is not an accepted image, so a
    // sweep retains it rather than deleting it — and the next store simply
    // writes its own.
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "spec");
    place(&root, &id, ".tmp-abc123", &png("interrupted"));
    save_prompt(&root, &id, "no images here\n");

    let pass = sweep(&root, &id);
    assert_eq!((pass.scanned, pass.removed, pass.retained), (1, 0, 1));
    assert_eq!(asset_names(&root, &id), vec![".tmp-abc123".to_string()]);
}


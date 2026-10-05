//! Store, read, and discard one asset.

use super::*;

// ---------------------------------------------------------------------------
// Store, read, discard (DAS-FR-03, DAS-FR-04, DAS-FR-08, DAS-FR-09)
// ---------------------------------------------------------------------------

/// [`crate::drafts::draft_assets_dir`] with this module's typed refusal in place
/// of its prose (DAS-FR-27).
pub(super) fn assets_dir(root: &fs::RootFs, draft_id: &str) -> Result<PathBuf, String> {
    crate::drafts::draft_assets_dir(root, draft_id)
        .map_err(|_| ERR_DRAFT_NOT_FOUND.to_string())
}

/// Monotonic within the process, so two stores in the same millisecond still get
/// distinct identifiers.
pub(super) static ASSET_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// DAS-FR-02: the opaque identifier a stored asset is named by, generated at the
/// store.
///
/// The supplied filename never reaches the path, so a name cannot collide with a
/// stored asset, cannot direct a write, and cannot carry a path separator or a
/// traversal segment. Two stores of byte-identical images produce two
/// identifiers, this module holding no content-addressed store.
pub(super) fn new_asset_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let counter = ASSET_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u64(millis);
    hasher.write_u32(counter);
    let entropy = hasher.finish() as u64;
    format!("{millis:011x}-{counter:04x}-{entropy:016x}")
}

/// DAS-FR-03 / DAS-FR-04: validate an image and store it under an opaque
/// identifier.
///
/// Validation happens **before anything is written**, so a refused store writes
/// no file, creates no directory entry, and leaves the draft byte-for-byte as it
/// was. The three validation refusals say the image was not acceptable; the
/// fourth, `asset_store_failed`, says the store did not happen — a caller
/// reports them differently (per `../ui/NAW-new-artifact.md` NAW-FR-51).
pub fn store_image_impl(
    root: &fs::RootFs,
    draft_id: &str,
    media_type: &str,
    filename: Option<&str>,
    data: &str,
    // DAS-FR-14: called with the asset's name the moment it is minted and
    // **before** the bytes are written, so a sweep can never see the file
    // without also seeing the hold. Registering the hold after the write would
    // leave a window in which the asset exists, no saved reference names it
    // yet, and nothing protects it — which is exactly the instant DAS-FR-14
    // exists to cover.
    hold: &dyn Fn(&str),
) -> Result<DraftAsset, String> {
    // Resolved first: an id no draft carries is a refusal to resolve, and
    // nothing was read or written (DAS-FR-27).
    let dir = assets_dir(root, draft_id)?;
    // DRS-FR-QPSC: a GitHub-shadow draft takes no asset write.
    crate::drafts::require_not_github_shadow(root, draft_id)?;
    // DRS-FR-15: an inconsistent draft is reported rather than written to.
    crate::drafts::require_prompt(root, draft_id)?;

    if !is_image_media_type(media_type) {
        return Err(ERR_UNSUPPORTED_MEDIA_TYPE.to_string());
    }
    // A media type inside the image family this module cannot name a file for
    // is `malformed_image`: DAS-FR-03 refuses data "that carries no filename
    // extension it can derive", and the extension is derived from the type.
    let Some(kind) = kind_of(media_type) else {
        return Err(ERR_MALFORMED_IMAGE.to_string());
    };
    use base64::Engine as _;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map_err(|_| ERR_MALFORMED_IMAGE.to_string())?;
    // Bounded on the **decoded** length, which is what lands on disk: base64
    // inflates by a third, and bounding the encoded form would refuse files
    // under the stated limit.
    if bytes.len() > MAX_IMAGE_BYTES {
        return Err(ERR_IMAGE_TOO_LARGE.to_string());
    }
    if !bytes_are(kind, &bytes) {
        return Err(ERR_MALFORMED_IMAGE.to_string());
    }

    let name = format!("{}.{}", new_asset_id(), kind.extension);
    // DAS-FR-14: held from here, before a byte of it is on disk. The identifier
    // is opaque and freshly generated (DAS-FR-02), so holding one that then
    // fails to store costs nothing: it names no file, and the hold ends with
    // the session or with the next prompt write.
    hold(&name);
    // DAS-FR-04: the folder is created where a draft predates this module. A
    // folder that cannot be created is `asset_store_failed` — the store was
    // attempted and did not complete — carrying a safe diagnostic category
    // rather than the underlying error's own text (DAS-FR-27).
    if root.file_info(&dir).is_err() {
        crate::drafts::draft_dir(root, draft_id)
            .map_err(|_| ERR_DRAFT_NOT_FOUND.to_string())
            .and_then(|draft_dir| {
                root.create_dir_under(&draft_dir, crate::drafts::ASSETS_DIR)
                    .map_err(|_| typed(ERR_ASSET_STORE_FAILED, category::FOLDER_UNAVAILABLE))
            })?;
    }
    // DAS-FR-04: the bytes are written to a temporary name inside the draft's
    // own `assets/` folder and moved into their final name in one step, which is
    // exactly what the atomic write primitive does — so the folder holds either
    // the whole asset or no entry for it at all. A store interrupted part-way
    // leaves no partial file, no zero-length file, and no entry a later sweep
    // can find: a temporary name is not an accepted image, so a sweep retains it
    // (DAS-FR-12, DAS-FR-16).
    root.write_bytes_atomic(dir.join(&name), &bytes)
        .map_err(|_| typed(ERR_ASSET_STORE_FAILED, category::WRITE_REFUSED))?;

    // DAS-FR-02: the supplied filename is recorded as metadata alone, after the
    // bytes are in place — so a record that will not write leaves a stored
    // asset with no recorded name rather than a failed store.
    let filename = filename
        .map(str::trim)
        .filter(|f| !f.is_empty())
        .map(str::to_string);
    if let Some(supplied) = filename.as_deref() {
        record_filename(root, &dir, &name, supplied);
    }

    Ok(DraftAsset {
        path: format!("{}/{}", crate::drafts::ASSETS_DIR, name),
        // DAS-FR-05: the prompt sits at the root of `files/` and the asset under
        // `assets/`, so the destination that resolves to it is `../assets/<…>`.
        // Returned so no caller derives a path of its own.
        reference: format!("../{}/{}", crate::drafts::ASSETS_DIR, name),
        media_type: media_base(media_type),
        filename,
        bytes: bytes.len() as u64,
    })
}

/// The media type an asset's own filename extension names.
///
/// The extension is the store's own (DAS-FR-02), derived from the media type it
/// accepted, so reading it back is reading what this module wrote rather than
/// trusting a name.
pub(super) fn media_type_of(name: &str) -> Option<&'static str> {
    let extension = name.rsplit_once('.').map(|(_, e)| e)?;
    kind_of_extension(extension).map(|k| k.media_type)
}

/// DAS-FR-02: where the supplied filenames are recorded.
///
/// A file of the folder's own rather than part of an asset's name, because the
/// supplied filename must never reach a path: it cannot collide with a stored
/// asset, cannot direct a write, and cannot carry a path separator or a
/// traversal segment. It carries no accepted image extension, so a sweep
/// retains it exactly as it retains anything else it does not recognise
/// (DAS-FR-12, DAS-FR-16), and it is one more ordinary file in an ordinary
/// folder an author can read with a text editor.
pub(super) const FILENAMES_FILE: &str = "filenames.toml";

/// The recorded filenames of one draft's assets, by stored name.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(super) struct RecordedFilenames {
    #[serde(default)]
    names: std::collections::BTreeMap<String, String>,
}

/// DAS-FR-08: the name one asset was supplied under, or `None` where none was
/// recorded.
///
/// A record that will not parse answers `None` rather than failing the read: a
/// filename is metadata beside the bytes, and losing it costs a caller a label
/// while losing the read would cost them the picture.
pub(super) fn recorded_filename(root: &fs::RootFs, dir: &Path, name: &str) -> Option<String> {
    root.read_toml::<RecordedFilenames>(dir.join(FILENAMES_FILE))
        .ok()?
        .names
        .get(name)
        .cloned()
}

/// DAS-FR-02: record the name an asset was supplied under, beside the folder
/// rather than in the path.
///
/// Best-effort by design. The asset is already stored and readable by the time
/// this runs, so a record that will not write costs the caller a label and
/// nothing else — the shape says `filename` is "absent where none was
/// recorded", and this is that case. Failing the store here would be worse: it
/// would report `asset_store_failed` for an asset that is on disk, which
/// DAS-FR-04 says that value never means.
pub(super) fn record_filename(root: &fs::RootFs, dir: &Path, name: &str, filename: &str) {
    let path = dir.join(FILENAMES_FILE);
    let mut recorded = root
        .read_toml::<RecordedFilenames>(&path)
        .unwrap_or_default();
    recorded.names.insert(name.to_string(), filename.to_string());
    let _ = root.write_toml_atomic(&path, &recorded);
}

/// DAS-FR-08: the stored bytes of one asset, together with its media type.
///
/// The only way an asset's content leaves this module to a surface. A path
/// naming nothing under the draft's `assets/` is `asset_not_found`; a path
/// leaving the draft is `path_escape`, and nothing outside the draft was opened,
/// followed, or requested to reach that answer.
pub fn read_image_impl(
    root: &fs::RootFs,
    draft_id: &str,
    path: &str,
) -> Result<DraftImageContent, String> {
    let dir = assets_dir(root, draft_id)?;
    crate::drafts::require_prompt(root, draft_id)?;
    let name = match resolve_named_path(path) {
        Destination::Asset(name) => name,
        // Inside the draft but not an asset: the caller named a file this module
        // does not serve, and it is not there to be found under `assets/`.
        Destination::InsideDraft => return Err(ERR_ASSET_NOT_FOUND.to_string()),
        Destination::Outside => return Err(ERR_PATH_ESCAPE.to_string()),
    };
    let abs = dir.join(&name);
    // DAS-FR-07: a path reaching **through** a symbolic link out of the draft is
    // an escape the syntactic walk above cannot see, because a link's name is an
    // ordinary path segment. Refused before the file is opened, so the link is
    // never followed and the content of a file the draft does not own can never
    // be handed to a surface.
    match root.file_info(&abs) {
        Ok(info) if info.kind == fs::EntryKind::Symlink => {
            return Err(ERR_PATH_ESCAPE.to_string())
        }
        Ok(info) if info.kind == fs::EntryKind::File => {}
        _ => return Err(ERR_ASSET_NOT_FOUND.to_string()),
    }
    let media_type = media_type_of(&name).ok_or(ERR_ASSET_NOT_FOUND)?;
    let bytes = root
        .read_bytes(&abs)
        .map_err(|_| ERR_ASSET_NOT_FOUND.to_string())?;
    use base64::Engine as _;
    Ok(DraftImageContent {
        media_type: media_type.to_string(),
        // DAS-FR-08: the name it was attached under, where one was recorded.
        filename: recorded_filename(root, &dir, &name),
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

/// DAS-FR-09: remove one asset, and remove it only where the saved prompt holds
/// no valid reference to it.
///
/// The caller's way to take back an insertion that did not finish, and
/// **idempotent**: a path naming nothing is reported discarded having had
/// nothing to discard, so a retried insertion never fails on work already
/// undone. Atomic per file: the asset is gone or it is untouched.
pub fn discard_image_impl(
    root: &fs::RootFs,
    draft_id: &str,
    path: &str,
    sweeper: &DraftAssetSweeper,
) -> Result<DraftAssetDiscarded, String> {
    let dir = assets_dir(root, draft_id)?;
    // DRS-FR-QPSC: a GitHub-shadow draft takes no asset write.
    crate::drafts::require_not_github_shadow(root, draft_id)?;
    let prompt = crate::drafts::require_prompt(root, draft_id)?;
    let name = match resolve_named_path(path) {
        Destination::Asset(name) => name,
        Destination::InsideDraft => {
            // Nothing under `assets/` carries this name, so there is nothing to
            // discard — which is the idempotent answer rather than a failure.
            return Ok(DraftAssetDiscarded { discarded: true, retained: false });
        }
        Destination::Outside => return Err(ERR_PATH_ESCAPE.to_string()),
    };

    // DAS-FR-09 / DAS-FR-11: an asset a saved reference still names is left in
    // place and reported as retained. Every valid reference protects it, not the
    // first, so this asks the whole saved prompt rather than a count.
    //
    // DAS-FR-16: a prompt that cannot be read protects everything, here exactly
    // as it does in a sweep. A set of references this module could not read is
    // not evidence that the asset is unreferenced, and deleting on the strength
    // of it would be the one guess this module refuses to make.
    let referenced = match crate::drafts::load_draft_file_impl(root, draft_id, &prompt) {
        Ok(contents) => read_references(&contents.body).protected.contains(&name),
        Err(_) => true,
    };
    if referenced {
        return Ok(DraftAssetDiscarded { discarded: false, retained: true });
    }

    let abs = dir.join(&name);
    match root.file_info(&abs) {
        // DAS-FR-09: a path naming nothing is reported discarded having had
        // nothing to discard.
        Err(_) => {
            sweeper.release(draft_id, &name);
            Ok(DraftAssetDiscarded { discarded: true, retained: false })
        }
        Ok(info) if info.kind == fs::EntryKind::Symlink => Err(ERR_PATH_ESCAPE.to_string()),
        Ok(_) => match root.delete_under(&dir, &name, false) {
            Ok(()) => {
                sweeper.release(draft_id, &name);
                Ok(DraftAssetDiscarded { discarded: true, retained: false })
            }
            // DAS-FR-28: a removal the filesystem refuses leaves the asset
            // exactly as it was, so the caller may retry it, leave it to the
            // next sweep, or report it.
            Err(_) => Err(typed(ERR_ASSET_CLEANUP_FAILED, category::DELETE_REFUSED)),
        },
    }
}

//! The read path a conversation takes.

use super::*;

// ---------------------------------------------------------------------------
// The read path a conversation takes (DAS-FR-22)
// ---------------------------------------------------------------------------

/// DAS-FR-22: the images the **saved** prompt uses, in the order they occur in
/// it, carrying **the bytes themselves** rather than a handle to fetch them by.
///
/// The one path by which asset bytes reach a conversation (per
/// `AGC-agent-conversations.md` AGC-FR-35). It **refuses nothing**: an
/// unresolved, external, or escaping destination is returned unresolved rather
/// than raised as an error, so one bad path in a prompt costs the caller that
/// one image and nothing else, and no address is ever requested. It writes
/// nothing, emits nothing, and is registered as no Tauri command.
pub fn read_prompt_images(root: &fs::RootFs, draft_id: &str) -> Vec<DraftPromptImage> {
    let Ok(prompt) = crate::drafts::require_prompt(root, draft_id) else {
        return Vec::new();
    };
    let Ok(contents) = crate::drafts::load_draft_file_impl(root, draft_id, &prompt) else {
        return Vec::new();
    };
    read_prompt_images_of(root, draft_id, &contents.body)
}

/// DAS-FR-22 over a prompt the caller **already holds**.
///
/// The offsets an entry carries are offsets into the text they were read from,
/// and the one consumer slices that text by them to interleave a conversation's
/// material (per `AGC-agent-conversations.md` AGC-FR-35). A second read of the
/// file would be a second set of bytes: a prompt written between the two — an
/// autosave, an accepted proposal — would leave the caller slicing one document
/// by another's offsets. Taking the text the caller has is what makes that
/// impossible rather than merely unlikely.
pub fn read_prompt_images_of(
    root: &fs::RootFs,
    draft_id: &str,
    markdown: &str,
) -> Vec<DraftPromptImage> {
    let Ok(dir) = crate::drafts::draft_assets_dir(root, draft_id) else {
        return Vec::new();
    };
    read_references(markdown)
        .images
        .into_iter()
        .map(|used| resolve_prompt_image(root, &dir, used))
        .collect()
}

/// One image use, resolved and read.
pub(super) fn resolve_prompt_image(root: &fs::RootFs, dir: &Path, used: ImageUse) -> DraftPromptImage {
    let context = image_context(&used.alt, &used.reference);
    // DAS-FR-22: an entry whose destination resolved to no draft-owned asset is
    // still returned in its own position with its context intact, so the caller
    // carries safe metadata where the picture would have been rather than
    // dropping the fact that one is there.
    let unresolved = DraftPromptImage {
        reference: used.reference.clone(),
        alt: used.alt.clone(),
        context: context.clone(),
        resolved: false,
        asset_path: None,
        media_type: None,
        filename: None,
        data: None,
        start: used.start,
        end: used.end,
    };
    let Destination::Asset(name) = resolve_destination(&used.reference) else {
        return unresolved;
    };
    let Some(media_type) = media_type_of(&name) else {
        return unresolved;
    };
    let abs = dir.join(&name);
    // A symbolic link is not this draft's own asset, whatever it points at, and
    // it is never followed here (DAS-FR-16).
    match root.file_info(&abs) {
        Ok(info) if info.kind == fs::EntryKind::File => {}
        _ => return unresolved,
    }
    // DAS-FR-22: an asset whose bytes cannot be read is returned unresolved on
    // exactly these terms, one unreadable file costing the caller nothing but
    // that one image.
    let Ok(bytes) = root.read_bytes(&abs) else {
        return unresolved;
    };
    use base64::Engine as _;
    DraftPromptImage {
        resolved: true,
        asset_path: Some(format!("{}/{}", crate::drafts::ASSETS_DIR, name)),
        media_type: Some(media_type.to_string()),
        // DAS-FR-22: the asset's supplied filename where one was recorded, and
        // absent where none was.
        filename: recorded_filename(root, dir, &name),
        data: Some(base64::engine::general_purpose::STANDARD.encode(bytes)),
        ..unresolved
    }
}

/// DAS-FR-22: the text that identifies an image to a reader — its Markdown
/// reference together with its alt text.
///
/// Written as the Markdown itself, because that is what the author wrote and
/// what they will look for: a model reading `![flow diagram](../assets/a.png)`
/// beside a picture learns both what the prompt called it and where the prompt
/// put it, and an author whose destination is broken reads the same string back
/// in the conversation.
pub(super) fn image_context(alt: &str, reference: &str) -> String {
    format!("![{alt}]({reference})")
}

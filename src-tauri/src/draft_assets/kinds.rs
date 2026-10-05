//! The typed error vocabulary and the image kinds this module stores.

// ---------------------------------------------------------------------------
// The typed error vocabulary (DAS-FR-27)
// ---------------------------------------------------------------------------

/// DAS-FR-27: no project is open, so nothing was read or written.
pub const ERR_NO_PROJECT_OPEN: &str = "no_project_open";
/// DAS-FR-27: `draft_id` names no draft in the active worktree.
pub const ERR_DRAFT_NOT_FOUND: &str = "draft_not_found";
/// DAS-FR-03: the media type is outside the accepted image set.
pub const ERR_UNSUPPORTED_MEDIA_TYPE: &str = "unsupported_media_type";
/// DAS-FR-03: the decoded content is beyond the size bound.
pub const ERR_IMAGE_TOO_LARGE: &str = "image_too_large";
/// DAS-FR-03: the data is not valid base64, carries no derivable extension, or
/// its bytes are not the image kind the media type claims.
pub const ERR_MALFORMED_IMAGE: &str = "malformed_image";
/// DAS-FR-04: the store was attempted and did not complete.
pub const ERR_ASSET_STORE_FAILED: &str = "asset_store_failed";
/// DAS-FR-08: the path named nothing under the draft's `assets/`.
pub const ERR_ASSET_NOT_FOUND: &str = "asset_not_found";
/// DAS-FR-07: the path was not the draft's to reach.
pub const ERR_PATH_ESCAPE: &str = "path_escape";
/// DAS-FR-09 / DAS-FR-28: a removal was attempted and refused.
pub const ERR_ASSET_CLEANUP_FAILED: &str = "asset_cleanup_failed";

/// DAS-FR-04 / DAS-FR-27: the safe diagnostic categories a failed store or a
/// refused removal carries in place of the underlying error's own text.
///
/// An operating-system error string is where a path outside the draft would
/// otherwise reach a log — `ENOENT: /Users/someone/Secret/…` names a directory
/// this module never opened and has no business recording. A category says what
/// kind of thing went wrong, which is what a reader acts on, and carries nothing
/// the filesystem chose the words for.
pub mod category {
    /// The `assets/` folder could not be created.
    pub const FOLDER_UNAVAILABLE: &str = "folder_unavailable";
    /// The bytes could not be written, or could not be moved into place.
    pub const WRITE_REFUSED: &str = "write_refused";
    /// A delete the filesystem refused.
    pub const DELETE_REFUSED: &str = "delete_refused";
}

/// DAS-FR-27: one typed value carrying its safe diagnostic category, spelled
/// `<value>:<category>`.
///
/// The typed value leads, so every caller that recognises a value recognises it
/// here too — a surface matching `asset_store_failed` reads the same refusal
/// whether or not a category is attached (per `../ui/NAW-new-artifact.md`
/// NAW-FR-51). The category follows for a reader of the log, and both halves
/// are constants of this module: neither is composed from a path, an
/// operating-system message, or anything the author wrote.
pub(super) fn typed(value: &str, category: &str) -> String {
    format!("{value}:{category}")
}

// ---------------------------------------------------------------------------
// What an asset may be (DAS-FR-03)
// ---------------------------------------------------------------------------

/// DAS-FR-03: the bound past which an image is refused, which is the comment
/// attachment bound (per `CMS-comments-storage.md` CMS-FR-47).
///
/// One rule governs how large a picture the application accepts, wherever the
/// author put it, so the same screenshot is either acceptable in both places or
/// in neither.
pub const MAX_IMAGE_BYTES: usize = crate::comments::MAX_ATTACHMENT_BYTES;

/// One image kind this module stores: the media type's base, the filename
/// extension that belongs to it, and how its bytes are recognised.
pub(super) struct ImageKind {
    /// The lower-cased media type base, e.g. `image/png`.
    pub(super) media_type: &'static str,
    /// The extension a stored asset of this kind carries (DAS-FR-02).
    pub(super) extension: &'static str,
}

/// The image kinds DAS-FR-03 accepts.
///
/// A fixed table rather than "anything beginning `image/`", because DAS-FR-03
/// requires two things a bare prefix test cannot give: an extension the store
/// can *derive*, and a check that the bytes are the kind the media type claims.
/// A media type inside the image family this table does not carry is therefore
/// `malformed_image` — the store cannot name the file it would write, and it
/// will not guess.
pub(super) const IMAGE_KINDS: &[ImageKind] = &[
    ImageKind { media_type: "image/png", extension: "png" },
    ImageKind { media_type: "image/jpeg", extension: "jpg" },
    ImageKind { media_type: "image/gif", extension: "gif" },
    ImageKind { media_type: "image/webp", extension: "webp" },
    ImageKind { media_type: "image/bmp", extension: "bmp" },
    ImageKind { media_type: "image/svg+xml", extension: "svg" },
    ImageKind { media_type: "image/avif", extension: "avif" },
    ImageKind { media_type: "image/heic", extension: "heic" },
    ImageKind { media_type: "image/heif", extension: "heif" },
    ImageKind { media_type: "image/tiff", extension: "tiff" },
];

/// A media type's base, lower-cased — so a parameterised `image/png; foo=bar`
/// is the same type as a bare `image/png`.
pub(super) fn media_base(media_type: &str) -> String {
    media_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

/// DAS-FR-03: whether this is an image media type at all.
///
/// The image half of what the comment attachment path accepts (per
/// `CMS-comments-storage.md` CMS-FR-47): a PDF and a plain-text file are
/// attachments a reviewer may point at and are not images a prompt embeds, so
/// they are `unsupported_media_type` here while remaining acceptable there.
pub(super) fn is_image_media_type(media_type: &str) -> bool {
    crate::comments::media_type_accepted(media_type) && media_base(media_type).starts_with("image/")
}

/// The kind this media type names, if this module stores it.
pub(super) fn kind_of(media_type: &str) -> Option<&'static ImageKind> {
    let base = media_base(media_type);
    IMAGE_KINDS.iter().find(|k| k.media_type == base)
}

/// The kind this filename extension names, if this module stores it.
pub(super) fn kind_of_extension(extension: &str) -> Option<&'static ImageKind> {
    let lower = extension.to_ascii_lowercase();
    IMAGE_KINDS.iter().find(|k| k.extension == lower)
}

/// DAS-FR-03: whether `bytes` really are the image kind `kind` claims.
///
/// Signature bytes rather than a decode: what this guards against is a caller
/// labelling arbitrary content `image/png` and having it stored under a name a
/// surface will later hand to an `<img>`, and the signature settles that without
/// this module having to understand any image format. A container that carries
/// its kind in a box header (AVIF, HEIC, HEIF) is read from that header; SVG is
/// text, so it is recognised as the XML it is.
pub(super) fn bytes_are(kind: &ImageKind, bytes: &[u8]) -> bool {
    match kind.extension {
        "png" => bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A]),
        "jpg" => bytes.starts_with(&[0xFF, 0xD8, 0xFF]),
        "gif" => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        "webp" => bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP",
        "bmp" => bytes.starts_with(b"BM"),
        "tiff" => bytes.starts_with(b"II*\0") || bytes.starts_with(b"MM\0*"),
        "avif" => ftyp_brand_is(bytes, &["avif", "avis"]),
        "heic" => ftyp_brand_is(bytes, &["heic", "heix", "hevc", "mif1", "msf1"]),
        "heif" => ftyp_brand_is(bytes, &["heic", "heix", "mif1", "msf1", "heif"]),
        // SVG is a text format, so there is no signature to match — what makes
        // it an SVG is that it parses as XML naming an `<svg>` element. Read
        // over the leading bytes alone: an XML declaration, a doctype, or a
        // comment may precede the root element, and a whole-file scan of a large
        // document would be a scan for a substring that proves nothing.
        "svg" => svg_looks_like_svg(bytes),
        _ => false,
    }
}

/// Whether an ISO base-media-format file's `ftyp` box names one of `brands`.
///
/// The box is `[u32 size][b"ftyp"][major brand][minor version][compatible
/// brands…]`, so the major brand sits at offset 8 and the compatible brands
/// follow from offset 16. Both are read, because an AVIF written by one encoder
/// declares `avif` as its major brand and another declares it only as a
/// compatible one.
pub(super) fn ftyp_brand_is(bytes: &[u8], brands: &[&str]) -> bool {
    if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return false;
    }
    let matches = |chunk: &[u8]| brands.iter().any(|b| chunk == b.as_bytes());
    if matches(&bytes[8..12]) {
        return true;
    }
    // The compatible brands begin at 16, and a file may end before them: a
    // twelve-byte `ftyp` box is a truncated image, not a slice this may index
    // into. Sliced through `get`, because the alternative is a panic on a
    // hostile or merely damaged file — and this runs both on a store the author
    // asked for and inside the housekeeping worker, where a panic would take
    // the pass down mid-sweep.
    bytes
        .get(16..)
        .is_some_and(|tail| tail.chunks_exact(4).any(matches))
}

/// Whether the leading bytes read as an XML document naming an `<svg>` element.
pub(super) fn svg_looks_like_svg(bytes: &[u8]) -> bool {
    // Enough to carry an XML declaration, a doctype, and a comment before the
    // root element, and bounded so a large file costs a fixed read.
    const WINDOW: usize = 4096;
    let head = &bytes[..bytes.len().min(WINDOW)];
    let Ok(text) = std::str::from_utf8(head) else {
        // A truncated multi-byte character at the window edge is not evidence of
        // anything; fall back to the largest valid prefix.
        return match std::str::from_utf8(head) {
            Ok(t) => t.contains("<svg"),
            Err(e) => std::str::from_utf8(&head[..e.valid_up_to()])
                .map(|t| t.contains("<svg"))
                .unwrap_or(false),
        };
    };
    let trimmed = text.trim_start();
    (trimmed.starts_with('<')) && text.contains("<svg")
}

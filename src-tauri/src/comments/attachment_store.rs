//! Staging, storing and reading a comment's attachments.

use super::*;


/// One validated attachment, with the bytes still to be written when it is a
/// freshly-supplied `inline` payload.
///
/// The split into staging and committing is what makes CMS-FR-47's all-or-nothing
/// refusal true: every input in the list is validated and decoded *before* any of
/// them touches the disk, so a list holding one acceptable attachment and one
/// refused stores neither.
pub(super) struct StagedAttachment {
    attachment: Attachment,
    /// `None` for a `url`, and for a `blob` whose digest is already on disk.
    bytes: Option<Vec<u8>>,
}

/// CMS-FR-43 / CMS-FR-47: validate one input and decode it, writing nothing.
pub(super) fn stage_attachment(
    root: &crate::fs::RootFs,
    scope: LogScope<'_>,
    input: AttachmentInput,
) -> Result<StagedAttachment, String> {
    match input {
        AttachmentInput::Url {
            url,
            media_type,
            label,
        } => {
            if !media_type_accepted(&media_type) {
                return Err(ERR_UNSUPPORTED_MEDIA_TYPE.to_string());
            }
            // Well-formed enough to be an address this application would ever
            // render. Deliberately not a fetch: CMS-FR-43 records a `url`
            // verbatim and this module makes no network request of any kind.
            let trimmed = url.trim();
            if trimmed.is_empty()
                || !(trimmed.starts_with("http://") || trimmed.starts_with("https://"))
            {
                return Err(ERR_MALFORMED_ATTACHMENT.to_string());
            }
            Ok(StagedAttachment {
                attachment: Attachment::Url {
                    url: trimmed.to_string(),
                    media_type,
                    label: label.filter(|l| !l.trim().is_empty()),
                },
                bytes: None,
            })
        }
        AttachmentInput::Inline {
            media_type,
            filename,
            data,
        } => {
            if !media_type_accepted(&media_type) {
                return Err(ERR_UNSUPPORTED_MEDIA_TYPE.to_string());
            }
            if filename.trim().is_empty() {
                return Err(ERR_MALFORMED_ATTACHMENT.to_string());
            }
            use base64::Engine as _;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(data.as_bytes())
                .map_err(|_| ERR_MALFORMED_ATTACHMENT.to_string())?;
            // Bounded on the *decoded* length, which is what lands in the
            // repository — base64 inflates by a third and bounding the encoded
            // form would refuse files under the stated limit.
            if bytes.len() > MAX_ATTACHMENT_BYTES {
                return Err(ERR_ATTACHMENT_TOO_LARGE.to_string());
            }
            let digest = fsa::sha256_bytes(&bytes);
            let path = scope.attachment_path(root, &digest)?;
            // CMS-FR-44: byte-identical content is one file. An existing digest
            // is left untouched rather than rewritten — the bytes cannot differ,
            // and not writing keeps a re-attach free.
            let already = path.exists();
            Ok(StagedAttachment {
                attachment: Attachment::Blob {
                    digest,
                    media_type,
                    filename,
                    bytes: bytes.len() as u64,
                },
                bytes: if already { None } else { Some(bytes) },
            })
        }
    }
}

/// CMS-FR-45 / CMS-FR-47: stage every input, then write the bytes.
///
/// Returns the stored attachments in the order they were supplied. Every byte is
/// on disk when this returns, which is what lets the caller append the line
/// afterwards knowing no folded comment can name a missing blob.
pub(super) fn store_attachments(
    root: &fsa::RootFs,
    scope: LogScope<'_>,
    inputs: Vec<AttachmentInput>,
) -> Result<Vec<Attachment>, String> {
    if inputs.is_empty() {
        return Ok(Vec::new());
    }
    let staged: Vec<StagedAttachment> = inputs
        .into_iter()
        .map(|input| stage_attachment(root, scope, input))
        .collect::<Result<_, _>>()?;
    let mut out = Vec::with_capacity(staged.len());
    for item in staged {
        if let Some(bytes) = item.bytes {
            let Attachment::Blob { ref digest, .. } = item.attachment else {
                unreachable!("only a blob stages bytes");
            };
            let path = scope.attachment_path(root, digest)?;
            root.write_bytes_atomic(&path, &bytes).map_err(|e| e.to_string())?;
        }
        out.push(item.attachment);
    }
    Ok(out)
}

/// CMS-FR-48: the stored bytes of a `blob` in this scope, or `attachment_not_found`.
///
/// A `url` attachment is not served here, because this module holds no content
/// for one — the digest simply names no file in the scope's folder.
pub(super) fn read_attachment_in(
    root: &fsa::RootFs,
    scope: LogScope<'_>,
    thread: &Discussion,
    digest: &str,
) -> Result<AttachmentContent, String> {
    // The thread is what names the media type and filename to serve back: the
    // stored file is bytes alone, content-addressed and therefore shared, so the
    // name it was attached under lives in the log rather than beside the bytes.
    let found = thread.comments.iter().flat_map(|c| &c.attachments).find_map(
        |a| match a {
            Attachment::Blob {
                digest: d,
                media_type,
                filename,
                ..
            } if d == digest => Some((media_type.clone(), filename.clone())),
            _ => None,
        },
    );
    let (media_type, filename) = found.ok_or(ERR_ATTACHMENT_NOT_FOUND)?;
    let path = scope.attachment_path(root, digest)?;
    let bytes = root.read_bytes(&path).map_err(|_| ERR_ATTACHMENT_NOT_FOUND.to_string())?;
    use base64::Engine as _;
    Ok(AttachmentContent {
        media_type,
        filename,
        data: base64::engine::general_purpose::STANDARD.encode(bytes),
    })
}

/// CMS-FR-67: a stored `blob` served to a caller **inside the backend**, on
/// exactly the terms `read_comment_attachment` serves one to a surface
/// (CMS-FR-48).
///
/// The same scope resolution from the thread id (CMS-FR-49), the same bytes,
/// media type, and filename, and the same typed `attachment_not_found` for a
/// digest that scope does not carry. It serves neither a `url`, a `proposal`,
/// nor a `prompt_proposal` — this module holds content for none of them — and it
/// fetches nothing: an address stays an address here as everywhere (CMS-FR-43).
///
/// Its one consumer is the conversational input assembly of
/// `AGC-agent-conversations.md` (AGC-FR-35), which reads an image a comment
/// carries so an agent can be shown it rather than told about it. It exists so
/// that reading an attachment for a model and reading one for a card are one
/// path with one set of rules rather than two.
///
/// Registered as no Tauri command, so it widens nothing a frontend call can
/// reach (CMS-FR-67, CMS-FR-48, CMS-FR-49, CMS-FR-43).
pub fn read_attachment_bytes(
    root: &crate::fs::RootFs,
    worktree: &crate::fs::RootFs,
    thread_id: &str,
    digest: &str,
    draft_id: Option<&str>,
) -> Result<AttachmentContent, String> {
    let thread = locate_discussion_for_attachment(root, worktree, thread_id, draft_id)
        .ok_or(ERR_DISCUSSION_NOT_FOUND)?;
    read_attachment_in(root, attachment_scope(&thread), &thread, digest)
}

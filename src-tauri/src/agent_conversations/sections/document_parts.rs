//! AGC-FR-VQWY, AGC-FR-GADC, AGC-FR-XEGG: the parts a PDF or DOCX attachment
//! contributes to the section that holds its comment.

use super::*;
use crate::doc_extraction::{DocBlock, DocumentKind};

/// The parts of one document attachment (AGC-FR-VQWY).
pub(in crate::agent_conversations) struct DocumentParts {
    pub parts: Vec<InputPart>,
}

/// AGC-FR-VQWY: a label line, the blocks in document order, and an end line.
///
/// `None` for an attachment that is not a document `blob`, and for a document
/// whose read fails, which is then metadata as any other `blob` (AGC-FR-XEGG).
pub(in crate::agent_conversations) fn document_parts(
    roots: Roots<'_>,
    thread: &comments::Discussion,
    comment: &comments::Comment,
    attachment: &comments::Attachment,
) -> Option<DocumentParts> {
    let comments::Attachment::Blob { digest, media_type, filename, .. } = attachment else {
        return None;
    };
    DocumentKind::of_media_type(media_type)?;
    let document = comments::read_attachment_document(
        roots.store,
        roots.worktree,
        &thread.id,
        digest,
        thread.draft_id(),
    )
    .ok()?;
    let handle = handle_of(&comment.author);
    let mut parts = vec![InputPart::Text(format!("[attachment: {filename} ({media_type})]\n"))];
    let image_line = |ordinal: usize, image_type: &str| {
        format!("[image {ordinal} of {filename} ({image_type}), not shown]\n")
    };
    for block in &document.blocks {
        match block {
            DocBlock::Text(text) => {
                let mut text = text.clone();
                if !text.ends_with('\n') {
                    text.push('\n');
                }
                parts.push(InputPart::Text(text));
            }
            // AGC-FR-GADC: an image part with its bytes, its media type, and
            // a context that names the document, the ordinal, and the comment.
            DocBlock::Image { media_type: image_type, data, ordinal } => {
                use base64::Engine as _;
                parts.push(InputPart::Image(InputImage {
                    data: base64::engine::general_purpose::STANDARD.encode(data),
                    media_type: image_type.clone(),
                    context: format!(
                        "[image {ordinal} of {filename} ({image_type}), on the message @{handle} posted (comment {})]",
                        comment.id
                    ),
                    metadata: image_line(*ordinal, image_type),
                }));
            }
            // AGC-FR-GADC: text in its position on every endpoint.
            DocBlock::ImageMetadata { media_type: image_type, ordinal } => {
                parts.push(InputPart::Text(image_line(*ordinal, image_type)));
            }
        }
    }
    parts.push(InputPart::Text(format!("[end of attachment: {filename}]\n")));
    Some(DocumentParts { parts })
}

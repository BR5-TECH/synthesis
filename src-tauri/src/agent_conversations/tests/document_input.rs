//! Document attachments in a turn's input (AGC-FR-09, AGC-FR-VQWY,
//! AGC-FR-GADC, AGC-FR-XEGG, AGC-FR-XDNX).

use super::images::{encoded, image_parts, open_draft_discussion};
use super::*;
use crate::comments::{AttachmentInput, Discussion, Stamp, ThreadRef};
use crate::doc_extraction::tests::fixtures::{self as fx, DocxItem, PdfImage};
use crate::doc_extraction::{cache, DOCX_MEDIA_TYPE};

fn docx_input(filename: &str, bytes: &[u8]) -> AttachmentInput {
    AttachmentInput::Inline {
        media_type: DOCX_MEDIA_TYPE.into(),
        filename: filename.into(),
        data: encoded(bytes),
    }
}

fn pdf_input(filename: &str, bytes: &[u8]) -> AttachmentInput {
    AttachmentInput::Inline {
        media_type: "application/pdf".into(),
        filename: filename.into(),
        data: encoded(bytes),
    }
}

fn draft(h: &Harness, name: &str) -> String {
    crate::drafts::create_draft_at_root(&h.root(), Some(name)).expect("draft").draft.id
}

fn post(
    h: &Harness,
    draft_id: &str,
    thread: &Discussion,
    body: &str,
    attachments: Vec<AttachmentInput>,
    at: &str,
) -> Discussion {
    crate::comments::add_comment_to(
        &h.root(),
        ThreadRef::discussion(draft_id),
        &thread.id,
        body.into(),
        Vec::new(),
        attachments,
        Stamp { by: &human("ada"), at },
    )
    .expect("posted")
}

fn last_id(thread: &Discussion) -> String {
    thread.comments.last().expect("a comment").id.clone()
}

/// The DOCX of AGC-FR-VQWY: text, one picture, text.
fn spec_docx(marker: &str) -> (Vec<u8>, Vec<u8>) {
    let picture = fx::png(64, 64);
    let bytes = fx::docx(&[
        fx::para(&format!("Alpha {marker}")),
        DocxItem::Image(picture.clone()),
        fx::para(&format!("Omega {marker}")),
    ]);
    (bytes, picture)
}

fn position(body: &str, needle: &str) -> usize {
    body.find(needle).unwrap_or_else(|| panic!("{needle:?} is not in:\n{body}"))
}

// AGC-FR-VQWY, AGC-FR-GADC, AGC-FR-09: the document on the trigger gives a
// label line, its text and its image in document order, and an end line, in
// the current comment after the prose. The image part carries its bytes, its
// media type, and a context that names the document, the ordinal, and the
// comment.
#[test]
fn a_document_on_the_trigger_gives_its_text_and_images_in_place() {
    let h = Harness::new(vec![]);
    let draft_id = draft(&h, "doc-trigger");
    let thread = open_draft_discussion(&h, &draft_id, "opening");
    let (bytes, picture) = spec_docx("ZQX-trigger");
    let thread = post(
        &h,
        &draft_id,
        &thread,
        "@arch read this",
        vec![docx_input("spec.docx", &bytes)],
        "2026-01-01T00:00:01Z",
    );
    let trigger = last_id(&thread);
    let root = h.root();
    let sections = build_input(Roots::same(&root), &ConversationOrigin::of(&thread), &trigger);
    let current = section_of(&sections, TAG_CURRENT_COMMENT);
    let body = &current.body;
    let order = [
        "@arch read this",
        &format!("[attachment: spec.docx ({DOCX_MEDIA_TYPE})]"),
        "Alpha ZQX-trigger",
        "[image 1 of spec.docx (image/png), not shown]",
        "Omega ZQX-trigger",
        "[end of attachment: spec.docx]",
    ];
    let found: Vec<usize> = order.iter().map(|needle| position(body, needle)).collect();
    assert!(found.windows(2).all(|w| w[0] < w[1]), "in document order: {found:?}\n{body}");
    let images = image_parts(current);
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].data, encoded(&picture));
    assert_eq!(images[0].media_type, "image/png");
    for needle in ["image 1 of spec.docx", "@ada", trigger.as_str()] {
        assert!(images[0].context.contains(needle), "{needle}: {}", images[0].context);
    }
    assert!(!current.truncated);
}

// AGC-FR-09, AGC-FR-VQWY: a document on an earlier comment is in
// `discussion_history`, and not in the current comment.
#[test]
fn a_document_on_an_earlier_comment_is_in_the_history() {
    let h = Harness::new(vec![]);
    let draft_id = draft(&h, "doc-history");
    let thread = open_draft_discussion(&h, &draft_id, "opening");
    let bytes = fx::text_pdf("ZQX-history-text");
    let thread = post(
        &h,
        &draft_id,
        &thread,
        "a report",
        vec![pdf_input("r.pdf", &bytes)],
        "2026-01-01T00:00:01Z",
    );
    let thread = post(&h, &draft_id, &thread, "@arch thoughts?", vec![], "2026-01-01T00:00:02Z");
    let root = h.root();
    let sections =
        build_input(Roots::same(&root), &ConversationOrigin::of(&thread), &last_id(&thread));
    let history = section_of(&sections, TAG_DISCUSSION_HISTORY);
    assert!(history.body.contains("[attachment: r.pdf (application/pdf)]"));
    assert!(history.body.contains("ZQX-history-text"));
    assert!(!section_of(&sections, TAG_CURRENT_COMMENT).body.contains("ZQX-history-text"));
}

// AGC-FR-XEGG: a posted document that no longer extracts is the metadata of a
// non-image blob, and the input is still built.
#[test]
fn a_document_that_no_longer_extracts_is_metadata() {
    let h = Harness::new(vec![]);
    let draft_id = draft(&h, "doc-broken");
    let thread = open_draft_discussion(&h, &draft_id, "opening");
    let bytes = fx::text_pdf("ZQX-broken-later");
    let thread = post(
        &h,
        &draft_id,
        &thread,
        "@arch this",
        vec![pdf_input("f.pdf", &bytes)],
        "2026-01-01T00:00:01Z",
    );
    let digest = crate::fs::sha256_bytes(&bytes);
    let blob = walkdir_find(h.root().path(), &digest).expect("the stored blob");
    cache::clear_for_test();
    std::fs::write(blob, b"garbage").unwrap();
    let root = h.root();
    let sections =
        build_input(Roots::same(&root), &ConversationOrigin::of(&thread), &last_id(&thread));
    let current = section_of(&sections, TAG_CURRENT_COMMENT);
    assert!(current.body.contains("[attachment: f.pdf (application/pdf), not shown]"));
    assert!(!current.body.contains("ZQX-broken-later"));
}

fn walkdir_find(dir: &std::path::Path, name: &str) -> Option<std::path::PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = walkdir_find(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}

// AGC-FR-XDNX: document text on the current comment is not cut by the
// 40,000-character bound, whether the rest of the comment is short, near the
// bound, or above it. The rest of the comment is cut at the bound.
#[test]
fn document_text_on_the_current_comment_is_exempt_from_the_bound() {
    let h = Harness::new(vec![]);
    let draft_id = draft(&h, "doc-bound");
    let thread = open_draft_discussion(&h, &draft_id, "opening");
    let document = format!("{}ZQX-document-end", "d".repeat(100_000));
    let bytes = fx::docx(&[fx::para(&document)]);
    let root = h.root();
    let cases = vec![
        ("@arch short".to_string(), false),
        ("x".repeat(39_990), false),
        ("y".repeat(40_500), true),
    ];
    let mut thread = thread;
    for (i, (body, cut)) in cases.into_iter().enumerate() {
        thread = post(
            &h,
            &draft_id,
            &thread,
            &body,
            vec![docx_input("long.docx", &bytes)],
            &format!("2026-01-01T00:00:0{}Z", i + 1),
        );
        let sections =
            build_input(Roots::same(&root), &ConversationOrigin::of(&thread), &last_id(&thread));
        let current = section_of(&sections, TAG_CURRENT_COMMENT);
        assert!(current.body.contains(&document), "case {i}: the whole document text");
        assert!(current.body.contains("[end of attachment: long.docx]"), "case {i}");
        assert_eq!(current.truncated, cut, "case {i}");
        if cut {
            assert!(!current.body.contains(&"y".repeat(40_001)), "the body is cut at the bound");
        }
    }
}

// AGC-FR-XDNX: in `discussion_history`, document text counts against the
// bound, so an earlier comment whose document is above it is left out.
#[test]
fn document_text_in_the_history_counts_against_its_bound() {
    let h = Harness::new(vec![]);
    let draft_id = draft(&h, "doc-history-bound");
    let thread = open_draft_discussion(&h, &draft_id, "opening");
    let bytes = fx::docx(&[fx::para(&format!("{}ZQX-long-history", "h".repeat(60_000)))]);
    let thread = post(
        &h,
        &draft_id,
        &thread,
        "a long one",
        vec![docx_input("long.docx", &bytes)],
        "2026-01-01T00:00:01Z",
    );
    let thread = post(&h, &draft_id, &thread, "a short one", vec![], "2026-01-01T00:00:02Z");
    let thread = post(&h, &draft_id, &thread, "@arch now", vec![], "2026-01-01T00:00:03Z");
    let root = h.root();
    let history = discussion_history_section(Roots::same(&root), &thread, &last_id(&thread));
    assert!(history.truncated);
    assert!(!history.body.contains("ZQX-long-history"));
    assert!(history.body.contains("a short one"));
}

/// A thread whose trigger carries the DOCX of [`spec_docx`].
fn dispatch_spec_docx(h: &Harness, name: &str, marker: &str) -> (AgentTurn, Vec<u8>) {
    let draft_id = draft(h, name);
    h.create_agent("arch", "Argue about structure.");
    let thread = open_draft_discussion(h, &draft_id, "opening");
    let (bytes, picture) = spec_docx(marker);
    let thread = post(
        h,
        &draft_id,
        &thread,
        "@arch read this",
        vec![docx_input("spec.docx", &bytes)],
        "2026-01-01T00:00:01Z",
    );
    let dispatched =
        h.dispatch("arch", ConversationOrigin::of(&thread), &last_id(&thread)).expect("dispatched");
    (wait_for_terminal(h, &dispatched.id), picture)
}

// AGC-FR-GADC, AGC-FR-36: on an endpoint that accepts images, the document's
// image is image content of the request, after its context.
#[test]
fn an_image_capable_turn_carries_the_document_image() {
    use rig::completion::message::{DocumentSourceKind, UserContent};
    let h = Harness::new(vec![Ok("Answered.".into())]);
    h.seed_image_capable_provider();
    let (turn, picture) = dispatch_spec_docx(&h, "doc-capable", "ZQX-capable");
    assert_eq!(turn.state, AgentTurnState::Delivered);
    assert!(!turn.images_omitted);
    let exchange = h.seam.exchanges();
    let rig::completion::Message::User { content } = &exchange[0][0] else {
        panic!("the input is a user message");
    };
    let images: Vec<_> = content
        .iter()
        .filter_map(|c| match c {
            UserContent::Image(image) => Some(image.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(images.len(), 1);
    assert_eq!(images[0].data, DocumentSourceKind::Base64(encoded(&picture)));
}

// AGC-FR-GADC, AGC-FR-37: on an endpoint with no image input, the turn sends
// the metadata in place, no bytes, and records that it omitted images. It is
// an ordinary successful turn.
#[test]
fn a_text_only_turn_sends_the_document_image_as_metadata() {
    let h = Harness::new(vec![Ok("Answered.".into())]);
    h.seed_verified_provider();
    let (turn, picture) = dispatch_spec_docx(&h, "doc-text-only", "ZQX-text-only");
    assert_eq!(turn.state, AgentTurnState::Delivered);
    assert!(turn.failure.is_none());
    assert!(turn.images_omitted);
    let wire = format!("{:?}", h.seam.exchanges());
    assert!(!wire.contains(&encoded(&picture)), "no bytes reached the request");
    assert!(wire.contains("[image 1 of spec.docx (image/png), not shown]"));
    assert!(wire.contains("Alpha ZQX-text-only"));
}

// AGC-FR-GADC: a metadata block is text on every endpoint, so a document whose
// only image has no accepted encoding does not make the turn carry images.
#[test]
fn a_metadata_block_does_not_make_the_turn_carry_images() {
    let h = Harness::new(vec![Ok("Answered.".into())]);
    h.seed_verified_provider();
    h.create_agent("arch", "Argue about structure.");
    let draft_id = draft(&h, "doc-metadata");
    let thread = open_draft_discussion(&h, &draft_id, "opening");
    let bytes = fx::pdf(&[fx::page(&["ZQX-metadata-only"], &[PdfImage::Jpx(80, 80)])]);
    let thread = post(
        &h,
        &draft_id,
        &thread,
        "@arch read this",
        vec![pdf_input("m.pdf", &bytes)],
        "2026-01-01T00:00:01Z",
    );
    let dispatched =
        h.dispatch("arch", ConversationOrigin::of(&thread), &last_id(&thread)).expect("dispatched");
    let turn = wait_for_terminal(&h, &dispatched.id);
    assert!(!turn.images_omitted);
    let wire = format!("{:?}", h.seam.exchanges());
    assert!(wire.contains("[image 1 of m.pdf (image/jp2), not shown]"));
}

// AGC-FR-XEGG: a turn whose document no longer extracts is an ordinary
// successful turn, with the document as metadata.
#[test]
fn a_turn_with_a_document_that_no_longer_extracts_is_delivered() {
    let h = Harness::new(vec![Ok("Answered.".into())]);
    h.seed_verified_provider();
    h.create_agent("arch", "Argue about structure.");
    let draft_id = draft(&h, "doc-broken-turn");
    let thread = open_draft_discussion(&h, &draft_id, "opening");
    let bytes = fx::text_pdf("ZQX-broken-turn");
    let thread = post(
        &h,
        &draft_id,
        &thread,
        "@arch this",
        vec![pdf_input("t.pdf", &bytes)],
        "2026-01-01T00:00:01Z",
    );
    let blob = walkdir_find(h.root().path(), &crate::fs::sha256_bytes(&bytes)).expect("blob");
    cache::clear_for_test();
    std::fs::write(blob, b"garbage").unwrap();
    let dispatched =
        h.dispatch("arch", ConversationOrigin::of(&thread), &last_id(&thread)).expect("dispatched");
    let turn = wait_for_terminal(&h, &dispatched.id);
    assert_eq!(turn.state, AgentTurnState::Delivered);
    assert!(turn.failure.is_none());
    let wire = format!("{:?}", h.seam.exchanges());
    assert!(wire.contains("[attachment: t.pdf (application/pdf), not shown]"));
}

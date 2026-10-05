//! The messages a request opens with, and the pictures they may carry.
//!
//! One rule builds the opening exchange (CVL-FR-12), the stable head that a
//! cache marker is placed at (CVL-FR-39), and the multimodal message of
//! CVL-FR-10 — so the head and the message it is a prefix of cannot drift.

use super::*;

/// CVL-FR-12: the exchange a turn opens with — the input as its single first
/// message, before any reply or tool result has been appended to it.
///
/// `with_images` is the decision of AGC-FR-36, already taken from the resolved
/// endpoint's own capability and **all or nothing for the turn**: true builds
/// the multimodal message of CVL-FR-10, false builds the text-and-metadata one
/// of AGC-FR-37. No request is ever built with some of its images and not
/// others.
pub(super) fn opening_exchange(request: &AgentRequest, with_images: bool) -> Vec<rig::completion::Message> {
    if with_images {
        if let Some(message) = multimodal_user_message(request) {
            return vec![message];
        }
        // AGC-FR-36 is all or nothing, and the caller has already recorded that
        // this turn carries its images. Reaching here would mean sending the
        // text-and-metadata request while reporting the opposite, so it is a
        // condition to know about rather than one to paper over. It is
        // unreachable: `multimodal_user_message` answers `None` only where the
        // input holds no image, and `with_images` is false in that case.
        debug_assert!(
            !input_has_images(&request.input),
            "a turn carrying images must build the multimodal request",
        );
    }
    vec![rig::completion::Message::user(render_input(request))]
}

/// CVL-FR-39: the text of the input's **stable head**, or `None` where the
/// material has none to mark apart.
///
/// The head is returned as the text itself rather than as a character offset,
/// and the bridge locates it as a prefix of the message it carries. That is what
/// makes the split safe: the head is produced by the **same renderer** the
/// message itself came from, over the first `stable_head_sections` sections
/// alone, so it is a real prefix by construction — and a build where the two
/// ever disagreed would lose a mark rather than cut a message in the wrong
/// place.
///
/// `None` where the head is the whole input as well as where it is nothing: a
/// boundary at the end of a message is the mark that already stands there, and
/// marking it twice would spend a breakpoint on nothing.
pub(super) fn stable_head_text(
    request: &AgentRequest,
    exchange: &[rig::completion::Message],
) -> Option<String> {
    let sections = request.stable_head_sections;
    if sections == 0 || sections >= request.input.len() {
        return None;
    }
    let head = &request.input[..sections];
    // The head is rendered through whichever renderer produced the message it
    // must be a prefix of (CVL-FR-37): an opening message carrying image content
    // was built by `multimodal_user_message`, and one that is prose alone by
    // `render_input`.
    let opening_is_multimodal = matches!(
        exchange.first(),
        Some(rig::completion::Message::User { content })
            if content.iter().any(|part| matches!(
                part,
                rig::completion::message::UserContent::Image(_)
            ))
    );
    let message = if opening_is_multimodal {
        multimodal_message_of(head)
            .unwrap_or_else(|| rig::completion::Message::user(render_sections(head)))
    } else {
        rig::completion::Message::user(render_sections(head))
    };
    let text = openrouter_bridge::user_message_text(&message);
    (!text.is_empty()).then_some(text)
}

/// CVL-FR-39: how many leading sections of a conversation's input are its
/// stable head.
///
/// The head is what the next turn of this conversation presents again unchanged:
/// the artifact, the draft, or the note under discussion. What follows it is the
/// discussion itself, which grows by a comment each turn, and the current
/// comment, which is new every turn — so the head stops at the first of those
/// two, wherever the builder put it. Read from the tags rather than from a fixed
/// count, because a section the material did not supply is not pushed at all
/// (AGC-FR-05) and a count would then name the wrong boundary.
pub(super) fn stable_head_of(input: &[InputSection]) -> usize {
    input
        .iter()
        .take_while(|section| {
            section.tag != TAG_DISCUSSION_HISTORY && section.tag != TAG_CURRENT_COMMENT
        })
        .count()
}

/// AGC-FR-35: whether an assembled input holds a picture at all.
///
/// What the decision of AGC-FR-36 is taken against: a turn whose material holds
/// no image sends the request a text-only turn always produced, and omits
/// nothing whatever the endpoint takes (CVL-FR-38).
pub fn input_has_images(input: &[InputSection]) -> bool {
    input
        .iter()
        .flat_map(|section| &section.parts)
        .any(|part| matches!(part, InputPart::Image(_)))
}

/// CVL-FR-10 / CVL-FR-37: the first user message as `rig`'s own **ordered
/// content parts**.
///
/// Each section's parts become text and image content **in the order the section
/// holds them**, so a provider receives a picture as the framework's image
/// content rather than as an encoded string inside prose, and the wire format is
/// the framework's affair exactly as it is for text. The tag scaffolding
/// `render_input` writes is preserved around each section, and consecutive text
/// is coalesced into one content part, so an image sits between the prose that
/// precedes it and the prose that follows it rather than between two empty
/// strings.
///
/// The context an image carries — its Markdown reference and alt text, or its
/// filename and the comment it belongs to — is text **beside it in that same
/// position**, so a model reads a picture where the material put it and reads
/// what the material said about it. An image is never re-encoded into prose,
/// never replaced by its filename, and never moved to the head or the foot of
/// the input.
///
/// `None` where the input holds no image, which is where the ordinary text
/// message is exactly right and building a content list would be ceremony.
fn multimodal_user_message(request: &AgentRequest) -> Option<rig::completion::Message> {
    multimodal_message_of(&request.input)
}

/// The same, over the sections themselves, for the reason [`render_sections`]
/// stands apart from [`render_input`].
fn multimodal_message_of(sections: &[InputSection]) -> Option<rig::completion::Message> {
    use rig::completion::message::UserContent;
    use rig::OneOrMany;

    if !input_has_images(sections) {
        return None;
    }
    let mut contents: Vec<UserContent> = Vec::new();
    let mut text = String::new();
    let flush = |text: &mut String, contents: &mut Vec<UserContent>| {
        if !text.is_empty() {
            contents.push(UserContent::text(std::mem::take(text)));
        }
    };
    for (index, section) in sections.iter().enumerate() {
        if index > 0 {
            text.push_str("\n\n");
        }
        text.push('<');
        text.push_str(&section.tag);
        for (name, value) in &section.attributes {
            text.push_str(&format!(" {name}=\"{}\"", escape_attribute(value)));
        }
        text.push_str(">\n");
        if section.parts.is_empty() {
            text.push_str(&section.body);
            if !section.body.ends_with('\n') {
                text.push('\n');
            }
        } else {
            for part in &section.parts {
                match part {
                    InputPart::Text(part) => text.push_str(part),
                    InputPart::Image(image) => {
                        // The context stands beside the image, at the end of the
                        // prose that runs up to it, so the two reach the model
                        // in one position rather than two.
                        text.push_str(&image.context);
                        text.push('\n');
                        flush(&mut text, &mut contents);
                        contents.push(UserContent::image_base64(
                            image.data.clone(),
                            rig_image_media_type(&image.media_type),
                            None,
                        ));
                    }
                }
            }
            if !text.ends_with('\n') {
                text.push('\n');
            }
        }
        text.push_str(&format!("</{}>", section.tag));
    }
    flush(&mut text, &mut contents);
    // Non-empty by construction: the input holds an image, so at least one
    // content was pushed. Checked rather than assumed, because the alternative
    // — falling through to the text rendering — would send metadata for a turn
    // that had already recorded `images_omitted` false, and AGC-FR-37 makes
    // that combination a lie about what the model was shown.
    let content = OneOrMany::many(contents).ok()?;
    Some(rig::completion::Message::User { content })
}

/// The framework's own name for an image media type, or `None` where it has
/// none for it.
///
/// `None` is not a refusal: `rig` carries the bytes either way, and a media type
/// it does not model is one the provider will read from the payload itself. What
/// this must never do is substitute a *different* type — a WEBP announced as a
/// PNG is a request a provider is entitled to reject.
fn rig_image_media_type(media_type: &str) -> Option<rig::completion::message::ImageMediaType> {
    use rig::completion::message::ImageMediaType;
    match media_type.split(';').next().unwrap_or("").trim().to_ascii_lowercase().as_str() {
        "image/png" => Some(ImageMediaType::PNG),
        "image/jpeg" => Some(ImageMediaType::JPEG),
        "image/gif" => Some(ImageMediaType::GIF),
        "image/webp" => Some(ImageMediaType::WEBP),
        "image/heic" => Some(ImageMediaType::HEIC),
        "image/heif" => Some(ImageMediaType::HEIF),
        "image/svg+xml" => Some(ImageMediaType::SVG),
        _ => None,
    }
}

/// The plain text of a `rig` user message. The assembled input is text by
/// construction (CVL-FR-01), so anything else in the message is not something
/// this module put there.
#[cfg(test)]
pub(super) fn rig_user_text(message: &rig::completion::Message) -> String {
    use rig::completion::message::UserContent;
    match message {
        rig::completion::Message::User { content, .. } => content
            .iter()
            .filter_map(|c| match c {
                UserContent::Text(text) => Some(text.text.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}

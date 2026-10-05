//! Images in a turn's material.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

// ---------------------------------------------------------------------------
// Images in a turn's material (AGC-FR-35 – AGC-FR-40, CVL-FR-37, CVL-FR-38)
// ---------------------------------------------------------------------------

/// The smallest byte sequence the asset store accepts as a PNG, tagged so two
/// pictures can be told apart by their content.
fn png_bytes(tag: &str) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(tag.as_bytes());
    bytes
}

fn encoded(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// A draft whose prompt reads a paragraph, an embedded image, a second
/// paragraph, and a second embedded image — the shape AGC-FR-06, AGC-FR-35 names.
fn draft_with_two_embedded_images(h: &Harness) -> (String, Vec<u8>, Vec<u8>) {
    let root = h.root();
    let created = crate::drafts::create_draft_at_root(&root, Some("pictures")).expect("draft");
    let draft_id = created.draft.id.clone();
    let first = crate::draft_assets::store_image_impl(
        &root,
        &draft_id,
        "image/png",
        Some("flow.png"),
        &encoded(&png_bytes("first")),
        &|_: &str| {},
    )
    .expect("stored");
    let second = crate::draft_assets::store_image_impl(
        &root,
        &draft_id,
        "image/png",
        Some("second.png"),
        &encoded(&png_bytes("second")),
        &|_: &str| {},
    )
    .expect("stored");
    crate::drafts::save_draft_file_impl(
        &root,
        &draft_id,
        &created.file,
        &format!(
            "A first paragraph.\n\n![flow diagram]({})\n\nA second paragraph.\n\n![the other]({})\n",
            first.reference, second.reference
        ),
    )
    .expect("write");
    (draft_id, png_bytes("first"), png_bytes("second"))
}

fn open_draft_discussion(h: &Harness, draft_id: &str, body: &str) -> Discussion {
    crate::comments::open_discussion_in(
        &h.root(),
        &h.root(),
        &crate::comments::DiscussionTarget::Draft {
            draft_id: draft_id.to_string(),
        },
        None,
        body.into(),
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion")
}

fn image_parts(section: &InputSection) -> Vec<&InputImage> {
    section
        .parts
        .iter()
        .filter_map(|p| match p {
            InputPart::Image(image) => Some(image),
            _ => None,
        })
        .collect()
}

/// AGC-FR-06, AGC-FR-35: the `artifact` section of a draft turn carries four ordered
/// parts, each image carrying its bytes, its media type, its Markdown
/// reference, and its alt text — and the sections, their tags, and their order
/// are the three of AGC-FR-06.
#[test]
fn agc_ts53_a_drafts_prompt_interleaves_its_text_with_the_images_it_embeds() {
    let h = Harness::new(vec![]);
    let (draft_id, first, second) = draft_with_two_embedded_images(&h);
    let discussion = open_draft_discussion(&h, &draft_id, "@arch what does this say?");

    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
    );
    assert_eq!(
        tags_of(&sections),
        vec![TAG_ARTIFACT, TAG_DISCUSSION_HISTORY, TAG_CURRENT_COMMENT],
        "AGC-FR-06: three sections, in this order, and no fourth",
    );

    let artifact = section_of(&sections, TAG_ARTIFACT);
    // Text, image, text, image — in the order the prompt puts them, an image in
    // the position its Markdown reference occupies. Runs of adjacent prose are
    // one "text" here: how the section happens to split its own prose is not
    // what AGC-FR-35 is about, and asserting it would fail on a harmless change
    // to where the tag scaffolding is added.
    let mut shape: Vec<&str> = Vec::new();
    for part in &artifact.parts {
        let kind = match part {
            InputPart::Text(_) => "text",
            InputPart::Image(_) => "image",
        };
        if shape.last() != Some(&"text") || kind != "text" {
            shape.push(kind);
        }
    }
    assert_eq!(
        shape,
        vec!["text", "image", "text", "image", "text"],
        "the pictures sit between the paragraphs that frame them",
    );
    let images = image_parts(artifact);
    assert_eq!(images.len(), 2);
    for (image, bytes, alt) in [
        (images[0], &first, "flow diagram"),
        (images[1], &second, "the other"),
    ] {
        assert_eq!(image.media_type, "image/png");
        assert_eq!(image.data, encoded(bytes), "the asset's own bytes");
        assert!(image.context.contains(alt), "its alt text: {}", image.context);
        assert!(image.context.contains("../assets/"), "its Markdown reference: {}", image.context);
    }
    // The prose around them is still the prompt's own text.
    assert!(artifact.body.contains("A first paragraph."));
    assert!(artifact.body.contains("A second paragraph."));
}

/// AGC-FR-09, AGC-FR-35: a comment's stored image attachments reach the request as image
/// content in the positions their comments occupy, while a PDF and a link reach
/// it as metadata and no address is requested.
#[test]
fn agc_ts54_a_comments_image_attachments_reach_the_section_holding_them() {
    let h = Harness::new(vec![]);
    let root = h.root();
    let created = crate::drafts::create_draft_at_root(&root, Some("attachments")).expect("draft");
    let draft_id = created.draft.id.clone();

    let target = crate::comments::DiscussionTarget::Draft {
        draft_id: draft_id.clone(),
    };
    let thread = crate::comments::open_discussion_in(
        &root,
        &root,
        &target,
        None,
        "here is a picture".into(),
        vec![crate::comments::AttachmentInput::Inline {
            media_type: "image/png".into(),
            filename: "diff.png".into(),
            data: encoded(&png_bytes("attached")),
        }],
        &human("ada"),
        "2026-01-01T00:00:00Z",
    )
    .expect("discussion");
    crate::comments::add_comment_to(
        &root,
        crate::comments::ThreadRef::discussion(&draft_id),
        &thread.id,
        "and a document".into(),
        Vec::new(),
        vec![crate::comments::AttachmentInput::Inline {
            media_type: "application/pdf".into(),
            filename: "notes.pdf".into(),
            data: encoded(b"%PDF-1.7"),
        }],
        &human("ada"),
        "2026-01-01T00:00:01Z",
    )
    .expect("pdf");
    crate::comments::add_comment_to(
        &root,
        crate::comments::ThreadRef::discussion(&draft_id),
        &thread.id,
        "and a link".into(),
        Vec::new(),
        vec![crate::comments::AttachmentInput::Url {
            url: "https://example.invalid/spec".into(),
            media_type: "text/plain".into(),
            label: Some("spec v2".into()),
        }],
        &human("ada"),
        "2026-01-01T00:00:02Z",
    )
    .expect("link");
    let asking = crate::comments::add_comment_to(
        &root,
        crate::comments::ThreadRef::discussion(&draft_id),
        &thread.id,
        "@arch and this one?".into(),
        Vec::new(),
        vec![crate::comments::AttachmentInput::Inline {
            media_type: "image/png".into(),
            filename: "current.png".into(),
            data: encoded(&png_bytes("current")),
        }],
        &human("ada"),
        "2026-01-01T00:00:03Z",
    )
    .expect("current");
    let trigger = asking.comments.last().expect("the trigger").id.clone();

    let sections = build_input(
        Roots::same(&root),
        &ConversationOrigin::of(&thread),
        &trigger,
    );

    let history = section_of(&sections, TAG_DISCUSSION_HISTORY);
    let history_images = image_parts(history);
    assert_eq!(history_images.len(), 1, "the one stored image the history holds");
    assert_eq!(history_images[0].data, encoded(&png_bytes("attached")));
    // AGC-FR-35: an image part names the comment it belongs to.
    assert!(history_images[0].context.contains("diff.png"), "{}", history_images[0].context);
    assert!(history_images[0].context.contains("@ada"), "{}", history_images[0].context);
    // AGC-FR-09: a PDF and a link are metadata, and always were.
    assert!(history.body.contains("[attachment: notes.pdf (application/pdf), not shown]"));
    assert!(history.body.contains("[attachment: spec v2 (text/plain), not shown]"));
    assert!(
        !history.body.contains("example.invalid/spec\n"),
        "nothing was fetched from the link",
    );

    let current = section_of(&sections, TAG_CURRENT_COMMENT);
    let current_images = image_parts(current);
    assert_eq!(current_images.len(), 1);
    assert_eq!(current_images[0].data, encoded(&png_bytes("current")));
    assert!(current_images[0].context.contains("current.png"));
}

/// CVL-FR-07, CVL-FR-10, CVL-FR-37: the first user message carries the section's parts in order, each
/// image as `rig` image content, each image's reference and alt text beside it,
/// and the preamble carries the compiled prompt alone.
#[test]
fn cvl_ts65_the_first_user_message_carries_ordered_content_parts() {
    use rig::completion::message::{DocumentSourceKind, UserContent};

    let h = Harness::new(vec![]);
    let (draft_id, first, second) = draft_with_two_embedded_images(&h);
    let discussion = open_draft_discussion(&h, &draft_id, "@arch what does this say?");
    let input = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
    );
    let request = AgentRequest {
        instructions: "The compiled prompt.".into(),
        input,
        tools: Vec::new(),
        native_tools: Vec::new(),
        stable_head_sections: 0,
    };

    let exchange = opening_exchange(&request, true);
    assert_eq!(exchange.len(), 1, "the input occupies one first user message");
    let rig::completion::Message::User { content } = &exchange[0] else {
        panic!("the input is a user message");
    };
    let contents: Vec<&UserContent> = content.iter().collect();
    // Material order: text, image, text, image — the images in the positions the
    // prompt put them and nowhere else.
    let kinds: Vec<&str> = contents
        .iter()
        .map(|c| match c {
            UserContent::Image(_) => "image",
            _ => "text",
        })
        .collect();
    assert_eq!(
        kinds.iter().filter(|k| **k == "image").count(),
        2,
        "two image contents: {kinds:?}",
    );
    let image_positions: Vec<usize> = kinds
        .iter()
        .enumerate()
        .filter(|(_, k)| **k == "image")
        .map(|(i, _)| i)
        .collect();
    assert!(image_positions[0] < image_positions[1], "in the prompt's own order");
    assert!(image_positions[0] > 0, "text stands before the first picture");

    for (position, bytes) in [(image_positions[0], &first), (image_positions[1], &second)] {
        let UserContent::Image(image) = contents[position] else {
            panic!("an image content");
        };
        // CVL-FR-37: `rig` image content carrying that image's bytes and media
        // type — never an encoded string inside prose.
        assert_eq!(
            image.data,
            DocumentSourceKind::Base64(encoded(bytes)),
            "the bytes themselves",
        );
        assert_eq!(
            image.media_type,
            Some(rig::completion::message::ImageMediaType::PNG),
        );
        // Its reference and alt text stand beside it, in the text immediately
        // before it.
        let UserContent::Text(before) = contents[position - 1] else {
            panic!("text stands beside the image");
        };
        assert!(before.text.contains("../assets/"), "{}", before.text);
    }
    // Every alt text the prompt wrote reached the request beside its picture.
    let all_text: String = contents
        .iter()
        .filter_map(|c| match c {
            UserContent::Text(t) => Some(t.text.clone()),
            _ => None,
        })
        .collect();
    assert!(all_text.contains("flow diagram"));
    assert!(all_text.contains("the other"));
    // No image was encoded into prose.
    assert!(!all_text.contains(&encoded(&first)));
    assert!(!all_text.contains(&encoded(&second)));

    // CVL-FR-07 / CVL-FR-10: the preamble carries the compiled prompt alone.
    let built = build_completion_request(&request, &exchange, &any_endpoint());
    assert_eq!(built.preamble.as_deref(), Some("The compiled prompt."));
}

/// CVL-FR-20, CVL-FR-37, CVL-FR-38: a text-only endpoint is sent the request a text-only turn always
/// produced, and an input carrying no image is unaffected either way.
#[test]
fn cvl_ts66_a_text_only_request_carries_metadata_where_the_pictures_were() {
    use rig::completion::message::UserContent;

    let h = Harness::new(vec![]);
    let (draft_id, first, second) = draft_with_two_embedded_images(&h);
    let discussion = open_draft_discussion(&h, &draft_id, "@arch what does this say?");
    let input = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
    );
    let request = AgentRequest {
        instructions: "The compiled prompt.".into(),
        input,
        tools: Vec::new(),
        native_tools: Vec::new(),
        stable_head_sections: 0,
    };

    let exchange = opening_exchange(&request, false);
    let rig::completion::Message::User { content } = &exchange[0] else {
        panic!("a user message");
    };
    let contents: Vec<&UserContent> = content.iter().collect();
    assert_eq!(contents.len(), 1, "one text content and nothing else");
    let UserContent::Text(text) = contents[0] else {
        panic!("text");
    };
    // AGC-FR-37: safe metadata for each omitted image, in its original position,
    // naming its media type together with what identifies it.
    assert!(text.text.contains("[image: ![flow diagram](../assets/"), "{}", text.text);
    assert!(text.text.contains("(image/png), not shown]"), "{}", text.text);
    // No bytes and no encoded payload appear anywhere.
    assert!(!text.text.contains(&encoded(&first)));
    assert!(!text.text.contains(&encoded(&second)));

    // An input carrying no image part is what a text-only turn always produced,
    // whichever way it is rendered.
    let plain = AgentRequest {
        instructions: "p".into(),
        input: vec![InputSection {
            tag: TAG_ARTIFACT.into(),
            attributes: Vec::new(),
            body: "Just prose.".into(),
            truncated: false,
            parts: Vec::new(),
        }],
        tools: Vec::new(),
        native_tools: Vec::new(),
        stable_head_sections: 0,
    };
    assert_eq!(
        opening_exchange(&plain, true),
        opening_exchange(&plain, false),
        "no image, no difference",
    );
    assert!(!input_has_images(&plain.input));
}

/// CVL-FR-20, CVL-FR-37, CVL-FR-38 / CVL-FR-23, CVL-FR-26: the exchange a retry presents is byte-for-byte the one
/// that failed, and no image reaches a tool result, a record, or an event.
#[test]
fn cvl_ts66_a_retry_presents_the_identical_content_parts() {
    let h = Harness::new(vec![Err("unreachable"), Ok("Answered.".into())]);
    let (draft_id, _, _) = draft_with_two_embedded_images(&h);
    h.seed_verified_provider();
    h.create_agent("arch", "Argue about structure.");
    let discussion = open_draft_discussion(&h, &draft_id, "@arch what does this say?");

    h.dispatch(
        "arch",
        ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
    )
    .expect("dispatched");
    h.settle();

    let exchanges = h.seam.exchanges();
    assert!(exchanges.len() >= 2, "the call was repeated: {}", exchanges.len());
    assert_eq!(
        exchanges[0], exchanges[1],
        "the retried request carries the identical content parts in the identical order",
    );

    // CVL-FR-26 / AGC-FR-40: no event payload carries an image or its encoded
    // payload.
    let payloads = serde_json::to_string(&*h.events.lock().unwrap()).expect("events");
    assert!(!payloads.contains(&encoded(&png_bytes("first"))));
    assert!(!payloads.contains(&encoded(&png_bytes("second"))));
}

/// AGC-FR-36, AGC-FR-37 / AGC-FR-38: a turn whose model takes no image sends text and
/// metadata, is an ordinary successful conversation, and changes nothing about
/// the material it was answering.
#[test]
fn agc_ts55_a_text_only_model_answers_and_the_turn_records_that_it_omitted_images() {
    let h = Harness::new(vec![Ok("Answered.".into())]);
    let (draft_id, first, _) = draft_with_two_embedded_images(&h);
    h.seed_verified_provider();
    h.create_agent("arch", "Argue about structure.");
    let discussion = open_draft_discussion(&h, &draft_id, "@arch what does this say?");
    let prompt_before = std::fs::read(
        crate::drafts::draft_file_abs_path(
            &h.root(),
            &draft_id,
            &crate::drafts::require_prompt(&h.root(), &draft_id).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();

    let dispatched = h.dispatch(
        "arch",
        ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
    )
    .expect("dispatched");
    // Waited out on this turn's own terminal event rather than on `settle()`.
    // `settle()` returns when the in-flight set empties, and `terminate` empties
    // it **before** the terminal event is published, so everything below it read
    // a turn that had not finished announcing itself — and `last()` on the
    // captured events could name another turn's altogether.
    let terminal = wait_for_terminal(&h, &dispatched.id);

    // The seeded model declares nothing about its input modalities and the
    // shipped catalog names it nowhere, so the capability is absent — which is
    // the fallback an unknown endpoint gets (AGC-FR-36).
    let seen = h.seam.requests();
    let (_, endpoint) = seen.first().expect("one call");
    assert!(!endpoint.accepts_image_input);

    let turn = &terminal;
    assert_eq!(turn.state, AgentTurnState::Delivered, "an ordinary successful conversation");
    assert!(turn.failure.is_none());
    assert!(!turn.retry_permitted, "it is in no recovery registry");
    assert!(turn.images_omitted, "and it says the pictures were not sent");

    // No bytes and no encoded payload appear anywhere in the request.
    let exchange = h.seam.exchanges();
    let wire = format!("{:?}", exchange);
    assert!(!wire.contains(&encoded(&first)), "no encoded payload reached the request");
    assert!(wire.contains("not shown"), "the metadata stood where the picture was");

    // AGC-FR-38: nothing about the fallback altered the author's prompt, and the
    // instructions carry no word about images.
    let prompt_after = std::fs::read(
        crate::drafts::draft_file_abs_path(
            &h.root(),
            &draft_id,
            &crate::drafts::require_prompt(&h.root(), &draft_id).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(prompt_before, prompt_after, "the draft's Markdown is byte-for-byte unchanged");
    let (request, _) = seen.first().expect("one call");
    assert!(
        !request.instructions.to_lowercase().contains("image"),
        "the compiled prompt is what it always is",
    );
    // The delivered comment carries no marker of any kind.
    let thread = crate::comments::locate_discussion_of(
        &h.root(),
        &crate::comments::DiscussionTarget::Draft {
            draft_id: draft_id.clone(),
        },
        &discussion.id,
    )
    .expect("thread");
    let answer = thread.comments.last().expect("the answer");
    assert_eq!(answer.body, "Answered.");
    assert!(!answer.body.to_lowercase().contains("image"));
}

/// AGC-FR-18, AGC-FR-21, AGC-FR-39: the notice registry holds the current state of a conversation
/// rather than a history of it, and its transitions emit nothing.
#[test]
fn agc_ts58_the_notice_registry_replaces_and_retires_its_entry() {
    let h = Harness::new(vec![]);
    let turns = h.turns();
    let origin = ConversationOrigin::stub_draft("thread-1", "draft-1", false);
    let other = ConversationOrigin::stub_draft("thread-2", "draft-1", false);
    let entry = |id: &str, origin: &ConversationOrigin, omitted: bool| AgentTurn {
        id: id.into(),
        agent_id: "a".into(),
        nickname: "arch".into(),
        origin: origin.clone(),
        trigger_comment_id: "c1".into(),
        state: AgentTurnState::Delivered,
        failure: None,
        retry_permitted: false,
        started_at: "2026-01-01T00:00:00Z".into(),
        ended_at: Some("2026-01-01T00:00:01Z".into()),
        active_tool_calls: Vec::new(),
        images_omitted: omitted,
    };

    turns.record_image_notice(&entry("turn-1", &origin, true), PROJECT_KEY);
    assert_eq!(
        turns
            .image_notices(Some(&origin))
            .iter()
            .map(|t| t.id.clone())
            .collect::<Vec<_>>(),
        vec!["turn-1".to_string()],
    );
    // A later turn in the same conversation that also omitted images replaces it.
    turns.record_image_notice(&entry("turn-2", &origin, true), PROJECT_KEY);
    let held = turns.image_notices(Some(&origin));
    assert_eq!(held.len(), 1, "the registry holds the later turn alone");
    assert_eq!(held[0].id, "turn-2");
    // Another conversation's entry is its own.
    turns.record_image_notice(&entry("turn-3", &other, true), PROJECT_KEY);
    assert_eq!(turns.image_notices(None).len(), 2);
    // A later turn that carried its images retires the entry.
    turns.record_image_notice(&entry("turn-4", &origin, false), PROJECT_KEY);
    assert!(turns.image_notices(Some(&origin)).is_empty());
    assert_eq!(turns.image_notices(Some(&other)).len(), 1);

    // Entering, replacing, and retiring emit nothing of their own.
    assert!(
        h.events.lock().unwrap().is_empty(),
        "no event was emitted for any of it",
    );
}

/// AGC-FR-40, AGC-FR-26: no image, no encoded payload, and no filename beyond the metadata
/// the material already held reaches an event payload, a log record, or an
/// appended comment.
#[test]
fn agc_ts59_no_image_reaches_any_record_event_or_comment() {
    let h = Harness::new(vec![Ok("Answered.".into())]);
    let (draft_id, first, second) = draft_with_two_embedded_images(&h);
    h.seed_verified_provider();
    h.create_agent("arch", "Argue about structure.");
    let discussion = open_draft_discussion(&h, &draft_id, "@arch what does this say?");

    h.dispatch(
        "arch",
        ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
    )
    .expect("dispatched");
    h.settle();

    let payloads = serde_json::to_string(&*h.events.lock().unwrap()).expect("events");
    // The harness reports into its own buffer (`TEST_BUFFER`), so the records
    // this turn actually wrote are the ones `all_records` returns — the
    // process-global one would answer for a turn that never touched it, and the
    // assertion below would hold however much this path logged.
    let log = format!("{:?}", all_records());
    let thread = crate::comments::locate_discussion_of(
        &h.root(),
        &crate::comments::DiscussionTarget::Draft {
            draft_id: draft_id.clone(),
        },
        &discussion.id,
    )
    .expect("thread");
    let conversation = serde_json::to_string(&thread).expect("thread");
    for encoded_image in [encoded(&first), encoded(&second)] {
        for (what, text) in [
            ("an event payload", &payloads),
            ("a log record", &log),
            ("an appended comment", &conversation),
        ] {
            assert!(!text.contains(&encoded_image), "{what} carried an image");
        }
    }
    // And an `AgentRequest` has no wire form for its parts at all, so nothing
    // that serialises one can carry a picture (AGC-FR-40).
    //
    // The **field** rather than the word: a tool's own fixed description is part
    // of a serialised request, and one of them says a question asked in three
    // parts comes back answered in one. A bare substring search would read that
    // prose as a content part and fail over a sentence addressed to a model.
    let (request, _) = h.seam.requests().first().cloned().expect("one call");
    let serialised = serde_json::to_string(&request).expect("request");
    assert!(!serialised.contains("\"parts\""), "{serialised}");
}


/// AGC-FR-06, AGC-FR-35 / AGC-FR-37, AGC-FR-38 / CVL-FR-07, CVL-FR-10, CVL-FR-37: a turn whose resolved model **accepts
/// image content** carries every image its material holds, as `rig` image
/// content, and records that it omitted none.
#[test]
fn agc_ts53_a_turn_against_an_image_capable_model_carries_its_pictures() {
    use rig::completion::message::{DocumentSourceKind, UserContent};

    let h = Harness::new(vec![Ok("Answered.".into())]);
    let (draft_id, first, second) = draft_with_two_embedded_images(&h);
    h.seed_image_capable_provider();
    h.create_agent("arch", "Argue about structure.");
    let discussion = open_draft_discussion(&h, &draft_id, "@arch what does this say?");

    let dispatched = h.dispatch(
        "arch",
        ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
    )
    .expect("dispatched");
    // Waited out on this turn's own terminal event rather than on `settle()`.
    // `settle()` returns when the in-flight set empties, and `terminate` empties
    // it **before** the terminal event is published, so everything below it read
    // a turn that had not finished announcing itself — and `last()` on the
    // captured events could name another turn's altogether.
    let terminal = wait_for_terminal(&h, &dispatched.id);

    // AGC-FR-14 / AAP-FR-35: the endpoint that served the call is the one whose
    // capability decided the shape.
    let seen = h.seam.requests();
    let (_, endpoint) = seen.first().expect("one call");
    assert!(endpoint.accepts_image_input, "the resolved model takes images");

    // CVL-FR-10 / CVL-FR-37: the input occupies the first user message, and its
    // pictures are `rig` image content carrying their bytes and media type.
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
    assert_eq!(images.len(), 2, "both pictures reached the request");
    assert_eq!(images[0].data, DocumentSourceKind::Base64(encoded(&first)));
    assert_eq!(images[1].data, DocumentSourceKind::Base64(encoded(&second)));
    for image in &images {
        assert_eq!(
            image.media_type,
            Some(rig::completion::message::ImageMediaType::PNG),
        );
    }
    // AGC-FR-37: nothing was omitted, so the turn says so — and CTA-FR-LVPC's
    // notice has nothing to render.
    let turn = &terminal;
    assert_eq!(turn.state, AgentTurnState::Delivered);
    assert!(!turn.images_omitted);
}

/// CVL-FR-20, CVL-FR-37, CVL-FR-38: a retry of a turn **carrying images** presents the identical
/// content parts in the identical order — no image removed, none turned into
/// text, and no partial multimodal variant ever built.
#[test]
fn cvl_ts66_a_retry_of_a_multimodal_turn_repeats_it_byte_for_byte() {
    use rig::completion::message::UserContent;

    let h = Harness::new(vec![Err("unreachable"), Ok("Answered.".into())]);
    let (draft_id, first, _) = draft_with_two_embedded_images(&h);
    h.seed_image_capable_provider();
    h.create_agent("arch", "Argue about structure.");
    let discussion = open_draft_discussion(&h, &draft_id, "@arch what does this say?");

    h.dispatch(
        "arch",
        ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
    )
    .expect("dispatched");
    h.settle();

    let exchanges = h.seam.exchanges();
    assert!(exchanges.len() >= 2, "the call was repeated: {}", exchanges.len());
    // The claim is only worth anything if the first attempt carried pictures.
    let carried = |exchange: &Vec<rig::completion::Message>| {
        let rig::completion::Message::User { content } = &exchange[0] else {
            panic!("a user message");
        };
        content
            .iter()
            .filter(|c| matches!(c, UserContent::Image(_)))
            .count()
    };
    assert_eq!(carried(&exchanges[0]), 2, "the first attempt carried both");
    assert_eq!(
        exchanges[0], exchanges[1],
        "the retried request is byte-for-byte the one that failed",
    );

    // AGC-FR-40 / CVL-FR-26: and no event payload carries an image.
    let payloads = serde_json::to_string(&*h.events.lock().unwrap()).expect("events");
    assert!(!payloads.contains(&encoded(&first)));
}

/// AGC-FR-37, AGC-FR-36, CVL-FR-20: a model whose provider declares **nothing** about its input
/// modalities is treated as taking no image, so the turn sends the
/// text-and-metadata request of AGC-FR-37.
#[test]
fn agc_ts56_an_undeclared_capability_gets_the_fallback() {
    let h = Harness::new(vec![Ok("Answered.".into())]);
    let (draft_id, first, _) = draft_with_two_embedded_images(&h);
    // The ordinary harness provider: model `m` declares no input modalities at
    // all, and the shipped catalog for `openrouter` names no model.
    h.seed_verified_provider();
    h.create_agent("arch", "Argue about structure.");
    let discussion = open_draft_discussion(&h, &draft_id, "@arch what does this say?");

    let dispatched = h.dispatch(
        "arch",
        ConversationOrigin::of(&discussion),
        &discussion.comments[0].id,
    )
    .expect("dispatched");
    // Waited out on this turn's own terminal event rather than on `settle()`.
    // `settle()` returns when the in-flight set empties, and `terminate` empties
    // it **before** the terminal event is published, so everything below it read
    // a turn that had not finished announcing itself — and `last()` on the
    // captured events could name another turn's altogether.
    let terminal = wait_for_terminal(&h, &dispatched.id);

    let (_, endpoint) = h.seam.requests().first().cloned().expect("one call");
    assert!(
        !endpoint.accepts_image_input,
        "an undeclared capability is absent rather than guessed at",
    );
    let wire = format!("{:?}", h.seam.exchanges());
    assert!(!wire.contains(&encoded(&first)), "no payload reached the request");
    assert!(wire.contains("not shown"), "the metadata stood where the picture was");
    assert!(terminal.images_omitted);
}

/// AGC-FR-18, AGC-FR-21, AGC-FR-39 / CTA-FR-ARBB: the notice registry answers for a conversation whose
/// turn actually ran, and is retired by a later turn that carried its images.
#[test]
fn agc_ts58_the_registry_follows_a_real_turn_and_its_successor() {
    let h = Harness::new(vec![Ok("First.".into()), Ok("Second.".into())]);
    let (draft_id, _, _) = draft_with_two_embedded_images(&h);
    h.seed_verified_provider();
    h.create_agent("arch", "Argue about structure.");
    let discussion = open_draft_discussion(&h, &draft_id, "@arch what does this say?");
    let origin = ConversationOrigin::of(&discussion);

    h.dispatch("arch", origin.clone(), &discussion.comments[0].id)
        .expect("dispatched");
    h.settle();
    assert_eq!(
        h.turns().image_notices(Some(&origin)).len(),
        1,
        "the turn that omitted its images is the conversation's entry",
    );

    // The author changes the agent's model to one that takes images and asks
    // again: that turn carries them, and the entry is retired.
    h.seed_image_capable_provider();
    h.dispatch("arch", origin.clone(), &discussion.comments[0].id)
        .expect("dispatched");
    h.settle();
    assert!(
        h.turns().image_notices(Some(&origin)).is_empty(),
        "a turn that carried its images retires the notice",
    );
}

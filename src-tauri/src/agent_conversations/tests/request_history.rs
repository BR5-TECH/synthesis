//! The rest of what a request carries: the history behind it, and the
//! material a turn is given to read.
//!
//! One part of `../tests/mod.rs`, which holds the harness these all run
//! against and the rule they are all written under.

use super::*;

#[test]
fn the_artifact_builder_gathers_the_artifact_the_history_and_the_current_comment() {
    // AGC-FR-05, AGC-FR-06, AGC-FR-07.
    //
    // Several paragraphs and several comments, with the trigger in the *middle*
    // — the one arrangement that pins all three of "the earlier lines are
    // present", "the trigger is not in the history", and "the later line is in
    // neither" at once. Every body is asserted whole rather than searched: a
    // `contains` suite would pass against a builder that had sent one paragraph,
    // reordered the comments, or prepended a phrasing of the application's own,
    // which is precisely what CVL-FR-01 forbids.
    const SOURCE: &str = "First paragraph.\n\nSecond paragraph.\n\nThird paragraph.";
    let h = Harness::new(vec![]);
    let thread = h.seed_artifact_thread("spec.md", SOURCE);
    for (body, at) in [
        ("@arch and one more thing.", "2026-01-01T00:00:05Z"),
        ("A later comment nobody asked about.", "2026-01-01T00:00:06Z"),
    ] {
        crate::comments::add_comment_in(
            &h.root(),
            "spec.md",
            &thread.id,
            body.into(),
            vec![],
            vec![],
            &human("ada"),
            at,
        )
        .expect("comment");
    }
    let threads = h.threads("spec.md");
    let trigger = threads[0].comments[1].id.clone();

    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &trigger,
    );
    assert_eq!(tags_of(&sections), INPUT_TAGS.map(str::to_string).to_vec());

    // The whole Markdown source, named by its path.
    let artifact = section_of(&sections, TAG_ARTIFACT);
    assert_eq!(artifact.body, SOURCE);
    assert!(!artifact.truncated);
    assert_eq!(
        artifact.attributes,
        vec![("path".to_string(), "spec.md".to_string())],
    );

    // The anchor opens the history — a thread is anchored rather than a comment
    // is — and carries its offsets *and* its quoted passage. Then the comments
    // before the trigger, oldest first.
    let quote: String = SOURCE.chars().skip(4).take(8).collect();
    let history = section_of(&sections, TAG_DISCUSSION_HISTORY);
    assert_eq!(
        history.body,
        format!(
            "This conversation is anchored to characters 4..12 of the file:\n\n\
             {quote}\n\n\
             @ada:\n@arch what do you think?\n\n"
        ),
    );

    // The current comment is the one that addressed the agent, and it is in
    // neither the history nor anywhere else — the two tags exist to draw exactly
    // that distinction, and a message repeated in both would be supporting
    // context and the thing being answered at once.
    let current = section_of(&sections, TAG_CURRENT_COMMENT);
    assert_eq!(current.body, "@ada:\n@arch and one more thing.\n");

    // AGC-FR-13: the comment posted after the trigger is in no section at all.
    for s in &sections {
        assert!(
            !s.body.contains("A later comment"),
            "a comment posted after the trigger reached <{}>",
            s.tag,
        );
    }
}

#[test]
fn an_attribute_value_cannot_close_its_own_tag() {
    // CVL-FR-07. A draft's name and an artifact's path are the two participant-
    // written strings that land inside tag *structure* rather than inside a
    // body, so they are the one place an escape would put a participant's text
    // where the frame belongs.
    assert_eq!(escape_attribute(r#"a&b"c"#), "a&amp;b&quot;c");
    assert_eq!(
        escape_attribute("&quot;"),
        "&amp;quot;",
        "an escape is not itself re-escaped",
    );

    let rendered = render_input(&AgentRequest {
        instructions: String::new(),
        input: vec![InputSection {
            tag: TAG_ARTIFACT.into(),
            attributes: vec![("draft".into(), r#"Spec "v2" & notes"#.into())],
            body: "b".into(),
            truncated: false,
            parts: Vec::new(),
        }],
        tools: Vec::new(),
        native_tools: Vec::new(),
        stable_head_sections: 0,
    });
    assert!(
        rendered.starts_with(r#"<artifact draft="Spec &quot;v2&quot; &amp; notes">"#),
        "the value stayed inside its own attribute:\n{rendered}",
    );
}

#[test]
fn the_history_names_every_author_by_handle() {
    // AGC-FR-08.
    let h = Harness::new(vec![]);
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    crate::comments::append_agent_comment(
        &h.root(),
        LogScope::Artifact,
        "spec.md",
        &thread.id,
        "An earlier agent line.".into(),
        &Participant::Agent {
            agent_id: "id".into(),
            handle: "scribe".into(),
            model: Some("m".into()),
            title: None,
        },
        "2026-01-01T00:00:01Z",
    )
    .expect("agent comment");

    // A third line, so both earlier authors sit in the history rather than one
    // of them being the current comment.
    crate::comments::add_comment_in(
        &h.root(),
        "spec.md",
        &thread.id,
        "@arch so which is it?".into(),
        vec![],
        vec![],
        &human("ada"),
        "2026-01-01T00:00:02Z",
    )
    .expect("third comment");

    let threads = h.threads("spec.md");
    let last = threads[0].comments.last().unwrap().id.clone();
    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &last,
    );
    let history = section_of(&sections, TAG_DISCUSSION_HISTORY);
    let ada = history.body.find("@ada:").expect("the human's login");
    let scribe = history.body.find("@scribe:").expect("the agent's nickname");
    // AGC-FR-06: the anchor opens the history, then the comments oldest first.
    // Asserted as positions rather than presence, because a history that
    // reversed its comments or trailed its anchor would hold all three strings.
    assert!(
        history
            .body
            .starts_with("This conversation is anchored to characters"),
        "the anchor opens the history:\n{}",
        history.body,
    );
    assert!(ada < scribe, "comments oldest first");
}

#[test]
fn a_quote_reaches_the_agent_attributed_to_whoever_wrote_the_passage() {
    // AGC-FR-06, AGC-FR-08 / AGC-FR-10. A quote's whole contribution is the *pointer*: the
    // quoted comment is one of the same thread (CMS-FR-16), so its body is
    // already in the history in its own place, and what the excerpt adds is
    // which passage of it a reply singled out.
    let h = Harness::new(vec![]);
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    crate::comments::append_agent_comment(
        &h.root(),
        LogScope::Artifact,
        "spec.md",
        &thread.id,
        "the rail's floor is wrong on a narrow tab".into(),
        &Participant::Agent {
            agent_id: "id".into(),
            handle: "arch".into(),
            model: Some("m".into()),
            title: None,
        },
        "2026-01-01T00:00:01Z",
    )
    .expect("the comment that will be quoted");

    let quoted_id = h.threads("spec.md")[0]
        .comments
        .last()
        .expect("the agent's line")
        .id
        .clone();

    // The reply that quotes it, and a third line so the reply sits in the
    // history rather than being the current comment.
    crate::comments::add_comment_in(
        &h.root(),
        "spec.md",
        &thread.id,
        "Agreed, but only on a narrow tab.".into(),
        vec![crate::comments::CommentQuote {
            comment_id: quoted_id.clone(),
            excerpt: "the rail's floor is wrong".into(),
        }],
        vec![],
        &human("sam"),
        "2026-01-01T00:00:02Z",
    )
    .expect("the quoting comment");
    crate::comments::add_comment_in(
        &h.root(),
        "spec.md",
        &thread.id,
        "@arch so which is it?".into(),
        vec![],
        vec![],
        &human("ada"),
        "2026-01-01T00:00:03Z",
    )
    .expect("the trigger");

    let threads = h.threads("spec.md");
    let trigger = threads[0].comments.last().unwrap().id.clone();
    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &trigger,
    );
    let history = section_of(&sections, TAG_DISCUSSION_HISTORY);

    // The pointer, attributed to whoever wrote the passage rather than to
    // whoever is quoting it.
    assert!(
        history
            .body
            .contains("[quoting @arch: the rail's floor is wrong]"),
        "the quote names its author and carries the excerpt:\n{}",
        history.body,
    );
    // AGC-FR-10: between the quoting author's handle and that comment's own
    // body. Asserted as positions, because a rendering that put the quote after
    // the body — or attributed it to `@sam` — would hold every one of these
    // strings and still read as a remark that merely follows the passage.
    let sam = history.body.find("@sam:").expect("the quoting author");
    let quote = history.body.find("[quoting @arch:").expect("the quote");
    let body = history
        .body
        .find("Agreed, but only on a narrow tab.")
        .expect("the quoting comment's own words");
    assert!(sam < quote && quote < body, "handle, quote, body:\n{}", history.body);
    // The quoted comment is still in the history in its own place: the excerpt
    // points into material the agent already has rather than replacing it.
    assert!(
        history
            .body
            .contains("@arch:\nthe rail's floor is wrong on a narrow tab"),
        "the quoted comment still stands in its own place:\n{}",
        history.body,
    );
}

#[test]
fn a_quote_that_resolves_to_nobody_keeps_its_excerpt_and_loses_only_the_handle() {
    // AGC-FR-06, AGC-FR-08 (final clause) / AGC-FR-10. An excerpt whose author cannot be
    // named still says which passage was meant, so it is rendered unattributed
    // rather than dropped — dropping it would lose the one thing the quote was
    // carrying.
    //
    // Exercised at the **resolution** step rather than at the formatter, because
    // that is where the claim lives: `comment_lines` looking a quote's target up
    // among the thread's own handles and finding nothing. A test that handed
    // `None` straight to `quote_line` would still pass if the resolution step had
    // learned to *skip* an unresolvable quote, which is the failure this guards
    // against.
    //
    // It cannot be reached through the write path at all: `add_comment` refuses a
    // quote naming a comment outside its thread with `quoted_comment_not_in_thread`
    // (CMS-FR-16), so the only way a stored quote resolves to nothing is a
    // comment the *fold* dropped — a duplicate id, or a log that union-merged
    // oddly (CMS-FR-39). The fallback is defence against that, and this asserts
    // it at the only layer that can produce it.
    let h = Harness::new(vec![]);
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let handles = handles_of(&thread);

    let orphaned = comments::Comment {
        id: "c-later".into(),
        author: human("ada"),
        body: "@arch what about this?".into(),
        quotes: vec![crate::comments::CommentQuote {
            comment_id: "a-comment-the-fold-dropped".into(),
            excerpt: "a passage from nowhere".into(),
        }],
        attachments: Vec::new(),
        created_at: "2026-01-01T00:00:02Z".into(),
    };
    let rendered = comment_text(&orphaned, &handles);
    assert!(
        rendered.contains("[quoting: a passage from nowhere]"),
        "the excerpt survives, unattributed:\n{rendered}",
    );
    assert!(
        !rendered.contains("[quoting @"),
        "nothing was invented to attribute it to:\n{rendered}",
    );
    assert!(
        rendered.contains("@arch what about this?"),
        "the comment's own body is still rendered:\n{rendered}",
    );

    // And the same comment with a resolvable target is attributed, so the test
    // above is not passing because attribution never happens.
    let target = thread.comments.first().expect("the opener");
    let attributable = comments::Comment {
        quotes: vec![crate::comments::CommentQuote {
            comment_id: target.id.clone(),
            excerpt: "a passage from somewhere".into(),
        }],
        ..orphaned
    };
    assert!(
        comment_text(&attributable, &handles).contains(&format!(
            "[quoting @{}: a passage from somewhere]",
            handle_of(&target.author),
        )),
        "a resolvable quote is attributed",
    );
}

#[test]
fn several_quotes_render_in_the_order_the_comment_holds_them() {
    // AGC-FR-06, AGC-FR-08 (second clause) / AGC-FR-10. A comment may single out more than
    // one passage, and which order they are read in is the order the author put
    // them in — a rendering that reversed them, or that resolved them through an
    // unordered map, would still carry every string.
    let h = Harness::new(vec![]);
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let opener = thread.comments.first().expect("the opener").id.clone();
    crate::comments::append_agent_comment(
        &h.root(),
        LogScope::Artifact,
        "spec.md",
        &thread.id,
        "Two separate concerns here.".into(),
        &Participant::Agent {
            agent_id: "id".into(),
            handle: "arch".into(),
            model: Some("m".into()),
            title: None,
        },
        "2026-01-01T00:00:01Z",
    )
    .expect("a second quotable comment");
    let second = h.threads("spec.md")[0]
        .comments
        .last()
        .expect("the agent's line")
        .id
        .clone();

    crate::comments::add_comment_in(
        &h.root(),
        "spec.md",
        &thread.id,
        "Both, then.".into(),
        vec![
            crate::comments::CommentQuote {
                comment_id: opener,
                excerpt: "FIRST-EXCERPT".into(),
            },
            crate::comments::CommentQuote {
                comment_id: second,
                excerpt: "SECOND-EXCERPT".into(),
            },
        ],
        vec![],
        &human("ada"),
        "2026-01-01T00:00:02Z",
    )
    .expect("a comment quoting two passages");

    let threads = h.threads("spec.md");
    let trigger = threads[0].comments.last().unwrap().id.clone();
    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &trigger,
    );
    let current = section_of(&sections, TAG_CURRENT_COMMENT);
    let first = current.body.find("FIRST-EXCERPT").expect("the first quote");
    let second_at = current
        .body
        .find("SECOND-EXCERPT")
        .expect("the second quote");
    let body = current.body.find("Both, then.").expect("the comment's words");
    assert!(
        first < second_at && second_at < body,
        "quotes in the order they were made, then the body:\n{}",
        current.body,
    );
    // Each is attributed to whoever wrote the passage, not to the quoter.
    assert!(
        current.body.contains("[quoting @arch: SECOND-EXCERPT]"),
        "the second quote names the agent that wrote it:\n{}",
        current.body,
    );
}

#[test]
fn the_comment_that_addressed_the_agent_carries_its_quotes_too() {
    // AGC-FR-06, AGC-FR-08 / AGC-FR-10: "the section holding it" — `current_comment` for
    // the comment that addressed the agent, on the same terms as the history.
    let h = Harness::new(vec![]);
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let opener = thread.comments.first().expect("the opening comment").id.clone();
    crate::comments::add_comment_in(
        &h.root(),
        "spec.md",
        &thread.id,
        "@arch what about this bit?".into(),
        vec![crate::comments::CommentQuote {
            comment_id: opener,
            excerpt: "the passage in question".into(),
        }],
        vec![],
        &human("ada"),
        "2026-01-01T00:00:02Z",
    )
    .expect("the trigger, carrying a quote");

    let threads = h.threads("spec.md");
    let trigger = threads[0].comments.last().unwrap().id.clone();
    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &trigger,
    );
    let current = section_of(&sections, TAG_CURRENT_COMMENT);
    assert!(
        current.body.contains("[quoting @"),
        "the current comment renders its quote attributed:\n{}",
        current.body,
    );
    assert!(
        current.body.contains("the passage in question"),
        "the current comment carries the excerpt:\n{}",
        current.body,
    );
}

#[test]
fn a_current_comment_alone_is_not_material_to_answer_against() {
    // AGC-FR-12. The predicate asks after the two material tags rather than
    // after an empty vector, so a builder added later cannot reach a model with
    // a question and nothing it is a question about.
    let s = |tag: &str| section(tag, Vec::new(), "x".into());
    assert!(!has_material(&[]));
    assert!(!has_material(&[s(TAG_CURRENT_COMMENT)]));
    assert!(has_material(&[s(TAG_ARTIFACT)]));
    assert!(has_material(&[s(TAG_DISCUSSION_HISTORY)]));
    assert!(has_material(&[s(TAG_DISCUSSION_HISTORY), s(TAG_CURRENT_COMMENT)]));
}

#[test]
fn a_trigger_the_thread_does_not_hold_leaves_the_current_comment_empty() {
    // AGC-FR-06's fallback. Not reachable through dispatch — a trigger id always
    // names a comment the thread holds — but the builder is total, and pinning
    // what it does here is what stops the branch drifting into something worse
    // than an empty section.
    let h = Harness::new(vec![]);
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        "no-such-comment",
    );
    assert_eq!(tags_of(&sections), INPUT_TAGS.map(str::to_string).to_vec());
    assert!(section_of(&sections, TAG_CURRENT_COMMENT).body.is_empty());
    // Nothing is lost: with no trigger to stop at, the history holds the whole
    // conversation, and the turn still has material to proceed on.
    assert!(section_of(&sections, TAG_DISCUSSION_HISTORY)
        .body
        .contains("@arch what do you think?"));
    assert!(has_material(&sections));
}

/// CVL-FR-01, AAP-FR-21 / AGC-FR-09: an attachment is **named** to the agent, and the
/// request a text-only endpoint is sent stays text throughout — no payload, no
/// image content, no fetch.
///
/// AGC-FR-35 changed what a *section* may hold: a stored image blob now
/// contributes an image part carrying its bytes, so that a turn against a model
/// that takes pictures can be shown one. What did **not** change is either half
/// of what this scenario is about — the metadata every attachment is named by,
/// which is what a text-only request carries in the picture's place
/// (AGC-FR-37), and the rule that nothing is ever fetched. So the assertions
/// below are made about the **rendered request** rather than about the section,
/// which is where they were always meaningful.
#[test]
fn attachments_are_named_without_being_carried() {
    use base64::Engine as _;
    let png = b"\x89PNG-not-really";
    let encoded = base64::engine::general_purpose::STANDARD.encode(png);

    let h = Harness::new(vec![]);
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    crate::comments::add_comment_in(
        &h.root(),
        "spec.md",
        &thread.id,
        "Here is what I mean.".into(),
        vec![],
        vec![
            crate::comments::AttachmentInput::Inline {
                media_type: "image/png".into(),
                filename: "diff.png".into(),
                data: encoded.clone(),
            },
            crate::comments::AttachmentInput::Url {
                url: "https://example.test/spec-v2.png".into(),
                media_type: "image/png".into(),
                label: Some("spec v2".into()),
            },
        ],
        &human("ada"),
        "2026-01-01T00:00:02Z",
    )
    .expect("comment with attachments");

    let threads = h.threads("spec.md");
    let last = threads[0].comments.last().unwrap().id.clone();
    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &last,
    );
    // AGC-FR-09: rendered into the section holding the comment — this one
    // addressed the agent, so its attachments are named in `current_comment`.
    let current = section_of(&sections, TAG_CURRENT_COMMENT);

    // Named, with its media type, in the order the comment carries them.
    let blob_at = current.body.find("diff.png").expect("blob named");
    let url_at = current.body.find("spec v2").expect("link named by label");
    assert!(blob_at < url_at, "named in the order the comment carries them");
    assert!(current.body.contains("(image/png)"));
    assert!(
        current.body.contains("not shown"),
        "the agent is told it cannot see the attachment",
    );

    // Never carried into the text a text-only endpoint is sent: not the
    // payload, not the digest, not the address. Asserted over the request as it
    // is actually rendered, so the claim covers the whole of what reaches the
    // model rather than one field of one section.
    let request = AgentRequest {
        instructions: String::new(),
        input: sections.clone(),
        tools: Vec::new(),
        native_tools: Vec::new(),
        stable_head_sections: 0,
    };
    let rendered = render_input(&request);
    for forbidden in [
        encoded.as_str(),
        &crate::fs::sha256_bytes(png),
        "https://example.test/spec-v2.png",
    ] {
        assert!(
            !rendered.contains(forbidden),
            "{forbidden:?} reached the rendered request",
        );
        for s in &sections {
            assert!(
                !s.body.contains(forbidden),
                "{forbidden:?} reached the request in section {:?}",
                s.tag,
            );
        }
    }
    // And a text-only turn sends exactly that rendering: the image the section
    // holds contributes its metadata and none of its bytes (AGC-FR-37).
    let text_only = opening_exchange(&request, false);
    assert!(
        !format!("{text_only:?}").contains(encoded.as_str()),
        "a text-only request carries no encoded payload",
    );
    // AGC-FR-35: the bytes are there to be sent to an endpoint that takes them,
    // and they are in the image part rather than anywhere in the prose — which
    // is the distinction the loop above would otherwise not be testing.
    assert_eq!(
        current
            .parts
            .iter()
            .filter(|p| matches!(p, InputPart::Image(_)))
            .count(),
        1,
        "the stored image is one part of its own",
    );
    // AGC-FR-09: a link with no label is named by its address, which is the
    // branch the forbidden-substring loop above would otherwise hide — it holds
    // only because the fixture's link carries a label.
    crate::comments::add_comment_in(
        &h.root(),
        "spec.md",
        &thread.id,
        "and this one".into(),
        vec![],
        vec![crate::comments::AttachmentInput::Url {
            url: "https://example.test/unlabelled.png".into(),
            media_type: "image/png".into(),
            label: None,
        }],
        &human("ada"),
        "2026-01-01T00:00:03Z",
    )
    .expect("unlabelled link");

    let threads = h.threads("spec.md");
    let newest = threads[0].comments.last().unwrap().id.clone();
    let with_bare_link = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &newest,
    );
    assert!(
        section_of(&with_bare_link, TAG_CURRENT_COMMENT)
            .body
            .contains("https://example.test/unlabelled.png"),
        "a link with no label is named by its address",
    );
    // The other branch of AGC-FR-09: the earlier comment's attachments are now
    // history rather than the current comment, and are named there on the same
    // terms.
    let history = section_of(&with_bare_link, TAG_DISCUSSION_HISTORY);
    assert!(history.body.contains("diff.png"));
    assert!(history.body.contains("spec v2"));
    assert!(history.body.contains("not shown"));
}

/// AGC-FR-09, CVL-FR-01, AAP-FR-21's second clause: *every* builder, not just the artifact one.
#[test]
fn the_draft_builder_names_attachments_on_the_same_terms() {
    use base64::Engine as _;
    let png = b"\x89PNG-draft";
    let encoded = base64::engine::general_purpose::STANDARD.encode(png);

    let h = Harness::new(vec![]);
    let created =
        crate::drafts::create_draft_at_root(&h.root(), Some("Review me")).expect("draft");
    let draft_id = created.draft.id.clone();
    let file_rel = created.file.clone();
    let thread = seed_draft_thread(&h, &draft_id, &file_rel);
    crate::comments::append_as_scoped(
        &h.root(),
        LogScope::Draft { draft_id: &draft_id },
        &file_rel,
        &human("ada"),
        "2026-01-01T00:00:02Z",
        vec![(
            thread.id.clone(),
            crate::comments::EventBody::CommentAdded {
                comment_id: "c-draft".into(),
                body: "here is the mock".into(),
                quotes: vec![],
                attachments: vec![crate::comments::Attachment::Blob {
                    digest: "deadbeef".into(),
                    media_type: "image/png".into(),
                    filename: "mock.png".into(),
                    bytes: png.len() as u64,
                }],
            },
        )],
    )
    .expect("draft comment with an attachment");

    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        "c-draft",
    );
    let current = section_of(&sections, TAG_CURRENT_COMMENT);
    assert!(current.body.contains("mock.png"));
    assert!(current.body.contains("not shown"));
    for s in &sections {
        assert!(
            !s.body.contains(&encoded) && !s.body.contains("deadbeef"),
            "no payload and no digest reaches the request, in section {:?}",
            s.tag,
        );
    }
}

/// AGC-FR-16 / AGC-FR-17: an agent contributes prose and never a file.
#[test]
fn a_delivered_agent_comment_carries_no_attachment() {
    use base64::Engine as _;
    let h = Harness::new(vec![]);
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    crate::comments::add_comment_in(
        &h.root(),
        "spec.md",
        &thread.id,
        "with a picture".into(),
        vec![],
        vec![crate::comments::AttachmentInput::Inline {
            media_type: "image/png".into(),
            filename: "shot.png".into(),
            data: base64::engine::general_purpose::STANDARD.encode(b"bytes"),
        }],
        &human("ada"),
        "2026-01-01T00:00:02Z",
    )
    .expect("author's attachment");

    crate::comments::append_agent_comment(
        &h.root(),
        LogScope::Artifact,
        "spec.md",
        &thread.id,
        "I cannot see it, but I take your word for it.".into(),
        &Participant::Agent {
            agent_id: "id".into(),
            handle: "arch".into(),
            model: Some("m".into()),
            title: None,
        },
        "2026-01-01T00:00:03Z",
    )
    .expect("agent comment");

    let folded = &h.threads("spec.md")[0];
    let agent_comment = folded.comments.last().unwrap();
    assert!(
        matches!(agent_comment.author, Participant::Agent { .. }),
        "the last comment is the agent's",
    );
    assert!(
        agent_comment.attachments.is_empty(),
        "an agent contributes prose and never a file",
    );
    let total: usize = folded.comments.iter().map(|c| c.attachments.len()).sum();
    assert_eq!(total, 1, "the only attachment is the one the author attached");
}

#[test]
fn the_history_ends_at_the_comment_that_addressed_the_agent() {
    // AGC-FR-13: a message posted while a turn is in flight belongs
    // to the next turn rather than changing the one already running.
    let h = Harness::new(vec![]);
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    let trigger = thread.comments[0].id.clone();
    crate::comments::add_comment_in(
        &h.root(),
        "spec.md",
        &thread.id,
        "A later comment nobody asked about.".into(),
        vec![],
        Vec::new(),
        &human("ada"),
        "2026-01-01T00:00:02Z")
    .expect("later comment");

    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &trigger,
    );
    // The trigger is the thread's first comment, so the history holds the anchor
    // and no comment at all — and the later one is in neither section.
    assert!(section_of(&sections, TAG_CURRENT_COMMENT)
        .body
        .contains("what do you think?"));
    for tag in INPUT_TAGS {
        assert!(
            !section_of(&sections, tag).body.contains("A later comment"),
            "a comment posted after the trigger reached <{tag}>",
        );
    }
}

#[test]
fn a_section_past_its_bound_says_that_it_is_partial() {
    // AGC-FR-11.
    //
    // Multibyte throughout, because the bound is counted in characters and read
    // back by character: a byte-sliced implementation passes an all-ASCII
    // fixture and panics on the first Cyrillic or CJK artifact, which in a specs
    // repository is an ordinary document rather than an exotic one.
    let h = Harness::new(vec![]);
    let big = "日".repeat(SECTION_MAX_CHARS + 100);
    let thread = h.seed_artifact_thread("big.md", &big);
    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &thread.comments[0].id,
    );
    let source = section_of(&sections, TAG_ARTIFACT);
    assert!(source.truncated);
    assert!(source.body.contains("partial"));
    // It actually shrank, and what was kept is the head. Without these an
    // implementation that appended the marker to the whole body would pass.
    assert!(source.body.chars().count() < big.chars().count());
    assert!(source.body.starts_with(&"日".repeat(100)));

    let small = h.seed_artifact_thread("small.md", "Short enough for one section.");
    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&small),
        &small.comments[0].id,
    );
    assert!(!section_of(&sections, TAG_ARTIFACT).truncated);

    // The bound itself: exactly at it is whole, one past it is partial.
    assert!(!bounded("é".repeat(SECTION_MAX_CHARS)).1);
    assert!(bounded("é".repeat(SECTION_MAX_CHARS + 1)).1);
}

#[test]
fn an_unreadable_artifact_costs_its_section_alone_and_no_material_at_all_fails_the_turn() {
    // AGC-FR-12.
    let h = Harness::new(vec![Ok("ok".into())]);
    h.create_agent("arch", "");
    let thread = h.seed_artifact_thread("spec.md", "Some artifact source here.");
    std::fs::remove_file(h.root().join("spec.md")).expect("remove");

    let sections = build_input(Roots::same(&h.root()),
        &ConversationOrigin::of(&thread),
        &thread.comments[0].id,
    );
    assert_eq!(
        tags_of(&sections),
        vec![TAG_DISCUSSION_HISTORY, TAG_CURRENT_COMMENT],
        "the rest is still assembled",
    );

    assert!(build_input(Roots::same(&h.root()),
        &ConversationOrigin::stub_artifact("no-such-thread", "a.md", true),
        "x",
    )
    .is_empty());

    let turn = h
        .dispatch(
            "arch",
            ConversationOrigin::stub_artifact("no-such-thread", "a.md", true),
            "x",
        )
        .expect("dispatch registers");
    let failure = wait_for_terminal(&h, &turn.id);
    assert_eq!(failure.state, AgentTurnState::Failed);
    assert_eq!(failure.failure.as_deref(), Some(FAIL_CONTEXT_UNAVAILABLE));
}

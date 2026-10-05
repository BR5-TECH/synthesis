//! The pieces a context section is assembled from (AGC-FR-05).
//!
//! One tagged section at a time: the bounding, the attribution, the quoted
//! material, the attachments, and the pictures. The per-origin-kind builders
//! that order them into an input stand in `builders`.

use super::*;

// ---------------------------------------------------------------------------
// Input builders (AGC-FR-05)
// ---------------------------------------------------------------------------

/// AGC-FR-11: bound a section's material, and say so in the section's own text
/// when it was bounded. An agent is never silently handed half a document.
pub(super) fn bounded(body: String) -> (String, bool) {
    if body.chars().count() <= SECTION_MAX_CHARS {
        return (body, false);
    }
    let mut truncated: String = body.chars().take(SECTION_MAX_CHARS).collect();
    truncated.push_str("\n\n[This material is partial: it was truncated here.]");
    (truncated, true)
}

pub(super) fn section(tag: &str, attributes: Vec<(String, String)>, body: String) -> InputSection {
    let (body, truncated) = bounded(body);
    InputSection {
        tag: tag.to_string(),
        attributes,
        body,
        truncated,
        parts: Vec::new(),
    }
}

/// AGC-FR-35: a section built from **ordered parts**, one of which may be an
/// image.
///
/// The two renderings are assembled together rather than one being derived from
/// the other: `body` is the text form, with each image's safe metadata standing
/// where the image is (AGC-FR-37), and `parts` is the multimodal form. Which of
/// the two a turn sends is decided by the endpoint's capability alone
/// (AGC-FR-36), and neither is a partial version of the other.
///
/// The bound of AGC-FR-11 applies to the section's **text**: an image is not
/// prose and does not count toward it, and a section whose text runs past the
/// bound is truncated there and carries no part after that point.
pub(super) fn section_of_parts(
    tag: &str,
    attributes: Vec<(String, String)>,
    parts: Vec<InputPart>,
) -> InputSection {
    let mut body = String::new();
    let mut kept: Vec<InputPart> = Vec::new();
    let mut truncated = false;
    for part in parts {
        if truncated {
            break;
        }
        match part {
            InputPart::Text(text) => {
                let remaining = SECTION_MAX_CHARS.saturating_sub(body.chars().count());
                if text.chars().count() > remaining {
                    let cut: String = text.chars().take(remaining).collect();
                    body.push_str(&cut);
                    body.push_str("\n\n[This material is partial: it was truncated here.]");
                    kept.push(InputPart::Text(format!(
                        "{cut}\n\n[This material is partial: it was truncated here.]"
                    )));
                    truncated = true;
                } else {
                    body.push_str(&text);
                    kept.push(InputPart::Text(text));
                }
            }
            InputPart::Image(image) => {
                body.push_str(&image.metadata);
                kept.push(InputPart::Image(image));
            }
        }
    }
    InputSection {
        tag: tag.to_string(),
        attributes,
        body,
        truncated,
        // A section that turned out to hold no image is an ordinary text
        // section, and carrying a single text part for it would make every
        // text-only turn assemble a content list it does not need.
        parts: if kept.iter().any(|p| matches!(p, InputPart::Image(_))) {
            kept
        } else {
            Vec::new()
        },
    }
}

/// AGC-FR-08: every comment names its author by handle — an agent's nickname and
/// a human's login alike, from the participant the log stamped — so an agent
/// answering a thread that other agents are in can tell who said what.
pub(super) fn handle_of(participant: &Participant) -> &str {
    match participant {
        Participant::Human { login, .. } => login,
        Participant::Agent { handle, .. } => handle,
    }
}

/// AGC-FR-09: an attachment named, never carried.
///
/// The agent is told what was attached to a message — enough to ask about it, or
/// to say it cannot see it — and is shown none of the content. Nothing here
/// decodes a blob or resolves a URL, so the assembled request stays text however
/// much the conversation is carrying.
fn attachment_line(attachment: &comments::Attachment, digests: &ProposalDigests) -> String {
    match attachment {
        comments::Attachment::Blob {
            filename,
            media_type,
            ..
        } => format!("[attachment: {filename} ({media_type}), not shown]"),
        comments::Attachment::Url {
            url,
            media_type,
            label,
        } => {
            let name = label.as_deref().filter(|l| !l.is_empty()).unwrap_or(url);
            format!("[attachment: {name} ({media_type}), not shown]")
        }
        // AGC-FR-RVQP: a proposal reference carries no content of its own
        // (CMS-FR-60), so what it contributes is read from the proposal store at
        // assembly time. It names each change and how the author decided it,
        // which is what tells an agent which passages this conversation has
        // already settled. A reference the read did not answer for — a
        // `current_comment` attachment, a draft that no longer resolves — keeps
        // the bare line, the file as it now stands being in the `artifact`
        // section (AGC-FR-07).
        comments::Attachment::Proposal {
            path, proposal_id, ..
        } => match digests.get(proposal_id.as_str()) {
            Some(digest) => proposal_digest_line(path, digest),
            None => format!("[proposed a new version of {path}]"),
        },
        // CMS-FR-66: the same line for a rewrite offered against a prompt the
        // project holds. The two references name different stores and read
        // identically here, for the reason above: what an agent needs is that a
        // change was offered for that file, and the file as it now stands is in
        // the `artifact` section rather than in this line.
        comments::Attachment::PromptProposal { path, .. } => {
            format!("[proposed a new version of {path}]")
        }
    }
}

/// AGC-FR-RVQP: how many of one draft's proposals render their changes in full.
///
/// A conversation accumulates proposals without limit, and the ones that decide
/// what an agent may still touch are the recent ones. An older proposal keeps its
/// counts, so the history says a change was offered and decided without carrying
/// the text of it.
const PROPOSALS_RENDERED_IN_FULL: usize = 3;

/// AGC-FR-RVQP: how many changes of one proposal are named.
///
/// The bound exists so a digest cannot evict the conversation: a section is
/// truncated at its own bound (AGC-FR-11), and a proposal of hundreds of changes
/// would push the comments after it out of the section entirely.
pub(super) const CHANGES_RENDERED_PER_PROPOSAL: usize = 12;

/// AGC-FR-RVQP: the bound on the excerpt naming one change's text.
///
/// Enough to recognise the passage, far short of reproducing it — the draft as it
/// now stands is in the `artifact` section (AGC-FR-07).
pub(super) const CHANGE_EXCERPT_MAX_CHARS: usize = 120;

/// What one proposal contributes to `discussion_history` (AGC-FR-RVQP).
pub(super) struct ProposalDigest {
    pub(super) counts: crate::draft_proposals::hunks::HunkCounts,
    /// `None` where this proposal is old enough to contribute its counts alone.
    pub(super) changes: Option<Vec<crate::draft_proposals::ProposalChange>>,
}

/// The proposals a thread's attachments name, by proposal id.
type ProposalDigests = HashMap<String, ProposalDigest>;

/// AGC-FR-RVQP: read every proposal the thread's `proposal` attachments name.
///
/// One read per **draft** rather than one per attachment, so a conversation
/// carrying a dozen proposals of one draft costs one read. It reaches
/// `DCP-draft-change-proposals.md`'s `read_proposal_changes` (DCP-FR-HNWD),
/// which runs no reconciliation and writes nothing — a turn assembling its input
/// must not change the draft it is about to read (`../ai/CVL-conversation-loop.md`
/// CVL-FR-09).
fn proposal_digests(roots: Roots<'_>, thread: &comments::Discussion) -> ProposalDigests {
    // The references this thread actually holds, in the order it holds them.
    // Rank is taken over these rather than over the draft's proposals as a whole:
    // a draft discussed in two threads has proposals this one never mentioned,
    // and letting those decide the rank would collapse this thread's own latest
    // proposal to its counts.
    let mut referenced: Vec<(&str, &str)> = Vec::new();
    for comment in &thread.comments {
        for attachment in &comment.attachments {
            if let comments::Attachment::Proposal {
                draft_id,
                proposal_id,
                ..
            } = attachment
            {
                if !referenced.iter().any(|(_, id)| *id == proposal_id.as_str()) {
                    referenced.push((draft_id, proposal_id));
                }
            }
        }
    }

    // One read per draft rather than one per reference.
    let mut read: HashMap<String, crate::draft_proposals::ProposalChanges> = HashMap::new();
    let mut drafts: Vec<&str> = Vec::new();
    for (draft_id, _) in &referenced {
        if !drafts.contains(draft_id) {
            drafts.push(draft_id);
        }
    }
    for draft_id in drafts {
        // A draft that will not read contributes nothing: its references keep the
        // bare line rather than costing the turn its history.
        let Ok(proposals) = crate::draft_proposals::read_proposal_changes(roots.worktree, draft_id)
        else {
            continue;
        };
        for proposal in proposals {
            read.insert(proposal.id.clone(), proposal);
        }
    }

    // The last references the thread made are the ones that render in full.
    let full_from = referenced.len().saturating_sub(PROPOSALS_RENDERED_IN_FULL);
    let mut digests = ProposalDigests::new();
    for (position, (_, proposal_id)) in referenced.iter().enumerate() {
        let Some(proposal) = read.remove(*proposal_id) else {
            continue;
        };
        digests.insert(
            proposal.id,
            ProposalDigest {
                counts: proposal.counts,
                changes: (position >= full_from).then_some(proposal.changes),
            },
        );
    }
    digests
}

/// AGC-FR-RVQP: one proposal's changes and how each was decided.
pub(super) fn proposal_digest_line(path: &str, digest: &ProposalDigest) -> String {
    let counts = &digest.counts;
    let total = counts.accepted + counts.rejected + counts.undecided();
    let mut line = format!(
        "[proposed {total} {} to {path}: {} accepted, {} rejected, {} undecided]",
        if total == 1 { "change" } else { "changes" },
        counts.accepted,
        counts.rejected,
        counts.undecided(),
    );
    let Some(changes) = digest.changes.as_deref() else {
        return line;
    };
    for (position, change) in changes.iter().take(CHANGES_RENDERED_PER_PROPOSAL).enumerate() {
        let kind = match change.kind {
            crate::draft_proposals::anchors::HunkKind::Replace => "replace",
            crate::draft_proposals::anchors::HunkKind::Add => "add",
            crate::draft_proposals::anchors::HunkKind::Del => "del",
        };
        let state = match change.state {
            crate::draft_proposals::hunks::HunkState::Accepted => "accepted",
            crate::draft_proposals::hunks::HunkState::Rejected => "rejected",
            crate::draft_proposals::hunks::HunkState::Pending | crate::draft_proposals::hunks::HunkState::Discussing => {
                "undecided"
            }
        };
        line.push_str(&format!("\n  {}. {kind} — {state}", position + 1));
        if !change.before.is_empty() {
            line.push_str(&format!(" — changed: {}", excerpt(&change.before)));
        }
        // AGC-FR-RVQP: the text a change offered is rendered for a rejected or an
        // undecided change alone, an accepted change's new text already standing
        // in the `artifact` section. An **insertion** is the exception: it names
        // no existing text, so withholding what it offered would leave the change
        // carrying no text at all — and a change an agent cannot recognise is one
        // it cannot tell apart from a passage still open.
        let accepted = change.state == crate::draft_proposals::hunks::HunkState::Accepted;
        if !change.after.is_empty() && (!accepted || change.before.is_empty()) {
            line.push_str(&format!(" — offered: {}", excerpt(&change.after)));
        }
    }
    // AGC-FR-RVQP / AGC-FR-11: the changes of one proposal are bounded as well as
    // the proposals themselves. A proposal holding hundreds of changes would
    // otherwise fill the whole section and truncate the comments after it, which
    // costs the agent the most recent history — the opposite of what rendering a
    // recent proposal in full is for.
    if let Some(dropped) = changes.len().checked_sub(CHANGES_RENDERED_PER_PROPOSAL) {
        if dropped > 0 {
            line.push_str(&format!("\n  …and {dropped} more, not listed here"));
        }
    }
    line
}

/// One bounded, single-line excerpt of a change's text (AGC-FR-RVQP).
///
/// Newlines are folded so one change stays one line: the rendering is read by
/// position, and a multi-line excerpt would make the next change look like part
/// of the last.
pub(super) fn excerpt(text: &str) -> String {
    let folded = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if folded.chars().count() <= CHANGE_EXCERPT_MAX_CHARS {
        return format!("\"{folded}\"");
    }
    let kept: String = folded.chars().take(CHANGE_EXCERPT_MAX_CHARS).collect();
    format!("\"{kept}…\"")
}

/// AGC-FR-10: a quote, as the attributed pointer the model reads.
///
/// The handle of whoever wrote the quoted comment and the excerpt itself, which
/// together say *which passage of what the agent already has* is being answered
/// — a quote names a comment of the same thread (CMS-FR-16), so the body it
/// points into is elsewhere in `discussion_history` in its own place and the
/// excerpt is a pointer rather than new material.
///
/// A quote whose `comment_id` matches nothing the fold produced keeps its
/// excerpt and loses only the handle: an excerpt whose author cannot be named
/// still says which passage was meant, and dropping it would lose the one thing
/// the quote was carrying.
pub(super) fn quote_line(quote: &comments::CommentQuote, handle: Option<&str>) -> String {
    // AGC-FR-INFL: an excerpt is a part of a body, so it loses its markers on
    // the same terms, each on its own.
    let excerpt = strip_citation_markers(&quote.excerpt);
    match handle {
        Some(handle) => format!("[quoting @{handle}: {excerpt}]"),
        None => format!("[quoting: {excerpt}]"),
    }
}


/// The text a comment contributes when nothing about it is an image — the
/// rendering [`comment_parts`] produces for a section holding no picture.
///
/// Kept as its own function because it is the rendering every text-only turn
/// has always carried, and it is what the fallback of AGC-FR-37 falls back to.
#[cfg(test)]
pub(super) fn comment_text(comment: &comments::Comment, handles: &HashMap<&str, &str>) -> String {
    let mut out = format!("@{}:\n", handle_of(&comment.author));
    for quote in &comment.quotes {
        out.push_str(&quote_line(
            quote,
            handles.get(quote.comment_id.as_str()).copied(),
        ));
        out.push('\n');
    }
    out.push_str(&strip_citation_markers(&comment.body));
    out.push('\n');
    for attachment in &comment.attachments {
        out.push_str(&attachment_line(attachment, &ProposalDigests::new()));
        out.push('\n');
    }
    out
}

/// AGC-FR-09 / AGC-FR-35: one comment as **ordered parts** — its prose, then its
/// attachments in the order the comment carries them, each in the position that
/// comment occupies.
///
/// A stored **image** `blob` becomes an image part carrying its bytes; every
/// other attachment is metadata, and always was. Nothing here **fetches**: a
/// `url` attachment's address is never resolved, requested, or read, so what it
/// serves reaches no request whatever the endpoint accepts (per
/// `CMS-comments-storage.md` CMS-FR-43).
fn comment_parts(
    roots: Roots<'_>,
    thread: &comments::Discussion,
    comment: &comments::Comment,
    handles: &HashMap<&str, &str>,
    digests: &ProposalDigests,
) -> Vec<InputPart> {
    let mut prose = format!("@{}:\n", handle_of(&comment.author));
    for quote in &comment.quotes {
        prose.push_str(&quote_line(
            quote,
            handles.get(quote.comment_id.as_str()).copied(),
        ));
        prose.push('\n');
    }
    // AGC-FR-INFL: a body goes back to the model without the citation markers
    // a provider left in it, so the model does not copy them into its next
    // reply. The stored comment is not changed.
    prose.push_str(&strip_citation_markers(&comment.body));
    prose.push('\n');
    let mut parts = vec![InputPart::Text(prose)];
    for attachment in &comment.attachments {
        let line = format!("{}\n", attachment_line(attachment, digests));
        match attachment_image(roots, thread, comment, attachment, &line) {
            Some(image) => parts.push(InputPart::Image(image)),
            None => parts.push(InputPart::Text(line)),
        }
    }
    parts
}

/// AGC-FR-35: a comment's stored image attachment, read through
/// `CMS-comments-storage.md`'s `read_attachment_bytes` (CMS-FR-67).
///
/// `None` for every attachment that is not a stored image — a `url`, a PDF, a
/// plain-text file, a `proposal`, a `prompt_proposal` — and for a stored image
/// whose bytes cannot be read, which reaches the request as the metadata it
/// would otherwise have carried rather than costing the turn.
fn attachment_image(
    roots: Roots<'_>,
    thread: &comments::Discussion,
    comment: &comments::Comment,
    attachment: &comments::Attachment,
    metadata: &str,
) -> Option<InputImage> {
    let comments::Attachment::Blob {
        digest,
        media_type,
        filename,
        ..
    } = attachment
    else {
        return None;
    };
    if !media_type.split(';').next()?.trim().to_ascii_lowercase().starts_with("image/") {
        return None;
    }
    let content = comments::read_attachment_bytes(
        roots.store,
        roots.worktree,
        &thread.id,
        digest,
        thread.draft_id(),
    )
    .ok()?;
    Some(InputImage {
        data: content.data,
        media_type: content.media_type,
        // AGC-FR-35: the filename **and the comment it belongs to**, so an agent
        // is never handed a picture with nothing but a filename beside it.
        context: format!(
            "[attachment: {filename} ({media_type}), on the message @{} posted (comment {})]",
            handle_of(&comment.author),
            comment.id
        ),
        metadata: metadata.to_string(),
    })
}

/// AGC-FR-35 / AGC-FR-37: the parts a draft's prompt contributes to its
/// `artifact` section — the prompt's text interleaved with the images it
/// embeds, each in the position its Markdown reference occupies.
///
/// The images come from `DAS-draft-assets.md`'s `read_prompt_images`
/// (DAS-FR-22), which returns the bytes themselves rather than a handle and
/// **refuses nothing**: an entry that resolved to no draft-owned asset — missing,
/// malformed, unresolved by label, external, or escaping the draft — arrives
/// already marked unresolved and carrying its own `context`, and this places
/// that context where the picture would have been. An author who typed a bad
/// path is answered about the rest of their prompt.
pub(super) fn prompt_parts(
    root: &crate::fs::RootFs,
    draft_id: &str,
    text: &str,
    prefix: &str,
    suffix: &str,
) -> Vec<InputPart> {
    // Read against the very bytes this section is being built from, so an
    // entry's offsets and the text they index can never belong to two different
    // readings of the file (per `DAS-draft-assets.md` DAS-FR-22).
    let images = crate::draft_assets::read_prompt_images_of(root, draft_id, text);
    let mut parts: Vec<InputPart> = Vec::new();
    if !prefix.is_empty() {
        parts.push(InputPart::Text(prefix.to_string()));
    }
    let mut cursor = 0usize;
    for image in images {
        // The offsets are into the saved prompt; the text handed here is that
        // same prompt. A reading that has fallen out of step with it — the file
        // changed between the two reads — is caught here rather than panicking
        // on a slice boundary, and the section falls back to its plain text.
        // Both ends are checked, and both are checked for being a character
        // boundary: `end` becomes the next slice's start, so an unchecked one
        // would index into the middle of a UTF-8 sequence and panic — on any
        // prompt carrying a curly quote, an accent, or an emoji near a picture.
        // Reading the same bytes the images were read from makes this
        // unreachable; it is checked anyway, because the cost of being wrong is
        // the whole turn.
        if image.start < cursor
            || image.end > text.len()
            || image.end < image.start
            || !text.is_char_boundary(image.start)
            || !text.is_char_boundary(image.end)
        {
            return plain_prompt_parts(prefix, text, suffix);
        }
        if image.start > cursor {
            parts.push(InputPart::Text(text[cursor..image.start].to_string()));
        }
        let metadata = match image.media_type.as_deref() {
            Some(media_type) => {
                format!("[image: {} ({media_type}), not shown]", image.context)
            }
            None => format!("[image: {}, not shown]", image.context),
        };
        match (image.resolved, image.data, image.media_type) {
            (true, Some(data), Some(media_type)) => parts.push(InputPart::Image(InputImage {
                data,
                media_type,
                context: image.context,
                metadata,
            })),
            // AGC-FR-37: no bytes to send, so it contributes its metadata in its
            // own position whichever endpoint is serving — because there is
            // nothing to show rather than because anything was refused.
            _ => parts.push(InputPart::Text(metadata)),
        }
        cursor = image.end;
    }
    if cursor < text.len() {
        parts.push(InputPart::Text(text[cursor..].to_string()));
    }
    if !suffix.is_empty() {
        parts.push(InputPart::Text(suffix.to_string()));
    }
    parts
}

/// The prompt as one text part, for a draft whose reading of its own images
/// could not be trusted to line up with the bytes in hand.
fn plain_prompt_parts(prefix: &str, text: &str, suffix: &str) -> Vec<InputPart> {
    vec![InputPart::Text(format!("{prefix}{text}{suffix}"))]
}

/// AGC-FR-10: who wrote each comment of a thread, for resolving the quotes that
/// point at them.
///
/// Built once per section rather than searched per quote, so a long thread whose
/// comments all quote each other costs one pass rather than one per pointer.
pub(super) fn handles_of(thread: &comments::Discussion) -> HashMap<&str, &str> {
    thread
        .comments
        .iter()
        .map(|comment| (comment.id.as_str(), handle_of(&comment.author)))
        .collect()
}

/// AGC-FR-06: the fragment first, then the discussion's comments oldest first.
///
/// The anchor belongs here because a *thread* is anchored rather than a comment
/// is, so an agent reading the history learns what the whole conversation is
/// about before it reads a word of it.
///
/// The history stops short of the comment that addressed the agent, which
/// `current_comment` carries on its own (AGC-FR-06): a message repeated in both
/// sections would be supporting context and the thing being answered at once,
/// which is the one distinction the two tags exist to draw. AGC-FR-13: a message
/// posted while a turn is in flight is therefore absent from both, belonging to
/// the next turn rather than changing the one already running.
/// AGC-FR-06: a discussion about a whole target (CMS-FR-53) opens this section
/// with its comments directly — an absent fragment being nothing to report rather
/// than something to describe.
pub(super) fn discussion_history_section(
    roots: Roots<'_>,
    thread: &comments::Discussion,
    trigger_comment_id: &str,
) -> InputSection {
    let anchor = match thread.fragment_target.as_ref() {
        Some(fragment) => format!(
            "This conversation is anchored to characters {}..{} of the file:\n\n{}\n\n",
            fragment.start, fragment.end, fragment.quote
        ),
        None => String::new(),
    };
    let handles = handles_of(thread);
    // AGC-FR-RVQP: read once for the whole section rather than once per attachment.
    let digests = proposal_digests(roots, thread);
    // AGC-FR-09 / AGC-FR-35: a comment's attachments contribute into the section
    // holding it, in the order the comment carries them and in the position that
    // comment occupies.
    let mut parts: Vec<InputPart> = Vec::new();
    if !anchor.is_empty() {
        parts.push(InputPart::Text(anchor));
    }
    for comment in &thread.comments {
        if comment.id == trigger_comment_id {
            break;
        }
        parts.extend(comment_parts(roots, thread, comment, &handles, &digests));
        parts.push(InputPart::Text("\n".to_string()));
    }
    section_of_parts(TAG_DISCUSSION_HISTORY, Vec::new(), parts)
}

/// AGC-FR-06: the body of the comment that addressed the agent.
///
/// A trigger the thread does not hold yields an empty section rather than none:
/// every builder produces the three sections, and a turn whose thread resolved
/// is not one AGC-FR-12 fails.
pub(super) fn current_comment_section(
    roots: Roots<'_>,
    thread: &comments::Discussion,
    trigger_comment_id: &str,
) -> InputSection {
    let handles = handles_of(thread);
    let parts = thread
        .comments
        .iter()
        .find(|c| c.id == trigger_comment_id)
        .map(|comment| comment_parts(roots, thread, comment, &handles, &ProposalDigests::new()))
        .unwrap_or_default();
    section_of_parts(TAG_CURRENT_COMMENT, Vec::new(), parts)
}

/// AGC-FR-07: the `type` attribute the `artifact` section carries, or nothing at
/// all for material carrying no resolved artifact type.
///
/// Stated rather than left to be inferred from the path, because what an agent is
/// asked to do with a document turns on what kind of document it is — a skill is
/// a procedure, a specification is a decision already taken, a flow is a graph —
/// and an agent that has to guess from a filename guesses wrong for exactly the
/// artifacts whose filename does not identify them (per `ASC-artifact-scanning.md`
/// ASC-FR-19).
///
/// Absent rather than `unclassified` for a file the scan resolves no type for: a
/// value naming the absence would be a ninth type in the vocabulary, and the
/// prompt reads an attribute that is not there as the thing not being known.
pub(super) fn type_attribute(resolved: Option<scanning::ArtifactType>) -> Option<(String, String)> {
    resolved.map(|t| ("type".to_string(), t.as_str().to_string()))
}

/// AGC-FR-07: the artifact type of one file of a draft.
///
/// A draft's files are outside the project scan — they live under
/// `.synthesis/drafts/` and nothing has classified them — so the type is resolved
/// against **the path the file would occupy once the draft graduates**: the
/// draft's destination root joined with the file's draft-relative path. That is
/// the classification the same bytes will carry the moment they are published
/// (per `DRS-draft-storage.md` DRS-FR-07), so a discussion before graduation and
/// a discussion after it name the same kind of thing.
///
/// Composes the one resolver of ASC-FR-06 rather than re-deriving it, and hands
/// it the draft file's own text for the content tiebreak — which is what
/// classifies a draft still sitting at the project root, where path inference has
/// nothing to go on.
pub(super) fn draft_file_type(
    root: &crate::fs::RootFs,
    draft_id: &str,
    file_rel: &str,
) -> Option<scanning::ArtifactType> {
    let Ok(contents) = crate::drafts::load_draft_file_impl(root, draft_id, file_rel) else {
        return None;
    };
    // DRS-FR-07: a draft carries no project destination of any kind — where its
    // specification lands is chosen by the graduation agent from the captured
    // prompt, so there is no
    // published path to classify against and the file's own name is what path
    // inference has to work with.
    let as_published = file_rel.to_string();
    let body = contents.body;
    let content = move |rel: &str| -> scanning::ContentFacts {
        if rel.to_ascii_lowercase().ends_with(".md") {
            scanning::read_content_facts(&body)
        } else {
            scanning::ContentFacts::default()
        }
    };
    scanning::classify_file(&as_published, &scanning::load_assignments(root), &content).0
}

/// AGC-FR-12: whether an input holds anything to answer *against*.
///
/// A current comment on its own is the question with none of the material it is
/// a question about, so it is not material — which is why this asks after the
/// two tags rather than after an empty vector. The distinction is not reachable
/// through the builders today, both of which push a discussion history whenever
/// the thread resolves at all; it is stated here so that a builder added later
/// cannot reach a model with a question and nothing else by simply omitting a
/// section.
pub(super) fn has_material(input: &[InputSection]) -> bool {
    input
        .iter()
        .any(|s| s.tag == TAG_ARTIFACT || s.tag == TAG_DISCUSSION_HISTORY)
}

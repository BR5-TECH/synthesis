//! The wire shapes of the AGC contract surface.
//!
//! The origin a turn comes from, the state it is in, the record a surface
//! reads, and the sections an input is assembled from. Nothing here calls a
//! model or touches a disk: these are the types the rest of the module moves.

use super::*;

// ---------------------------------------------------------------------------
// Wire shapes (AGC contract surface)
// ---------------------------------------------------------------------------

/// Which conversation a turn belongs to (AGC-FR-05).
///
/// The discussion's id, its owner target, and its fragment target where it has
/// one. The backend resolves `target` and `fragment_target` from the discussion
/// the id names at dispatch (see [`normalise_origin`]), so a caller cannot misstate
/// either. What the context builder, the prompt and the tool set depend on is the
/// owner target and whether a fragment target is present, never a separate
/// conversation kind.
///
/// Two origins are equal when they name the same discussion of the same owner. The
/// fragment target is left out of the comparison because it moves with the text,
/// and the registry's filters must keep matching the conversation after it moves.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationOrigin {
    pub discussion_id: String,
    pub target: DiscussionTarget,
    #[serde(default)]
    pub fragment_target: Option<comments::FragmentTarget>,
}

impl PartialEq for ConversationOrigin {
    fn eq(&self, other: &Self) -> bool {
        self.discussion_id == other.discussion_id && self.target == other.target
    }
}

impl Eq for ConversationOrigin {}

/// CVL-FR-03: the part of an origin the prompt selection depends on, and the only
/// part of it it depends on.
///
/// Derived from the owner and the presence of a fragment. It is not stored and not
/// sent: a remark about a fragment is answered under the comment prompt, and a
/// discussion about a whole target under the discuss prompt of that owner.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OriginKind {
    ArtifactComment,
    ArtifactDiscussion,
    DraftComment,
    DraftDiscussion,
    NoteDiscussion,
}

impl ConversationOrigin {
    /// The origin of a folded discussion.
    pub fn of(discussion: &Discussion) -> Self {
        Self {
            discussion_id: discussion.id.clone(),
            target: discussion.target.clone(),
            fragment_target: discussion.fragment_target.clone(),
        }
    }

    pub fn discussion_id(&self) -> &str {
        &self.discussion_id
    }

    pub fn kind(&self) -> OriginKind {
        match (&self.target, self.fragment_target.is_some()) {
            (DiscussionTarget::Artifact { .. }, true) => OriginKind::ArtifactComment,
            (DiscussionTarget::Artifact { .. }, false) => OriginKind::ArtifactDiscussion,
            (DiscussionTarget::Draft { .. }, true) => OriginKind::DraftComment,
            (DiscussionTarget::Draft { .. }, false) => OriginKind::DraftDiscussion,
            (DiscussionTarget::Note { .. }, _) => OriginKind::NoteDiscussion,
        }
    }

    /// CVL-FR-08: whether this conversation is about a draft, which is what
    /// decides the ninth tool.
    ///
    /// A conversation about unpublished working material is one an agent may
    /// offer a rewrite of; one about a file already in the project is not.
    pub fn is_draft(&self) -> bool {
        matches!(self.target, DiscussionTarget::Draft { .. })
    }

    /// CVL-FR-08: whether this conversation is about a file the project already
    /// holds, which is what decides the other ninth tool.
    ///
    /// A conversation about a published file is one an agent may offer a rewrite
    /// of **where that file is a prompt** — a condition the tool checks for
    /// itself (PPC-FR-06), this being a fact about the file rather than about
    /// the origin.
    pub fn is_artifact(&self) -> bool {
        matches!(self.target, DiscussionTarget::Artifact { .. })
    }

    /// CVL-FR-08: whether this conversation is about a whole target, which is what
    /// decides `ask_discussion_questions`.
    ///
    /// A whole-target discussion is where material is decided rather than
    /// corrected, and an agent reading a whole document finds several open points
    /// where one reading a fragment finds one (ADQ-FR-LFDX). A fragment discussion
    /// is one remark, so it is attached none.
    pub fn is_discussion(&self) -> bool {
        self.fragment_target.is_none()
    }

    /// The draft this conversation is about, for a caller that needs it without
    /// re-matching on the shape.
    pub fn draft_id(&self) -> Option<&str> {
        match &self.target {
            DiscussionTarget::Draft { draft_id } => Some(draft_id),
            _ => None,
        }
    }

    /// The note this conversation is about.
    pub fn note_id(&self) -> Option<&str> {
        match &self.target {
            DiscussionTarget::Note { note_id } => Some(note_id),
            _ => None,
        }
    }
}

/// Test-only constructors for an origin whose discussion need not exist in a store.
#[cfg(test)]
impl ConversationOrigin {
    pub(crate) fn stub_artifact(discussion_id: impl Into<String>, artifact_id: impl Into<String>, fragment: bool) -> Self {
        let artifact_id = artifact_id.into();
        Self {
            discussion_id: discussion_id.into(),
            fragment_target: fragment.then(|| comments::FragmentTarget::in_artifact(&artifact_id, 0, 1, "x")),
            target: DiscussionTarget::Artifact { artifact_id },
        }
    }

    pub(crate) fn stub_draft(discussion_id: impl Into<String>, draft_id: impl Into<String>, fragment: bool) -> Self {
        let draft_id = draft_id.into();
        Self {
            discussion_id: discussion_id.into(),
            fragment_target: fragment.then(|| comments::FragmentTarget::in_draft(&draft_id, "a.md", 0, 1, "x")),
            target: DiscussionTarget::Draft { draft_id },
        }
    }

    pub(crate) fn stub_note(discussion_id: impl Into<String>, note_id: impl Into<String>) -> Self {
        Self {
            discussion_id: discussion_id.into(),
            fragment_target: None,
            target: DiscussionTarget::Note { note_id: note_id.into() },
        }
    }
}

impl OriginKind {
    /// The kind as a log field. The wire spelling of the origin's tag, so a
    /// record found in the Logs panel searches on the same term the payload of
    /// `"agent turn state changed"` carries.
    pub fn as_str(self) -> &'static str {
        match self {
            OriginKind::ArtifactComment => "artifact_comment",
            OriginKind::ArtifactDiscussion => "artifact_discussion",
            OriginKind::DraftComment => "draft_comment",
            OriginKind::DraftDiscussion => "draft_discussion",
            OriginKind::NoteDiscussion => "note_discussion",
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentTurnState {
    Running,
    Delivered,
    /// AGC-FR-28: the turn asked the author something and ended on it.
    ///
    /// Terminal and outstanding at once, which is the one state that is both.
    /// *Terminal*: the loop made its last model call, and the exchange, the
    /// session, the slot, and the progress operation are all released — the turn
    /// is discarded, silently, exactly as a delivered one is. *Outstanding*:
    /// `list_agent_turns` keeps returning it, because the registration is what
    /// tells the next comment in that conversation which agent is owed a reply.
    /// Nothing is held open, nothing is watched, and no bound applies to the
    /// waiting (AGC-FR-28).
    AwaitingReply,
    Failed,
    Cancelled,
}

impl AgentTurnState {
    /// The state as a log field, in its wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            AgentTurnState::Running => "running",
            AgentTurnState::Delivered => "delivered",
            AgentTurnState::AwaitingReply => "awaiting_reply",
            AgentTurnState::Failed => "failed",
            AgentTurnState::Cancelled => "cancelled",
        }
    }
}

/// AGC-FR-RWPT: why the provider's certificate was refused. The host has no port,
/// path, or user information, and nothing else of the request is in it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTlsFailure {
    pub host: String,
    pub cause: String,
}

/// One agent, answering one message, in one conversation (AGC-FR-01).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AgentTurn {
    /// Unique for the lifetime of the running application. Nothing about a turn
    /// survives a relaunch (AGC-FR-23), so uniqueness needs no more than that.
    pub id: String,
    pub agent_id: String,
    pub nickname: String,
    pub origin: ConversationOrigin,
    /// The comment that addressed the agent.
    pub trigger_comment_id: String,
    pub state: AgentTurnState,
    pub failure: Option<String>,
    /// AGC-FR-RWPT: the host and the cause of a refused certificate. Present
    /// only when `failure` is `tls_untrusted`.
    pub tls_failure: Option<AgentTlsFailure>,
    /// AGC-FR-31: true only while this turn is its conversation's **current**
    /// recoverable failure.
    ///
    /// A consumer that was watching when the turn failed learns from that one
    /// terminal event whether an offer to retry stands, and never has to derive
    /// it from the failure value — which it could not do anyway, `timed_out`
    /// naming both the retryable per-call deadline and the whole-turn one that
    /// is not (CVL-FR-17, CVL-FR-18). A consumer that was not watching reads the
    /// same thing from `list_recoverable_agent_turn_failures`.
    pub retry_permitted: bool,
    pub started_at: String,
    /// `None` while running.
    pub ended_at: Option<String>,
    /// AGC-FR-33: the tool calls **active** in this turn — those the loop has
    /// begun and has not yet finished — ordered by activation.
    ///
    /// What is happening now and never what has happened: a turn that has made
    /// no tool call carries an empty list, a call leaves the list the moment it
    /// succeeds, refuses, or is abandoned, and a turn in any terminal state
    /// carries an empty list whatever was active when it ended (AGC-FR-34).
    /// Names and their order and nothing else — no argument the model composed,
    /// no result a tool produced, and no part of the exchange (CVL-FR-26).
    pub active_tool_calls: Vec<ActiveToolCall>,
    /// AGC-FR-37: true where the turn sent **text and safe image metadata** in
    /// place of the pictures its material held, because the resolved endpoint
    /// does not take image content (AGC-FR-36).
    ///
    /// Set from the moment that decision is taken and carried in every payload
    /// the turn appears in, its terminal event among them — which is what lets
    /// the surface that dispatched it say that the pictures were not sent (per
    /// `../ui/CMT-comments.md` CTA-FR-XSGX).
    ///
    /// It is **not a failure**: the turn is an ordinary successful conversation,
    /// carries no value of `AgentTurnFailure`, is not recoverable, is entered in
    /// no recovery registry, and its answer is delivered into its origin exactly
    /// as any answer is.
    #[serde(default)]
    pub images_omitted: bool,
}

/// AGC-FR-33: one tool call the loop has begun and has not yet finished.
///
/// The tool the model called — a portable tool's own name, and a
/// provider-native tool's entry type (TLC-FR-02, TLC-FR-21) — under an id
/// unique within the turn and an **activation sequence**. The sequence is what
/// orders them, so two calls a single model response began are ordered among
/// themselves and a consumer reads which one is the latest from the record
/// rather than from the order events reached it (per
/// `../ui/CMT-comments.md` CTA-FR-KWOF).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ActiveToolCall {
    /// Unique within the turn, and never reused within it.
    pub id: String,
    /// The tool's name, and nothing drawn from the call itself.
    pub tool: String,
    /// An integer ascending with activation, unique within the turn, never
    /// reused.
    pub activation_seq: u64,
}

/// AGC-FR-06: the three tags every builder emits, in the order they are
/// presented. The input carries no fourth, and the prompt names exactly these
/// (CVL-FR-06) — which is what stops the instruction telling a model how to read
/// the material and the material itself from drifting apart.
pub const TAG_ARTIFACT: &str = "artifact";
pub const TAG_DISCUSSION_HISTORY: &str = "discussion_history";
pub const TAG_CURRENT_COMMENT: &str = "current_comment";

/// AGC-FR-06: the material tag of a **note** turn, which stands where
/// [`TAG_ARTIFACT`] stands on the four origins whose subject is a file or a
/// draft.
///
/// Exactly one of the two occurs in any one input: a note turn carries no
/// `artifact` section, so nothing offers the agent the artifact the note happens
/// to be filed against as the thing being discussed.
pub const TAG_NOTE_CONTEXT: &str = "note_context";

/// The tag vocabulary of a file-or-draft turn, in presentation order.
pub const INPUT_TAGS: [&str; 3] = [TAG_ARTIFACT, TAG_DISCUSSION_HISTORY, TAG_CURRENT_COMMENT];

/// The tag vocabulary of a note turn, in presentation order (AGC-FR-06).
pub const NOTE_INPUT_TAGS: [&str; 3] = [
    TAG_NOTE_CONTEXT,
    TAG_DISCUSSION_HISTORY,
    TAG_CURRENT_COMMENT,
];

/// CVL-FR-06: the tags a turn of this origin kind emits, in order.
///
/// The parity check of CVL-FR-06 reads this and the prompt the same kind
/// selects, so the instruction and the material cannot drift apart across a
/// change to either.
pub fn input_tags(kind: OriginKind) -> [&'static str; 3] {
    match kind {
        OriginKind::NoteDiscussion => NOTE_INPUT_TAGS,
        _ => INPUT_TAGS,
    }
}

/// One named section of what the agent is shown (AGC-FR-06).
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InputSection {
    /// One of [`INPUT_TAGS`].
    pub tag: String,
    /// The tag's own attributes, in the order they are rendered.
    pub attributes: Vec<(String, String)>,
    /// The section's whole material as **text**, with safe metadata standing
    /// where each image is (AGC-FR-37).
    ///
    /// This is what a text-only request carries and what [`render_input`]
    /// renders, so a turn whose endpoint takes no image needs nothing stripped
    /// out or rewritten — the text form was assembled beside the multimodal one
    /// and each is complete on its own.
    pub body: String,
    /// AGC-FR-11: true when the section holds less than the whole of its
    /// material.
    pub truncated: bool,
    /// AGC-FR-35: the section's **ordered content parts**, where it holds an
    /// image. Empty for a section that is all text, which is every section of
    /// every text-only turn.
    ///
    /// An image occupies a part of its own in the position the material puts it,
    /// because a diagram means what the sentence before it says it means. The
    /// order here is the prompt's or the comment history's and is never
    /// rearranged: an image is never re-encoded into prose, never replaced by
    /// its filename, and never moved to the head or the foot of the input (per
    /// `../ai/CVL-conversation-loop.md` CVL-FR-37).
    ///
    /// **Never serialised.** A turn's records, events, and reports carry no
    /// image and no encoded payload (AGC-FR-40, CVL-FR-26), and the surest way
    /// to keep that true of a field holding megabytes of base64 is for it to
    /// have no wire form at all.
    #[serde(skip)]
    pub parts: Vec<InputPart>,
}

/// AGC-FR-35: one ordered part of a section's material.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum InputPart {
    /// Prose, exactly as the section's own text carries it.
    Text(String),
    /// One image, in the position the material puts it.
    Image(InputImage),
}

/// AGC-FR-35: an image part — the bytes, the media type, and the context the
/// material gives it.
///
/// Both sources hand this module the **bytes themselves** rather than a handle:
/// `read_prompt_images` returns each resolved image's data in its own entry
/// (per `DAS-draft-assets.md` DAS-FR-22), and `read_attachment_bytes` returns an
/// attachment's content on the same terms (per `CMS-comments-storage.md`
/// CMS-FR-67), so assembling a turn takes no second fetch and no path composed
/// here.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct InputImage {
    /// The image's own bytes, base64-encoded.
    pub data: String,
    /// The IANA media type of those bytes.
    pub media_type: String,
    /// The surrounding context the material gives it — a prompt image's Markdown
    /// reference together with its alt text, and an attachment's filename
    /// together with the comment it belongs to. Stands as text beside the image
    /// in that same position, so an agent is never handed a picture with nothing
    /// but a filename beside it (CVL-FR-37).
    pub context: String,
    /// AGC-FR-37: what stands in the image's place in a text-only request — its
    /// media type together with what identifies it, carrying no bytes, no
    /// encoded payload, and nothing fetched from any address.
    ///
    /// Composed beside the image rather than derived from it later, so the two
    /// renderings of one section cannot disagree about what was there.
    pub metadata: String,
}

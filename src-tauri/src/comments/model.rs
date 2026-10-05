//! The wire shapes of the comments contract surface.

use super::*;

// ---------------------------------------------------------------------------
// Wire shapes (the Contract surface of CMS-comments-storage.md)
// ---------------------------------------------------------------------------

/// CMS-FR-09: who produced an event.
///
/// A tagged union rather than one shape with an `is_agent` flag, so nothing about
/// a human's record has to bend to fit an agent: a human is a GitHub account, an
/// agent is an integration. A third kind can be added without touching either.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Participant {
    #[serde(rename_all = "camelCase")]
    Human {
        login: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        display_name: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Agent {
        agent_id: String,
        handle: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        /// CMS-FR-65: the agent's title as the registry held it when this line
        /// was appended — a **snapshot**, taken once and never resolved again,
        /// which is what lets a conversation read as it was held after the
        /// agent has been retitled.
        ///
        /// `Option` rather than `String` because absence is a distinct state
        /// this module has to preserve rather than paper over: a line written
        /// before the field existed carries none, and folding it must not
        /// invent one. Every append path this build has stamps `Some(_)` — the
        /// empty string included, for an agent that carries no title — so only
        /// an older log produces `None`. Both render alike (CTA-FR-KYPK), so the
        /// distinction costs a reader nothing while keeping an old log honest.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        title: Option<String>,
    },
}

impl Participant {
    /// CMS-FR-HTOA: the fixed human a project that stores no GitHub token writes
    /// as — an empty login, the display name `Me`, and no email.
    pub fn local_human() -> Self {
        Participant::Human {
            login: String::new(),
            display_name: Some(LOCAL_PARTICIPANT_NAME.to_string()),
            email: None,
        }
    }
}

impl From<GithubIdentity> for Participant {
    fn from(identity: GithubIdentity) -> Self {
        Participant::Human {
            login: identity.login,
            display_name: identity.display_name,
            email: identity.email,
        }
    }
}

/// A passage position as a legacy log line stored it (`thread_opened`,
/// `thread_reanchored`).
///
/// Read-only: no line this build writes carries one. The fold turns it into a
/// [`FragmentTarget`] on read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyAnchor {
    pub start: usize,
    pub end: usize,
    pub quote: String,
}

/// CMS-FR-22: the part of an owner's source a discussion is about.
///
/// `start`/`end` are offsets in **Unicode scalar values**, not bytes, so a fragment
/// means the same range whatever encodes the surrounding text. This module stores
/// and serves them and never reads the source to check them: matching a fragment
/// against content belongs to the owner surface, which holds the buffer the user is
/// editing.
///
/// Only an `artifact` or a `draft` owner can have a fragment. `path` is the
/// project-relative path for an artifact and the draft-relative path of the draft
/// file for a draft.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FragmentTarget {
    pub owner: DiscussionTarget,
    pub path: String,
    pub start: usize,
    pub end: usize,
    pub quote: String,
}

/// CMS-FR-16: a comment quoting an earlier one in the same thread.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentQuote {
    pub comment_id: String,
    pub excerpt: String,
}

/// CMS-FR-42: an attachment as stored and as served back.
///
/// Three kinds rather than one shape with optional fields, because what an
/// attachment *is* differs at every match site and a tagged union makes that
/// unmissable. A `url` is an address this module records and never resolves
/// (CMS-FR-43); a `blob` is content it holds, named by the digest of its bytes so
/// the same picture attached twice is one file (CMS-FR-44); a `proposal` holds no
/// content at all (CMS-FR-60).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Attachment {
    #[serde(rename_all = "camelCase")]
    Url {
        url: String,
        media_type: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Blob {
        digest: String,
        media_type: String,
        filename: String,
        bytes: u64,
    },
    /// CMS-FR-60: a reference to a change an agent proposed to a draft file
    /// (`DCP-draft-change-proposals.md` DCP-FR-05).
    ///
    /// It carries the proposal's **identity** and nothing else — no bytes, no
    /// address, no media type — which is what makes it immutable on exactly the
    /// terms CMS-FR-50 sets for the other two even though the proposal it names
    /// gets accepted or rejected afterwards: the state lives in that module, and
    /// what is stored here never changes. This module validates nothing about it
    /// and never reads the proposal, so a reference whose draft has since gone
    /// folds and serves exactly as one whose proposal still stands — the same
    /// relationship a `url` has to whatever its address stops serving.
    #[serde(rename_all = "camelCase")]
    Proposal {
        proposal_id: String,
        draft_id: String,
        /// Draft-relative path the proposal would replace.
        path: String,
    },
    /// CMS-FR-66: a reference to a change an agent proposed to a **prompt
    /// artifact the project already holds** (`PCP-prompt-change-proposals.md`
    /// PCP-FR-05).
    ///
    /// Separate from [`Attachment::Proposal`] rather than one kind carrying a
    /// target, because the two name two stores with two lifetimes: a `proposal`
    /// dies with the draft folder it points into, while this one
    /// names a file of the project that outlives every conversation about it.
    ///
    /// Like its sibling it carries the proposal's **identity** and nothing else
    /// — no bytes, no address, no media type — and this module validates
    /// nothing about it: it neither reads the proposal, nor checks that it
    /// exists, nor learns of its decision, so a reference whose artifact has
    /// since gone folds and serves exactly as one whose proposal still stands.
    #[serde(rename_all = "camelCase")]
    PromptProposal {
        proposal_id: String,
        artifact_id: String,
        /// Project-relative path the proposal would replace.
        path: String,
    },
}

/// CMS-FR-43: what a caller supplies on an append.
///
/// Distinct from [`Attachment`] on purpose, and in two ways. A caller may not
/// name a `digest`, or it could claim content it never supplied and produce a
/// comment pointing at another comment's file; an `inline` input carries bytes
/// and this module derives the digest itself. And there is deliberately **no
/// `proposal` variant** (CMS-FR-60): a proposal reference is what a surface
/// renders an accept-this-rewrite control from, so a frontend able to compose one
/// could offer the author a control that applies text no agent ever proposed. It
/// reaches a comment through `append_as` alone, called by
/// `crate::draft_proposals`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum AttachmentInput {
    #[serde(rename_all = "camelCase")]
    Url {
        url: String,
        media_type: String,
        #[serde(default)]
        label: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Inline {
        media_type: String,
        filename: String,
        /// Base64 of the file's bytes.
        data: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    pub id: String,
    pub author: Participant,
    /// Markdown source. Rendered as rich text by the rail (CMT-FR-09).
    pub body: String,
    #[serde(default)]
    pub quotes: Vec<CommentQuote>,
    /// CMS-FR-42: empty when the comment attaches nothing, which is the ordinary
    /// case. `default` so a line written before attachments existed still folds.
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    pub created_at: String,
}

/// What `read_comment_attachment` serves back (CMS-FR-48).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AttachmentContent {
    pub media_type: String,
    pub filename: String,
    /// Base64 of the stored bytes.
    pub data: String,
}

impl FragmentTarget {
    /// A fragment of an artifact's source, with the path the artifact's own.
    pub fn in_artifact(artifact_id: &str, start: usize, end: usize, quote: &str) -> Self {
        Self {
            owner: DiscussionTarget::Artifact {
                artifact_id: artifact_id.to_string(),
            },
            path: artifact_id.to_string(),
            start,
            end,
            quote: quote.to_string(),
        }
    }

    /// A fragment of one file of a draft, with the draft-relative path of that file.
    pub fn in_draft(draft_id: &str, path: &str, start: usize, end: usize, quote: &str) -> Self {
        Self {
            owner: DiscussionTarget::Draft {
                draft_id: draft_id.to_string(),
            },
            path: path.to_string(),
            start,
            end,
            quote: quote.to_string(),
        }
    }
}

/// A discussion as the fold produces it (CMS-FR-09).
///
/// One record for every conversation. `target` is the owner. `fragment_target` is
/// `null` for a discussion about the whole target and holds the fragment for one
/// about a part of an artifact or a draft prompt. **Serialized as an explicit
/// `null`** when absent, so a caller reads "no fragment" and not "a fragment I have
/// not resolved".
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Discussion {
    pub id: String,
    pub target: DiscussionTarget,
    #[serde(default)]
    pub fragment_target: Option<FragmentTarget>,
    /// Oldest first.
    pub comments: Vec<Comment>,
    pub locked: bool,
    pub resolved: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl Discussion {
    /// The log this discussion is written in.
    pub fn log_ref(&self) -> ThreadRef<'_> {
        opening::log_ref_of(&self.target, self.fragment_target.as_ref())
    }

    /// Whether the discussion is about a part of its owner.
    pub fn is_fragment_targeted(&self) -> bool {
        self.fragment_target.is_some()
    }

    /// The artifact path of an artifact-owned discussion.
    pub fn artifact_id(&self) -> Option<&str> {
        match &self.target {
            DiscussionTarget::Artifact { artifact_id } => Some(artifact_id),
            _ => None,
        }
    }

    pub fn draft_id(&self) -> Option<&str> {
        match &self.target {
            DiscussionTarget::Draft { draft_id } => Some(draft_id),
            _ => None,
        }
    }

    pub fn note_id(&self) -> Option<&str> {
        match &self.target {
            DiscussionTarget::Note { note_id } => Some(note_id),
            _ => None,
        }
    }

    /// The file the discussion is about: the fragment's path, else the artifact of
    /// an artifact-owned discussion, else `""`.
    pub fn file_rel(&self) -> &str {
        match &self.fragment_target {
            Some(fragment) => fragment.path.as_str(),
            None => self.artifact_id().unwrap_or(""),
        }
    }

    /// The path of the fragment, or `""` for a whole-target discussion.
    pub fn fragment_path(&self) -> &str {
        self.fragment_target.as_ref().map_or("", |f| f.path.as_str())
    }
}

// ---------------------------------------------------------------------------
// On-disk shape: one event per line
// ---------------------------------------------------------------------------

/// CMS-FR-05: the event vocabulary. `discussion_opened` begins a discussion and
/// `fragment_moved` moves its fragment. There is deliberately no event that edits
/// or removes a comment: a discussion that has outlived its usefulness is resolved
/// rather than erased (CMT-FR-14).
///
/// Read tolerantly, written in one shape. `thread_opened` and `thread_reanchored`
/// are lines older builds wrote. A log is append-only and never rewritten
/// (CMS-FR-01), so those lines stay in the file and the fold reads them as the
/// unified events (CMS-FR-04). This build writes neither of them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventBody {
    /// Legacy. Read as a `discussion_opened` carrying a fragment.
    #[serde(rename_all = "camelCase")]
    ThreadOpened {
        artifact_path: String,
        anchor: LegacyAnchor,
    },
    /// CMS-FR-53 / CMS-FR-57: the opening event of a discussion, naming its owner
    /// and, where it has one, its fragment. Fixed here and never changed except
    /// for the fragment's position (`fragment_moved`).
    ///
    /// `draft_id` is only ever read: lines from before a discussion could be about
    /// anything but a draft name the draft directly.
    #[serde(rename_all = "camelCase")]
    DiscussionOpened {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<DiscussionTarget>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        draft_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fragment_target: Option<FragmentTarget>,
    },
    #[serde(rename_all = "camelCase")]
    CommentAdded {
        comment_id: String,
        body: String,
        #[serde(default)]
        quotes: Vec<CommentQuote>,
        /// CMS-FR-42: references rather than content, so a log line stays a
        /// line however large the picture attached to it and folding a
        /// discussion never decodes a payload.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        attachments: Vec<Attachment>,
    },
    ThreadLocked,
    ThreadUnlocked,
    ThreadResolved,
    ThreadReopened,
    /// Legacy. Read as a `fragment_moved`.
    #[serde(rename_all = "camelCase")]
    ThreadReanchored { anchor: LegacyAnchor },
    #[serde(rename_all = "camelCase")]
    FragmentMoved { fragment_target: FragmentTarget },
}

/// One line of a log (CMS-FR-04).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub v: u32,
    pub event_id: String,
    pub thread_id: String,
    pub at: String,
    pub by: Participant,
    #[serde(flatten)]
    pub body: EventBody,
}


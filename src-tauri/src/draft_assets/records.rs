//! The record shapes the asset commands answer with.

use super::*;

// ---------------------------------------------------------------------------
// Record shapes
// ---------------------------------------------------------------------------

/// One stored asset, as `store_draft_image` answers with it (DAS-FR-02,
/// DAS-FR-05).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftAsset {
    /// The draft-relative path of the stored file: `assets/<opaque-id>.<ext>`.
    pub path: String,
    /// The Markdown destination that resolves to it from the prompt:
    /// `../assets/<opaque-id>.<ext>` (DAS-FR-05). Returned so no caller derives
    /// a path of its own.
    pub reference: String,
    /// The IANA media type of the stored bytes.
    pub media_type: String,
    /// The name the image was supplied under, absent where none was
    /// (DAS-FR-02: it is metadata alone and never reaches the path).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// The byte length of the stored content.
    pub bytes: u64,
}

/// What `read_draft_image` serves back (DAS-FR-08).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftImageContent {
    pub media_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    /// The stored bytes, base64-encoded.
    pub data: String,
}

/// What `discard_draft_image` answers with (DAS-FR-09).
///
/// A discard says which of the two things it did, because the caller asked for a
/// removal and is owed the answer for it: a path naming nothing is reported
/// discarded having had nothing to discard, and an asset the saved prompt still
/// names is reported retained rather than deleted.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftAssetDiscarded {
    /// True where the asset is not on disk when this returns — whether this call
    /// removed it or nothing was there to remove.
    pub discarded: bool,
    /// True where a valid reference in the saved prompt still names it, so it
    /// was left in place.
    pub retained: bool,
}

/// One image use resolved out of the saved prompt (DAS-FR-22).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftPromptImage {
    /// The destination exactly as the Markdown carries it.
    pub reference: String,
    /// The image's alt text; empty where the image has none.
    pub alt: String,
    /// The text that identifies this image to a reader: its Markdown reference
    /// together with its alt text. Always present, resolved or not.
    pub context: String,
    /// True only where the destination resolved to a draft-owned asset whose
    /// bytes were read.
    pub resolved: bool,
    /// The draft-relative path of the asset it resolves to; `None` when
    /// `resolved` is false.
    pub asset_path: Option<String>,
    /// The asset's media type; `None` when `resolved` is false.
    pub media_type: Option<String>,
    /// The asset's supplied filename; absent when none was recorded and when
    /// `resolved` is false.
    pub filename: Option<String>,
    /// The asset's own bytes, base64-encoded; `None` when `resolved` is false.
    pub data: Option<String>,
    /// Where this image's Markdown sits in the saved prompt, in bytes.
    ///
    /// Not part of what a surface reads — no Tauri command returns this shape
    /// (DAS-FR-22) — and carried because the one consumer needs it: the
    /// conversational input assembly interleaves the prompt's text with its
    /// pictures **in the positions their references occupy** (per
    /// `AGC-agent-conversations.md` AGC-FR-35), which is a fact about the source
    /// that only the reading of it holds.
    pub start: usize,
    pub end: usize,
}

/// What one housekeeping pass did (DAS-FR-12).
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftAssetSweep {
    /// Files examined under this draft's `assets/`.
    pub scanned: u32,
    /// Files deleted.
    pub removed: u32,
    /// Files left in place: referenced, held, or uncertain.
    pub retained: u32,
    /// Files whose delete was refused (DAS-FR-28).
    pub failed: u32,
    /// False where the pass ended without deciding every file it had scanned
    /// (DAS-FR-20, DAS-FR-21).
    pub complete: bool,
}

/// What `sweep_draft_assets` answers with (DAS-FR-15).
///
/// An **acknowledgement and not a result**: it says that a pass will run and
/// whether this request joined one already pending, and it carries no tally,
/// because no tally exists yet. A caller that wants the outcome reads the log.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DraftAssetSweepScheduled {
    /// True where a pass will run for this draft.
    pub scheduled: bool,
    /// True where this request joined a pass already scheduled or already
    /// running rather than adding one.
    pub coalesced: bool,
}

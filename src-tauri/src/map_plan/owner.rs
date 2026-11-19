//! The owner of the discussion of a plan proposal (`MPR-map-plan-proposals.md`
//! MPR-FR-TQVZ, MPR-FR-RVRN, MPR-FR-KDTQ).
//!
//! A proposal can belong to a Map thread or to any other discussion of the
//! project. The record holds the owner of its discussion, so that a recording,
//! an update, a re-prepare request, and a follow-up find the log of the
//! discussion without a search of the store. A record with no owner belongs to
//! a Map thread, which is also how the records of older builds read.

use serde::{Deserialize, Serialize};

use super::errors::error;
use super::model::MapPlanError;
use crate::agent_conversations::ConversationOrigin;
use crate::comments::{DiscussionTarget, ThreadRef, ERR_DISCUSSION_NOT_FOUND};
use crate::fs::RootFs;

/// MPR-FR-RVRN: the owner target of the discussion of a proposal and, for a
/// discussion with a fragment target, the file of that target.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalOwner {
    pub target: DiscussionTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment_file: Option<String>,
}

/// MPR-FR-TQVZ: the discussion that a recording or an update names: its id and
/// its owner. `owner` is `None` for a Map thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProposalDiscussion {
    pub id: String,
    pub owner: Option<ProposalOwner>,
}

impl ProposalDiscussion {
    /// A Map thread by its id.
    pub fn map_thread(id: impl Into<String>) -> Self {
        Self { id: id.into(), owner: None }
    }

    /// MPR-FR-RVRN: the discussion of a turn.
    pub fn of_origin(origin: &ConversationOrigin) -> Self {
        let owner = (!origin.is_map()).then(|| ProposalOwner {
            target: origin.target.clone(),
            fragment_file: origin.fragment_target.as_ref().map(|f| f.path.clone()),
        });
        Self { id: origin.discussion_id().to_string(), owner }
    }
}

impl From<&str> for ProposalDiscussion {
    fn from(id: &str) -> Self {
        Self::map_thread(id)
    }
}

impl From<&String> for ProposalDiscussion {
    fn from(id: &String) -> Self {
        Self::map_thread(id.clone())
    }
}

impl From<&ConversationOrigin> for ProposalDiscussion {
    fn from(origin: &ConversationOrigin) -> Self {
        Self::of_origin(origin)
    }
}

/// MPR-FR-KDTQ: the log that holds the discussion of `owner`.
pub fn log_of(owner: Option<&ProposalOwner>) -> ThreadRef<'_> {
    match owner {
        None => ThreadRef::map_discussion(),
        Some(owner) => crate::comments::log_ref_for(&owner.target, owner.fragment_file.as_deref()),
    }
}

/// MPR-FR-KDTQ: the owner target of the discussion of `owner`.
pub fn target_of(owner: Option<&ProposalOwner>) -> DiscussionTarget {
    owner.map_or(DiscussionTarget::Map, |o| o.target.clone())
}

/// MPR-FR-KDTQ, MPR-FR-RVRN: the discussion `id` in the log of `owner`, or
/// `discussion_not_found` where that log does not hold it.
pub fn find_thread(
    store: &RootFs,
    owner: Option<&ProposalOwner>,
    id: &str,
) -> Result<crate::comments::Discussion, MapPlanError> {
    crate::comments::find_in_log(store, log_of(owner), id)
        .map_err(|_| error(ERR_DISCUSSION_NOT_FOUND, "The discussion does not exist."))
}

#[cfg(test)]
mod tests {
    use super::*;

    // MPR-FR-RVRN: a Map thread records no owner; a passage thread of a draft
    // records its target and the file of its fragment; a whole note discussion
    // records no file; no owner names the Map target; the owner is written in
    // camelCase.
    #[test]
    fn the_owner_of_a_discussion_comes_from_its_origin() {
        let map = ProposalDiscussion::of_origin(&ConversationOrigin::stub_map("m1"));
        assert_eq!(map, ProposalDiscussion::map_thread("m1"));
        let passage =
            ProposalDiscussion::of_origin(&ConversationOrigin::stub_draft("d1", "dr", true));
        let owner = passage.owner.clone().expect("a draft owner");
        assert_eq!(owner.target, DiscussionTarget::Draft { draft_id: "dr".into() });
        assert_eq!(owner.fragment_file.as_deref(), Some("a.md"));
        let whole = ProposalDiscussion::of_origin(&ConversationOrigin::stub_note("n1", "note"));
        assert_eq!(whole.owner.unwrap().fragment_file, None);
        assert_eq!(target_of(None), DiscussionTarget::Map);
        let text = serde_json::to_string(&owner).unwrap();
        assert_eq!(text, r#"{"target":{"kind":"draft","draftId":"dr"},"fragmentFile":"a.md"}"#);
    }
}

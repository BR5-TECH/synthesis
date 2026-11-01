//! The `updates` argument both proposal tools share
//! (`PDC-propose-draft-changes-tool.md` PDC-FR-GMWR, PDC-FR-UOBH, PDC-FR-SNUK;
//! `PPC-propose-prompt-changes-tool.md` PPC-FR-RVKS).
//!
//! An update of an open proposal is a list of entries, each a `change`, an
//! `add`, or a `withdraw`. This module reads the entries the model composed,
//! refuses a malformed one with its place, and holds the refusal texts the two
//! tools say alike.

use serde::Deserialize;

use super::ToolRefusal;
use crate::text_hunks::hunks::ProposedHunk;
use crate::text_hunks::updates::UpdateEntry;

/// PDC-FR-GPLQ: an empty `updates` list.
pub const UPDATE_NONE: &str =
    "Pass at least one update. An empty list has nothing for the author to decide.";
/// PDC-FR-UOBH: an entry whose `action` or `id` does not fit.
pub const UPDATE_MALFORMED: &str =
    "Each update gives `action`: `change` or `withdraw` with the `id` of a change, or `add` with no `id`.";
/// PDC-FR-UOBH: an `id` that names no hunk of the open proposal.
pub const UPDATE_NO_SUCH_CHANGE: &str =
    "The `id` you gave is not a change of your open proposal. Name a change of it, or use `add` for a new change.";
/// PDC-FR-DBFT: a `change` or `withdraw` of an accepted or a withdrawn hunk.
pub const UPDATE_CHANGE_SETTLED: &str =
    "The change you named is accepted or withdrawn, so it can not be updated. Use `add` for a new change.";
/// PDC-FR-DBFT: an `id` that a second entry names again.
pub const UPDATE_NAMED_TWICE: &str =
    "Two of your updates name the same change. Put what you want for it in one update.";

/// One entry of `updates`, as the model composed it.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateArg {
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub after_text: Option<String>,
    #[serde(default)]
    pub lead: Option<String>,
    #[serde(default)]
    pub trail: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

impl UpdateArg {
    /// The hunk fields, the kind read from the fields (PDC-FR-VKMR).
    fn hunk(&self) -> ProposedHunk {
        ProposedHunk::from_fields(
            self.before.clone(),
            self.after.clone(),
            self.after_text.clone(),
            self.lead.clone(),
            self.trail.clone(),
            self.note.clone(),
        )
    }

    /// The `id`, trimmed, or `None` where it is absent or blank.
    fn named(&self) -> Option<String> {
        self.id.as_deref().map(str::trim).filter(|id| !id.is_empty()).map(str::to_string)
    }
}

/// A list of updates, an explicit `null` reading as none of them (TLC-FR-07).
pub fn updates_or_none<'de, D>(deserializer: D) -> Result<Option<Vec<UpdateArg>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<Vec<UpdateArg>>::deserialize(deserializer)
}

/// PDC-FR-UOBH / PDC-FR-SNUK: the entries, or the refusal of the first
/// malformed one with its place.
pub fn to_entries(args: &[UpdateArg]) -> Result<Vec<UpdateEntry>, ToolRefusal> {
    let of = args.len();
    let malformed = |at: usize| ToolRefusal::InvalidUpdateAt { message: UPDATE_MALFORMED, at, of };
    args.iter()
        .enumerate()
        .map(|(index, arg)| {
            let action = arg.action.as_deref().map(str::trim).map(str::to_ascii_lowercase);
            match (action.as_deref(), arg.named()) {
                (Some("change"), Some(id)) => Ok(UpdateEntry::Change { id, hunk: arg.hunk() }),
                (Some("withdraw"), Some(id)) => Ok(UpdateEntry::Withdraw { id }),
                (Some("add"), None) => Ok(UpdateEntry::Add(arg.hunk())),
                _ => Err(malformed(index + 1)),
            }
        })
        .collect()
}

/// The count of each action, for the log record (PDC-FR-18, PPC-FR-19).
pub fn action_counts(args: &[UpdateArg]) -> (usize, usize, usize) {
    let count = |name: &str| {
        args.iter()
            .filter(|a| {
                a.action.as_deref().map(str::trim).is_some_and(|x| x.eq_ignore_ascii_case(name))
            })
            .count()
    };
    (count("change"), count("add"), count("withdraw"))
}

/// TLC-FR-06: the schema of `updates`. The hunk fields of an entry carry
/// short descriptions that point to the rules of `hunks`, so the definition a
/// turn sends stays small; `with_note` adds the one-change note a draft's hunk
/// carries.
pub fn schema(description: &str, with_note: bool) -> serde_json::Value {
    let mut properties = serde_json::json!({
        "action": {
            "type": "string",
            "enum": ["change", "add", "withdraw"],
            "description": ACTION_DESCRIPTION,
        },
        "id": { "type": "string", "description": ID_DESCRIPTION },
        "before": { "type": "string", "description": "As in `hunks`: the exact existing text." },
        "after": { "type": "string", "description": "As in `hunks`: the exact new text." },
        "after_text": {
            "type": "string",
            "description": "As in `hunks`: for an insertion, the exact text it follows.",
        },
        "lead": { "type": "string", "description": "As in `hunks`: the exact text just before." },
        "trail": { "type": "string", "description": "As in `hunks`: the exact text just after." },
    });
    if let (true, Some(into)) = (with_note, properties.as_object_mut()) {
        into.insert(
            "note".into(),
            serde_json::json!({ "type": "string", "description": "As in `hunks`." }),
        );
    }
    serde_json::json!({
        "type": "array",
        "description": description,
        "minItems": 1,
        "items": {
            "type": "object",
            "description": ENTRY_DESCRIPTION,
            "properties": properties,
            "required": ["action"],
        },
    })
}

const ENTRY_DESCRIPTION: &str = "One update of your open proposal: `change` an existing change, `add` a new one, or `withdraw` one you no longer offer.";

const ACTION_DESCRIPTION: &str = "`change` replaces the change named by `id` with the fields you give; `add` offers a new change and takes no `id`; `withdraw` takes back the change named by `id`.";

const ID_DESCRIPTION: &str = "For `change` and `withdraw`: the id of a change of your open proposal. Leave it out for `add`.";

#[cfg(test)]
mod tests {
    use super::*;

    fn arg(action: Option<&str>, id: Option<&str>) -> UpdateArg {
        UpdateArg {
            action: action.map(str::to_string),
            id: id.map(str::to_string),
            before: Some("x".into()),
            after: Some("y".into()),
            ..UpdateArg::default()
        }
    }

    // PDC-FR-UOBH / PDC-FR-SNUK: a malformed entry is refused with its place.
    #[test]
    fn a_malformed_entry_is_refused_with_its_place() {
        let refused = |args: &[UpdateArg]| to_entries(args).unwrap_err().to_string();
        let ok = arg(Some("withdraw"), Some("h1"));
        assert!(refused(&[ok.clone(), arg(None, Some("h1"))]).starts_with("Update 2 of 2: "));
        assert!(refused(&[arg(Some("change"), None)]).contains(UPDATE_MALFORMED));
        assert!(refused(&[arg(Some("add"), Some("h1"))]).contains(UPDATE_MALFORMED));
        assert!(refused(&[arg(Some("rewrite"), Some("h1"))]).contains(UPDATE_MALFORMED));
        assert!(refused(&[arg(Some("withdraw"), Some("  "))]).contains(UPDATE_MALFORMED));
    }

    // PDC-FR-GMWR: each action reads to its entry, the id trimmed.
    #[test]
    fn each_action_reads_to_its_entry() {
        let entries = to_entries(&[
            arg(Some("change"), Some(" h1 ")),
            arg(Some("ADD"), None),
            arg(Some("withdraw"), Some("h2")),
        ])
        .unwrap();
        assert!(matches!(&entries[0], UpdateEntry::Change { id, .. } if id == "h1"));
        assert!(matches!(&entries[1], UpdateEntry::Add(_)));
        assert!(matches!(&entries[2], UpdateEntry::Withdraw { id } if id == "h2"));
    }
}

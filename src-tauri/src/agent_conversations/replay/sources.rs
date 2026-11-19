//! Source stamps (`ERP-exchange-replay.md` ERP-FR-NKLF, ERP-FR-RVYX).
//!
//! A result of a tool that reads one file records the SHA-256 of that file. A
//! later turn reads the file again before it replays the result, and replaces a
//! result whose file changed or is gone, so the agent never reads an old text
//! as the current one.

use super::super::*;

/// ERP-FR-RVYX: the line that replaces a result whose file changed.
pub(in crate::agent_conversations) fn outdated_line(path: &str) -> String {
    format!("[Outdated: {path} changed after this result was read. Read it again if you need it.]")
}

/// ERP-FR-NKLF: the stamp of one result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::agent_conversations) struct SourceStamp {
    /// The id of the call whose result the stamp is for.
    pub call_id: String,
    /// What the stamp reads again: a file of the worktree, or a draft prompt.
    pub source: Source,
    /// The SHA-256 of the source when the result was made.
    pub sha256: String,
}

/// ERP-FR-NKLF: the file a stamp reads.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::agent_conversations) enum Source {
    /// A path relative to the worktree root.
    Worktree(String),
    /// The prompt of a draft, by the draft id.
    DraftPrompt(String),
}

impl Source {
    /// The name of the source in the outdated line.
    pub(in crate::agent_conversations) fn label(&self) -> String {
        match self {
            Source::Worktree(path) => path.clone(),
            Source::DraftPrompt(id) => format!("the prompt of draft {id}"),
        }
    }

    /// The SHA-256 of the source now, or `None` where it does not resolve.
    pub(in crate::agent_conversations) fn digest(&self, roots: Roots<'_>) -> Option<String> {
        match self {
            Source::Worktree(path) => roots.worktree.sha256_file(path).ok(),
            Source::DraftPrompt(id) => {
                let prompt = crate::drafts::read_draft_prompt(roots.worktree, id).ok()?;
                use sha2::{Digest, Sha256};
                Some(format!("{:x}", Sha256::digest(prompt.content.as_bytes())))
            }
        }
    }
}

/// A string argument of a call, trimmed, where it is not empty.
fn argument(call: &rig::completion::message::ToolCall, name: &str) -> Option<String> {
    let value = call.function.arguments.get(name)?.as_str()?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

/// ERP-FR-NKLF: the source a call reads, where the tool reads one file and its
/// call names a file. `load_skill` and `read_graduation_file` name a skill and
/// a file of a graduation run, which this turn does not resolve, so they get no
/// stamp. Pure.
pub(in crate::agent_conversations) fn source_of(
    call: &rig::completion::message::ToolCall,
) -> Option<Source> {
    match call.function.name.as_str() {
        "read_file" => argument(call, "path").map(Source::Worktree),
        "measure_document" => match argument(call, "path") {
            Some(path) => Some(Source::Worktree(path)),
            None => argument(call, "draft_id").map(Source::DraftPrompt),
        },
        "read_draft" => argument(call, "draft_id")
            .or_else(|| argument(call, "draftId"))
            .map(Source::DraftPrompt),
        "read_openspec_spec" => argument(call, "capability")
            .filter(|c| !c.contains('/') && !c.contains(".."))
            .map(|c| Source::Worktree(format!("openspec/specs/{c}/spec.md"))),
        _ => None,
    }
}

/// ERP-FR-NKLF: the stamp of the result of `call`, where its source resolves
/// now.
pub(in crate::agent_conversations) fn stamp_of(
    roots: Roots<'_>,
    call: &rig::completion::message::ToolCall,
) -> Option<SourceStamp> {
    let source = source_of(call)?;
    let sha256 = source.digest(roots)?;
    Some(SourceStamp { call_id: call.id.wire().into_owned(), source, sha256 })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(name: &str, arguments: serde_json::Value) -> rig::completion::message::ToolCall {
        rig::completion::message::ToolCall::from_wire(
            "call-1",
            rig::completion::message::ToolFunction::new(
                rig::completion::message::ToolName::new(name).expect("a tool name"),
                arguments,
            ),
        )
    }

    // ERP-FR-NKLF: the file-reading tools name their source, other tools and a
    // call with no file name none.
    #[test]
    fn the_reading_tools_name_their_source() {
        let path = serde_json::json!({ "path": " docs/a.md " });
        assert_eq!(
            source_of(&call("read_file", path.clone())),
            Some(Source::Worktree("docs/a.md".into()))
        );
        assert_eq!(
            source_of(&call("measure_document", path)),
            Some(Source::Worktree("docs/a.md".into()))
        );
        let draft = serde_json::json!({ "draft_id": "d1" });
        assert_eq!(
            source_of(&call("read_draft", draft.clone())),
            Some(Source::DraftPrompt("d1".into()))
        );
        assert_eq!(
            source_of(&call("measure_document", draft)),
            Some(Source::DraftPrompt("d1".into()))
        );
        assert_eq!(
            source_of(&call("read_openspec_spec", serde_json::json!({ "capability": "auth" }))),
            Some(Source::Worktree("openspec/specs/auth/spec.md".into()))
        );
        assert_eq!(
            source_of(&call("read_openspec_spec", serde_json::json!({ "capability": "../x" }))),
            None
        );
        assert_eq!(
            source_of(&call("search_specifications", serde_json::json!({ "query": "a" }))),
            None
        );
        assert_eq!(source_of(&call("load_skill", serde_json::json!({ "name": "s" }))), None);
        assert_eq!(source_of(&call("read_file", serde_json::json!({ "path": "  " }))), None);
    }

    // ERP-FR-NKLF, ERP-FR-RVYX: a stamp holds the hash of the file, which
    // changes when the file changes, and a file that is gone gives no hash.
    #[test]
    fn a_stamp_follows_the_file() {
        let dir = tempfile::TempDir::new().expect("tempdir");
        let root = crate::fs::RootFs::for_root(dir.path());
        std::fs::write(dir.path().join("a.md"), "one").expect("write");
        let roots = Roots::same(&root);
        let read = call("read_file", serde_json::json!({ "path": "a.md" }));
        let stamp = stamp_of(roots, &read).expect("a stamp");
        assert_eq!(stamp.call_id, "call-1");
        assert_eq!(stamp.source.digest(roots), Some(stamp.sha256.clone()));
        std::fs::write(dir.path().join("a.md"), "two").expect("write");
        assert_ne!(stamp.source.digest(roots), Some(stamp.sha256.clone()));
        std::fs::remove_file(dir.path().join("a.md")).expect("remove");
        assert_eq!(stamp.source.digest(roots), None);
        assert!(stamp_of(roots, &read).is_none(), "no file, no stamp");
        assert_eq!(
            outdated_line("a.md"),
            "[Outdated: a.md changed after this result was read. Read it again if you need it.]"
        );
    }
}

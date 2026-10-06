//! The application-owned storage floor (`PST-project-storage.md` PST-FR-XKVD).
//!
//! The one folder this application writes into the project's repository on the
//! author's behalf. Part of it is committed at draft events (PST-FR-DQZT) and
//! part of it is private to the checkout (`DRS-draft-storage.md` DRS-FR-WYIN),
//! but none of it is the author's uncommitted work, and every gate that asks
//! "has the author left something uncommitted here" must answer "no" for it.
//!
//! One predicate rather than a string comparison at each gate, because the same
//! question is asked in several places and a rename that missed one of them
//! would reintroduce the fault without failing anything. An agent turn writes a
//! proposal and a history entry into a draft's folder while it answers, so a
//! gate that counted either would offer the author a refusal they cannot clear:
//! nothing they commit stops the next write.
//!
//! The conversation logs and the statistics logs are **not** here. They stand in
//! the repository machine store, outside every worktree and outside Git
//! altogether (`RMS-repository-machine-storage.md` RMS-FR-HDNZ), so no gate has
//! to read past them and no commit has to name them.
//!
//! A draft-event commit names a narrower set than the gates read past: the
//! committed draft storage of one draft (`DRS-draft-storage.md` DRS-FR-VECL).

pub mod commit;

/// `DRS-draft-storage.md` DRS-FR-04: the drafts and everything they hold.
pub const DRAFTS_REL: &str = ".synthesis/drafts";

/// Every folder of the floor (PST-FR-XKVD).
pub const OWNED_FOLDERS: [&str; 1] = [DRAFTS_REL];

/// PST-FR-XKVD: whether a project-relative path is material this application
/// owns.
///
/// Matched on **whole path segments** rather than as a raw prefix, so
/// `.synthesis/drafts-old/x` is the author's folder and not this one. The
/// folder itself is a member as well as its contents, because a status read can
/// report an untracked directory at directory granularity, and because the
/// folder is what a commit of the set names.
///
/// This is what the **clean-source gates** read past, and it reads past the
/// whole of `.synthesis/drafts/` — a file the author put there themselves
/// included. A gate exists to protect the author's uncommitted work from a
/// publication landing over it, and the drafts root is a folder the application
/// writes into at a cadence no author controls, so a refusal naming anything
/// under it is a refusal they cannot clear. The dirty check before a stream
/// checkout reads past less: a tracked path here that differs from `HEAD`
/// counts on the side it checks out, because no draft event commits it first
/// (`WKS-work-streams.md` WKS-FR-JVLM). What is **committed** under that prefix
/// is narrower ([`is_draft_event_path`]).
pub fn is_application_storage_rel(rel: &str) -> bool {
    let normalized = normalize_rel(rel);
    OWNED_FOLDERS.iter().any(|owned| {
        normalized == *owned
            || normalized
                .strip_prefix(owned)
                .is_some_and(|rest| rest.starts_with('/'))
    })
}

/// PST-FR-TYNC / `DRS-draft-storage.md` DRS-FR-VECL: whether a draft-event
/// commit for `draft_id` may name this project-relative path.
///
/// Narrower than what the gates read past: the drafts root's two Git files and
/// the committed draft storage of that one draft, or for a deletion every path
/// under that draft's folder. A transient file and a file the author created
/// under the drafts root are never named (DRS-FR-XPQI).
pub fn is_draft_event_path(rel: &str, draft_id: &str, deletion: bool) -> bool {
    let normalized = normalize_rel(rel);
    match normalized.strip_prefix(DRAFTS_REL) {
        Some(rest) if rest.starts_with('/') => {
            crate::drafts::is_draft_event_rel(&rest[1..], draft_id, deletion)
        }
        _ => false,
    }
}

/// `WKS-work-streams.md` WKS-FR-UZHT / `GRB-graduation-rebase.md` GRB-FR-QIHE:
/// whether a **repository-relative** path is application-owned storage.
///
/// A working copy's status reports paths against the repository root, and a
/// co-located project's `.synthesis/` may stand below it. The project prefix of
/// a stream's working copy is not known to the dirty check, so a path is read
/// as application storage where any of its segment-aligned tails is.
pub fn is_application_storage_in_repo(repo_rel: &str) -> bool {
    let normalized = normalize_rel(repo_rel);
    let mut tail = normalized.as_str();
    loop {
        if is_application_storage_rel(tail) {
            return true;
        }
        match tail.split_once('/') {
            Some((_, rest)) => tail = rest,
            None => return false,
        }
    }
}

/// The drafts root's `.gitattributes` and `.gitignore` (DRS-FR-BKFG).
pub(crate) fn is_root_git_file(rel: &str) -> bool {
    matches!(rel, ".gitattributes" | ".gitignore")
}

/// A repository-relative path as the project prefix it stands under and its
/// path beneath that project's drafts root, where it has one.
pub(crate) fn split_at_drafts(path: &str) -> Option<(String, String)> {
    let marker = format!("{DRAFTS_REL}/");
    let mut start = 0;
    loop {
        let tail = &path[start..];
        if let Some(rel) = tail.strip_prefix(&marker) {
            let prefix = path[..start].trim_end_matches('/').to_string();
            return Some((prefix, rel.to_string()));
        }
        let next = tail.find('/')?;
        start += next + 1;
    }
}

/// One spelling for a path a status read may report in several.
fn normalize_rel(rel: &str) -> String {
    let normalized = rel.replace('\\', "/");
    normalized.trim_start_matches("./").trim_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::is_application_storage_rel as owned;

    /// WKS-FR-JVLM: a repository path splits at its own project's drafts root,
    /// below a prefix for a co-located project, and the drafts root's two Git
    /// files are told apart from a draft's own.
    #[test]
    fn a_path_splits_at_its_projects_drafts_root() {
        use super::{is_root_git_file, split_at_drafts};
        assert_eq!(
            split_at_drafts(".synthesis/drafts/.gitignore"),
            Some((String::new(), ".gitignore".to_string()))
        );
        assert_eq!(
            split_at_drafts("apps/web/.synthesis/drafts/UI/x/files/P.md"),
            Some(("apps/web".to_string(), "UI/x/files/P.md".to_string()))
        );
        for none in [".synthesis/drafts", ".synthesis/drafts-old/x", "src/main.rs"] {
            assert_eq!(split_at_drafts(none), None, "{none}");
        }
        assert!(is_root_git_file(".gitignore") && is_root_git_file(".gitattributes"));
        assert!(!is_root_git_file("UI/.gitignore"));
    }

    /// PST-FR-XKVD: the set holds the drafts folder and nothing else. The
    /// conversation and statistics folders an earlier build committed are the
    /// author's to remove (`RMS-repository-machine-storage.md` RMS-FR-XRPT) and
    /// are no gate's to read past.
    #[test]
    fn pst_ts_jqwr_the_floor_holds_the_drafts_folder_and_nothing_else() {
        for member in [
            // DRS-FR-04: the drafts, whole prefix — the gates read past a file
            // the author put there themselves too, that being the point of the
            // exception (PST-FR-XKVD).
            ".synthesis/drafts",
            ".synthesis/drafts/.gitattributes",
            ".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/draft.toml",
            ".synthesis/drafts/UI/1a2b3c4d5e6-0001-deadbeef/files/prompt.md",
            ".synthesis/drafts/UI/notes-of-my-own.md",
        ] {
            assert!(owned(member), "{member} is application-owned storage");
        }
        for outsider in [
            "",
            ".synthesis",
            ".synthesis/notes/note.md",
            ".synthesis/project.toml",
            // The legacy folders of RMS-FR-JVEC: read from, never committed and
            // never read past.
            ".synthesis/statistics",
            ".synthesis/statistics/1a0.jsonl",
            ".synthesis/comments",
            ".synthesis/comments/abc.jsonl",
            "src/main.rs",
            "specifications/core/PST-project-storage.md",
        ] {
            assert!(!owned(outsider), "{outsider} is the author's");
        }
    }

    /// A prefix test would take a neighbouring folder with a longer name.
    #[test]
    fn the_floor_matches_whole_segments_rather_than_a_raw_prefix() {
        for neighbour in [
            ".synthesis/drafts-old/1a0/draft.toml",
            ".synthesis/drafts.bak",
            ".synthesis/draftspublic/x.jsonl",
            "vendor/.synthesis/drafts/1a0/draft.toml",
        ] {
            assert!(!owned(neighbour), "{neighbour} is not the floor's");
        }
    }

    /// A status reader may hand back a separator or a prefix the caller did not
    /// normalize, and the answer must not change with the spelling.
    #[test]
    fn the_floor_reads_past_a_paths_spelling() {
        for spelling in [
            "./.synthesis/drafts/1a0/draft.toml",
            ".synthesis/drafts/",
            ".synthesis\\drafts\\1a0\\draft.toml",
        ] {
            assert!(owned(spelling), "{spelling} is application-owned storage");
        }
    }

    // PST-FR-TYNC / DRS-FR-VECL / DRS-FR-ZIVL / DRS-FR-XPQI: a draft-event
    // commit names the committed draft storage of its one draft and the drafts
    // root's two Git files, and nothing else.
    #[test]
    fn an_event_names_the_committed_storage_of_its_draft_alone() {
        use super::is_draft_event_path as named;
        let id = "1a2b3c4d5e6-0001-deadbeef";
        for path in [
            ".synthesis/drafts/.gitattributes",
            ".synthesis/drafts/.gitignore",
            ".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/draft.toml",
            "./.synthesis/drafts/UI/1a2b3c4d5e6-0001-deadbeef/files/prompt.md",
            ".synthesis/drafts/UI/1a2b3c4d5e6-0001-deadbeef/assets/logo.png",
        ] {
            assert!(named(path, id, false), "{path} is named");
        }
        for path in [
            // DRS-FR-WYIN: private draft storage is named by no event commit.
            ".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/proposals/p.toml",
            ".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/history/h.snapshot",
            ".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/publication.toml",
            ".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/conversation.jsonl",
            ".synthesis/drafts/1a2b3c4d5e6-0001-deadbeef/assets/.a.png.tmp.42.99",
            // Another draft, the author's own files, and the rest of the project.
            ".synthesis/drafts/1a2b3c4d5e6-0002-cafebabe/draft.toml",
            ".synthesis/drafts/UI/notes-of-my-own.md",
            ".synthesis/drafts",
            ".synthesis/notes/note.md",
            ".synthesis/statistics/1a0.jsonl",
            "src/main.rs",
        ] {
            assert!(!named(path, id, false), "{path} is not named");
        }
    }

    // DRS-FR-21 / DRS-FR-VECL / DRS-FR-PIWL: the deletion commit names every
    // path under the deleted draft's folder, a private one an earlier build
    // committed included, and no transient file and no other draft's path.
    #[test]
    fn a_deletion_names_every_path_under_its_draft() {
        use super::is_draft_event_path as named;
        let id = "1a2b3c4d5e6-0001-deadbeef";
        for path in [
            ".synthesis/drafts/UI/1a2b3c4d5e6-0001-deadbeef/draft.toml",
            ".synthesis/drafts/UI/1a2b3c4d5e6-0001-deadbeef/proposals/p.toml",
            ".synthesis/drafts/UI/1a2b3c4d5e6-0001-deadbeef/comments/discussion.jsonl",
        ] {
            assert!(named(path, id, true), "{path} is named");
        }
        for path in [
            ".synthesis/drafts/UI/1a2b3c4d5e6-0001-deadbeef/assets/.a.png.tmp.42.99",
            ".synthesis/drafts/UI/1a2b3c4d5e6-0002-cafebabe/draft.toml",
            ".synthesis/drafts/UI/notes-of-my-own.md",
        ] {
            assert!(!named(path, id, true), "{path} is not named");
        }
    }

    // WKS-FR-UZHT / GRB-FR-QIHE: a repository-relative path reads as
    // application storage at the repository root and below a co-located
    // project's prefix alike.
    #[test]
    fn a_repository_path_is_read_past_below_a_project_prefix() {
        use super::is_application_storage_in_repo as read_past;
        for path in [
            ".synthesis/drafts/UI/1a0/files/p.md",
            "apps/web/.synthesis/drafts/1a0/draft.toml",
        ] {
            assert!(read_past(path), "{path} is read past");
        }
        for path in [
            "src/main.rs",
            ".synthesis/notes/n.toml",
            "apps/web/.synthesis/project.toml",
            ".synthesis/drafts-old/x",
        ] {
            assert!(!read_past(path), "{path} is the author's");
        }
    }
}

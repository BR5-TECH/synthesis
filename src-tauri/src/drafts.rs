//! Draft storage — the shared working sets behind the New Artifact tab
//! (`../../specifications/ui/NAW-new-artifact.md` /
//! `../../specifications/ui/DRP-drafts-panel.md`).
//!
//! A **draft** is a single Markdown **prompt** being developed before the
//! specification it produces exists in the project: a name, a status, and
//! exactly one file, all held under `.synthesis/drafts/<draft-id>/` — which is
//! **committed** to the project's repository (DRS-FR-04, FSA-FR-08), scaffolded
//! by `create_project` (PST-FR-02), skipped by the scan (ASC-FR-09) and
//! therefore invisible to the Library and to search (SCC-FR-03). A draft and the
//! whole collaboration around it reach every checkout of the project through
//! Git. Nothing a draft holds reaches the project tree itself until it is
//! **graduated**, which writes the published prompt to a destination the author
//! chose (DRS-FR-20).
//!
//! What Git is told about that storage — the union-merge attributes of the
//! append-only logs, the ignore rules for private draft storage, which paths
//! the application owns, and the draft events that commit them — is
//! [`git_storage`] (DRS-FR-DGMI, DRS-FR-JDRY, DRS-FR-ZIVL, DRS-FR-QHHY).
//!
//! On-disk layout, per draft:
//!
//! ```text
//! .synthesis/drafts/<id>/
//!   draft.toml     — the record: name, status, destination root, timestamps
//!   files/         — the draft's ONE prompt (DRS-FR-11), at the root, no folders
//!   comments/      — the review logs (CMS-FR-37)
//!   proposals/     — the changes agents have proposed (DCP-FR-01)
//!   history/       — the prompt's settled versions and the acceptance journal
//!                    (DHS-FR-01)
//!   conversation.jsonl
//! ```
//!
//! A draft is never created empty and never created holding more than one
//! thing: `create_draft` scaffolds it holding one Markdown file named for the
//! draft (DRS-FR-06) and an empty `history/`, a draft settling no version until
//! a change is accepted against it (DHS-FR-07). That file is the draft's
//! **prompt**, and
//! its name and the draft's name are one thing, bound in one direction
//! (DRS-FR-25): renaming the draft renames the prompt, and there is no operation
//! anywhere that adds a file to a draft, cuts a folder inside one, moves the
//! prompt within it, or removes it (DRS-FR-13). A draft whose `files/` holds
//! anything else is **inconsistent**: it is reported rather than repaired, every
//! product operation against it returns [`ERR_NOT_SINGLE_FILE`], and the one
//! thing left to do with it is to delete it (DRS-FR-15).
//!
//! Every path this module resolves goes through `fs::resolve_under`, so the
//! FSA-FR-10 escape gate applies to a draft id, to a draft-relative file path,
//! and to a graduation destination alike. No raw `std::fs` mutation happens
//! here — writes go through the `fs` atomic primitives, exactly as PST-FR-10
//! requires of the project's own writes.
//!
//! Deliberately absent: anything AI. Driving a conversational model or an agent
//! against a draft is a separate concern with no core spec yet, and this module
//! neither calls one nor knows one exists.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager, State};

use crate::fs;
use crate::notes::now_rfc3339;
use crate::project::ProjectState;
use crate::project_settings;
use crate::storage_floor::commit::DraftEvent;

pub mod git_storage;

pub use git_storage::is_draft_event_rel;

/// Project-relative home of every draft (PST-FR-02 scaffolds it; DRS-FR-04
/// commits it, and FSA-FR-08 keeps every ignore rule off it).
const DRAFTS_REL: &str = crate::storage_floor::DRAFTS_REL;
/// The subfolder holding a draft's files, kept apart from its record so a draft
/// can hold a file called `draft.toml` of its own.
pub(crate) const FILES_DIR: &str = "files";
const RECORD_FILE: &str = "draft.toml";
/// What an append-only event log is named (CMS-FR-01). DRS-FR-37 needs to tell
/// one from a stored attachment when it merges misplaced storage back into its
/// draft: a log concatenates, where content-addressed bytes do not.
const LOG_EXT: &str = ".jsonl";
/// DRS-FR-01: the changes agents have proposed to this draft's files, owned by
/// `crate::draft_proposals` (DCP-FR-01). Created with the draft and deleted with
/// it, like every other folder here; nothing in this module reads or writes it.
pub(crate) const PROPOSALS_DIR: &str = "proposals";
/// DRS-FR-01: the prompt's settled versions and the acceptance journal, owned by
/// `crate::draft_history` (DHS-FR-01). Created with the draft and deleted with
/// it, like every other folder here; nothing in this module reads or writes it.
pub(crate) const HISTORY_DIR: &str = "history";
/// DRS-FR-01 / DAS-FR-01: the images the prompt embeds, owned by
/// `crate::draft_assets` (DAS-FR-01). Created with the draft and deleted with
/// it, like every other folder here; nothing in this module reads or writes it.
///
/// It sits **beside** `files/` rather than inside it, which is what keeps the
/// single-prompt invariant of DRS-FR-11 a statement about `files/` alone: an
/// image the prompt embeds is the draft's material rather than a second prompt
/// file, so a draft carrying ten of them is consistent exactly as an empty one
/// is (DRS-FR-15).
pub(crate) const ASSETS_DIR: &str = "assets";
/// DRS-FR-23: the record of a draft's collaboration with an agent. Created with
/// the draft and deleted with it; no operation here appends to it or reads it.
const CONVERSATION_FILE: &str = "conversation.jsonl";
/// DRS-FR-EJBM: the draft-owned publication store — the append-only GitHub
/// publication history and the draft's one standing attempt.
pub(crate) const PUBLICATION_FILE: &str = "publication.toml";
/// DRS-FR-27: what an unnamed draft is called, before the disambiguating
/// `Untitled 2`, `Untitled 3`, … that follow it in a worktree already holding
/// one.
const DEFAULT_NAME: &str = "Untitled";
/// DRS-FR-25: the extension the prompt's name always carries — a draft's prompt
/// is Markdown for its whole life.
const PROMPT_EXT: &str = "md";
/// DRS-FR-15: what every product operation returns for a draft whose `files/` is
/// not the single prompt DRS-FR-11 requires.
///
/// A stable token rather than prose, because four surfaces have to recognise it
/// and render a state of their own for it (NAW-FR-41, DRP-FR-33) — and because
/// `crate::draft_proposals` and `crate::draft_history` pass it through unchanged
/// (DCP-FR-22, DHS-FR-25).
pub const ERR_NOT_SINGLE_FILE: &str = "draft_not_single_file";
/// DRS-FR-38: the draft resolved and holds its one prompt, and that prompt could
/// not be read — it does not decode as UTF-8, or the read itself failed.
///
/// Its own value rather than a "not found", because the draft demonstrably
/// exists: a caller that reported it as missing would send its user looking for
/// an id that was never wrong. Distinct from [`ERR_NOT_SINGLE_FILE`] too, whose
/// sentence is about a draft's *file set* and would be a false description of a
/// draft holding exactly the one file it should.
pub const ERR_PROMPT_UNREADABLE: &str = "draft_prompt_unreadable";
/// A file larger than this is not read by `search_drafts` (DRS-FR-17). A draft
/// holds prose the author is writing; anything past this is not something a
/// text filter over a panel should be reading on every query.
const SEARCH_MAX_BYTES: u64 = 2 * 1024 * 1024;

mod commands;
mod folders;
mod hierarchy;
mod naming;
mod records;
mod shadow;
mod store;

pub use commands::*;
pub use folders::*;
pub use hierarchy::*;
pub use naming::*;
pub use records::*;
pub use shadow::*;
pub use store::*;

/// DRS-FR-06 / DRS-FR-07: scaffold a draft, filed in the drafts folder `folder`
/// names.
///
/// `folder` is the **only** thing a creation says about where anything goes, and
/// it is expressed by where the draft's directory is created rather than
/// recorded anywhere: the path *is* the answer. A draft carries no project
/// destination of any kind — where its specification lands is chosen by the
/// graduation agent from the captured prompt (per `GRD-graduation.md`
/// ).
///
/// [`create_draft_impl`] filing the draft at the drafts root — what every caller
/// that has no folder to file it in wants, and what `folder = None` means.
pub fn create_draft_at_root(
    root: &fs::RootFs,
    name: Option<&str>,
) -> Result<DraftCreated, String> {
    create_draft_impl(root, name, None)
}

pub fn create_draft_impl(
    root: &fs::RootFs,
    name: Option<&str>,
    folder: Option<&str>,
) -> Result<DraftCreated, String> {
    let now = now_rfc3339();
    // Resolved before anything is written: a `folder` naming no existing drafts
    // folder is a typed "not found" that creates nothing (DRS-FR-07).
    let folder = folder.unwrap_or("");
    let parent = resolve_drafts_folder(root, folder)?;
    // DRS-FR-39: the project's draft template, read from the project-public
    // store of the active worktree (`PSS-project-settings-storage.md`
    // PSS-FR-21) **before anything is written**. A store that cannot be read —
    // a malformed `project.toml` (PSS-FR-10) — fails the creation with that
    // typed error and creates no draft, rather than falling back to the
    // empty-prompt terms and silently withholding the starting content the
    // project asked for.
    //
    // After the folder is resolved rather than before it, so a caller who is
    // wrong about both is told about the folder they named — the argument they
    // supplied — rather than about a store they said nothing about (DRS-FR-07).
    // Neither has written anything by this point, so the ordering costs the
    // guarantee nothing.
    //
    // Reading it here, once, is also what makes the copy a **snapshot**: the
    // draft holds text of its own from this moment, and a template later
    // edited, replaced, or cleared changes nothing in a draft that already
    // exists. Nothing links the draft back to the template it came from.
    let config = project_settings::load_project_config_from(root)?;
    // DRS-FR-27: an unnamed draft is `Untitled`, disambiguated against the
    // drafts the worktree already holds.
    let name = match name.map(str::trim).filter(|n| !n.is_empty()) {
        Some(supplied) => supplied.to_string(),
        None => default_draft_name(root),
    };
    // DRS-FR-25: the one file the draft is created holding is named from the
    // draft's name, and is the file that name stays bound to.
    let primary = prompt_file_name(&name);
    let record = DraftRecord {
        id: new_draft_id(),
        name,
        prompt_path: Some(primary.clone()),
        // DRS-FR-06: a draft is created active.
        status: DraftStatus::Active,
        created_at: now.clone(),
        updated_at: now,
        github_issue: None,
    };
    // DRS-FR-39: the bytes the prompt is created with — the project's template
    // where one is configured, and nothing at all where the store reports it
    // unset (DRS-FR-06), which is the whole of the difference between the two
    // kinds of project. Normalised to the project's line-ending convention
    // exactly as `save_draft_file_impl` normalises a write (DRS-FR-12), so a
    // template authored under one convention lands under the project's.
    let starting = config
        .draft_template
        .as_deref()
        .map(|text| config.line_endings.normalize(text))
        .unwrap_or_default();
    scaffold_draft(root, &parent, &record, &primary, &starting)?;
    Ok(DraftCreated {
        draft: record,
        file: primary,
    })
}

/// DRS-FR-06 / DRS-FR-01: lay out one new draft's folder under `parent` and
/// write its record, or leave nothing behind.
///
/// Shared by [`create_draft_impl`] and `create_github_shadow_draft`
/// (DRS-FR-INCJ), so both kinds of draft have exactly one layout.
pub(crate) fn scaffold_draft(
    root: &fs::RootFs,
    parent: &Path,
    record: &DraftRecord,
    primary: &str,
    starting: &str,
) -> Result<(), String> {
    // DRS-FR-DGMI / DRS-FR-JDRY: the drafts root's `.gitattributes` and
    // `.gitignore` are established **before the first draft is created**, so
    // the private storage of the first draft is ignored from its first write.
    // Preserves every entry already in either file (DRS-FR-BKFG).
    //
    // Ahead of the first directory the scaffold creates, deliberately: a
    // refusal here must leave nothing behind, and the cleanup that takes a
    // half-built draft back runs only for a failure inside `scaffold` below.
    git_storage::ensure_git_files(root)?;
    let dir = parent.join(&record.id);
    root.create_dir_under(&dir, FILES_DIR).map_err(|e| e.to_string())?;
    // DRS-FR-06: a draft is created holding exactly one Markdown file, so the
    // workspace opens on something to type into rather than on an empty
    // workspace — and on a file whose name already says what the draft is
    // rather than one the author must name a second time.
    //
    // A name the filesystem will not take as a filename — one longer than its
    // limit — fails here, and the half-built folder goes with it: a directory
    // with no record is not a draft, and leaving one behind would accumulate
    // silently under `.synthesis/drafts/` where nothing lists it.
    //
    let scaffold = || -> Result<(), String> {
        root.write_text_atomic(dir.join(FILES_DIR).join(primary), starting)
            .map_err(|e| e.to_string())?;
        // DRS-FR-01 / DCP-FR-01: created with the draft and deleted with it, so
        // `crate::draft_proposals` never has to scaffold a folder of its own —
        // and an empty one is the honest state of a draft nothing has been
        // proposed against.
        root.create_dir_under(&dir, PROPOSALS_DIR).map_err(|e| e.to_string())?;
        // DRS-FR-01 / DHS-FR-01: the folder the prompt's settled versions will
        // be written into, owned by `crate::draft_history`. Scaffolded here for
        // the same reason `proposals/` is — the layout of a draft's folder is
        // this module's — and created **empty**: a new draft has no version,
        // its live prompt being its `Original` until a change is accepted
        // against it (DHS-FR-07).
        root.create_dir_under(&dir, HISTORY_DIR).map_err(|e| e.to_string())?;
        // DRS-FR-01 / DAS-FR-01: the folder the prompt's images will be stored
        // in, owned by `crate::draft_assets`. Scaffolded **empty** with the
        // draft and deleted with it, for the same reason `proposals/` and
        // `history/` are: the layout of a draft's folder is this module's, so
        // the module that owns the images never has to create a folder of its
        // own — and an empty one is the honest state of a draft whose prompt
        // embeds no picture (DRS-FR-01, DRS-FR-04, DRS-FR-11, DRS-FR-15, DRS-FR-21, DRS-FR-40).
        root.create_dir_under(&dir, ASSETS_DIR).map_err(|e| e.to_string())?;
        // DRS-FR-23: a draft is created with no `conversation.jsonl`.
        // Written into the directory just created rather than through a lookup:
        // until `draft.toml` lands the directory is not a draft folder at all
        // (DRS-FR-29), so no walk could find it.
        write_record_at(root, &dir, record)
    };
    if let Err(e) = scaffold() {
        let _ = root.delete_under(parent, &record.id, true);
        return Err(e);
    }
    // PST-FR-DQZT: a creation is a draft event. Raised here, after the record
    // landed, so every route that creates a draft — the Drafts panel and a
    // claim of a GitHub Task alike — commits the draft it created.
    git_storage::commit_event(root, &record.id, &record.name, DraftEvent::Created);
    Ok(())
}

/// DRS-FR-09 / DRS-FR-25: write the new name and rename the primary file to
/// match it — one act rather than two.
///
/// The file is moved *before* the record is written, so the two never disagree
/// about what the draft is called: a collision refuses the rename whole
/// (DRS-FR-26) with nothing on disk and nothing in the record changed, and a
/// record that will not write puts the file back under its old name. A draft
/// whose primary file has been deleted renames its record alone, there being no
/// file left bound to the name (DRS-FR-15).
pub fn rename_draft_impl(root: &crate::fs::RootFs, id: &str, name: &str) -> Result<DraftRecord, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        // DRP-FR-11: committing an empty name leaves the stored name untouched,
        // so this is a refusal rather than a rename to nothing.
        return Err("a draft name cannot be empty".to_string());
    }
    let dir = draft_dir(root, id)?;
    let mut record = read_record_at(root, &dir, id)?;
    // DRS-FR-QPSC: a GitHub-shadow draft is immutable.
    refuse_shadow_record(&record)?;
    let files_dir = dir.join(FILES_DIR);
    // Where the primary file was moved to, and the name to put it back under if
    // the record cannot be written.
    let mut moved: Option<(String, String)> = None;
    if let Some(primary) = record.prompt_path.clone() {
        let target = prompt_file_name(trimmed);
        let (_, current) = split_path(&primary);
        if current != target {
            if files_dir.join(&primary).is_file() {
                // DRS-FR-26: a collision names the path it refused, so the
                // author corrects the one name that could not be honoured.
                rename_entry(root, &files_dir, &primary, &target)?;
                moved = Some((with_basename(&primary, &target), primary.clone()));
                record.prompt_path = Some(with_basename(&primary, &target));
            } else if let Some(staged) = staged_leftover(&files_dir, &primary) {
                // A case-only rename was interrupted between its two legs and
                // the file is parked under the staging name. Finish the move
                // rather than reading the gap as a deleted file, which would end
                // the binding for a file that is still there.
                rename_entry(root, &files_dir, &staged, &target)?;
                moved = Some((with_basename(&primary, &target), primary.clone()));
                record.prompt_path = Some(with_basename(&primary, &target));
            } else {
                // The file the record points at is gone — deleted outside this
                // application, since `delete_draft_path` clears the pointer
                // itself. The binding ends the same way it does there: the name
                // is the draft's alone from here, and no other file is promoted.
                record.prompt_path = None;
            }
        }
    }
    record.name = trimmed.to_string();
    record.updated_at = now_rfc3339();
    if let Err(e) = write_record_at(root, &dir, &record) {
        // The file has already moved. Put it back rather than leaving a draft
        // whose record and whose file disagree about its name (DRS-FR-26).
        if let Some((now_at, was)) = moved {
            let (_, original) = split_path(&was);
            // Through `rename_entry`, not the raw primitive: undoing a case-only
            // rename is itself case-only, and the primitive refuses those.
            let _ = rename_entry(root, &files_dir, &now_at, original);
        }
        return Err(e);
    }
    Ok(record)
}

/// DRS-FR-10: the two positions a caller may set, and only those.
///
/// A call naming `graduated` is a typed validation error rather than a status
/// change: that status is set by `set_draft_graduated` and by nothing else
/// (DRS-FR-20).
pub const ERR_STATUS_NOT_SETTABLE: &str = "draft_status_not_settable";

/// DRS-FR-10 / DRS-FR-KQTW: move the draft between `active` and `archived`.
///
/// A draft whose `graduated` status **stands** never reaches here: the command
/// above refuses it through `require_unlocked_draft`, which reads the runs
/// rather than the record. What does reach here is a **released** draft whose
/// record still says `graduated` — every run that published for it having ended
/// `discarded` or `failed` — and the write normalises that record, which is the
/// one path by which the stored value catches up with the resolved one.
pub fn set_draft_status_impl(
    root: &crate::fs::RootFs,
    id: &str,
    status: DraftStatus,
) -> Result<DraftRecord, String> {
    // DRS-FR-10: neither terminal-ish position is one a caller may set.
    // `graduated` belongs to `set_draft_graduated` (DRS-FR-20) and `published`
    // to `set_draft_published` (DRS-FR-VKQO).
    // DRS-FR-WFLY: `github_shadow` belongs to `create_github_shadow_draft`.
    if matches!(
        status,
        DraftStatus::Graduated | DraftStatus::Published | DraftStatus::GithubShadow
    ) {
        return Err(ERR_STATUS_NOT_SETTABLE.to_string());
    }
    let dir = draft_dir(root, id)?;
    let mut record = read_record_at(root, &dir, id)?;
    // DRS-FR-QPSC / DRS-FR-WFLY: a GitHub-shadow draft does not move out of
    // its status by this route, whatever status it reports.
    refuse_shadow_record(&record)?;
    record.status = status;
    record.updated_at = now_rfc3339();
    write_record_at(root, &dir, &record)?;
    Ok(record)
}

pub fn delete_draft_impl(root: &fs::RootFs, store: &fs::RootFs, id: &str) -> Result<(), String> {
    let at = find_draft(root, id)?;
    // DRS-FR-QPSC: a GitHub-shadow draft is never deleted. A record that does
    // not read is no shadow, so the deletion of a damaged draft stays possible.
    let record = read_record_at(root, &at.dir, id).ok();
    if let Some(record) = record.as_ref() {
        refuse_shadow_record(record)?;
    }
    // PST-FR-YWXF: the deletion commit names the draft by the name it held. A
    // damaged record has no name to read, so its id stands in for one.
    let name = record.map(|record| record.name).unwrap_or_else(|| id.to_string());
    // DRS-FR-ZLBK: the draft's statistics log goes first, which is what makes
    // the operation retryable — the log is removed, then the folder, and a
    // failure at either step returns a typed error with the draft's record
    // still on disk, so the same call repeated completes what is left. A log
    // that is already absent is an already completed deletion (DSS-FR-VPBS),
    // so a second `delete_draft` after a partial one succeeds. Success means
    // both are gone: the draft is never reported deleted while either remains.
    // The barrier that makes the removal below final. Statistics recording is
    // asynchronous (per `DSS-draft-statistics-storage.md` DSS-FR-TUMX), so an
    // event handed over before this call may still be queued; draining the
    // writer first lands every one of them while the draft is still here, and
    // the writer refuses to append for a draft whose folder has gone, so
    // nothing can arrive after this point to recreate the log. Waiting here
    // costs the author a deletion that is already removing a directory tree,
    // and DSS-FR-TUMX is about never blocking the operation that *produced* an
    // event rather than the one that ends the draft.
    crate::statistics::wait_for_writer();
    crate::statistics::delete_draft_statistics(store, id)?;
    // DRS-FR-WNTA: the draft's review threads go with it, through
    // `CMS-comments-storage.md`'s deletion (CMS-FR-39) and by no other route.
    // Before the folder is removed, on exactly the terms the statistics log is,
    // so a failure at either step leaves the record on disk and the same call
    // repeated completes what is left.
    crate::comments::delete_draft_comments(store, id)?;
    // DRP-FR-12: the draft's files, brief, and history go with it — and nothing
    // outside `.synthesis/drafts/` is touched, because a draft has never been in
    // the project. Its statistics and its conversations are in the repository
    // machine store, removed above. DRS-FR-21: the drafts
    // folder it was filed in is left standing and empty rather than removed,
    // because it is the author's structure and outlives what was in it.
    root.delete_under(drafts_dir(root), folder_join(&at.folder, id), true)
        .map_err(|e| e.to_string())?;
    // DRS-FR-21 / PST-FR-DQZT: a deletion is a draft event. The commit names
    // every path Git tracks under that draft's folder and no other path
    // (DRS-FR-VECL).
    git_storage::commit_event(root, id, &name, DraftEvent::Deleted);
    // The log again, now that the draft's record is gone. Statistics recording
    // is asynchronous (per `DSS-draft-statistics-storage.md` DSS-FR-TUMX), so an
    // event queued before this call can reach the writer between the two
    // removals above; the writer refuses to append for a draft whose folder has
    // gone, and this second removal is what closes the instant before it had.
    // Idempotent by DSS-FR-VPBS, so in the ordinary case it removes nothing and
    // costs nothing, and DRS-FR-ZLBK's "success means both are gone" is what it
    // is here to keep true.
    crate::statistics::delete_draft_statistics(store, id)?;
    // DRS-FR-WNTA: and the review again, on exactly the same terms. An agent
    // turn settling while this call runs reaches the same window a queued
    // statistics event does, and CMS-FR-39's deletion is idempotent, so the
    // second removal costs nothing in the ordinary case and closes that instant
    // in the one that matters.
    crate::comments::delete_draft_comments(store, id)
}


/// DRS-FR-12: read the draft's prompt. `path` names that prompt and any other
/// path is a typed "not found"; an inconsistent draft is refused outright.
pub fn load_draft_file_impl(root: &fs::RootFs, id: &str, path: &str) -> Result<DraftContents, String> {
    let dir = draft_dir(root, id)?;
    let prompt = require_prompt_at(root, &dir, id)?;
    if path != prompt {
        return Err(format!("draft file not found: {path}"));
    }
    let abs = draft_file_path_in(root, &dir, path)?;
    let body = root.read_text(&abs).map_err(|e| e.to_string())?;
    let checksum = fs::sha256_bytes(body.as_bytes());
    Ok(DraftContents { body, checksum })
}

/// DRS-FR-12: **the only write path for a draft's prompt**, whoever composed the
/// text.
///
/// The author's typing reaches it through the tab's debounce (NAW-FR-13) and an
/// accepted proposal reaches it from inside the acceptance transaction
/// (DHS-FR-15) rather than through a write of its own, so the prompt changes in
/// exactly one way and the bytes a history snapshot captures are the bytes this
/// call persisted.
///
/// It overwrites the prompt whole, normalises every line ending in `body` to the
/// project's convention (PSS-FR-17) before writing, and returns the checksum of
/// the bytes **actually written** rather than of the string it was handed — on
/// exactly the terms PST-FR-22 normalises and checksums an artifact write.
pub fn save_draft_file_impl(
    root: &fs::RootFs,
    id: &str,
    file: &str,
    body: &str,
) -> Result<DraftSaveResult, String> {
    let dir = draft_dir(root, id)?;
    let record = read_record_at(root, &dir, id)?;
    // DRS-FR-QPSC: a GitHub-shadow draft's prompt takes no write. Checked on
    // the record the prompt is resolved from, so the guard costs no read.
    refuse_shadow_record(&record)?;
    let prompt = prompt_of(root, &dir, &record).ok_or_else(|| ERR_NOT_SINGLE_FILE.to_string())?;
    // DRS-FR-12 / DRS-FR-13: `path` names the prompt and nothing else. Without
    // this the atomic write would *create* whatever path it was given, which is
    // the one way a caller could put a draft into the state DRS-FR-11 forbids.
    if file != prompt {
        return Err(format!("draft file not found: {file}"));
    }
    let path = draft_file_path_in(root, &dir, file)?;
    let normalised = project_settings::line_endings_for(root).normalize(body);
    root.write_text_atomic(&path, &normalised).map_err(|e| e.to_string())?;
    // PST-FR-DQZT: a save is no draft event, so nothing is committed here.
    // The next draft event of this draft, or the author, commits it.
    touch_at(root, &dir, id);
    Ok(DraftSaveResult {
        checksum: fs::sha256_bytes(normalised.as_bytes()),
    })
}

/// [`save_draft_file_impl`] for the acceptance transaction, which has already
/// resolved the draft's directory and checked the invariant, and which must be
/// able to restore the prior bytes during a rollback without re-deriving either
/// (DHS-FR-15, DHS-FR-19).
pub(crate) fn save_prompt_at(
    root: &fs::RootFs,
    dir: &Path,
    id: &str,
    prompt: &str,
    body: &str,
) -> Result<DraftSaveResult, String> {
    let path = draft_file_path_in(root, dir, prompt)?;
    let normalised = project_settings::line_endings_for(root).normalize(body);
    root.write_text_atomic(&path, &normalised).map_err(|e| e.to_string())?;
    // PST-FR-DQZT: a save is no draft event, so nothing is committed here.
    // The next draft event of this draft, or the author, commits it.
    touch_at(root, dir, id);
    Ok(DraftSaveResult {
        checksum: fs::sha256_bytes(normalised.as_bytes()),
    })
}

/// The line-ending convention a draft write applies (DRS-FR-12), for a caller
/// that has to know what the bytes on disk will be before it writes them —
/// the acceptance transaction's `no_change` comparison and its journal's
/// candidate digest (DHS-FR-15, DCP-FR-07).
pub fn normalise_for_write(root: &fs::RootFs, body: &str) -> String {
    project_settings::line_endings_for(root).normalize(body)
}

/// Record activity on a draft so the panel's ordering and its relative
/// timestamp follow what the author actually did (DRP-FR-03 / DRP-FR-08).
/// Best-effort: failing to restamp must never fail the write that succeeded.
fn touch(root: &crate::fs::RootFs, id: &str) {
    if let Ok(dir) = draft_dir(root, id) {
        touch_at(root, &dir, id);
    }
}

/// [`touch`] for a caller that already holds the draft's directory.
///
/// Resolving a draft id now means walking the drafts root (DRS-FR-29), and the
/// bare `touch` costs two of those walks — one to read the record and one to
/// write it back. Every single-draft mutation below resolves the directory once
/// and threads it through, so a save costs one walk rather than three.
fn touch_at(root: &crate::fs::RootFs, dir: &Path, id: &str) {
    if let Ok(mut record) = read_record_at(root, dir, id) {
        record.updated_at = now_rfc3339();
        let _ = write_record_at(root, dir, &record);
    }
}

/// [`draft_file_path`] for a caller that already holds the draft's directory.
fn draft_file_path_in(root: &fs::RootFs, dir: &Path, path: &str) -> Result<PathBuf, String> {
    if !is_valid_draft_path(path) {
        return Err(format!("invalid draft path: {path:?}"));
    }
    let files = dir.join(FILES_DIR);
    let abs = fs::resolve_under(&files, path).map_err(|e| e.to_string())?;
    reject_symlinked_components(root, &files, path)?;
    Ok(abs)
}

/// DRS-FR-13: there is no operation that creates, renames, or deletes a file or
/// a folder inside a draft.
///
/// Nothing stood here by accident. A draft's file set is settled at creation
/// (DRS-FR-06) and changes only when the draft is renamed, which renames the one
/// file it holds (DRS-FR-14) — so a draft can never be reduced to no file, never
/// grown into a tree, and no caller can put one into the state DRS-FR-11
/// forbids.

/// DRS-FR-20: mark a draft `graduated`, the one path to that status.
///
/// Reached by `GRD-graduation.md` alone when a run publishes successfully in
/// Git mode or is accepted in place (GRD-FR-ARLT). It is registered as no Tauri
/// command, so no frontend call can graduate a draft.
///
/// It is not archiving and not deletion: the draft's folder, its record, its
/// prompt, its history, its proposals, its comment logs, and its conversation
/// log are all retained exactly as they were. `run_id` names the run that
/// graduated it, which the caller logs — the durable link between the two is
/// the graduation queue's own record (DRS-FR-18), so nothing is copied into the
/// draft that could fall out of step with it.
pub fn set_draft_graduated(
    root: &fs::RootFs,
    id: &str,
    run_id: &str,
) -> Result<DraftRecord, String> {
    let _ = run_id;
    let dir = draft_dir(root, id)?;
    let mut record = read_record_at(root, &dir, id)?;
    // DRS-FR-20 / GRD-FR-ARLT: the status becomes `graduated` at exactly the
    // moment the boundary is crossed and no later. A draft already carrying it
    // is left **byte-for-byte** as it is rather than rewritten with a fresh
    // `updated_at`: the run goes on transitioning through its implementation
    // part, and a record rewritten on each of those transitions would show the
    // draft changing long after the one act that changed it — and, where the
    // draft lives inside the repository, would make the author's own checkout
    // dirty every time the run moved.
    if record.status == DraftStatus::Graduated {
        return Ok(record);
    }
    record.status = DraftStatus::Graduated;
    record.updated_at = now_rfc3339();
    write_record_at(root, &dir, &record)?;
    Ok(record)
}

/// DRS-FR-VKQO: mark a draft `published`, the one path to that status.
///
/// Reached by `crate::github_publication` alone, once a GitHub issue exists
/// (GHP-FR-BSYH). It is registered as no Tauri command, so no frontend call can
/// publish a draft.
///
/// It is set **only over `active`**. A `graduated` draft keeps `graduated`,
/// because local graduation and GitHub publication are different facts and the
/// scalar status records the stronger one; a draft already `published` is left
/// byte-for-byte as it is, so a second publication does not make the author's
/// checkout dirty for a record that did not change. An `archived` draft never
/// reaches here: publication is refused against one (GHP-FR-GJEO).
pub fn set_draft_published(root: &fs::RootFs, id: &str) -> Result<DraftRecord, String> {
    let dir = draft_dir(root, id)?;
    let mut record = read_record_at(root, &dir, id)?;
    if record.status != DraftStatus::Active {
        return Ok(record);
    }
    record.status = DraftStatus::Published;
    record.updated_at = now_rfc3339();
    write_record_at(root, &dir, &record)?;
    Ok(record)
}

// ---------------------------------------------------------------------------
// Tauri commands
// ---------------------------------------------------------------------------

/// DRS-FR-22: the set of drafts, or one draft's record or file set, changed.
///
/// Emitted after the change has landed on disk, so a listener that re-reads on
/// it sees the result rather than racing it. It is what keeps the Drafts panel
/// (DRP-FR-05) and any open New Artifact tab current without polling.
///
/// The wire name is kebab-case rather than the spec's prose name: Tauri accepts
/// only alphanumerics, `-`, `/`, `:` and `_` in an event name, and rejects the
/// rest at `emit` — which returns an error every emit site discards, leaving a
/// channel that is silently dead in production. Every other event in this
/// application spells itself the same way.
pub const DRAFTS_CHANGED: &str = "drafts-changed";

/// One change to a draft, as it travels the internal channel of DRS-FR-28.
///
/// The channel exists because `.synthesis/drafts/` sits outside what the
/// project scan surfaces (ASC-FR-09), so no watcher reports a draft's files and
/// an indexer has nothing else to observe them through. It is internal by
/// construction: not a Tauri event, and reachable by no frontend `invoke`.
///
/// It carries paths alone and never a file's contents — a draft's text is user
/// content, and the consumer reads it from disk itself if it wants it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DraftChange {
    /// The draft that changed.
    pub draft_id: String,
    /// Draft-relative paths this change created, rewrote, renamed, or removed.
    /// Empty when the change was to the record alone, or when the whole draft
    /// went.
    ///
    /// There is no second list of project paths beside it: a graduation writes
    /// into the project through its own targeted operation and leaves the draft
    /// where it is (DRS-FR-18), so nothing this channel carries ever spans both
    /// sides (`../core/BMI-bm25-indexing.md` BMI-FR-20).
    pub paths: Vec<String>,
}

impl DraftChange {
    /// A change confined to one draft's own files.
    pub(crate) fn to(draft_id: &str, paths: Vec<String>) -> DraftChange {
        DraftChange {
            draft_id: draft_id.to_string(),
            paths,
        }
    }

    /// A change to the draft's record alone — no file of it moved.
    fn record_of(draft_id: &str) -> DraftChange {
        DraftChange::to(draft_id, Vec::new())
    }
}

/// Announce a change: the `"drafts changed"` event the panel and any open New
/// Artifact tab listen on (DRS-FR-22), and the internal channel of DRS-FR-28.
///
/// Best-effort on both: a draft that was written must not be reported as failed
/// because the notification of it could not be delivered, and surfacing on the
/// internal channel is not a precondition of any command here — a consumer's
/// absence or slowness delays no draft operation.
pub(crate) fn announce<R: tauri::Runtime>(app: &tauri::AppHandle<R>, change: DraftChange) {
    let _ = app.emit(DRAFTS_CHANGED, ());
    // PST-FR-35: which drafts qualify for the Dashboard's Active workstreams
    // widget, and what their names and statuses are, follow this event. A
    // refresh caused by it re-reads the same prompt-file times and therefore
    // reorders nothing — what moves the ordering is `"draft prompt changed"`
    // (DRS-FR-42).
    crate::dashboard::note_active_drafts_changed(app);
    crate::bm25_index::note_draft_change(app, &change);
}

/// DRS-FR-22: a sibling module changed one draft's record, so the panel and any
/// open tab re-read it.
///
/// The record and not a file: publication moves the status and nothing under
/// `files/`, so the change names no path.
pub fn announce_drafts_changed<R: tauri::Runtime>(app: &tauri::AppHandle<R>, draft_id: &str) {
    announce(app, DraftChange::record_of(draft_id));
}

/// DRS-FR-01: the folder a draft's change proposals live in, owned by
/// `crate::draft_proposals` (`DCP-draft-change-proposals.md` DCP-FR-01).
///
/// Resolved here rather than there because the draft's folder layout is this
/// module's (DRS-FR-01), so the escape gate on a draft id binds that module too
/// — and because a proposal folder is deleted with the draft by nothing more
/// than being inside it (DRS-FR-20, DRS-FR-21), which is only true while this
/// module decides where it sits.
pub fn draft_proposals_dir(root: &crate::fs::RootFs, id: &str) -> Result<PathBuf, String> {
    Ok(draft_dir(root, id)?.join(PROPOSALS_DIR))
}

/// The absolute on-disk path of a draft file, through the escape gate of
/// DRS-FR-16, for a reader outside this module.
///
/// `../core/BMI-bm25-indexing.md` (BMI-FR-04) indexes a draft's files and needs
/// to open them; routing it through this keeps the symlink and `..` refusals of
/// DRS-FR-16 binding on that reader too, rather than leaving it to rebuild the
/// path itself and inherit none of them.
pub fn draft_file_abs_path(root: &crate::fs::RootFs, id: &str, path: &str) -> Result<PathBuf, String> {
    draft_file_path(root, id, path)
}


#[cfg(test)]
mod tests;

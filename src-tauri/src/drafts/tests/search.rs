//! Search across drafts, by name and by contents.

use super::*;

// -- search ------------------------------------------------------------

#[test]
fn search_matches_a_draft_by_name_without_reading_its_files() {
    // DRS-FR-17: the name is checked first and short-circuits the walk, so
    // the common case — typing a name you remember — opens no file at all.
    //
    // A file the walk *cannot* read is what makes that observable: it holds
    // the needle, so a search that reached it would either report a contents
    // match or take the time to find out. Neither happens.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "editor-tweaks");
    let files = root.join(DRAFTS_REL).join(&id).join(FILES_DIR);
    let mut oversized = "x".repeat(SEARCH_MAX_BYTES as usize + 1);
    oversized.push_str("editor");
    std::fs::write(files.join("big.md"), oversized).unwrap();

    let hits = search_drafts_impl(root, "EDITOR");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].draft_id, id);
    assert_eq!(hits[0].matched_in, DraftMatchedIn::Name);
}

#[test]
fn search_matches_a_draft_by_the_text_inside_its_files() {
    // DRS-FR-17: the whole point — a draft is found by a phrase written in
    // it, not only by what it was called, and the reason is reported so the
    // panel can annotate the row (DRP-FR-17).
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let named = draft(root, "overlay-work");
    let inside = draft(root, "editor-tweaks");
    save_draft_file_impl(root, &inside, "editor-tweaks.md", "the single-OVERLAY invariant")
        .unwrap();

    let hits = search_drafts_impl(root, "overlay");

    assert_eq!(hits.len(), 2);
    let by_name = hits.iter().find(|h| h.draft_id == named).unwrap();
    let by_text = hits.iter().find(|h| h.draft_id == inside).unwrap();
    assert_eq!(by_name.matched_in, DraftMatchedIn::Name);
    // Case-insensitively on both sides.
    assert_eq!(by_text.matched_in, DraftMatchedIn::Contents);
}

#[test]
fn search_finds_nothing_in_a_draft_that_holds_the_text_nowhere() {
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "unrelated");
    save_draft_file_impl(root, &id, "unrelated.md", "nothing to see").unwrap();

    assert!(search_drafts_impl(root, "overlay").is_empty());
}

#[test]
fn an_empty_query_matches_every_draft_on_its_name() {
    // DRS-FR-17: an empty filter is not a filter, and annotates no row.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    draft(root, "a");
    draft(root, "b");

    for query in ["", "   "] {
        let hits = search_drafts_impl(root, query);
        assert_eq!(hits.len(), 2, "{query:?}");
        assert!(hits.iter().all(|h| h.matched_in == DraftMatchedIn::Name));
    }
}

#[test]
fn a_draft_matching_on_both_its_name_and_its_contents_reports_the_name() {
    // DRS-FR-17: the name is checked first and short-circuits the walk. The
    // reported reason is the observable half of that — remove the `continue`
    // in the name branch and this draft comes back as a contents match.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "overlay-work");
    save_draft_file_impl(root, &id, "overlay-work.md", "the overlay invariant").unwrap();

    let hits = search_drafts_impl(root, "overlay");

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].matched_in, DraftMatchedIn::Name);
}

#[test]
fn a_file_past_the_size_cap_is_not_searched() {
    // DRS-FR-17 / the DRP non-functional requirement: this runs per query
    // over every draft, so it is not a project search and does not pretend
    // to be one.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "huge");
    let mut body = "x".repeat(SEARCH_MAX_BYTES as usize + 1);
    body.push_str("needle");
    save_draft_file_impl(root, &id, "huge.md", &body).unwrap();

    assert!(search_drafts_impl(root, "needle").is_empty());

    // Just under the cap is searched normally, so the cap is a cap rather
    // than a general refusal to read.
    save_draft_file_impl(root, &id, "huge.md", "needle").unwrap();
    assert_eq!(search_drafts_impl(root, "needle").len(), 1);
}

#[cfg(unix)]
#[test]
fn neither_walk_follows_a_symlinked_directory_or_loops_on_one() {
    // DRS-FR-16. Following would put another folder's files in the rail and
    // have the panel's filter read a home directory on every query; a link
    // to an ancestor would recurse until the stack gave out, taking down
    // `list_drafts` — which the Drafts panel calls on every mount.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::create_dir(root.join("elsewhere")).unwrap();
    std::fs::write(root.join("elsewhere/secret.md"), b"needle").unwrap();
    let id = draft(root, "d");
    let files = root.join(DRAFTS_REL).join(&id).join(FILES_DIR);
    std::os::unix::fs::symlink(root.join("elsewhere"), files.join("linked")).unwrap();
    std::os::unix::fs::symlink(&files, files.join("loop")).unwrap();

    // Terminates, and the search never reached through either link.
    // DRS-FR-11: a symlink inside a draft is not the prompt, so the draft
    // reads as inconsistent — reported rather than silently reduced to
    // whichever entry looks most like one (DRS-FR-15).
    assert!(listed(root)[0].inconsistent);
    assert!(search_drafts_impl(root, "needle").is_empty());
}

#[test]
fn a_file_that_is_not_utf8_contributes_no_match() {
    // DRS-FR-17: not text the author wrote, so not something to search.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "binary-holder");
    let files = root.join(DRAFTS_REL).join(&id).join(FILES_DIR);
    // The needle's bytes are present, but the file does not decode.
    let mut bytes = b"overlay".to_vec();
    bytes.push(0xff);
    std::fs::write(files.join("blob.bin"), bytes).unwrap();

    assert!(search_drafts_impl(root, "overlay").is_empty());
}

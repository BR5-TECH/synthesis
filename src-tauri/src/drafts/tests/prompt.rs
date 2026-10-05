//! The prompt path — what a draft path may and may not reach.

use super::*;

// -- the prompt --------------------------------------------------------

#[test]
fn a_draft_path_cannot_escape_its_own_draft() {
    // DRS-FR-16, through `resolve_under` plus the segment gate in front of it.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let id = draft(root, "d");
    let other = draft(root, "other");
    save_draft_file_impl(root, &other, "other.md", "mine").unwrap();

    for escape in [
        format!("../../{other}/files/other.md"),
        "/other.md".to_string(),
        "..".to_string(),
    ] {
        assert!(
            save_draft_file_impl(root, &id, &escape, "pwned").is_err(),
            "{escape:?} must be refused"
        );
        assert!(load_draft_file_impl(root, &id, &escape).is_err());
    }
    assert_eq!(
        load_draft_file_impl(root, &other, "other.md").unwrap().body,
        "mine"
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_inside_a_draft_is_not_followed_out_of_it() {
    // DRS-FR-16. `resolve_under` is syntactic, so it accepts
    // `secret.md` as a path within the draft; only the link check refuses
    // it. Without that check a read hands back the target's contents and a
    // write lands on the target — outside `.synthesis/drafts/` entirely.
    let dir = project();
    let root = &crate::fs::RootFs::for_root(dir.path());
    std::fs::write(root.join("outside.md"), b"not yours").unwrap();
    let id = draft(root, "d");
    let files = root.join(DRAFTS_REL).join(&id).join(FILES_DIR);
    std::os::unix::fs::symlink(root.join("outside.md"), files.join("secret.md")).unwrap();
    // And a linked *folder*, so the check is not just about the leaf.
    std::fs::create_dir(root.join("elsewhere")).unwrap();
    std::fs::write(root.join("elsewhere/x.md"), b"also not yours").unwrap();
    std::os::unix::fs::symlink(root.join("elsewhere"), files.join("linked")).unwrap();

    for path in ["secret.md", "linked/x.md"] {
        assert!(
            load_draft_file_impl(root, &id, path).is_err(),
            "{path:?} must not be readable through a symlink"
        );
        assert!(
            save_draft_file_impl(root, &id, path, "pwned").is_err(),
            "{path:?} must not be writable through a symlink"
        );
    }

    assert_eq!(
        std::fs::read_to_string(root.join("outside.md")).unwrap(),
        "not yours"
    );
    assert_eq!(
        std::fs::read_to_string(root.join("elsewhere/x.md")).unwrap(),
        "also not yours"
    );
}


#[test]
fn an_invalid_draft_id_is_refused_before_it_reaches_the_filesystem() {
    let dir = project();
    for bad in ["", "../evil", "a/b", "a\\b", ".", ".."] {
        assert!(!is_valid_draft_id(bad), "{bad:?} must be refused");
        assert!(open_draft_impl(&crate::fs::RootFs::for_root(dir.path()), bad).is_err());
    }
}

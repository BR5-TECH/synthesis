//! The organisation of the drafts root: the author's own folders
//! (`DRS-draft-storage.md` DRS-FR-29 … DRS-FR-35).

use super::*;

// ---------------------------------------------------------------------------
// The organisation of the drafts root (DRS-FR-29 … DRS-FR-35)
//
// Every operation below moves directories and nothing else. There is no
// manifest to keep in step, so each one either changes the shape on disk or
// returns a typed error having changed nothing — and a caller acting on a
// hierarchy that has since moved learns of it as a path that no longer
// resolves (DRS-FR-35), which is why none of them carries a revision token.
// ---------------------------------------------------------------------------

/// DRS-FR-30: create one empty folder called `name` directly inside `parent`.
pub fn create_drafts_folder_impl(
    root: &fs::RootFs,
    parent: &str,
    name: &str,
) -> Result<DraftFolder, String> {
    if !is_valid_folder_name(name) {
        return Err(format!("invalid folder name: {name:?}"));
    }
    let parent_dir = resolve_drafts_folder(root, parent)?;
    if name_taken(root, &parent_dir, name) {
        return Err(format!("a folder called {name:?} is already here"));
    }
    root.create_dir_under(&parent_dir, name).map_err(|e| e.to_string())?;
    Ok(DraftFolder {
        path: folder_join(parent, name),
        parent: parent.to_string(),
    })
}

/// DRS-FR-30: rename a folder in place, keeping its parent and its subtree.
///
/// Nothing moves: every draft beneath keeps its id and its files, and the
/// folder keeps everything under it. What *does* change is the identity of every
/// descendant path, a drafts folder having no id beyond where it sits
/// (DRS-FR-29) — which is why the panel re-lists rather than patching its tree.
pub fn rename_drafts_folder_impl(
    root: &fs::RootFs,
    path: &str,
    name: &str,
) -> Result<DraftFolder, String> {
    if path.is_empty() {
        return Err("the drafts root cannot be renamed".to_string());
    }
    if !is_valid_folder_name(name) {
        return Err(format!("invalid folder name: {name:?}"));
    }
    // Resolved first, so a folder that has since been renamed or removed
    // underneath the caller is the typed "not found" rather than a collision
    // against something unrelated.
    resolve_drafts_folder(root, path)?;
    let parent = folder_parent(path);
    let parent_dir = resolve_drafts_folder(root, parent)?;
    let (_, current) = split_path(path);
    if current == name {
        return Err(format!("this folder is already called {name:?}"));
    }
    // A case-only change collides with the folder itself on APFS and NTFS, and
    // capitalising a folder one made is an ordinary act — so the collision test
    // skips the entry being renamed and `rename_entry_named` parks it under a
    // staging name to get past the filesystem's own refusal. The same predicate
    // the staging decision is made with, so the two cannot drift; `current` and
    // `name` already differ by the refusal just above, which is what makes this
    // exactly "not a case-only change".
    if !is_case_only_rename(current, name) && name_taken(root, &parent_dir, name) {
        return Err(format!("a folder called {name:?} is already here"));
    }
    rename_entry_named(root, &drafts_dir(root), path, name, &|n: &str| {
        format!("a folder called {n:?} is already here")
    })?;
    Ok(DraftFolder {
        path: with_basename(path, name),
        parent: parent.to_string(),
    })
}

/// DRS-FR-31: remove a folder without removing anything it holds.
///
/// Every direct child is checked against the destination *before* the first one
/// moves, so the only failure the checks leave is an I/O one and the operation
/// is all-or-nothing in every case a caller can provoke. An I/O failure part-way
/// stops immediately and names the child that failed, leaving the children
/// already moved in their new parent — which the Drafts panel resolves by
/// re-listing rather than by assuming (DRP-FR-32).
///
/// The emptied folder is removed **non-recursively**, so one that unexpectedly
/// still holds something is reported rather than deleted.
pub fn delete_drafts_folder_impl(root: &fs::RootFs, path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("the drafts root cannot be deleted".to_string());
    }
    let dir = resolve_drafts_folder(root, path)?;
    let parent = folder_parent(path).to_string();
    let parent_dir = resolve_drafts_folder(root, &parent)?;

    let entries = root.list_dir(&dir).map_err(|e| e.to_string())?;
    // A drafts folder holds directories — the author's folders and the drafts
    // themselves. Anything else got there some other way, and moving it is not
    // this command's business; refused up front rather than left for the
    // non-recursive removal below to trip over, because *that* failure would
    // come after every child had already moved and would leave exactly the
    // partial result DRS-FR-31 says a caller cannot provoke.
    if let Some(stray) = entries.iter().find(|e| e.kind != fs::EntryKind::Dir) {
        return Err(format!(
            "{path:?} holds {:?}, which is not a folder or a draft; nothing was moved",
            stray.name
        ));
    }
    let children: Vec<String> = entries.into_iter().map(|e| e.name).collect();

    // Pre-flight. The folder being deleted is itself an entry of `parent_dir`,
    // so a child sharing its name collides here — correctly: at the moment the
    // move would run, that name is taken.
    for child in &children {
        if name_taken(root, &parent_dir, child) {
            return Err(format!(
                "{child:?} cannot move out of {path:?}: something of that name is already there"
            ));
        }
    }
    // …and against each other. `UI/docs` and `UI/Docs` coexist on ext4 and both
    // pass the check above, but the second to arrive in the destination would
    // collide with the first — after it had already moved. Checked here so the
    // whole call is refused with nothing moved.
    for (i, child) in children.iter().enumerate() {
        let folded = child.to_lowercase();
        if children[..i].iter().any(|other| other.to_lowercase() == folded) {
            return Err(format!(
                "{path:?} holds two entries called {child:?}; nothing was moved"
            ));
        }
    }
    for child in &children {
        root.move_under(
            drafts_dir(root),
            folder_join(path, child),
            folder_join(&parent, child),
        )
        .map_err(|e| format!("{child:?} could not be moved out of {path:?}: {e}"))?;
    }
    root.delete_under(drafts_dir(root), path, false)
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// DRS-FR-32: move a draft's own directory into the drafts folder `folder`
/// names.
///
/// Exactly one thing about the draft changes — the path its directory sits at.
/// Its id, name, `primary_path`, status, `destination_root`, `created_at`, every
/// file, every comment log, every proposal and its conversation log all travel
/// with the directory unchanged, and no *draft-relative* path changes at all,
/// which is why the internal channel of DRS-FR-28 surfaces nothing and the
/// drafts index needs no work.
pub fn move_draft_to_folder_impl(
    root: &fs::RootFs,
    id: &str,
    folder: &str,
) -> Result<DraftSummary, String> {
    let at = find_draft(root, id)?;
    // DRS-FR-QPSC: a GitHub-shadow draft stays where it was created.
    if let Ok(record) = read_record_at(root, &at.dir, id) {
        refuse_shadow_record(&record)?;
    }
    if same_folder(&at.folder, folder) {
        // A validation error rather than a no-op, so a caller learns it asked
        // for nothing instead of being told a move happened that did not.
        // Case-folded, or on APFS the same folder under another capitalisation
        // would fall through to a bare "already exists" from the primitive.
        //
        // Ahead of resolving the destination, so the answer does not depend on
        // the filesystem: `ui` resolves to `UI` on APFS and to nothing on ext4,
        // and a caller told "already there" on one platform and "no such
        // folder" on the other has been told two different things about one
        // state. The folded name IS the folder here, as it is everywhere else
        // in this module (DRS-FR-30), so the draft is in the folder named.
        return Err("this draft is already in that folder".to_string());
    }
    let dest_dir = resolve_drafts_folder(root, folder)?;
    if name_taken(root, &dest_dir, id) {
        return Err(format!("{id:?} is already in the destination"));
    }
    root.move_under(
        drafts_dir(root),
        folder_join(&at.folder, id),
        folder_join(folder, id),
    )
    .map_err(|e| e.to_string())?;
    // DRS-FR-24: filing a draft is a change to it, so its last-activity instant
    // moves with it.
    touch(root, id);
    list_drafts_impl(root)
        .drafts
        .into_iter()
        .find(|d| d.id == id)
        .ok_or_else(|| format!("draft not found: {id}"))
}

/// DRS-FR-33: move a folder and its whole subtree into `destination`.
///
/// One directory rename: the subtree arrives whole, every folder beneath keeps
/// its position relative to the moved one, and every draft beneath keeps its id
/// and everything it holds.
pub fn move_drafts_folder_impl(
    root: &fs::RootFs,
    path: &str,
    destination: &str,
) -> Result<DraftFolder, String> {
    if path.is_empty() {
        return Err("the drafts root cannot be moved".to_string());
    }
    resolve_drafts_folder(root, path)?;
    // A folder is never placed inside itself, at any depth. Checked on the paths
    // rather than on the resolved directories because that is where the answer
    // is: `destination` is inside `path` exactly when it is `path` or begins
    // with it followed by a separator.
    if folder_contains(path, destination) {
        return Err(format!("{path:?} cannot be moved inside itself"));
    }
    // Before the destination is resolved, for the reason the same check in
    // `move_draft_to_folder_impl` is: a folded name resolves on APFS and not on
    // ext4, and "already there" is the true answer on both.
    if same_folder(folder_parent(path), destination) {
        return Err("this folder is already there".to_string());
    }
    let dest_dir = resolve_drafts_folder(root, destination)?;
    let (_, name) = split_path(path);
    if name_taken(root, &dest_dir, name) {
        return Err(format!("a folder called {name:?} is already there"));
    }
    root.move_under(
        drafts_dir(root),
        path,
        folder_join(destination, name),
    )
    .map_err(|e| e.to_string())?;
    Ok(DraftFolder {
        path: folder_join(destination, name),
        parent: destination.to_string(),
    })
}


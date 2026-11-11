# Change version history

**Spec code:** `CVH`

## Intent
The version history of one OpenSpec change on a work stream, read from Git. A change lives in the stream's working copy and every settled revision of it reaches the stream branch as a commit: the propose run's commits, one commit for each closed edit proposal (per `CEP-change-edit-proposals.md` CEP-FR-FOJW), the apply run's ticks of `tasks.md`, and any commit the author makes by hand. So the branch already holds every version the author can want to read back, and this module keeps no second record of it. A **version** is a commit of the stream branch that changes a file of the change folder, and a version reads as the change folder held at that commit. It exists because a change is refined over many turns, and an author who accepts an edit must be able to see what the change said before and compare the two, as a draft's History does for a prompt (per `DHS-draft-history.md`). Out of scope: rendering the versions and the comparison, which is `../ui/OCW-openspec-change-workspace.md`'s; the commits themselves, which the modules that make them own; and the history of any file outside the change folder, which `HIS-history.md` serves.

## Functional requirements
1. **CVH-FR-HDTA** A **version** of a change is a commit reachable from the tip of the stream branch whose tree differs from the tree of its first parent under `openspec/changes/<change>/`. A root commit that holds the change folder is a version. A merge commit is a version where it differs from its first parent under that folder.
2. **CVH-FR-VZGR** `list_change_versions(stream_id, change)` returns every version of the change, most recent first, by the commit graph of the stream branch. Each version names its commit id, the first line of its message, its author name, its commit time, and the change-relative paths it adds, modifies, or deletes under the change folder.
3. **CVH-FR-URCF** A commit that renames a file inside the change folder lists the old path as deleted and the new path as added. A commit that moves the change folder to `openspec/changes/archive/` is a version that deletes every file of the change.
4. **CVH-FR-ITHO** The list reads the commits of the stream branch and nothing else. A commit on another branch, an uncommitted edit of the working copy, and the stash are not versions.
5. **CVH-FR-VDZZ** `read_change_version(stream_id, change, commit_id)` returns the files of the change folder as the tree of that commit holds them, ordered by change-relative path. A file whose bytes decode as UTF-8 carries its text. Any other file carries its path, its size in bytes, and a statement that it is not text, and no content.
6. **CVH-FR-VSQD** `read_change_version` refuses `unknown_version` where `commit_id` names no commit that `list_change_versions` would return for that change. A commit that holds no change folder is not a version, so a version always reads at least one file or is the version that deleted them all, which reads as no files.
7. **CVH-FR-HCOJ** The **current** state of a change is not a version. A surface that compares a version with the change as it stands reads the current state through `"read openspec change"` (per `OSM-openspec-mode.md` OSM-FR-MBTI) or through the working copy, and this module serves no read of it.
8. **CVH-FR-HXVV** A change folder that no longer exists in the working copy still has its versions. The list refuses `change_not_found` only where the branch holds no version of the change at all.
9. **CVH-FR-YDVP** Both reads refuse `unknown_stream` where the stream is not a live stream of the project (per `WKS-work-streams.md`), `invalid_change_name` where the name fails OSM-FR-IHUO, and `no_project_open` where no project is open.
10. **CVH-FR-DVTL** Both reads open the stream's working copy repository through `GTC-git.md`'s repository access and read Git objects alone. They write nothing to the repository, the index, or the working copy, and they take no lock that a commit takes.
11. **CVH-FR-DAAQ** The list reads at most 500 versions, newest first. A change with more versions returns the 500 newest and `truncated: true`.
12. **CVH-FR-NCKN** Each read emits through `LGC-logging.md` under the `backend` domain: a `DEBUG` record with the stream id, the change name, the count of versions or files, and the time taken, and a `WARN` record with the refusal code on every refusal. No record holds a commit message, an author name, or file text.
13. **CVH-FR-GHMO** Nothing in this module reaches the network. A read of a large repository runs on a blocking thread and never on the main thread.

## Contract surface

### Tauri commands
- `"list change versions"` → `list_change_versions(stream_id, change)` → `{ versions: ChangeVersion[], truncated: boolean }` (CVH-FR-VZGR, CVH-FR-DAAQ).
- `"read change version"` → `read_change_version(stream_id, change, commit_id)` → `{ files: ChangeVersionFile[] }` (CVH-FR-VDZZ).

Errors are typed: `no_project_open`, `unknown_stream`, `invalid_change_name`, `change_not_found`, `unknown_version`, and `read_failed`.

### Payload shapes
```
ChangeVersion {
  commit_id,            // full hexadecimal id
  summary,              // first line of the commit message
  author_name,
  committed_at,         // RFC 3339 UTC
  added: string[],      // change-relative paths
  modified: string[],
  deleted: string[]
}

ChangeVersionFile {
  path,                 // change-relative
  text?,                // present where the bytes decode as UTF-8
  bytes,                // size in bytes
  binary: boolean       // true where `text` is absent
}
```

### Internal (Rust API)
- `count_change_versions(stream_id, change)` — the number of versions, for `CST-change-statistics.md` (CST-FR-PUAN). It refuses on the terms of CVH-FR-YDVP.

## Non-functional requirements
- A list of a change with 100 versions on a repository of 10,000 commits answers in under one second on a machine that meets the application's floor.
- A read changes no state anywhere, so two surfaces may read the same version at once.

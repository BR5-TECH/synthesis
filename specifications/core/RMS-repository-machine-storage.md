# Repository machine storage

**Spec code:** `RMS`

## Intent
The per-machine home for the records this application keeps about a **repository** rather than about a checkout of one: the conversation logs of `CMS-comments-storage.md` and the draft statistics logs of `DSS-draft-statistics-storage.md`. These records are the application's own account of work, not the author's material, so they stand outside every project worktree and reach Git by no route. The store is selected by the **repository identity**, so all worktrees of one repository read and write one set of records and a change of active worktree selects nothing new. Out of scope: what those logs hold, how they fold, and when they are written, each of which belongs to the module that owns the log; the drafts, the notes, and every other project file, which stay in the active worktree; and the user-global settings store, which is `GSS-global-settings-storage.md`'s.

## Functional requirements
1. **RMS-FR-QJVT** The **repository identity** of an open project is the absolute path of the repository's primary worktree, resolved as `WTC-worktree-context.md` WTC-FR-02 resolves the repository. A project whose content root is in no Git repository takes that content root as its identity.
   - *Why:* The primary worktree is the one anchor every linked worktree of a repository shares, and it is already the key of the per-project slot (`GSS-global-settings-storage.md` GSS-FR-18).
2. **RMS-FR-BNKD** The identity is normalised before it is used: the path is made absolute and canonical, every `\` becomes `/`, and a trailing `/` is removed. A path that cannot be canonicalised is normalised as given.
3. **RMS-FR-ZXHM** The store directory of an identity is `app_data_dir()/repositories/<slug>-<digest>`, where `<digest>` is the first 32 lowercase hexadecimal characters of the SHA-256 of the normalised identity, and `<slug>` is the identity's final path segment reduced to at most 24 characters of `a-z`, `0-9`, `-`, and `_`. An empty reduction gives `repository`.
   - *Why:* The digest makes two identities collide only by a 128-bit accident, and the slug lets a person recognise the folder.
4. **RMS-FR-LPWG** One identity always selects one store directory, and two different identities always select two. The directory name carries no active worktree path and no branch name, so opening a second repository selects a different store and opening the same repository again selects the same one.
5. **RMS-FR-TCAF** Changing the project's active worktree (per `WTC-worktree-context.md` WTC-FR-08) selects the same store, the identity being the repository. Every conversation and every statistics total reads the same after the change as before it, and no record is copied, moved, or re-resolved.
6. **RMS-FR-MVDU** A store holds `repository.toml`, which records the normalised identity; `comments/`, the conversation logs and attachments of `CMS-comments-storage.md` (CMS-FR-37); `statistics/`, the per-draft logs of `DSS-draft-statistics-storage.md` (DSS-FR-KQVN); and `drafts/<draft-id>/comments/`, the conversation logs and attachments of one draft. This module owns none of their content.
7. **RMS-FR-KRYP** `repository_store()` creates the store directory and its `repository.toml` on the first call for an open project, and preserves an existing `repository.toml` unchanged. A read of a log the store does not hold answers as an empty log, exactly as a read of an absent log does.
8. **RMS-FR-WGQS** `repository_store()` returns a typed `store_unavailable` error where `app_data_dir()` cannot be resolved or the store directory cannot be created. It creates nothing, and it records the failure once per open project through `LGC-logging.md` at `WARN`.
9. **RMS-FR-HAJC** Every caller returns `store_unavailable` rather than an answer, a read as well as a write. `CMS-comments-storage.md` (CMS-FR-YQND) and `DSS-draft-statistics-storage.md` (DSS-FR-PWXG) each state their own refusal, and no operation of either module invents a record the store could not hold.
   - *Why:* An empty result reads exactly as a store holding nothing, so it would tell an author their conversations do not exist.
10. **RMS-FR-HDNZ** No path of a store is inside any worktree of any project. This module stages nothing, commits nothing, and writes no path under a `.git` directory, and a Git status of a project reports no path of a store.
11. **RMS-FR-PFOB** Several application processes may hold one store at the same time. Every write is an append of whole lines or an atomic whole-file write, so a reader never reads a part-written record, no process takes a lock, and no process is refused. A reader folds what is on disk at the moment it reads.
    - *Why:* A fold applies the first event it sees for an `event_id` and ignores every later one (per `CMS-comments-storage.md` CMS-FR-06), so a line two processes both append counts once.
12. **RMS-FR-JVEC** The **import pass** copies the records an earlier build committed into the project into the store. It reads `.synthesis/comments/`, `.synthesis/statistics/`, and each draft's `comments/` folder in the active worktree, and it writes each line into the matching log of the store and each attachment into the matching folder.
13. **RMS-FR-SUAK** The import pass is **additive and idempotent**. It appends a line only where the store's matching log holds no event with that `event_id`, and it writes an attachment only where the store holds no file of that digest. It writes no `.gitattributes`.
14. **RMS-FR-XRPT** The import pass **removes nothing and commits nothing**. It deletes, rewrites, moves, stages, and commits no path of the project, and it rewrites no Git history. A committed log the pass has read stays exactly as it is on disk and in the repository, and the author removes it in a commit of their own.
15. **RMS-FR-XLTR** Opening a project and changing the active worktree each schedule one import pass (per `PST-project-storage.md` PST-FR-MHZB). The pass runs in the background: it delays neither operation, a pass that fails is reported through `LGC-logging.md` alone, and a pass still running when the application quits is abandoned.
16. **RMS-FR-DGWY** Every read and write this module performs goes through `FSA-filesystem-access.md` primitives and is subject to the path-escape rejection of FSA-FR-10 with the store as the root. This module performs no raw `fs::write`, `fs::remove`, or `fs::rename` call, and it writes no file outside the store.
17. **RMS-FR-EYAM** Closing a project releases the store selection. A later command that names no open project returns the typed `no_project_open` of the module it belongs to, and this module resolves no store for a closed project.

## Contract surface
This module registers no Tauri command and is unreachable from the frontend. It exposes one internal Rust API to the backend modules that own the records it houses.

- `repository_identity()` → the normalised identity of the open project (RMS-FR-QJVT, RMS-FR-BNKD), or the typed `no_project_open` error (no UI consumer).
- `repository_store()` → the store directory of that identity, created where it does not exist (RMS-FR-ZXHM, RMS-FR-KRYP), or the typed `store_unavailable` error (RMS-FR-WGQS) (no UI consumer).
- `import_legacy_storage()` → runs one import pass for the open project's active worktree (RMS-FR-JVEC) and returns the count of lines and attachments it added (no UI consumer).

Typed errors: `no_project_open`, `store_unavailable`.

Store layout:

```
app_data_dir()/repositories/<slug>-<digest>/
  repository.toml                            // { identity }
  comments/                                  // CMS-comments-storage.md CMS-FR-37
    <log-id>.jsonl
    <log-id>.discussion.jsonl
    attachments/<digest>
    notes/<note-id>/discussion.jsonl
    notes/<note-id>/attachments/<digest>
  statistics/<draft-id>.jsonl                // DSS-draft-statistics-storage.md DSS-FR-KQVN
  drafts/<draft-id>/comments/
    <log-id>.jsonl
    discussion.jsonl
    attachments/<digest>
```

## Non-functional requirements
- The store holds no project file. A person who deletes a store loses the conversations and the statistics of that repository on that machine and loses no work of the project.
- Selecting a store costs one hash of a path. It reads no repository, walks no worktree, and reaches no network.
- The import pass costs one walk of the legacy folders the worktree holds. A project that never committed a conversation log or a statistics log has nothing for it to read, and a project whose legacy folders the author has removed has nothing for it to read again.
- The store is not synchronised between machines and is not backed up by the project's repository. A conversation held on one machine is read on that machine.

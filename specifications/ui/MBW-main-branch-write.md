# Main branch write confirmation

**Spec code:** `MBW`

## Intent
The application writes to a main branch only when the author says so. A main branch is the default branch of the repository or a branch that GitHub protects (per `../core/GTC-git.md` GTC-FR-TTSV). Work on a large project goes to a branch and reaches main through a pull request, so a write to main is a decision, not a side effect of editing or of a run. This surface asks for that decision once per branch and session, at the moment the author asks for a write and before anything is written. It never shows at a read: the author opens and reads drafts and artifacts on a main branch with no warning and no block. It never interrupts an autosave or a background commit. A branch that GitHub protects gets the same confirmation, with a line that GitHub may refuse a push. The author owns that choice, so the surface does not block the write. Out of scope: which branch is a main branch and how protection is read, which `../core/GTC-git.md` owns; the refusals of each write operation, which their own modules own; the pull of a branch, a rollback, and a discard, which never ask for the confirmation.

## Functional requirements
1. **MBW-FR-DBGA** The confirmation shows before a write, at the moment the author asks for it: **+ Draft** (per `DRP-drafts-panel.md` DRP-FR-MKCP), the first change of a file in an editor (per `EDT-editor.md` EDT-FR-QXEK), the accept of a change proposal (per `PCR-prompt-change-review.md` PCR-FR-AXPW), the creation of an artifact, the rename of a draft or an artifact, the deletion of a draft or an artifact, a commit, a push, a merge of a stream, and the start of a graduation or a direct run.
2. **MBW-FR-KDCM** The confirmation never shows at an autosave, a draft-event commit, or any other write that the author did not ask for at that moment. A run that started past the confirmation makes its later writes with no confirmation (per `../core/GRD-graduation.md` GRD-FR-MOWZ).
3. **MBW-FR-RMCO** Before a write, the surface reads `"get branch write guard (branch, refresh)"` for the branch the write lands on. When `is_main` is true and `confirmed` is false, the surface shows the confirmation and invokes no write. Otherwise it invokes the write with no confirmation.
4. **MBW-FR-YSMC** A write that the backend refuses with `main_branch_write_unconfirmed` shows the confirmation with the guard that the refusal carries. After the author confirms, the surface invokes the same write again, once. This holds also for a branch that GitHub protects.
5. **MBW-FR-RXAR** The confirmation names the branch, says why it is a main branch (the default branch, protected by GitHub, or both), and names the write the author asked for. When GitHub protects the branch, it adds the line of MBW-FR-JZME. It offers **Confirm write to `<branch>`** and **Cancel**.
6. **MBW-FR-XBJS** **Confirm** invokes `"confirm main branch write (branch)"`, and then the surface invokes the write. **Cancel**, Escape, and a click on the backdrop close the confirmation and invoke no write. A failed confirmation shows its error inline and invokes no write.
7. **MBW-FR-ILFN** After a confirmation, no further confirmation shows for that branch until the application restarts, on any surface (per `../core/GTC-git.md` GTC-FR-YWRZ). The confirmation is kept in memory only and is never saved to disk. A switch of the project does not clear it.
8. **MBW-FR-JZME** A branch whose protection is `protected` shows the same confirmation as another main branch, with the extra line: GitHub protects the branch and may refuse a push. **Confirm** stays available, and the author owns the choice. The confirmation offers **New worktree** as a secondary action, which opens the New worktree dialog (per `WTS-worktree-selector.md` WTS-FR-05). After a confirm, the guard reports the branch as confirmed, and the write runs as on any other main branch.
9. **MBW-FR-UNAN** When the protection of the default branch is `unknown`, the confirmation says that the protection could not be read, with its cause, and offers **Check again**, which reads the guard with `refresh` true. **Confirm** stays available. A guard read that fails shows the confirmation with the error and **Try again**, and invokes no write.
10. **MBW-FR-GYVD** The standalone confirmation is a floating overlay of the main window and is mutually exclusive with every other (per `SNV-shell-navigation.md` SNV-FR-56). It is modal, takes focus on **Cancel** when it opens, keeps focus inside itself, and returns focus to the control that opened it when it closes.
11. **MBW-FR-URIL** A write asked for inside an open overlay shows the confirmation as an inline section of it, and the overlay stays open: the commit message window (per `CMW-commit-message.md` CMW-FR-RMCO), the merge confirmation of the work stream selector (per `WSS-work-stream-selector.md` WSS-FR-DBGA), and the graduation start dialog (per `GSD-graduation-start-dialog.md` GSD-FR-YSMC). The merge-or-rebase dialog of a pull has no inline section (per `GIT-git.md` GIT-FR-RCDL).
12. **MBW-FR-IOGF** The inline section holds the text and the actions of MBW-FR-RXAR, MBW-FR-JZME, and MBW-FR-UNAN. Until the author confirms, the primary action of the overlay is unavailable, and the section is its accessible description.
13. **MBW-FR-WCBQ** The confirmation is operable by keyboard alone. The branch name, the reason, and each error are in the accessible semantics, and an inline section is announced when it appears.
14. **MBW-FR-PJTG** The surface logs each confirmation shown, confirmed, and cancelled, with the branch name and the kind of write, and no file content.
15. **MBW-FR-QKVA** Opening, viewing, or listing a draft or an artifact never shows a confirmation or a block, also on a main branch and also on a branch that GitHub protects. Opening the next draft or artifact shows none either. The guard that an editing surface reads when it opens a file (per `EDT-editor.md` EDT-FR-QXEK) shows no interface. The confirmation shows at the first change of the author only.
16. **MBW-FR-HXTE** The confirmation never shows for a pull of any kind, that is a plain pull and the merge or rebase that joins a diverged branch with its upstream, for a rollback, or for a discard. The author decided that a rollback and a discard are not confirmed. The pull dialog holds no inline confirmation section.
17. **MBW-FR-ZLPC** A push that GitHub refuses on a confirmed protected branch shows the reason of GitHub in words, in the action that failed. The surface does not replace the reason with the text of the confirmation.
18. **MBW-FR-UQXR** An inline section that appears inside a scrolled container, such as the stream list, scrolls into view in that container with its actions, so Confirm and Cancel are visible without a scroll by the author.

## Wireframes

```
┌ Write to main? ────────────────────────────────────┐
│ main is the default branch of this repository.     │
│ You asked to create a draft. The draft is          │
│ committed to main.                                 │
│                                                    │
│ ⓘ The branch protection could not be read:         │
│   GitHub unreachable.            [ Check again ]   │
│                                                    │
│                 [ Cancel ] [ Confirm write to main ]│
└────────────────────────────────────────────────────┘
```

The confirmation of a branch that GitHub protects:

```
┌ Write to main? ────────────────────────────────────┐
│ main is the default branch of this repository, and │
│ GitHub protects it.                                │
│ You asked to push main.                            │
│                                                    │
│ ⓘ GitHub protects main and may refuse a push.      │
│                                                    │
│  [ New worktree ]  [ Cancel ] [ Confirm write to main ]│
└────────────────────────────────────────────────────┘
```

- Layout notes: the confirmation uses the frame of the other modal windows (per `CMW-commit-message.md`). The inline section inside another overlay stands above that overlay's action row and reads as a warning. A long branch name truncates from the middle and keeps its whole value in accessible semantics.

## UI contract boundary

**Owned by the UI**: when the confirmation shows, its wording, the inline section, the line for a protected branch and its route to a new worktree, and the second invocation of a write after a confirmation.

**Delegated to backend (abstract)**:
- `"get branch write guard (branch, refresh)"` — owned by `../core/GTC-git.md`.
- `"confirm main branch write (branch)"` — owned by `../core/GTC-git.md`.

The write operations themselves stay with the surfaces that invoke them. Each refuses an unconfirmed write on its own (per `../core/GTC-git.md` GTC-FR-JWOL), so a surface that does not read the guard first still never writes to main without a confirmation.

## Non-functional requirements
- A guard read for a branch that holds a confirmation adds no visible delay to the write.
- No text of the confirmation holds a token or file content.

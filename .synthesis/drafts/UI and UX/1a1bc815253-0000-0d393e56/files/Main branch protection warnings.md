## Intent

The main branch warning is too intrusive. Today the user sees it when they only open a draft to read it, and again when they open the next draft. This is a defect: reading never shows a warning or a block (MBW-FR-KDCM already says this).

The warning must show only before a write that the user asks for, on a main branch. After the user accepts the risk for a branch, the app does not ask again for that branch until the app restarts. The choice stays in memory only and is never saved to disk. A branch that GitHub protects is no longer a hard block. The user may continue, and the user owns that choice.

This draft replaces the hard-block rule of the earlier "Git Flow" draft and of MBW-FR-JZME.

## User journey

- The user opens a draft or an artifact on a main branch, one after the other. Nothing asks for confirmation. The user reads freely.
- The user accepts a change proposal on a draft, creates an artifact, renames or deletes a draft or an artifact, or does any other write of the list below. The confirmation shows once, with the branch name and the action.
- The user presses **Confirm write to `<branch>`**. The write runs. Later writes to the same branch, on any surface, run with no confirmation until the app restarts.
- The user presses **Cancel**. Nothing is written.
- The branch is protected by GitHub. The confirmation says that GitHub may refuse a push. The user may still confirm. Local writes work. A push that GitHub refuses shows the real GitHub error on the action that failed.
- The user pulls on a main branch, also a pull that needs a merge or rebase. No confirmation shows.

## Requirements

- **No warning on read.** Opening, viewing or listing a draft or an artifact never shows a confirmation or a block. Find the read path that shows it today (candidates: the hard block of MBW-FR-JZME shown on open, or a read path that writes and gets `main_branch_write_unconfirmed`) and fix it. The guard read when a file opens (EDT-FR-QXEK, EDT-FR-UPNU) must not show UI. The confirmation shows at the first change only.
- **Writes that confirm on a main branch:** the existing MBW-FR-DBGA list (+ Draft, first editor change, commit, push, stream merge, draft deletion, graduation start, direct run) plus: accept of a change proposal (whole-file write), artifact creation, artifact deletion, and rename of a draft or an artifact.
- **Never confirm:** pull of any kind (a plain pull and a merge or rebase reconcile), rollback, discard, autosave, draft-event commit. Record the rollback and discard exclusion as an explicit decision of the author.
- **Once per branch per session.** One confirmation per branch, in memory only, kept until the app restarts. A project switch does not clear it. It holds on every surface (MBW-FR-ILFN, GTC-FR-YWRZ).
- **Protected branch.** Show the same confirmation, with a line that GitHub protects the branch and may refuse a push. Keep **New worktree** as a secondary action. The hard block is removed. The backend stops refusing with `main_branch_protected` and uses the unconfirmed path. After confirm, the guard reports the protected branch as confirmed.
- **Backend enforcement.** Each new write command (accept change proposal, create, rename, delete artifact) runs the same guard check as the other writes (GTC-FR-JWOL). The UI check alone is not enough. Pull and reconcile commands must not refuse on an unconfirmed main branch.
- **Unknown protection.** The **Check again** flow of MBW-FR-UNAN stays as it is.
- **Errors.** A push that GitHub refuses on a confirmed protected branch shows the GitHub reason in words, in the action that failed.
- **Keep behaviour.** Keyboard operation, focus rules, logging and no file content in the text stay as MBW says.

## Expected deliverables

- Frontend: no confirmation on read paths; confirmation before the new writes; the protected-branch confirmation with the GitHub line; the pull dialog has no inline confirmation section.
- Backend: the new write commands call the guard check; protected branches use the unconfirmed and confirm flow; pull and reconcile have no guard check.
- Tests: opening several drafts and artifacts on a main branch shows no confirmation; each write of the list confirms once per branch; no pull confirms; a protected branch confirms and then a push refusal shows the GitHub error.
- Specifications to update: `specifications/ui/MBW-main-branch-write.md` (Intent, MBW-FR-DBGA, MBW-FR-YSMC, MBW-FR-RXAR, MBW-FR-JZME, MBW-FR-UNAN, MBW-FR-URIL, MBW-FR-IOGF, MBW-FR-PJTG, the hard-block wireframe, Out of scope); `specifications/core/GTC-git.md` (GTC-FR-YWRZ, GTC-FR-JWOL, GTC-FR-LRNB, GTC-FR-EMLM, the `BranchWriteGuard` doc, the `main_branch_protected` refusal); `specifications/ui/WSS-work-stream-selector.md` (WSS-FR-DBGA); `specifications/core/WKS-work-streams.md` (WKS-FR-WUUH); `specifications/ui/GIT-git.md` (GIT-FR-RCDL); the inline sections of EDT, CMW and GSD that mention the hard block; the specs of draft creation (DRP-FR-MKCP), artifact creation and change-proposal accept (PCR-FR-AXPW).
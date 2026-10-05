## Intent
Revert the decision to store AI communications and draft statistics in the project Git history. Complete conversation logs and draft statistics must remain available to the application, but must be stored in per-machine application data outside all project worktrees. Draft files and their existing Git behavior are unchanged: drafts remain active-worktree files and continue to use the existing draft storage rules.

## Requirements

- Complete project conversation logs, including human and AI messages in the same threads, must not be stored in the project worktree or Git history.
- Draft statistics must not be stored in the project worktree or Git history.
- Conversation logs and draft statistics must be stored in per-machine application data, outside every project worktree.
- The storage identity for conversations and statistics must be the Git repository identity, not the active worktree. All worktrees of one repository must read and write the same conversation and statistics data on that machine.
- Switching the active worktree must not change the conversation or statistics store selected for the open repository.
- Opening another repository must select a different store. The implementation must define the repository identity and the safe, collision-resistant mapping from that identity to the application-data path.
- Draft storage must remain scoped to the active worktree. This change must not make drafts repository-wide and must not move draft files out of the worktree.
- Remove automatic Git commit behavior for conversation logs and draft statistics. Do not stage, commit, or modify any path for these stores in the project repository.
- Existing committed conversation logs and statistics require an explicit migration plan. The migration must define whether existing data is imported into the per-machine store, removed from Git only in a user-authored commit, or both. It must not silently rewrite Git history.
- Update all affected specifications and project lifecycle behavior, including `CMS-comments-storage.md`, `DSS-draft-statistics-storage.md`, `PST-project-storage.md`, and any repository/worktree identity or application-data storage specification that owns the new location.
- Preserve the existing user-visible conversation and statistics behavior where possible, including reads, writes, events, deletion with a draft where applicable, and behavior after worktree switches. Define the behavior when the per-machine store is unavailable or when the same repository is opened concurrently by multiple application processes.

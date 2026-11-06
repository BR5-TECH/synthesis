/**
 * A request to open the New worktree dialog of the worktree selector
 * (`WTS-worktree-selector.md` WTS-FR-05) from outside the selector: the hard
 * block of a protected branch routes there (`MBW-main-branch-write.md`
 * MBW-FR-JZME).
 */
const listeners = new Set<() => void>();

/** The selector listens while it is mounted. */
export function onNewWorktreeRequested(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** Ask the selector to open its New worktree dialog. */
export function requestNewWorktree(): void {
  for (const listener of listeners) listener();
}

export function resetNewWorktreeRequestsForTest(): void {
  listeners.clear();
}

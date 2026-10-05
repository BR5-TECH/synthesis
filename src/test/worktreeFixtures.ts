/**
 * The worktree and branch shapes every `WorktreeSelector` test builds from
 * (`../../specifications/ui/WTS-worktree-selector.md`).
 *
 * Shared rather than copied, on the model of `streamFixtures.ts`: more than one
 * test file renders the same selector, and a listing that drifted between them
 * would let one of them pass against a shape the backend never sends.
 */

import type { WorktreeContext, WorktreeEntry } from "../types";

export const REPO = "/dev/acme-platform";

export function wt(
  over: Partial<WorktreeEntry> & { path: string },
): WorktreeEntry {
  return {
    name: over.path.split("/").pop() ?? over.path,
    headShortHash: "4f2a10c",
    isDetached: false,
    isActive: false,
    isPrimary: false,
    isMissing: false,
    ...over,
  };
}

/** The repository's own checkout — where branches may be checked out. */
export const ACTIVE = wt({
  path: REPO,
  branch: "feature/new-window",
  isActive: true,
  isPrimary: true,
});

/** A linked worktree, where no branch may be checked out (WTS-FR-12). */
export const LINKED = wt({
  path: "/dev/acme-platform-main",
  branch: "main",
  isActive: true,
  isPrimary: false,
});

/** The listing the mocked backend answers with, unless a test replaces it. */
export function defaultWorktreeContext(): WorktreeContext {
  return {
    repositoryRoot: REPO,
    activeWorktreePath: REPO,
    worktrees: [
      ACTIVE,
      wt({ path: "/dev/acme-platform-main", branch: "main" }),
      wt({
        path: "/dev/acme-platform-spike",
        isDetached: true,
        headShortHash: "4f2a10c",
      }),
      wt({
        path: "/dev/acme-platform-release",
        branch: "release/2.1",
        isMissing: true,
      }),
    ],
    branches: [
      { name: "chore/deps", kind: "local", headShortHash: "aaa1111" },
      { name: "fix/editor-scroll", kind: "local", headShortHash: "bbb2222" },
      { name: "origin/experiment", kind: "remote", headShortHash: "ccc3333" },
    ],
  };
}

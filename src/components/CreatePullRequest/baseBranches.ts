import type { GitBranch } from "../../types";

/**
 * CPR-FR-FZHF: the base branch names the window offers, as GitHub knows them.
 *
 * A remote-tracking entry names its remote (`origin/main`), which GitHub does
 * not, so the remote is dropped. The head branch is never offered, and each
 * name appears once.
 */
export function baseBranchNames(branches: GitBranch[], head: string): string[] {
  const names = new Set<string>();
  for (const branch of branches) {
    const name =
      branch.kind === "remote"
        ? branch.name.slice(branch.name.indexOf("/") + 1)
        : branch.name;
    if (name === "" || name === "HEAD" || name === head) continue;
    names.add(name);
  }
  return [...names].sort((a, b) => a.localeCompare(b));
}

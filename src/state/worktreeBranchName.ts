/**
 * WTS-FR-UKSX: the Branch field of the New worktree dialog, normalized as the
 * user types, by the rule of `WTC-worktree-context.md` WTC-FR-XLXY.
 *
 * The value is lower case and holds only `a-z`, `0-9`, `_`, `-` and `/`. Every
 * other character becomes `-`, a run of `-` or of `/` becomes one, and no `-`
 * stands beside an inner `/`. A `-` or `/` at an end of the value stays, so
 * that the next keystroke can continue the name: the backend removes them when
 * it creates the branch.
 */
export function normalizeBranchInput(raw: string): string {
  const segments = raw
    .replace(/[A-Z]/g, (ch) => ch.toLowerCase())
    .replace(/[^a-z0-9_/-]/g, "-")
    .replace(/-+/g, "-")
    .replace(/\/+/g, "/")
    .split("/");
  const last = segments.length - 1;
  return segments
    .map((segment, i) => {
      let kept = segment;
      if (i > 0) kept = kept.replace(/^-/, "");
      if (i < last) kept = kept.replace(/-$/, "");
      return kept;
    })
    .join("/")
    .replace(/\/+/g, "/");
}

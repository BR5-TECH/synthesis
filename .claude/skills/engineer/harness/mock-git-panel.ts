/*
 * Git panel stand-in (GIT-git.md, GTC-git.md), for UI auditing only.
 *
 * Serves the commit history, branch information, branch deletion and pull
 * request commands with canned data. State a flag can reach, on the URL:
 *   ?prsFail         pull request reads fail with `github_unreachable`
 *   ?prsNoToken      pull request reads fail with `github_token_missing`
 *   ?prsSelectToken  pull request reads fail with the selection-required error
 *   ?prsTruncated    the timeline is marked truncated
 *   ?noHistory       the repository has no commit
 *   ?detachedHead    HEAD is detached
 * Branch deletion: `main` is the primary worktree's branch, `feature/new-
 * artifact-window` is the active one, `feature/stale-work` holds uncommitted
 * paths, `develop` has a remote branch, and `synthesis/stream/demo` belongs to
 * a work stream.
 */

import { NOT_HANDLED } from "./mock-github-polling";

const flag = (name: string) => new URLSearchParams(location.search).has(name);

type Branch = { name: string; kind: "local" | "remote"; isCurrent: boolean };

const branches: Branch[] = [
  { name: "main", kind: "local", isCurrent: false },
  { name: "feature/new-artifact-window", kind: "local", isCurrent: true },
  { name: "develop", kind: "local", isCurrent: false },
  { name: "feature/stale-work", kind: "local", isCurrent: false },
  { name: "feature/old-experiment", kind: "local", isCurrent: false },
  { name: "synthesis/stream/demo", kind: "local", isCurrent: false },
  { name: "origin/main", kind: "remote", isCurrent: false },
  { name: "origin/develop", kind: "remote", isCurrent: false },
  { name: "origin/feature/old-experiment", kind: "remote", isCurrent: false },
];

const SUBJECTS = [
  "tighten editor toolbar token usage",
  "spec: flow editor rules",
  "add settings sections placeholder",
  "fix tab strip overflow at narrow widths",
  "initial synthesis scaffold",
];

function commit(i: number) {
  const hex = (0xa1b2c30 + i * 7919).toString(16).padStart(7, "0").slice(0, 7);
  const subject = `${SUBJECTS[i % SUBJECTS.length]} (${i})`;
  return {
    id: hex + "d".repeat(33),
    shortId: hex,
    authorName: i % 3 === 0 ? "Kira Volkova" : "Mara Quinn",
    authorEmail: i % 3 === 0 ? "kira@example.com" : "mara@example.com",
    authoredAt: 1_785_000_000 - i * 5400,
    subject,
    message: `${subject}\n\nA longer explanation of commit ${i}, wrapped in the body.`,
    refs: i === 0 ? ["feature/new-artifact-window", "origin/feature/new-artifact-window"] : i === 4 ? ["main"] : [],
  };
}

const commits = Array.from({ length: 100 }, (_, i) => commit(i));

const FILES = [
  { path: "README.md", status: "modified", isBinary: false },
  { path: "specifications/ui/14-editor.md", status: "modified", isBinary: false },
  { path: "specifications/ui/GIT-git.md", status: "added", isBinary: false },
  { path: "src/components/Git.tsx", status: "modified", isBinary: false },
  { path: "src/components/Git/GitLog.tsx", status: "added", isBinary: false },
  { path: "src/old/Legacy.tsx", status: "deleted", isBinary: false },
  {
    path: "docs/guide.md",
    previousPath: "docs/old-guide.md",
    status: "renamed",
    isBinary: false,
  },
  { path: "assets/logo.png", status: "added", isBinary: true },
];

const DIFF = {
  isBinary: false,
  hunks: [
    {
      header: "@@ -12,5 +12,6 @@ ## Functional requirements",
      lines: [
        { kind: "context", oldLineno: 12, newLineno: 12, content: "## Functional requirements" },
        { kind: "context", oldLineno: 13, newLineno: 13, content: "" },
        { kind: "del", oldLineno: 14, content: "1. **FR-1** Editor is a tab in the main viewport." },
        { kind: "add", newLineno: 14, content: "1. **FR-1** Editor is a tab in the main viewport opened when an artifact is selected." },
        { kind: "context", oldLineno: 15, newLineno: 15, content: "2. **FR-2** Editor renders Markdown content as WYSIWYG." },
        { kind: "add", newLineno: 16, content: "3. **FR-3** All v1 Markdown artifact types share the same Editor component." },
      ],
    },
  ],
};

const WORKTREE = (path: string, name: string, isActive: boolean, isPrimary: boolean) => ({
  path,
  name,
  isActive,
  isPrimary,
});

function prError(): string | null {
  if (flag("prsFail")) return "github_unreachable";
  if (flag("prsNoToken")) return "github_token_missing";
  if (flag("prsSelectToken")) return "github_token_selection_required";
  return null;
}

const OPEN_PRS = [
  { number: 142, title: "Tighten editor toolbar", state: "open", isDraft: false, author: "kira", headBranch: "feature/toolbar", baseBranch: "main" },
  { number: 139, title: "Add the Git panel history views", state: "open", isDraft: true, author: "mara", headBranch: "feature/git-panel", baseBranch: "main" },
  { number: 131, title: "Docs: explain worktrees", state: "open", isDraft: false, author: "ivan", headBranch: "docs/worktrees", baseBranch: "develop" },
].map((p) => ({ ...p, createdAt: "2026-09-20T09:00:00Z", updatedAt: "2026-09-28T16:30:00Z" }));

const CLOSED_PRS = [
  { number: 120, title: "Fix tab strip overflow", state: "merged", isDraft: false, author: "kira", headBranch: "fix/tabs", baseBranch: "main" },
  { number: 118, title: "Try a new sidebar", state: "closed", isDraft: false, author: "mara", headBranch: "try/sidebar", baseBranch: "main" },
].map((p) => ({ ...p, createdAt: "2026-09-01T09:00:00Z", updatedAt: "2026-09-10T11:00:00Z" }));

function pullRequestDetail(id: number) {
  const row = [...OPEN_PRS, ...CLOSED_PRS].find((p) => p.number === id);
  // eslint-disable-next-line no-throw-literal
  if (!row) throw "pull_request_not_found";
  return {
    ...row,
    url: `https://github.com/acme/platform/pull/${id}`,
    body:
      "This change tightens the **editor toolbar**.\n\n- Uses design tokens only\n- Keeps `aria-label` on every control\n\nSee the [spec](https://example.com/spec) for the rules.",
    closedAt: row.state === "open" ? undefined : "2026-09-10T11:00:00Z",
    mergedAt: row.state === "merged" ? "2026-09-10T11:00:00Z" : undefined,
  };
}

function timeline() {
  return {
    truncated: flag("prsTruncated"),
    items: [
      { id: "c1", kind: "comment", actor: "mara", createdAt: "2026-09-20T10:00:00Z", body: "Looks **good** overall. One question about the spacing." },
      { id: "k1", kind: "commit", actor: "kira", createdAt: "2026-09-21T08:00:00Z", commitId: "7e3f1a2b9c0d", subject: "Use spacing tokens in the toolbar" },
      { id: "r1", kind: "review", actor: "ivan", createdAt: "2026-09-22T12:00:00Z", reviewState: "changes_requested", body: "Please add a test for the overflow menu." },
      { id: "rc1", kind: "review_comment", actor: "ivan", createdAt: "2026-09-22T12:01:00Z", path: "src/components/Toolbar.tsx", body: "This branch needs a `Escape` handler." },
      { id: "e1", kind: "event", actor: "kira", createdAt: "2026-09-23T09:00:00Z", event: "ready_for_review" },
      { id: "r2", kind: "review", actor: "ivan", createdAt: "2026-09-24T09:30:00Z", reviewState: "approved" },
    ],
  };
}

export function gitPanelInvoke(cmd: string, a: Record<string, any>): unknown {
  switch (cmd) {
    case "list_branches":
      return branches.map((b) => ({ ...b }));

    case "list_commit_history": {
      if (flag("noHistory")) return { isDetached: false, branch: "main", commits: [] };
      if (flag("detachedHead"))
        return { isDetached: true, headId: commits[0].id, commits };
      return { branch: "feature/new-artifact-window", isDetached: false, headId: commits[0].id, commits };
    }
    case "list_commit_files": {
      // eslint-disable-next-line no-throw-literal
      if (!commits.some((c) => c.id === a.commitId)) throw "unknown_commit";
      return FILES;
    }
    case "get_commit_file_diff": {
      // eslint-disable-next-line no-throw-literal
      if (!commits.some((c) => c.id === a.commitId)) throw "unknown_commit";
      const file = FILES.find((f) => f.path === a.path);
      // eslint-disable-next-line no-throw-literal
      if (!file) throw "path_not_in_commit";
      return file.isBinary ? { isBinary: true, hunks: [] } : DIFF;
    }

    // GTC-FR-YQVD / GTC-FR-MBBH: the stream's branch grows from
    // `feature/new-artifact-window`, every other branch from `main`, and `main`
    // is its own base.
    case "list_branch_compare_files": {
      const branch = branches.find((b) => b.name === a.name && b.kind === a.kind);
      // eslint-disable-next-line no-throw-literal
      if (!branch) throw "unknown branch";
      const base =
        branch.name === "synthesis/stream/demo" ? "feature/new-artifact-window" : "main";
      if (branch.kind === "local" && branch.name === base)
        return { branch: branch.name, kind: branch.kind, base, sameAsBase: true, files: [] };
      return {
        branch: branch.name,
        kind: branch.kind,
        base,
        mergeBase: commits[3].id,
        sameAsBase: false,
        files: FILES,
      };
    }
    case "get_branch_compare_file_diff": {
      // eslint-disable-next-line no-throw-literal
      if (!branches.some((b) => b.name === a.name && b.kind === a.kind)) throw "unknown branch";
      const file = FILES.find((f) => f.path === a.path);
      // eslint-disable-next-line no-throw-literal
      if (!file) throw "path_not_in_comparison";
      return file.isBinary ? { isBinary: true, hunks: [] } : DIFF;
    }

    case "get_branch_information": {
      const branch = branches.find((b) => b.name === a.name && b.kind === a.kind);
      // eslint-disable-next-line no-throw-literal
      if (!branch) throw "unknown branch";
      return {
        name: branch.name,
        kind: branch.kind,
        upstream: branch.kind === "local" && branch.name !== "feature/stale-work" ? `origin/${branch.name}` : undefined,
        isCurrent: branch.isCurrent,
        worktree:
          branch.name === "main"
            ? WORKTREE("/Users/demo/dev/acme", "acme", false, true)
            : branch.isCurrent
              ? WORKTREE("/Users/demo/dev/acme-window", "acme-window", true, false)
              : branch.name === "feature/stale-work"
                ? WORKTREE("/Users/demo/dev/acme-stale", "acme-stale", false, false)
                : undefined,
        stream:
          branch.name === "synthesis/stream/demo"
            ? { streamId: "s-demo", streamName: "demo" }
            : undefined,
        tip: commits[1],
        commits: commits.slice(1, 21),
      };
    }

    case "inspect_branch_deletion": {
      const name = String(a.name ?? "");
      // eslint-disable-next-line no-throw-literal
      if (name === "main") throw "branch_in_primary_worktree";
      // eslint-disable-next-line no-throw-literal
      if (name === "feature/new-artifact-window") throw "branch_in_active_worktree";
      if (!branches.some((b) => b.name === name && b.kind === "local"))
        // eslint-disable-next-line no-throw-literal
        throw "unknown branch";
      const worktree =
        name === "feature/stale-work"
          ? WORKTREE("/Users/demo/dev/acme-stale", "acme-stale", false, false)
          : name === "synthesis/stream/demo"
            ? WORKTREE("/Users/demo/.synthesis/streams/demo", "demo", false, false)
            : undefined;
      return {
        branch: name,
        worktree,
        stream:
          name === "synthesis/stream/demo"
            ? { streamId: "s-demo", streamName: "demo", busyRunId: null, aheadOfBase: 6 }
            : undefined,
        uncommittedPaths:
          name === "feature/stale-work"
            ? ["src/a.ts", "src/b.ts", "src/c.ts", "docs/d.md", "docs/e.md", "docs/f.md", "README.md"]
            : [],
        remoteBranch: branches.some((b) => b.kind === "remote" && b.name === `origin/${name}`)
          ? `origin/${name}`
          : undefined,
      };
    }
    case "delete_branch": {
      const name = String(a.name ?? "");
      const at = branches.findIndex((b) => b.name === name && b.kind === "local");
      // eslint-disable-next-line no-throw-literal
      if (at < 0) throw "unknown branch";
      // eslint-disable-next-line no-throw-literal
      if (name === "feature/stale-work" && !a.discardUncommitted)
        throw "worktree_dirty: src/a.ts, src/b.ts, src/c.ts, docs/d.md, docs/e.md, docs/f.md, README.md, src/new.ts";
      branches.splice(at, 1);
      const remoteAt = branches.findIndex((b) => b.name === `origin/${name}`);
      const hasRemote = remoteAt >= 0;
      if (a.deleteRemote && hasRemote) branches.splice(remoteAt, 1);
      queueMicrotask(() => {
        const fire = (globalThis as Record<string, unknown>).__fireBusEvent as
          | ((name: string, payload?: unknown) => void)
          | undefined;
        fire?.("branches-changed", { repositoryRoot: "/Users/demo/dev/acme" });
      });
      return {
        branch: name,
        removedWorktreePath:
          name === "feature/stale-work" ? "/Users/demo/dev/acme-stale" : undefined,
        remote: {
          requested: Boolean(a.deleteRemote),
          state: a.deleteRemote && hasRemote ? "deleted" : "not_requested",
          branch: a.deleteRemote && hasRemote ? `origin/${name}` : undefined,
        },
      };
    }
    case "delete_work_stream": {
      // The demo stream's branch goes with it; any other stream is the core
      // mock's to answer.
      if (a.streamId !== "s-demo") return NOT_HANDLED;
      // WKS-FR-EIBC: the demo stream holds 6 commits its base does not.
      // eslint-disable-next-line no-throw-literal
      if (!a.force) throw "stream_unmerged: 6";
      const at = branches.findIndex((b) => b.name === "synthesis/stream/demo");
      if (at >= 0) branches.splice(at, 1);
      return null;
    }
    case "get_work_stream_uncommitted_paths":
      return ["src/scratch.ts"];

    case "list_pull_requests": {
      const failure = prError();
      if (failure) throw failure; // eslint-disable-line no-throw-literal
      if (a.state === "closed") return CLOSED_PRS;
      return flag("noPrs") ? [] : OPEN_PRS;
    }
    case "get_pull_request_detail": {
      const failure = prError();
      if (failure) throw failure; // eslint-disable-line no-throw-literal
      return pullRequestDetail(Number(a.id));
    }
    case "list_pull_request_timeline": {
      const failure = prError();
      if (failure) throw failure; // eslint-disable-line no-throw-literal
      return timeline();
    }
    default:
      return NOT_HANDLED;
  }
}

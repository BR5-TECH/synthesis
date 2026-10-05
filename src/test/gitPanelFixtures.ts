import type {
  BranchDeletionPlan,
  BranchInformation,
  CommitFile,
  CommitHistory,
  CommitSummary,
  DiffPayload,
  PullRequestDetail,
  PullRequestSummary,
  PullRequestTimeline,
} from "../types";

/** One commit, with the index spread into every field so rows are distinguishable. */
export function commit(i: number, over: Partial<CommitSummary> = {}): CommitSummary {
  const hex = i.toString(16).padStart(7, "0");
  return {
    id: `${hex}${"a".repeat(33)}`,
    shortId: hex,
    authorName: `Author ${i}`,
    authorEmail: `author${i}@example.com`,
    authoredAt: 1_760_000_000 - i * 3600,
    subject: `Subject of commit ${i}`,
    message: `Subject of commit ${i}\n\nBody of commit ${i}.`,
    refs: [],
    ...over,
  };
}

/** `count` commits, newest first, on `branch`. */
export function history(count: number, over: Partial<CommitHistory> = {}): CommitHistory {
  return {
    branch: "main",
    isDetached: false,
    headId: commit(0).id,
    commits: Array.from({ length: count }, (_, i) => commit(i)),
    ...over,
  };
}

export const COMMIT_FILES: CommitFile[] = [
  { path: "README.md", status: "modified", isBinary: false },
  { path: "src/app/main.ts", status: "added", isBinary: false },
  { path: "src/app/util.ts", status: "deleted", isBinary: false },
  { path: "docs/logo.png", status: "added", isBinary: true },
  {
    path: "docs/guide.md",
    previousPath: "docs/old-guide.md",
    status: "renamed",
    isBinary: false,
  },
];

export const COMMIT_DIFF: DiffPayload = {
  isBinary: false,
  hunks: [
    {
      header: "@@ -1,2 +1,2 @@",
      lines: [
        { kind: "context", oldLineno: 1, newLineno: 1, content: "# Title" },
        { kind: "del", oldLineno: 2, content: "old line" },
        { kind: "add", newLineno: 2, content: "new line" },
      ],
    },
  ],
};

export function branchInformation(over: Partial<BranchInformation> = {}): BranchInformation {
  return {
    name: "feature/x",
    kind: "local",
    upstream: "origin/feature/x",
    isCurrent: false,
    worktree: {
      path: "/work/feature-x",
      name: "feature-x",
      isActive: false,
      isPrimary: false,
    },
    tip: commit(1),
    commits: [commit(1), commit(2)],
    ...over,
  };
}

export function deletionPlan(over: Partial<BranchDeletionPlan> = {}): BranchDeletionPlan {
  return {
    branch: "feature/x",
    worktree: {
      path: "/work/feature-x",
      name: "feature-x",
      isActive: false,
      isPrimary: false,
    },
    uncommittedPaths: [],
    ...over,
  };
}

export function pullRequest(
  n: number,
  over: Partial<PullRequestSummary> = {},
): PullRequestSummary {
  return {
    number: n,
    title: `Pull request ${n}`,
    state: "open",
    isDraft: false,
    author: `dev${n}`,
    headBranch: `topic-${n}`,
    baseBranch: "main",
    createdAt: "2026-09-01T10:00:00Z",
    updatedAt: "2026-09-02T10:00:00Z",
    ...over,
  };
}

export function pullRequestDetail(
  n: number,
  over: Partial<PullRequestDetail> = {},
): PullRequestDetail {
  return {
    ...pullRequest(n),
    body: `Description of **pull request ${n}**.`,
    url: `https://github.com/acme/app/pull/${n}`,
    ...over,
  };
}

export const FULL_TIMELINE: PullRequestTimeline = {
  truncated: false,
  items: [
    {
      id: "c1",
      kind: "comment",
      actor: "ann",
      createdAt: "2026-09-01T11:00:00Z",
      body: "A plain **comment** body.",
    },
    {
      id: "r1",
      kind: "review",
      actor: "bob",
      createdAt: "2026-09-01T12:00:00Z",
      reviewState: "changes_requested",
      body: "Please change this.",
    },
    {
      id: "rc1",
      kind: "review_comment",
      actor: "bob",
      createdAt: "2026-09-01T12:01:00Z",
      path: "src/lib.rs",
      body: "This line needs a test.",
    },
    {
      id: "k1",
      kind: "commit",
      actor: "ann",
      createdAt: "2026-09-01T13:00:00Z",
      commitId: "abcdef1234567890",
      subject: "Add the missing test",
    },
    {
      id: "e1",
      kind: "event",
      actor: "ann",
      createdAt: "2026-09-01T14:00:00Z",
      event: "ready_for_review",
    },
  ],
};

/** A promise the test settles by hand, to put two responses out of order. */
export function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (error: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

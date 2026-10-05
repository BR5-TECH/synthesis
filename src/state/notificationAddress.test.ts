import { describe, expect, it } from "vitest";

import {
  BOTTOM_TARGETS,
  PANEL_TARGETS,
  describeTarget,
  mintAddress,
  parseAddress,
  sameRoot,
  sameTarget,
  type NotificationTarget,
} from "./notificationAddress";

/**
 * The `synthesis://` address space (`NTF-notifications.md` NTF-FR-03 through
 * NTF-FR-06), covering NTF-FR-04, NTF-FR-19, NTF-FR-05, and the grammar half of NTF-FR-03, NTF-FR-17.
 */

const PROJECT = "/Users/me/dev/acme";
const WORKTREE = "/Users/me/dev/acme-main";

const roundTrip = (target: NotificationTarget) =>
  parseAddress(mintAddress(PROJECT, WORKTREE, target));

describe("the address grammar (NTF-FR-03)", () => {
  it("round-trips every target form the grammar admits", () => {
    // NTF-FR-03, NTF-FR-17: the Dashboard, a file, a draft, every vertical-panel surface,
    // every bottom-panel surface, and both settings tabs.
    const targets: NotificationTarget[] = [
      { kind: "dashboard" },
      { kind: "file", path: "specifications/ui/RUN-runs.md" },
      { kind: "draft", draftId: "draft-01H9" },
      ...PANEL_TARGETS.map((surface) => ({ kind: "panel", surface }) as const),
      ...BOTTOM_TARGETS.map((surface) => ({ kind: "bottom", surface }) as const),
      { kind: "settings", which: "global" },
      { kind: "settings", which: "project" },
    ];

    for (const target of targets) {
      const parsed = roundTrip(target);
      expect(parsed, `round-tripping ${JSON.stringify(target)}`).not.toBeNull();
      expect(parsed!.projectKey).toBe(PROJECT);
      expect(parsed!.worktree).toBe(WORKTREE);
      expect(parsed!.target).toEqual(target);
    }
  });

  it("mints the documented shape", () => {
    // Pinned literally: this string crosses the IPC boundary, is carried by the
    // operating system, and comes back — a silent change to its shape would
    // strand every notification posted by a running application.
    expect(
      mintAddress("acme", "acme", { kind: "bottom", surface: "runs" }),
    ).toBe("synthesis://acme/acme/bottom/runs");
    expect(mintAddress("acme", "acme", { kind: "dashboard" })).toBe(
      "synthesis://acme/acme/dashboard",
    );
  });

  it("preserves case in the project key and the worktree", () => {
    // The reason parsing is hand-rolled rather than `new URL()`: URL treats the
    // segment after `//` as a host and LOWERCASES it, so `/Users/Me` would come
    // back `/users/me` and match no open project — for reasons nothing in the
    // UI could explain.
    const address = mintAddress("/Users/Me/Dev/ACME", "/Users/Me/Dev/ACME", {
      kind: "dashboard",
    });
    const parsed = parseAddress(address);
    expect(parsed!.projectKey).toBe("/Users/Me/Dev/ACME");
    expect(parsed!.worktree).toBe("/Users/Me/Dev/ACME");
  });

  it("survives characters that would otherwise split or break a segment", () => {
    // A path with spaces, a `#`, a `?`, and a literal `%` must come back
    // byte-identical, and a project key containing a slash must not become two
    // segments.
    const path = "specs/a b/c#d?e/100% done.md";
    const parsed = parseAddress(
      mintAddress(PROJECT, WORKTREE, { kind: "file", path }),
    );
    expect(parsed!.target).toEqual({ kind: "file", path });

    const draftId = "draft/with/slashes";
    const draft = parseAddress(
      mintAddress(PROJECT, WORKTREE, { kind: "draft", draftId }),
    );
    expect(draft!.target).toEqual({ kind: "draft", draftId });
  });

  it("keeps nested file paths whole", () => {
    const path = "a/b/c/d/RUN-runs.md";
    const parsed = parseAddress(
      mintAddress(PROJECT, WORKTREE, { kind: "file", path }),
    );
    expect(parsed!.target).toEqual({ kind: "file", path });
  });
});

describe("addresses that are not addresses (NTF-FR-19)", () => {
  it("returns null rather than throwing, whatever it is handed", () => {
    // NTF-FR-19 folds a malformed payload into the same branch as an
    // unreachable target, which only works if parsing never throws.
    const rubbish = [
      "",
      "not a uri at all",
      "https://example.com/a/b",
      "synthesis://",
      "synthesis://only-a-project",
      "synthesis://project/worktree",
      "synthesis://project/worktree/nonsense",
      "synthesis://project/worktree/panel/not-a-surface",
      "synthesis://project/worktree/bottom/library", // right shape, wrong zone
      "synthesis://project/worktree/panel/runs", // and the converse
      "synthesis://project/worktree/settings/nope",
      "synthesis://project/worktree/draft", // a draft with no id
      "synthesis://project/worktree/file", // a file with no path
      "synthesis://project/worktree/dashboard/extra",
      "synthesis://project/worktree/draft/a/b", // a draft id is one segment
      "synthesis:///worktree/dashboard", // an empty project key
      "synthesis://project//dashboard", // an empty worktree
      "synthesis://project/worktree/file/%ZZ", // an undecodable escape
      "synthesis://%ZZ/worktree/dashboard",
    ];
    for (const raw of rubbish) {
      expect(() => parseAddress(raw), `parsing ${raw!}`).not.toThrow();
      expect(parseAddress(raw), `parsing ${raw!}`).toBeNull();
    }
  });

  it("rejects a non-string payload without throwing", () => {
    // The payload arrives over IPC, so its type is a claim rather than a fact.
    expect(parseAddress(undefined as unknown as string)).toBeNull();
    expect(parseAddress(null as unknown as string)).toBeNull();
    expect(parseAddress(42 as unknown as string)).toBeNull();
  });

  it("admits no address for a Search, History, or Diff tab (NTF-FR-03)", () => {
    // The grammar is the enforcement: there is no target kind for any of the
    // three, so nothing can mint one and nothing can parse one.
    for (const kind of ["search", "history", "diff"]) {
      expect(parseAddress(`synthesis://p/w/${kind}/anything`)).toBeNull();
    }
  });
});

describe("sameRoot (NTF-FR-04)", () => {
  it("distinguishes one file in two worktrees of a repository", () => {
    // NTF-FR-04, NTF-FR-19: the same project-relative path in worktree A and worktree B
    // yields two addresses, and only the one naming the active worktree
    // resolves.
    const target: NotificationTarget = { kind: "file", path: "a.md" };
    const inA = parseAddress(mintAddress(PROJECT, "/wt/a", target))!;
    const inB = parseAddress(mintAddress(PROJECT, "/wt/b", target))!;

    expect(mintAddress(PROJECT, "/wt/a", target)).not.toBe(
      mintAddress(PROJECT, "/wt/b", target),
    );
    expect(sameRoot(inA, PROJECT, "/wt/a")).toBe(true);
    expect(sameRoot(inB, PROJECT, "/wt/a")).toBe(false);
    expect(sameRoot(inA, "/other/project", "/wt/a")).toBe(false);
  });
});

describe("an address is a value, not a handle (NTF-FR-05)", () => {
  it("parses identically however long it has been held", () => {
    // NTF-FR-05: nothing about parsing depends on time, on what is open, or on
    // anything the address might once have referred to — which is what makes
    // holding one cost nothing.
    const address = mintAddress(PROJECT, WORKTREE, {
      kind: "file",
      path: "a.md",
    });
    const first = parseAddress(address);
    const second = parseAddress(address);
    expect(first).toEqual(second);
    expect(first).toEqual({
      projectKey: PROJECT,
      worktree: WORKTREE,
      target: { kind: "file", path: "a.md" },
    });
  });
});

describe("describeTarget (NTF-FR-20)", () => {
  it("names a file by its basename rather than its whole path", () => {
    // The statement says what an author would recognise from the tab strip.
    expect(
      describeTarget({ kind: "file", path: "specifications/ui/RUN-runs.md" }),
    ).toBe("RUN-runs.md");
  });

  it("names every other target in prose rather than as a URI", () => {
    expect(describeTarget({ kind: "dashboard" })).toBe("the Dashboard");
    expect(describeTarget({ kind: "draft", draftId: "d1" })).toBe("that draft");
    expect(describeTarget({ kind: "settings", which: "global" })).toBe(
      "Global settings",
    );
    expect(describeTarget({ kind: "settings", which: "project" })).toBe(
      "Project settings",
    );
    // Whatever it says, it never leaks the address itself into the statement.
    for (const target of [
      { kind: "panel", surface: "comments" } as const,
      { kind: "bottom", surface: "runs" } as const,
    ]) {
      expect(describeTarget(target)).not.toContain("synthesis://");
    }
  });
});

describe("a run address (NTF-FR-03 / GRU-FR-BLSS)", () => {
  it("round-trips, and names a place work happens rather than a file", () => {
    const address = mintAddress("/dev/acme", "/dev/acme", {
      kind: "run",
      runId: "g1a0012be-4c9",
    });
    expect(address).toContain("/run/");
    const parsed = parseAddress(address);
    expect(parsed?.target).toEqual({ kind: "run", runId: "g1a0012be-4c9" });
    expect(parsed?.projectKey).toBe("/dev/acme");
  });

  it("is not this address when it says more or less than a run id", () => {
    expect(parseAddress("synthesis://p/w/run")).toBeNull();
    expect(parseAddress("synthesis://p/w/run/a/b")).toBeNull();
  });

  it("NTF-FR-28: no tab is a view onto a run, so none is ever marked by one", () => {
    // `targetForTab` is what maps the strip onto addresses; a run has no tab,
    // so nothing it returns can equal a run target.
    const runTarget = { kind: "run", runId: "run-1" } as const;
    expect(sameTarget(runTarget, { kind: "draft", draftId: "run-1" })).toBe(
      false,
    );
    expect(sameTarget(runTarget, runTarget)).toBe(true);
    expect(describeTarget(runTarget)).toBe("that graduation run");
  });
});

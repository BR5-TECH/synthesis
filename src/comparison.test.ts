import { describe, expect, it } from "vitest";

import {
  comparisonKey,
  comparisonLabel,
  diffScopeFor,
  diffTabId,
  scopeKey,
} from "./comparison";
import type { Comparison, DiffTarget } from "./types";

const uncommitted: Comparison = { kind: "uncommitted" };
const againstMain: Comparison = {
  kind: "branch",
  targetBranch: "main",
  mergeBase: "abc123",
};
const againstDevelop: Comparison = {
  kind: "branch",
  targetBranch: "develop",
  mergeBase: "def456",
};

function target(path: string, comparison: Comparison): DiffTarget {
  return {
    path,
    name: path.split("/").pop() ?? path,
    scope: diffScopeFor(comparison, path),
    comparisonLabel: comparisonLabel(comparison),
  };
}

describe("comparisonKey", () => {
  it("distinguishes the two modes and each branch target", () => {
    expect(comparisonKey(uncommitted)).toBe("uncommitted");
    expect(comparisonKey(againstMain)).not.toBe(comparisonKey(uncommitted));
    expect(comparisonKey(againstMain)).not.toBe(comparisonKey(againstDevelop));
  });

  it("ignores the merge base, so a moving base is still the same comparison", () => {
    // The panel reloads when the repository changes (CHG-FR-21); a new merge
    // base must re-render the open Diff tab, not open a second one.
    expect(comparisonKey({ ...againstMain, mergeBase: "999zzz" })).toBe(
      comparisonKey(againstMain),
    );
  });
});

describe("diffScopeFor (CHG-FR-18)", () => {
  it("maps the uncommitted comparison to a path scope", () => {
    expect(diffScopeFor(uncommitted, "src/App.tsx")).toEqual({
      kind: "path",
      path: "src/App.tsx",
    });
  });

  it("carries the target branch into a branch scope", () => {
    expect(diffScopeFor(againstMain, "src/App.tsx")).toEqual({
      kind: "branch",
      path: "src/App.tsx",
      targetBranch: "main",
    });
  });
});

describe("diffTabId (CHG-FR-19)", () => {
  it("is the pair of file and comparison, and nothing else", () => {
    const a = diffTabId(target("src/App.tsx", uncommitted));
    const again = diffTabId(target("src/App.tsx", uncommitted));
    expect(a).toBe(again);
  });

  it("gives the same file under a different comparison a different id", () => {
    expect(diffTabId(target("src/App.tsx", uncommitted))).not.toBe(
      diffTabId(target("src/App.tsx", againstMain)),
    );
    expect(diffTabId(target("src/App.tsx", againstMain))).not.toBe(
      diffTabId(target("src/App.tsx", againstDevelop)),
    );
  });

  it("gives different files under one comparison different ids", () => {
    expect(diffTabId(target("a/x.md", uncommitted))).not.toBe(
      diffTabId(target("b/x.md", uncommitted)),
    );
  });

  it("ignores previousPath, so staging a rename does not open a second tab", () => {
    // CHG-FR-19: identity is (file, comparison). `previousPath` rides along in
    // the scope so the backend can pair a rename, but it is not part of who the
    // tab is — and it changes as a rename is staged and unstaged.
    const withPrevious: DiffTarget = {
      path: "src-tauri/lib.rs",
      name: "lib.rs",
      scope: diffScopeFor(uncommitted, "src-tauri/lib.rs", "src-tauri/main.rs"),
      comparisonLabel: comparisonLabel(uncommitted),
    };
    const withoutPrevious = target("src-tauri/lib.rs", uncommitted);
    expect(diffTabId(withPrevious)).toBe(diffTabId(withoutPrevious));
    expect(scopeKey(withPrevious.scope)).toBe(scopeKey(withoutPrevious.scope));
  });

  it("namespaces Diff tabs away from the art: ids Editor and Flow tabs use", () => {
    // CHG-FR-20 / TAB-FR-06: a Diff tab must be able to coexist with a live
    // Editor tab for the same artifact.
    const id = diffTabId(target("src/App.tsx", uncommitted));
    expect(id.startsWith("diff:")).toBe(true);
    expect(id).not.toBe("art:src/App.tsx");
  });
});

describe("scopeKey", () => {
  it("agrees with comparisonKey for the scopes a comparison produces", () => {
    expect(scopeKey(diffScopeFor(uncommitted, "a.md"))).toBe(
      comparisonKey(uncommitted),
    );
    expect(scopeKey(diffScopeFor(againstMain, "a.md"))).toBe(
      comparisonKey(againstMain),
    );
  });

  it("keeps the staged scope distinct from the uncommitted one", () => {
    expect(scopeKey({ kind: "staged" })).not.toBe(
      scopeKey({ kind: "path", path: "a.md" }),
    );
  });
});

describe("comparisonLabel", () => {
  it("names the target branch so a Diff tab says what it is comparing", () => {
    expect(comparisonLabel(againstMain)).toContain("main");
    expect(comparisonLabel(uncommitted)).not.toContain("main");
  });
});

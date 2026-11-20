/**
 * The pure row model of the Project panel (`treeModel.ts`): which rows show,
 * which folders load, and which filters a reveal relaxes
 * (`../../../specifications/ui/LIB-library.md`).
 */
import { describe, expect, it } from "vitest";

import type { FilterResult, TreeNode } from "../../types";
import { withFolder } from "./useLazyTree";
import {
  ancestorsOf,
  filtersToRelax,
  folderVisible,
  foldersToLoad,
  lensOf,
  nodeVisible,
  parentOf,
  resultHolds,
  visibleRows,
  type RowsInput,
  type TreeRowModel,
} from "./treeModel";

function file(path: string, artifactType?: TreeNode["artifactType"]): TreeNode {
  return { id: path, name: path.split("/").pop()!, path, nodeKind: "file", artifactType };
}

/** A wire folder: no `children`, flags as the backend sends them. */
function dir(path: string, over: Partial<TreeNode> = {}): TreeNode {
  return {
    id: path,
    name: path.split("/").pop()!,
    path,
    nodeKind: "folder",
    hasArtifacts: true,
    subtreeTypes: ["spec"],
    hasChildren: true,
    ...over,
  };
}

function input(over: Partial<RowsInput>): RowsInput {
  return {
    root: null,
    childrenOf: new Map(),
    expanded: new Set(),
    typeFilter: "files",
    filtered: null,
    ...over,
  };
}

/** A short form of each row, so a whole list reads in one assertion. */
function shape(rows: TreeRowModel[]): string[] {
  return rows.map((r) => {
    if (r.kind === "loading") return `loading:${r.depth}`;
    if (r.kind === "truncated") return "truncated";
    return `${r.node.id}:${r.depth}${r.open ? ":open" : ""}`;
  });
}

describe("lensOf (LIB-FR-04)", () => {
  it("LIB-FR-04: maps the panel's lens to the backend's spelling", () => {
    expect(lensOf("artifacts")).toBe("all_artifacts");
    expect(lensOf("files")).toBe("all_files");
    expect(lensOf("spec")).toBe("spec");
  });
});

describe("folder and node visibility from the backend's flags (LIB-FR-09)", () => {
  it("LIB-FR-09: All files admits every folder, All artifacts reads hasArtifacts, a type reads subtreeTypes", () => {
    const empty = dir("misc", { hasArtifacts: false, subtreeTypes: [] });
    const specs = dir("specs", { hasArtifacts: true, subtreeTypes: ["spec"] });
    expect(folderVisible(empty, "files")).toBe(true);
    expect(folderVisible(empty, "artifacts")).toBe(false);
    expect(folderVisible(specs, "artifacts")).toBe(true);
    expect(folderVisible(specs, "spec")).toBe(true);
    expect(folderVisible(specs, "skill")).toBe(false);
    // Flags that are absent read as nothing held.
    expect(folderVisible(dir("x", { hasArtifacts: undefined, subtreeTypes: undefined }), "artifacts")).toBe(false);
    expect(folderVisible(dir("x", { subtreeTypes: undefined }), "spec")).toBe(false);
  });

  it("LIB-FR-05: a file shows when the lens admits its own type", () => {
    expect(nodeVisible(file("a.md", "spec"), "spec")).toBe(true);
    expect(nodeVisible(file("a.md", "spec"), "skill")).toBe(false);
    expect(nodeVisible(file("a.txt"), "artifacts")).toBe(false);
    expect(nodeVisible(file("a.txt"), "files")).toBe(true);
  });

  it("LIB-FR-VIAZ: a repository marker changes no visibility rule", () => {
    for (const nestedRepo of ["submodule", "repository", "uninitialised"] as const) {
      const hollow = dir("vendor", { hasArtifacts: false, subtreeTypes: [], nestedRepo });
      const typed = dir("lib", { nestedRepo });
      expect(nodeVisible(hollow, "artifacts")).toBe(false);
      expect(nodeVisible(hollow, "files")).toBe(true);
      expect(nodeVisible(typed, "artifacts")).toBe(true);
      expect(nodeVisible(typed, "spec")).toBe(true);
      expect(nodeVisible(typed, "skill")).toBe(false);
    }
  });
});

describe("visibleRows (LIB-FR-EMOO, LIB-FR-JZNT, LIB-FR-PPAT)", () => {
  it("LIB-FR-EMOO: gives no row before the root is loaded", () => {
    expect(visibleRows(input({}))).toEqual([]);
  });

  it("LIB-FR-EMOO: a collapsed folder shows no row under it, loaded or not", () => {
    const docs = dir("docs");
    const rows = visibleRows(
      input({
        root: [docs, file("a.md", "spec")],
        childrenOf: new Map([["docs", [file("docs/b.md", "spec")]]]),
      }),
    );
    expect(shape(rows)).toEqual(["docs:0", "a.md:0"]);
  });

  it("LIB-FR-JZNT: an expanded folder that is not loaded shows one loading row under it", () => {
    const rows = visibleRows(
      input({ root: [dir("docs"), file("a.md", "spec")], expanded: new Set(["docs"]) }),
    );
    expect(shape(rows)).toEqual(["docs:0:open", "loading:1", "a.md:0"]);
  });

  it("LIB-FR-JZNT: an expanded folder whose hasChildren is false shows no loading row", () => {
    const rows = visibleRows(
      input({ root: [dir("empty", { hasChildren: false })], expanded: new Set(["empty"]) }),
    );
    expect(shape(rows)).toEqual(["empty:0:open"]);
  });

  it("LIB-FR-07: loaded children of expanded folders show in tree order with their depth", () => {
    const rows = visibleRows(
      input({
        root: [dir("a"), file("z.md", "spec")],
        childrenOf: new Map([
          ["a", [dir("a/b"), file("a/c.md", "spec")]],
          ["a/b", [file("a/b/d.md", "spec")]],
        ]),
        expanded: new Set(["a", "a/b"]),
      }),
    );
    expect(shape(rows)).toEqual(["a:0:open", "a/b:1:open", "a/b/d.md:2", "a/c.md:1", "z.md:0"]);
  });

  it("LIB-FR-05, LIB-FR-09: the lens drops the rows it does not admit, with an empty text filter", () => {
    const rows = visibleRows(
      input({
        root: [
          dir("misc", { hasArtifacts: false, subtreeTypes: [] }),
          file("notes.txt"),
          file("a.md", "spec"),
        ],
        typeFilter: "artifacts",
      }),
    );
    expect(shape(rows)).toEqual(["a.md:0"]);
  });

  it("LIB-FR-PPAT: with a filter result, the rows come from it alone, and the expand state still holds", () => {
    const filtered: FilterResult = {
      root: [
        { ...dir("docs"), children: [file("docs/match.md", "spec")] },
        { ...dir("other"), children: [file("other/match2.md", "spec")] },
      ],
      truncated: false,
    };
    const rows = visibleRows(
      input({
        // Loaded rows that the result does not hold do not show.
        root: [file("loaded.md", "spec")],
        expanded: new Set(["docs"]),
        // The lens is applied by the backend, so it filters nothing here.
        typeFilter: "skill",
        filtered,
      }),
    );
    expect(shape(rows)).toEqual(["docs:0:open", "docs/match.md:1", "other:0"]);
  });

  it("LIB-FR-PPAT: a truncated result ends with one truncated row", () => {
    const rows = visibleRows(
      input({ root: [], filtered: { root: [file("a.md", "spec")], truncated: true } }),
    );
    expect(shape(rows)).toEqual(["a.md:0", "truncated"]);
  });
});

describe("foldersToLoad (LIB-FR-EMOO)", () => {
  it("LIB-FR-EMOO: names each expanded folder on a row that has no children loaded", () => {
    const rows = visibleRows(
      input({
        root: [dir("a"), dir("b"), dir("c")],
        childrenOf: new Map([["b", []]]),
        expanded: new Set(["a", "b"]),
      }),
    );
    expect(foldersToLoad(rows)).toEqual(["a"]);
  });

  it("LIB-FR-EMOO: never names a folder inside a collapsed folder, even when it is expanded", () => {
    const rows = visibleRows(
      input({
        root: [dir("a")],
        childrenOf: new Map([["a", [dir("a/b")]]]),
        // `a/b` is in the expanded set, but `a` is collapsed.
        expanded: new Set(["a/b"]),
      }),
    );
    expect(foldersToLoad(rows)).toEqual([]);
  });

  it("LIB-FR-EMOO: names nothing for a filter result, which carries its own children", () => {
    const rows = visibleRows(
      input({
        root: [],
        expanded: new Set(["docs"]),
        filtered: { root: [{ ...dir("docs"), children: [] }], truncated: false },
      }),
    );
    expect(foldersToLoad(rows)).toEqual([]);
  });
});

describe("node paths (LIB-FR-18)", () => {
  it("LIB-FR-18: ancestorsOf gives the ancestor folders, root first", () => {
    expect(ancestorsOf("a/b/c.md")).toEqual(["a", "a/b"]);
    expect(ancestorsOf("top.md")).toEqual([]);
  });

  it("LIB-FR-18: parentOf gives the parent folder, the root as an empty path", () => {
    expect(parentOf("a/b/c.md")).toBe("a/b");
    expect(parentOf("top.md")).toBe("");
  });
});

describe("resultHolds (LIB-FR-PPAT)", () => {
  const root: TreeNode[] = [
    { ...dir("docs"), children: [{ ...dir("docs/sub"), children: [file("docs/sub/x.md")] }] },
    file("readme.md"),
  ];

  it("LIB-FR-PPAT: finds a node at any depth of the nested result", () => {
    expect(resultHolds(root, "docs")).toBe(true);
    expect(resultHolds(root, "docs/sub/x.md")).toBe(true);
    expect(resultHolds(root, "readme.md")).toBe(true);
  });

  it("LIB-FR-PPAT: does not take a sibling whose name starts with the same text", () => {
    expect(resultHolds(root, "docs-old")).toBe(false);
    expect(resultHolds(root, "docs/sub/y.md")).toBe(false);
  });
});

describe("filtersToRelax (LIB-FR-18)", () => {
  const typed = dir("drafts", { artifactType: "spec", subtreeTypes: ["spec"] });
  const hollow = dir("inbox", { hasArtifacts: false, subtreeTypes: [] });
  const docs = dir("docs", { subtreeTypes: ["spec"] });

  it("LIB-FR-18: touches nothing when the active filters admit the node", () => {
    expect(filtersToRelax(typed, "spec", "", null)).toEqual({ lens: false, text: false });
    expect(filtersToRelax(typed, "spec", "dra", null)).toEqual({ lens: false, text: false });
    expect(filtersToRelax(hollow, "files", "inbox", null)).toEqual({ lens: false, text: false });
  });

  it("LIB-FR-18: with an empty text filter, drops the lens when the flags hide the node", () => {
    expect(filtersToRelax(hollow, "artifacts", "", null)).toEqual({ lens: true, text: false });
  });

  it("LIB-FR-18: drops only the lens when the name matches the text", () => {
    expect(filtersToRelax(hollow, "artifacts", "inbox", null)).toEqual({ lens: true, text: false });
  });

  it("LIB-FR-18: drops only the lens when the two filters hide a folder jointly", () => {
    // `docs` matches the text by name and holds a spec, but no one node
    // matches both, so the backend's result does not hold it.
    const result: FilterResult = { root: [], truncated: false };
    expect(filtersToRelax(docs, "spec", "docs", result)).toEqual({ lens: true, text: false });
  });

  it("LIB-FR-18: clears only the text when the lens admits the node and the text does not", () => {
    expect(filtersToRelax(typed, "spec", "zzz", null)).toEqual({ lens: false, text: true });
  });

  it("LIB-FR-18: relaxes both when each filter hides the node on its own", () => {
    expect(filtersToRelax(hollow, "artifacts", "zzz", null)).toEqual({ lens: true, text: true });
  });

  it("LIB-FR-18: touches nothing for a folder the current result holds as an ancestor of a match", () => {
    const result: FilterResult = {
      root: [{ ...docs, children: [file("docs/LIB.md", "spec")] }],
      truncated: false,
    };
    expect(filtersToRelax(docs, "spec", "LIB", result)).toEqual({ lens: false, text: false });
  });
});

describe("withFolder (LIB-FR-10)", () => {
  const f = (id: string): TreeNode => ({ id, name: id.split("/").pop()!, path: id, nodeKind: "folder" });
  it("LIB-FR-10: a child folder that is gone is dropped with everything below it", () => {
    const prev = new Map<string, TreeNode[]>([
      ["", [f("a")]],
      ["a", [f("a/b"), f("a/c")]],
      ["a/b", [f("a/b/d")]],
      ["a/b/d", []],
      ["a/c", []],
      ["ab", []],
    ]);
    const next = withFolder(prev, "a", [f("a/c")]);
    expect([...next.keys()].sort()).toEqual(["", "a", "a/c", "ab"]);
    expect(next.get("a")).toEqual([f("a/c")]);
  });
  it("LIB-FR-10: a reload of the root keeps the folders it still holds", () => {
    const prev = new Map<string, TreeNode[]>([["", [f("a"), f("b")]], ["a", []], ["b", []]]);
    const next = withFolder(prev, "", [f("a")]);
    expect([...next.keys()].sort()).toEqual(["", "a"]);
  });
});

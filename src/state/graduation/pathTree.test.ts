import { describe, expect, it } from "vitest";

import {
  buildPathTree,
  defaultOpenFolders,
  OPEN_ALL_LIMIT,
  pathTotal,
  type PathFolder,
  type PathNode,
} from "./pathTree";

/** The tree as indented lines, so a test reads the shape it asserts. */
function outline(nodes: readonly PathNode[], depth = 0): string[] {
  return nodes.flatMap((node) =>
    node.kind === "file"
      ? [`${"  ".repeat(depth)}${node.name}`]
      : [
          `${"  ".repeat(depth)}${node.label}/ (${node.count})`,
          ...outline(node.children, depth + 1),
        ],
  );
}

describe("buildPathTree", () => {
  it("GRU-FR-TXLW: folders come first, each level in name order, with counts at every depth", () => {
    const tree = buildPathTree([
      "src/b.ts",
      "README.md",
      "src/state/x.ts",
      "src/a.ts",
      "docs/guide.md",
      "src/state/y.ts",
    ]);
    expect(outline(tree)).toEqual([
      "docs/ (1)",
      "  guide.md",
      "src/ (4)",
      "  state/ (2)",
      "    x.ts",
      "    y.ts",
      "  a.ts",
      "  b.ts",
      "README.md",
    ]);
  });

  it("GRU-FR-TXLW: a chain of folders that each hold one folder is one row", () => {
    const tree = buildPathTree([
      "src-tauri/src/agent/tests/a.rs",
      "src-tauri/src/agent/tests/b.rs",
      "src-tauri/src/agent/mod.rs",
    ]);
    expect(outline(tree)).toEqual([
      "src-tauri/src/agent/ (3)",
      "  tests/ (2)",
      "    a.rs",
      "    b.rs",
      "  mod.rs",
    ]);
    // The row names the deepest folder it stands for.
    const top = tree[0] as PathFolder;
    expect(top.path).toBe("src-tauri/src/agent");
  });

  it("GRU-FR-TXLW: a folder that holds a file is not compacted into the folder under it", () => {
    const tree = buildPathTree(["a/x.ts", "a/b/y.ts"]);
    expect(outline(tree)).toEqual(["a/ (2)", "  b/ (1)", "    y.ts", "  x.ts"]);
  });

  it("GRU-FR-TXLW: a file and a folder of one name are two rows with two keys", () => {
    const tree = buildPathTree(["a", "a/b.ts"]);
    expect(outline(tree)).toEqual(["a/ (1)", "  b.ts", "a"]);
    expect(new Set(tree.map((node) => node.key)).size).toBe(2);
  });

  it("GRU-FR-WJHV: a folder the ignore rules hide whole is one path row with its slash", () => {
    const tree = buildPathTree(["node_modules/", "src-tauri/gen/schemas/", "src-tauri/target/"]);
    expect(outline(tree)).toEqual([
      "src-tauri/ (2)",
      "  gen/ (1)",
      "    schemas/",
      "  target/",
      "node_modules/",
    ]);
    const leaf = (tree[1] as { path: string }).path;
    expect(leaf).toBe("node_modules/");
  });

  it("GRU-FR-TXLW: a path listed twice is one row and counts once", () => {
    const tree = buildPathTree(["src/a.ts", "src/a.ts"]);
    expect(outline(tree)).toEqual(["src/ (1)", "  a.ts"]);
  });

  it("GRU-FR-TXLW: no paths make no tree", () => {
    expect(buildPathTree([])).toEqual([]);
  });
});

describe("buildPathTree at its edges", () => {
  it("GRU-FR-TXLW: a deep chain is one row keyed by its deepest folder", () => {
    const tree = buildPathTree(["a/b/c/d/e/f.ts"]);
    expect(outline(tree)).toEqual(["a/b/c/d/e/ (1)", "  f.ts"]);
    expect(tree[0].key).toBe("d:a/b/c/d/e");
  });

  it("GRU-FR-TXLW: a chain that ends in a hidden folder keeps the folder as a path row", () => {
    expect(outline(buildPathTree(["a/b/c/"]))).toEqual(["a/b/ (1)", "  c/"]);
  });

  it("GRU-FR-TXLW: a hidden folder and a folder of one name are two rows, the folder first", () => {
    const tree = buildPathTree(["x/a/", "x/a/b.ts"]);
    expect(outline(tree)).toEqual(["x/ (2)", "  a/ (1)", "    b.ts", "  a/"]);
    const inner = (tree[0] as PathFolder).children.map((node) => node.key);
    expect(new Set(inner).size).toBe(2);
  });

  it("GRU-FR-TXLW: an empty entry names no path", () => {
    const tree = buildPathTree(["", "/", "a.ts"]);
    expect(outline(tree)).toEqual(["a.ts"]);
    expect(pathTotal(tree)).toBe(1);
  });

  it("GRU-FR-TXLW: a number in a name sorts by its value", () => {
    expect(outline(buildPathTree(["SPEC-10.md", "SPEC-2.md", "SPEC-1.md"]))).toEqual([
      "SPEC-1.md",
      "SPEC-2.md",
      "SPEC-10.md",
    ]);
  });

  it("GRU-FR-TXLW: names sort the same in every locale", () => {
    expect(outline(buildPathTree(["Zeta.ts", "alpha.ts", "beta.ts"]))).toEqual([
      "alpha.ts",
      "beta.ts",
      "Zeta.ts",
    ]);
  });
});

describe("defaultOpenFolders", () => {
  it("GRU-FR-HEQB: the limit is the 24 paths the spec names", () => {
    expect(OPEN_ALL_LIMIT).toBe(24);
  });

  const deep = (count: number) =>
    Array.from({ length: count }, (_, i) => `src/${i % 2 ? "a" : "b"}/deep/f${i}.ts`).concat(
      "docs/x/one.md",
      "docs/y/two.md",
    );

  it(`GRU-FR-HEQB: a list of ${OPEN_ALL_LIMIT} paths or fewer opens every folder`, () => {
    const paths = deep(OPEN_ALL_LIMIT - 2);
    expect(paths).toHaveLength(OPEN_ALL_LIMIT);
    const tree = buildPathTree(paths);
    const open = defaultOpenFolders(tree, paths.length);
    const every: string[] = [];
    const walk = (nodes: readonly PathNode[]) => {
      for (const node of nodes) {
        if (node.kind !== "folder") continue;
        every.push(node.key);
        walk(node.children);
      }
    };
    walk(tree);
    expect([...open].sort()).toEqual(every.sort());
  });

  it("GRU-FR-HEQB: a longer list opens its root folders and keeps every deeper folder closed", () => {
    const paths = deep(OPEN_ALL_LIMIT - 1);
    expect(paths).toHaveLength(OPEN_ALL_LIMIT + 1);
    const tree = buildPathTree(paths);
    const open = defaultOpenFolders(tree, paths.length);
    expect([...open].sort()).toEqual(
      tree.filter((node) => node.kind === "folder").map((node) => node.key).sort(),
    );
  });
});

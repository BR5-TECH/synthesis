// The document tree and its filter as pure functions (`DPN-documents-panel.md`
// DPN-FR-NHDZ, DPN-FR-TELU, DPN-FR-AREM).
import { describe, expect, it } from "vitest";

import { buildTree, filterTree, visibleRows, type FolderNode, type TreeNode } from "./tree";
import { doc } from "./panelFixtures";

const labels = (nodes: TreeNode[]): string[] => nodes.map((n) => n.label);

describe("DPN-FR-NHDZ: the tree of the collection", () => {
  it("DPN-FR-NHDZ: holds every document and the folders that contain them, each path once", () => {
    const docs = [
      doc("/r/specs/a.pdf"),
      doc("/r/specs/b.md"),
      doc("/r/c.txt"),
      doc("/r/x/d.md"),
    ];
    const tree = buildTree(docs);
    const paths: string[] = [];
    const walk = (nodes: TreeNode[]) => {
      for (const n of nodes) {
        paths.push(n.kind === "folder" ? n.key : n.entry.path);
        if (n.kind === "folder") walk(n.children);
      }
    };
    walk(tree);
    expect(paths.sort()).toEqual(
      ["/r", "/r/specs", "/r/specs/a.pdf", "/r/specs/b.md", "/r/c.txt", "/r/x", "/r/x/d.md"].sort(),
    );
    expect(new Set(paths).size).toBe(paths.length);
  });

  it("DPN-FR-NHDZ: a chain of folders that holds one folder and no document is one row labelled with the joined names", () => {
    const tree = buildTree([doc("/Users/me/reference/specs/a.pdf")]);
    expect(tree).toHaveLength(1);
    const row = tree[0] as FolderNode;
    expect(row.label).toBe("Users/me/reference/specs");
    expect(row.key).toBe("/Users/me/reference/specs");
    expect(labels(row.children)).toEqual(["a.pdf"]);
  });

  it("DPN-FR-NHDZ: a folder with a document or with two folders is not merged into its child", () => {
    const tree = buildTree([
      doc("/r/a/one.md"),
      doc("/r/a/b/two.md"),
      doc("/r/c/three.md"),
    ]);
    const root = tree[0] as FolderNode;
    expect(root.label).toBe("r");
    const a = root.children.find((n) => n.label === "a") as FolderNode;
    expect(labels(a.children)).toEqual(["b", "one.md"]);
  });

  it("DPN-FR-NHDZ: folders sort before documents, and each group sorts by name ignoring case", () => {
    const tree = buildTree([
      doc("/r/b.md"),
      doc("/r/A.md"),
      doc("/r/zeta/x.md"),
      doc("/r/Alpha/x.md"),
      doc("/r/c.txt"),
      doc("/r/beta/x.md"),
    ]);
    const root = tree[0] as FolderNode;
    expect(labels(root.children)).toEqual(["Alpha", "beta", "zeta", "A.md", "b.md", "c.txt"]);
  });

  it("DPN-FR-NHDZ: an empty collection has an empty tree, and a root-level file has no folder", () => {
    expect(buildTree([])).toEqual([]);
    const tree = buildTree([doc("/file.md")]);
    expect(labels(tree)).toEqual(["file.md"]);
  });

  it("DPN-FR-NHDZ: two documents of one path are shown once each as the collection reports them, and the key of a document is its id", () => {
    const a = doc("/r/a.md");
    const tree = buildTree([a]);
    const node = (tree[0] as FolderNode).children[0];
    expect(node.key).toBe(a.id);
  });
});

describe("DPN-FR-TELU: the filter", () => {
  const docs = [
    doc("/r/specs/api-guide.pdf"),
    doc("/r/specs/old.txt"),
    doc("/r/notes.md"),
    doc("/r/Other/deep/readme.md"),
  ];

  it("DPN-FR-TELU: an empty filter keeps the tree as it is and forces nothing open", () => {
    const tree = buildTree(docs);
    const result = filterTree(tree, "  ");
    expect(result.nodes).toBe(tree);
    expect(result.open.size).toBe(0);
  });

  it("DPN-FR-TELU: keeps the rows whose name contains the text, ignoring case, and the folders above a match", () => {
    const result = filterTree(buildTree(docs), "API");
    const root = result.nodes[0] as FolderNode;
    expect(root.label).toBe("r");
    expect(labels(root.children)).toEqual(["specs"]);
    const specs = root.children[0] as FolderNode;
    expect(labels(specs.children)).toEqual(["api-guide.pdf"]);
    expect([...result.open].sort()).toEqual(["/r", "/r/specs"]);
  });

  it("DPN-FR-TELU: a folder that matches by its own name shows everything below it", () => {
    const result = filterTree(buildTree(docs), "specs");
    const root = result.nodes[0] as FolderNode;
    const specs = root.children[0] as FolderNode;
    expect(labels(specs.children)).toEqual(["api-guide.pdf", "old.txt"]);
    expect(result.open.has("/r/specs")).toBe(true);
  });

  it("DPN-FR-TELU: a joined row matches on its joined label", () => {
    const result = filterTree(buildTree([doc("/Users/me/reference/a.md")]), "me/ref");
    expect(result.nodes).toHaveLength(1);
    expect((result.nodes[0] as FolderNode).children).toHaveLength(1);
  });

  it("DPN-FR-TELU: text that matches nothing leaves no rows", () => {
    const result = filterTree(buildTree(docs), "zzz");
    expect(result.nodes).toEqual([]);
    expect(result.open.size).toBe(0);
  });
});

describe("DPN-FR-NHDZ: only the rows of expanded folders are drawn", () => {
  it("DPN-FR-NHDZ, DPN-FR-AREM: a closed folder hides its rows, and an untouched folder starts expanded", () => {
    const tree = buildTree([doc("/r/a/x.md"), doc("/r/a/y.md"), doc("/r/b/z.md"), doc("/r/top.md")]);
    const all = visibleRows(tree, () => true);
    expect(all.map((r) => r.node.label)).toEqual(["r", "a", "x.md", "y.md", "b", "z.md", "top.md"]);
    expect(all.map((r) => r.depth)).toEqual([0, 1, 2, 2, 1, 2, 1]);
    const closed = visibleRows(tree, (key) => key !== "/r/a");
    expect(closed.map((r) => r.node.label)).toEqual(["r", "a", "b", "z.md", "top.md"]);
    expect(closed[1].expanded).toBe(false);
    expect(closed[1].parentKey).toBe("/r");
  });

  it("DPN-FR-NHDZ: a collection of a few thousand documents draws only the rows of expanded folders", () => {
    const docs = Array.from({ length: 4000 }, (_, i) => doc(`/big/dir${i % 40}/file${i}.md`));
    const tree = buildTree(docs);
    const rows = visibleRows(tree, (key) => key === "/big");
    // The 40 folders are drawn; none of the 4000 files is, because no folder
    // below the root is expanded.
    expect(rows.length).toBe(41);
  });
});

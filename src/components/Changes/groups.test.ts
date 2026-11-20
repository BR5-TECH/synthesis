/**
 * The pure rules of the repository groups of the Changes tree (CHG-FR-BYWJ,
 * CHG-FR-IGSD, CHG-FR-UMCU, CHG-FR-EQYC, CHG-FR-GBTE, CHG-FR-AYTR, CHG-FR-54,
 * CHG-FR-NUZB).
 */
import { describe, expect, it } from "vitest";

import { change } from "../../test/changesFixtures";
import type { RepositoryInfo } from "../../types";
import {
  buildRepositoryGroups,
  commitRepositoryChain,
  impliedPointerPaths,
  nodeCheckOf,
  pointerCommitsText,
  pushSkipLines,
  repositoriesOf,
  repositoryBranchLabel,
  repositoryLabel,
  revealKeys,
  toggleableFilesUnder,
} from "./groups";
import { rollbackFailureText } from "./rollbackText";
import { buildChangeTree } from "./tree";

const MAIN: RepositoryInfo = { path: "", name: "acme", branch: "main" };
const KIT: RepositoryInfo = { path: "vendor/kit", name: "kit", parent: "" };
/** A submodule of a submodule (GSM-FR-VPAS). */
const DEEP: RepositoryInfo = {
  path: "vendor/kit/deep",
  name: "deep",
  parent: "vendor/kit",
  branch: "dev",
};

describe("labels (CHG-FR-UMCU, CHG-FR-GBTE)", () => {
  it("CHG-FR-UMCU: names the main repository by name and a submodule by path", () => {
    expect(repositoryLabel(MAIN)).toBe("acme");
    expect(repositoryLabel(KIT)).toBe("vendor/kit");
  });

  it("CHG-FR-UMCU: states the branch, or `detached` and the short id", () => {
    expect(repositoryBranchLabel(MAIN)).toBe("main");
    expect(repositoryBranchLabel({ ...KIT, headId: "0123456789abcdef" })).toBe(
      "detached 0123456",
    );
    // A detached HEAD on a branch with no commit has no id to state.
    expect(repositoryBranchLabel(KIT)).toBe("detached");
  });

  it("CHG-FR-GBTE: states the two short ids, or `+` and the current one when added", () => {
    const pointer = change("vendor/kit", {
      entryKind: "submodule",
      previousCommit: "aaaaaaa0000",
      currentCommit: "bbbbbbb1111",
    });
    expect(pointerCommitsText(pointer)).toBe("aaaaaaa → bbbbbbb");
    expect(pointerCommitsText({ ...pointer, previousCommit: undefined })).toBe(
      "+ bbbbbbb",
    );
  });
});

describe("repositoriesOf (CHC-FR-TMNY)", () => {
  it("CHG-FR-IGSD: an absent list is the main repository alone", () => {
    expect(repositoriesOf(undefined, [change("a.md")])).toEqual([
      { path: "", name: "" },
    ]);
  });

  it("CHG-FR-BYWJ: keeps the listed order and adds a repository only an entry names", () => {
    const listed = repositoriesOf(
      [MAIN, KIT],
      [change("z/x.md", { repository: "z" })],
    );
    expect(listed.map((r) => r.path)).toEqual(["", "vendor/kit", "z"]);
  });
});

describe("buildRepositoryGroups (CHG-FR-BYWJ, CHG-FR-IGSD, CHG-FR-EQYC)", () => {
  it("CHG-FR-IGSD: one repository gives one model with the keys the panel used before", () => {
    const [model, ...rest] = buildRepositoryGroups([change("docs/a.md")], [MAIN]);
    expect(rest).toHaveLength(0);
    expect(model.sections.map((s) => s.node.key)).toEqual([
      "revisioned",
      "unrevisioned",
    ]);
    expect(model.sections[0].node.children[0].key).toBe("r:d:docs");
  });

  it("CHG-FR-BYWJ: skips a repository with no visible entry and keeps the set order", () => {
    const models = buildRepositoryGroups(
      [
        change("vendor/kit/deep/x.md", { repository: DEEP.path }),
        change("a.md"),
      ],
      [MAIN, KIT, DEEP],
    );
    expect(models.map((m) => m.info.path)).toEqual(["", "vendor/kit/deep"]);
    expect(models[1].node.key).toBe("repo:vendor/kit/deep");
  });

  it("CHG-FR-EQYC, CHG-FR-09: builds the tree of a submodule from paths relative to its root, split by status", () => {
    const [, kit] = buildRepositoryGroups(
      [
        change("a.md"),
        change("vendor/kit/docs/b.md", { repository: KIT.path }),
        change("vendor/kit/new.md", {
          repository: KIT.path,
          changeStatus: "untracked",
        }),
      ],
      [MAIN, KIT],
    );
    const [revisioned, unrevisioned] = kit.sections;
    const docs = revisioned.node.children[0];
    expect(docs.name).toBe("docs");
    // The node keeps its project-relative path, so labels and keys stay unique.
    expect(docs.path).toBe("vendor/kit/docs");
    expect(docs.key).toBe("m:vendor/kit|r:d:vendor/kit/docs");
    expect(unrevisioned.node.children.map((n) => n.name)).toEqual(["new.md"]);
    expect(kit.node.children).toHaveLength(2);
  });

  it("CHG-FR-EQYC: a tree with no root keeps the old shape", () => {
    const tree = buildChangeTree([change("x/y.md")], "r:");
    expect(tree[0].key).toBe("r:d:x");
    expect(tree[0].children[0].key).toBe("r:f:x/y.md");
  });
});

describe("revealKeys (CHG-FR-54)", () => {
  it("CHG-FR-54: with one repository, opens the group and the ancestor folders under the old keys", () => {
    const { opening, fileKey } = revealKeys(change("a/b/c.md"), false);
    expect([...opening].sort()).toEqual(["r:d:a", "r:d:a/b", "revisioned"]);
    expect(fileKey).toBe("r:f:a/b/c.md");
  });

  it("CHG-FR-54, CHG-FR-BYWJ: with more than one, also opens the repository group", () => {
    const { opening, fileKey } = revealKeys(
      change("vendor/kit/docs/n.md", {
        repository: KIT.path,
        changeStatus: "untracked",
      }),
      true,
    );
    expect([...opening].sort()).toEqual([
      "m:vendor/kit|u:d:vendor/kit/docs",
      "m:vendor/kit|unrevisioned",
      "repo:vendor/kit",
    ]);
    expect(fileKey).toBe("m:vendor/kit|u:f:vendor/kit/docs/n.md");
  });

  it("CHG-FR-54: the keys match the nodes the groups build", () => {
    const entry = change("vendor/kit/docs/b.md", { repository: KIT.path });
    const [, kit] = buildRepositoryGroups([change("a.md"), entry], [MAIN, KIT]);
    const { opening, fileKey } = revealKeys(entry, true);
    const docs = kit.sections[0].node.children[0];
    expect(opening.has(docs.key)).toBe(true);
    expect(docs.children[0].key).toBe(fileKey);
  });
});

describe("impliedPointerPaths (CHG-FR-AYTR, GSM-FR-MPWK)", () => {
  it("CHG-FR-AYTR: a file of a submodule implies its pointer", () => {
    const implied = impliedPointerPaths(
      [change("vendor/kit/a.md", { repository: KIT.path })],
      [MAIN, KIT],
    );
    expect([...implied]).toEqual(["vendor/kit"]);
  });

  it("CHG-FR-AYTR: a file of a nested submodule implies every pointer up to the main repository", () => {
    const implied = impliedPointerPaths(
      [change("vendor/kit/deep/a.md", { repository: DEEP.path })],
      [MAIN, KIT, DEEP],
    );
    expect([...implied].sort()).toEqual(["vendor/kit", "vendor/kit/deep"]);
  });

  it("CHG-FR-AYTR: a file of the main repository implies no pointer", () => {
    expect(impliedPointerPaths([change("a.md")], [MAIN, KIT]).size).toBe(0);
  });

  it("CHG-FR-AYTR: a parent cycle in a malformed list does not loop", () => {
    const a: RepositoryInfo = { path: "a", name: "a", parent: "b" };
    const b: RepositoryInfo = { path: "b", name: "b", parent: "a" };
    const implied = impliedPointerPaths([change("a/x.md", { repository: "a" })], [MAIN, a, b]);
    expect([...implied].sort()).toEqual(["a", "b"]);
  });
});

describe("commitRepositoryChain (CMW-FR-WZRQ)", () => {
  it("CMW-FR-WZRQ: names the owner, then each parent up to the main repository", () => {
    expect(commitRepositoryChain(DEEP.path, [MAIN, KIT, DEEP])).toEqual([
      { path: "vendor/kit/deep", label: "vendor/kit/deep" },
      { path: "vendor/kit", label: "vendor/kit" },
      { path: "", label: "acme" },
    ]);
    expect(commitRepositoryChain("", [MAIN, KIT])).toEqual([
      { path: "", label: "acme" },
    ]);
  });
});

describe("pushSkipLines (CHG-FR-NUZB)", () => {
  it("CHG-FR-NUZB: labels each skipped repository and states the reason", () => {
    expect(
      pushSkipLines(
        {
          pushed: [],
          skipped: [
            { repository: "vendor/kit", reason: "detached_head" },
            { repository: "", reason: "no_remote" },
          ],
        },
        [MAIN, KIT],
      ),
    ).toEqual([
      { repository: "vendor/kit", label: "vendor/kit", reason: "detached HEAD" },
      { repository: "", label: "acme", reason: "no remote configured" },
    ]);
  });

  it("CHG-FR-NUZB: no summary states nothing", () => {
    expect(pushSkipLines(undefined, [MAIN])).toEqual([]);
  });
});

describe("rollbackFailureText (CHG-FR-65, GTC-FR-DGJV)", () => {
  it("CHG-FR-65: states the submodule pointer failure in words", () => {
    expect(rollbackFailureText("submodule_pointer")).toBe(
      "is a submodule pointer and cannot be rolled back",
    );
  });
});

describe("nodeCheckOf (CHG-FR-28, CHG-FR-AYTR)", () => {
  const pointer = change("vendor/kit", { entryKind: "submodule" });
  const file = change("vendor/a.md");
  const folder = (entries: ReturnType<typeof change>[]) =>
    buildChangeTree(entries, "r:")[0];

  it("CHG-FR-AYTR: an implied pointer row is checked and inert", () => {
    const row = folder([pointer]).children[0];
    expect(nodeCheckOf(row, new Set(), new Set(["vendor/kit"]))).toEqual({
      state: "checked",
      inert: true,
    });
    expect(nodeCheckOf(row, new Set(), new Set())).toEqual({
      state: "unchecked",
      inert: false,
    });
  });

  it("CHG-FR-AYTR, CHG-FR-28: a folder of implied rows alone is checked and inert", () => {
    expect(nodeCheckOf(folder([pointer]), new Set(), new Set(["vendor/kit"]))).toEqual({
      state: "checked",
      inert: true,
    });
  });

  it("CHG-FR-28: a folder ignores its implied rows and counts the toggleable ones", () => {
    const node = folder([pointer, file]);
    const implied = new Set(["vendor/kit"]);
    expect(nodeCheckOf(node, new Set(), implied)).toEqual({ state: "unchecked", inert: false });
    expect(nodeCheckOf(node, new Set([file.path]), implied)).toEqual({
      state: "checked",
      inert: false,
    });
    // Without the implied pointer, one of two rows checked is indeterminate.
    expect(nodeCheckOf(node, new Set([file.path]), new Set()).state).toBe("indeterminate");
    expect(toggleableFilesUnder(node, implied).map((e) => e.path)).toEqual([file.path]);
  });
});

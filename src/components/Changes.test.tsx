import { readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  buildChangeTree,
  canPush,
  commitSetOf,
  folderCheckState,
  partitionEntries,
  visibleFileCount,
} from "./Changes";
import type {
  UpstreamSyncState,
} from "../types";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetPanelReveals } from "../state/panelReveal";
import {
  pickSelector,
} from "../test/selectors";
import {
  HOOK,
  LIB,
  SKILL,
  SPEC,
  change,
  changeSet,
  checkbox,
  cssRule,
  makeBackend,
  renderPanel,
  resetHandlers,
  row,
  rowNames,
  rowsNamed,
  showAllFiles,
} from "../test/changesFixtures";
import { readStylesheet } from "../test/readStylesheet";

const invokeMock = vi.fn();
const unlistenMock = vi.fn();
let listeners: Record<string, (event: { payload: unknown }) => void> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (event: { payload: unknown }) => void) => {
    listeners[event] = cb;
    return unlistenMock;
  }),
}));

const backend = makeBackend(invokeMock);

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  // Module-level, like the preferences cache: reveal nonces are minted from
  // one counter for the app's life, so a suite that does not reset finds its
  // first request already consumed by the previous one.
  resetPanelReveals();
  invokeMock.mockReset();
  unlistenMock.mockReset();
  // The preferences record is a module-level cache shared across the process;
  // a test that leaves a stored action behind would seed the next one.
  resetAppPreferencesCache();
  resetHandlers();
  listeners = {};
  backend();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// Pure tree assembly (CHG-FR-08 / CHG-FR-09)
// ---------------------------------------------------------------------------

describe("buildChangeTree (CHG-FR-08)", () => {
  it("mirrors the folder structure with changed files as the only leaves", () => {
    const tree = buildChangeTree([LIB, HOOK], "t:");
    expect(tree).toHaveLength(1);
    const src = tree[0];
    expect(src.name).toBe("src");
    expect(src.kind).toBe("folder");
    expect(src.children.map((c) => c.name)).toEqual(["components", "hooks"]);
    const components = src.children[0];
    expect(components.children.map((c) => c.name)).toEqual(["Library.tsx"]);
    expect(components.children[0].kind).toBe("file");
    expect(components.children[0].entry).toBe(LIB);
  });

  it("creates no folder without a changed descendant", () => {
    // Only the folders on a changed file's path exist at all — the rule of
    // CHG-FR-08 falls out of building the tree from the filtered entries.
    const tree = buildChangeTree([HOOK], "t:");
    const src = tree[0];
    expect(src.children.map((c) => c.name)).toEqual(["hooks"]);
  });

  it("sorts folders before files, each alphabetically", () => {
    const tree = buildChangeTree(
      [change("z.md"), change("a/b.md"), change("a.md")],
      "t:",
    );
    expect(tree.map((n) => n.name)).toEqual(["a", "a.md", "z.md"]);
  });

  it("keys a file and a folder of the same name apart", () => {
    // A branch comparison can hold a deleted file `a` alongside a new file
    // `a/b.md`. Two sibling nodes named `a` sharing one key would collide in
    // React and share expand state.
    const tree = buildChangeTree(
      [change("a", { changeStatus: "deleted" }), change("a/b.md")],
      "t:",
    );
    const keys = tree.map((n) => n.key);
    expect(new Set(keys).size).toBe(keys.length);
    expect(tree.map((n) => n.kind).sort()).toEqual(["file", "folder"]);
  });

  it("namespaces keys so the same path in two groups keeps independent state", () => {
    // CHG-FR-09 / CHG-FR-17: an `src/components` under Revisioned and one under
    // Unrevisioned must collapse independently.
    const revisioned = buildChangeTree([LIB], "r:");
    const untracked = buildChangeTree([LIB], "u:");
    expect(revisioned[0].key).not.toBe(untracked[0].key);
  });
});

describe("partitionEntries (CHG-FR-09 / CHG-FR-16)", () => {
  it("splits the entries into the two groups and filters both identically", () => {
    const fresh = change("src/components/Changes.tsx", {
      changeStatus: "untracked",
    });
    const all = [LIB, fresh, SKILL];
    const everything = partitionEntries(all, "files", "");
    expect(everything.unrevisioned).toEqual([fresh]);
    expect(everything.revisioned).toEqual([LIB, SKILL]);

    // The type lens applies inside each group exactly as it does in the other.
    const artifactsOnly = partitionEntries(all, "artifacts", "");
    expect(artifactsOnly.unrevisioned).toEqual([]);
    expect(artifactsOnly.revisioned).toEqual([SKILL]);
  });

  it("puts every status Git already tracks under Revisioned", () => {
    // CHG-FR-09 splits on "does Git track this", not on "is this a plain edit".
    // A staged new file is `added` and is tracked, so it belongs with the rest
    // of the session's edits rather than under Unrevisioned.
    for (const status of ["added", "modified", "deleted", "renamed"] as const) {
      const entry = change(`src/${status}.ts`, { changeStatus: status });
      const split = partitionEntries([entry], "files", "");
      expect(split.revisioned, `${status} belongs under Revisioned`).toEqual([
        entry,
      ]);
      expect(split.unrevisioned).toEqual([]);
    }
    const fresh = change("src/fresh.ts", { changeStatus: "untracked" });
    const split = partitionEntries([fresh], "files", "");
    expect(split.unrevisioned).toEqual([fresh]);
    expect(split.revisioned).toEqual([]);
  });

  it("AND-combines the type lens with the text filter", () => {
    const all = [SKILL, SPEC];
    expect(partitionEntries(all, "skill", "").revisioned).toEqual([SKILL]);
    expect(partitionEntries(all, "skill", "CHG").revisioned).toEqual([]);
    expect(partitionEntries(all, "files", "CHG").revisioned).toEqual([SPEC]);
  });
});

// ---------------------------------------------------------------------------
// Modes (CHG-FR-02, CHG-FR-03, CHG-FR-04 / CHG-FR-05 / CHG-FR-06 / CHG-FR-07)
// ---------------------------------------------------------------------------

describe("modes (CHG-FR-02..FR-07)", () => {
  it("lists uncommitted changes by default and branch changes after the toggle", async () => {
    renderPanel();
    await waitFor(() => expect(calls("list_uncommitted_changes")).toHaveLength(1));
    expect(calls("list_branch_changes")).toHaveLength(0);

    await userEvent.click(screen.getByRole("radio", { name: "Branch" }));
    await waitFor(() => expect(calls("list_branch_changes")).toHaveLength(1));
    // The two modes are mutually exclusive.
    expect(screen.getByRole("radio", { name: "Branch" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Uncommitted" })).not.toBeChecked();
  });

  it("shows the target-branch picker in Branch mode only, populated from the backend", async () => {
    renderPanel();
    await waitFor(() => expect(calls("list_uncommitted_changes")).toHaveLength(1));
    expect(screen.queryByLabelText("Target branch")).not.toBeInTheDocument();

    await userEvent.click(screen.getByRole("radio", { name: "Branch" }));
    const picker = await screen.findByLabelText<HTMLSelectElement>("Target branch");
    await waitFor(() =>
      expect(
        within(picker)
          .getAllByRole("option")
          .map((o) => (o as HTMLOptionElement).value),
      ).toContain("feature/x"),
    );
  });

  it("takes the initial target branch from the repository default (CHG-FR-06)", async () => {
    backend({ panelState: { mode: "uncommitted" }, defaultBranch: "trunk" });
    renderPanel();
    await waitFor(() => expect(calls("get_default_branch")).toHaveLength(1));

    await userEvent.click(screen.getByRole("radio", { name: "Branch" }));
    await waitFor(() =>
      expect(calls("list_branch_changes")[0][1]).toEqual({ targetBranch: "trunk" }),
    );
  });

  it("restores the persisted mode and target branch (CHG-FR-07)", async () => {
    backend({ panelState: { mode: "branch", targetBranch: "develop" } });
    renderPanel();

    await waitFor(() => expect(calls("list_branch_changes")).toHaveLength(1));
    expect(calls("list_branch_changes")[0][1]).toEqual({ targetBranch: "develop" });
    expect(calls("get_default_branch")).toHaveLength(0);
    expect(screen.getByRole("radio", { name: "Branch" })).toBeChecked();
    expect(await screen.findByLabelText<HTMLSelectElement>("Target branch")).toHaveValue(
      "develop",
    );
  });

  it("persists the mode and target branch when they change (CHG-FR-07)", async () => {
    backend({ defaultBranch: "main" });
    renderPanel();
    await waitFor(() => expect(calls("list_uncommitted_changes")).toHaveLength(1));

    await userEvent.click(screen.getByRole("radio", { name: "Branch" }));
    await waitFor(() => {
      const saved = calls("save_changes_panel_state").map((c) => c[1]);
      expect(saved).toContainEqual({
        state: { mode: "branch", targetBranch: "main" },
      });
    });
  });

  it("re-renders against the newly selected branch (CHG-FR-05)", async () => {
    renderPanel();
    await userEvent.click(screen.getByRole("radio", { name: "Branch" }));
    const picker = await screen.findByLabelText<HTMLSelectElement>("Target branch");
    await waitFor(() => expect(picker.options.length).toBeGreaterThan(1));

    await userEvent.selectOptions(picker, "feature/x");
    await waitFor(() =>
      expect(
        calls("list_branch_changes").map((c) => (c[1] as { targetBranch: string }).targetBranch),
      ).toContain("feature/x"),
    );
  });
});

// ---------------------------------------------------------------------------
// Tree rendering (CHG-FR-08 .. CHG-FR-13)
// ---------------------------------------------------------------------------

describe("tree rendering", () => {
  it("groups changed files per folder (CHG-FR-08)", async () => {
    backend({ uncommitted: changeSet([LIB, HOOK]) });
    renderPanel();
    await showAllFiles();

    await screen.findByText("Library.tsx");
    // The `src` beneath **Revisioned** holds both folders (CHG-FR-09).
    expect(rowNames()).toEqual([
      "Revisioned",
      "src",
      "components",
      "Library.tsx",
      "hooks",
      "useEditHistory.ts",
    ]);
  });

  it("splits the tree into Revisioned and Unrevisioned, Revisioned first (CHG-FR-09)", async () => {
    const fresh = change("src/components/Changes.tsx", {
      changeStatus: "untracked",
      addedLines: 142,
      removedLines: 0,
    });
    const shell = change("src/components/Shell.tsx", { addedLines: 3, removedLines: 1 });
    backend({ uncommitted: changeSet([fresh, shell]) });
    renderPanel();
    await showAllFiles();

    await screen.findByText("Unrevisioned");
    // Each group carries its own `src` > `components` nesting, Revisioned
    // leads, and no changed file is rendered outside either group.
    expect(rowNames()).toEqual([
      "Revisioned",
      "src",
      "components",
      "Shell.tsx",
      "Unrevisioned",
      "src",
      "components",
      "Changes.tsx",
    ]);
  });

  it("renders both group labels in a heavier weight than the folders (CHG-FR-09)", async () => {
    const fresh = change("src/new.ts", { changeStatus: "untracked" });
    backend({ uncommitted: changeSet([LIB, fresh]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Unrevisioned");

    for (const name of ["Revisioned", "Unrevisioned"]) {
      expect(row(name).className).toContain("tree-row--group");
    }
    // The folder nodes beneath them do not take that treatment.
    expect(row("src").className).not.toContain("tree-row--group");

    // `vitest.config.ts` sets `css: false`, so jsdom loads no stylesheet and
    // the class alone proves nothing about weight — the rule itself is asserted
    // against the real file, the way `src/styles/fontRoles.test.ts` does it.
    const css = readStylesheet("components.css");
    expect(cssRule(css, ".tree-row--group .tree-row__name")).toMatch(
      /font-weight\s*:\s*var\(--fw-semi\)/,
    );
    // And heavier *than the folders beneath it*: the rows those groups wrap
    // declare no weight of their own, so nothing else is competing.
    expect(cssRule(css, ".tree-row")).not.toMatch(/font-weight/);
    expect(cssRule(css, ".tree-row__name")).not.toMatch(/font-weight/);
  });

  it("keeps its group label when it is the only group with content (CHG-FR-09)", async () => {
    // Nothing untracked at all: the tracked changes still sit under Revisioned
    // rather than flattening to the top level, so the select-all checkbox is
    // where it always is.
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await showAllFiles();

    await screen.findByText("Library.tsx");
    expect(rowNames()).toEqual(["Revisioned", "src", "components", "Library.tsx"]);
    expect(screen.queryByText("Unrevisioned")).not.toBeInTheDocument();
  });

  it("collapses the same path in the two groups independently (CHG-FR-09 / CHG-FR-17)", async () => {
    // Each group builds its tree under its own key namespace, so the `src`
    // under Revisioned and the `src` under Unrevisioned are two nodes with two
    // collapse states — one shared key would collapse and select both at once.
    const shell = change("src/components/Shell.tsx");
    const fresh = change("src/components/Changes.tsx", {
      changeStatus: "untracked",
    });
    backend({ uncommitted: changeSet([shell, fresh]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Unrevisioned");
    expect(rowsNamed("src")).toHaveLength(2);

    // Collapse the one under Revisioned: its file goes, the other group's stays.
    await userEvent.click(rowsNamed("src")[0]);
    expect(rowNames()).not.toContain("Shell.tsx");
    expect(rowNames()).toContain("Changes.tsx");
    // And only one of the two rows took the selection.
    expect(
      rowsNamed("src").filter((r) => r.getAttribute("data-selected") === "true"),
    ).toHaveLength(1);

    // Then the one under Unrevisioned, leaving both groups collapsed to their
    // own labels plus the folder rows that hold the collapse.
    await userEvent.click(rowsNamed("src")[1]);
    expect(rowNames()).toEqual(["Revisioned", "src", "Unrevisioned", "src"]);
  });

  it("collapses each group independently of the other (CHG-FR-17)", async () => {
    const fresh = change("docs/draft.md", { changeStatus: "untracked" });
    backend({ uncommitted: changeSet([LIB, fresh]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Unrevisioned");

    await userEvent.click(row("Revisioned"));
    expect(rowNames()).not.toContain("Library.tsx");
    // The other group is untouched by that collapse.
    expect(rowNames()).toContain("draft.md");

    await userEvent.click(row("Unrevisioned"));
    expect(rowNames()).toEqual(["Revisioned", "Unrevisioned"]);
  });

  it("shows the diffstat, and a binary marker in its place (CHG-FR-10, CHG-FR-11)", async () => {
    const binary = change("src-tauri/Cargo.lock", {
      isBinary: true,
      addedLines: null,
      removedLines: null,
    });
    backend({ uncommitted: changeSet([LIB, binary]) });
    renderPanel();
    await showAllFiles();

    await screen.findByText("Library.tsx");
    const stat = row("Library.tsx").querySelector(".change-row__stat")!;
    expect(stat.textContent).toContain("+18");
    expect(stat.textContent).toContain("−4");

    const binaryStat = row("Cargo.lock").querySelector(".change-row__stat")!;
    expect(binaryStat.textContent).toContain("binary");
    expect(binaryStat.textContent).not.toMatch(/[+−]\d/);
  });

  it("places a renamed entry at its current path and shows its previous one (CHG-FR-12)", async () => {
    const renamed = change("src-tauri/lib.rs", {
      changeStatus: "renamed",
      previousPath: "src-tauri/main.rs",
      addedLines: 30,
      removedLines: 7,
    });
    backend({ uncommitted: changeSet([renamed]) });
    renderPanel();
    await showAllFiles();

    await screen.findByText("src-tauri");
    const node = row("lib.rs (was src-tauri/main.rs)");
    expect(node.textContent).toContain("was src-tauri/main.rs");
    // Its position in the tree is its *current* path.
    expect(rowNames()).toEqual([
      "Revisioned",
      "src-tauri",
      "lib.rs (was src-tauri/main.rs)",
    ]);
  });

  it("tags a classified file and leaves an unclassified one untagged (CHG-FR-13)", async () => {
    const plain = change("package.json");
    backend({ uncommitted: changeSet([SPEC, plain]) });
    renderPanel();
    await pickSelector("Filter by type", "files");

    await screen.findByText("package.json");
    expect(row("CHG-changes.md").querySelector(".chip-type")).not.toBeNull();
    expect(row("package.json").querySelector(".chip-type")).toBeNull();
  });
});

describe("visibleFileCount (CHG-FR-50)", () => {
  it("counts the files at any depth beneath a node", () => {
    const tree = buildChangeTree([LIB, HOOK, change("src/index.ts")], "t:");
    // `src`, holding a direct file and two subfolders each holding one.
    expect(visibleFileCount(tree[0])).toBe(3);
    const components = tree[0].children.find((n) => n.name === "components")!;
    expect(visibleFileCount(components)).toBe(1);
  });

  it("counts a file node as itself and an empty folder as nothing", () => {
    const tree = buildChangeTree([LIB], "t:");
    const file = tree[0].children[0].children[0];
    expect(file.kind).toBe("file");
    expect(visibleFileCount(file)).toBe(1);
    // Defensive: the tree never renders a childless folder (CHG-FR-08), so this
    // is the shape the requirement's "no count of zero" rests on.
    expect(
      visibleFileCount({
        key: "k",
        name: "empty",
        path: "empty",
        kind: "folder",
        children: [],
      }),
    ).toBe(0);
  });
});

// ---------------------------------------------------------------------------
// Pure check helpers (CHG-FR-27 / CHG-FR-28 / CHG-FR-37)
// ---------------------------------------------------------------------------

describe("folderCheckState (CHG-FR-28)", () => {
  it("is checked, unchecked, or indeterminate by how many descendants are ticked", () => {
    const tree = buildChangeTree([LIB, HOOK], "t:");
    const src = tree[0];
    expect(folderCheckState(src, new Set())).toBe("unchecked");
    expect(folderCheckState(src, new Set([LIB.path]))).toBe("indeterminate");
    expect(folderCheckState(src, new Set([LIB.path, HOOK.path]))).toBe("checked");
  });

  it("counts only the descendants the filters left visible", () => {
    // The tree is built from filtered entries, so a hidden sibling is simply
    // not in it — which is what makes a folder read "checked" while an unticked
    // but hidden file exists beneath it on disk (CHG-FR-27).
    const visibleOnly = buildChangeTree([LIB], "t:");
    expect(folderCheckState(visibleOnly[0], new Set([LIB.path]))).toBe("checked");
  });
});

describe("commitSetOf (CHG-FR-27)", () => {
  it("is the intersection of checked and currently visible", () => {
    const set = commitSetOf([LIB, SKILL], new Set([LIB.path, HOOK.path]));
    expect(set).toEqual([{ path: LIB.path, untracked: false }]);
  });

  it("marks an untracked entry so the window can say so (CMW-FR-06)", () => {
    const fresh = change("src/new.ts", { changeStatus: "untracked" });
    expect(commitSetOf([fresh], new Set([fresh.path]))).toEqual([
      { path: fresh.path, untracked: true },
    ]);
  });
});

describe("canPush (CHG-FR-37)", () => {
  it("offers a push only when the branch holds commits the remote does not", () => {
    const state = (over: Partial<UpstreamSyncState>): UpstreamSyncState => ({
      hasRemote: true,
      hasUpstream: true,
      ahead: 0,
      behind: 0,
      ...over,
    });
    // Level with the upstream, and behind it, are both nothing to publish.
    expect(canPush(state({}))).toBe(false);
    expect(canPush(state({ ahead: 0, behind: 3 }))).toBe(false);
    expect(canPush(state({ ahead: 2 }))).toBe(true);
    // Never published, with a remote configured: the push publishes it.
    expect(canPush(state({ hasUpstream: false, ahead: null, behind: null }))).toBe(
      true,
    );
    // No remote at all — and no reading yet.
    expect(canPush(state({ hasRemote: false, hasUpstream: false }))).toBe(false);
    expect(canPush(null)).toBe(false);
  });
});

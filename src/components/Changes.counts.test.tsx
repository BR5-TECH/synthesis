import { readFileSync } from "node:fs";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  buildChangeTree,
} from "./Changes";
import { CHANGES_UPDATED } from "../events";
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
  onOpenDiff,
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
// Folder and group counts (CHG-FR-50, CHG-FR-51, CHG-FR-26 .. CHG-FR-53)
// ---------------------------------------------------------------------------

/** The count stated beside the row labelled `name` (CHG-FR-50). */
function countOn(name: string): string | null {
  return row(name).querySelector(".tree-row__count")?.textContent ?? null;
}

/** The count on each row labelled `name`, in render order. */
function countsOn(name: string): (string | null)[] {
  return rowsNamed(name).map(
    (r) => r.querySelector(".tree-row__count")?.textContent ?? null,
  );
}

describe("folder and group counts (CHG-FR-50..FR-53)", () => {
  it("counts every visible file beneath a node, at any depth (CHG-FR-50, CHG-FR-51, CHG-FR-26)", async () => {
    const shell = change("src/components/Shell.tsx");
    const rs = change("src-tauri/lib.rs");
    backend({ uncommitted: changeSet([LIB, shell, rs, SPEC]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    // Recursive over the whole subtree rather than over direct children: `src`
    // holds no file of its own and still states the two beneath `components`.
    expect(countOn("Revisioned")).toBe("4 files");
    expect(countOn("src")).toBe("2 files");
    expect(countOn("components")).toBe("2 files");
    expect(countOn("src-tauri")).toBe("1 file");
    expect(countOn("specifications")).toBe("1 file");
    // A file row states no count — it is one file, and the number would read as
    // part of the diffstat beside it.
    expect(countOn("Library.tsx")).toBeNull();
  });

  it("states the noun it counts, agreeing in number (CHG-FR-51)", async () => {
    // A bare digit beside a folder leaves the reader to work out what it
    // measures; the noun is what makes the row read as a sentence.
    let entries = [change("src/one.ts")];
    backend({ uncommitted: () => changeSet(entries) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("one.ts");
    expect(countOn("src")).toBe("1 file");
    expect(countOn("Revisioned")).toBe("1 file");

    entries = [...entries, change("src/two.ts")];
    act(() => {
      listeners[CHANGES_UPDATED]?.({ payload: undefined });
    });
    await waitFor(() => expect(countOn("src")).toBe("2 files"));

    // Plural all the way up, including past a single digit.
    entries = Array.from({ length: 13 }, (_, i) => change(`src/f${i}.ts`));
    act(() => {
      listeners[CHANGES_UPDATED]?.({ payload: undefined });
    });
    await waitFor(() => expect(countOn("src")).toBe("13 files"));
  });

  it("counts each group's own subtree, and only its own (CHG-FR-50, CHG-FR-51, CHG-FR-26)", async () => {
    const fresh = change("src/components/Changes.tsx", { changeStatus: "untracked" });
    const draft = change("docs/draft.md", { changeStatus: "untracked" });
    backend({ uncommitted: changeSet([LIB, fresh, draft]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Unrevisioned");

    expect(countOn("Revisioned")).toBe("1 file");
    expect(countOn("Unrevisioned")).toBe("2 files");
    // The `src` in each group counts that group's files alone.
    expect(countsOn("src")).toEqual(["1 file", "1 file"]);
  });

  it("keeps stating its count while collapsed (CHG-FR-50, CHG-FR-51, CHG-FR-26)", async () => {
    const shell = change("src/components/Shell.tsx");
    backend({ uncommitted: changeSet([LIB, shell]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    expect(countOn("src")).toBe("2 files");

    // A folder answers "how much is in here?" without being opened — which is
    // exactly when the question is worth asking.
    await userEvent.click(row("src"));
    expect(rowNames()).not.toContain("Library.tsx");
    expect(countOn("src")).toBe("2 files");
  });

  it("states counts in Branch mode, which carries no checkbox (CHG-FR-50, CHG-FR-51, CHG-FR-26)", async () => {
    backend({
      panelState: { mode: "branch", targetBranch: "main" },
      branchChanges: changeSet([SPEC, SKILL], "main"),
    });
    renderPanel();
    await screen.findByText("CHG-changes.md");

    expect(countOn("Revisioned")).toBe("2 files");
    expect(countOn("specifications")).toBe("1 file");
    // CHG-FR-26: no checkbox anywhere, and the counts are there regardless.
    expect(document.querySelectorAll("input[type=checkbox]")).toHaveLength(0);
  });

  it("counts what the filters admit, not what the change set holds (CHG-FR-52)", async () => {
    // One classified file and three unclassified ones in the same folder.
    const a = change("src/a.ts");
    const b = change("src/b.ts");
    const c = change("src/c.ts");
    const spec = change("src/SPEC.md", { artifactType: "spec", typeSource: "inferred" });
    backend({ uncommitted: changeSet([a, b, c, spec]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("a.ts");
    expect(countOn("src")).toBe("4 files");

    // Under the default lens the folder states the one row it now reveals.
    await pickSelector("Filter by type", "artifacts");
    await waitFor(() => expect(countOn("src")).toBe("1 file"));
    expect(rowNames()).toEqual(["Revisioned", "src", "SPEC.md"]);
  });

  it("narrows a count by the text filter as well as the lens (CHG-FR-52)", async () => {
    const a = change("src/alpha.ts");
    const b = change("src/beta.ts");
    backend({ uncommitted: changeSet([a, b]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("alpha.ts");
    expect(countOn("src")).toBe("2 files");

    // A fragment matching one of the two leaves the folder standing, counting
    // the one it still reveals — the two filters narrow a count alike.
    await userEvent.type(screen.getByLabelText("Filter changes"), "alph");
    await waitFor(() => expect(countOn("src")).toBe("1 file"));
    expect(rowNames()).toEqual(["Revisioned", "src", "alpha.ts"]);
  });

  it("is never rendered stating zero (CHG-FR-52)", async () => {
    const a = change("src/a.ts");
    backend({ uncommitted: changeSet([a]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("a.ts");
    // A count is on screen to begin with, so what follows is the feature going
    // to zero rather than the feature being absent.
    expect(countOn("src")).toBe("1 file");
    expect(countOn("Revisioned")).toBe("1 file");

    // A text filter matching nothing empties the tree rather than leaving a
    // folder — or a group — behind stating 0 (CHG-FR-08 / CHG-FR-16).
    await userEvent.type(screen.getByLabelText("Filter changes"), "zzz");
    await waitFor(() => expect(rowNames()).toEqual([]));
    expect(document.querySelectorAll(".tree-row__count")).toHaveLength(0);
  });

  it("re-counts on a reload, keeping the tree as the author left it (CHG-FR-52)", async () => {
    const shell = change("src/components/Shell.tsx");
    let entries = [LIB, shell];
    backend({ uncommitted: () => changeSet(entries) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    expect(countOn("components")).toBe("2 files");

    // CHG-FR-19: a third file arrives on the debounced backend event.
    entries = [LIB, shell, change("src/components/Panel.tsx")];
    act(() => {
      listeners[CHANGES_UPDATED]?.({ payload: undefined });
    });
    await waitFor(() => expect(countOn("components")).toBe("3 files"));
    expect(countOn("Revisioned")).toBe("3 files");
  });

  it("states what a tick on that node takes (CHG-FR-50, CHG-FR-51, CHG-FR-26 / CHG-FR-28)", async () => {
    const shell = change("src/components/Shell.tsx");
    const rs = change("src-tauri/lib.rs");
    backend({ uncommitted: changeSet([LIB, shell, rs]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    expect(countOn("components")).toBe("2 files");

    // The number is honest about the cascade: ticking the folder takes exactly
    // as many files as it says.
    await userEvent.click(checkbox("components"));
    expect(screen.getByText("2 files selected")).toBeInTheDocument();

    // And unticking one leaves the folder indeterminate while the count — which
    // describes what is visible, not what is ticked — stands.
    await userEvent.click(checkbox("Library.tsx"));
    expect(checkbox("components").indeterminate).toBe(true);
    expect(countOn("components")).toBe("2 files");
    expect(screen.getByText("1 file selected")).toBeInTheDocument();
  });

  it("counts what is visible, not what is checked (CHG-FR-52 / CHG-FR-29)", async () => {
    const plain = change("src/package.json");
    backend({ uncommitted: changeSet([SPEC, plain]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("package.json");
    await userEvent.click(checkbox("package.json"));
    expect(countOn("Revisioned")).toBe("2 files");

    // CHG-FR-29 retains the check while the filters hide the row; the count
    // must not: it describes the tree, and the hidden row is not in it.
    await pickSelector("Filter by type", "artifacts");
    await waitFor(() => expect(countOn("Revisioned")).toBe("1 file"));
    expect(screen.getByText("0 files selected")).toBeInTheDocument();
  });

  it("re-counts without going back to the backend (CHG-FR-52)", async () => {
    const plain = change("src/package.json");
    backend({ uncommitted: changeSet([SPEC, plain]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("package.json");
    const before = calls("list_uncommitted_changes").length;

    await pickSelector("Filter by type", "artifacts");
    await waitFor(() => expect(countOn("Revisioned")).toBe("1 file"));
    // The count is derived from the tree already on screen, so narrowing it
    // costs no additional call.
    expect(calls("list_uncommitted_changes")).toHaveLength(before);
  });

  it("counts a folder's own files alongside its subfolders' (CHG-FR-50, CHG-FR-51, CHG-FR-26)", async () => {
    // A folder holding both a direct file and a subfolder: a count that walked
    // only the subfolders, or only the direct children, would each be wrong.
    const index = change("src/index.ts");
    const shell = change("src/components/Shell.tsx");
    const readme = change("README.md");
    backend({ uncommitted: changeSet([index, shell, LIB, readme]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("index.ts");

    expect(countOn("src")).toBe("3 files");
    expect(countOn("components")).toBe("2 files");
    // A root-level file sits in no folder at all and still reaches the group's
    // total, which is the only place it is counted.
    expect(countOn("Revisioned")).toBe("4 files");
  });

  it("counts a renamed entry once, at its current path (CHG-FR-50, CHG-FR-51, CHG-FR-26 / CHG-FR-12)", async () => {
    const renamed = change("app/lib.rs", {
      changeStatus: "renamed",
      previousPath: "old/main.rs",
    });
    backend({ uncommitted: changeSet([renamed]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("app");

    expect(countOn("app")).toBe("1 file");
    expect(countOn("Revisioned")).toBe("1 file");
    // The path it came from is not a second place in the tree to count it.
    expect(rowNames()).not.toContain("old");
  });

  it("counts on a folder and not on the file beside it that shares its name (CHG-FR-50)", async () => {
    // `buildChangeTree` deliberately keys a folder and a file of the same name
    // apart; the count is gated on that same discriminator.
    const folderChild = change("a/b.md");
    const file = change("a", { changeStatus: "untracked" });
    backend({ uncommitted: changeSet([folderChild, file]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("b.md");

    const [inRevisioned, inUnrevisioned] = rowsNamed("a");
    expect(inRevisioned.querySelector(".tree-row__count")?.textContent).toBe("1 file");
    // The untracked file named `a` is a leaf, and leaves state no count.
    expect(inUnrevisioned.querySelector(".tree-row__count")).toBeNull();
  });

  it("states the number of file rows expanding it reveals, whatever the lens (CHG-FR-52)", async () => {
    // The property the requirement actually makes: for every rendered folder or
    // group, its count is the number of file rows in its subtree. Asserted over
    // a whole tree rather than one node, under each lens.
    const entries = [
      LIB,
      HOOK,
      SPEC,
      SKILL,
      change("src/index.ts"),
      change("docs/new.md", { changeStatus: "untracked" }),
      change("docs/guide/intro.md", { changeStatus: "untracked" }),
    ];
    backend({ uncommitted: changeSet(entries) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    /** Every rendered row's depth, count, and whether it is a file. */
    const walk = () =>
      Array.from(document.querySelectorAll<HTMLElement>(".tree-row")).map((el) => ({
        name: (el.querySelector(".tree-row__name")?.textContent ?? "").trim(),
        count: el.querySelector(".tree-row__count")?.textContent ?? null,
        indent: Number.parseInt(el.style.paddingLeft || "4", 10),
        isFile: el.querySelector(".change-row__stat") != null,
      }));

    const check = () => {
      const rows = walk();
      rows.forEach((r, i) => {
        if (r.count == null) return;
        // The subtree is the following rows indented deeper than this one.
        let files = 0;
        for (let j = i + 1; j < rows.length && rows[j].indent > r.indent; j++) {
          if (rows[j].isFile) files += 1;
        }
        expect(r.count, `${r.name} states ${r.count}`).toBe(
          `${files} ${files === 1 ? "file" : "files"}`,
        );
      });
      // And the walk actually saw counted rows, so this cannot pass vacuously.
      expect(rows.filter((r) => r.count != null).length).toBeGreaterThan(3);
    };

    check();
    await pickSelector("Filter by type", "artifacts");
    await waitFor(() => expect(rowNames()).not.toContain("Library.tsx"));
    check();
  });

  it("sets the count a step below its label, recessive, and tracking the role (CHG-FR-53)", async () => {
    // `vitest.config.ts` sets `css: false`, so jsdom applies no stylesheet and
    // the markup alone proves nothing about size or colour — the rules are
    // asserted against the real file, as the group-weight test above does.
    const css = readStylesheet("components.css");
    const count = cssRule(css, ".tree-row__count");

    // A step below `.tree-row`'s own `--fs-ui-sm`: a point smaller at the
    // role's default size, and a ratio of it rather than a size of its own, so
    // label and count move together when the author changes the UI role. That
    // both steps are ratios of `--font-ui-size` is enforced corpus-wide by
    // `src/styles/fontRoles.test.ts`, so it is not re-asserted here.
    expect(cssRule(css, ".tree-row")).toMatch(/font-size\s*:\s*var\(--fs-ui-sm\)/);
    expect(count).toMatch(/font-size\s*:\s*var\(--fs-ui-xs\)/);
    // The group label takes the same size as a folder's, so "a step below the
    // label" means the same step on both kinds of counted row.
    expect(cssRule(css, ".tree-row--group .tree-row__name")).not.toMatch(
      /font-size/,
    );
    // Recessive against the label, which takes `--fg-1`.
    expect(count).toMatch(/color\s*:\s*var\(--fg-3\)/);
    expect(cssRule(css, ".tree-row")).toMatch(/color\s*:\s*var\(--fg-1\)/);
    // Two words, on a fixed-height row: it must not wrap.
    expect(count).toMatch(/white-space\s*:\s*nowrap/);

    // How the row degrades as the panel narrows: the label absorbs it, because
    // the leftover width lands after the count and the count does not shrink.
    // A count is whole or it is nothing — measured in the browser, a shrinkable
    // count reads `2 fi…` at ordinary widths, which says less than the numeral
    // it replaced. This is the rule the diffstat on a file row already narrows
    // by, so the two kinds of row behave alike.
    expect(count).toMatch(/margin-right\s*:\s*auto/);
    expect(count).toMatch(/flex\s*:\s*none/);
    expect(count).not.toMatch(/text-overflow/);
    expect(cssRule(css, ".tree-row__name")).toMatch(/min-width\s*:\s*0/);
    expect(cssRule(css, ".tree-row__name")).toMatch(/text-overflow\s*:\s*ellipsis/);
    // Compound rather than a bare modifier class: `.tree-row__name` sets
    // `flex: 1` at equal specificity, so a single class would win on source
    // order alone and moving either rule would take the truncation away.
    expect(cssRule(css, ".tree-row__name.tree-row__name--counted")).toMatch(
      /flex\s*:\s*0 1 auto/,
    );
  });

  it("adds no activation target of its own (CHG-FR-53)", async () => {
    const shell = change("src/components/Shell.tsx");
    backend({ uncommitted: changeSet([LIB, shell]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    // Start from a ticked folder, so a click that changed the check in either
    // direction would show.
    await userEvent.click(checkbox("src"));
    expect(screen.getByText("2 files selected")).toBeInTheDocument();

    const countEl = () => row("src").querySelector(".tree-row__count")!;
    expect(countEl().tagName).toBe("SPAN");
    // A click on it does what a click on the label does — collapse the folder —
    // and neither ticks anything nor opens a Diff tab.
    await userEvent.click(countEl());
    expect(rowNames()).not.toContain("Library.tsx");
    expect(checkbox("src").checked).toBe(true);
    expect(screen.getByText("2 files selected")).toBeInTheDocument();
    expect(onOpenDiff).not.toHaveBeenCalled();

    // Symmetric, as a click on the label is: the second one re-expands.
    await userEvent.click(countEl());
    expect(rowNames()).toContain("Library.tsx");
  });

  it("adds no activation target on a group row either (CHG-FR-53)", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    await userEvent.click(row("Revisioned").querySelector(".tree-row__count")!);
    expect(rowNames()).toEqual(["Revisioned"]);
    expect(checkbox("Revisioned").checked).toBe(false);
    expect(onOpenDiff).not.toHaveBeenCalled();
  });
});

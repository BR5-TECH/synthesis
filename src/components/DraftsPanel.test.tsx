/**
 * The Drafts panel: its path helpers, the tree it renders, the two filters, and
 * the menus its rows open (`../../specifications/ui/DRP-drafts-panel.md`).
 *
 * Folder editing, moves, expansion, the row behaviour and the reveal each live
 * in a `DraftsPanel.<topic>.test.tsx` sibling.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  act,
  cleanup,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/**
 * The panel opens surfaces that subscribe to backend channels — the Information
 * modal listens for `"draft statistics changed"` (DFI-FR-ZGBU) — and `listen`
 * reaches for a Tauri IPC context that does not exist under jsdom. Mocked
 * beside `invoke` for the same reason `invoke` is: a UI test must not need the
 * Tauri runtime, and an un-mocked subscription rejects on a microtask nothing
 * in the test is awaiting, which fails the run without failing an assertion.
 */
const listenMock = vi.fn(async (_name: string, _handler: unknown) => () => {});
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: unknown) => listenMock(name, handler),
}));

import {
  DRAFTS,
  draftRow,
  flat,
  folderRow,
  nested,
  openDraftMenu,
  openFolderMenu,
  renderPanel,
} from "../test/draftsPanelFixtures";
import {
  ancestorsOf,
  countPhrase,
  folderJoin,
  folderName,
  folderNameProblem,
  folderParent,
  isWithin,
  itemKey,
  toMatchMap,
} from "./DraftsPanel";
import type { DraftSummary, GraduationRunState } from "../types";
import { resetPanelReveals } from "../state/panelReveal";

/** Every `invoke` of one operation, in the order they were made. */
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  // Module-level, like the preferences cache: reveal nonces are minted from
  // one counter for the app's life, so a suite that does not reset finds its
  // first request already consumed by the previous one.
  resetPanelReveals();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "list_drafts") return flat();
    // DRS-FR-17: the real backend matches names *and* file contents. This
    // stand-in matches names alone, which is enough for every test that is not
    // about content matching; the ones that are override it.
    if (cmd === "search_drafts") {
      const text = String((args as { text: string }).text).toLowerCase();
      return DRAFTS.filter((d) => d.name.toLowerCase().includes(text)).map(
        (d) => ({ draftId: d.id, matchedIn: "name" }),
      );
    }
    return undefined;
  });
});
afterEach(cleanup);

// ---------------------------------------------------------------------------
// Pure helpers — the rules the drop feedback and the picker both read from
// ---------------------------------------------------------------------------

describe("path helpers", () => {
  it("names, parents and ancestors of a drafts-root-relative path", () => {
    expect(folderName("UI/Components")).toBe("Components");
    expect(folderName("UI")).toBe("UI");
    expect(folderParent("UI/Components")).toBe("UI");
    expect(folderParent("UI")).toBe("");
    expect(ancestorsOf("a/b/c")).toEqual(["a", "a/b"]);
    expect(ancestorsOf("a")).toEqual([]);
  });

  it("DRP-FR-27: containment is what refuses a folder into its own subtree", () => {
    expect(isWithin("UI/Components", "UI")).toBe(true);
    expect(isWithin("UI", "UI")).toBe(true);
    expect(isWithin("UIX", "UI")).toBe(false);
    // Everything is inside the implicit root.
    expect(isWithin("UI/Components", "")).toBe(true);
  });

  it("DRP-FR-23: a malformed name is refused in the panel, before any call", () => {
    expect(folderNameProblem("")).not.toBeNull();
    expect(folderNameProblem("   ")).not.toBeNull();
    expect(folderNameProblem("a/b")).not.toBeNull();
    expect(folderNameProblem("a\\b")).not.toBeNull();
    expect(folderNameProblem("..")).not.toBeNull();
    expect(folderNameProblem("Components")).toBeNull();
  });

  it("DRP-FR-25: an empty count is elided rather than stated as zero", () => {
    expect(countPhrase(2, 1)).toBe("The 2 drafts and 1 folder it holds move");
    // Eliding the zero leaves a singular subject, so the verb agrees with it.
    expect(countPhrase(0, 1)).toBe("The 1 folder it holds moves");
    expect(countPhrase(1, 0)).toBe("The 1 draft it holds moves");
    expect(countPhrase(0, 2)).toBe("The 2 folders it holds move");
    expect(countPhrase(0, 0)).toBe("Nothing it holds moves");
  });

  it("folderJoin treats the implicit root as no prefix at all", () => {
    expect(folderJoin("", "UI")).toBe("UI");
    expect(folderJoin("UI", "Components")).toBe("UI/Components");
  });

  it("item keys tell a draft and a folder of the same name apart", () => {
    expect(itemKey({ kind: "folder", path: "UI" })).not.toEqual(
      itemKey({ kind: "draft", id: "UI", name: "UI", folder: "" }),
    );
  });

  it("toMatchMap keys the hits by draft", () => {
    expect(toMatchMap([{ draftId: "a", matchedIn: "contents" }]).get("a")).toBe(
      "contents",
    );
  });
});

describe("Drafts panel (DRP-drafts-panel.md)", () => {
  it("DRP-FR-01, DRP-FR-02: renders from `list_drafts` and reads no draft's contents", async () => {
    renderPanel();
    await screen.findByText("artifact-window");

    expect(invokeMock.mock.calls.map((c) => c[0])).toEqual(["list_drafts"]);
  });

  it("DRP-FR-03: a row carries a relative timestamp and no file count", async () => {
    renderPanel();
    await screen.findByText("artifact-window");

    // DRP-FR-03: a draft is one prompt (DRS-FR-11), so a count of its files
    // says nothing — the row carries the name and the relative timestamp.
    expect(screen.queryByText(/\d+ files?$/)).toBeNull();
    const meta = document.querySelector(".draft-row__meta") as HTMLElement;
    expect(meta.textContent).toMatch(/ago|now|just/i);
  });

  it("DRP-FR-33, DRS-FR-15, NAW-FR-41: an inconsistent draft is listed, marked, and offers Delete alone", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd !== "list_drafts") return undefined;
      return {
        folders: [],
        drafts: [
          DRAFTS[0],
          {
            id: "d-broken",
            name: "half a draft",
            status: "active",
            folder: "",
            inconsistent: true,
            updatedAt: "2026-07-31T08:00:00Z",
          },
        ],
      };
    });
    renderPanel();

    // DRP-FR-33: never hidden — the author is the one who decides what becomes
    // of material that is not a draft.
    await screen.findByText("half a draft");
    expect(screen.getByText("artifact-window")).toBeInTheDocument();
    const marks = screen.getAllByTestId("draft-row-inconsistent");
    expect(marks).toHaveLength(1);
    expect(marks[0]).toHaveTextContent("cannot be opened");

    await act(async () => void openDraftMenu("half a draft"));
    const menu = screen.getByRole("menu");
    for (const name of ["Rename…", "Move to Folder…", "Archive"]) {
      expect(within(menu).getByRole("menuitem", { name })).toHaveAttribute(
        "aria-disabled",
        "true",
      );
    }
    expect(
      within(menu).getByRole("menuitem", { name: "Delete" }),
    ).not.toHaveAttribute("aria-disabled");
  });

  // -------------------------------------------------------------------------
  // The tree (DRP-FR-20, DRP-FR-23, DRP-FR-26, DRP-FR-32, DRP-FR-09, NAW-FR-02)
  // -------------------------------------------------------------------------

  it("DRP-FR-20, DRP-FR-23, DRP-FR-26, DRP-FR-32: the whole hierarchy renders from one list call, at every depth", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI", "UI/Components", "backend"] });
    await screen.findByText("scratch");

    // One call, however deep the tree runs (DRP non-functional requirements).
    expect(calls("list_drafts")).toHaveLength(1);
    expect(folderRow("UI")).toBeInTheDocument();
    expect(folderRow("Components")).toBeInTheDocument();
    expect(folderRow("backend")).toBeInTheDocument();
    expect(screen.getByText("button-lens")).toBeInTheDocument();
    // DRP-FR-20: the implicit root is the container, never a row of its own.
    expect(
      screen.queryByRole("treeitem", { name: /^Folder Drafts$/ }),
    ).not.toBeInTheDocument();
  });

  it("DRP-FR-20: a collapsed folder hides its children, and expanding costs no call", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel();
    await screen.findByText("scratch");

    // Nothing expanded: the top-level folders render, their contents do not.
    expect(screen.queryByText("artifact-window")).not.toBeInTheDocument();
    expect(folderRow("UI")).toHaveAttribute("aria-expanded", "false");

    await userEvent.click(folderRow("UI"));
    expect(await screen.findByText("artifact-window")).toBeInTheDocument();
    expect(folderRow("UI")).toHaveAttribute("aria-expanded", "true");
    // DRP-FR-20 / non-functional: expanding issues nothing.
    expect(calls("list_drafts")).toHaveLength(1);
    // A folder's expansion says nothing about its children's.
    expect(folderRow("Components")).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText("button-lens")).not.toBeInTheDocument();
  });

  it("DRP-FR-20, DRP-FR-23, DRP-FR-26, DRP-FR-32: folders render before drafts, ordered case-insensitively by name", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts"
        ? {
            folders: [
              { path: "zebra", parent: "" },
              { path: "Alpha", parent: "" },
              { path: "beta", parent: "" },
            ],
            drafts: [
              { ...DRAFTS[1], folder: "" },
              { ...DRAFTS[2], folder: "" },
            ],
          }
        : undefined,
    );
    renderPanel();
    await screen.findByText("editor-tweaks");

    const rendered = Array.from(
      document.querySelectorAll(".drafts-tree__name, .draft-row__label"),
    ).map((el) => el.textContent);
    expect(rendered).toEqual([
      "Alpha",
      "beta",
      "zebra",
      // DRP-FR-21: drafts most-recent-activity first, the order `list_drafts`
      // returns them in, preserved rather than re-sorted.
      "editor-tweaks",
      "library-lens",
    ]);
  });

  it("DRP-FR-09, DRP-FR-20, NAW-FR-02: activating a draft row opens it; activating a folder row expands it", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    const { onOpenDraft } = renderPanel();
    await screen.findByText("scratch");

    await userEvent.click(screen.getByText("scratch"));
    expect(onOpenDraft).toHaveBeenCalledWith(
      expect.objectContaining({ id: "d-scratch" }),
    );

    onOpenDraft.mockClear();
    await userEvent.click(folderRow("backend"));
    expect(folderRow("backend")).toHaveAttribute("aria-expanded", "true");
    await userEvent.click(folderRow("backend"));
    expect(folderRow("backend")).toHaveAttribute("aria-expanded", "false");
    // No tab was opened either time.
    expect(onOpenDraft).not.toHaveBeenCalled();
  });

  it("DRP-FR-28, DRP-FR-30: a folder row expands and collapses from the keyboard", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel();
    await screen.findByText("scratch");

    folderRow("UI").focus();
    await userEvent.keyboard("{Enter}");
    expect(folderRow("UI")).toHaveAttribute("aria-expanded", "true");
    await userEvent.keyboard("{ArrowLeft}");
    expect(folderRow("UI")).toHaveAttribute("aria-expanded", "false");
    await userEvent.keyboard("{ArrowRight}");
    expect(folderRow("UI")).toHaveAttribute("aria-expanded", "true");
  });

  // -------------------------------------------------------------------------
  // Status and text filters (DRP-FR-07, DRP-FR-08, DRP-FR-13, DRP-FR-17 … DRP-FR-29)
  // -------------------------------------------------------------------------

  it("DRP-FR-07, DRP-FR-08: the tree is the only structure — no status headers, an archived marker instead", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts"
        ? {
            folders: [{ path: "UI", parent: "" }],
            drafts: [
              { ...DRAFTS[0], folder: "UI" },
              { ...DRAFTS[1], folder: "UI" },
            ],
          }
        : undefined,
    );
    const { view } = renderPanel({ filter: "all", expanded: ["UI"] });
    await screen.findByText("artifact-window");

    // DRP-FR-08: no group header anywhere, and the folder renders once holding
    // both rows rather than twice.
    expect(document.querySelectorAll(".note-group__header")).toHaveLength(0);
    expect(screen.getAllByRole("treeitem", { name: "Folder UI" })).toHaveLength(1);
    // The archived one carries the marker; the active one does not.
    expect(screen.getAllByTestId("draft-archived-marker")).toHaveLength(1);

    // The archived position renders the same tree carrying only those drafts,
    // and no row carries the marker where every row would.
    view.rerender(<div />);
    cleanup();
    renderPanel({ filter: "archived", expanded: ["UI"] });
    await screen.findByText("artifact-window");
    expect(screen.queryByText("editor-tweaks")).not.toBeInTheDocument();
    expect(screen.queryAllByTestId("draft-archived-marker")).toHaveLength(0);
  });

  it("DRP-FR-07, DRP-FR-08, DRP-FR-35: a discarded run moves its draft out of the graduated position", async () => {
    // DRP-FR-07 / DRP-FR-05 / DRS-FR-KQTW: the panel filters on the status the
    // row reports. `list_drafts` resolves that status against the graduation
    // queue, so discarding the run moves the row on the next re-list — which is
    // what the shell asks for when `"graduation run changed"` arrives.
    const row = (
      status: DraftSummary["status"],
      state: GraduationRunState,
      graduated: boolean,
    ): DraftSummary => ({
      ...DRAFTS[1],
      id: "d-released",
      name: "run-logs",
      status,
      graduation: { runId: "run-9", state, locked: !graduated, graduated },
    });
    // The backend answers `graduated` while the run stands, and `active` once it
    // is discarded — the resolution of DRS-FR-KQTW, which happens there.
    let current = row("graduated", "working", true);
    const onOpenRun = vi.fn();
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? { folders: [], drafts: [current] } : undefined,
    );

    const { bump } = renderPanel({ filter: "graduated", onOpenRun });
    expect(await screen.findByText("run-logs")).toBeInTheDocument();

    // The author discards the run.
    current = row("active", "discarded", false);
    bump(1);

    await waitFor(() =>
      expect(screen.queryByText("run-logs")).not.toBeInTheDocument(),
    );
    expect(
      screen.getByText("No draft matches this filter."),
    ).toBeInTheDocument();

    // DRP-FR-08 / DRP-FR-35: it stands in the active position instead, with no
    // graduated marker, and the row still names the discarded run and offers
    // the route to it.
    cleanup();
    renderPanel({ filter: "all", onOpenRun });
    await screen.findByText("run-logs");
    expect(screen.queryAllByTestId("draft-graduated-marker")).toHaveLength(0);
    const marker = screen.getByTestId("draft-graduation-marker");
    expect(marker).toHaveTextContent(/discarded/i);
    await userEvent.click(marker);
    expect(onOpenRun).toHaveBeenCalledWith("run-9");
  });

  it("DRP-FR-13, DRP-FR-07, DRP-FR-29: the status filter narrows drafts and never folders", async () => {
    // DRP-FR-07: a folder left with no admitted draft still renders — it is the
    // author's structure rather than a consequence of what is in it.
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts"
        ? {
            folders: [{ path: "UI", parent: "" }],
            drafts: [{ ...DRAFTS[1], folder: "UI" }],
          }
        : undefined,
    );
    renderPanel({ filter: "archived", expanded: ["UI"] });
    await screen.findByRole("treeitem", { name: "Folder UI" });

    expect(screen.queryByText("editor-tweaks")).not.toBeInTheDocument();
    // DRP-FR-29: and it says so rather than disappearing.
    expect(screen.getByText("Nothing here yet.")).toBeInTheDocument();
  });

  it("DRP-FR-13, DRP-FR-17/13: the text filter reveals ancestors, and the revelation is not persisted", async () => {
    const tree = nested();
    invokeMock.mockImplementation(async (cmd, args) => {
      if (cmd === "list_drafts") return tree;
      if (cmd === "search_drafts") {
        const text = String((args as { text: string }).text).toLowerCase();
        return tree.drafts
          .filter((d) => d.name.toLowerCase().includes(text))
          .map((d) => ({ draftId: d.id, matchedIn: "name" }));
      }
      return undefined;
    });
    const onExpandedChange = vi.fn();
    renderPanel({ onExpandedChange });
    await screen.findByText("scratch");

    // Collapsed to start with: `button-lens` is two levels down.
    expect(screen.queryByText("button-lens")).not.toBeInTheDocument();

    await userEvent.type(screen.getByLabelText("Filter drafts"), "button");
    // The tree stays interactive while the search is outstanding and never
    // blanks between results (DRP non-functional requirements), so the narrowing
    // lands when the settled query comes back rather than on the keystroke.
    await waitFor(() =>
      expect(
        screen.queryByRole("treeitem", { name: "Folder backend" }),
      ).not.toBeInTheDocument(),
    );
    // Every ancestor revealed, and the folders holding nothing admitted hidden.
    expect(screen.getByText("button-lens")).toBeInTheDocument();
    expect(folderRow("UI")).toBeInTheDocument();
    expect(folderRow("Components")).toBeInTheDocument();
    // DRP-FR-13: forced revelation is the filter's, not the author's.
    expect(onExpandedChange).not.toHaveBeenCalled();

    await userEvent.clear(screen.getByLabelText("Filter drafts"));
    await waitFor(() =>
      expect(screen.queryByText("button-lens")).not.toBeInTheDocument(),
    );
    expect(screen.getByText("scratch")).toBeInTheDocument();
    // The decisive assertion: `UI` is collapsed again, so the revelation really
    // was the filter's and not something the author now has to undo.
    expect(folderRow("UI")).toHaveAttribute("aria-expanded", "false");
  });

  it("DRP-FR-13: collapsing a filter-revealed folder does not persist an expansion", async () => {
    // A folder the filter forced open is not in the author's expanded set, so a
    // bare toggle would ADD it — leaving it expanded (the filter is still
    // forcing it) and silently persisting an expansion they were trying to undo,
    // which would still be there when the filter cleared.
    const tree = nested();
    invokeMock.mockImplementation(async (cmd, args) => {
      if (cmd === "list_drafts") return tree;
      if (cmd === "search_drafts") {
        const text = String((args as { text: string }).text).toLowerCase();
        return tree.drafts
          .filter((d) => d.name.toLowerCase().includes(text))
          .map((d) => ({ draftId: d.id, matchedIn: "name" }));
      }
      return undefined;
    });
    const onExpandedChange = vi.fn();
    renderPanel({ onExpandedChange });
    await screen.findByText("scratch");

    await userEvent.type(screen.getByLabelText("Filter drafts"), "button");
    await waitFor(() => expect(screen.getByText("button-lens")).toBeInTheDocument());

    await userEvent.click(folderRow("UI"));
    expect(onExpandedChange).not.toHaveBeenCalled();

    await userEvent.clear(screen.getByLabelText("Filter drafts"));
    await waitFor(() =>
      expect(screen.queryByText("button-lens")).not.toBeInTheDocument(),
    );
    expect(folderRow("UI")).toHaveAttribute("aria-expanded", "false");
  });

  it("DRP-FR-13, DRP-FR-17, DRP-FR-14: a folder whose own name matches renders, even with no admitted draft", async () => {
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "list_drafts") return nested();
      // Nothing matches by name or contents.
      if (cmd === "search_drafts") return [];
      return undefined;
    });
    renderPanel({ expanded: ["backend"] });
    await screen.findByText("scratch");

    await userEvent.type(screen.getByLabelText("Filter drafts"), "backend");
    await waitFor(() =>
      expect(screen.queryByRole("treeitem", { name: "Folder UI" })).not.toBeInTheDocument(),
    );
    // Admitted on its own name (per LIB-FR-09), holding nothing the filter lets
    // through — so it renders with the affordance of DRP-FR-29 rather than
    // disappearing along with its excluded draft.
    expect(folderRow("backend")).toBeInTheDocument();
    expect(screen.getByText("Nothing here yet.")).toBeInTheDocument();
    expect(screen.queryByText("editor-tweaks")).not.toBeInTheDocument();
  });

  it("DRP-FR-13, DRP-FR-17, DRP-FR-14: a draft matched on its contents is annotated; one matched on its name is not", async () => {
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "list_drafts") return flat();
      if (cmd === "search_drafts")
        return [
          { draftId: "d-active", matchedIn: "contents" },
          { draftId: "d-old", matchedIn: "name" },
        ];
      return undefined;
    });
    renderPanel();
    await screen.findByText("editor-tweaks");

    await userEvent.type(screen.getByLabelText("Filter drafts"), "overlay");
    await waitFor(() =>
      expect(screen.getByText("matches text")).toBeInTheDocument(),
    );
    expect(screen.getAllByText("matches text")).toHaveLength(1);
    expect(screen.queryByText("artifact-window")).not.toBeInTheDocument();
  });

  it("DRP-FR-13: an empty filter searches nothing at all", async () => {
    renderPanel();
    await screen.findByText("artifact-window");
    await userEvent.type(screen.getByLabelText("Filter drafts"), "a{Backspace}");
    await new Promise((r) => setTimeout(r, 320));
    expect(calls("search_drafts")).toHaveLength(0);
  });

  // -------------------------------------------------------------------------
  // Menus (DRP-FR-10, DRP-FR-KDVX, DRP-FR-18, DRP-FR-35, DRP-FR-22, LCM-FR-07)
  // -------------------------------------------------------------------------

  it("DRP-FR-10, DRP-FR-KDVX, DRP-FR-18, DRP-FR-35: the draft menu offers exactly six entries and no folder action", async () => {
    renderPanel();
    await screen.findByText("artifact-window");

    await act(async () => void openDraftMenu("artifact-window"));
    expect(
      within(screen.getByRole("menu"))
        .getAllByRole("menuitem")
        .map((b) => b.textContent),
    ).toEqual([
      "Open",
      "Information",
      "Rename…",
      "Move to Folder…",
      "Restore",
      "Delete",
    ]);

    // DRP-FR-18: the same entry reads Archive on an active draft.
    await act(async () => void openDraftMenu("editor-tweaks"));
    expect(
      within(screen.getByRole("menu"))
        .getAllByRole("menuitem")
        .map((b) => b.textContent),
    ).toEqual([
      "Open",
      "Information",
      "Rename…",
      "Move to Folder…",
      "Archive",
      "Delete",
    ]);
  });

  it("DRP-FR-KDVX, DRP-FR-10, DRP-FR-35: Information is the second entry on every draft row and opens the modal", async () => {
    // DRP-FR-KDVX / DRP-FR-10. A locked row, a graduated one, and an
    // inconsistent one all offer it, enabled, immediately after Open.
    const rows: DraftSummary[] = [
      { id: "d-locked", name: "locked-one", status: "active", folder: "", updatedAt: "2026-07-31T08:00:00Z", graduation: { runId: "r-1", state: "working", locked: true, graduated: false } },
      { id: "d-grad", name: "graduated-one", status: "graduated", folder: "", updatedAt: "2026-07-31T08:00:00Z" },
      { id: "d-bad", name: "broken-one", status: "active", folder: "", updatedAt: "2026-07-31T08:00:00Z", inconsistent: true },
    ];
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "list_drafts" ? flat(rows) : undefined,
    );
    renderPanel();
    await screen.findByText("locked-one");

    for (const row of rows) {
      await act(async () => void openDraftMenu(row.name));
      const entries = within(screen.getByRole("menu")).getAllByRole("menuitem");
      expect(entries[1].textContent).toBe("Information");
      expect(entries[1]).not.toHaveAttribute("aria-disabled");
    }

    // DRP-FR-35: on a row a run holds, Go to run is still the entry below the
    // divider and Information is still immediately after Open.
    await act(async () => void openDraftMenu("locked-one"));
    const menu = screen.getByRole("menu");
    const labels = within(menu).getAllByRole("menuitem").map((n) => n.textContent);
    expect(labels[0]).toBe("Open");
    expect(labels[1]).toBe("Information");
    expect(labels[labels.length - 1]).toBe("Go to run");
    expect(menu.querySelectorAll(".menu-sep")).toHaveLength(1);

    // Activating it opens that draft's modal and invokes nothing of the panel's.
    const before = invokeMock.mock.calls.length;
    await act(async () => {
      within(menu).getByText("Information").click();
    });
    expect(
      screen.getByRole("dialog", { name: "Information for locked-one" }),
    ).toBeInTheDocument();
    // DRP-FR-KDVX: the row, the selection, and the tree are exactly as they were.
    expect(draftRow("locked-one")).toBeInTheDocument();
    expect(
      invokeMock.mock.calls
        .slice(before)
        .map(([cmd]) => cmd)
        // The modal's own two reads are its own (DFI-FR-ZGBU, DFI-FR-BZQN);
        // what this asserts is that the panel invoked nothing.
        .filter(
          (cmd) =>
            cmd !== "read_draft_statistics" && cmd !== "get_draft_publication",
        ),
    ).toEqual([]);
  });

  it("DRP-FR-KDVX: the modal the panel opens subscribes through the mocked runtime and rejects nothing", async () => {
    // The regression behind the mock at the head of this file. The Information
    // modal subscribes to `"draft statistics changed"` when it mounts
    // (DFI-FR-ZGBU), and an un-mocked `listen` rejects on a microtask no
    // assertion is awaiting — a failure that reaches the run rather than a
    // test. This mounts it from the panel and watches for exactly that.
    const rejections: unknown[] = [];
    const onRejection = (e: PromiseRejectionEvent) => rejections.push(e.reason);
    window.addEventListener("unhandledrejection", onRejection);
    try {
      renderPanel();
      await screen.findByText("editor-tweaks");

      await act(async () => void openDraftMenu("editor-tweaks"));
      await act(async () => {
        within(screen.getByRole("menu")).getByText("Information").click();
      });
      expect(
        await screen.findByRole("dialog", { name: "Information for editor-tweaks" }),
      ).toBeInTheDocument();
      // The subscription went through the mocked runtime rather than reaching
      // for a Tauri context jsdom does not have.
      expect(listenMock).toHaveBeenCalledWith(
        "draft-statistics-changed",
        expect.any(Function),
      );
      await act(async () => {
        await Promise.resolve();
      });
      expect(rejections).toEqual([]);
    } finally {
      window.removeEventListener("unhandledrejection", onRejection);
    }
  });

  it("DRP-FR-22, DRP-FR-10, LCM-FR-07: the folder menu leads with its creation cluster; the root menu offers those two alone", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel();
    await screen.findByText("scratch");

    await act(async () => void openFolderMenu("UI"));
    const menu = screen.getByRole("menu");
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((b) => b.textContent),
    ).toEqual([
      "New Draft",
      "New Folder",
      "Rename",
      "Move to Folder…",
      "Delete",
    ]);
    // DRP-FR-22 / LCM-FR-01: the two creation entries lead the menu as one
    // cluster above a divider.
    expect(menu.querySelectorAll(".menu-sep")).toHaveLength(1);
    // DRP-FR-22 / LCM-FR-07: the Library's own primitives, so the two surfaces
    // read as one system.
    expect(menu).toHaveClass("menu");
    expect(menu.querySelectorAll(".menu-item").length).toBe(5);
    // Every entry carries a leading icon.
    for (const entry of within(menu).getAllByRole("menuitem"))
      expect(entry.querySelector("svg")).not.toBeNull();

    // DRP-FR-22: the root has nothing to rename, move, or delete.
    await userEvent.click(screen.getByRole("button", { name: "Drafts actions" }));
    expect(
      within(screen.getByRole("menu"))
        .getAllByRole("menuitem")
        .map((b) => b.textContent),
    ).toEqual(["New Draft", "New Folder"]);
  });

});

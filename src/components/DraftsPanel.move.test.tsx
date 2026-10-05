/**
 * Moving an item in the Drafts panel — by drag and drop, through the
 * destination chooser, and what happens when a move fails
 * (`../../specifications/ui/DRP-drafts-panel.md`, DRP-FR-27 … DRP-FR-32).
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
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

describe("Drafts panel (DRP-drafts-panel.md)", () => {
  // -------------------------------------------------------------------------
  // Drag and drop (DRP-FR-27, DRP-FR-30, DRP-FR-32)
  // -------------------------------------------------------------------------

  it("DRP-FR-27, DRP-FR-30: dragging a draft onto a folder highlights it and moves the draft", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    const { onDraftChanged } = renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    const row = screen.getByRole("treeitem", { name: /Draft artifact-window/ });
    const target = folderRow("backend");

    fireEvent.dragStart(row);
    fireEvent.dragOver(target);
    // DRP-FR-27: highlighted before the release, so the author sees the outcome.
    expect(folderRow("backend")).toHaveAttribute("data-drop-target", "true");
    // A drag never writes.
    expect(calls("move_draft_to_folder")).toHaveLength(0);

    fireEvent.drop(target);
    await waitFor(() =>
      expect(calls("move_draft_to_folder")[0][1]).toEqual({
        draftId: "d-window",
        folder: "backend",
      }),
    );
    expect(calls("move_draft_to_folder")).toHaveLength(1);
    expect(onDraftChanged).toHaveBeenCalled();
  });

  it("DRP-FR-27, DRP-FR-30: a drop into the folder a draft already sits in is refused", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    const row = screen.getByRole("treeitem", { name: /Draft artifact-window/ });
    fireEvent.dragStart(row);
    fireEvent.dragOver(folderRow("UI"));
    expect(folderRow("UI")).not.toHaveAttribute("data-drop-target");
    fireEvent.drop(folderRow("UI"));
    expect(calls("move_draft_to_folder")).toHaveLength(0);
  });

  it("DRP-FR-27, DRP-FR-32: a folder dropped onto itself or a descendant is refused, and nothing is invoked", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    fireEvent.dragStart(folderRow("UI"));
    for (const name of ["Components", "UI"]) {
      fireEvent.dragOver(folderRow(name));
      expect(folderRow(name)).not.toHaveAttribute("data-drop-target");
      fireEvent.drop(folderRow(name));
    }
    expect(calls("move_drafts_folder")).toHaveLength(0);
    // DRP-FR-32: and a subsequent list reports the hierarchy unchanged.
    expect(folderRow("UI")).toBeInTheDocument();
    expect(folderRow("Components")).toBeInTheDocument();
  });

  it("DRP-FR-27, DRP-FR-32: a folder moved where its name is already taken is refused", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts"
        ? {
            folders: [
              { path: "UI", parent: "" },
              { path: "UI/backend", parent: "UI" },
              { path: "backend", parent: "" },
            ],
            drafts: [],
          }
        : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByRole("treeitem", { name: "Folder UI" });

    const source = screen.getAllByRole("treeitem", { name: "Folder backend" })[0];
    fireEvent.dragStart(source);
    // Dropping `UI/backend` at the root collides with the root's own `backend`.
    fireEvent.dragOver(document.querySelector(".vpanel__body") as HTMLElement);
    fireEvent.drop(document.querySelector(".vpanel__body") as HTMLElement);
    expect(calls("move_drafts_folder")).toHaveLength(0);
  });

  it("DRP-FR-27, DRP-FR-30: a draft dropped on the panel's root is filed at the root", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    const row = screen.getByRole("treeitem", { name: /Draft artifact-window/ });
    const body = document.querySelector(".vpanel__body") as HTMLElement;
    fireEvent.dragStart(row);
    fireEvent.dragOver(body);
    fireEvent.drop(body);

    await waitFor(() =>
      expect(calls("move_draft_to_folder")[0][1]).toEqual({
        draftId: "d-window",
        folder: "",
      }),
    );
  });

  // -------------------------------------------------------------------------
  // The non-pointer path (DRP-FR-28, DRP-FR-30)
  // -------------------------------------------------------------------------

  it("DRP-FR-28, DRP-FR-30: Move to Folder… lists refused destinations as unavailable rather than omitting them", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Move to Folder…" }));

    const dialog = screen.getByRole("dialog", { name: /Move UI/ });
    // DRP-FR-28: the chooser is the application's own filterable select, so a
    // worktree with twenty folders is as reachable as one with three.
    await userEvent.click(
      within(dialog).getByRole("combobox", { name: "Destination folder" }),
    );
    const byPath = new Map(
      screen
        .getAllByRole("option")
        .map((o) => [o.getAttribute("data-testid")?.replace(
          "drafts-move-destination-option-",
          "",
        ) ?? "", o]),
    );
    // Every folder is offered, the ones it cannot go to included.
    expect([...byPath.keys()].sort()).toEqual([
      "(root)",
      "UI",
      "UI/Components",
      "backend",
    ]);
    // Its current parent (the root), itself, and its own subtree: unavailable.
    expect(byPath.get("(root)")).toHaveAttribute("aria-disabled", "true");
    expect(byPath.get("UI")).toHaveAttribute("aria-disabled", "true");
    expect(byPath.get("UI/Components")).toHaveAttribute("aria-disabled", "true");
    expect(byPath.get("backend")).not.toHaveAttribute("aria-disabled");
    // DRP-FR-28: the reason a folder cannot be a destination is legible, not
    // merely encoded in the disabled attribute.
    expect(screen.getByRole("listbox")).toHaveTextContent("A folder cannot hold itself.");
    expect(screen.getByRole("listbox")).toHaveTextContent(
      "A folder cannot move inside its own subtree.",
    );
    expect(screen.getByRole("listbox")).toHaveTextContent("Already in this folder.");
  });

  it("DRP-FR-28: the destination chooser filters, and will not commit a refused folder", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Move to Folder…" }));
    const dialog = screen.getByRole("dialog", { name: /Move UI/ });
    await userEvent.click(
      within(dialog).getByRole("combobox", { name: "Destination folder" }),
    );

    // Typing narrows the list — the only way to reach a folder whose name the
    // author remembers but whose place in the tree they do not.
    await userEvent.type(
      screen.getByLabelText("Filter destination folder"),
      "compon",
    );
    expect(screen.getAllByRole("option")).toHaveLength(1);
    expect(screen.getByRole("option")).toHaveTextContent("UI/Components");

    // …and it is one of the refused ones, so choosing it commits nothing.
    await userEvent.click(screen.getByRole("option"));
    expect(screen.getByRole("listbox")).toBeInTheDocument();
    expect(calls("move_drafts_folder")).toHaveLength(0);
  });

  it("DRP-FR-28, DRP-FR-30: the picker confirms into the same operation a drop invokes", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    await act(async () => void openDraftMenu("artifact-window"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Move to Folder…" }));

    const dialog = screen.getByRole("dialog", { name: /Move artifact-window/ });
    await userEvent.click(
      within(dialog).getByRole("combobox", { name: "Destination folder" }),
    );
    await userEvent.click(
      screen.getByTestId("drafts-move-destination-option-backend"),
    );
    await userEvent.click(within(dialog).getByRole("button", { name: "Move" }));

    await waitFor(() =>
      expect(calls("move_draft_to_folder")[0][1]).toEqual({
        draftId: "d-window",
        folder: "backend",
      }),
    );
  });

  it("DRP-FR-28: the tree announces what happened and where the item now sits", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    const row = screen.getByRole("treeitem", { name: /Draft artifact-window/ });
    fireEvent.dragStart(row);
    fireEvent.dragOver(folderRow("backend"));
    fireEvent.drop(folderRow("backend"));

    expect(await screen.findByRole("status")).toHaveTextContent(
      "artifact-window moved to backend.",
    );
  });

  // -------------------------------------------------------------------------
  // Errors, in-flight lockout, re-list (DRP-FR-31, DRP-FR-32, DRP-FR-27, DRP-FR-30, DRP-FR-14)
  // -------------------------------------------------------------------------

  it("DRP-FR-31, DRP-FR-32, DRP-FR-27: a failed move reports against the row and the tree re-lists", async () => {
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "list_drafts") return nested();
      if (cmd === "move_draft_to_folder") throw new Error("disk is full");
      return undefined;
    });
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    const row = screen.getByRole("treeitem", { name: /Draft artifact-window/ });
    fireEvent.dragStart(row);
    fireEvent.dragOver(folderRow("backend"));
    fireEvent.drop(folderRow("backend"));

    expect(await screen.findByRole("alert")).toHaveTextContent("disk is full");
    // Still rendered INSIDE `UI` — scoped, because an unscoped query would pass
    // for a row rendered anywhere at all.
    expect(
      within(folderRow("UI").parentElement as HTMLElement).getByText("artifact-window"),
    ).toBeInTheDocument();
    // …and the error is against that row rather than as a window-level notice.
    expect(document.querySelector(".notes__message")).toBeNull();
    expect(calls("list_drafts").length).toBeGreaterThan(1);
  });

  it("DRP-FR-31, DRP-FR-32, DRP-FR-27: a failed folder delete leaves the folder and reports against its row", async () => {
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "list_drafts") return nested();
      if (cmd === "delete_drafts_folder")
        throw new Error('"docs" is already in the destination');
      return undefined;
    });
    renderPanel();
    await screen.findByText("scratch");

    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    await userEvent.click(
      within(screen.getByRole("dialog", { name: "Delete folder" })).getByRole(
        "button",
        { name: "Delete" },
      ),
    );

    expect(await screen.findByRole("alert")).toHaveTextContent("already in the destination");
    expect(folderRow("UI")).toBeInTheDocument();
  });

  it("DRP-FR-31, DRP-FR-32, DRP-FR-27: a second gesture is not accepted while the first is in flight", async () => {
    let release: (() => void) | null = null;
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "list_drafts") return nested();
      if (cmd === "move_draft_to_folder")
        return new Promise((resolve) => {
          release = () => resolve(undefined);
        });
      return undefined;
    });
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    const row = screen.getByRole("treeitem", { name: /Draft artifact-window/ });
    fireEvent.dragStart(row);
    fireEvent.dragOver(folderRow("backend"));
    fireEvent.drop(folderRow("backend"));

    await waitFor(() => expect(calls("move_draft_to_folder")).toHaveLength(1));
    // DRP-FR-31: the row cannot be dragged and will not open its menu while the
    // operation is outstanding.
    await waitFor(() =>
      expect(draftRow("artifact-window")).toHaveAttribute("draggable", "false"),
    );
    await act(async () => void openDraftMenu("artifact-window"));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    await act(async () => {
      release?.();
    });
    expect(calls("move_draft_to_folder")).toHaveLength(1);
  });

  it("DRP-FR-32, DRP-FR-30, DRP-FR-14: a `drafts changed` event re-lists and the tree follows the disk", async () => {
    let tree = nested();
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? tree : undefined,
    );
    const { bump } = renderPanel({ expanded: ["UI", "backend"] });
    await screen.findByText("artifact-window");
    // The draft is the author's current selection, which is the state DRP-FR-30
    // says must survive.
    fireEvent.focus(screen.getByRole("treeitem", { name: /Draft artifact-window/ }));
    expect(
      screen.getByRole("treeitem", { name: /Draft artifact-window/ }),
    ).toHaveAttribute("aria-selected", "true");

    // Another surface moved the draft.
    tree = {
      ...tree,
      drafts: tree.drafts.map((d) =>
        d.id === "d-window" ? { ...d, folder: "backend" } : d,
      ),
    };
    bump(1);

    await waitFor(() => expect(calls("list_drafts")).toHaveLength(2));
    // Still rendered, now under `backend`, and `UI` is still expanded.
    const backend = folderRow("backend").parentElement as HTMLElement;
    expect(within(backend).getByText("artifact-window")).toBeInTheDocument();
    expect(folderRow("UI")).toHaveAttribute("aria-expanded", "true");
    // DRP-FR-30: and it is still the selection.
    expect(
      within(backend).getByRole("treeitem", { name: /Draft artifact-window/ }),
    ).toHaveAttribute("aria-selected", "true");
  });

  it("DRP-FR-05, DRP-FR-29, DRP-FR-35: a draft graduated elsewhere leaves the tree while its folder remains", async () => {
    let tree = nested();
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? tree : undefined,
    );
    const { bump } = renderPanel({ expanded: ["backend"] });
    await screen.findByText("editor-tweaks");

    tree = { ...tree, drafts: tree.drafts.filter((d) => d.id !== "d-editor") };
    bump(1);

    await waitFor(() =>
      expect(screen.queryByText("editor-tweaks")).not.toBeInTheDocument(),
    );
    // DRP-FR-29: the folder that held it remains, carrying the empty affordance.
    expect(folderRow("backend")).toBeInTheDocument();
    expect(screen.getByText("Nothing here yet.")).toBeInTheDocument();
  });


});

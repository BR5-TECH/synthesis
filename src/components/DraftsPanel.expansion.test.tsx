/**
 * Expansion, focus, the one-gesture-one-call rule, and the defects reported
 * against the drop region, the row target and the menu gesture
 * (`../../specifications/ui/DRP-drafts-panel.md`, DRP-FR-24 … DRP-FR-31).
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
import {
  folderJoin,
  folderName,
  folderParent,
} from "./DraftsPanel";
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
  // Expansion, focus and the one-gesture-one-call rule (DRP-FR-24, FR-30, FR-31)
  // -------------------------------------------------------------------------

  it("DRP-FR-24: a renamed folder keeps its expansion, and its subtree's", async () => {
    // Expansion is keyed by path because a drafts folder has no id (DRS-FR-29),
    // so a rename silently collapses the folder and everything under it unless
    // the keys move with it.
    let tree = nested();
    invokeMock.mockImplementation(async (cmd, args) => {
      if (cmd === "list_drafts") return tree;
      if (cmd === "rename_drafts_folder") {
        const { path, name } = args as { path: string; name: string };
        const to = folderParent(path) ? `${folderParent(path)}/${name}` : name;
        tree = {
          folders: tree.folders.map((f) =>
            f.path === path || f.path.startsWith(`${path}/`)
              ? { path: `${to}${f.path.slice(path.length)}`, parent: f.parent === path ? to : f.parent }
              : f,
          ),
          drafts: tree.drafts.map((d) =>
            d.folder === path || d.folder?.startsWith(`${path}/`)
              ? { ...d, folder: `${to}${d.folder.slice(path.length)}` }
              : d,
          ),
        };
        return { path: to, parent: folderParent(path) };
      }
      return undefined;
    });
    renderPanel({ expanded: ["UI", "UI/Components"] });
    await screen.findByText("button-lens");

    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    const field = screen.getByLabelText("Folder name");
    await userEvent.clear(field);
    await userEvent.type(field, "Interface{Enter}");

    await waitFor(() => expect(folderRow("Interface")).toBeInTheDocument());
    expect(folderRow("Interface")).toHaveAttribute("aria-expanded", "true");
    // …and the child kept its own expansion, so the draft two levels down is
    // still on screen rather than having collapsed out of view.
    expect(folderRow("Components")).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText("button-lens")).toBeInTheDocument();
  });

  it("DRP-FR-30: a moved folder arrives expanded if it was expanded", async () => {
    let tree = nested();
    invokeMock.mockImplementation(async (cmd, args) => {
      if (cmd === "list_drafts") return tree;
      if (cmd === "move_drafts_folder") {
        const { path, destination } = args as { path: string; destination: string };
        const to = destination ? `${destination}/${folderName(path)}` : folderName(path);
        tree = {
          folders: tree.folders.map((f) =>
            f.path === path || f.path.startsWith(`${path}/`)
              ? { path: `${to}${f.path.slice(path.length)}`, parent: f.path === path ? destination : f.parent }
              : f,
          ),
          drafts: tree.drafts.map((d) =>
            d.folder === path || d.folder?.startsWith(`${path}/`)
              ? { ...d, folder: `${to}${d.folder.slice(path.length)}` }
              : d,
          ),
        };
        return { path: to, parent: destination };
      }
      return undefined;
    });
    renderPanel({ expanded: ["UI", "UI/Components", "backend"] });
    await screen.findByText("button-lens");

    fireEvent.dragStart(folderRow("Components"));
    fireEvent.dragOver(folderRow("backend"));
    fireEvent.drop(folderRow("backend"));

    await waitFor(() => expect(calls("move_drafts_folder")).toHaveLength(1));
    await waitFor(() =>
      expect(folderRow("Components")).toHaveAttribute("aria-expanded", "true"),
    );
    expect(screen.getByText("button-lens")).toBeInTheDocument();
  });

  it("DRP-FR-30: a move into a collapsed folder reveals it, so the item stays in view", async () => {
    // Focus follows the moved item, which it can only do if the row renders at
    // all — otherwise the item vanishes while the panel announces where it went.
    let tree = nested();
    invokeMock.mockImplementation(async (cmd, args) => {
      if (cmd === "list_drafts") return tree;
      if (cmd === "move_draft_to_folder") {
        const { draftId, folder } = args as { draftId: string; folder: string };
        tree = {
          ...tree,
          drafts: tree.drafts.map((d) => (d.id === draftId ? { ...d, folder } : d)),
        };
        return tree.drafts.find((d) => d.id === draftId);
      }
      return undefined;
    });
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("artifact-window");
    expect(folderRow("backend")).toHaveAttribute("aria-expanded", "false");

    fireEvent.dragStart(screen.getByRole("treeitem", { name: /Draft artifact-window/ }));
    fireEvent.dragOver(folderRow("backend"));
    fireEvent.drop(folderRow("backend"));

    await waitFor(() =>
      expect(folderRow("backend")).toHaveAttribute("aria-expanded", "true"),
    );
    const backend = folderRow("backend").parentElement as HTMLElement;
    expect(within(backend).getByText("artifact-window")).toBeInTheDocument();
  });

  it("DRP-FR-31: two drops in one tick produce one call, not two", async () => {
    // The lock is read synchronously: two gestures dispatched in the same batch
    // both see the same render's state, so a state-based guard would let both
    // through — which is exactly the second invocation the spec forbids.
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    const row = screen.getByRole("treeitem", { name: /Draft artifact-window/ });
    const target = folderRow("backend");
    fireEvent.dragStart(row);
    fireEvent.dragOver(target);
    fireEvent.drop(target);
    fireEvent.dragStart(row);
    fireEvent.dragOver(target);
    fireEvent.drop(target);

    await waitFor(() => expect(calls("move_draft_to_folder").length).toBeGreaterThan(0));
    expect(calls("move_draft_to_folder")).toHaveLength(1);
  });

  it("DRP-FR-27: a drop released on a draft row files the item where that row sits", async () => {
    // The events would otherwise bubble to the panel body and read as a drop on
    // the implicit root, so releasing over a draft two levels down would file
    // the item at the top of the tree. Aiming at a row means aiming at where
    // that row sits.
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI", "UI/Components", "backend"] });
    await screen.findByText("button-lens");

    const dragged = screen.getByRole("treeitem", { name: /Draft editor-tweaks/ });
    const over = screen.getByRole("treeitem", { name: /Draft button-lens/ });
    fireEvent.dragStart(dragged);
    fireEvent.dragOver(over);
    fireEvent.drop(over);

    await waitFor(() =>
      expect(calls("move_draft_to_folder")[0][1]).toEqual({
        draftId: "d-editor",
        folder: "UI/Components",
      }),
    );
  });

  it("DRP-FR-27: a drag abandoned without a drop invokes nothing", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    const row = screen.getByRole("treeitem", { name: /Draft artifact-window/ });
    fireEvent.dragStart(row);
    fireEvent.dragOver(folderRow("backend"));
    expect(folderRow("backend")).toHaveAttribute("data-drop-target", "true");
    fireEvent.dragEnd(row);

    expect(calls("move_draft_to_folder")).toHaveLength(0);
    expect(folderRow("backend")).not.toHaveAttribute("data-drop-target");
  });

  it("DRP-FR-27: the panel's root highlights for a valid drop and not for a refused one", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");
    const body = document.querySelector(".vpanel__body") as HTMLElement;

    // A draft filed under `UI` may go to the root.
    fireEvent.dragStart(screen.getByRole("treeitem", { name: /Draft artifact-window/ }));
    fireEvent.dragOver(body);
    expect(body).toHaveAttribute("data-drop-target", "true");
    fireEvent.dragEnd(screen.getByRole("treeitem", { name: /Draft artifact-window/ }));

    // One already at the root may not.
    fireEvent.dragStart(screen.getByRole("treeitem", { name: /Draft scratch/ }));
    fireEvent.dragOver(body);
    expect(body).not.toHaveAttribute("data-drop-target");
    fireEvent.drop(body);
    expect(calls("move_draft_to_folder")).toHaveLength(0);
  });

  it("DRP-FR-30: focus lands on the first reparented child after a folder is deleted", async () => {
    let tree = nested();
    invokeMock.mockImplementation(async (cmd, args) => {
      if (cmd === "list_drafts") return tree;
      if (cmd === "delete_drafts_folder") {
        const { path } = args as { path: string };
        const parent = folderParent(path);
        tree = {
          folders: tree.folders
            .filter((f) => f.path !== path)
            .map((f) =>
              f.parent === path
                ? { path: parent ? `${parent}/${folderName(f.path)}` : folderName(f.path), parent }
                : f,
            ),
          drafts: tree.drafts.map((d) => (d.folder === path ? { ...d, folder: parent } : d)),
        };
        return undefined;
      }
      return undefined;
    });
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("artifact-window");

    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    await userEvent.click(
      within(screen.getByRole("dialog", { name: "Delete folder" })).getByRole(
        "button",
        { name: "Delete" },
      ),
    );

    // `Components` was `UI`'s only child folder, so it leads the tree's order.
    await waitFor(() => expect(folderRow("Components")).toHaveFocus());
    expect(folderRow("Components")).toHaveAttribute("aria-selected", "true");
  });


  it("DRP-FR-22: a row menu escapes the scrolling panel body", async () => {
    // The body is `overflow: auto`, so a menu positioned inside it is clipped at
    // its edge — a row near the bottom of the tree loses most of its entries
    // under the pinned create affordance. The Library's context menu escapes its
    // own panel the same way, which is what DRP-FR-22 says these must match.
    renderPanel();
    await screen.findByText("artifact-window");

    await act(async () => void openDraftMenu("artifact-window"));
    const menu = screen.getByRole("menu");
    // Fixed positioning is what escapes the clip: a fixed box is laid out
    // against the viewport and is not clipped by an ancestor's `overflow`.
    expect(menu.style.position).toBe("fixed");
    expect(Number(menu.style.zIndex)).toBeGreaterThan(100);
    // …and it hangs off the button rather than off wherever the row happens to
    // sit, so a scrolled tree does not leave it behind.
    expect(menu.style.top).not.toBe("");
    expect(menu.style.left).not.toBe("");
  });

  it("DRP-FR-30: children reparented by a folder delete keep their expansion", async () => {
    let tree = nested();
    invokeMock.mockImplementation(async (cmd, args) => {
      if (cmd === "list_drafts") return tree;
      if (cmd === "delete_drafts_folder") {
        const { path } = args as { path: string };
        const parent = folderParent(path);
        const rehome = (p: string) =>
          `${folderJoin(parent, folderName(p.slice(0, p.indexOf("/", path.length + 1) === -1 ? p.length : p.indexOf("/", path.length + 1))))}${
            p.indexOf("/", path.length + 1) === -1
              ? ""
              : p.slice(p.indexOf("/", path.length + 1))
          }`;
        tree = {
          folders: tree.folders
            .filter((f) => f.path !== path)
            .map((f) =>
              f.path.startsWith(`${path}/`)
                ? { path: rehome(f.path), parent: f.parent === path ? parent : rehome(f.parent) }
                : f,
            ),
          drafts: tree.drafts.map((d) =>
            d.folder === path
              ? { ...d, folder: parent }
              : d.folder?.startsWith(`${path}/`)
                ? { ...d, folder: rehome(d.folder) }
                : d,
          ),
        };
        return undefined;
      }
      return undefined;
    });
    renderPanel({ expanded: ["UI", "UI/Components"] });
    await screen.findByText("button-lens");

    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    await userEvent.click(
      within(screen.getByRole("dialog", { name: "Delete folder" })).getByRole(
        "button",
        { name: "Delete" },
      ),
    );

    // `Components` moved up to the root and is still expanded, so the draft it
    // holds did not vanish along with the folder that used to contain it.
    await waitFor(() =>
      expect(folderRow("Components")).toHaveAttribute("aria-expanded", "true"),
    );
    expect(screen.getByText("button-lens")).toBeInTheDocument();
  });

  it("DRP-FR-28: a modal takes focus, keeps it, and gives it back", async () => {
    // Without this the dialog opens behind the author's focus and Tab walks the
    // tree behind the scrim.
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel();
    await screen.findByText("scratch");

    const opener = folderRow("UI");
    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Move to Folder…" }));

    const dialog = screen.getByRole("dialog", { name: /Move UI/ });
    expect(dialog).toHaveAttribute("aria-modal", "true");
    await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));

    // Tab wraps inside the dialog rather than escaping to the tree behind it.
    for (let i = 0; i < 12; i += 1) await userEvent.tab();
    expect(dialog.contains(document.activeElement)).toBe(true);

    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    // …and focus lands back on the control it was opened from.
    expect(document.activeElement).toBe(opener);
  });

  it("DRP-FR-28: ArrowDown and ArrowUp walk the rows in render order", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    folderRow("UI").focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(folderRow("Components")).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(screen.getByRole("treeitem", { name: /Draft artifact-window/ })).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    expect(folderRow("Components")).toHaveFocus();
  });


  // -------------------------------------------------------------------------
  // Reported defects: the drop region, the row target, and the menu gesture
  // -------------------------------------------------------------------------

  it("DRP-FR-27: a drop released anywhere in an empty folder's region files it there", async () => {
    // A folder just created is empty, so the only thing under the pointer is
    // the line saying so. Released there, the event used to bubble to the panel
    // background and be read as a drop on the implicit root — which, for a
    // draft already at the root, is refused, so nothing happened at all.
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts"
        ? {
            folders: [{ path: "AI", parent: "" }],
            drafts: [{ ...DRAFTS[1], folder: "" }],
          }
        : undefined,
    );
    renderPanel({ expanded: ["AI"] });
    await screen.findByText("editor-tweaks");

    const empty = screen.getByText("Nothing here yet.");
    fireEvent.dragStart(draftRow("editor-tweaks"));
    fireEvent.dragOver(empty);
    fireEvent.drop(empty);

    await waitFor(() =>
      expect(calls("move_draft_to_folder")[0][1]).toEqual({
        draftId: "d-active",
        folder: "AI",
      }),
    );
  });

  it("DRP-FR-27: a nested folder still wins over the region of the folder holding it", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI", "UI/Components", "backend"] });
    await screen.findByText("button-lens");

    fireEvent.dragStart(draftRow("editor-tweaks"));
    fireEvent.dragOver(folderRow("Components"));
    fireEvent.drop(folderRow("Components"));

    await waitFor(() =>
      expect(calls("move_draft_to_folder")[0][1]).toEqual({
        draftId: "d-editor",
        folder: "UI/Components",
      }),
    );
  });

  it("DRP-FR-09: the whole draft row opens it, second line included", async () => {
    const { onOpenDraft } = renderPanel();
    await screen.findByText("artifact-window");

    // The meta line — the relative timestamp — is part of the row, not a strip
    // of dead space beneath the only clickable thing on it.
    await userEvent.click(
      document.querySelectorAll(".draft-row__meta")[0] as HTMLElement,
    );
    expect(onOpenDraft).toHaveBeenCalledWith(
      expect.objectContaining({ id: "d-archived" }),
    );

    onOpenDraft.mockClear();
    await userEvent.click(screen.getByText("artifact-window"));
    expect(onOpenDraft).toHaveBeenCalledWith(
      expect.objectContaining({ id: "d-archived" }),
    );
  });

  it("DRP-FR-09: clicking a row being renamed edits it rather than opening it", async () => {
    const { onOpenDraft } = renderPanel();
    await screen.findByText("artifact-window");

    await act(async () => void openDraftMenu("artifact-window"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename…" }));
    onOpenDraft.mockClear();

    await userEvent.click(screen.getByLabelText("Draft name"));
    expect(onOpenDraft).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Draft name")).toBeInTheDocument();
  });

  it("DRP-FR-22: menus open on right-click, and no row carries a button of its own", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    // The per-row `[⋯]` is gone — the gesture is the Project panel's own.
    expect(
      screen.queryByRole("button", { name: /actions for/ }),
    ).not.toBeInTheDocument();

    await act(async () => void openDraftMenu("scratch"));
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

    await act(async () => void openFolderMenu("UI"));
    expect(
      within(screen.getByRole("menu"))
        .getAllByRole("menuitem")
        .map((b) => b.textContent),
    ).toEqual([
      "New Draft",
      "New Folder",
      "Rename",
      "Move to Folder…",
      "Delete",
    ]);
  });

  it("DRP-FR-22: a right-click opens the menu at the pointer", async () => {
    renderPanel();
    await screen.findByText("artifact-window");

    await act(async () =>
      void fireEvent.contextMenu(draftRow("artifact-window"), {
        clientX: 320,
        clientY: 210,
      }),
    );
    const menu = screen.getByRole("menu");
    expect(menu.style.position).toBe("fixed");
    expect(menu.style.left).toBe("320px");
    expect(menu.style.top).toBe("210px");
  });

  it("DRP-FR-28: the Menu key and Shift+F10 open the focused row's menu", async () => {
    // A right-click alone would leave the menus without any keyboard route.
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel();
    await screen.findByText("scratch");

    folderRow("UI").focus();
    await userEvent.keyboard("{ContextMenu}");
    expect(screen.getByRole("menu", { name: "Folder actions for UI" })).toBeInTheDocument();

    await userEvent.keyboard("{Escape}");
    draftRow("scratch").focus();
    await userEvent.keyboard("{Shift>}{F10}{/Shift}");
    expect(
      screen.getByRole("menu", { name: "Draft actions for scratch" }),
    ).toBeInTheDocument();
  });

  it("DRP-FR-22: right-clicking the panel background opens the root's menu", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel();
    await screen.findByText("scratch");

    await act(async () =>
      void fireEvent.contextMenu(document.querySelector(".vpanel__body") as HTMLElement),
    );
    expect(
      within(screen.getByRole("menu"))
        .getAllByRole("menuitem")
        .map((b) => b.textContent),
    ).toEqual(["New Draft", "New Folder"]);
  });

  it("DRP-FR-23: the inline field stands where the name it replaces stood", async () => {
    // Left to the input's own box it sits a few pixels right of every other
    // row's name, which reads as a misaligned tree.
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel();
    await screen.findByText("scratch");

    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "New Folder" }));

    const field = screen.getByLabelText("New folder name");
    const provisional = field.closest(".drafts-tree__folder") as HTMLElement;
    // The provisional row is an ordinary folder row: same class, same caret and
    // icon before the field, so its name column starts where every other's does.
    expect(provisional).toBeTruthy();
    expect(provisional.querySelector(".drafts-tree__caret")).not.toBeNull();
    expect(provisional.querySelector(".drafts-tree__icon")).not.toBeNull();
    expect(field.closest(".drafts-tree__field")).not.toBeNull();
  });


  it("DRP-FR-09: a menu entry acts on the row without also opening it", async () => {
    // A draft's menu hangs inside its row so it can be keyed to it, and the
    // whole row is the open target — so an entry click would otherwise open the
    // draft on its way past, which is what "Delete" opening a tab looks like.
    const { onOpenDraft } = renderPanel();
    await screen.findByText("artifact-window");

    await act(async () => void openDraftMenu("artifact-window"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename…" }));
    expect(onOpenDraft).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Draft name")).toBeInTheDocument();

    await userEvent.keyboard("{Escape}");
    await act(async () => void openDraftMenu("artifact-window"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(onOpenDraft).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog", { name: "Delete draft" })).toBeInTheDocument();
  });

  it("DRP-FR-23: the root's inline field is indented like a root row", async () => {
    // Every real row carries its depth as an inline padding, which at depth 0
    // writes `0px` over the stylesheet's own. A provisional row without it sits
    // 8px right of the folders it is about to join.
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel();
    await screen.findByText("scratch");

    await act(async () =>
      void fireEvent.contextMenu(document.querySelector(".vpanel__body") as HTMLElement),
    );
    await userEvent.click(screen.getByRole("menuitem", { name: "New Folder" }));

    const provisional = screen
      .getByLabelText("New folder name")
      .closest(".drafts-tree__folder") as HTMLElement;
    expect(provisional.style.paddingLeft).toBe(
      folderRow("UI").style.paddingLeft || "0px",
    );
  });

  it("DRP-FR-16: Escape closes the destination list before it closes the dialog", async () => {
    // Both firing on one press loses the author's place: the list they opened
    // and the dialog they were working in go together.
    //
    // Asserted on **propagation**, not on what is left in the DOM afterwards.
    // The panel dismisses its overlay from a `window` listener, and React
    // flushes the select's own close synchronously — so in jsdom, where the
    // commit is deferred to the `act` boundary, a DOM-shape assertion passes
    // whether or not the press was stopped. Watching the window is the only
    // thing here that fails when the fix is removed.
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Move to Folder…" }));
    await userEvent.click(
      screen.getByRole("combobox", { name: "Destination folder" }),
    );
    expect(screen.getByRole("listbox")).toBeInTheDocument();

    const reachedWindow: string[] = [];
    const spy = (e: KeyboardEvent) => reachedWindow.push(e.key);
    window.addEventListener("keydown", spy);
    try {
      await userEvent.keyboard("{Escape}");
      // The open list consumed it, so the panel's own dismissal never ran.
      expect(reachedWindow).toEqual([]);
      expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
      expect(screen.getByRole("dialog", { name: /Move UI/ })).toBeInTheDocument();

      // …and with the list closed, the next press reaches the panel and closes
      // the dialog, invoking nothing.
      await userEvent.keyboard("{Escape}");
      expect(reachedWindow).toEqual(["Escape"]);
    } finally {
      window.removeEventListener("keydown", spy);
    }
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(calls("move_drafts_folder")).toHaveLength(0);
  });

});

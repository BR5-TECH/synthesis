/**
 * Creating, renaming and deleting a Drafts-panel folder, and the immediate
 * rename mode a new draft opens in
 * (`../../specifications/ui/DRP-drafts-panel.md`, DRP-FR-23 … DRP-FR-36).
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
  // Create / rename / delete folders (DRP-FR-20, DRP-FR-23, DRP-FR-26, DRP-FR-32, DRP-FR-25 … DRP-FR-24, DRP-FR-31)
  // -------------------------------------------------------------------------

  it("DRP-FR-20, DRP-FR-23, DRP-FR-26, DRP-FR-32: New Folder from the root commits with Enter and reveals the folder", async () => {
    let tree = nested();
    invokeMock.mockImplementation(async (cmd, args) => {
      if (cmd === "list_drafts") return tree;
      if (cmd === "create_drafts_folder") {
        const { parent, name } = args as { parent: string; name: string };
        const path = parent ? `${parent}/${name}` : name;
        tree = { ...tree, folders: [...tree.folders, { path, parent }] };
        return { path, parent };
      }
      return undefined;
    });
    renderPanel();
    await screen.findByText("scratch");

    await userEvent.click(screen.getByRole("button", { name: "Drafts actions" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "New Folder" }));
    await userEvent.type(screen.getByLabelText("New folder name"), "research{Enter}");

    await waitFor(() =>
      expect(calls("create_drafts_folder")[0][1]).toEqual({
        parent: "",
        name: "research",
      }),
    );
    expect(await screen.findByRole("treeitem", { name: "Folder research" })).toBeInTheDocument();
    // DRP-FR-23: revealed expanded and empty, and it becomes the selection.
    await waitFor(() =>
      expect(folderRow("research")).toHaveAttribute("aria-selected", "true"),
    );
    expect(folderRow("research")).toHaveAttribute("aria-expanded", "true");
  });

  it("DRP-FR-20, DRP-FR-23, DRP-FR-26, DRP-FR-32: New Folder inside a collapsed folder expands it and files the folder there", async () => {
    let tree = nested();
    invokeMock.mockImplementation(async (cmd, args) => {
      if (cmd === "list_drafts") return tree;
      if (cmd === "create_drafts_folder") {
        const { parent, name } = args as { parent: string; name: string };
        const path = parent ? `${parent}/${name}` : name;
        tree = { ...tree, folders: [...tree.folders, { path, parent }] };
        return { path, parent };
      }
      return undefined;
    });
    renderPanel();
    await screen.findByText("scratch");

    expect(folderRow("UI")).toHaveAttribute("aria-expanded", "false");
    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "New Folder" }));
    // DRP-FR-23: the folder is expanded so the field is where the folder will be.
    expect(folderRow("UI")).toHaveAttribute("aria-expanded", "true");
    await userEvent.type(screen.getByLabelText("New folder name"), "widgets{Enter}");

    await waitFor(() =>
      expect(calls("create_drafts_folder")[0][1]).toEqual({
        parent: "UI",
        name: "widgets",
      }),
    );
  });

  it("DRP-FR-24, DRP-FR-31: a malformed folder name never reaches the backend, and Escape invokes nothing", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel();
    await screen.findByText("scratch");

    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    const field = screen.getByLabelText("Folder name");

    await userEvent.clear(field);
    await userEvent.type(field, "{Enter}");
    expect(calls("rename_drafts_folder")).toHaveLength(0);
    expect(await screen.findByRole("alert")).toBeInTheDocument();

    await userEvent.clear(field);
    await userEvent.type(field, "a/b{Enter}");
    expect(calls("rename_drafts_folder")).toHaveLength(0);
    // The field stays open holding what was typed.
    expect(screen.getByLabelText("Folder name")).toHaveValue("a/b");

    // The current name is refused too — a rename to nothing is not a rename.
    await userEvent.clear(field);
    await userEvent.type(field, "UI{Enter}");
    expect(calls("rename_drafts_folder")).toHaveLength(0);

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByLabelText("Folder name")).not.toBeInTheDocument();
    expect(folderRow("UI")).toBeInTheDocument();
  });

  it("DRP-FR-24, DRP-FR-31: a backend refusal shows at the field and leaves the folder's name", async () => {
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "list_drafts") return nested();
      if (cmd === "rename_drafts_folder")
        throw new Error('a folder called "BACKEND" is already here');
      return undefined;
    });
    renderPanel();
    await screen.findByText("scratch");

    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    const field = screen.getByLabelText("Folder name");
    await userEvent.clear(field);
    await userEvent.type(field, "BACKEND{Enter}");

    expect(await screen.findByRole("alert")).toHaveTextContent("already here");
    expect(screen.getByLabelText("Folder name")).toHaveValue("BACKEND");
    expect(folderRow("UI")).toBeInTheDocument();
  });

  it("DRP-FR-25: deleting a folder confirms, counting what moves, and reparents rather than deletes", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    const { onDraftChanged } = renderPanel();
    await screen.findByText("scratch");

    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));

    const dialog = screen.getByRole("dialog", { name: "Delete folder" });
    expect(dialog).toHaveTextContent("Delete “UI”?");
    // One draft filed directly in `UI`, one folder (`Components`).
    expect(dialog).toHaveTextContent("The 1 draft and 1 folder it holds move to Drafts");
    expect(dialog).toHaveTextContent("Nothing is deleted.");
    expect(calls("delete_drafts_folder")).toHaveLength(0);

    await userEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() =>
      expect(calls("delete_drafts_folder")[0][1]).toEqual({ path: "UI" }),
    );
    expect(onDraftChanged).toHaveBeenCalled();
    // DRP-FR-32: the tree is redrawn from what the backend reports.
    expect(calls("list_drafts").length).toBeGreaterThan(1);
  });

  it("DRP-FR-25: dismissing the folder confirmation invokes nothing", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel();
    await screen.findByText("scratch");

    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    await userEvent.click(
      within(screen.getByRole("dialog", { name: "Delete folder" })).getByRole(
        "button",
        { name: "Cancel" },
      ),
    );

    expect(calls("delete_drafts_folder")).toHaveLength(0);
    expect(folderRow("UI")).toBeInTheDocument();
  });

  it("DRP-FR-25: the confirmation names the deleted folder's own parent, not the root", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel({ expanded: ["UI"] });
    await screen.findByText("scratch");

    await act(async () => void openFolderMenu("Components"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(screen.getByRole("dialog", { name: "Delete folder" })).toHaveTextContent(
      "The 1 draft it holds moves to UI",
    );
  });

  // -------------------------------------------------------------------------
  // New Draft in a folder (DRP-FR-26, DRP-FR-06, DRP-FR-36, DRS-FR-39, NAW-FR-03)
  // -------------------------------------------------------------------------

  it("DRP-FR-26, DRP-FR-06, DRP-FR-36, DRS-FR-39, NAW-FR-03: New Draft on a folder expands it and passes that folder, not a destination root", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    const { onCreateDraft } = renderPanel();
    await screen.findByText("scratch");

    expect(folderRow("backend")).toHaveAttribute("aria-expanded", "false");
    await act(async () => void openFolderMenu("backend"));
    await userEvent.click(screen.getByRole("menuitem", { name: "New Draft" }));

    expect(onCreateDraft).toHaveBeenCalledWith("backend");
    expect(folderRow("backend")).toHaveAttribute("aria-expanded", "true");
  });

  it("DRP-FR-06, DRP-FR-36, NAW-FR-03, NAW-FR-04: the pinned affordance and the root menu both create at the root", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    const { onCreateDraft } = renderPanel();
    await screen.findByText("scratch");

    const pinned = screen.getByRole("button", { name: /Draft$/ });
    // The one child of the footer that the stylesheet centres (DRP-FR-06).
    const footer = pinned.closest(".drafts__footer");
    expect(footer).not.toBeNull();
    expect(footer!.children).toHaveLength(1);
    await userEvent.click(pinned);
    // The pinned affordance creates at the tree's root, which it says by
    // passing no folder (DRP-FR-06).
    expect(onCreateDraft).toHaveBeenLastCalledWith(undefined);

    await userEvent.click(screen.getByRole("button", { name: "Drafts actions" }));
    await userEvent.click(screen.getByRole("menuitem", { name: "New Draft" }));
    expect(onCreateDraft).toHaveBeenLastCalledWith(undefined);
  });

  // -------------------------------------------------------------------------
  // Immediate rename mode (DRP-FR-36 / DRP-FR-06, DRP-FR-26, DRS-FR-39, SET-FR-17 / DRP-FR-11, DRP-FR-16)
  // -------------------------------------------------------------------------

  /**
   * A backend whose listing gains the created draft, and a creation callback
   * that reports it back — which is what puts the new row into rename mode
   * (DRP-FR-36).
   */
  function backendCreatingDraft(folder = "") {
    const created = { id: "d-new", name: "Untitled" };
    let exists = false;
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd !== "list_drafts") return undefined;
      const hierarchy = nested();
      if (exists)
        hierarchy.drafts.push({
          id: created.id,
          name: created.name,
          status: "active",
          folder,
          updatedAt: "2026-07-31T12:00:00Z",
        });
      return hierarchy;
    });
    const onCreateDraft = vi.fn(async () => {
      exists = true;
      return created;
    });
    return { onCreateDraft, created };
  }

  it("DRP-FR-06, NAW-FR-03, NAW-FR-04 / DRP-FR-36: a draft created from the pinned affordance opens in rename mode", async () => {
    const { onCreateDraft } = backendCreatingDraft();
    renderPanel({ onCreateDraft });
    await screen.findByText("scratch");

    await userEvent.click(screen.getByRole("button", { name: /Draft$/ }));

    // The row appears at the tree's root, selected, with the inline name field
    // open, seeded with the created name, its text selected, and focused.
    const field = (await screen.findByLabelText(
      "Draft name",
    )) as HTMLInputElement;
    expect(field).toHaveValue("Untitled");
    expect(field).toHaveFocus();
    expect(field.selectionStart).toBe(0);
    expect(field.selectionEnd).toBe("Untitled".length);
    expect(
      document.querySelector('[data-row-key="draft:d-new"]'),
    ).toHaveAttribute("aria-selected", "true");

    // DRP-FR-11: committing renames the draft through the ordinary path.
    await userEvent.keyboard("overview{Enter}");
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some(
          (c) => c[0] === "rename_draft" && c[1]?.name === "overview",
        ),
      ).toBe(true),
    );
  });

  it("DRP-FR-26, DRP-FR-06, DRS-FR-39, NAW-FR-03 / DRP-FR-36: New Draft on a folder expands it and opens the row's name field inside it", async () => {
    const { onCreateDraft } = backendCreatingDraft("backend");
    renderPanel({ onCreateDraft });
    await screen.findByText("scratch");

    expect(folderRow("backend")).toHaveAttribute("aria-expanded", "false");
    await act(async () => void openFolderMenu("backend"));
    await userEvent.click(screen.getByRole("menuitem", { name: "New Draft" }));

    expect(onCreateDraft).toHaveBeenCalledWith("backend");
    await waitFor(() =>
      expect(folderRow("backend")).toHaveAttribute("aria-expanded", "true"),
    );
    const field = await screen.findByLabelText("Draft name");
    expect(field).toHaveFocus();
    // DRP-FR-26, DRP-FR-06, DRP-FR-36, DRS-FR-39, NAW-FR-03: nothing was written into the project artifact tree, and the
    // panel neither read the project's draft template nor passed one — the copy
    // is the creation operation's (DRS-FR-39). Asserted over the panel's whole
    // backend traffic, which is `list_drafts` and nothing else.
    expect([...new Set(invokeMock.mock.calls.map((c) => c[0]))]).toEqual([
      "list_drafts",
    ]);
    expect(onCreateDraft.mock.calls[0]).toEqual(["backend"]);
  });

  it("DRP-FR-36, DRP-FR-11, DRP-FR-16: the field abandons, refuses an empty name, and reopens from Rename…", async () => {
    const { onCreateDraft } = backendCreatingDraft();
    renderPanel({ onCreateDraft });
    await screen.findByText("scratch");

    // Opening one of the panel's floating surfaces closes the field without
    // committing it; the draft keeps the name it was created under.
    await userEvent.click(screen.getByRole("button", { name: /Draft$/ }));
    await screen.findByLabelText("Draft name");
    await userEvent.click(screen.getByRole("button", { name: "Drafts actions" }));
    await waitFor(() => expect(screen.queryByLabelText("Draft name")).toBeNull());
    expect(invokeMock.mock.calls.some((c) => c[0] === "rename_draft")).toBe(
      false,
    );
    await userEvent.keyboard("{Escape}");

    // And **Rename…** afterwards opens the same field, seeded with the name the
    // draft is still called.
    await act(async () => void openDraftMenu("Untitled"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename…" }));
    expect(await screen.findByLabelText("Draft name")).toHaveValue("Untitled");

    // A commit of an empty name invokes nothing and leaves the name standing.
    await userEvent.clear(screen.getByLabelText("Draft name"));
    await userEvent.keyboard("{Enter}");
    expect(invokeMock.mock.calls.some((c) => c[0] === "rename_draft")).toBe(
      false,
    );
    expect(
      document.querySelector('[data-row-key="draft:d-new"]'),
    ).toBeInTheDocument();
  });

  it("DRP-FR-06, DRP-FR-26, DRS-FR-39, SET-FR-17: the panel neither reads nor passes the project's draft template", async () => {
    // DRP-FR-06 / DRP-FR-26: the copy is the creation operation's (DRS-FR-39),
    // which is what makes both routes produce the same starting content. The
    // panel's whole part in it is to name a folder, or not.
    const { onCreateDraft } = backendCreatingDraft();
    renderPanel({ onCreateDraft });
    await screen.findByText("scratch");

    await userEvent.click(screen.getByRole("button", { name: /Draft$/ }));
    await screen.findByLabelText("Draft name");
    await userEvent.keyboard("{Escape}");
    await act(async () => void openFolderMenu("backend"));
    await userEvent.click(screen.getByRole("menuitem", { name: "New Draft" }));

    // The two routes differ in the folder each names and in nothing else, and
    // neither carries a template — nor did the panel read one.
    expect(onCreateDraft.mock.calls).toEqual([[undefined], ["backend"]]);
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "load_project_config"),
    ).toBe(false);
  });

  it("DRP-FR-36: a refused creation opens no name field", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    // The shell reported the failure itself; there is no row to name.
    const onCreateDraft = vi.fn(async () => null);
    renderPanel({ onCreateDraft });
    await screen.findByText("scratch");

    await userEvent.click(screen.getByRole("button", { name: /Draft$/ }));
    await waitFor(() => expect(onCreateDraft).toHaveBeenCalled());
    expect(screen.queryByLabelText("Draft name")).toBeNull();
  });

});

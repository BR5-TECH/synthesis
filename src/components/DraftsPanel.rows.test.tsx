/**
 * Overlay exclusivity, the empty states, the draft-row actions, the pending
 * proposal marker, and the status filter's toggle row
 * (`../../specifications/ui/DRP-drafts-panel.md`, DRP-FR-07 … DRP-FR-19).
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
import type { DraftHierarchy, DraftSummary } from "../types";
import { pickSelector, selectorValue } from "../test/selectors";
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
  // Overlay exclusivity (DRP-FR-16, DRP-FR-24) and the empty states (DRP-FR-06, DRP-FR-29, DRP-FR-15, SNV-FR-60, SNV-FR-61)
  // -------------------------------------------------------------------------

  it("DRP-FR-16, DRP-FR-24: at most one floating surface is open, and an inline field closes with it", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? nested() : undefined,
    );
    renderPanel();
    await screen.findByText("scratch");

    // A draft dropdown, then its delete confirmation: the dropdown is dismissed.
    await act(async () => void openDraftMenu("scratch"));
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Delete draft" })).toBeInTheDocument();

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(calls("delete_draft")).toHaveLength(0);

    // An open inline field closes, uncommitted, when a menu opens.
    await act(async () => void openFolderMenu("UI"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename" }));
    expect(screen.getByLabelText("Folder name")).toBeInTheDocument();
    await act(async () => void openFolderMenu("backend"));
    expect(screen.queryByLabelText("Folder name")).not.toBeInTheDocument();
    expect(calls("rename_drafts_folder")).toHaveLength(0);
  });

  it("DRP-FR-16: an outside click dismisses the open menu", async () => {
    renderPanel();
    await screen.findByText("artifact-window");
    await act(async () => void openDraftMenu("artifact-window"));
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await userEvent.click(document.body);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("DRP-FR-06, DRP-FR-29, DRP-FR-15, SNV-FR-60, SNV-FR-61: a worktree holding neither draft nor folder is the first-class empty state", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? { folders: [], drafts: [] } : undefined,
    );
    const { onCreateDraft } = renderPanel();
    await screen.findByText("No drafts yet.");

    expect(screen.queryByLabelText("Draft status")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Filter drafts")).not.toBeInTheDocument();
    // One button to activate, not the same action twice.
    expect(screen.getAllByRole("button", { name: /draft/i })).toHaveLength(1);
    await userEvent.click(screen.getByRole("button", { name: "New draft" }));
    expect(onCreateDraft).toHaveBeenCalled();
  });

  it("DRP-FR-06, DRP-FR-29, DRP-FR-15, SNV-FR-60, SNV-FR-61: a worktree holding folders but no draft is not that state", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts"
        ? { folders: [{ path: "UI", parent: "" }], drafts: [] }
        : undefined,
    );
    renderPanel();
    await screen.findByRole("treeitem", { name: "Folder UI" });

    expect(screen.queryByText("No drafts yet.")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Draft status")).toBeInTheDocument();
  });

  it("DRP-FR-06, DRP-FR-29, DRP-FR-15, SNV-FR-60, SNV-FR-61: a filter matching nothing keeps both controls and stays in the tree's region", async () => {
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "list_drafts") return flat();
      if (cmd === "search_drafts") return [];
      return undefined;
    });
    renderPanel();
    await screen.findByText("artifact-window");

    await userEvent.type(screen.getByLabelText("Filter drafts"), "nothing");
    expect(
      await screen.findByText("No draft matches this filter."),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Draft status")).toBeInTheDocument();
    expect(screen.getByLabelText("Filter drafts")).toHaveValue("nothing");
    expect(screen.queryByText("No drafts yet.")).not.toBeInTheDocument();
  });

  // -------------------------------------------------------------------------
  // Draft-row behaviour retained (DRP-FR-11, NAW-FR-25, DRP-FR-12, DRP-FR-18, DRP-FR-05, DRP-FR-07, NAW-FR-28)
  // -------------------------------------------------------------------------

  it("DRP-FR-11, NAW-FR-25: Escape leaves a draft rename uncommitted and the stored name untouched", async () => {
    renderPanel();
    await screen.findByText("artifact-window");

    await act(async () => void openDraftMenu("artifact-window"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename…" }));
    const field = screen.getByLabelText("Draft name");
    await userEvent.clear(field);
    await userEvent.type(field, "renamed{Escape}");

    expect(calls("rename_draft")).toHaveLength(0);
    expect(screen.queryByLabelText("Draft name")).not.toBeInTheDocument();
    expect(screen.getByText("artifact-window")).toBeInTheDocument();
  });

  it("DRP-FR-11: committing a draft rename renames its open tab and never moves it between folders", async () => {
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "list_drafts")
        return { folders: [{ path: "UI", parent: "" }], drafts: [{ ...DRAFTS[0], folder: "UI" }] };
      if (cmd === "rename_draft") return { ...DRAFTS[0], name: "renamed" };
      return undefined;
    });
    const { onDraftRenamed } = renderPanel({ expanded: ["UI"] });
    await screen.findByText("artifact-window");

    await act(async () => void openDraftMenu("artifact-window"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename…" }));
    const field = screen.getByLabelText("Draft name");
    await userEvent.clear(field);
    await userEvent.type(field, "renamed{Enter}");

    await waitFor(() =>
      expect(onDraftRenamed).toHaveBeenCalledWith("d-archived", "renamed"),
    );
    // No folder operation was invoked: a rename never files a draft elsewhere.
    expect(calls("move_draft_to_folder")).toHaveLength(0);
  });

  it("DRP-FR-11, NAW-FR-25: a name the binding cannot honour is refused against the field", async () => {
    invokeMock.mockImplementation(async (cmd) => {
      if (cmd === "list_drafts") return flat();
      if (cmd === "rename_draft")
        throw new Error("already exists in this draft: notes.md");
      return undefined;
    });
    renderPanel();
    await screen.findByText("artifact-window");

    await act(async () => void openDraftMenu("artifact-window"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Rename…" }));
    const field = screen.getByLabelText("Draft name");
    await userEvent.clear(field);
    await userEvent.type(field, "notes{Enter}");

    expect(await screen.findByRole("alert")).toHaveTextContent("notes.md");
    expect(screen.getByLabelText("Draft name")).toHaveValue("notes");
  });

  it("DRP-FR-12: Delete confirms first, then removes the draft and its tab", async () => {
    const { onDraftDeleted } = renderPanel();
    await screen.findByText("artifact-window");

    await act(async () => void openDraftMenu("artifact-window"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Delete" }));
    expect(calls("delete_draft")).toHaveLength(0);

    await userEvent.click(
      within(screen.getByRole("dialog", { name: "Delete draft" })).getByRole(
        "button",
        { name: "Delete" },
      ),
    );
    await waitFor(() => expect(onDraftDeleted).toHaveBeenCalledWith("d-archived"));
  });

  it("DRP-FR-18, DRP-FR-05, DRP-FR-07: Archive moves the status with no confirmation and leaves the folder alone", async () => {
    let listed: DraftHierarchy = {
      folders: [{ path: "UI", parent: "" }],
      drafts: [{ ...DRAFTS[1], folder: "UI" }],
    };
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "list_drafts") return listed;
      if (cmd === "set_draft_status") {
        const { id, status } = args as { id: string; status: string };
        listed = {
          ...listed,
          drafts: listed.drafts.map((d) =>
            d.id === id ? { ...d, status: status as DraftSummary["status"] } : d,
          ),
        };
        return { ...listed.drafts.find((d) => d.id === id) };
      }
      return undefined;
    });
    const { onDraftChanged, onDraftDeleted } = renderPanel({
      filter: "active",
      expanded: ["UI"],
    });
    await screen.findByText("editor-tweaks");

    await act(async () => void openDraftMenu("editor-tweaks"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Archive" }));

    await waitFor(() =>
      expect(screen.queryByText("editor-tweaks")).not.toBeInTheDocument(),
    );
    expect(calls("set_draft_status")[0][1]).toEqual({
      id: "d-active",
      status: "archived",
    });
    // Nothing was confirmed, nothing was deleted, nothing was moved.
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(calls("delete_draft")).toHaveLength(0);
    expect(calls("move_draft_to_folder")).toHaveLength(0);
    expect(onDraftDeleted).not.toHaveBeenCalled();
    expect(onDraftChanged).toHaveBeenCalled();
    // DRP-FR-18: the folder it was filed in remains.
    expect(folderRow("UI")).toBeInTheDocument();
  });

  it("DRP-FR-18, NAW-FR-28: an archived draft sits in the folder it was filed in and Restore returns it there", async () => {
    let listed: DraftHierarchy = {
      folders: [{ path: "UI", parent: "" }],
      drafts: [{ ...DRAFTS[0], folder: "UI" }],
    };
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "list_drafts") return listed;
      if (cmd === "set_draft_status") {
        const { id, status } = args as { id: string; status: string };
        listed = {
          ...listed,
          drafts: listed.drafts.map((d) =>
            d.id === id ? { ...d, status: status as DraftSummary["status"] } : d,
          ),
        };
        return { ...listed.drafts.find((d) => d.id === id) };
      }
      return undefined;
    });
    renderPanel({ filter: "archived", expanded: ["UI"] });
    await screen.findByText("artifact-window");
    // Carrying its timestamp like any other row.
    expect(document.querySelector(".draft-row__meta")).not.toBeNull();

    await act(async () => void openDraftMenu("artifact-window"));
    await userEvent.click(screen.getByRole("menuitem", { name: "Restore" }));

    await waitFor(() =>
      expect(calls("set_draft_status")[0][1]).toEqual({
        id: "d-archived",
        status: "active",
      }),
    );
  });
});

describe("a proposed change (DRP-FR-19)", () => {
  it("marks a row whose draft carries an undecided proposal", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts"
        ? flat([{ ...DRAFTS[1], hasPendingProposal: true }])
        : undefined,
    );
    renderPanel();
    await screen.findByText("editor-tweaks");

    expect(screen.getByTestId("draft-row-proposal")).toBeInTheDocument();
    // The panel offers no accept or decline control anywhere.
    expect(
      screen.queryByRole("button", { name: /accept|decline/i }),
    ).not.toBeInTheDocument();
  });

  it("carries no marker for a draft with nothing pending", async () => {
    renderPanel();
    await screen.findByText("editor-tweaks");
    expect(screen.queryByTestId("draft-row-proposal")).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// DRP-FR-07 / SNV-FR-62 / SNV-FR-58: the status filter is the shared toggle
// row, and it sits beneath the text filter.
//
// Every scenario above injects a position through the harness, which exercises
// the tree but never the control — the wiring from the row's activation back to
// `panel.setFilter` is what these two cover.
// ---------------------------------------------------------------------------

describe("the status filter's toggle row (DRP-FR-07)", () => {
  it("narrows the tree when a position is activated, not only when one is injected", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? flat() : undefined,
    );
    renderPanel({ filter: "active" });
    await screen.findByText("editor-tweaks");

    // The default position: the archived draft is one position away, not gone.
    expect(selectorValue("Draft status")).toBe("active");
    expect(screen.queryByText("artifact-window")).not.toBeInTheDocument();

    await pickSelector("Draft status", "archived");
    expect(selectorValue("Draft status")).toBe("archived");
    expect(await screen.findByText("artifact-window")).toBeInTheDocument();
    expect(screen.queryByText("editor-tweaks")).not.toBeInTheDocument();

    await pickSelector("Draft status", "all");
    expect(await screen.findByText("artifact-window")).toBeInTheDocument();
    expect(screen.getByText("editor-tweaks")).toBeInTheDocument();
  });

  it("sits beneath the text filter, both above the tree (SNV-FR-58)", async () => {
    invokeMock.mockImplementation(async (cmd) =>
      cmd === "list_drafts" ? flat() : undefined,
    );
    renderPanel({ filter: "all" });
    await screen.findByText("editor-tweaks");

    const controls = document.querySelector(".panel-controls")!;
    const text = screen.getByLabelText("Filter drafts");
    const status = screen.getByRole("radiogroup", { name: "Draft status" });
    const body = document.querySelector(".vpanel__body")!;

    expect(controls).toContainElement(text);
    expect(controls).toContainElement(status);
    expect(
      text.compareDocumentPosition(status) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      controls.compareDocumentPosition(body) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("DRP-FR-07: the graduated position admits a published draft as well as a graduated one", async () => {
    const rows: DraftSummary[] = [
      { id: "d-grad", name: "notes-cleanup", status: "graduated", folder: "", updatedAt: "2026-07-31T08:00:00Z" },
      { id: "d-pub", name: "relay-endpoint", status: "published", folder: "", updatedAt: "2026-07-31T07:30:00Z" },
      { id: "d-active", name: "editor-tweaks", status: "active", folder: "", updatedAt: "2026-07-31T07:00:00Z" },
      { id: "d-arch", name: "artifact-window", status: "archived", folder: "", updatedAt: "2026-07-31T06:00:00Z" },
    ];
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "list_drafts" ? { folders: [], drafts: rows } : undefined,
    );

    renderPanel({ filter: "graduated" });
    await screen.findByText("notes-cleanup");
    expect(screen.getByText("relay-endpoint")).toBeInTheDocument();
    // Neither of the other two statuses is admitted by that position.
    expect(screen.queryByText("editor-tweaks")).toBeNull();
    expect(screen.queryByText("artifact-window")).toBeNull();
  });

  it("DRP-FR-08: a graduated row and a published row each state their own status", async () => {
    const rows: DraftSummary[] = [
      { id: "d-grad", name: "notes-cleanup", status: "graduated", folder: "", updatedAt: "2026-07-31T08:00:00Z" },
      { id: "d-pub", name: "relay-endpoint", status: "published", folder: "", updatedAt: "2026-07-31T07:30:00Z" },
    ];
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "list_drafts" ? { folders: [], drafts: rows } : undefined,
    );

    // The two statuses stand together in the graduated position, so each row
    // has to say which it is.
    renderPanel({ filter: "graduated" });
    await screen.findByText("notes-cleanup");
    expect(screen.getByTestId("draft-graduated-marker")).toHaveTextContent("Graduated");
    expect(screen.getByTestId("draft-published-marker")).toHaveTextContent("Published");
    // And in the all-drafts position, where every status stands together.
    cleanup();
    renderPanel({ filter: "all" });
    await screen.findByText("notes-cleanup");
    expect(screen.getByTestId("draft-graduated-marker")).toBeInTheDocument();
    expect(screen.getByTestId("draft-published-marker")).toBeInTheDocument();
  });

  it("DRP-FR-18: the archive entry on a published draft archives it", async () => {
    const rows: DraftSummary[] = [
      { id: "d-pub", name: "relay-endpoint", status: "published", folder: "", updatedAt: "2026-07-31T07:30:00Z" },
    ];
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "list_drafts" ? { folders: [], drafts: rows } : undefined,
    );
    renderPanel({ filter: "all" });
    await screen.findByText("relay-endpoint");

    openDraftMenu("relay-endpoint");
    // It reads Archive rather than Restore: a published draft is not retired.
    const menu = await screen.findByRole("menu");
    const entry = within(menu).getByRole("menuitem", { name: "Archive" });
    await userEvent.click(entry);
    await waitFor(() => expect(calls("set_draft_status")).toHaveLength(1));
    expect(calls("set_draft_status")[0][1]).toEqual({
      id: "d-pub",
      status: "archived",
    });
  });

  it("DRP-FR-35: a published row is not locked — every entry is enabled and it drags", async () => {
    const rows: DraftSummary[] = [
      { id: "d-pub", name: "relay-endpoint", status: "published", folder: "", updatedAt: "2026-07-31T07:30:00Z" },
    ];
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "list_drafts" ? { folders: [], drafts: rows } : undefined,
    );
    renderPanel({ filter: "all" });
    await screen.findByText("relay-endpoint");

    openDraftMenu("relay-endpoint");
    const menu = await screen.findByRole("menu");
    // Publication is a fact about where the prompt was sent rather than a hold
    // on the draft, so nothing here is disabled the way a graduated row's is.
    for (const name of ["Open", "Rename…", "Move to Folder…", "Archive", "Delete"]) {
      expect(within(menu).getByRole("menuitem", { name }), name).toBeEnabled();
    }
    // And the panel offers no publication action of its own.
    expect(within(menu).queryByRole("menuitem", { name: /Publish/ })).toBeNull();
    await userEvent.keyboard("{Escape}");

    const row = screen.getByText("relay-endpoint").closest(".draft-row");
    expect(row).toHaveAttribute("draggable", "true");
  });
});

/**
 * How the Library context menu is composed: the Artifact Type submenu, the
 * folder-only creation entries, and the menu's own affordances
 * (`../../specifications/ui/LIB-library.md`, LCM-FR-04 … LCM-FR-08).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
const unlistenMock = vi.fn();
// Captured event subscribers, keyed by event name, so a test can fire the
// backend `"project tree changed"` event by hand.
let listeners: Record<string, (event: { payload: TreeChangedPayload }) => void> =
  {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (
      event: string,
      cb: (event: { payload: TreeChangedPayload }) => void,
    ) => {
      listeners[event] = cb;
      return unlistenMock;
    },
  ),
}));

import {
  baseTree,
  expandedAll,
  file,
  folder,
  makeTreeStubs,
  renderLibrary,
} from "../test/libraryFixtures";
import type { TreeChangedPayload } from "../types";
import { resetPanelReveals } from "../state/panelReveal";

const { mockInvoke, setLoadTree } = makeTreeStubs(invokeMock);

/** The payload of the most recent `save_library_panel_state` call, if any. */
/**
 * The commands invoked so far, minus the traffic that is not the panel acting on
 * the user's behalf: its own tree load and panel-state round trip, and
 * `append_log_records`. The log batch is flushed off a timer (`../logging`), so
 * whether it lands inside any given test is a matter of timing rather than
 * behaviour — leaving it in makes every exhaustive assertion below flaky.
 */
function commandsBeyondPanelTraffic(): string[] {
  return invokeMock.mock.calls
    .map((c) => c[0] as string)
    .filter(
      (cmd) =>
        cmd !== "load_project_tree" &&
        cmd !== "load_library_panel_state" &&
        cmd !== "save_library_panel_state" &&
        cmd !== "append_log_records",
    );
}

beforeEach(() => {
  // Module-level, like the preferences cache: reveal nonces are minted from
  // one counter for the app's life, so a suite that does not reset finds its
  // first request already consumed by the previous one.
  resetPanelReveals();
  invokeMock.mockReset();
  unlistenMock.mockReset();
  listeners = {};
});

afterEach(() => {
  cleanup();
});

describe("Library filesystem tree", () => {
  // LCM-FR-04: choosing a type on a FILE node assigns with
  // scope = file; **Clear Type**, now the last entry inside the same submenu,
  // invokes clear_artifact_type.
  it("assigns a file-scoped type and clears it from the submenu", async () => {
    mockInvoke(
      {
        load_project_tree: () => baseTree(),
        assign_artifact_type: () => baseTree(),
        clear_artifact_type: () => baseTree(),
      },
      expandedAll(baseTree()),
    );
    renderLibrary();

    // Artifact Type -> Spec on a file node, via the nested submenu.
    fireEvent.contextMenu(await screen.findByText("onboarding.md"));
    let menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Artifact Type"));
    fireEvent.click(within(menu).getByText("Spec"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("assign_artifact_type", {
        path: ".claude/skills/onboarding.md",
        artifactType: "spec",
        scope: "file",
      }),
    );

    // Clear Type sits at the bottom of the same submenu — open the submenu, then
    // choose it.
    fireEvent.contextMenu(screen.getByText("onboarding.md"));
    menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Artifact Type"));
    fireEvent.click(within(menu).getByText("Clear Type"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("clear_artifact_type", {
        path: ".claude/skills/onboarding.md",
      }),
    );
  });

  // LCM-FR-04: the artifact-type choices live under a nested "Artifact Type"
  // submenu rather than at the menu's top level, with Clear Type as the very
  // last entry inside it (below a divider).
  it("nests the type choices and Clear Type under the Artifact Type submenu", async () => {
    setLoadTree(baseTree());
    renderLibrary();

    fireEvent.contextMenu(await screen.findByText("onboarding.md"));
    const menu = document.querySelector(".menu") as HTMLElement;
    // The parent entry sits at the top level…
    expect(within(menu).getByText("Artifact Type")).toBeInTheDocument();
    // …but neither the type labels nor Clear Type show until the submenu opens.
    expect(within(menu).queryByText("Scratchpad")).not.toBeInTheDocument();
    expect(within(menu).queryByText("Clear Type")).not.toBeInTheDocument();

    fireEvent.click(within(menu).getByText("Artifact Type"));
    const sub = menu.querySelector(".menu--sub") as HTMLElement;
    expect(within(sub).getByText("Scratchpad")).toBeInTheDocument();
    expect(within(sub).getByText("Clear Type")).toBeInTheDocument();

    // Clear Type is the very last entry in the submenu, after a divider that
    // separates it from the eight type rows.
    const items = Array.from(sub.querySelectorAll(".menu-item"));
    expect(items[items.length - 1].textContent).toContain("Clear Type");
    expect(sub.querySelector(".menu-sep")).toBeInTheDocument();
  });

  // LCM-FR-01: a folder menu and an artifact file menu expose the right entry
  // sets; the file ops are shared, Notes is artifact-only, and Paste and the
  // three creation entries are folder-only.
  it("composes folder and skill menus from the applicability mapping", async () => {
    setLoadTree(baseTree());
    renderLibrary();
    await screen.findByText("specifications");

    // Folder menu: New Artifact/Delete/Copy/Rename/Paste/Artifact Type, no
    // Notes (LCM-FR-08 adds the folder-only New Artifact entry).
    fireEvent.contextMenu(screen.getByText("specifications"));
    let menu = document.querySelector(".menu") as HTMLElement;
    for (const label of [
      "New File",
      "New Artifact",
      "New Folder",
      "Delete",
      "Copy",
      "Rename",
      "Paste",
      "Artifact Type",
    ]) {
      expect(within(menu).getByText(label)).toBeInTheDocument();
    }
    expect(within(menu).queryByText("Notes")).not.toBeInTheDocument();
    expect(within(menu).queryByText("Inject…")).not.toBeInTheDocument();

    // Skill menu: Notes/Delete/Copy/Rename/Artifact Type, no Paste and none of
    // the three creation entries (folder-only, LCM-FR-08 / LCM-FR-09 /
    // LCM-FR-10).
    fireEvent.contextMenu(screen.getByText("onboarding.md"));
    menu = document.querySelector(".menu") as HTMLElement;
    for (const label of [
      "Notes",
      "Delete",
      "Copy",
      "Rename",
      "Artifact Type",
    ]) {
      expect(within(menu).getByText(label)).toBeInTheDocument();
    }
    expect(within(menu).queryByText("Paste")).not.toBeInTheDocument();
    expect(within(menu).queryByText("New File")).not.toBeInTheDocument();
    expect(within(menu).queryByText("New Artifact")).not.toBeInTheDocument();
    expect(within(menu).queryByText("New Folder")).not.toBeInTheDocument();
  });

  // NTA-FR-08, LCM-FR-09, LCM-FR-10 / LCM-FR-08: a folder menu leads with the creation cluster; a file
  // menu omits it; and selecting New Artifact opens that modal scoped to the
  // folder (no Editor/Flow tab opens here).
  it("offers a folder-only New Artifact entry that opens the modal scoped to the folder", async () => {
    setLoadTree(baseTree());
    const { onOpenArtifact, onShowNotes, onNewArtifact } = renderLibrary();

    // Folder menu: New File leads the creation cluster and carries a leading
    // icon (LCM-FR-10)…
    fireEvent.contextMenu(await screen.findByText("specifications"));
    let menu = document.querySelector(".menu") as HTMLElement;
    const newFile = within(menu).getByText("New File").closest(".menu-item")!;
    expect(menu.querySelector(".menu-item")).toBe(newFile);
    expect((newFile as HTMLElement).firstElementChild).toHaveClass("icon");
    // …then New Artifact, then New Folder, completing the three-entry cluster,
    // and then the one separator that divides it from the file ops (LCM-FR-08 /
    // LCM-FR-09).
    const newArtifact = within(menu).getByText("New Artifact").closest(".menu-item")!;
    expect((newArtifact as HTMLElement).firstElementChild).toHaveClass("icon");
    const newFolder = within(menu).getByText("New Folder").closest(".menu-item")!;
    expect(newFile.nextElementSibling).toBe(newArtifact);
    expect(newArtifact.nextElementSibling).toBe(newFolder);
    expect(newFolder.nextElementSibling).toHaveClass("menu-sep");

    // Selecting it hands the folder off to the modal — no backend op, no tab.
    fireEvent.click(within(menu).getByText("New Artifact"));
    expect(onNewArtifact).toHaveBeenCalledTimes(1);
    expect(onNewArtifact.mock.calls[0][0]).toMatchObject({
      path: "specifications",
      nodeKind: "folder",
    });
    // No backend operation beyond the panel's own load/restore traffic.
    expect(commandsBeyondPanelTraffic()).toEqual([]);
    expect(onOpenArtifact).not.toHaveBeenCalled();
    expect(onShowNotes).not.toHaveBeenCalled();

    // A file menu omits all three creation entries entirely (folder-only).
    fireEvent.contextMenu(screen.getByText("onboarding.md"));
    menu = document.querySelector(".menu") as HTMLElement;
    expect(within(menu).queryByText("New File")).not.toBeInTheDocument();
    expect(within(menu).queryByText("New Artifact")).not.toBeInTheDocument();
    expect(within(menu).queryByText("New Folder")).not.toBeInTheDocument();
  });

  /**
   * LCM-FR-08 / NTA-FR-08, LCM-FR-09, LCM-FR-10 with a REAL mouse press rather than a synthetic
   * click.
   *
   * The menu dismisses on `mousedown` as well as `click` (SNV-FR-56). Without a
   * containment guard that listener fires for a press *inside* the menu too,
   * unmounting it between `mousedown` and `mouseup` — and the browser then
   * dispatches no `click` at all, so every entry is dead to a pointer while
   * still passing under a dispatched `click`. `userEvent.click` sends the whole
   * sequence, which is exactly what a `fireEvent.click` test cannot see.
   */
  it("LCM-FR-08: a real mouse press on New Artifact opens the modal rather than dismissing the menu", async () => {
    setLoadTree(baseTree());
    const { onNewArtifact, onNewFile, onNewFolder } = renderLibrary();

    fireEvent.contextMenu(await screen.findByText("specifications"));
    await userEvent.click(screen.getByText("New Artifact"));
    expect(onNewArtifact).toHaveBeenCalledTimes(1);

    // The two entries beside it are on the same terms.
    fireEvent.contextMenu(screen.getByText("specifications"));
    await userEvent.click(screen.getByText("New File"));
    expect(onNewFile).toHaveBeenCalledTimes(1);

    fireEvent.contextMenu(screen.getByText("specifications"));
    await userEvent.click(screen.getByText("New Folder"));
    expect(onNewFolder).toHaveBeenCalledTimes(1);

    // And a press that begins OUTSIDE still dismisses, which is what the
    // listener is there for (LCM non-functional requirement / SNV-FR-56).
    fireEvent.contextMenu(screen.getByText("specifications"));
    expect(document.querySelector(".menu")).not.toBeNull();
    fireEvent.mouseDown(document.body);
    expect(document.querySelector(".menu")).toBeNull();
  });

  // NFW-FR-07 / LCM-FR-09: the folder-only New Folder entry hands the
  // right-clicked folder to the modal as its starting parent — and invokes
  // nothing itself, because the creation belongs to the modal (NFW-FR-09).
  it("offers a folder-only New Folder entry that hands the folder off as the starting parent", async () => {
    // `specifications` carries an associated type of its own; NFW-FR-07 requires
    // the modal to open with the type unset regardless, so nothing about that
    // type may be passed here.
    const typed = folder("specifications", true, [
      file("specifications/LIB-library.md", "spec", "inherited"),
    ]);
    typed.artifactType = "spec";
    typed.typeSource = "assigned";
    setLoadTree(folder("", true, [typed, file("AGENTS.md", "agent", "inferred")]));
    const { onNewFolder, onOpenArtifact, onNewArtifact } = renderLibrary();

    fireEvent.contextMenu(await screen.findByText("specifications"));
    let menu = document.querySelector(".menu") as HTMLElement;
    const entry = within(menu).getByText("New Folder").closest(".menu-item")!;
    // LCM-FR-07: a leading icon like every other entry.
    expect((entry as HTMLElement).firstElementChild).toHaveClass("icon");

    fireEvent.click(within(menu).getByText("New Folder"));
    expect(onNewFolder).toHaveBeenCalledTimes(1);
    expect(onNewFolder.mock.calls[0][0]).toMatchObject({
      path: "specifications",
      nodeKind: "folder",
    });
    // The menu only triggers the modal: no creation, no tab, no sibling action.
    expect(commandsBeyondPanelTraffic()).toEqual([]);
    expect(onOpenArtifact).not.toHaveBeenCalled();
    expect(onNewArtifact).not.toHaveBeenCalled();

    // A file node offers no New Folder entry.
    fireEvent.contextMenu(screen.getByText("AGENTS.md"));
    menu = document.querySelector(".menu") as HTMLElement;
    expect(within(menu).queryByText("New Folder")).not.toBeInTheDocument();
  });

  // NFI-FR-07 / LCM-FR-10: the folder-only New File entry hands the right-clicked
  // folder to the modal as its starting location — and invokes nothing itself,
  // because the creation belongs to the modal (NFI-FR-09).
  it("offers a folder-only New File entry that hands the folder off as the starting location", async () => {
    // The folder is artifact-bearing so the default **All artifacts** lens shows
    // it (LIB-FR-09); the entry itself is folder-only whatever the folder holds.
    setLoadTree(
      folder("", true, [
        folder("src/hooks", true, [
          file("src/hooks/useProjectFolders.md", "spec", "inferred"),
        ]),
        file("AGENTS.md", "agent", "inferred"),
      ]),
    );
    const { onNewFile, onOpenArtifact, onNewArtifact, onNewFolder } =
      renderLibrary();

    fireEvent.contextMenu(await screen.findByText("hooks"));
    let menu = document.querySelector(".menu") as HTMLElement;
    const entry = within(menu).getByText("New File").closest(".menu-item")!;
    // LCM-FR-07: a leading icon like every other entry.
    expect((entry as HTMLElement).firstElementChild).toHaveClass("icon");

    fireEvent.click(within(menu).getByText("New File"));
    expect(onNewFile).toHaveBeenCalledTimes(1);
    expect(onNewFile.mock.calls[0][0]).toMatchObject({
      path: "src/hooks",
      nodeKind: "folder",
    });
    // The menu only triggers the modal: no creation, no tab, no sibling action.
    expect(commandsBeyondPanelTraffic()).toEqual([]);
    expect(onOpenArtifact).not.toHaveBeenCalled();
    expect(onNewArtifact).not.toHaveBeenCalled();
    expect(onNewFolder).not.toHaveBeenCalled();
    // Selecting it also closes the menu.
    expect(document.querySelector(".menu")).toBeNull();

    // A file node offers no New File entry.
    fireEvent.contextMenu(screen.getByText("AGENTS.md"));
    menu = document.querySelector(".menu") as HTMLElement;
    expect(within(menu).queryByText("New File")).not.toBeInTheDocument();
  });

  // LCM-FR-01 / LCM-FR-05: Notes is the one entry that varies by
  // artifact type, and it follows whether the scan resolved a type at all —
  // every typed file offers it, an untyped one does not, and all of them offer
  // the same Delete / Copy / Rename / Artifact Type entries.
  it("offers Notes on typed files and withholds it from untyped ones", async () => {
    setLoadTree(baseTree());
    renderLibrary();

    const universal = ["Delete", "Copy", "Rename", "Artifact Type"];

    // Spec file: Notes present. New Artifact is folder-only so absent (LCM-FR-08).
    fireEvent.contextMenu(await screen.findByText("LIB-library.md"));
    let menu = document.querySelector(".menu") as HTMLElement;
    expect(within(menu).getByText("Notes")).toBeInTheDocument();
    for (const label of universal) {
      expect(within(menu).getByText(label)).toBeInTheDocument();
    }
    expect(within(menu).queryByText("New Artifact")).not.toBeInTheDocument();

    // Flow file: LCM-FR-05 names Flows explicitly.
    fireEvent.contextMenu(screen.getByText("review.flow"));
    menu = document.querySelector(".menu") as HTMLElement;
    expect(within(menu).getByText("Notes")).toBeInTheDocument();

    // Skill file: the same again — no entry is reserved to one type.
    fireEvent.contextMenu(screen.getByText("onboarding.md"));
    menu = document.querySelector(".menu") as HTMLElement;
    expect(within(menu).getByText("Notes")).toBeInTheDocument();
    for (const label of universal) {
      expect(within(menu).getByText(label)).toBeInTheDocument();
    }
    // No artifact-scoped staging action is offered anywhere in the menu.
    expect(within(menu).queryByText("Inject…")).not.toBeInTheDocument();
  });

  // LCM-FR-04: the Artifact Type submenu opens on mouse-over, without a click.
  it("opens the Artifact Type submenu on hover, without a click", async () => {
    setLoadTree(baseTree());
    renderLibrary();

    fireEvent.contextMenu(await screen.findByText("onboarding.md"));
    const menu = document.querySelector(".menu") as HTMLElement;
    // Closed initially — no type rows shown.
    expect(within(menu).queryByText("Scratchpad")).not.toBeInTheDocument();

    // Hover the parent entry (no click); the submenu opens. React derives
    // onMouseEnter from the native mouseover event.
    const parent = within(menu)
      .getByText("Artifact Type")
      .closest(".menu-item--parent") as HTMLElement;
    fireEvent.mouseOver(parent);

    expect(within(menu).getByText("Scratchpad")).toBeInTheDocument();
  });

  // LCM-FR-07: every entry and submenu carries a leading icon —
  // none renders without one.
  it("renders a leading icon on every menu entry, top-level and in the submenu", async () => {
    setLoadTree(baseTree());
    renderLibrary();

    // Open a skill menu, then expand the Artifact Type submenu so its eight type
    // rows and Clear Type are present too.
    fireEvent.contextMenu(await screen.findByText("onboarding.md"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Artifact Type"));

    const items = Array.from(menu.querySelectorAll<HTMLElement>(".menu-item"));
    // Sanity: top-level actions + parent + 8 types + Clear Type are all present.
    expect(items.length).toBeGreaterThanOrEqual(5 + 8 + 1);
    for (const item of items) {
      // The icon is the LEADING element of each entry (the parent row also has a
      // trailing caret, so assert the *first* element child is the icon — a bare
      // `querySelector` would pass on the caret alone).
      expect(item.firstElementChild).toHaveClass("icon");
    }
  });

  // LCM-FR-07: the folder menu's Paste entry also carries its leading icon.
  it("renders a leading icon on the folder Paste entry", async () => {
    setLoadTree(baseTree());
    renderLibrary();

    fireEvent.contextMenu(await screen.findByText("specifications"));
    const menu = document.querySelector(".menu") as HTMLElement;
    const paste = within(menu).getByText("Paste").closest(".menu-item")!;
    expect((paste as HTMLElement).querySelector("svg.icon")).not.toBeNull();
  });

  // LCM non-functional requirement: the menu dismisses on Escape without
  // invoking any action — including out of the inline delete-confirm mode.
  it("dismisses the menu on Escape without invoking an action", async () => {
    setLoadTree(baseTree());
    renderLibrary();

    // Open the menu and press Escape without choosing anything. (Delete is not
    // clicked here: it acts immediately now (LCM-FR-11), so clicking it would be
    // choosing an action rather than opening a dismissible sub-mode.)
    fireEvent.contextMenu(await screen.findByText("onboarding.md"));
    expect(document.querySelector(".menu")).not.toBeNull();

    fireEvent.keyDown(window, { key: "Escape" });

    // The menu is gone and no delete (or any other action) was invoked.
    expect(document.querySelector(".menu")).toBeNull();
    expect(invokeMock.mock.calls.some((c) => c[0] === "delete_path")).toBe(false);
    expect(screen.getByText("onboarding.md")).toBeInTheDocument();
  });
});

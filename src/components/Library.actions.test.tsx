/**
 * The Library context menu's file and folder actions: Notes, delete, rename,
 * copy and paste (`../../specifications/ui/LIB-library.md`, LCM-FR-04 …).
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
import { flushSync } from "react-dom";

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
  panelState,
  renderLibrary,
} from "../test/libraryFixtures";
import type { TreeChangedPayload } from "../types";
import { pressLandsOutside } from "./Library";
import { PROJECT_TREE_CHANGED } from "../events";
import { resetPanelReveals } from "../state/panelReveal";

const { mockInvoke, setLoadTree } = makeTreeStubs(invokeMock);

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
  // LCM-FR-04: an artifact file's context menu offers a Notes action that
  // switches the Notes panel to that artifact's scope without opening a tab.
  it("offers a Notes action that scopes Notes to the artifact, opening no tab", async () => {
    setLoadTree(baseTree());
    const { onShowNotes, onOpenArtifact } = renderLibrary();

    fireEvent.contextMenu(await screen.findByText("onboarding.md"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Notes"));

    expect(onShowNotes).toHaveBeenCalledWith(
      expect.objectContaining({ name: "onboarding.md", artifactType: "skill" }),
    );
    // No Editor/Flow tab is opened by the Notes action.
    expect(onOpenArtifact).not.toHaveBeenCalled();
  });

  // LCM-FR-01: a folder's context menu has no Notes entry — Notes is for a file
  // the scan resolved a type for.
  it("does not offer Notes on a folder context menu", async () => {
    setLoadTree(baseTree());
    renderLibrary();
    await screen.findByText("specifications");

    fireEvent.contextMenu(screen.getByText("specifications"));
    const menu = document.querySelector(".menu") as HTMLElement;
    expect(within(menu).queryByText("Notes")).not.toBeInTheDocument();
    // …but the universal file operations are present.
    expect(within(menu).getByText("Delete")).toBeInTheDocument();
    expect(within(menu).getByText("Rename")).toBeInTheDocument();
  });

  // LCM-FR-01, LCM-FR-05: Delete invokes `delete_path` after confirmation; the node then
  // disappears on the watcher-driven reload while expand state is preserved.
  it("deletes a file via delete_path, then drops it on reload, preserving state", async () => {
    // After the delete, the next tree load no longer contains the spec file.
    const afterDelete = folder("", true, [
      folder(".claude", true, [
        folder(".claude/skills", true, [
          file(".claude/skills/onboarding.md", "skill", "inferred"),
        ]),
      ]),
      file("AGENTS.md", "agent", "inferred"),
      file("review.flow", "flow", "inferred"),
    ]);
    mockInvoke(
      {
        load_project_tree: () =>
          invokeMock.mock.calls.filter((c) => c[0] === "load_project_tree")
            .length === 1
            ? baseTree()
            : afterDelete,
        delete_path: () => undefined,
      },
      expandedAll(baseTree()),
    );
    renderLibrary();

    // Collapse `.claude` first so we can prove the reload preserves expand state.
    fireEvent.click(await screen.findByText(".claude"));
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();

    // Delete the spec file via the menu's inline confirm.
    fireEvent.contextMenu(screen.getByText("specifications"));
    let menu = document.querySelector(".menu") as HTMLElement;
    // (Open the spec folder's child via the tree, then delete the file node.)
    fireEvent.click(screen.getByText("AGENTS.md")); // dismiss menu / select something
    fireEvent.contextMenu(screen.getByText("LIB-library.md"));
    menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Delete"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_path", {
        // LCM-FR-11: the first call is always the narrow one — a file is removed
        // by it and no confirmation is raised.
        recursive: false,
        path: "specifications/LIB-library.md",
      }),
    );

    // The watcher fires; the reloaded tree no longer has the deleted file…
    listeners[PROJECT_TREE_CHANGED]?.({ payload: { changeCount: 1 } });
    await waitFor(() =>
      expect(screen.queryByText("LIB-library.md")).not.toBeInTheDocument(),
    );
    // …and `.claude` is still collapsed (expand state preserved, LIB-FR-06).
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();
  });

  // LCM-FR-11: **Cancel** on the recursive-delete confirmation issues no second
  // call and leaves the folder alone. The companion to the Escape and
  // outside-click paths, all three of which the requirement names.
  it("does not delete recursively when the confirmation is cancelled", async () => {
    const deleteCalls: unknown[] = [];
    mockInvoke(
      {
        load_project_tree: () => baseTree(),
        delete_path: (args) => {
          deleteCalls.push(args);
          throw "directory not empty";
        },
      },
      expandedAll(baseTree()),
    );
    renderLibrary();

    fireEvent.contextMenu(await screen.findByText("specifications"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Delete"));

    const dialog = await screen.findByRole("dialog", { name: "Delete folder" });
    fireEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Delete folder" })).toBeNull(),
    );
    // Only the probe went out; nothing was removed.
    expect(deleteCalls).toEqual([{ path: "specifications", recursive: false }]);
    expect(screen.getByText("specifications")).toBeInTheDocument();
  });

  // LCM-FR-02, LCM-FR-11, LIB-FR-06: Rename invokes `rename_path` with a valid basename; a name with a
  // path separator is rejected client-side and makes no call.
  it("renames a file via rename_path with a valid basename", async () => {
    mockInvoke(
      { load_project_tree: () => baseTree(), rename_path: () => undefined },
      expandedAll(baseTree()),
    );
    renderLibrary();

    fireEvent.contextMenu(await screen.findByText("onboarding.md"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Rename"));
    const input = screen.getByLabelText("New name");
    fireEvent.change(input, { target: { value: "welcome.md" } });
    fireEvent.keyDown(input, { key: "Enter" });

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("rename_path", {
        path: ".claude/skills/onboarding.md",
        newName: "welcome.md",
      }),
    );
  });

  /**
   * LCM-FR-02, LCM-FR-11, LIB-FR-06 (regression): pressing **Rename** opens the rename input, and the
   * press does not also dismiss the menu.
   *
   * The menu swaps its entry list for the rename input, so the entry that was
   * pressed leaves the document as the press is still being delivered. Read as an
   * outside press, that closed the menu on its way into rename mode and
   * **Rename** did nothing at all in the shipped window.
   *
   * The ordering that exposes it is a browser property. A browser empties the JS
   * stack between the listeners along one propagation path, so React's re-render
   * lands between its own root-container listener and the `window` listener that
   * dismisses on an outside press; jsdom runs the whole path in a single task and
   * re-renders only after it, which is why every test here stayed green with the
   * control dead. The `document` listener below stands in for that checkpoint: it
   * sits between the two on the path, and flushing React's pending work there
   * reproduces the browser's ordering exactly.
   */
  it("opens the rename input under a browser's flush ordering", async () => {
    mockInvoke(
      { load_project_tree: () => baseTree(), rename_path: () => undefined },
      expandedAll(baseTree()),
    );
    renderLibrary();

    fireEvent.contextMenu(await screen.findByText("onboarding.md"));
    const menu = document.querySelector(".menu") as HTMLElement;
    const flush = () => flushSync(() => {});
    document.addEventListener("click", flush);
    try {
      fireEvent.click(within(menu).getByText("Rename"));
    } finally {
      document.removeEventListener("click", flush);
    }

    expect(document.querySelector(".menu")).not.toBeNull();
    expect(screen.getByLabelText("New name")).toBeInTheDocument();
  });

  // LCM-FR-02: the rename input's own **Rename** button submits, under a real
  // press rather than a synthetic click — it lives in the same menu frame as the
  // entry that opened it, and closing the menu is part of what it does.
  it("submits the rename from its button under a real press", async () => {
    mockInvoke(
      { load_project_tree: () => baseTree(), rename_path: () => undefined },
      expandedAll(baseTree()),
    );
    renderLibrary();

    fireEvent.contextMenu(await screen.findByText("onboarding.md"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Rename"));
    fireEvent.change(screen.getByLabelText("New name"), {
      target: { value: "welcome.md" },
    });
    await userEvent.click(
      screen.getByRole("button", { name: "Rename" }),
    );

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("rename_path", {
        path: ".claude/skills/onboarding.md",
        newName: "welcome.md",
      }),
    );
    expect(document.querySelector(".menu")).toBeNull();
  });

  // LCM (dismissal): a bare `click` outside the menu dismisses it, with no
  // `mousedown` before it. The menu listens on both events, and this is the one
  // the other listener cannot stand in for — a keyboard-driven activation raises
  // a `click` alone.
  it("dismisses the menu on an outside click", async () => {
    setLoadTree(baseTree());
    renderLibrary();

    fireEvent.contextMenu(await screen.findByText("onboarding.md"));
    expect(document.querySelector(".menu")).not.toBeNull();

    fireEvent.click(document.body);
    await waitFor(() => expect(document.querySelector(".menu")).toBeNull());
  });

  // LCM (dismissal): rename mode dismisses on an outside press like any other
  // mode, and abandons the rename rather than submitting it. The rename branch
  // renders its own subtree, so it carries the guard separately from the entry
  // list.
  it("dismisses rename mode on an outside press without renaming", async () => {
    setLoadTree(baseTree());
    renderLibrary();

    fireEvent.contextMenu(await screen.findByText("onboarding.md"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Rename"));
    fireEvent.change(screen.getByLabelText("New name"), {
      target: { value: "welcome.md" },
    });

    fireEvent.mouseDown(document.body);
    await waitFor(() => expect(document.querySelector(".menu")).toBeNull());
    expect(invokeMock.mock.calls.some((c) => c[0] === "rename_path")).toBe(
      false,
    );
  });

  /**
   * The rule the dismissal guard rests on, asserted on its own: a target the
   * press itself took out of the document is not an outside press.
   *
   * This holds for any detached target, not only one the menu owns — a row the
   * watcher-driven reload (LIB-FR-10) removed under the pointer answers the same
   * way. That is deliberate: an outside press dismisses on its `mousedown`, which
   * runs before anything has re-rendered, so the `click` that follows has nothing
   * left to dismiss.
   */
  it("reads a press whose target the menu detached as inside, not outside", () => {
    const frame = document.createElement("div");
    const entry = document.createElement("div");
    frame.appendChild(entry);
    document.body.appendChild(frame);
    const elsewhere = document.createElement("div");
    document.body.appendChild(elsewhere);

    try {
      // Inside the frame, and outside it: unchanged by the guard.
      expect(pressLandsOutside(entry, frame)).toBe(false);
      expect(pressLandsOutside(elsewhere, frame)).toBe(true);
      // A press with no target at all has nothing placing it in the menu, and
      // neither has one that arrives before the frame is mounted.
      expect(pressLandsOutside(null, frame)).toBe(true);
      expect(pressLandsOutside(elsewhere, null)).toBe(true);

      // The regression: the re-render took the pressed entry out of the document
      // before the listener ran. It is still the entry the user pressed.
      frame.removeChild(entry);
      expect(pressLandsOutside(entry, frame)).toBe(false);
    } finally {
      frame.remove();
      elsewhere.remove();
    }
  });

  it("rejects an invalid rename (separator, backslash, or empty) without calling the backend", async () => {
    setLoadTree(baseTree());
    renderLibrary();
    await screen.findByText("onboarding.md");

    // Each invalid basename is rejected client-side with an inline hint and no
    // backend call: a forward slash, a backslash, and an empty/whitespace name.
    for (const bad of ["sub/evil.md", "sub\\evil.md", "   "]) {
      fireEvent.contextMenu(screen.getByText("onboarding.md"));
      const menu = document.querySelector(".menu") as HTMLElement;
      fireEvent.click(within(menu).getByText("Rename"));
      const input = screen.getByLabelText("New name");
      fireEvent.change(input, { target: { value: bad } });
      fireEvent.keyDown(input, { key: "Enter" });

      // The input stays open with the validation hint instead of doing nothing.
      expect(
        screen.getByText(/Enter a name|cannot contain a path separator/),
      ).toBeInTheDocument();
      fireEvent.keyDown(input, { key: "Escape" });
    }

    expect(
      invokeMock.mock.calls.some((c) => c[0] === "rename_path"),
    ).toBe(false);
  });

  // TAB-FR-19 / LCM-FR-11: a non-empty folder's delete comes back as the typed
  // "not empty" having removed nothing, which is what raises the recursive
  // confirmation. Dismissing invokes nothing further; confirming re-invokes the
  // same operation with `recursive` set.
  it("raises the recursive-delete confirmation only when the backend reports a folder non-empty", async () => {
    const deleteCalls: unknown[] = [];
    mockInvoke(
      {
        load_project_tree: () => baseTree(),
        delete_path: (args) => {
          deleteCalls.push(args);
          const { recursive } = args as { recursive: boolean };
          if (!recursive) throw new Error("directory not empty: specifications");
          return undefined;
        },
      },
      expandedAll(baseTree()),
    );
    renderLibrary();
    await screen.findByText("specifications");

    fireEvent.contextMenu(screen.getByText("specifications"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Delete"));

    // The confirmation names the folder and says what is at stake.
    const dialog = await screen.findByRole("dialog", { name: "Delete folder" });
    expect(within(dialog).getByText(/specifications.*is not empty/)).toBeInTheDocument();
    expect(deleteCalls).toEqual([
      { path: "specifications", recursive: false },
    ]);

    // Escape dismisses without a second call and without removing anything.
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Delete folder" })).toBeNull(),
    );
    expect(deleteCalls).toHaveLength(1);

    // Repeat, and confirm this time.
    fireEvent.contextMenu(screen.getByText("specifications"));
    const menu2 = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu2).getByText("Delete"));
    const dialog2 = await screen.findByRole("dialog", { name: "Delete folder" });
    fireEvent.click(within(dialog2).getByRole("button", { name: "Delete" }));

    // The second attempt probes again before asking — the panel holds no memory
    // that this folder was non-empty a moment ago, which is correct: the
    // filesystem may have changed since, and the probe is cheap.
    await waitFor(() =>
      expect(deleteCalls).toEqual([
        { path: "specifications", recursive: false },
        { path: "specifications", recursive: false },
        { path: "specifications", recursive: true },
      ]),
    );
  });

  // PST-FR-18 / LCM-FR-11: the prompt comes from the BACKEND's answer, not from
  // the rendered tree. `vendor/` renders with no children at all — its contents
  // are gitignored, so the scan surfaces none of them (ASC-FR-09) — and the
  // confirmation must still appear.
  //
  // This is the test that makes the two-phase design mean anything: without it,
  // deciding emptiness from `node.children` passes the whole suite while
  // silently removing subtrees the user was never warned about.
  it("prompts from the backend's answer even when the tree shows the folder as empty", async () => {
    const treeWithHiddenContents = folder("", true, [
      folder("vendor", false, []),
      file("AGENTS.md", "agent", "inferred"),
    ]);
    const deleteCalls: unknown[] = [];
    mockInvoke(
      {
        load_project_tree: () => treeWithHiddenContents,
        delete_path: (args) => {
          deleteCalls.push(args);
          const { recursive } = args as { recursive: boolean };
          // The real wire payload: `mutation_error` maps FsError::NotEmpty to
          // this bare contract string, not to the primitive's Display text.
          if (!recursive) throw "directory not empty";
          return undefined;
        },
      },
      panelState({ artifactTypeFilter: "all_files" }),
    );
    renderLibrary();
    const vendorRow = await screen.findByText("vendor");

    // Premise: the panel really is rendering it as childless.
    expect(
      (treeWithHiddenContents.children ?? []).find((c) => c.path === "vendor")
        ?.children,
    ).toEqual([]);

    fireEvent.contextMenu(vendorRow);
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Delete"));

    expect(
      await screen.findByRole("dialog", { name: "Delete folder" }),
    ).toBeInTheDocument();
    expect(deleteCalls).toEqual([{ path: "vendor", recursive: false }]);
  });

  // LCM-FR-12: renaming a node to the name it already has is refused client-side
  // — no backend call at all — and the input stays open carrying the value so
  // the user can correct it rather than retype it.
  it("refuses a rename to the unchanged name without calling the backend", async () => {
    setLoadTree(baseTree());
    renderLibrary();
    await screen.findByText("onboarding.md");

    fireEvent.contextMenu(screen.getByText("onboarding.md"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Rename"));
    const input = screen.getByLabelText("New name") as HTMLInputElement;

    // The input is seeded with the current name, so submitting it untouched is
    // the exact case this refuses — and the likeliest way a user hits it.
    expect(input.value).toBe("onboarding.md");
    fireEvent.keyDown(input, { key: "Enter" });

    expect(screen.getByText(/already the name/)).toBeInTheDocument();
    expect(input.value).toBe("onboarding.md");
    expect(invokeMock.mock.calls.some((c) => c[0] === "rename_path")).toBe(false);

    // Correcting it proceeds normally.
    fireEvent.change(input, { target: { value: "welcome.md" } });
    fireEvent.keyDown(input, { key: "Enter" });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("rename_path", {
        path: ".claude/skills/onboarding.md",
        newName: "welcome.md",
      }),
    );
  });

  // LCM-FR-02: Paste is greyed while the clipboard is empty; after a Copy it
  // pastes the copied path into the right-clicked folder.
  it("greys Paste until a Copy fills the clipboard, then copies into the folder", async () => {
    mockInvoke(
      {
        load_project_tree: () => baseTree(),
        copy_path_into_folder: () => undefined,
      },
      expandedAll(baseTree()),
    );
    renderLibrary();
    await screen.findByText("specifications");

    // Clipboard empty -> Paste on a folder is disabled.
    fireEvent.contextMenu(screen.getByText("specifications"));
    let menu = document.querySelector(".menu") as HTMLElement;
    expect(within(menu).getByText("Paste")).toHaveAttribute("aria-disabled", "true");

    // Copy a file (closes the menu, fills the clipboard).
    fireEvent.contextMenu(screen.getByText("onboarding.md"));
    menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Copy"));

    // Paste into the folder is now enabled and invokes copy_path_into_folder.
    fireEvent.contextMenu(screen.getByText("specifications"));
    menu = document.querySelector(".menu") as HTMLElement;
    const paste = within(menu).getByText("Paste");
    expect(paste).toHaveAttribute("aria-disabled", "false");
    fireEvent.click(paste);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("copy_path_into_folder", {
        sourcePath: ".claude/skills/onboarding.md",
        destFolder: "specifications",
      }),
    );
  });

  // LCM-FR-03: a paste collision surfaces the backend's typed error inline and
  // leaves the tree unchanged.
  it("surfaces a paste collision error inline and leaves the tree unchanged", async () => {
    mockInvoke(
      {
        load_project_tree: () => baseTree(),
        copy_path_into_folder: () => {
          throw "destination already exists: specifications/onboarding.md";
        },
      },
      expandedAll(baseTree()),
    );
    renderLibrary();
    await screen.findByText("specifications");

    // Copy a file, then paste into a folder that already holds that name.
    fireEvent.contextMenu(screen.getByText("onboarding.md"));
    let menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Copy"));
    fireEvent.contextMenu(screen.getByText("specifications"));
    menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Paste"));

    // The typed error is shown inline…
    expect(await screen.findByText(/already exists/)).toBeInTheDocument();
    // …and the tree is unchanged (the folder and file are still present).
    expect(screen.getByText("specifications")).toBeInTheDocument();
    expect(screen.getByText("onboarding.md")).toBeInTheDocument();
  });

  // LCM-FR-03: the clipboard is copy-only — a Paste does not clear it, so the
  // same source can be pasted into several folders.
  it("keeps the clipboard after a paste so the source pastes into many folders", async () => {
    mockInvoke(
      {
        load_project_tree: () => baseTree(),
        copy_path_into_folder: () => undefined,
      },
      expandedAll(baseTree()),
    );
    renderLibrary();
    await screen.findByText("specifications");

    // Copy a file.
    fireEvent.contextMenu(screen.getByText("onboarding.md"));
    let menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Copy"));

    // Paste into folder #1.
    fireEvent.contextMenu(screen.getByText("specifications"));
    menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Paste"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("copy_path_into_folder", {
        sourcePath: ".claude/skills/onboarding.md",
        destFolder: "specifications",
      }),
    );

    // The clipboard is NOT cleared: Paste into folder #2 is still enabled and
    // issues a second copy with the same source.
    fireEvent.contextMenu(screen.getByText(".claude"));
    menu = document.querySelector(".menu") as HTMLElement;
    const paste2 = within(menu).getByText("Paste");
    expect(paste2).toHaveAttribute("aria-disabled", "false");
    fireEvent.click(paste2);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("copy_path_into_folder", {
        sourcePath: ".claude/skills/onboarding.md",
        destFolder: ".claude",
      }),
    );
  });
});

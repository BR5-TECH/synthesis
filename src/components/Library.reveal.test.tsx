/**
 * The Library reveal: how a freshly-created artifact is selected and made
 * visible, and how the loaded tree is published to the shell
 * (`../../specifications/ui/LIB-library.md`, LIB-FR-18, NAW-FR-10, NFW-FR-04).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
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
  allFolderIds,
  expandedAll,
  file,
  folder,
  LibraryHost,
  makeTreeStubs,
  panelState,
  renderLibrary,
} from "../test/libraryFixtures";
import { PERSIST_DEBOUNCE_MS } from "../hooks/useLibraryPanelState";
import type { LibraryPanelState, TreeChangedPayload } from "../types";
import { PROJECT_TREE_CHANGED } from "../events";
import { selectorValue } from "../test/selectors";
import { resetPanelReveals } from "../state/panelReveal";

const { mockInvoke, setLoadTree } = makeTreeStubs(invokeMock);

function lastSave(): LibraryPanelState | null {
  const calls = invokeMock.mock.calls.filter(
    (c) => c[0] === "save_library_panel_state",
  );
  if (calls.length === 0) return null;
  return (calls[calls.length - 1][1] as { state: LibraryPanelState }).state;
}

/**
 * Wait for a write to land. The coalescing window eats a third of `waitFor`'s
 * default budget before the call is even issued, which is a thin margin on an
 * oversubscribed CI runner — so these waits get an explicit one rather than
 * relying on the default.
 */
const SAVE_TIMEOUT = { timeout: PERSIST_DEBOUNCE_MS + 2500 };

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

// NAW-FR-10: after a creation the Library reveals and selects the new artifact.
describe("reveal/select a freshly-created artifact (NAW-FR-10)", () => {
  function renderWithReveal(revealId: string | null) {
    return render(<LibraryHost revealId={revealId} />);
  }

  it("selects the node whose id matches revealId", async () => {
    setLoadTree(baseTree());
    renderWithReveal("specifications/LIB-library.md");

    const row = (await screen.findByText("LIB-library.md")).closest(
      ".tree-row",
    ) as HTMLElement;
    await waitFor(() => expect(row).toHaveAttribute("data-selected", "true"));
  });

  it("expands a collapsed ancestor folder so the revealed node is visible", async () => {
    // `specifications` is not in the persisted expanded set, so it renders
    // collapsed (LIB-FR-15) and its child is hidden.
    setLoadTree(baseTree(), panelState());
    const { rerender } = renderWithReveal(null);

    await screen.findByText("specifications");
    expect(screen.queryByText("LIB-library.md")).not.toBeInTheDocument();

    // Revealing a child expands the ancestor and surfaces the row.
    rerender(<LibraryHost revealId="specifications/LIB-library.md" />);
    expect(await screen.findByText("LIB-library.md")).toBeInTheDocument();
  });

  // LIB-FR-17 / LIB-FR-18: a reveal ends with the node visible, so a lens that
  // would hide it is switched to All files and a text filter that would hide it
  // is cleared — and both changes persist like any other filter change
  // (LIB-FR-17). This is the path NFW-FR-11 takes after creating an untyped
  // folder, which the default lens would otherwise swallow.
  it("relaxes the lens and the text filter that would hide a revealed node, and persists both", async () => {
    const tree = folder("", true, [
      file("AGENTS.md", "agent", "inferred"),
      folder("notes-inbox", false, []),
    ]);
    setLoadTree(
      tree,
      panelState({
        expandedPaths: allFolderIds(tree),
        artifactTypeFilter: "all_artifacts",
        textFilter: "agents",
      }),
    );
    const { rerender } = renderWithReveal(null);

    await screen.findByText("AGENTS.md");
    // Under All artifacts + the text filter, the new empty folder is hidden.
    expect(screen.queryByText("notes-inbox")).not.toBeInTheDocument();

    rerender(<LibraryHost revealId="notes-inbox" />);

    // The lens dropped to All files and the text filter was cleared, so the row
    // is visible and selected.
    expect(await screen.findByText("notes-inbox")).toBeInTheDocument();
    expect(selectorValue("Filter by type")).toBe("files");
    expect(screen.getByLabelText("Filter tree")).toHaveValue("");
    await waitFor(() =>
      expect(
        screen.getByText("notes-inbox").closest(".tree-row"),
      ).toHaveAttribute("data-selected", "true"),
    );
    // LIB-FR-17: the relaxed filters are written, not just applied in memory.
    await waitFor(
      () => {
        expect(lastSave()?.artifactTypeFilter).toBe("all_files");
        expect(lastSave()?.textFilter).toBe("");
      },
      SAVE_TIMEOUT,
    );
  });

  it("LIB-FR-18: relaxes ONLY the lens when the lens alone is hiding the node", async () => {
    // The text filter matches the node's name, so clearing it would be gratuitous
    // — and would throw away something the user typed.
    const tree = folder("", true, [
      file("AGENTS.md", "agent", "inferred"),
      folder("inbox", false, []),
    ]);
    setLoadTree(
      tree,
      panelState({
        expandedPaths: allFolderIds(tree),
        artifactTypeFilter: "all_artifacts",
        textFilter: "inbox",
      }),
    );
    const { rerender } = renderWithReveal(null);
    await screen.findByLabelText("Filter tree");
    expect(screen.queryByText("inbox")).not.toBeInTheDocument();

    rerender(<LibraryHost revealId="inbox" />);

    expect(await screen.findByText("inbox")).toBeInTheDocument();
    expect(selectorValue("Filter by type")).toBe("files");
    expect(screen.getByLabelText("Filter tree")).toHaveValue("inbox");
  });

  it("LIB-FR-18: clears ONLY the text filter when the text filter alone is hiding the node", async () => {
    // The lens admits the node (it carries a `spec` assignment) so the lens must
    // survive; only the text filter is in the way.
    const typed = folder("drafts", true, []);
    typed.artifactType = "spec";
    typed.typeSource = "assigned";
    const tree = folder("", true, [typed, file("AGENTS.md", "agent", "inferred")]);
    setLoadTree(
      tree,
      panelState({
        expandedPaths: allFolderIds(tree),
        artifactTypeFilter: "spec",
        textFilter: "zzz",
      }),
    );
    const { rerender } = renderWithReveal(null);
    await screen.findByLabelText("Filter tree");
    expect(screen.queryByText("drafts")).not.toBeInTheDocument();

    rerender(<LibraryHost revealId="drafts" />);

    expect(await screen.findByText("drafts")).toBeInTheDocument();
    expect(screen.getByLabelText("Filter tree")).toHaveValue("");
    expect(selectorValue("Filter by type")).toBe("spec");
  });

  it("LIB-FR-18: relaxes only the lens when the two filters hide the node jointly", async () => {
    // The subtle case. `docs/` matches the text filter by NAME and holds a file
    // matching the LENS — so each filter admits it on its own, yet no single
    // descendant satisfies both and the folder is hidden. Dropping the lens to All
    // files is enough on its own (the name still matches), so the user's text
    // filter must be left alone.
    const tree = folder("", true, [
      folder("docs", true, [file("docs/s.md", "spec", "inferred")]),
      file("AGENTS.md", "agent", "inferred"),
    ]);
    setLoadTree(
      tree,
      panelState({
        expandedPaths: [],
        artifactTypeFilter: "spec",
        textFilter: "docs",
      }),
    );
    const { rerender } = renderWithReveal(null);
    await screen.findByLabelText("Filter tree");
    expect(screen.queryByText("docs")).not.toBeInTheDocument();

    rerender(<LibraryHost revealId="docs" />);

    expect(await screen.findByText("docs")).toBeInTheDocument();
    expect(selectorValue("Filter by type")).toBe("files");
    expect(screen.getByLabelText("Filter tree")).toHaveValue("docs");
  });

  it("LIB-FR-18, LIB-FR-17: reveals a node nested under a collapsed, filter-hidden ancestor", async () => {
    // The literal scenario: expand the ancestors AND relax the filter, together.
    const tree = folder("", true, [
      folder("docs", false, [folder("docs/inbox", false, [])]),
      file("AGENTS.md", "agent", "inferred"),
    ]);
    setLoadTree(
      tree,
      panelState({ expandedPaths: [], artifactTypeFilter: "all_artifacts" }),
    );
    const { rerender } = renderWithReveal(null);
    await screen.findByText("AGENTS.md");
    // Neither the ancestor nor the target is visible under All artifacts.
    expect(screen.queryByText("docs")).not.toBeInTheDocument();
    expect(screen.queryByText("inbox")).not.toBeInTheDocument();

    rerender(<LibraryHost revealId="docs/inbox" />);

    expect(await screen.findByText("inbox")).toBeInTheDocument();
    expect(screen.getByText("docs")).toBeInTheDocument();
    expect(selectorValue("Filter by type")).toBe("files");
    await waitFor(() =>
      expect(screen.getByText("inbox").closest(".tree-row")).toHaveAttribute(
        "data-selected",
        "true",
      ),
    );
    // NFW-FR-11: revealed collapsed — the node itself is never expanded, only its
    // ancestors.
    await waitFor(() =>
      expect(lastSave()?.expandedPaths).toEqual(["docs"]),
      SAVE_TIMEOUT,
    );
  });

  it("LIB-FR-06: a later tree reload does not re-assert a finished reveal", async () => {
    // `revealId` is a standing prop, not a one-shot event. An effect that re-ran on
    // every reload would re-select the old target and re-expand a folder the user
    // had since collapsed — on every watcher-driven reload, forever.
    const tree = baseTree();
    setLoadTree(tree);
    render(<LibraryHost revealId="specifications/LIB-library.md" />);

    const revealed = (await screen.findByText("LIB-library.md")).closest(
      ".tree-row",
    ) as HTMLElement;
    await waitFor(() =>
      expect(revealed).toHaveAttribute("data-selected", "true"),
    );

    // The user moves on: collapses the revealed node's folder and selects
    // something else.
    fireEvent.click(screen.getByText("specifications"));
    fireEvent.click(screen.getByText("AGENTS.md"));
    expect(screen.queryByText("LIB-library.md")).not.toBeInTheDocument();
    expect(
      screen.getByText("AGENTS.md").closest(".tree-row"),
    ).toHaveAttribute("data-selected", "true");

    // An unrelated structural change fires; the panel reloads.
    listeners[PROJECT_TREE_CHANGED]?.({ payload: { changeCount: 1 } });

    await waitFor(() =>
      expect(
        screen.getByText("AGENTS.md").closest(".tree-row"),
      ).toHaveAttribute("data-selected", "true"),
    );
    // Neither the selection nor the collapse was stolen back.
    expect(screen.queryByText("LIB-library.md")).not.toBeInTheDocument();
  });

  it("leaves the filters alone when revealing a node the active lens already admits", async () => {
    // The second half of LIB-FR-18: only the filters that would ACTUALLY hide the
    // node are touched. A reveal that reset the lens unconditionally would throw
    // away a lens the user had deliberately chosen.
    const typed = folder("drafts", true, []);
    typed.artifactType = "spec";
    typed.typeSource = "assigned";
    const tree = folder("", true, [typed, file("AGENTS.md", "agent", "inferred")]);
    setLoadTree(
      tree,
      panelState({ expandedPaths: allFolderIds(tree), artifactTypeFilter: "spec" }),
    );
    const { rerender } = renderWithReveal(null);

    // The typed folder is already visible under its own type lens (LIB-FR-09).
    expect(await screen.findByText("drafts")).toBeInTheDocument();

    rerender(<LibraryHost revealId="drafts" />);

    await waitFor(() =>
      expect(screen.getByText("drafts").closest(".tree-row")).toHaveAttribute(
        "data-selected",
        "true",
      ),
    );
    // Untouched.
    expect(selectorValue("Filter by type")).toBe("spec");
  });
});

// NFW-FR-04: the Library is the one surface that scans, so the folder set the
// New Folder window offers as parents comes from the tree published here rather
// than from a scan of its own.
describe("publishing the loaded tree to the shell (NFW-FR-04)", () => {
  it("hands each freshly-loaded tree up, including after a watcher reload", async () => {
    const first = baseTree();
    setLoadTree(first);
    const { onTreeLoaded } = renderLibrary();

    await screen.findByText("AGENTS.md");
    await waitFor(() => expect(onTreeLoaded).toHaveBeenCalledTimes(1));
    expect(onTreeLoaded.mock.calls[0][0]).toEqual(first);

    // LIB-FR-10: the watcher-driven reload publishes the new tree too, so a
    // folder created since the last look is offered as a parent.
    const second = folder("", true, [
      ...(first.children ?? []),
      folder("brand-new", false, []),
    ]);
    setLoadTree(second);
    listeners[PROJECT_TREE_CHANGED]?.({ payload: { changeCount: 1 } });

    await waitFor(() => expect(onTreeLoaded).toHaveBeenCalledTimes(2));
    expect(onTreeLoaded.mock.calls[1][0]).toEqual(second);
  });

  it("publishes the tree a manual rescan returns", async () => {
    // LIB-FR-11's rescan exists precisely for the case where the watcher missed a
    // change, so it is the last thing that should leave the shell's folder list on
    // a stale tree.
    const rescanned = folder("", true, [
      file("AGENTS.md", "agent", "inferred"),
      folder("appeared-offline", false, []),
    ]);
    mockInvoke(
      {
        load_project_tree: () => structuredClone(baseTree()),
        rescan_project_tree: () => structuredClone(rescanned),
      },
      expandedAll(baseTree()),
    );
    const { onTreeLoaded } = renderLibrary();
    await waitFor(() => expect(onTreeLoaded).toHaveBeenCalledTimes(1));

    await userEvent.click(screen.getByLabelText("Rescan project tree"));

    await waitFor(() => expect(onTreeLoaded).toHaveBeenCalledTimes(2));
    expect(onTreeLoaded.mock.calls[1][0]).toEqual(rescanned);
  });
});

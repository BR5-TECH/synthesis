/**
 * Shared fixtures for the Library panel test files (`Library.test.tsx` and its
 * `Library.<topic>.test.tsx` siblings).
 *
 * Everything here is independent of the `@tauri-apps` module mocks, which are
 * hoisted and file-scoped: each test file declares its own `invokeMock` and
 * hands it to `makeTreeStubs` to get the two backend stubs back.
 */
import { render } from "@testing-library/react";
import { vi } from "vitest";
import type { Mock } from "vitest";
import { useMemo } from "react";

import { Library } from "../components/Library";
import { useLibraryPanelState } from "../hooks/useLibraryPanelState";
import type {
  LibraryPanelState,
  OpenableArtifact,
  TreeNode,
} from "../types";

/** Ticket source for the reveal requests this host mints — see `LibraryHost`. */
let revealSeq = 0;


// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

export function file(
  path: string,
  artifactType?: TreeNode["artifactType"],
  typeSource?: TreeNode["typeSource"],
): TreeNode {
  const name = path.split("/").pop()!;
  return { id: path, name, path, nodeKind: "file", artifactType, typeSource };
}

export function folder(path: string, hasArtifacts: boolean, children: TreeNode[]): TreeNode {
  const name = path.split("/").pop()!;
  return {
    id: path,
    name,
    path,
    nodeKind: "folder",
    hasArtifacts,
    children,
  };
}

// A representative on-disk tree: a Claude skill, a spec, a top-level Codex
// agent + a top-level Flow, an unclassified file, and an EMPTY folder
// (`misc`, hasArtifacts=false) that must be hidden by default.
export function baseTree(): TreeNode {
  return folder("", true, [
    folder(".claude", true, [
      folder(".claude/skills", true, [
        file(".claude/skills/onboarding.md", "skill", "inferred"),
      ]),
    ]),
    folder("specifications", true, [
      file("specifications/LIB-library.md", "spec", "inferred"),
    ]),
    folder("misc", false, [file("misc/todo.txt")]),
    file("AGENTS.md", "agent", "inferred"),
    file("review.flow", "flow", "inferred"),
    file("notes.txt"),
  ]);
}

/** Every folder id in a tree, excluding the unnamed root (which is not rendered). */
export function allFolderIds(node: TreeNode): string[] {
  if (node.nodeKind !== "folder") return [];
  const here = node.id === "" ? [] : [node.id];
  return here.concat((node.children ?? []).flatMap(allFolderIds));
}

export function panelState(over: Partial<LibraryPanelState> = {}): LibraryPanelState {
  return {
    expandedPaths: [],
    artifactTypeFilter: "all_artifacts",
    textFilter: "",
    ...over,
  };
}

/**
 * Folders render collapsed unless the persisted state says otherwise
 * (LIB-FR-15). Most tests below are about filtering, the context menu, or
 * reloads rather than about the expansion default, so they render as a user who
 * has been in this project would see it: everything already expanded. The tests
 * that ARE about the default pass their own state.
 */
export function expandedAll(tree: TreeNode): LibraryPanelState {
  return panelState({ expandedPaths: allFolderIds(tree) });
}

/**
 * Stands in for `VPanel`, which owns the Library's expand/filter state so it
 * outlives a switch to another vertical-panel surface (LIB-FR-06). Rendering
 * the real hook here means these tests exercise the restore/persist machinery
 * as it actually ships, and `surface` lets a test reproduce a surface switch —
 * which unmounts `Library` while the state above it stays alive.
 */
export function LibraryHost({
  surface = "library",
  onOpenArtifact = vi.fn(),
  onShowNotes = vi.fn(),
  onNewFile = vi.fn(),
  onNewArtifact = vi.fn(),
  onNewFolder = vi.fn(),
  onTreeLoaded,
  revealId,
  onViewSpecMap,
}: {
  surface?: "library" | "other";
  onOpenArtifact?: (node: OpenableArtifact) => void;
  onShowNotes?: (artifact: OpenableArtifact) => void;
  onNewFile?: (folder: TreeNode) => void;
  onNewArtifact?: (folder: TreeNode) => void;
  onNewFolder?: (folder: TreeNode) => void;
  onTreeLoaded?: (tree: TreeNode) => void;
  revealId?: string | null;
  onViewSpecMap?: () => void;
}) {
  const panel = useLibraryPanelState();
  // The panel takes a nonce-bearing request rather than a bare id (LIB-FR-18).
  // Minted here so the host's own `revealId` prop keeps reading as it did: a new
  // id is a new request, and re-rendering with the SAME id is not — which is
  // exactly what the "a later tree reload does not re-assert a finished reveal"
  // case below relies on.
  const reveal = useMemo(
    () =>
      revealId
        ? {
            panel: "library" as const,
            id: revealId,
            nonce: ++revealSeq,
            // These cases are all creation reveals, which name a node that may
            // still be arriving and are the author asking to be taken to it.
            optimistic: true,
            focus: true,
          }
        : null,
    [revealId],
  );
  if (surface !== "library") return <div>other surface</div>;
  return (
    <Library
      panel={panel}
      onOpenArtifact={onOpenArtifact}
      onShowNotes={onShowNotes}
      onNewFile={onNewFile}
      onNewArtifact={onNewArtifact}
      onNewFolder={onNewFolder}
      onTreeLoaded={onTreeLoaded}
      reveal={reveal}
      onViewSpecMap={onViewSpecMap}
    />
  );
}

export function renderLibrary() {
  const onOpenArtifact = vi.fn<(node: OpenableArtifact) => void>();
  const onShowNotes = vi.fn<(artifact: OpenableArtifact) => void>();
  const onNewFile = vi.fn<(folder: TreeNode) => void>();
  const onNewArtifact = vi.fn<(folder: TreeNode) => void>();
  const onNewFolder = vi.fn<(folder: TreeNode) => void>();
  const onTreeLoaded = vi.fn<(tree: TreeNode) => void>();
  const view = render(
    <LibraryHost
      onOpenArtifact={onOpenArtifact}
      onShowNotes={onShowNotes}
      onNewFile={onNewFile}
      onNewArtifact={onNewArtifact}
      onNewFolder={onNewFolder}
      onTreeLoaded={onTreeLoaded}
    />,
  );
  return {
    onOpenArtifact,
    onShowNotes,
    onNewFile,
    onNewArtifact,
    onNewFolder,
    onTreeLoaded,
    ...view,
  };
}

/**
 * The tree/panel-state backend stubs, bound to the calling file's own
 * `invoke` mock.
 */
export function makeTreeStubs(invokeMock: Mock) {
  /**
   * Install an invoke mock. The panel-state load/save are always answered,
   * because every render issues the load (LIB-FR-14) and any state change issues
   * the save (LIB-FR-17); `handlers` covers what the test itself is about.
   */
  function mockInvoke(
    handlers: Record<string, (args?: unknown) => unknown>,
    state: LibraryPanelState,
  ) {
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "load_library_panel_state") return state;
      if (cmd === "save_library_panel_state") return undefined;
      const handler = handlers[cmd];
      if (!handler) throw new Error(`unexpected invoke ${cmd}`);
      return handler(args);
    });
  }

  /**
   * A fresh deep copy per call, deliberately.
   *
   * `invoke` deserialises a new object on every round trip, so each reload gives
   * the component a tree with a *different identity* even when the content is
   * unchanged — which is what makes `setTree` re-render and any effect keyed on the
   * tree re-run. Handing back one shared object instead would silently switch that
   * off and hide every bug that lives in a reload (a reveal re-asserting itself, a
   * one-shot guard that isn't).
   */
  function setLoadTree(tree: TreeNode, state?: LibraryPanelState) {
    mockInvoke(
      { load_project_tree: () => structuredClone(tree) },
      state ?? expandedAll(tree),
    );
  }

  return { mockInvoke, setLoadTree };
}

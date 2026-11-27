/**
 * Shared fixtures for the Library panel test files (`Library.test.tsx` and its
 * `Library.<topic>.test.tsx` siblings).
 *
 * Everything here is independent of the `@tauri-apps` module mocks, which are
 * hoisted and file-scoped: each test file declares its own `invokeMock` and
 * hands it to `makeTreeStubs` to get the two backend stubs back.
 */
import { render, waitFor } from "@testing-library/react";
import { expect, vi } from "vitest";
import type { Mock } from "vitest";
import { useMemo } from "react";

import { Library } from "../components/Library";
import { FILTER_DEBOUNCE_MS } from "../components/Library/useTreeFilter";
import { TREE_COMMANDS, treeCommand } from "./treeCommands";
import { useLibraryPanelState } from "../hooks/useLibraryPanelState";
import type {
  LibraryPanelState,
  OpenableArtifact,
  TerminalSummary,
  TreeNode,
} from "../types";

/**
 * Wait until no folder of the panel is loading (LIB-FR-JZNT). Each expanded
 * folder loads its children on its own read (LIB-FR-EMOO), so a nested row
 * arrives after its top-level ancestor. A test that reads nested rows calls
 * this first rather than depend on how fast the reads answer.
 */
export async function waitForTree(options?: { timeout?: number }) {
  await waitFor(() => expect(document.querySelector(".tree-row--loading")).toBeNull(), options);
}

/**
 * The wait for a text filter result (LIB-FR-PPAT). The request starts only
 * after the rest in typing, which eats into the default budget of `waitFor`
 * before the call is even sent, so these waits get an explicit one.
 */
export const FILTER_TIMEOUT = { timeout: FILTER_DEBOUNCE_MS + 2500 };

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
  revealId,
  onViewSpecMap,
  onOpenTerminal,
  hasActiveWorktree,
}: {
  surface?: "library" | "other";
  onOpenArtifact?: (node: OpenableArtifact) => void;
  onShowNotes?: (artifact: OpenableArtifact) => void;
  onNewFile?: (folder: TreeNode) => void;
  onNewArtifact?: (folder: TreeNode) => void;
  onNewFolder?: (folder: TreeNode) => void;
  revealId?: string | null;
  onViewSpecMap?: () => void;
  onOpenTerminal?: (session: TerminalSummary) => void;
  hasActiveWorktree?: boolean;
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
      reveal={reveal}
      onViewSpecMap={onViewSpecMap}
      onOpenTerminal={onOpenTerminal}
      hasActiveWorktree={hasActiveWorktree}
    />
  );
}

export function renderLibrary() {
  const onOpenArtifact = vi.fn<(node: OpenableArtifact) => void>();
  const onShowNotes = vi.fn<(artifact: OpenableArtifact) => void>();
  const onNewFile = vi.fn<(folder: TreeNode) => void>();
  const onNewArtifact = vi.fn<(folder: TreeNode) => void>();
  const onNewFolder = vi.fn<(folder: TreeNode) => void>();
  const view = render(
    <LibraryHost
      onOpenArtifact={onOpenArtifact}
      onShowNotes={onShowNotes}
      onNewFile={onNewFile}
      onNewArtifact={onNewArtifact}
      onNewFolder={onNewFolder}
    />,
  );
  return {
    onOpenArtifact,
    onShowNotes,
    onNewFile,
    onNewArtifact,
    onNewFolder,
    ...view,
  };
}

/**
 * The handlers of one `mockInvoke` call. `tree` is the nested fixture the
 * tree reads are answered from (`treeCommands.ts`): it is called on each read,
 * so a test that changes what it returns changes what the next read sees. A
 * handler named after a tree read takes precedence over `tree`.
 */
export type TreeHandlers = Record<string, (args?: unknown) => unknown> & {
  tree?: () => TreeNode;
};

/** The number of `invoke` calls of `cmd`, with `path` when it is given. */
export function countCalls(invokeMock: Mock, cmd: string, path?: string): number {
  return invokeMock.mock.calls.filter(
    (c) =>
      c[0] === cmd &&
      (path === undefined || (c[1] as { path?: string } | undefined)?.path === path),
  ).length;
}

/**
 * The tree/panel-state backend stubs, bound to the calling file's own
 * `invoke` mock.
 */
export function makeTreeStubs(invokeMock: Mock) {
  /**
   * Install an invoke mock. The panel-state load/save and the log flush are
   * always answered, because every render issues the load (LIB-FR-14), any
   * state change issues the save (LIB-FR-17), and a logged warning flushes in
   * the background; `handlers` covers what the test itself is about. Any other
   * command rejects.
   */
  function mockInvoke(handlers: TreeHandlers, state: LibraryPanelState) {
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "load_library_panel_state") return state;
      if (cmd === "save_library_panel_state") return undefined;
      if (cmd === "append_log_records") return undefined;
      const handler = handlers[cmd];
      if (handler && cmd !== "tree") return handler(args);
      if (handlers.tree && TREE_COMMANDS.has(cmd)) {
        return treeCommand(cmd, args, handlers.tree());
      }
      throw new Error(`unexpected invoke ${cmd}`);
    });
  }

  /**
   * Answer every tree read from `tree`. `treeCommand` builds new objects on
   * each read, as `invoke` deserialises a new object on every round trip, so
   * each reload gives the component nodes with a *different identity* even
   * when the content is unchanged. That keeps every bug that lives in a reload
   * (a reveal re-asserting itself, a one-shot guard that isn't) visible.
   */
  function setTree(tree: TreeNode, state?: LibraryPanelState) {
    mockInvoke({ tree: () => tree }, state ?? expandedAll(tree));
  }

  return { mockInvoke, setTree };
}

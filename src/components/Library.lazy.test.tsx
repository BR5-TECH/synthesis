/**
 * The Project panel loads one folder at a time and renders only the rows in
 * view (`../../specifications/ui/LIB-library.md` LIB-FR-EMOO, LIB-FR-JZNT,
 * LIB-FR-10, LIB-FR-PPAT, LIB-FR-BXMZ, LIB-FR-VCJO, LIB-FR-VIAZ, LIB-FR-18,
 * LIB-FR-19).
 */
import { resetLogBufferForTest } from "../logging";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";

const invokeMock = vi.fn();
// Captured event subscribers, keyed by event name, so a test can fire the
// backend `"project tree changed"` event by hand.
let listeners: Record<string, (event: { payload: TreeChangedPayload }) => void> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (event: { payload: TreeChangedPayload }) => void) => {
      listeners[event] = cb;
      return () => undefined;
    },
  ),
}));

import { Library } from "./Library";
import { FILTER_DEBOUNCE_MS } from "./Library/useTreeFilter";
import { ROW_HEIGHT } from "./Library/VirtualRows";
import { useLibraryPanelState } from "../hooks/useLibraryPanelState";
import {
  baseTree,
  countCalls,
  file,
  folder,
  LibraryHost,
  makeTreeStubs,
  panelState,
  renderLibrary,
  FILTER_TIMEOUT,
  waitForTree,
} from "../test/libraryFixtures";
import { treeCommand, wireNode } from "../test/treeCommands";
import { selectorValues } from "../test/selectors";
import { PROJECT_TREE_CHANGED } from "../events";
import { resetPanelReveals, revealAlreadyConsumed } from "../state/panelReveal";
import type { TreeChangedPayload, TreeNode } from "../types";

const { mockInvoke, setTree } = makeTreeStubs(invokeMock);

/** The folder reads of `path` so far. */
const reads = (path: string) => countCalls(invokeMock, "list_tree_children", path);

/** The row of the node whose name is `name`. */
const rowOf = (name: string) => screen.getByText(name).closest(".tree-row") as HTMLElement;

function fireTreeChanged(changedFolders: string[]) {
  act(() => {
    listeners[PROJECT_TREE_CHANGED]?.({ payload: { changeCount: 1, changedFolders } });
  });
}

/** A gate a mock can hold a read behind until the test opens it. */
function gate() {
  let open: () => void = () => undefined;
  const wait = new Promise<void>((resolve) => {
    open = resolve;
  });
  return { wait, open };
}

beforeEach(() => {
  resetLogBufferForTest();
  resetPanelReveals();
  invokeMock.mockReset();
  listeners = {};
});

afterEach(() => {
  vi.useRealTimers();
  cleanup();
});

describe("one folder at a time (LIB-FR-EMOO)", () => {
  it("LIB-FR-EMOO: reads the root on mount, and no folder that does not render expanded", async () => {
    // `.claude/skills` is in the expanded set, but its parent is collapsed, so
    // it does not render at all. `specifications` renders expanded, and its
    // child shows that the stored state was restored.
    setTree(baseTree(), panelState({ expandedPaths: [".claude/skills", "specifications"] }));
    renderLibrary();
    await screen.findByText("LIB-library.md");
    await waitForTree();

    expect(reads("")).toBe(1);
    expect(reads("specifications")).toBe(1);
    expect(reads(".claude")).toBe(0);
    expect(reads(".claude/skills")).toBe(0);
    // The panel holds no node of a folder it has not loaded.
    expect(screen.queryByText("skills")).not.toBeInTheDocument();
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();
  });

  it("LIB-FR-EMOO: loads a folder when it first renders expanded, and only once", async () => {
    setTree(baseTree(), panelState({ expandedPaths: [".claude/skills"] }));
    renderLibrary();
    await screen.findByText("specifications");
    await waitForTree();

    fireEvent.click(screen.getByText("specifications"));
    expect(await screen.findByText("LIB-library.md")).toBeInTheDocument();
    expect(reads("specifications")).toBe(1);

    // Collapse and open again: the loaded children show at once, with no read.
    fireEvent.click(screen.getByText("specifications"));
    expect(screen.queryByText("LIB-library.md")).not.toBeInTheDocument();
    fireEvent.click(screen.getByText("specifications"));
    expect(screen.getByText("LIB-library.md")).toBeInTheDocument();
    expect(reads("specifications")).toBe(1);

    // Opening `.claude` puts the expanded `.claude/skills` on a row, so that
    // folder loads now, after its parent.
    fireEvent.click(screen.getByText(".claude"));
    expect(await screen.findByText("onboarding.md")).toBeInTheDocument();
    expect(reads(".claude")).toBe(1);
    expect(reads(".claude/skills")).toBe(1);
    expect(reads("")).toBe(1);
  });
});

describe("carets and loading rows (LIB-FR-JZNT)", () => {
  it("LIB-FR-JZNT: a folder whose hasChildren is false has no caret, and an expanded one reads nothing", async () => {
    const tree = folder("", true, [
      folder("empty", true, []),
      folder("docs", true, [file("docs/a.md", "spec", "inferred")]),
      folder("closed", true, [file("closed/b.md", "spec", "inferred")]),
    ]);
    // Both `empty` and `docs` render expanded; `docs`'s child shows that the
    // stored state was restored.
    setTree(
      tree,
      panelState({ artifactTypeFilter: "all_files", expandedPaths: ["empty", "docs"] }),
    );
    renderLibrary();
    await screen.findByText("a.md");
    await waitForTree();

    expect(rowOf("empty").querySelector(".tree-row__caret svg")).toBeNull();
    expect(rowOf("docs").querySelector(".tree-row__caret svg")).not.toBeNull();
    expect(rowOf("closed").querySelector(".tree-row__caret svg")).not.toBeNull();
    // The expanded empty folder shows no loading row and is never read.
    expect(screen.queryByRole("status", { name: "Loading folder" })).not.toBeInTheDocument();
    expect(reads("empty")).toBe(0);
    expect(reads("docs")).toBe(1);
  });

  it("LIB-FR-JZNT: a folder whose children are loading shows a loading row, and the rest of the tree stays usable", async () => {
    const held = gate();
    const tree = baseTree();
    mockInvoke(
      {
        tree: () => tree,
        list_tree_children: async (args) => {
          if ((args as { path: string }).path === "specifications") await held.wait;
          return treeCommand("list_tree_children", args, tree);
        },
      },
      panelState({ expandedPaths: ["specifications"] }),
    );
    const { onOpenArtifact } = renderLibrary();
    await screen.findByText("specifications");

    const loading = await screen.findByRole("status", { name: "Loading folder" });
    // The loading row sits directly under its folder.
    expect(rowOf("specifications").nextElementSibling).toBe(loading);

    // The rest of the tree answers while the folder waits.
    fireEvent.click(screen.getByText("AGENTS.md"));
    expect(onOpenArtifact).toHaveBeenCalledWith(
      expect.objectContaining({ id: "AGENTS.md", artifactType: "agent" }),
    );
    expect(rowOf("AGENTS.md")).toHaveAttribute("data-selected", "true");

    held.open();
    expect(await screen.findByText("LIB-library.md")).toBeInTheDocument();
    expect(screen.queryByRole("status", { name: "Loading folder" })).not.toBeInTheDocument();
  });
});

describe("reload of changed folders (LIB-FR-10)", () => {
  it("LIB-FR-10: reloads each loaded folder that changedFolders names, and no other folder", async () => {
    let current = baseTree();
    mockInvoke({ tree: () => current }, panelState({ expandedPaths: ["specifications"] }));
    renderLibrary();
    await screen.findByText("LIB-library.md");
    await waitForTree();
    expect(reads("")).toBe(1);
    expect(reads("specifications")).toBe(1);

    // The disk changes in three folders. The event names two of them: one
    // the panel holds and one it has not loaded.
    current = baseTree();
    const specs = current.children!.find((c) => c.id === "specifications")!;
    specs.children!.push(file("specifications/NEW.md", "spec", "inferred"));
    const claude = current.children!.find((c) => c.id === ".claude")!;
    claude.children!.push(file(".claude/CLAUDE.md", "instructions", "inferred"));
    current.children!.push(file("ROOT-NEW.md", "spec", "inferred"));
    fireTreeChanged(["specifications", ".claude"]);

    expect(await screen.findByText("NEW.md")).toBeInTheDocument();
    expect(reads("specifications")).toBe(2);
    // `.claude` is not loaded, so it is not read.
    expect(reads(".claude")).toBe(0);
    // The root is not named, so it is not read, and its new file is not shown.
    expect(reads("")).toBe(1);
    expect(screen.queryByText("ROOT-NEW.md")).not.toBeInTheDocument();

    // A later event that names the root brings that file in.
    fireTreeChanged([""]);
    expect(await screen.findByText("ROOT-NEW.md")).toBeInTheDocument();
    expect(reads("")).toBe(2);
    expect(reads("specifications")).toBe(2);
  });

  it("LIB-FR-10: runs the text filter again on a tree change", async () => {
    let current = baseTree();
    mockInvoke(
      { tree: () => current },
      panelState({ expandedPaths: ["specifications"], textFilter: "LIB" }),
    );
    renderLibrary();
    expect(await screen.findByText("LIB-library.md")).toBeInTheDocument();
    // LIB-FR-PPAT: a loading row stands for the tree until the filter answers.
    await waitForTree(FILTER_TIMEOUT);
    await waitFor(
      () => expect(screen.queryByText("AGENTS.md")).not.toBeInTheDocument(),
      FILTER_TIMEOUT,
    );
    const before = countCalls(invokeMock, "filter_project_tree");

    current = baseTree();
    current.children!.find((c) => c.id === "specifications")!.children!.push(
      file("specifications/LIB-two.md", "spec", "inferred"),
    );
    fireTreeChanged(["specifications"]);

    expect(await screen.findByText("LIB-two.md", {}, FILTER_TIMEOUT)).toBeInTheDocument();
    expect(countCalls(invokeMock, "filter_project_tree")).toBe(before + 1);
  });
});

describe("the text filter (LIB-FR-PPAT)", () => {
  it("LIB-FR-PPAT: sends one filter request after a rest in typing, for the last text", async () => {
    setTree(baseTree(), panelState({ expandedPaths: ["specifications"] }));
    renderLibrary();
    await screen.findByText("LIB-library.md");
    await waitForTree();

    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    const input = screen.getByLabelText("Filter tree");
    // Each keystroke comes before the rest ends, so each one starts the rest
    // again.
    const step = FILTER_DEBOUNCE_MS - 50;
    fireEvent.change(input, { target: { value: "o" } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(step);
    });
    fireEvent.change(input, { target: { value: "onb" } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(step);
    });
    fireEvent.change(input, { target: { value: "onboard" } });
    expect(countCalls(invokeMock, "filter_project_tree")).toBe(0);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(FILTER_DEBOUNCE_MS - 1);
    });
    expect(countCalls(invokeMock, "filter_project_tree")).toBe(0);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(countCalls(invokeMock, "filter_project_tree")).toBe(1);
    expect(invokeMock).toHaveBeenCalledWith("filter_project_tree", {
      text: "onboard",
      lens: "all_artifacts",
      limit: 2000,
    });
    await act(async () => {
      await vi.runOnlyPendingTimersAsync();
    });
    vi.useRealTimers();

    // The result shows the ancestor of the match, and hides the rest.
    await waitFor(() => expect(screen.queryByText("AGENTS.md")).not.toBeInTheDocument());
    expect(screen.getByText(".claude")).toBeInTheDocument();
    expect(screen.queryByText("specifications")).not.toBeInTheDocument();
    expect(screen.queryByText("LIB-library.md")).not.toBeInTheDocument();
  });

  it("LIB-FR-PPAT: finds a match in a folder the panel has never loaded, and keeps the expand state", async () => {
    setTree(baseTree(), panelState({ expandedPaths: [] }));
    renderLibrary();
    await screen.findByText("specifications");
    await waitForTree();

    fireEvent.change(screen.getByLabelText("Filter tree"), { target: { value: "LIB" } });

    // `specifications` is collapsed, so its match does not show under it yet.
    // LIB-FR-PPAT: a loading row stands for the tree until the filter answers.
    await waitForTree(FILTER_TIMEOUT);
    await waitFor(
      () => expect(screen.queryByText("AGENTS.md")).not.toBeInTheDocument(),
      FILTER_TIMEOUT,
    );
    expect(screen.getByText("specifications")).toBeInTheDocument();
    expect(screen.queryByText("LIB-library.md")).not.toBeInTheDocument();

    // Opening it shows the match from the result, with no folder read.
    fireEvent.click(screen.getByText("specifications"));
    expect(screen.getByText("LIB-library.md")).toBeInTheDocument();
    expect(reads("specifications")).toBe(0);
  });

  it("LIB-FR-PPAT: while the filter has not answered, a loading row stands for the tree", async () => {
    const held = gate();
    mockInvoke(
      {
        tree: () => baseTree(),
        filter_project_tree: async () => {
          await held.wait;
          return { root: [wireNode(file("AGENTS.md", "agent", "inferred"))], truncated: false };
        },
      },
      panelState(),
    );
    renderLibrary();
    await screen.findByText("review.flow");
    await waitForTree();

    fireEvent.change(screen.getByLabelText("Filter tree"), { target: { value: "agents" } });

    // No unfiltered row shows while the answer is held.
    await waitFor(() => expect(screen.getByRole("status", { name: "Loading folder" })).toBeInTheDocument());
    expect(screen.queryByText("review.flow")).not.toBeInTheDocument();
    await waitFor(() => expect(countCalls(invokeMock, "filter_project_tree")).toBe(1), FILTER_TIMEOUT);

    held.open();
    expect(await screen.findByText("AGENTS.md")).toBeInTheDocument();
    expect(screen.queryByRole("status", { name: "Loading folder" })).not.toBeInTheDocument();
  });

  it("LIB-FR-PPAT: a truncated result ends with a row that says more matches exist", async () => {
    mockInvoke(
      {
        tree: () => baseTree(),
        filter_project_tree: () => ({
          root: [wireNode(file("AGENTS.md", "agent", "inferred"))],
          truncated: true,
        }),
      },
      panelState(),
    );
    renderLibrary();
    await screen.findByText("AGENTS.md");
    await waitForTree();
    expect(screen.queryByRole("note")).not.toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("Filter tree"), { target: { value: "a" } });

    const note = await screen.findByRole("note", {}, FILTER_TIMEOUT);
    expect(note).toHaveTextContent("More matches exist");
    expect(screen.getByText("AGENTS.md")).toBeInTheDocument();
    expect(screen.queryByText("specifications")).not.toBeInTheDocument();

    // Clearing the text drops the result and the row.
    fireEvent.change(screen.getByLabelText("Filter tree"), { target: { value: "" } });
    expect(screen.queryByRole("note")).not.toBeInTheDocument();
    expect(screen.getByText("specifications")).toBeInTheDocument();
  });
});

describe("rows in view only (LIB-FR-BXMZ)", () => {
  // Far more than the rows of one window, and small enough for a slow runner.
  const COUNT = 1500;

  function bigTree(): TreeNode {
    const files = Array.from({ length: COUNT }, (_, i) =>
      // Zero-padded, so the byte order of the names is the order of `i`.
      file(`big/f${String(i).padStart(4, "0")}.md`, "spec", "inferred"),
    );
    return folder("", true, [folder("big", true, files)]);
  }

  /** The height the spacers and the rendered rows stand for, in px. */
  function renderedHeight(body: HTMLElement): number {
    const rows = body.querySelectorAll(".tree-row").length;
    const spacers = [...body.querySelectorAll<HTMLElement>(":scope > div[aria-hidden='true']")];
    return rows * ROW_HEIGHT + spacers.reduce((sum, el) => sum + parseFloat(el.style.height), 0);
  }

  it("LIB-FR-BXMZ: a folder with 1500 children renders far fewer rows, and the scroll range holds them all", async () => {
    setTree(bigTree(), panelState({ expandedPaths: ["big"] }));
    renderLibrary();
    expect(await screen.findByText("f0000.md")).toBeInTheDocument();
    await waitForTree();

    const body = document.querySelector(".vpanel__body") as HTMLElement;
    const rendered = body.querySelectorAll(".tree-row").length;
    expect(rendered).toBeLessThan(200);
    expect(screen.queryByText(`f${String(COUNT - 1).padStart(4, "0")}.md`)).not.toBeInTheDocument();
    // The folder row and its 1500 children.
    expect(renderedHeight(body)).toBe((COUNT + 1) * ROW_HEIGHT);
  });

  it("LIB-FR-BXMZ: scrolling to the end renders the last rows instead of the first", async () => {
    setTree(bigTree(), panelState({ expandedPaths: ["big"] }));
    renderLibrary();
    await screen.findByText("f0000.md");
    await waitForTree();

    const body = document.querySelector(".vpanel__body") as HTMLElement;
    Object.defineProperty(body, "scrollTop", {
      configurable: true,
      writable: true,
      value: COUNT * ROW_HEIGHT,
    });
    fireEvent.scroll(body);

    expect(await screen.findByText(`f${String(COUNT - 1).padStart(4, "0")}.md`)).toBeInTheDocument();
    expect(screen.queryByText("f0000.md")).not.toBeInTheDocument();
    expect(body.querySelectorAll(".tree-row").length).toBeLessThan(200);
    expect(renderedHeight(body)).toBe((COUNT + 1) * ROW_HEIGHT);
  });
});

describe("repository markers (LIB-FR-VCJO, LIB-FR-VIAZ)", () => {
  function repoFolder(
    path: string,
    hasArtifacts: boolean,
    nestedRepo: TreeNode["nestedRepo"],
    children: TreeNode[],
  ): TreeNode {
    return { ...folder(path, hasArtifacts, children), nestedRepo };
  }

  function repoTree(): TreeNode {
    return folder("", true, [
      repoFolder("mods", true, "submodule", [file("mods/a.md", "spec", "inferred")]),
      repoFolder("vendor", false, "repository", [file("vendor/x.c")]),
      repoFolder("pending", false, "uninitialised", []),
      folder("plain", false, [file("plain/y.c")]),
      file("AGENTS.md", "agent", "inferred"),
    ]);
  }

  it("LIB-FR-VCJO: marks each repository boundary after its name, with its accessible name", async () => {
    setTree(repoTree(), panelState({ artifactTypeFilter: "all_files" }));
    renderLibrary();
    await screen.findByText("pending");
    await waitForTree();

    const sub = within(rowOf("mods")).getByRole("img", { name: "Git submodule" });
    expect(sub).toHaveAttribute("data-state", "submodule");
    const nested = within(rowOf("vendor")).getByRole("img", { name: "Nested repository" });
    expect(nested).toHaveAttribute("data-state", "repository");
    const muted = within(rowOf("pending")).getByRole("img", {
      name: "Git submodule, not initialised",
    });
    expect(muted).toHaveAttribute("data-state", "uninitialised");
    expect(muted).toHaveAttribute("title", "Git submodule, not initialised");
    // The marker follows the name.
    const name = within(rowOf("mods")).getByText("mods");
    expect(name.compareDocumentPosition(sub) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    // A plain folder and a file carry none.
    expect(within(rowOf("plain")).queryByRole("img")).not.toBeInTheDocument();
    expect(within(rowOf("AGENTS.md")).queryByRole("img")).not.toBeInTheDocument();
  });

  it("LIB-FR-VIAZ: a repository folder shows under the lens on the same terms as every other folder", async () => {
    setTree(repoTree(), panelState());
    renderLibrary();
    await screen.findByText("AGENTS.md");
    await waitForTree();

    // All artifacts: only the folders that hold artifacts, marker or not.
    expect(screen.getByText("mods")).toBeInTheDocument();
    expect(screen.queryByText("vendor")).not.toBeInTheDocument();
    expect(screen.queryByText("pending")).not.toBeInTheDocument();
    expect(screen.queryByText("plain")).not.toBeInTheDocument();

    // All files: every folder, marker or not.
    fireEvent.click(
      document.querySelector('[role="radio"][data-value="files"]') as HTMLElement,
    );
    expect(await screen.findByText("vendor")).toBeInTheDocument();
    expect(screen.getByText("pending")).toBeInTheDocument();
    expect(screen.getByText("plain")).toBeInTheDocument();
  });
});

describe("reveal through the folder path (LIB-FR-18)", () => {
  /** A tab-follow: not optimistic, so a missing target changes nothing. */
  function FollowHost({ id, nonce }: { id: string | null; nonce: number }) {
    const panel = useLibraryPanelState();
    return (
      <Library
        panel={panel}
        onOpenArtifact={vi.fn()}
        onShowNotes={vi.fn()}
        onNewArtifact={vi.fn()}
        reveal={id ? { panel: "library", id, nonce, optimistic: false, focus: false } : null}
      />
    );
  }

  function deepTree(): TreeNode {
    return folder("", true, [
      folder("docs", true, [
        folder("docs/deep", true, [file("docs/deep/x.md", "spec", "inferred")]),
      ]),
      file("AGENTS.md", "agent", "inferred"),
    ]);
  }

  it("LIB-FR-18: reveals a node in a folder the panel has not loaded, through list_tree_path", async () => {
    setTree(deepTree(), panelState());
    const { rerender } = render(<LibraryHost revealId={null} />);
    await screen.findByText("docs");
    await waitForTree();
    expect(reads("docs")).toBe(0);

    rerender(<LibraryHost revealId="docs/deep/x.md" />);

    expect(await screen.findByText("x.md")).toBeInTheDocument();
    await waitFor(() => expect(rowOf("x.md")).toHaveAttribute("data-selected", "true"));
    expect(invokeMock).toHaveBeenCalledWith("list_tree_path", { path: "docs/deep/x.md" });
    expect(countCalls(invokeMock, "list_tree_path")).toBe(1);
    expect(screen.getByText("deep")).toBeInTheDocument();
  });

  it("LIB-FR-18: a tab-follow to a node in an unloaded folder loads its path, then selects it", async () => {
    setTree(deepTree(), panelState());
    const { rerender } = render(<FollowHost id={null} nonce={0} />);
    await screen.findByText("docs");
    await waitForTree();

    rerender(<FollowHost id="docs/deep/x.md" nonce={7001} />);

    expect(await screen.findByText("x.md")).toBeInTheDocument();
    await waitFor(() => expect(rowOf("x.md")).toHaveAttribute("data-selected", "true"));
    expect(invokeMock).toHaveBeenCalledWith("list_tree_path", { path: "docs/deep/x.md" });
    // A follow waits for the path read, so the path read alone gave the
    // folders on the way.
    expect(reads("docs")).toBe(0);
    expect(reads("docs/deep")).toBe(0);
  });

  it("LIB-FR-18, SNV-FR-67: a tab-follow to a node the index does not hold changes nothing", async () => {
    setTree(baseTree(), panelState());
    const { rerender } = render(<FollowHost id={null} nonce={0} />);
    await screen.findByText("AGENTS.md");
    await waitForTree();
    fireEvent.click(screen.getByText("AGENTS.md"));
    expect(rowOf("AGENTS.md")).toHaveAttribute("data-selected", "true");

    rerender(<FollowHost id="specifications/gone.md" nonce={7002} />);

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("list_tree_path", {
        path: "specifications/gone.md",
      }),
    );
    // The answer landed and spent the request.
    await waitFor(() => expect(revealAlreadyConsumed(7002)).toBe(true));

    // The ancestor was not expanded (it would show its loading row or its
    // child), and the selection did not move.
    expect(screen.queryByRole("status", { name: "Loading folder" })).not.toBeInTheDocument();
    expect(screen.queryByText("LIB-library.md")).not.toBeInTheDocument();
    expect(rowOf("AGENTS.md")).toHaveAttribute("data-selected", "true");
  });
});

describe("the lens's types (LIB-FR-19)", () => {
  it("LIB-FR-19: renders a type button for each type list_project_artifacts reports, and reads it again on a tree change", async () => {
    // The list answer is set apart from the tree on purpose: the buttons
    // follow it, not the folders the panel has loaded.
    const scenarios = { ...wireNode(folder("drafts", true, [])), artifactType: "scenario" as const };
    let artifacts: TreeNode[] = [
      wireNode(file("deep/inside/release.flow", "flow", "inferred")),
      scenarios,
    ];
    mockInvoke(
      { tree: () => baseTree(), list_project_artifacts: () => artifacts },
      panelState(),
    );
    renderLibrary();
    await screen.findByText("AGENTS.md");
    await waitForTree();

    await waitFor(() => expect(selectorValues("Filter by type")).toContain("flow"));
    expect(selectorValues("Filter by type")).toContain("scenario");
    // The tree holds a skill and a spec, but the list does not report them.
    expect(selectorValues("Filter by type")).not.toContain("skill");
    expect(selectorValues("Filter by type")).not.toContain("spec");
    expect(selectorValues("Filter by type")).not.toContain("prompt");
    // No collapsed folder was read to find them.
    expect(reads(".claude")).toBe(0);
    expect(reads("specifications")).toBe(0);

    artifacts = [...artifacts, wireNode(file("ask.md", "prompt", "inferred"))];
    fireTreeChanged([""]);

    await waitFor(() => expect(selectorValues("Filter by type")).toContain("prompt"));
    expect(countCalls(invokeMock, "list_project_artifacts")).toBe(2);
  });
});

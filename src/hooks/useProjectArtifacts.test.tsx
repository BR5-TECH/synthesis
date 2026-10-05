import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";

import {
  flattenArtifacts,
  referenceableArtifacts,
  useProjectArtifacts,
} from "./useProjectArtifacts";
import type { TreeNode } from "../types";

const invokeMock = vi.fn();
const unlistenMock = vi.fn();
/** Every `"project tree changed"` handler registered, so a test can fire one. */
let treeHandlers: Array<(ev: { payload: unknown }) => void> = [];

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (_name: string, cb: (ev: { payload: unknown }) => void) => {
    treeHandlers.push(cb);
    return unlistenMock;
  }),
}));

const folder = (path: string, children: TreeNode[]): TreeNode => ({
  id: path,
  name: path.split("/").pop() ?? path,
  path,
  nodeKind: "folder",
  hasArtifacts: true,
  children,
});

const file = (
  path: string,
  artifactType?: TreeNode["artifactType"],
  displayName?: string,
): TreeNode => ({
  id: path,
  name: path.split("/").pop() ?? path,
  path,
  nodeKind: "file",
  ...(artifactType ? { artifactType, typeSource: "inferred" as const } : {}),
  ...(displayName ? { displayName } : {}),
});

const root = (children: TreeNode[]): TreeNode => ({
  id: "",
  name: "proj",
  path: "",
  nodeKind: "folder",
  hasArtifacts: true,
  children,
});

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
  treeHandlers = [];
});

afterEach(cleanup);

/** Renders the hook's output as text, so a test can read it out of the DOM. */
function Probe() {
  const { artifacts, referenceable, loaded } = useProjectArtifacts();
  return (
    <div>
      <span data-testid="loaded">{String(loaded)}</span>
      <span data-testid="ids">{artifacts.map((a) => a.id).join(",")}</span>
      <span data-testid="offered">
        {referenceable.map((a) => `${a.id}=${a.displayName}`).join(",")}
      </span>
    </div>
  );
}

describe("flattenArtifacts", () => {
  it("returns every classified file at any depth, in tree order", () => {
    const tree = root([
      folder("prompts", [
        file("prompts/gather.md", "prompt"),
        folder("prompts/nested", [file("prompts/nested/deep.md", "spec")]),
      ]),
      file("review.flow", "flow"),
    ]);

    expect(flattenArtifacts(tree)).toEqual([
      {
        id: "prompts/gather.md",
        name: "gather.md",
        path: "prompts/gather.md",
        artifactType: "prompt",
        displayName: "gather.md",
      },
      {
        id: "prompts/nested/deep.md",
        name: "deep.md",
        path: "prompts/nested/deep.md",
        artifactType: "spec",
        displayName: "deep.md",
      },
      {
        id: "review.flow",
        name: "review.flow",
        path: "review.flow",
        artifactType: "flow",
        displayName: "review.flow",
      },
    ]);
  });

  it("skips unclassified files and folders alike", () => {
    // A node's reference names an artifact, so a plain file is not something it
    // can point at — and neither is a folder, typed or not.
    const typedFolder: TreeNode = {
      ...folder("flows", [file("flows/a.flow", "flow")]),
      artifactType: "flow",
      typeSource: "assigned",
    };
    const tree = root([typedFolder, file("notes.txt"), file("README.md")]);

    expect(flattenArtifacts(tree).map((a) => a.id)).toEqual([
      "flows/a.flow",
    ]);
  });

  it("handles a folder with no children and an empty tree", () => {
    const childless: TreeNode = {
      id: "empty",
      name: "empty",
      path: "empty",
      nodeKind: "folder",
      hasArtifacts: false,
    };
    expect(flattenArtifacts(root([childless]))).toEqual([]);
    expect(
      flattenArtifacts({
        id: "",
        name: "p",
        path: "",
        nodeKind: "folder",
      }),
    ).toEqual([]);
  });
});

describe("referenceableArtifacts (FLO-FR-10)", () => {
  const catalogue = () =>
    flattenArtifacts(
      root([
        folder(".claude/skills/code-review", [
          file(".claude/skills/code-review/SKILL.md", "skill"),
          // Supporting files live beside the skill and classify as `skill` too,
          // because they sit under a skills directory. None of them is a skill.
          file(".claude/skills/code-review/references/rules.md", "skill"),
        ]),
        file("prompts/gather.md", "prompt"),
        file("AGENTS.md", "instructions"),
        file("specifications/ui/FLO-flow.md", "spec"),
        file("workflows/review.flow", "flow"),
        file("agents/reviewer.md", "agent"),
        file("scratch/notes.md", "scratchpad"),
      ]),
    );

  it("offers a skill only through its own SKILL.md", () => {
    expect(referenceableArtifacts(catalogue()).map((a) => a.id)).toContain(
      ".claude/skills/code-review/SKILL.md",
    );
    expect(referenceableArtifacts(catalogue()).map((a) => a.id)).not.toContain(
      ".claude/skills/code-review/references/rules.md",
    );
  });

  // A Flow is prompts and the instructions around them; a spec, another Flow,
  // an agent definition or a scratchpad is not a step in one, and listing them
  // buries the artifacts that are behind the ones that never will be.
  it("offers skills, prompts and instructions and nothing else", () => {
    expect(referenceableArtifacts(catalogue()).map((a) => a.id)).toEqual([
      ".claude/skills/code-review/SKILL.md",
      "prompts/gather.md",
      "AGENTS.md",
    ]);
  });
});

describe("useProjectArtifacts (FLO-FR-10 / FLO-FR-11)", () => {
  it("reads the project tree on mount and reports itself loaded", async () => {
    invokeMock.mockResolvedValue(
      root([folder("prompts", [file("prompts/gather.md", "prompt")])]),
    );

    render(<Probe />);

    await waitFor(() =>
      expect(screen.getByTestId("loaded")).toHaveTextContent("true"),
    );
    expect(screen.getByTestId("ids")).toHaveTextContent("prompts/gather.md");
    expect(invokeMock).toHaveBeenCalledWith("load_project_tree");
  });

  // Load-bearing for FLO-FR-11: `loaded` is what tells an unresolved reference
  // apart from one the canvas simply has not been able to check yet. A tree that
  // could not be read must leave it false, or every reference in the Flow would
  // render as a warning about an artifact that probably still exists.
  it("stays unloaded when the tree cannot be read", async () => {
    invokeMock.mockRejectedValue(new Error("no project open"));

    render(<Probe />);

    await waitFor(() => expect(invokeMock).toHaveBeenCalled());
    expect(screen.getByTestId("loaded")).toHaveTextContent("false");
    expect(screen.getByTestId("ids")).toHaveTextContent("");
  });

  // FLO-FR-11: an artifact created or deleted while a Flow tab is open changes
  // what resolves, so the list follows the structural-change channel.
  it("re-reads the tree when the project tree changes", async () => {
    invokeMock.mockResolvedValue(root([file("a.md", "prompt")]));
    render(<Probe />);
    await waitFor(() =>
      expect(screen.getByTestId("ids")).toHaveTextContent("a.md"),
    );

    invokeMock.mockResolvedValue(
      root([file("a.md", "prompt"), file("b.flow", "flow")]),
    );
    await act(async () => {
      treeHandlers.forEach((h) => h({ payload: { reason: "changed" } }));
    });

    await waitFor(() =>
      expect(screen.getByTestId("ids")).toHaveTextContent("a.md,b.flow"),
    );
  });

  // FLO-FR-11: every skill's file is `SKILL.md`, so the filename identifies
  // nothing. The name comes off the node the scan produced (ASC-FR-19); the
  // canvas never opens an artifact to find out what it is called.
  it("takes a skill's display name from the tree, and falls back to the filename", async () => {
    invokeMock.mockResolvedValue(
      root([
        folder(".claude/skills/code-review", [
          file(".claude/skills/code-review/SKILL.md", "skill", "Code review"),
        ]),
        // A skill whose file declares no name: the scan sends no display name,
        // and the picker falls back to the filename rather than to nothing.
        folder(".claude/skills/nameless", [
          file(".claude/skills/nameless/SKILL.md", "skill"),
        ]),
        file("prompts/gather.md", "prompt"),
      ]),
    );

    render(<Probe />);

    await waitFor(() =>
      expect(screen.getByTestId("offered")).toHaveTextContent(
        ".claude/skills/code-review/SKILL.md=Code review",
      ),
    );
    expect(screen.getByTestId("offered")).toHaveTextContent(
      ".claude/skills/nameless/SKILL.md=SKILL.md",
    );
    expect(screen.getByTestId("offered")).toHaveTextContent(
      "prompts/gather.md=gather.md",
    );
    // Nothing was read to produce any of that.
    expect(
      invokeMock.mock.calls.filter(
        (c) => c[0] === "load_artifact_contents_by_id",
      ),
    ).toEqual([]);
  });

  // Two structural changes in quick succession: the older read must not land
  // last and leave the picker offering a tree that no longer exists.
  it("ignores a tree read overtaken by a newer one", async () => {
    const first = root([file("old.md", "prompt")]);
    const second = root([file("new.md", "prompt")]);
    let releaseFirst: (tree: TreeNode) => void = () => {};
    invokeMock.mockImplementationOnce(
      () => new Promise<TreeNode>((resolve) => (releaseFirst = resolve)),
    );
    invokeMock.mockResolvedValue(second);

    render(<Probe />);
    await waitFor(() => expect(treeHandlers).toHaveLength(1));

    // The second read starts and completes while the first is still in flight.
    await act(async () => {
      treeHandlers.forEach((h) => h({ payload: { reason: "changed" } }));
    });
    await waitFor(() =>
      expect(screen.getByTestId("ids")).toHaveTextContent("new.md"),
    );

    await act(async () => {
      releaseFirst(first);
      await Promise.resolve();
    });

    expect(screen.getByTestId("ids")).toHaveTextContent("new.md");
    expect(screen.getByTestId("ids")).not.toHaveTextContent("old.md");
  });

  it("detaches its subscription on unmount", async () => {
    invokeMock.mockResolvedValue(root([]));
    const view = render(<Probe />);
    await waitFor(() => expect(treeHandlers).toHaveLength(1));

    view.unmount();

    await waitFor(() => expect(unlistenMock).toHaveBeenCalled());
  });
});

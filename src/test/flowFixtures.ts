import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type { FlowSessionStore } from "../state/flowSessions";
import type { TreeNode } from "../types";

const file = (
  path: string,
  artifactType?: TreeNode["artifactType"],
): TreeNode => ({
  id: path,
  name: path.split("/").pop() ?? path,
  path,
  nodeKind: "file",
  ...(artifactType ? { artifactType, typeSource: "inferred" as const } : {}),
});

const folder = (path: string, children: TreeNode[]): TreeNode => ({
  id: path,
  name: path.split("/").pop() ?? path,
  path,
  nodeKind: "folder",
  hasArtifacts: true,
  children,
});

export const SKILL_ID = ".claude/skills/code-review/SKILL.md";

/**
 * A project holding one of everything the picker has to decide about: a prompt
 * and a skill it offers, a supporting file inside the skill's folder and a spec
 * it does not (FLO-FR-10), and an unclassified file that is not an artifact at
 * all.
 */
export const TREE: TreeNode = {
  id: "",
  name: "proj",
  path: "",
  nodeKind: "folder",
  hasArtifacts: true,
  children: [
    folder("prompts", [file("prompts/gather-repo.md", "prompt")]),
    folder(".claude/skills/code-review", [
      // ASC-FR-19: the scan carries the name the skill declares for itself.
      { ...file(SKILL_ID, "skill"), displayName: "Code review" },
      file(".claude/skills/code-review/references/rules.md", "skill"),
    ]),
    folder("specifications", [file("specifications/FLO-flow.md", "spec")]),
    file("notes.txt"),
  ],
};

export const ID = "workflows/review.flow";

/** A two-node Flow with one edge, as it sits on disk. */
export const TWO_NODES = JSON.stringify({
  version: 1,
  nodes: [
    { id: "n1", name: "Collect context", position: { x: 0, y: 0 } },
    { id: "n2", name: "Draft review", position: { x: 300, y: 0 } },
  ],
  edges: [{ id: "e1", from: "n1", to: "n2", label: "ok" }],
});

export const nodeEl = (id: string) => screen.getByTestId(`flow-node-${id}`);
export const loopEl = (id: string) => screen.getByTestId(`flow-loop-${id}`);
export const canvas = () => document.querySelector(".flow") as HTMLElement;
export const doc = (flows: FlowSessionStore) => flows.get(ID)!.doc!;

/**
 * FLO-FR-07: add through the affordance's menu, which is the only way to add
 * anything — the `+` expands into a short list of what can be added rather than
 * adding a node outright.
 */
export async function addFromMenu(entry: "Node" | "Loop") {
  await userEvent.click(screen.getByLabelText("Add to flow"));
  await userEvent.click(
    within(screen.getByTestId("flow-add-menu")).getByRole("menuitem", {
      name: entry,
    }),
  );
}

/**
 * Select a node the way a click on its body does — press AND release, because a
 * press on a node arms a drag and a test that never releases would leave the
 * canvas mid-gesture.
 */
export function selectNode(id: string) {
  fireEvent.pointerDown(nodeEl(id));
  fireEvent.pointerUp(nodeEl(id));
}

/** Drag a connection from `from`'s port and release it over `to`. */
export function connectNodes(from: string, to: string) {
  const port = within(nodeEl(from)).getByLabelText(/^Connect from /);
  fireEvent.pointerDown(port, { pointerId: 1 });
  fireEvent.pointerUp(nodeEl(to), { pointerId: 1 });
}

/**
 * Drag a node by (dx, dy) through the full pointer sequence.
 *
 * `buttons: 1` on every move is what a browser reports while a button is held;
 * the canvas reads it to tell a real drag from pointer motion after a release
 * it never saw.
 */
export function dragNode(id: string, dx: number, dy: number, steps = 1) {
  fireEvent.pointerDown(nodeEl(id), {
    pointerId: 1,
    buttons: 1,
    clientX: 500,
    clientY: 500,
  });
  for (let i = 1; i <= steps; i += 1) {
    fireEvent.pointerMove(canvas(), {
      pointerId: 1,
      buttons: 1,
      clientX: 500 + (dx * i) / steps,
      clientY: 500 + (dy * i) / steps,
    });
  }
  fireEvent.pointerUp(canvas(), { pointerId: 1 });
}

/** Pan the canvas by (dx, dy) from its empty background. */
export function panCanvas(dx: number, dy: number) {
  fireEvent.pointerDown(canvas(), {
    pointerId: 1,
    buttons: 1,
    clientX: 100,
    clientY: 100,
  });
  fireEvent.pointerMove(canvas(), {
    pointerId: 1,
    buttons: 1,
    clientX: 100 + dx,
    clientY: 100 + dy,
  });
  fireEvent.pointerUp(canvas(), { pointerId: 1 });
}

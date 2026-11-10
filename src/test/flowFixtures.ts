/**
 * The project tree and the pointer helpers of the Flow canvas tests (FLO).
 *
 * jsdom lays nothing out: the canvas element reports a zero rectangle, so a
 * client point is the canvas point moved by the default view offset, and a
 * card is as tall as the card floor (`CARD_H`).
 */
import { fireEvent, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { DEFAULT_VIEW } from "../components/Flow/geometry";
import type { FlowItemKind, FlowPosition } from "../state/flowDocument";
import type { TreeNode } from "../types";

const file = (path: string, artifactType?: TreeNode["artifactType"]): TreeNode => ({
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
export const HLD_ID = "docs/payments-hld.md";

/**
 * A project with one of each thing the pickers decide about: a prompt, a
 * skill and a file in the skill's folder, a spec, a document, another Flow,
 * an unclassified file, and folders (FLO-FR-10, FLO-FR-ZEJU).
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
      { ...file(SKILL_ID, "skill"), displayName: "Code review" },
      file(".claude/skills/code-review/references/rules.md", "skill"),
    ]),
    folder("specifications", [file("specifications/FLO-flow.md", "spec")]),
    folder("docs", [file(HLD_ID, "doc")]),
    folder("flows", [file("flows/other.flow", "flow")]),
    folder("src", [file("src/main.ts")]),
    file("notes.txt"),
  ],
};

export const ID = "plans/payments.flow";

/** A client point for a canvas point, under the default view. */
export const client = (p: FlowPosition) => ({
  clientX: p.x + DEFAULT_VIEW.x,
  clientY: p.y + DEFAULT_VIEW.y,
});

export const itemEl = (id: string) => screen.getByTestId(`flow-item-${id}`);
export const queryItem = (id: string) => screen.queryByTestId(`flow-item-${id}`);
export const canvas = () => document.querySelector(".flow") as HTMLElement;

/** The element a press selects an item on: a card, or a container's header. */
export const grip = (id: string) => {
  const root = itemEl(id);
  return (root.querySelector(`[data-item-id="${id}"]`) as HTMLElement | null) ?? root;
};

/** Select an item as a click on it does: a press and a release. */
export function selectItem(id: string) {
  fireEvent.pointerDown(grip(id), { pointerId: 1, buttons: 1, clientX: 0, clientY: 0 });
  fireEvent.pointerUp(grip(id), { pointerId: 1 });
}

/** Draw a connection from `from`'s port and release it on `to`. */
export function connectItems(from: string, to: string) {
  const port = within(itemEl(from)).getByLabelText(/^Connect from /);
  fireEvent.pointerDown(port, { pointerId: 1, buttons: 1, ...client({ x: 0, y: 0 }) });
  fireEvent.pointerUp(itemEl(to), { pointerId: 1 });
}

/**
 * Drag an item from `start` to `end` (canvas points). The press is on the
 * item's grip, the moves on the canvas, as a browser reports them while the
 * button is held.
 */
export function dragItem(id: string, start: FlowPosition, end: FlowPosition, steps = 2) {
  fireEvent.pointerDown(grip(id), { pointerId: 1, buttons: 1, ...client(start) });
  for (let i = 1; i <= steps; i += 1) {
    fireEvent.pointerMove(canvas(), {
      pointerId: 1,
      buttons: 1,
      ...client({
        x: start.x + ((end.x - start.x) * i) / steps,
        y: start.y + ((end.y - start.y) * i) / steps,
      }),
    });
  }
  fireEvent.pointerUp(canvas(), { pointerId: 1 });
}

/** FLO-FR-07: add an item of `kind` through the add menu. */
export async function addFromMenu(kind: FlowItemKind) {
  await userEvent.click(screen.getByRole("button", { name: "Add to plan" }));
  const label = kind.charAt(0).toUpperCase() + kind.slice(1);
  await userEvent.click(
    within(screen.getByTestId("flow-add-menu")).getByRole("menuitem", { name: label }),
  );
}

/** Pan the canvas from its empty background. */
export function panCanvas(dx: number, dy: number) {
  fireEvent.pointerDown(canvas(), { pointerId: 1, buttons: 1, clientX: 900, clientY: 900 });
  fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, clientX: 900 + dx, clientY: 900 + dy });
  fireEvent.pointerUp(canvas(), { pointerId: 1 });
}

/** The selection panel of the selected item. */
export const panel = () => screen.getByTestId("flow-selection-panel");

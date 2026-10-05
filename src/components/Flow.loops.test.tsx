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
import userEvent from "@testing-library/user-event";

import { FlowCanvas } from "./Flow";
import { FlowSessionStore } from "../state/flowSessions";
import { addLoop, addNode, minLoopSize } from "../state/flowDocument";
import {
  addFromMenu,
  canvas,
  connectNodes,
  doc,
  ID,
  loopEl,
  nodeEl,
  TREE,
} from "../test/flowFixtures";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

let body = "";
let writes: string[] = [];
/**
 * FGV-FR-02: what `"validate flow document"` answers. Valid by default — the
 * rule set is the Rust suite's business (FGV-FR-05..14); what these tests care
 * about is that the canvas asks before it renders (FLO-FR-46) and what it shows
 * when the answer is `No` (FLO-FR-05).
 */
let validation: { valid: boolean; violations: Array<Record<string, string>> } = {
  valid: true,
  violations: [],
};

beforeEach(() => {
  writes = [];
  validation = { valid: true, violations: [] };
  invokeMock.mockReset();
  invokeMock.mockImplementation(
    async (cmd: string, args: Record<string, string>) => {
      switch (cmd) {
        case "load_project_tree":
          return TREE;
        case "validate_flow_document":
          return validation;
        case "load_artifact_contents_by_id":
          return { body, checksum: "ck1" };
        case "save_artifact_contents":
          writes.push(args.body);
          return { checksum: "ck2" };
        case "open_artifact_by_id":
          return { key: args.id, kind: "markdown" };
        default:
          throw new Error(`unexpected invoke ${cmd}`);
      }
    },
  );
});

afterEach(cleanup);

/** Load `content` as the Flow's body and render a canvas over it. */
async function renderCanvas(content: string) {
  body = content;
  const onOpenArtifact = vi.fn();
  const flows = new FlowSessionStore();
  await act(async () => {
    flows.openTab(ID);
    await flows.get(ID)?.pendingLoad;
  });
  const view = render(
    <FlowCanvas flowId={ID} flows={flows} onOpenArtifact={onOpenArtifact} />,
  );
  // The artifact list is fetched on mount; wait for it so reference resolution
  // is settled rather than in its `pending` state.
  await waitFor(() =>
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "load_project_tree"),
    ).toBe(true),
  );
  // The call having been ISSUED is not the same as its result having been
  // applied. Until the tree lands, `treeLoaded` is false and every reference
  // renders in the `pending` shape — a bare id with no click-through, so a test
  // that clicks "Open <name>" finds nothing (FLO-FR-11). Flush the resolution
  // and the state update it causes, which is what "settled" above claims.
  await act(async () => {});
  return { flows, view, onOpenArtifact };
}

describe("loops on the canvas (FLO-FR-42, FLO-FR-40 / FLO-FR-39 / FLO-FR-45)", () => {
  /** A loop holding n2 and n3, with n1 outside it. */
  const WITH_LOOP = JSON.stringify({
    version: 1,
    name: "F",
    loops: [
      {
        id: "l1",
        name: "Review cycle",
        position: { x: 400, y: 0 },
        size: { width: 420, height: 260 },
      },
    ],
    nodes: [
      { id: "n1", name: "Gather", position: { x: 0, y: 0 } },
      { id: "n2", name: "Draft", parentId: "l1", position: { x: 20, y: 70 } },
      { id: "n3", name: "Critique", parentId: "l1", position: { x: 200, y: 70 } },
    ],
    edges: [{ id: "e1", from: "n2", to: "n3" }],
  });

  it("counts loops in the readout and draws the container", async () => {
    await renderCanvas(WITH_LOOP);
    expect(loopEl("l1")).toBeInTheDocument();
    expect(screen.getByTestId("flow-status")).toHaveTextContent(
      "3 nodes · 1 loop · 1 edge",
    );
  });

  // FLO-FR-42, FLO-FR-40: siblings connect; across the boundary is refused immediately and
  // with no backend call.
  it("refuses a connection that crosses a loop boundary", async () => {
    const { flows } = await renderCanvas(WITH_LOOP);
    const before = invokeMock.mock.calls.length;

    connectNodes("n1", "n2");

    expect(doc(flows).edges).toHaveLength(1);
    expect(screen.getByRole("status")).toHaveTextContent(
      /Only elements in the same loop/,
    );
    expect(invokeMock.mock.calls.length).toBe(before);
    expect(flows.get(ID)?.dirty).toBe(false);
  });

  // FLO-FR-39, FLO-FR-40: the graph reaches a loop through the loop.
  it("connects an outer node to the loop itself", async () => {
    const { flows } = await renderCanvas(WITH_LOOP);
    const port = within(nodeEl("n1")).getByLabelText(/^Connect from /);
    fireEvent.pointerDown(port, { pointerId: 1 });
    fireEvent.pointerUp(loopEl("l1"), { pointerId: 1 });

    expect(
      doc(flows).edges.some((e) => e.from === "n1" && e.to === "l1"),
    ).toBe(true);
  });

  // FLO-FR-45: the dialog, and what each answer does.
  it("asks what to do with a loop's contents before removing it", async () => {
    const { flows } = await renderCanvas(WITH_LOOP);
    fireEvent.pointerDown(loopEl("l1"));
    fireEvent.pointerUp(loopEl("l1"));

    await userEvent.click(screen.getByLabelText("Remove loop Review cycle"));
    const dialog = screen.getByTestId("flow-remove-loop-confirm");

    // Cancelling changes nothing at all.
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(doc(flows).loops).toHaveLength(1);
    expect(flows.get(ID)?.dirty).toBe(false);

    fireEvent.pointerDown(loopEl("l1"));
    fireEvent.pointerUp(loopEl("l1"));
    await userEvent.click(screen.getByLabelText("Remove loop Review cycle"));
    await userEvent.click(
      within(screen.getByTestId("flow-remove-loop-confirm")).getByRole("button", {
        name: "Release contents",
      }),
    );

    // The members survive as top-level elements, still joined.
    expect(doc(flows).loops).toHaveLength(0);
    expect(doc(flows).nodes.map((n) => n.id).sort()).toEqual(["n1", "n2", "n3"]);
    expect(doc(flows).edges).toHaveLength(1);
  });

  // FLO-FR-45: a loop holding nothing goes outright, with no question asked.
  it("removes an empty loop with no dialog", async () => {
    const { flows } = await renderCanvas("");
    await addFromMenu("Loop");
    const id = doc(flows).loops[0].id;
    fireEvent.pointerDown(loopEl(id));
    fireEvent.pointerUp(loopEl(id));

    await userEvent.click(screen.getByLabelText(/^Remove loop /));

    expect(screen.queryByTestId("flow-remove-loop-confirm")).not.toBeInTheDocument();
    expect(doc(flows).loops).toHaveLength(0);
  });
});

describe("dragging between containers (FLO-FR-42, FLO-FR-40 / FLO-FR-43 / FLO-FR-44, FLO-FR-21)", () => {
  /**
   * A loop with room to spare, holding n2 and n3 joined both ways, with n1 on
   * open canvas well clear of it.
   */
  const DRAGGABLE = JSON.stringify({
    version: 1,
    name: "F",
    loops: [
      {
        id: "l1",
        name: "Review cycle",
        position: { x: 400, y: 0 },
        size: { width: 600, height: 400 },
      },
    ],
    nodes: [
      { id: "n1", name: "Gather", position: { x: 0, y: 0 } },
      { id: "n2", name: "Draft", parentId: "l1", position: { x: 20, y: 80 } },
      { id: "n3", name: "Critique", parentId: "l1", position: { x: 300, y: 80 } },
    ],
    edges: [
      { id: "e1", from: "n2", to: "n3" },
      { id: "e2", from: "n3", to: "n2" },
    ],
  });

  /**
   * Drag `id` by (dx, dy) in graph units. jsdom reports a zero-sized canvas
   * rect, so a client delta is a graph delta at scale 1 — which is what the
   * canvas is at on mount (FLO-FR-22).
   */
  function dragElement(el: HTMLElement, dx: number, dy: number) {
    fireEvent.pointerDown(el, { pointerId: 1, buttons: 1, clientX: 0, clientY: 0 });
    fireEvent.pointerMove(canvas(), {
      pointerId: 1,
      buttons: 1,
      clientX: dx,
      clientY: dy,
    });
    fireEvent.pointerUp(canvas(), { pointerId: 1 });
  }

  // FLO-FR-42: membership is decided by where the element is dropped — the
  // innermost loop whose bounds contain it.
  it("makes a node dropped inside a loop a member of it", async () => {
    const { flows } = await renderCanvas(DRAGGABLE);
    expect(doc(flows).nodes.find((n) => n.id === "n1")?.parentId).toBeUndefined();

    // n1 sits at (0,0); its centre lands well inside the loop at (400..1000, 0..400).
    dragElement(nodeEl("n1"), 560, 180);

    const n1 = doc(flows).nodes.find((n) => n.id === "n1")!;
    expect(n1.parentId).toBe("l1");
    // FLO-FR-21: the position is rebased into the container's frame, so the
    // element does not jump when it joins.
    expect(n1.position).toEqual({ x: 160, y: 180 });
    expect(screen.getByTestId("flow-dirty")).toBeInTheDocument();
    // No dialog: the move severed nothing.
    expect(screen.queryByTestId("flow-move-confirm")).not.toBeInTheDocument();
  });

  // FLO-FR-42: a drop whose centre falls outside the bounds joins nothing.
  it("leaves a node top-level when its centre lands outside the loop", async () => {
    const { flows } = await renderCanvas(DRAGGABLE);
    // Far short of the loop's leading edge at x = 400.
    dragElement(nodeEl("n1"), 100, 20);
    expect(doc(flows).nodes.find((n) => n.id === "n1")?.parentId).toBeUndefined();
  });

  // FLO-FR-43: the canvas asks before a move that costs connections.
  it("asks before a move severs connections, and cancelling changes nothing", async () => {
    const { flows } = await renderCanvas(DRAGGABLE);

    dragElement(nodeEl("n2"), 0, 900); // out of the loop, onto open canvas

    const dialog = screen.getByTestId("flow-move-confirm");
    expect(dialog).toHaveTextContent("removes 2 connections");
    // Nothing has happened yet.
    expect(doc(flows).nodes.find((n) => n.id === "n2")?.parentId).toBe("l1");
    expect(doc(flows).edges).toHaveLength(2);
    expect(flows.get(ID)?.dirty).toBe(false);

    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    expect(screen.queryByTestId("flow-move-confirm")).not.toBeInTheDocument();
    expect(doc(flows).nodes.find((n) => n.id === "n2")?.parentId).toBe("l1");
    expect(doc(flows).edges).toHaveLength(2);
    expect(flows.get(ID)?.dirty).toBe(false);
  });

  it("moves the element and removes exactly those edges when confirmed", async () => {
    const { flows } = await renderCanvas(DRAGGABLE);

    dragElement(nodeEl("n2"), 0, 900);
    await userEvent.click(
      within(screen.getByTestId("flow-move-confirm")).getByRole("button", {
        name: "Move and remove",
      }),
    );

    expect(doc(flows).nodes.find((n) => n.id === "n2")?.parentId).toBeUndefined();
    expect(doc(flows).edges).toEqual([]);
    // n3 keeps its place inside the loop.
    expect(doc(flows).nodes.find((n) => n.id === "n3")?.parentId).toBe("l1");
    expect(screen.getByTestId("flow-dirty")).toBeInTheDocument();
  });

  // FLO-FR-43: an element carrying no such edge moves with no question asked.
  it("moves an unconnected member out with no dialog", async () => {
    const { flows } = await renderCanvas(DRAGGABLE);
    // Drop the edges first, so n2 carries none.
    await act(async () => {
      flows.applyEdit(ID, (d) => ({ ...d, edges: [] }));
    });

    dragElement(nodeEl("n2"), 0, 900);

    expect(screen.queryByTestId("flow-move-confirm")).not.toBeInTheDocument();
    expect(doc(flows).nodes.find((n) => n.id === "n2")?.parentId).toBeUndefined();
  });

  // FLO-FR-44, FLO-FR-21: a loop's members travel with it, and only the loop's own
  // position changes in the document.
  it("carries a loop's members when the loop is dragged", async () => {
    const { flows } = await renderCanvas(DRAGGABLE);
    const before = doc(flows).nodes.find((n) => n.id === "n2")!.position;

    // The loop drags from its header alone.
    const head = loopEl("l1").querySelector(".flow-loop__head") as HTMLElement;
    fireEvent.pointerDown(head, { pointerId: 1, buttons: 1, clientX: 0, clientY: 0 });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, clientX: 60, clientY: 40 });
    fireEvent.pointerUp(canvas(), { pointerId: 1 });

    expect(doc(flows).loops[0].position).toEqual({ x: 460, y: 40 });
    expect(doc(flows).nodes.find((n) => n.id === "n2")?.position).toEqual(before);
    expect(doc(flows).nodes.find((n) => n.id === "n2")?.parentId).toBe("l1");
  });

  // FLO-FR-41, FLO-FR-40, FLO-FR-39: the sibling rule reads the same at every level, and a loop is
  // never placed inside itself.
  it("applies the rules at each level of a nest and refuses a self-nesting drop", async () => {
    const { flows } = await renderCanvas(DRAGGABLE);
    await act(async () => {
      flows.applyEdit(ID, (d) => {
        // An inner loop inside l1, holding one node.
        let next = addLoop(d, { x: 20, y: 200 }, "l1");
        next = addNode(next, { x: 10, y: 40 }, next.loops[1].id);
        return next;
      });
    });
    const inner = doc(flows).loops[1].id;
    const innerNode = doc(flows).nodes[doc(flows).nodes.length - 1].id;

    // A member of the inner loop cannot reach a member of the outer one.
    connectNodes(innerNode, "n2");
    expect(screen.getByRole("status")).toHaveTextContent(
      /Only elements in the same loop/,
    );
    // …but the inner loop itself is a sibling of n2 and can be reached.
    const port = within(nodeEl("n2")).getByLabelText(/^Connect from /);
    fireEvent.pointerDown(port, { pointerId: 1 });
    fireEvent.pointerUp(loopEl(inner), { pointerId: 1 });
    expect(
      doc(flows).edges.some((e) => e.from === "n2" && e.to === inner),
    ).toBe(true);

    // FLO-FR-41: dragging the outer loop over its own descendant moves it and
    // nothing more — a loop is never placed inside itself, and the containment
    // test excludes the dragged element's own subtree so the inner loop is
    // never even a candidate.
    const head = loopEl("l1").querySelector(".flow-loop__head") as HTMLElement;
    fireEvent.pointerDown(head, { pointerId: 1, buttons: 1, clientX: 0, clientY: 0 });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, clientX: 30, clientY: 230 });
    fireEvent.pointerUp(canvas(), { pointerId: 1 });

    expect(doc(flows).loops[0].parentId).toBeUndefined();
    // The inner loop is still inside the outer one, not the other way round.
    expect(doc(flows).loops.find((l) => l.id === inner)?.parentId).toBe("l1");
  });

  // FLO-FR-44, FLO-FR-21: the resize stops at the members' bounds — while it is being
  // dragged, not only when it lands.
  it("clamps a resize at its members' bounds as it is dragged", async () => {
    const { flows } = await renderCanvas(DRAGGABLE);
    const handle = screen.getByLabelText("Resize loop Review cycle");

    fireEvent.pointerDown(handle, { pointerId: 1, buttons: 1, clientX: 0, clientY: 0 });
    fireEvent.pointerMove(canvas(), {
      pointerId: 1,
      buttons: 1,
      clientX: -900,
      clientY: -900,
    });

    // n3 sits at x = 300 and is a node wide, so the floor clears it. The
    // RENDERED box never drops below it.
    const rendered = loopEl("l1");
    const floor = minLoopSize(doc(flows), "l1");
    expect(parseFloat(rendered.style.width)).toBeGreaterThanOrEqual(floor.width);
    expect(parseFloat(rendered.style.height)).toBeGreaterThanOrEqual(floor.height);

    fireEvent.pointerUp(canvas(), { pointerId: 1 });

    expect(doc(flows).loops[0].size).toEqual(floor);
    // Membership is untouched by the attempt.
    expect(doc(flows).nodes.filter((n) => n.parentId === "l1")).toHaveLength(2);
  });

  // FLO-FR-08 / FLO-FR-45: the other branch of the removal dialog.
  it("removes a loop's whole subtree when the author chooses that", async () => {
    const { flows } = await renderCanvas(DRAGGABLE);
    fireEvent.pointerDown(loopEl("l1"));
    fireEvent.pointerUp(loopEl("l1"));

    await userEvent.click(screen.getByLabelText("Remove loop Review cycle"));
    await userEvent.click(
      within(screen.getByTestId("flow-remove-loop-confirm")).getByRole("button", {
        name: "Remove contents",
      }),
    );

    expect(doc(flows).loops).toHaveLength(0);
    expect(doc(flows).nodes.map((n) => n.id)).toEqual(["n1"]);
    expect(doc(flows).edges).toEqual([]);
    expect(screen.getByTestId("flow-status")).toHaveTextContent("1 node · 0 edges");
  });
});

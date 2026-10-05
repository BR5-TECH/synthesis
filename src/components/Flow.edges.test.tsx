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
import {
  canvas,
  connectNodes,
  doc,
  ID,
  nodeEl,
  TREE,
  TWO_NODES,
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

describe("edge rules on the canvas (FLO-FR-16 – FLO-FR-19, FLO-FR-20)", () => {
  it("refuses a connection from a node back to itself and signals the rejection", async () => {
    const { flows } = await renderCanvas(TWO_NODES);

    connectNodes("n1", "n1");

    expect(doc(flows).edges).toHaveLength(1);
    expect(screen.getByRole("status")).toHaveTextContent(
      "An element cannot be connected to itself.",
    );
    // A refusal is not an edit.
    expect(flows.get(ID)?.dirty).toBe(false);
  });

  it("refuses a second edge on an ordered pair but allows the reverse one", async () => {
    const { flows } = await renderCanvas(TWO_NODES);

    connectNodes("n1", "n2");

    expect(doc(flows).edges).toHaveLength(1);
    expect(doc(flows).edges[0].label).toBe("ok");
    expect(screen.getByRole("status")).toHaveTextContent(
      "already connected in that direction",
    );

    // FLO-FR-17, FLO-FR-18 second half: B->A is a separate pair, giving a two-node cycle.
    connectNodes("n2", "n1");

    expect(doc(flows).edges.map((e) => [e.from, e.to])).toEqual([
      ["n1", "n2"],
      ["n2", "n1"],
    ]);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("creates an edge that closes a longer cycle with no warning (FLO-FR-18)", async () => {
    const { flows } = await renderCanvas(
      JSON.stringify({
        version: 1,
        nodes: [
          { id: "n1", name: "A", position: { x: 0, y: 0 } },
          { id: "n2", name: "B", position: { x: 200, y: 0 } },
          { id: "n3", name: "C", position: { x: 400, y: 0 } },
        ],
        edges: [
          { id: "e1", from: "n1", to: "n2" },
          { id: "e2", from: "n2", to: "n3" },
        ],
      }),
    );

    connectNodes("n3", "n1");

    expect(doc(flows).edges).toHaveLength(3);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("labels an edge in place and removes it without touching its endpoints", async () => {
    const { flows } = await renderCanvas(
      JSON.stringify({
        version: 1,
        nodes: [
          { id: "n1", name: "A", position: { x: 0, y: 0 } },
          { id: "n2", name: "C", position: { x: 200, y: 0 } },
        ],
        edges: [{ id: "e1", from: "n1", to: "n2" }],
      }),
    );
    // An unlabelled edge still carries a marker, which is what selects it.
    expect(screen.getByTestId("flow-edge-e1")).toHaveTextContent("＋");

    fireEvent.pointerDown(screen.getByTestId("flow-edge-e1"));
    fireEvent.change(screen.getByLabelText("Edge label"), {
      target: { value: "retry" },
    });
    expect(doc(flows).edges[0].label).toBe("retry");

    await userEvent.click(screen.getByRole("button", { name: "Remove edge" }));

    expect(doc(flows).edges).toEqual([]);
    expect(doc(flows).nodes.map((n) => n.name)).toEqual(["A", "C"]);
  });
});

describe("drawing a connection (FLO-FR-32 / FLO-FR-33, FLO-FR-16, FLO-FR-17)", () => {
  /** Three unconnected nodes in a row, 200 apart: n1 at 0, n2 at 200, n3 at 400. */
  const THREE_APART = JSON.stringify({
    version: 1,
    nodes: [
      { id: "n1", name: "A", position: { x: 0, y: 0 } },
      { id: "n2", name: "B", position: { x: 200, y: 0 } },
      { id: "n3", name: "C", position: { x: 400, y: 0 } },
    ],
    edges: [],
  });

  /** Press the source's port, then move the pointer to a canvas point. */
  function dragConnection(from: string, x: number, y: number) {
    fireEvent.pointerDown(within(nodeEl(from)).getByLabelText(/^Connect from /), {
      pointerId: 1,
      buttons: 1,
      clientX: 5,
      clientY: 30,
    });
    fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, clientX: x, clientY: y });
  }

  const preview = () => screen.queryByTestId("flow-connection");

  // FLO-FR-32: the connection is visible while it is being drawn, and it is a
  // preview only — nothing is in the document until it completes.
  it("draws a provisional edge to the pointer and commits nothing until release", async () => {
    const { flows } = await renderCanvas(THREE_APART);

    expect(preview()).not.toBeInTheDocument();
    dragConnection("n1", 150, 30);

    expect(preview()).toBeInTheDocument();
    // Not snapped to anything: the nearest node's centre is 155 away.
    expect(preview()).toHaveAttribute("data-magnet", "false");
    expect(preview()?.getAttribute("d")).toContain("150 30");
    expect(doc(flows).edges).toEqual([]);

    // Abandoned over empty canvas: the preview goes, and so does the gesture.
    fireEvent.pointerUp(canvas(), { pointerId: 1 });

    expect(preview()).not.toBeInTheDocument();
    expect(doc(flows).edges).toEqual([]);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(flows.get(ID)?.dirty).toBe(false);
  });

  // FLO-FR-33: the magnet. Releasing near a node — not on it — connects to it.
  it("snaps to the nearest node in reach and connects on release", async () => {
    const { flows } = await renderCanvas(THREE_APART);

    dragConnection("n1", 250, 30);

    expect(nodeEl("n2")).toHaveAttribute("data-magnet", "true");
    expect(nodeEl("n3")).toHaveAttribute("data-magnet", "false");
    expect(preview()).toHaveAttribute("data-magnet", "true");
    // Snapped: the free end sits on n2's anchor rather than on the pointer.
    // These three are 200 apart and NODE_W wide, so their boxes overlap and
    // there is no border between them to land on — the anchor falls back to the
    // element's origin, which still points the edge at n2 (FLO-FR-48).
    expect(preview()?.getAttribute("d")).toContain("305 30");

    fireEvent.pointerUp(canvas(), { pointerId: 1 });

    expect(doc(flows).edges.map((e) => [e.from, e.to])).toEqual([["n1", "n2"]]);
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("snaps to the nearer of two nodes both in reach", async () => {
    const { flows } = await renderCanvas(THREE_APART);

    dragConnection("n1", 420, 30);
    expect(nodeEl("n3")).toHaveAttribute("data-magnet", "true");
    expect(nodeEl("n2")).toHaveAttribute("data-magnet", "false");

    fireEvent.pointerUp(canvas(), { pointerId: 1 });
    expect(doc(flows).edges.map((e) => [e.from, e.to])).toEqual([["n1", "n3"]]);
  });

  // FLO-FR-33: a node the connection could only be refused on is not a target.
  // Snapping to one would replace a connection the author can still complete
  // with one that cannot be made at all.
  it("never snaps to the source node or to one already connected that way", async () => {
    const { flows } = await renderCanvas(TWO_NODES);

    // n1 -> n2 already exists, and the pointer is right on n2.
    dragConnection("n1", 405, 30);

    expect(nodeEl("n2")).toHaveAttribute("data-magnet", "false");
    expect(preview()).toHaveAttribute("data-magnet", "false");

    // …and back over the source itself, which FLO-FR-16 refuses.
    fireEvent.pointerMove(canvas(), {
      pointerId: 1,
      buttons: 1,
      clientX: 105,
      clientY: 30,
    });
    expect(nodeEl("n1")).toHaveAttribute("data-magnet", "false");

    fireEvent.pointerUp(canvas(), { pointerId: 1 });
    expect(doc(flows).edges).toHaveLength(1);
    expect(flows.get(ID)?.dirty).toBe(false);
  });

  // Releasing directly on a node still connects to it, magnet or not — which is
  // how the author finds out the rules refused it (FLO-FR-16, FLO-FR-17).
  it("connects to the node a release lands on, refusal included", async () => {
    const { flows } = await renderCanvas(TWO_NODES);

    dragConnection("n1", 405, 30);
    fireEvent.pointerUp(nodeEl("n2"), { pointerId: 1 });

    expect(doc(flows).edges).toHaveLength(1);
    expect(screen.getByRole("status")).toHaveTextContent(
      "already connected in that direction",
    );
    expect(preview()).not.toBeInTheDocument();
  });

  // FLO-FR-32: the preview exists from the first frame, before any movement —
  // and nothing is snapped yet, so a bare click on the port cannot mint an edge
  // to whatever happens to be sitting within the snap radius.
  it("draws from the press and snaps to nothing until the pointer moves", async () => {
    const { flows } = await renderCanvas(THREE_APART);

    fireEvent.pointerDown(within(nodeEl("n1")).getByLabelText(/^Connect from /), {
      pointerId: 1,
      buttons: 1,
      // The port sits on the node's trailing edge, 96 units from n2's centre —
      // well inside MAGNET_RADIUS.
      clientX: 210,
      clientY: 18,
    });

    expect(preview()).toBeInTheDocument();
    expect(preview()).toHaveAttribute("data-magnet", "false");
    // The press point is on n1's own border, so the preview starts from there
    // rather than from the centre the edge aims out of (FLO-FR-48).
    expect(preview()?.getAttribute("d")).toMatch(/^M 210 18/);

    // …and from a point well clear of the border it still starts ON it, which
    // is what shows the projection ran rather than the pointer being copied.
    // Away from n2 and n3, so this move tests the anchor and nothing else.
    fireEvent.pointerMove(canvas(), {
      pointerId: 1,
      buttons: 1,
      clientX: -200,
      clientY: 30,
    });
    expect(preview()).toHaveAttribute("data-magnet", "false");
    expect(preview()?.getAttribute("d")).toMatch(/^M 0 30/);

    fireEvent.pointerUp(canvas(), { pointerId: 1 });

    expect(doc(flows).edges).toEqual([]);
    expect(flows.get(ID)?.dirty).toBe(false);
  });

  // A sticky magnet would leave the wrong node highlighted and connect to it.
  it("releases the magnet when the pointer leaves its reach, and on release", async () => {
    await renderCanvas(THREE_APART);

    dragConnection("n1", 250, 30);
    expect(nodeEl("n2")).toHaveAttribute("data-magnet", "true");

    fireEvent.pointerMove(canvas(), {
      pointerId: 1,
      buttons: 1,
      clientX: 150,
      clientY: 30,
    });
    expect(nodeEl("n2")).toHaveAttribute("data-magnet", "false");
    expect(preview()).toHaveAttribute("data-magnet", "false");

    fireEvent.pointerMove(canvas(), {
      pointerId: 1,
      buttons: 1,
      clientX: 250,
      clientY: 30,
    });
    fireEvent.pointerUp(canvas(), { pointerId: 1 });

    expect(
      document.querySelectorAll('[data-magnet="true"]'),
    ).toHaveLength(0);
  });

  // The drop wins over the magnet, not the other way round: an implementation
  // that only filled the target in when nothing was snapped would connect to
  // the node the pointer was near rather than the one it was released on.
  it("connects to the node released on, not to the one it had snapped to", async () => {
    const { flows } = await renderCanvas(THREE_APART);

    dragConnection("n1", 420, 30);
    expect(nodeEl("n3")).toHaveAttribute("data-magnet", "true");

    fireEvent.pointerUp(nodeEl("n2"), { pointerId: 1 });

    expect(doc(flows).edges.map((e) => [e.from, e.to])).toEqual([["n1", "n2"]]);
  });

  // The release happened where nothing could see it — the pointer left the
  // window. Re-entering must not complete the connection the magnet was holding:
  // a Flow has no undo, so an edge nobody asked for has to be found and deleted.
  it("abandons a snapped connection whose release was never seen", async () => {
    const { flows } = await renderCanvas(THREE_APART);

    dragConnection("n1", 250, 30);
    expect(nodeEl("n2")).toHaveAttribute("data-magnet", "true");

    // The pointer comes back over the canvas with no button held.
    fireEvent.pointerMove(canvas(), {
      pointerId: 1,
      buttons: 0,
      clientX: 250,
      clientY: 30,
    });

    expect(doc(flows).edges).toEqual([]);
    expect(flows.get(ID)?.dirty).toBe(false);
    expect(preview()).not.toBeInTheDocument();
  });

  // The same lost release, seen from outside the canvas. The window handler is
  // the one that exists for exactly those moves, so without its own guard the
  // connection stays live across the whole window and the user's next unrelated
  // click completes it.
  it("abandons a lost release seen only outside the canvas", async () => {
    const { flows } = await renderCanvas(THREE_APART);

    dragConnection("n1", 250, 30);
    expect(nodeEl("n2")).toHaveAttribute("data-magnet", "true");

    // Re-entering over the toolbar band, not over `.flow`.
    fireEvent.pointerMove(window, {
      pointerId: 1,
      buttons: 0,
      clientX: 250,
      clientY: 30,
    });
    fireEvent.pointerUp(window, { pointerId: 1 });

    expect(doc(flows).edges).toEqual([]);
    expect(flows.get(ID)?.dirty).toBe(false);
    expect(preview()).not.toBeInTheDocument();
  });

  // A cancel is not a release: the OS took the pointer away and the user never
  // let go of anything.
  it("abandons a snapped connection on pointercancel", async () => {
    const { flows } = await renderCanvas(THREE_APART);

    dragConnection("n1", 250, 30);
    expect(nodeEl("n2")).toHaveAttribute("data-magnet", "true");

    fireEvent.pointerCancel(canvas(), { pointerId: 1 });

    expect(doc(flows).edges).toEqual([]);
    expect(flows.get(ID)?.dirty).toBe(false);
    expect(preview()).not.toBeInTheDocument();

    // …and the same through the window, which is where a cancel outside the
    // canvas lands.
    dragConnection("n1", 250, 30);
    fireEvent.pointerCancel(window, { pointerId: 1 });

    expect(doc(flows).edges).toEqual([]);
    expect(flows.get(ID)?.dirty).toBe(false);
  });

  // The preview has to keep following a pointer that has left the canvas, or a
  // connection dragged over the toolbar freezes mid-air and reads as stuck.
  it("follows a pointer that leaves the canvas and ends there", async () => {
    const { flows } = await renderCanvas(THREE_APART);

    dragConnection("n1", 150, 30);
    fireEvent.pointerMove(window, { pointerId: 1, buttons: 1, clientX: 250, clientY: 30 });

    expect(preview()).toHaveAttribute("data-magnet", "true");

    fireEvent.pointerUp(window, { pointerId: 1 });

    expect(preview()).not.toBeInTheDocument();
    expect(doc(flows).edges.map((e) => [e.from, e.to])).toEqual([["n1", "n2"]]);
  });
});

describe("reciprocal edges keep two labels (FLO-FR-19 / FLO-FR-17)", () => {
  // `A→B` and `B→A` are two edges whose midpoints coincide. Their labels have
  // to separate, or one buries the other's affordance entirely and neither edge
  // can be told from the other.
  it("separates the labels of a two-element cycle", async () => {
    await renderCanvas(
      JSON.stringify({
        version: 1,
        name: "F",
        nodes: [
          { id: "n1", name: "A", position: { x: 0, y: 0 } },
          { id: "n2", name: "B", position: { x: 400, y: 0 } },
        ],
        edges: [
          { id: "e1", from: "n1", to: "n2" },
          { id: "e2", from: "n2", to: "n1", label: "again" },
        ],
      }),
    );

    const a = screen.getByTestId("flow-edge-e1");
    const b = screen.getByTestId("flow-edge-e2");
    const at = (el: HTMLElement) => ({
      left: parseFloat(el.style.left),
      top: parseFloat(el.style.top),
    });
    const pa = at(a);
    const pb = at(b);
    expect(Math.hypot(pa.left - pb.left, pa.top - pb.top)).toBeGreaterThan(20);
  });

  // A one-way edge is not nudged: it sits on its own midpoint.
  it("leaves a lone edge's label on the midpoint", async () => {
    await renderCanvas(TWO_NODES);
    const only = screen.getByTestId("flow-edge-e1");
    // n1 at (0,0) and n2 at (300,0): the edge runs from n1's trailing border to
    // n2's leading one (FLO-FR-48), so its midpoint is halfway between 210 and
    // 300, on the anchor line both nominal boxes share.
    expect(parseFloat(only.style.left)).toBe(255);
    expect(parseFloat(only.style.top)).toBe(30);
  });
});

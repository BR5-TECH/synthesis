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

import { FlowCanvas } from "./Flow";
import { FlowSessionStore } from "../state/flowSessions";
import { MEMBER_NODE_H, MEMBER_NODE_W } from "../state/flowDocument";
import {
  canvas,
  connectNodes,
  doc,
  ID,
  loopEl,
  nodeEl,
  selectNode,
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

describe("edge direction markers (FLO-FR-48, FLO-FR-19, FLO-FR-17 / FLO-FR-32, FLO-FR-33, FLO-FR-39, FLO-FR-03)", () => {
  const edgePath = (id: string) =>
    screen.getByTestId(`flow-edge-path-${id}`) as unknown as SVGPathElement;

  const coords = (el: Element): number[] =>
    ((el.getAttribute("d") ?? "").match(/-?\d+(?:\.\d+)?/g) ?? []).map(Number);

  /**
   * The point a path ends at, which is where its arrowhead sits (FLO-FR-48).
   * The last coordinate pair of the `d` string is the curve's own end, so this
   * reads the same geometry the renderer drew rather than a copy of it.
   */
  function pathEnd(el: Element): { x: number; y: number } {
    const n = coords(el);
    return { x: n[n.length - 2], y: n[n.length - 1] };
  }

  /** Where the path begins — the border the edge leaves from. */
  function pathStart(el: Element): { x: number; y: number } {
    const n = coords(el);
    return { x: n[0], y: n[1] };
  }

  /**
   * The `<marker>` a path's `marker-end` actually resolves to.
   *
   * Asserting the reference string alone proves nothing: a `url(#…)` pointing
   * at a definition that does not exist renders no head at all, and every edge
   * on the canvas would still carry the attribute. This follows the reference
   * to the thing it names, which is what the browser does.
   */
  function headOf(el: Element): HTMLElement | null {
    const id = (el.getAttribute("marker-end") ?? "").match(/^url\(#(.+)\)$/)?.[1];
    return id ? document.getElementById(id) : null;
  }

  /** A -> B, unlabelled. A spans x 0..210 and B spans x 300..510. */
  const UNLABELLED = JSON.stringify({
    version: 1,
    name: "F",
    nodes: [
      { id: "n1", name: "A", position: { x: 0, y: 0 } },
      { id: "n2", name: "B", position: { x: 300, y: 0 } },
    ],
    edges: [{ id: "e1", from: "n1", to: "n2" }],
  });

  // FLO-FR-48: the reference resolves to an actual arrowhead, drawn the way the
  // requirement describes. Without this the whole feature can be deleted — every
  // edge keeps a `marker-end` naming a definition that is not there, the canvas
  // shows no head at all, and nothing that only reads the attribute notices.
  it("resolves an edge's head to an arrowhead aimed along the edge", async () => {
    await renderCanvas(UNLABELLED);

    const head = headOf(edgePath("e1"));
    expect(head).not.toBeNull();
    // Turned to the curve's own tangent rather than to a fixed angle, which is
    // what makes a head follow the bend instead of the chord.
    expect(head).toHaveAttribute("orient", "auto");
    // The reference point is the tip of the viewBox, so the tip lands ON the
    // anchor. Any other refX overshoots past it, into the element the edge
    // points at — which paints over the edge layer and would swallow the head.
    expect(head?.getAttribute("refX")).toBe(
      head?.getAttribute("viewBox")?.split(" ")[2],
    );
    // Sized in stroke widths (the default), so a selected edge's heavier stroke
    // carries a head to match it and the pair keep their proportion at any zoom.
    expect(head).not.toHaveAttribute("markerUnits");
    // A triangle closed back to its base — an arrowhead, not a dot or a square.
    expect(head?.querySelector("path")).toHaveAttribute("d", "M 0 0 L 10 5 L 0 10 z");
    expect(head?.querySelector("path")).toHaveAttribute("fill", "var(--border-3)");
  });

  // FLO-FR-48: the head is at the target end and at no other, and it sits on
  // the border the edge lands at rather than under the element it points to.
  it("gives an unlabelled edge a head at its target and keeps it once labelled", async () => {
    const { flows } = await renderCanvas(UNLABELLED);

    expect(headOf(edgePath("e1"))).not.toBeNull();
    expect(edgePath("e1")).not.toHaveAttribute("marker-start");
    // B's leading border, not its centre: an edge stopped at the centre would
    // put its head under the node and there would be nothing to see.
    expect(pathEnd(edgePath("e1"))).toEqual({ x: 300, y: MEMBER_NODE_H / 2 });

    // FLO-FR-19: a label says what the branch is and the head says which way it
    // runs. Gaining one takes nothing away from the other.
    fireEvent.pointerDown(screen.getByTestId("flow-edge-e1"));
    fireEvent.change(screen.getByLabelText("Edge label"), {
      target: { value: "ok" },
    });

    expect(doc(flows).edges[0].label).toBe("ok");
    expect(pathEnd(edgePath("e1"))).toEqual({ x: 300, y: MEMBER_NODE_H / 2 });
    expect(headOf(edgePath("e1"))).not.toBeNull();
  });

  // FLO-FR-48 / FLO-FR-17: the two edges of a reciprocal pair run between the
  // same elements over the same ground, so which end carries the head is the
  // whole of what tells them apart.
  it("puts the heads of a reciprocal pair at opposite ends", async () => {
    const { flows } = await renderCanvas(UNLABELLED);

    connectNodes("n2", "n1");

    const edges = doc(flows).edges;
    expect(edges.map((e) => [e.from, e.to])).toEqual([
      ["n1", "n2"],
      ["n2", "n1"],
    ]);
    // A→B arrives on B's leading border; B→A arrives on A's trailing one.
    expect(pathEnd(edgePath("e1"))).toEqual({ x: 300, y: MEMBER_NODE_H / 2 });
    expect(pathEnd(edgePath(edges[1].id))).toEqual({
      x: MEMBER_NODE_W,
      y: MEMBER_NODE_H / 2,
    });
    // …and they run over the same ground, which is what leaves the heads as the
    // only thing telling them apart: each one starts where the other finishes.
    expect(pathStart(edgePath("e1"))).toEqual(pathEnd(edgePath(edges[1].id)));
    expect(pathStart(edgePath(edges[1].id))).toEqual(pathEnd(edgePath("e1")));
  });

  // FLO-FR-48: the head is part of the edge and carries its emphasis with it.
  it("draws the head in the emphasis its own edge carries", async () => {
    await renderCanvas(UNLABELLED);

    const idle = edgePath("e1").getAttribute("marker-end");
    // The colour the head is actually DRAWN in, not merely which definition it
    // names: two definitions whose fills were swapped would name a different id
    // apiece and still paint every idle edge in the selected colour.
    expect(headOf(edgePath("e1"))?.querySelector("path")).toHaveAttribute(
      "fill",
      "var(--border-3)",
    );
    expect(edgePath("e1")).toHaveAttribute("stroke", "var(--border-3)");

    fireEvent.pointerDown(screen.getByTestId("flow-edge-e1"));

    expect(edgePath("e1").getAttribute("marker-end")).not.toBe(idle);
    expect(headOf(edgePath("e1"))?.querySelector("path")).toHaveAttribute(
      "fill",
      "var(--accent)",
    );
    expect(edgePath("e1")).toHaveAttribute("stroke", "var(--accent)");
  });

  /**
   * Gather on open canvas, level with a loop holding two members.
   *
   * Their anchor lines share a y — Gather's is `100 + MEMBER_NODE_H/2 = 130`
   * and the loop's is `0 + 260/2 = 130` — so the edge between them runs flat
   * and the border it lands on is an exact number rather than a rounding of
   * one. The loop's leading border is its own x, 400.
   */
  const GATHER_AND_LOOP = JSON.stringify({
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
      { id: "n1", name: "Gather", position: { x: 0, y: 100 } },
      { id: "n2", name: "Draft", parentId: "l1", position: { x: 20, y: 70 } },
      { id: "n3", name: "Critique", parentId: "l1", position: { x: 200, y: 70 } },
    ],
    edges: [],
  });

  // FLO-FR-33, FLO-FR-48, FLO-FR-39, FLO-FR-03. FLO-FR-32: the connection being drawn carries the head, so which
  // way it would run is shown before the release rather than after it.
  it("carries the head on the connection being drawn and lands it on a loop's border", async () => {
    const { flows } = await renderCanvas(GATHER_AND_LOOP);

    fireEvent.pointerDown(
      within(nodeEl("n1")).getByLabelText(/^Connect from /),
      { pointerId: 1, buttons: 1, clientX: 210, clientY: 130 },
    );
    fireEvent.pointerMove(canvas(), {
      pointerId: 1,
      buttons: 1,
      clientX: 300,
      clientY: 130,
    });

    const preview = screen.getByTestId("flow-connection");
    expect(preview).toHaveAttribute("data-magnet", "false");
    // FLO-FR-32: at the FREE end and at no other. A head on the source end too
    // would say the connection runs both ways, which no edge does.
    expect(headOf(preview)?.querySelector("path")).toHaveAttribute(
      "fill",
      "var(--accent)",
    );
    expect(preview).not.toHaveAttribute("marker-start");
    // Unsnapped, the head rides the pointer.
    expect(pathEnd(preview)).toEqual({ x: 300, y: 130 });

    // FLO-FR-33: within reach of the loop, the head settles onto the border a
    // completed edge would land on — and onto the container, never onto a
    // member inside it (FLO-FR-39).
    fireEvent.pointerMove(canvas(), {
      pointerId: 1,
      buttons: 1,
      clientX: 610,
      clientY: 130,
    });

    expect(loopEl("l1")).toHaveAttribute("data-magnet", "true");
    expect(nodeEl("n2")).toHaveAttribute("data-magnet", "false");
    expect(pathEnd(screen.getByTestId("flow-connection"))).toEqual({
      x: 400,
      y: 130,
    });

    fireEvent.pointerUp(canvas(), { pointerId: 1 });

    const edges = doc(flows).edges;
    expect(edges.map((e) => [e.from, e.to])).toEqual([["n1", "l1"]]);
    // The completed edge keeps the head at that same point on the border.
    expect(headOf(edgePath(edges[0].id))).not.toBeNull();
    expect(pathEnd(edgePath(edges[0].id))).toEqual({ x: 400, y: 130 });

    // FLO-FR-03: the head is a rendering of the direction the edge already has.
    // The document gains an edge and no field describing how it is drawn.
    await act(async () => {
      await flows.flush(ID);
    });
    expect(JSON.parse(writes[0]).edges).toEqual([
      { id: edges[0].id, from: "n1", to: "l1" },
    ]);
  });

  /** B sits directly below A, so the edge between them arrives vertically. */
  const STACKED = JSON.stringify({
    version: 1,
    name: "F",
    nodes: [
      { id: "n1", name: "A", position: { x: 0, y: 0 } },
      { id: "n2", name: "B", position: { x: 0, y: 200 } },
    ],
    edges: [{ id: "e1", from: "n1", to: "n2" }],
  });

  // FLO-FR-48: an edge does not have to run left-to-right. Stacked, it leaves
  // A's bottom border and arrives on B's top one — the face-selection branch of
  // the anchor, which a horizontal fixture never reaches.
  it("lands the head on the horizontal border when the edge arrives vertically", async () => {
    await renderCanvas(STACKED);

    expect(pathStart(edgePath("e1"))).toEqual({ x: MEMBER_NODE_W / 2, y: MEMBER_NODE_H });
    expect(pathEnd(edgePath("e1"))).toEqual({ x: MEMBER_NODE_W / 2, y: 200 });
    expect(headOf(edgePath("e1"))).not.toBeNull();
  });

  /**
   * The direction the curve is travelling as it arrives — the last control
   * point to the end point. This is the tangent `orient="auto"` turns the head
   * to, so it IS the direction the arrowhead points (FLO-FR-48).
   */
  function arrivalTangent(el: Element): { x: number; y: number } {
    const n = coords(el);
    return { x: n[n.length - 2] - n[n.length - 4], y: n[n.length - 1] - n[n.length - 3] };
  }

  // FLO-FR-48: the head points along the edge's own direction where it arrives.
  // The curve's control points decide that tangent, so offsetting them always
  // horizontally — which a left-to-right graph invites — points every head
  // sideways however the edge came in, and square across one running straight
  // down. It also leaves a purely vertical edge with no tangent at all.
  it("arrives along its own direction rather than always sideways", async () => {
    // Down: the case a horizontal offset gets exactly 90 degrees wrong.
    await renderCanvas(STACKED);
    const down = arrivalTangent(edgePath("e1"));
    expect(down.x).toBe(0);
    expect(down.y).toBeGreaterThan(0);

    // Up: the same edge the other way round still arrives along itself.
    cleanup();
    await renderCanvas(
      JSON.stringify({
        version: 1,
        name: "F",
        nodes: [
          { id: "n1", name: "A", position: { x: 0, y: 200 } },
          { id: "n2", name: "B", position: { x: 0, y: 0 } },
        ],
        edges: [{ id: "e1", from: "n1", to: "n2" }],
      }),
    );
    const up = arrivalTangent(edgePath("e1"));
    expect(up.x).toBe(0);
    expect(up.y).toBeLessThan(0);

    // Sideways, which is the case that was already right: still right.
    cleanup();
    await renderCanvas(UNLABELLED);
    const across = arrivalTangent(edgePath("e1"));
    expect(across.y).toBe(0);
    expect(across.x).toBeGreaterThan(0);
  });

  // FLO-FR-48: the head is square to the border it sits on. A node is far wider
  // than it is tall, so an edge running mostly sideways can still leave and
  // arrive through the top and bottom faces — and a tangent taken from the run
  // between the two nodes would then lay the head across its own stroke, on the
  // underside of a node it is supposed to be pointing into. It reads as an
  // arrow that slid off the border rather than one entering it.
  it("arrives square to the face it attaches to, not along the run", async () => {
    // B is 300 right of A and 150 above it: mostly a sideways run, but the ray
    // leaves A's box through the top and enters B's through the bottom, the
    // box being 210 wide and 60 tall.
    await renderCanvas(
      JSON.stringify({
        version: 1,
        name: "F",
        nodes: [
          { id: "n1", name: "A", position: { x: 0, y: 150 } },
          { id: "n2", name: "B", position: { x: 300, y: 0 } },
        ],
        edges: [{ id: "e1", from: "n1", to: "n2" }],
      }),
    );

    // Attached under B, at B's own bottom edge.
    expect(pathEnd(edgePath("e1"))).toEqual({ x: 345, y: MEMBER_NODE_H });
    // …and arriving straight up into it, rather than sideways across it.
    const t = arrivalTangent(edgePath("e1"));
    expect(t.x).toBe(0);
    expect(t.y).toBeLessThan(0);
    // Leaving A through the top, on the same terms.
    expect(pathStart(edgePath("e1"))).toEqual({ x: 165, y: 150 });
  });

  // FLO-FR-22 / FLO-FR-48: selecting a node reveals its picker and its prompt
  // field and makes it much taller, but selection is view state the document
  // knows nothing about. If it moved an anchor, clicking a node would re-route
  // every edge touching it — and swing one from a node's side to its underside.
  it("does not move an edge when the node it lands on is selected", async () => {
    const painted = vi
      .spyOn(HTMLElement.prototype, "offsetHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        if (!this.classList.contains("flow-node")) return 0;
        // What selecting one actually does to it.
        return this.dataset.selected === "true" ? 200 : 50;
      });
    try {
      await renderCanvas(UNLABELLED);
      const before = edgePath("e1").getAttribute("d");

      selectNode("n2");
      expect(nodeEl("n2")).toHaveAttribute("data-selected", "true");

      expect(edgePath("e1").getAttribute("d")).toBe(before);

      // …and it is still there, unmoved, once the node is let go of again.
      selectNode("n1");
      expect(nodeEl("n2")).toHaveAttribute("data-selected", "false");
      expect(edgePath("e1").getAttribute("d")).toBe(before);
    } finally {
      painted.mockRestore();
    }
  });

  // FLO-FR-48: whichever way it runs, the curve must arrive with a tangent at
  // all. A control point sitting on its own end point leaves the head's angle
  // to whatever the renderer falls back to, which is nothing the spec asked for.
  it("always leaves the arriving head a direction to read", async () => {
    for (const [dx, dy] of [
      [300, 0],
      [0, 200],
      [0, -200],
      [-300, 0],
      [140, 260],
      [260, 140],
    ]) {
      cleanup();
      await renderCanvas(
        JSON.stringify({
          version: 1,
          name: "F",
          nodes: [
            { id: "n1", name: "A", position: { x: 400, y: 400 } },
            { id: "n2", name: "B", position: { x: 400 + dx, y: 400 + dy } },
          ],
          edges: [{ id: "e1", from: "n1", to: "n2" }],
        }),
      );
      const t = arrivalTangent(edgePath("e1"));
      expect({ dx, dy, moving: t.x !== 0 || t.y !== 0 }).toEqual({
        dx,
        dy,
        moving: true,
      });
      // …and it travels toward the target rather than away from it.
      expect({ dx, dy, forward: t.x * dx + t.y * dy > 0 }).toEqual({
        dx,
        dy,
        forward: true,
      });
    }
  });

  // FLO-FR-48: the border an edge lands on is the one the node PAINTS, which
  // grows with its references and its rendered prompt (FLO-FR-13). Anchoring to
  // the nominal height instead puts the end — and the arrowhead on it — inside
  // a tall node, under the node itself, which is the one place a head must
  // never be. jsdom lays nothing out, so the painted height stands in here.
  it("anchors on the height a node paints, not on the nominal one", async () => {
    const TALL = 160;
    const painted = vi
      .spyOn(HTMLElement.prototype, "offsetHeight", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.classList.contains("flow-node") ? TALL : 0;
      });
    try {
      await renderCanvas(STACKED);

      // A leaves from its real bottom border. The nominal height would have put
      // this at y=60 — a full 100px up, well inside the node.
      expect(pathStart(edgePath("e1"))).toEqual({ x: MEMBER_NODE_W / 2, y: TALL });
      // …and arrives on B's real top border, which is B's own y either way.
      expect(pathEnd(edgePath("e1"))).toEqual({ x: MEMBER_NODE_W / 2, y: 200 });

      // The line edges radiate from is NOT the tall node's middle: it stays a
      // fixed distance below the top, so a node that grows — on being selected,
      // say — does not drag every edge incident to it downward and re-route a
      // graph nothing changed in. Side-on is where that shows.
      cleanup();
      await renderCanvas(UNLABELLED);
      expect(pathEnd(edgePath("e1"))).toEqual({ x: 300, y: MEMBER_NODE_H / 2 });
    } finally {
      painted.mockRestore();
    }
  });

  // FLO-FR-48: each end is measured out from its own element, so two elements
  // close enough to overlap can push their ends past one another — and an edge
  // drawn end-past-end runs BACKWARDS, head and all. A head that points the
  // wrong way is read rather than disregarded, so this is the one thing the
  // anchor may never do, however little room it is given.
  it("never reverses an edge, however close its two elements sit", async () => {
    // Every offset from well clear of overlapping to sitting almost on top,
    // through the width and the height of a node alike.
    for (const [dx, dy] of [
      [300, 0],
      [211, 0],
      [180, 40],
      [120, 0],
      [40, 55],
      [0, 61],
      [0, 30],
      [12, 8],
    ]) {
      cleanup();
      await renderCanvas(
        JSON.stringify({
          version: 1,
          name: "F",
          nodes: [
            { id: "n1", name: "A", position: { x: 0, y: 0 } },
            { id: "n2", name: "B", position: { x: dx, y: dy } },
          ],
          edges: [{ id: "e1", from: "n1", to: "n2" }],
        }),
      );

      const start = pathStart(edgePath("e1"));
      const end = pathEnd(edgePath("e1"));
      // The edge runs the same way B lies from A: their dot product is what
      // says so, and it is never allowed to go negative.
      const dot = (end.x - start.x) * dx + (end.y - start.y) * dy;
      expect({ dx, dy, dot: dot >= 0 }).toEqual({ dx, dy, dot: true });
      expect(coords(edgePath("e1")).every(Number.isFinite)).toBe(true);
    }
  });

  // Two elements on the same spot: the anchor has no direction to leave in and
  // returns the origin, which collapses the path to a point. Nothing renders,
  // and nothing is allowed to come out as NaN and take the whole `d` with it.
  it("degrades to a point rather than to NaN when two elements coincide", async () => {
    await renderCanvas(
      JSON.stringify({
        version: 1,
        name: "F",
        nodes: [
          { id: "n1", name: "A", position: { x: 40, y: 40 } },
          { id: "n2", name: "B", position: { x: 40, y: 40 } },
        ],
        edges: [{ id: "e1", from: "n1", to: "n2" }],
      }),
    );

    const d = edgePath("e1").getAttribute("d") ?? "";
    expect(d).not.toMatch(/NaN|Infinity/);
    expect(coords(edgePath("e1")).every(Number.isFinite)).toBe(true);
  });
});

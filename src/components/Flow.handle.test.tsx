/**
 * What a host asks of a mounted canvas through its `ref`
 * (`../../specifications/ui/MDL-map-delivery-lens.md` MDL-FR-JIKK,
 * MDL-FR-EEDO, MDL-FR-QAJN, MDL-FR-JRGK; `SDL-specification-delivery-links.md`
 * SDL-FR-MCGI; `FLO-flow.md` FLO-FR-27, FLO-FR-47).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { createRef, useState } from "react";

import { resetLogBufferForTest } from "../logging";
import {
  absolutePosition,
  findItem,
  renameItem,
  resolveLayout,
  type FlowDocument,
} from "../state/flowDocument";
import { FlowSessionStore } from "../state/flowSessions";
import { AUTOSAVE_DELAY_MS } from "../state/writeSchedule";
import { currentMainBranchWrite, rememberConfirmation, settleMainBranchWrite } from "../state/mainBranchWrite";
import { mainGuard, scriptedReads } from "../test/mainBranchWriteFixtures";
import { resetToasts } from "../state/toasts";
import {
  FlowCanvas,
  fileBinding,
  type FlowCanvasHandle,
  type FlowPlanAnswer,
  type FlowPlanBinding,
  type FlowPlanChange,
} from "./Flow";
import {
  callsOf,
  clearFlowStores,
  flowBackend,
  planOf,
  renderFlow,
  SCOPE,
} from "../test/flowCanvasHarness";
import { HLD_ID, ID, itemEl, queryItem } from "../test/flowFixtures";
import { resetListeners } from "../test/flowMocks";
import { item, plan, sampleLayout, samplePlan } from "../test/flowPlanFixtures";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("../test/flowMocks")).invoke,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../test/flowMocks")).listen,
}));

beforeEach(() => {
  resetLogBufferForTest();
  resetToasts();
  resetListeners();
});

afterEach(() => {
  cleanup();
  clearFlowStores();
});

/** A file-bound canvas with its handle. */
async function fileCanvas(doc: FlowDocument = samplePlan()) {
  const b = flowBackend(doc, sampleLayout());
  const handle = createRef<FlowCanvasHandle>();
  const r = await renderFlow(b, { ref: handle });
  return { b, handle, ...r };
}

/** A plan-bound canvas, with a host that answers each edit with the plan given. */
function planCanvas(answers: (change: FlowPlanChange) => FlowPlanAnswer) {
  const b = flowBackend("", sampleLayout());
  const handle = createRef<FlowCanvasHandle>();
  const changes: FlowPlanChange[] = [];
  function Lens() {
    const [plan, setPlan] = useState<FlowDocument>(samplePlan());
    const binding: FlowPlanBinding = {
      kind: "plan",
      scope: "github:PVT_1",
      document: plan,
      states: null,
      overlaps: null,
      tracker: [],
      repository: "acme/payments",
      onEdit: async (change) => {
        changes.push(change);
        setPlan(change.next);
        const answer = answers(change);
        if (answer.ok) setPlan(answer.document);
        return answer;
      },
      onRemoveRequest: () => {},
    };
    return <FlowCanvas binding={binding} ref={handle} />;
  }
  render(<Lens />);
  return { b, handle, changes };
}

describe("reveal (MDL-FR-JIKK, SDL-FR-MCGI)", () => {
  it("MDL-FR-JIKK: reveal selects the item and expands each collapsed container that holds it", async () => {
    const stored = sampleLayout();
    stored.items.m1 = { ...stored.items.m1, collapsed: true };
    stored.items.f2 = { ...stored.items.f2, collapsed: true };
    const again = flowBackend(samplePlan(), stored);
    const ref = createRef<FlowCanvasHandle>();
    await renderFlow(again, { ref });
    expect(queryItem("t3")).toBeNull();

    act(() => ref.current!.reveal("t3"));
    expect(await screen.findByTestId("flow-item-t3")).toHaveAttribute("data-selected", "true");
    expect(screen.getByRole("button", { name: "Collapse M1" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Collapse F2" })).toBeInTheDocument();
    // FLO-FR-TDHR: expanding is a layout change, stored once.
    const saves = callsOf(again, "save_delivery_layout");
    expect(saves).toHaveLength(1);
    expect(saves[0].scope).toBe(SCOPE);
  });

  it("SDL-FR-MCGI: a reveal asked before the plan loads applies when it renders", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    let release!: () => void;
    b.on.load_delivery_layout = () => new Promise((res) => (release = () => res(sampleLayout())));
    const ref = createRef<FlowCanvasHandle>();
    await renderFlow(b, { ref });
    act(() => ref.current!.reveal("t2"));
    expect(queryItem("t2")).toBeNull();

    await act(async () => release());
    await waitFor(() => expect(itemEl("t2")).toHaveAttribute("data-selected", "true"));
  });

  it("MDL-FR-JIKK: a reveal of an id the plan does not hold changes nothing", async () => {
    const { b, handle } = await fileCanvas();
    act(() => handle.current!.reveal("gone"));
    await act(async () => {});
    expect(document.querySelector('[data-selected="true"]')).toBeNull();
    expect(callsOf(b, "save_delivery_layout")).toEqual([]);
  });
});

describe("addTask (MDL-FR-EEDO)", () => {
  it("MDL-FR-EEDO: in a file binding, the task is one add at the top level with its name and references, and it is selected", async () => {
    const { handle, flows } = await fileCanvas();
    let id: string | null = null;
    await act(async () => {
      id = await handle.current!.addTask({ name: "Plan the specs", references: [HLD_ID, "docs/b.md"] });
    });

    expect(id).not.toBeNull();
    const added = findItem(planOf(flows), id!)!;
    expect(added).toMatchObject({ kind: "task", name: "Plan the specs", artifactIds: [HLD_ID, "docs/b.md"] });
    expect(added.parentId).toBeUndefined();
    expect(flows.get(ID)!.dirty).toBe(true);
    expect(itemEl(id!)).toHaveAttribute("data-selected", "true");
  });

  it("MDL-FR-EEDO: in a plan binding, the task is a create_item edit, then a set_references edit once the answer names it", async () => {
    const { handle, changes } = planCanvas((change) => {
      // The host's answer names the new issue by its own id.
      const created = change.edits.find((e) => e.kind === "create_item");
      if (!created) return { ok: true, document: change.next };
      const doc = samplePlan();
      return { ok: true, document: { ...doc, items: [...doc.items, item("issue-90", "task", { name: "Plan the specs" })] } };
    });
    await waitFor(() => expect(itemEl("t1")).toBeInTheDocument());

    let id: string | null = null;
    await act(async () => {
      id = await handle.current!.addTask({ name: "Plan the specs", references: [HLD_ID] });
    });

    expect(id).toBe("issue-90");
    expect(changes.map((c) => c.edits)).toEqual([
      [{ kind: "create_item", itemKind: "task", name: "Plan the specs", containerId: null }],
      [{ kind: "set_references", itemId: "issue-90", artifactIds: [HLD_ID] }],
    ]);
    await waitFor(() => expect(itemEl("issue-90")).toHaveAttribute("data-selected", "true"));
  });

  it("MDL-FR-EEDO: with no references, the plan binding sends the create_item edit alone", async () => {
    const { handle, changes } = planCanvas(() => {
      const doc = samplePlan();
      return { ok: true, document: { ...doc, items: [...doc.items, item("issue-91", "task", { name: "Solo" })] } };
    });
    await waitFor(() => expect(itemEl("t1")).toBeInTheDocument());

    let id: string | null = null;
    await act(async () => {
      id = await handle.current!.addTask({ name: "Solo", references: [] });
    });
    expect(id).toBe("issue-91");
    expect(changes).toHaveLength(1);
  });

  it("MDL-FR-EEDO: an answer that names no new task resolves null and sends no second edit", async () => {
    const { handle, changes } = planCanvas(() => ({ ok: true, document: samplePlan() }));
    await waitFor(() => expect(itemEl("t1")).toBeInTheDocument());

    let id: string | null = "x";
    await act(async () => {
      id = await handle.current!.addTask({ name: "Lost", references: [HLD_ID] });
    });
    expect(id).toBeNull();
    expect(changes).toHaveLength(1);
  });

  it("MDL-FR-WMGU, MDL-FR-EEDO: an inert canvas adds no task", async () => {
    const b = flowBackend(samplePlan(), sampleLayout());
    const handle = createRef<FlowCanvasHandle>();
    const { flows } = await renderFlow(b, { ref: handle, inert: true });
    let id: string | null = "x";
    await act(async () => {
      id = await handle.current!.addTask({ name: "No", references: [] });
    });
    expect(id).toBeNull();
    expect(planOf(flows).items).toHaveLength(8);
  });

  it("MDL-FR-EEDO: a refused create_item adds nothing and resolves null", async () => {
    const { handle, changes } = planCanvas(() => ({ ok: false, error: "issue_type_unavailable" }));
    await waitFor(() => expect(itemEl("t1")).toBeInTheDocument());

    let id: string | null = "x";
    await act(async () => {
      id = await handle.current!.addTask({ name: "Plan", references: [HLD_ID] });
    });
    expect(id).toBeNull();
    expect(changes).toHaveLength(1);
  });
});

describe("flush and resetLayout (FLO-FR-27, FLO-FR-47, MDL-FR-JRGK, MDL-FR-QAJN)", () => {
  it("FLO-FR-27, MDL-FR-JRGK: flush writes the pending changes of a file binding now", async () => {
    const { b, handle, flows } = await fileCanvas();
    act(() => {
      flows.applyEdit(ID, (d) => renameItem(d, "t1", "Tokenize"));
    });

    let result: Awaited<ReturnType<FlowCanvasHandle["flush"]>> | null = null;
    await act(async () => {
      result = await handle.current!.flush();
    });
    expect(result).toEqual({ ok: true });
    expect(b.writes).toHaveLength(1);
    expect(flows.get(ID)!.dirty).toBe(false);
  });

  it("FLO-FR-47, MDL-FR-JRGK: a refused write makes flush say so, so the host stops the publish", async () => {
    const { b, handle, flows } = await fileCanvas();
    vi.useFakeTimers();
    try {
      act(() => {
        flows.applyEdit(ID, (d) => renameItem(d, "t1", "Tokenize"));
      });
      b.validation = {
        valid: false,
        violations: [
          { code: "shape", message: "the first violation" },
          { code: "edge_within_lineage", message: "the second violation", edgeId: "e1" },
        ],
      };

      let result: { ok: boolean } | null = null;
      await act(async () => {
        result = await handle.current!.flush();
      });
      expect(result!.ok).toBe(false);
      expect(b.writes).toEqual([]);
      expect(flows.get(ID)!.dirty).toBe(true);
      const listed = screen.getByTestId("flow-violations");
      expect(listed).toHaveTextContent("the first violation");
      expect(listed).toHaveTextContent("the second violation");
      // No timer tries the write again.
      expect(flows.hasPendingWrite(ID)).toBe(false);
      const checks = callsOf(b, "validate_flow_document").length;
      await act(async () => {
        await vi.advanceTimersByTimeAsync(AUTOSAVE_DELAY_MS * 3);
      });
      expect(callsOf(b, "validate_flow_document")).toHaveLength(checks);
      expect(callsOf(b, "save_artifact_contents")).toEqual([]);
      expect(flows.get(ID)!.dirty).toBe(true);
    } finally {
      vi.useRealTimers();
    }
  });

  it("MDL-FR-XJBV, FLO-FR-27: flush of a plan binding writes nothing and resolves ok", async () => {
    const { b, handle } = planCanvas((c) => ({ ok: true, document: c.next }));
    await waitFor(() => expect(itemEl("t1")).toBeInTheDocument());
    let result: { ok: boolean } | null = null;
    await act(async () => {
      result = await handle.current!.flush();
    });
    expect(result).toEqual({ ok: true });
    expect(callsOf(b, "save_artifact_contents")).toEqual([]);
  });

  it("MDL-FR-QAJN: resetLayout stores the empty layout, and the canvas lays the plan out again", async () => {
    const { b, handle } = await fileCanvas();
    expect(itemEl("t2").style.left).toBe("240px");

    act(() => handle.current!.resetLayout());
    expect(callsOf(b, "save_delivery_layout")).toEqual([{ scope: SCOPE, layout: { items: {} } }]);
    const auto = resolveLayout(samplePlan(), { items: {} });
    expect(itemEl("t2").style.left).toBe(`${absolutePosition(samplePlan(), auto, "t2").x}px`);
  });
});

describe("a host that renders the canvas again", () => {
  it("FLO-FR-30: a file binding made again for the same Flow keeps the plan and its layout", async () => {
    const flows = new FlowSessionStore();
    const b = flowBackend(samplePlan(), sampleLayout());
    await act(async () => {
      flows.openTab(ID);
      await flows.get(ID)?.pendingLoad;
    });
    const view = render(<FlowCanvas binding={fileBinding(flows, ID)} />);
    await screen.findByTestId("flow-item-t1");
    view.rerender(<FlowCanvas binding={fileBinding(flows, ID)} />);
    expect(itemEl("t1").style.left).toBe("40px");
    expect(callsOf(b, "load_delivery_layout")).toHaveLength(1);
    flows.clear();
  });
});

describe("Plan work while the edit gate holds (MDL-FR-EEDO, SDL-FR-CFHH, EDT-FR-UPNU)", () => {
  async function addWhileHeld() {
    scriptedReads(mainGuard());
    const r = await fileCanvas();
    let added!: Promise<string | null>;
    act(() => {
      added = r.handle.current!.addTask({ name: "Planned", references: ["specifications/ui/A1-a1.md"] });
    });
    // The first change on the main branch is held, and the confirmation shows.
    await waitFor(() => expect(currentMainBranchWrite()).not.toBeNull());
    expect(planOf(r.flows).items.some((i) => i.name === "Planned")).toBe(false);
    return { ...r, added };
  }

  it("MDL-FR-EEDO, EDT-FR-UPNU: a task asked for while the gate holds goes in, with its references, once the author confirms the write", async () => {
    const { flows, added } = await addWhileHeld();
    await act(async () => {
      rememberConfirmation(mainGuard({ confirmed: true }));
      settleMainBranchWrite(true);
    });
    const id = await added;
    expect(id).not.toBeNull();
    const task = planOf(flows).items.find((i) => i.id === id)!;
    expect(task).toMatchObject({ kind: "task", name: "Planned", artifactIds: ["specifications/ui/A1-a1.md"] });
    expect(flows.get(ID)!.dirty).toBe(true);
    await waitFor(() => expect(itemEl(id!)).toHaveAttribute("data-selected", "true"));
  });

  it("MDL-FR-EEDO, EDT-FR-UPNU: a cancel of the confirmation adds nothing", async () => {
    const { flows, added } = await addWhileHeld();
    await act(async () => settleMainBranchWrite(false));
    expect(await added).toBeNull();
    expect(planOf(flows).items.some((i) => i.name === "Planned")).toBe(false);
    expect(flows.get(ID)!.dirty).toBe(false);
  });

  it("MDL-FR-EEDO: a task asked for while the branch read is under way goes in when the read finds no main branch", async () => {
    const reads = scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    let release!: () => void;
    const held = new Promise<void>((res) => (release = res));
    const read = reads.read.bind(reads);
    reads.read = (branch, refresh) => held.then(() => read(branch, refresh));
    const { flows, handle } = await fileCanvas();
    let added!: Promise<string | null>;
    act(() => {
      added = handle.current!.addTask({ name: "Planned", references: [] });
    });
    expect(planOf(flows).items.some((i) => i.name === "Planned")).toBe(false);
    await act(async () => release());
    const id = await added;
    expect(id).not.toBeNull();
    expect(planOf(flows).items.find((i) => i.id === id)?.name).toBe("Planned");
    expect(currentMainBranchWrite()).toBeNull();
  });
});

describe("a reveal the layout moves after (MDL-FR-JIKK, FLO-FR-SQUS)", () => {
  /** A plan binding whose host can render another plan later. */
  function movingPlan(first: FlowDocument) {
    flowBackend("", { items: {} });
    const handle = createRef<FlowCanvasHandle>();
    let show!: (doc: FlowDocument) => void;
    function Lens() {
      const [doc, setDoc] = useState(first);
      show = setDoc;
      const binding: FlowPlanBinding = {
        kind: "plan",
        scope: "github:PVT_1",
        document: doc,
        states: null,
        overlaps: null,
        tracker: [],
        repository: "acme/payments",
        onEdit: async (change) => ({ ok: true, document: change.next }),
        onRemoveRequest: () => {},
      };
      return <FlowCanvas binding={binding} ref={handle} />;
    }
    render(<Lens />);
    return { handle, show: (doc: FlowDocument) => act(() => show(doc)) };
  }

  const viewX = () => {
    const t = (document.querySelector(".flow-viewport") as HTMLElement).style.transform;
    return Number.parseFloat(/translate\((-?[\d.]+)px/.exec(t)![1]);
  };
  /** jsdom measures the canvas 0 wide, so the canvas takes it as 800 wide. */
  const centredX = (id: string) => {
    const el = itemEl(id);
    const width = Math.min(Number.parseFloat(el.style.width), 800);
    return 400 - (Number.parseFloat(el.style.left) + width / 2);
  };

  const before = plan([item("m1", "milestone"), item("m2", "milestone")]);
  const after = plan([item("m0", "milestone"), item("m1", "milestone"), item("m2", "milestone")]);

  it("MDL-FR-JIKK, FLO-FR-SQUS: a revealed column that the layout moves is centred again where it lands", async () => {
    const { handle, show } = movingPlan(before);
    await waitFor(() => expect(itemEl("m2")).toBeInTheDocument());
    act(() => handle.current!.reveal("m2"));
    await waitFor(() => expect(viewX()).toBe(centredX("m2")));
    const first = viewX();
    // A column comes before m2, so m2 moves right.
    show(after);
    await waitFor(() => expect(viewX()).toBe(centredX("m2")));
    expect(viewX()).toBeLessThan(first);
  });

  it("MDL-FR-JIKK: once the author moves the view, a later move of the layout does not centre the item again", async () => {
    const { handle, show } = movingPlan(before);
    await waitFor(() => expect(itemEl("m2")).toBeInTheDocument());
    act(() => handle.current!.reveal("m2"));
    await waitFor(() => expect(viewX()).toBe(centredX("m2")));
    fireEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    const moved = (document.querySelector(".flow-viewport") as HTMLElement).style.transform;
    show(after);
    await act(async () => {});
    expect((document.querySelector(".flow-viewport") as HTMLElement).style.transform).toBe(moved);
  });
});

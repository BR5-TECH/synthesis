/*
 * Delivery plan stand-in (FLO-flow.md, FDL, FOA, DMS), for UI auditing only.
 *
 * `flows/release.flow.md` is served as a version 3 delivery plan with a
 * hierarchy, blocked items, an overlap, and containers that hold enough to
 * show collapsed. The delivery state and the overlaps are answered from fixed
 * data, so every marker can be seen in a browser. The layout of each scope is
 * kept in memory, seeded for the plan, so a drag, a resize, and a collapse
 * survive a tab switch until the page reloads.
 */

import { NOT_HANDLED } from "./mock-github-polling";

/** FLO-FR-03: the plan, in the shape `serializeFlowDocument` writes. */
export const FLOW_PLAN = {
  version: 3,
  name: "Payments v2",
  description: "Card payments for the EU launch.",
  references: ["flows/payments-hld.md", "specifications/ui/LIB-library.md"],
  items: [
    {
      id: "m1",
      kind: "milestone",
      name: "Beta",
      description: "Accept **Visa** and **Mastercard**.",
      due: "2027-03-31",
    },
    { id: "ep1", kind: "epic", name: "Checkout", parentId: "m1" },
    {
      id: "f1",
      kind: "feature",
      name: "Card capture",
      description:
        "## Description\n\nFull card capture for the hosted checkout, with **tokenization** before any server sees the number.\n\n## Intent\n\nNo card data in our logs.",
      parentId: "ep1",
    },
    {
      id: "t1",
      kind: "task",
      name: "Tokenize card numbers",
      description: "Store a token, never the `PAN`.",
      parentId: "f1",
      draftId: "d-tok",
      footprint: ["specifications/core/PST-project-storage.md"],
    },
    {
      id: "t2",
      kind: "task",
      name: "Card form",
      description: "Card **number**, expiry and CVC.",
      parentId: "f1",
      draftId: "d-card",
      footprint: ["specifications/ui/", "src/payments/card_form.tsx"],
    },
    {
      id: "t3",
      kind: "task",
      name: "3DS challenge for cards that the issuer flags as high risk",
      description: [
        "## Description",
        "",
        "Run the **3-D Secure** challenge when the issuer asks for it, and resume the payment after the shopper answers. The challenge opens in a frame of the checkout page.",
        "",
        "- Support `frictionless` and `challenge` flows",
        "- Time out after 10 minutes",
        "- Show the issuer page in an iframe",
        "",
        "## Intent",
        "",
        "Meet the *PSD2* strong customer authentication rule without losing conversion.",
        "",
        "1. Measure the drop-off at the challenge",
        "2. Fall back to a redirect when the frame is blocked",
        "",
        "## Notes",
        "",
        "See [the issuer guide](https://example.com/3ds) for the full list of result codes.",
      ].join("\n"),
      parentId: "f1",
      footprint: ["specifications/ui/LIB-library.md"],
    },
    { id: "f2", kind: "feature", name: "Refunds", parentId: "ep1" },
    { id: "t4", kind: "task", name: "Refund API", parentId: "f2" },
    { id: "t5", kind: "task", name: "Partial refunds", parentId: "f2" },
    { id: "t6", kind: "task", name: "Refund emails", parentId: "f2" },
    {
      id: "x1",
      kind: "defect",
      name: "Decline message is wrong",
      parentId: "m1",
      draftId: "d-decline",
    },
    { id: "f3", kind: "feature", name: "Fraud rules", parentId: "m1" },
    {
      id: "t7",
      kind: "task",
      name: "Velocity check",
      description: "Footprint shares `specifications/ui/LIB-library.md`.",
      parentId: "f3",
      draftId: "d-velocity",
      footprint: ["specifications/ui/LIB-library.md"],
    },
    { id: "m2", kind: "milestone", name: "GA", due: "2027-06-30" },
    { id: "f4", kind: "feature", name: "Payouts", parentId: "m2" },
    { id: "t8", kind: "task", name: "Payout schedule", parentId: "f4", draftId: "d-payout" },
    {
      id: "f5",
      kind: "feature",
      name: "Disputes",
      description: "## Description\n\nLet the merchant answer a chargeback with evidence before the deadline of the card scheme.",
      parentId: "m2",
    },
  ],
  edges: [
    { id: "e1", from: "t1", to: "t2", type: "blocks" },
    { id: "e2", from: "f1", to: "f2", type: "blocks" },
    { id: "e3", from: "m1", to: "m2", type: "blocks" },
    { id: "e4", from: "x1", to: "t3", type: "relates" },
  ],
};

export const FLOW_PLAN_BODY = `${JSON.stringify(FLOW_PLAN, null, 2)}\n`;

type Layout = {
  items: Record<
    string,
    { position?: { x: number; y: number }; size?: { width: number; height: number }; collapsed?: boolean }
  >;
};

const at = (x: number, y: number, width?: number, height?: number) => ({
  position: { x, y },
  ...(width !== undefined && height !== undefined ? { size: { width, height } } : {}),
});

/** DMS-FR-DMWD: the layout of each scope, seeded for the plan of `FLOW_PLAN`. */
const layouts = new Map<string, Layout>([
  [
    "flow:flows/release.flow.md",
    {
      items: {
        m1: at(16, 48, 928, 526),
        ep1: at(16, 48, 600, 462),
        f1: at(16, 48, 568, 230),
        t1: at(16, 48),
        t2: at(204, 48),
        t3: at(392, 48),
        f2: at(16, 318, 568, 128),
        t4: at(16, 44),
        t5: at(204, 44),
        t6: at(392, 44),
        x1: at(650, 48),
        f3: at(632, 236, 280, 274),
        t7: at(55, 48),
        m2: at(976, 48, 354, 460),
        f5: at(16, 300),
        f4: at(16, 48, 322, 236),
        t8: at(16, 48),
      },
    },
  ],
]);

type State = "not_started" | "drafting" | "in_progress" | "done" | "dropped";

const leafStates: Record<string, State> = {
  t1: "in_progress",
  t2: "drafting",
  t3: "not_started",
  t4: "not_started",
  t5: "not_started",
  t6: "not_started",
  x1: "drafting",
  t7: "drafting",
  t8: "drafting",
};

const draftNames: Record<string, string> = {
  "d-tok": "tokenize-card-numbers",
  "d-card": "card-form",
  "d-decline": "decline-message",
  "d-velocity": "velocity-check",
  "d-payout": "payout-schedule",
};

interface PlanItem {
  id: string;
  kind: string;
  parentId?: string;
  draftId?: string;
}

/** FDL-FR-VGZI / FDL-FR-JYKC / FDL-FR-PXHL, from the fixed leaf states above. */
export function deliveryStateOf(body: string) {
  let doc: { items: PlanItem[]; edges: Array<{ from: string; to: string; type: string }> };
  try {
    doc = JSON.parse(body);
  } catch {
    return { items: [] };
  }
  const items = doc.items ?? [];
  const leaves = (id: string): string[] => {
    const kids = items.filter((i) => i.parentId === id);
    if (kids.length === 0) {
      const self = items.find((i) => i.id === id);
      return self && (self.kind === "task" || self.kind === "defect") ? [id] : [];
    }
    return kids.flatMap((k) => leaves(k.id));
  };
  const stateOf = (id: string): State => {
    const item = items.find((i) => i.id === id);
    if (!item) return "not_started";
    if (item.kind === "task" || item.kind === "defect") {
      return leafStates[id] ?? (item.draftId ? "drafting" : "not_started");
    }
    const ls = leaves(id).map(stateOf);
    if (ls.length === 0) return "not_started";
    if (ls.every((s) => s === "dropped")) return "dropped";
    if (ls.every((s) => s === "done" || s === "dropped")) return "done";
    if (ls.some((s) => s === "drafting" || s === "in_progress" || s === "done")) return "in_progress";
    return "not_started";
  };
  const ancestors = (id: string): string[] => {
    const out: string[] = [];
    let at = items.find((i) => i.id === id)?.parentId;
    while (at) {
      out.push(at);
      at = items.find((i) => i.id === at)?.parentId;
    }
    return out;
  };
  return {
    items: items.map((item) => {
      const state = stateOf(item.id);
      const targets = [item.id, ...ancestors(item.id)];
      const blockedBy = (doc.edges ?? [])
        .filter((e) => e.type === "blocks" && targets.includes(e.to))
        .map((e) => e.from)
        .filter((from) => !["done", "dropped"].includes(stateOf(from)));
      const container = !(item.kind === "task" || item.kind === "defect");
      const counts = { notStarted: 0, drafting: 0, inProgress: 0, done: 0, dropped: 0 };
      if (container) {
        for (const l of leaves(item.id)) {
          const s = stateOf(l);
          if (s === "not_started") counts.notStarted += 1;
          if (s === "drafting") counts.drafting += 1;
          if (s === "in_progress") counts.inProgress += 1;
          if (s === "done") counts.done += 1;
          if (s === "dropped") counts.dropped += 1;
        }
      }
      return {
        itemId: item.id,
        state,
        draft: item.draftId
          ? { id: item.draftId, name: draftNames[item.draftId] ?? item.draftId, status: "active" }
          : null,
        draftMissing: false,
        run: item.id === "t1" ? { runId: "run-tok", state: "working" } : null,
        blocked: blockedBy.length > 0 && state !== "done" && state !== "dropped",
        blockedBy,
        leafCounts: container ? counts : null,
      };
    }),
  };
}

/** The commands of the delivery plan; NOT_HANDLED for every other one. */
export function flowPlanInvoke(cmd: string, a: Record<string, unknown>): unknown {
  switch (cmd) {
    case "load_flow_delivery_state":
      return deliveryStateOf(String(a.body ?? ""));
    case "analyze_flow_overlap":
      return {
        overlaps: [{ items: ["t3", "t7"], paths: ["specifications/ui/LIB-library.md"] }],
      };
    case "load_delivery_layout":
      return layouts.get(String(a.scope)) ?? { items: {} };
    case "save_delivery_layout": {
      const layout = a.layout as Layout;
      if (Object.keys(layout?.items ?? {}).length === 0) layouts.delete(String(a.scope));
      else layouts.set(String(a.scope), layout);
      return undefined;
    }
    case "open_tracker_link":
      return undefined;
    case "find_flow_items_for_draft":
      return [];
    default:
      return NOT_HANDLED;
  }
}

/** FGV-FR-02: a cut-down stand-in for the validator of a version 3 plan. */
export function validateFlowPlan(body: string): {
  valid: boolean;
  violations: Array<{ code: string; message: string }>;
} {
  if (body.trim() === "") return { valid: true, violations: [] };
  let raw: Record<string, unknown>;
  try {
    raw = JSON.parse(body) as Record<string, unknown>;
  } catch (e) {
    return { valid: false, violations: [{ code: "not_json", message: `the file is not valid JSON: ${e}` }] };
  }
  if (raw?.version !== 3) {
    return {
      valid: false,
      violations: [{ code: "unknown_version", message: "expected a Flow document of version 3" }],
    };
  }
  return { valid: true, violations: [] };
}

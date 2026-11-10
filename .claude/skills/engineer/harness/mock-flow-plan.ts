/*
 * Delivery plan stand-in (FLO-flow.md, FSR-flow-sync-review.md, FDL, FOA, FTS),
 * for UI auditing only.
 *
 * `flows/release.flow.md` is served as a version 2 delivery plan with a
 * hierarchy, blocked items, an overlap, tracker links, a local-only task, and
 * containers that hold enough to show collapsed. The delivery state, the
 * overlaps, and the sync are answered from fixed data, so every marker and the
 * sync review can be seen in a browser.
 */

import { NOT_HANDLED } from "./mock-github-polling";

const ref = (number: number, kind: "issue" | "milestone" = "issue") => ({
  kind,
  host: "github.com",
  owner: "acme",
  repo: "payments",
  number,
  url:
    kind === "milestone"
      ? `https://github.com/acme/payments/milestone/${number}`
      : `https://github.com/acme/payments/issues/${number}`,
});

const base = (name: string, extra: Record<string, unknown> = {}) => ({
  name,
  description: "",
  state: "open",
  type: null,
  parent: null,
  milestone: null,
  blockedBy: [],
  ...extra,
});

const link = (itemId: string, number: number, name: string, kind: "issue" | "milestone" = "issue") => ({
  itemId,
  ref: ref(number, kind),
  base: kind === "milestone" ? { name, description: "", state: "open", due: null } : base(name),
});

/** FLO-FR-03: the plan, in the shape `serializeFlowDocument` writes. */
export const FLOW_PLAN = {
  version: 2,
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
      position: { x: 16, y: 48 },
      size: { width: 928, height: 526 },
    },
    { id: "ep1", kind: "epic", name: "Checkout", parentId: "m1", position: { x: 16, y: 48 }, size: { width: 600, height: 462 } },
    { id: "f1", kind: "feature", name: "Card capture", parentId: "ep1", position: { x: 16, y: 48 }, size: { width: 568, height: 230 } },
    {
      id: "t1",
      kind: "task",
      name: "Tokenize card numbers",
      description: "Store a token, never the `PAN`.",
      parentId: "f1",
      draftId: "d-tok",
      footprint: ["specifications/core/PST-project-storage.md"],
      position: { x: 16, y: 48 },
    },
    {
      id: "t2",
      kind: "task",
      name: "Card form",
      description: "Card **number**, expiry and CVC.",
      parentId: "f1",
      localOnly: true,
      draftId: "d-card",
      footprint: ["specifications/ui/", "src/payments/card_form.tsx"],
      position: { x: 204, y: 48 },
    },
    {
      id: "t3",
      kind: "task",
      name: "3DS challenge",
      parentId: "f1",
      footprint: ["specifications/ui/LIB-library.md"],
      position: { x: 392, y: 48 },
    },
    { id: "f2", kind: "feature", name: "Refunds", parentId: "ep1", position: { x: 16, y: 318 }, size: { width: 568, height: 128 } },
    { id: "t4", kind: "task", name: "Refund API", parentId: "f2", position: { x: 16, y: 44 } },
    { id: "t5", kind: "task", name: "Partial refunds", parentId: "f2", position: { x: 204, y: 44 } },
    { id: "t6", kind: "task", name: "Refund emails", parentId: "f2", position: { x: 392, y: 44 } },
    {
      id: "x1",
      kind: "defect",
      name: "Decline message is wrong",
      parentId: "m1",
      draftId: "d-decline",
      position: { x: 650, y: 48 },
    },
    { id: "f3", kind: "feature", name: "Fraud rules", parentId: "m1", position: { x: 632, y: 236 }, size: { width: 280, height: 274 } },
    {
      id: "t7",
      kind: "task",
      name: "Velocity check",
      description: "Footprint shares `specifications/ui/LIB-library.md`.",
      parentId: "f3",
      draftId: "d-velocity",
      footprint: ["specifications/ui/LIB-library.md"],
      position: { x: 55, y: 48 },
    },
    { id: "m2", kind: "milestone", name: "GA", due: "2027-06-30", position: { x: 976, y: 48 }, size: { width: 354, height: 300 } },
    { id: "f4", kind: "feature", name: "Payouts", parentId: "m2", position: { x: 16, y: 48 }, size: { width: 322, height: 236 } },
    { id: "t8", kind: "task", name: "Payout schedule", parentId: "f4", draftId: "d-payout", position: { x: 16, y: 48 } },
  ],
  edges: [
    { id: "e1", from: "t1", to: "t2", type: "blocks" },
    { id: "e2", from: "f1", to: "f2", type: "blocks" },
    { id: "e3", from: "m1", to: "m2", type: "blocks" },
    { id: "e4", from: "x1", to: "t3", type: "relates" },
  ],
  sync: {
    tracker: "github",
    links: [
      link("m1", 3, "Beta", "milestone"),
      link("ep1", 41, "Checkout"),
      link("f1", 42, "Card capture"),
      link("f2", 46, "Refunds"),
      link("f3", 48, "Fraud rules"),
      link("t1", 51, "Tokenize card numbers"),
      link("t3", 53, "3DS challenge"),
      link("x1", 47, "Decline message is wrong"),
      link("t7", 55, "Velocity check"),
      link("t9", 49, "Old spike"),
    ],
    // FTS-FR-YHVD: a tracker item an earlier Unlink keeps out of every sync.
    ignored: [ref(60)],
  },
};

export const FLOW_PLAN_BODY = `${JSON.stringify(FLOW_PLAN, null, 2)}\n`;

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
function deliveryState(body: string) {
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

/** FTS-FR-LVTU: the result the review shows, as in the FSR wireframe. */
const SYNC_RESULT = {
  created: 0,
  updated: 2,
  added: 0,
  changed: 1,
  conflicts: [
    {
      id: "c-name-f1",
      kind: "field",
      itemId: "f1",
      itemName: "Card capture",
      field: "name",
      base: "Card capture",
      flow: "Card capture",
      tracker: "Card capture and 3DS",
      choices: ["keep_flow", "take_tracker"],
      url: "https://github.com/acme/payments/issues/42",
    },
    {
      id: "c-parent-t1",
      kind: "invalid_on_tracker",
      itemId: "t1",
      itemName: "Tokenize card numbers",
      field: "parent",
      base: ref(42),
      flow: ref(42),
      tracker: ref(41),
      choices: ["keep_flow", "unlink"],
      url: "https://github.com/acme/payments/issues/51",
    },
    {
      id: "c-gone-t9",
      kind: "removed_in_flow",
      itemId: "t9",
      itemName: "Old spike",
      field: null,
      base: null,
      flow: null,
      tracker: null,
      choices: ["unlink", "restore"],
      url: "https://github.com/acme/payments/issues/49",
    },
  ],
  skipped: [
    {
      number: 58,
      title: "Load test plan",
      url: "https://github.com/acme/payments/issues/58",
      reason: "no_kind_fits",
    },
    {
      number: null,
      title: "Payout schedule",
      url: null,
      itemId: "t8",
      reason: "issue_type_unavailable",
    },
  ],
  stale: [],
};

/** The commands of the delivery plan; NOT_HANDLED for every other one. */
export function flowPlanInvoke(cmd: string, a: Record<string, unknown>): unknown {
  switch (cmd) {
    case "load_flow_delivery_state":
      return deliveryState(String(a.body ?? ""));
    case "analyze_flow_overlap":
      return {
        overlaps: [{ items: ["t3", "t7"], paths: ["specifications/ui/LIB-library.md"] }],
      };
    case "sync_flow_with_tracker":
      return new Promise((res) => setTimeout(() => res(SYNC_RESULT), 600));
    case "resolve_flow_sync_conflicts": {
      const ids = new Set(
        ((a.resolutions as Array<{ conflictId: string }>) ?? []).map((r) => r.conflictId),
      );
      return {
        ...SYNC_RESULT,
        conflicts: SYNC_RESULT.conflicts.filter((c) => !ids.has(c.id)),
      };
    }
    case "read_tracker_item": {
      const n = Number(a.number);
      if (n === 404) {
        return Promise.reject({
          code: "tracker_item_not_found",
          message: `No issue #${n} in acme/payments.`,
        });
      }
      return {
        ref: ref(n, a.kind === "milestone" ? "milestone" : "issue"),
        ...base(`Issue ${n} from GitHub`),
      };
    }
    case "open_tracker_link":
      return undefined;
    case "find_flow_items_for_draft":
      return [];
    default:
      return NOT_HANDLED;
  }
}

/** FGV-FR-02: a cut-down stand-in for the validator of a version 2 plan. */
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
  if (raw?.version !== 2) {
    return {
      valid: false,
      violations: [{ code: "unknown_version", message: "expected a Flow document of version 2" }],
    };
  }
  return { valid: true, violations: [] };
}

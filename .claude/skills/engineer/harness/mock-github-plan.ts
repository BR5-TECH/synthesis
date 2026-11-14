/*
 * GitHub delivery plan stand-in (GDP-github-delivery-plan.md), for UI auditing
 * only.
 *
 * With `?deliveryGithub` the delivery source is the GitHub Project "Roadmap"
 * (acme/platform), and its plan is the release plan of `mock-flow-plan.ts`
 * with tracker facts: milestones #3 and #4, issues from #40, a Ready task
 * (Card form, #44) that can be claimed, a Status on each issue, and an
 * external blocker on Payout schedule. The notices hold two unmapped Types
 * until Map Types writes them, one misplaced item, and one omitted relation.
 * With `?deliveryStale` the answer is marked stale.
 *
 * Each edit of `"apply github delivery edit"` changes the plan in memory and
 * answers it. A rename to a name that holds "conflict" refuses with
 * `changed_on_tracker`, and Add existing issue refuses issue 1 with
 * `already_in_plan`, so the refusal paths can be seen.
 */

import { FLOW_PLAN, deliveryStateOf } from "./mock-flow-plan";

const flag = (name: string) => new URLSearchParams(location.search).has(name);

type Item = {
  id: string;
  kind: string;
  name: string;
  parentId?: string;
  description?: string;
  due?: string;
  draftId?: string;
  footprint?: string[];
  artifactIds?: string[];
};
type Edge = { id: string; from: string; to: string; type: string };
type Doc = { version: 3; name: string; description?: string; references: string[]; items: Item[]; edges: Edge[] };

export const GITHUB_PROJECT = {
  nodeId: "PVT_roadmap",
  title: "Roadmap",
  ownerLogin: "acme",
  number: 7,
  /** MDL-FR-BZHY: the URL that the band's `owner/name` link opens. */
  url: "https://github.com/orgs/acme/projects/7",
};

const doc: Doc = structuredClone({ ...FLOW_PLAN, name: GITHUB_PROJECT.title }) as Doc;
const numbers = new Map<string, number>();
const updated = new Map<string, string>();
let nextIssue = 40;
let nextId = 1;
const issueOf = (id: string) => {
  if (!numbers.has(id)) {
    const item = doc.items.find((i) => i.id === id);
    numbers.set(id, item?.kind === "milestone" ? (id === "m1" ? 3 : 4) : nextIssue++);
  }
  return numbers.get(id)!;
};
for (const item of doc.items) issueOf(item.id);

const STATUS: Record<string, string> = { t1: "In Progress", t2: "Ready", x1: "In Progress", t7: "In Progress", t8: "Todo" };

function tracker() {
  return doc.items.map((item) => {
    const number = issueOf(item.id);
    const milestone = item.kind === "milestone";
    return {
      itemId: item.id,
      ref: {
        kind: milestone ? "milestone" : "issue",
        number,
        url: `https://github.com/acme/platform/${milestone ? "milestone" : "issues"}/${number}`,
      },
      state: "open",
      closeReason: null,
      updatedAt: updated.get(item.id) ?? "2026-11-01T09:00:00Z",
      status: milestone ? null : (STATUS[item.id] ?? "Todo"),
      claimEligible: item.kind === "task" && (STATUS[item.id] ?? "Todo") === "Ready" && !item.draftId,
      externalBlockers:
        item.id === "t8" ? [{ number: 88, title: "Payout provider contract", url: "https://github.com/acme/legal/issues/88" }] : [],
    };
  });
}

/** MDL-FR-IEWL: the Types that the read entries do not map yet. */
function unmappedTypes(readEntries: { type: string }[], untyped: string | null) {
  const out: { type: string | null; count: number }[] = [];
  if (!readEntries.some((e) => e.type === "Spike")) out.push({ type: "Spike", count: 3 });
  if (untyped === null) out.push({ type: null, count: 2 });
  return out;
}

export function githubPlanAnswer(settings: { readEntries: { type: string }[]; untyped: string | null }) {
  const body = JSON.stringify(doc);
  const states = deliveryStateOf(body).items.map((s: any) =>
    s.itemId === "t8" ? { ...s, blocked: true } : s,
  );
  const stale = flag("deliveryStale");
  return {
    document: structuredClone(doc),
    tracker: tracker(),
    states,
    overlaps: [{ items: ["t3", "t7"], paths: ["specifications/ui/LIB-library.md"] }],
    notices: {
      unmappedTypes: unmappedTypes(settings.readEntries, settings.untyped),
      misplaced: [{ itemId: "x1", parentItemId: "t3" }],
      omittedRelations: [{ blockedItemId: "t2", blockingItemId: "f1", reason: "lineage" }],
      skipped: 1,
    },
    readAt: stale ? new Date(Date.now() - 25 * 60_000).toISOString() : new Date(Date.now() - 60_000).toISOString(),
    stale,
    error: stale ? { code: "request_failed", message: "GitHub did not answer in time" } : null,
  };
}

function subtree(id: string): Set<string> {
  const out = new Set([id]);
  let grew = true;
  while (grew) {
    grew = false;
    for (const i of doc.items) {
      if (i.parentId && out.has(i.parentId) && !out.has(i.id)) {
        out.add(i.id);
        grew = true;
      }
    }
  }
  return out;
}

const touch = (id: string) => updated.set(id, new Date().toISOString());

/** GDP-FR-VNNU: apply one edit to the plan in memory, or refuse it. */
export function applyGithubEdit(edit: any): { code: string; message: string } | null {
  const find = (id: string) => doc.items.find((i) => i.id === id);
  const at = edit.itemId ? find(edit.itemId) : undefined;
  if (edit.itemId && !at && edit.kind !== "remove_item") {
    return { code: "tracker_item_not_found", message: "The item is not in the plan any more." };
  }
  switch (edit.kind) {
    case "create_item": {
      const id = `gh-${nextId++}`;
      doc.items.push({
        id,
        kind: edit.itemKind,
        name: edit.name,
        ...(edit.containerId ? { parentId: edit.containerId } : {}),
        ...(edit.description ? { description: edit.description } : {}),
        ...(edit.due ? { due: edit.due } : {}),
      });
      issueOf(id);
      touch(id);
      return null;
    }
    case "rename":
      if (String(edit.name).includes("conflict")) {
        touch(edit.itemId);
        return { code: "changed_on_tracker", message: "The issue changed on GitHub." };
      }
      at!.name = edit.name;
      break;
    case "set_description":
      at!.description = edit.description || undefined;
      break;
    case "set_due":
      at!.due = edit.due ?? undefined;
      break;
    case "set_kind":
      at!.kind = edit.itemKind;
      break;
    case "move":
      if (edit.containerId) at!.parentId = edit.containerId;
      else delete at!.parentId;
      break;
    case "add_edge":
      doc.edges.push({ id: `ge-${nextId++}`, from: edit.from, to: edit.to, type: edit.type });
      touch(edit.to);
      return null;
    case "remove_edge":
      doc.edges = doc.edges.filter((e) => !(e.from === edit.from && e.to === edit.to && e.type === edit.type));
      touch(edit.to);
      return null;
    case "remove_item": {
      const gone = subtree(edit.itemId);
      doc.items = doc.items.filter((i) => !gone.has(i.id));
      doc.edges = doc.edges.filter((e) => !gone.has(e.from) && !gone.has(e.to));
      return null;
    }
    case "add_existing_issue": {
      if (Number(edit.number) === 1) return { code: "already_in_plan", message: "Issue #1 is in the plan already." };
      const id = `gh-${nextId++}`;
      doc.items.push({
        id,
        kind: "task",
        name: `Issue ${edit.number} from GitHub`,
        ...(edit.containerId ? { parentId: edit.containerId } : {}),
      });
      numbers.set(id, Number(edit.number));
      return null;
    }
    case "set_references":
      at!.artifactIds = edit.artifactIds.length > 0 ? [...edit.artifactIds] : undefined;
      break;
    case "set_footprint":
      at!.footprint = edit.footprint.length > 0 ? [...edit.footprint] : undefined;
      break;
    case "link_draft":
      at!.draftId = edit.draftId;
      break;
    case "unlink_draft":
      delete at!.draftId;
      break;
    case "set_plan_references":
      doc.references = [...edit.references];
      return null;
    case "set_milestone_order": {
      // GDP-FR-YBHD: the overlay keeps the milestone order; the harness keeps
      // it as the order of the milestone items in the plan.
      const ids: string[] = edit.milestoneIds;
      const ordered = ids.map((id) => find(id)).filter((i): i is Item => i !== undefined);
      let next = 0;
      doc.items = doc.items.map((i) => (ids.includes(i.id) ? ordered[next++] : i));
      return null;
    }
    default:
      return { code: "unsupported", message: `The harness does not apply ${edit.kind}.` };
  }
  touch(edit.itemId);
  return null;
}

/** DSR-FR-JWYO: the plan as a Flow file body, for Export to file. */
export function githubPlanBody(): string {
  return `${JSON.stringify({ ...doc }, null, 2)}\n`;
}

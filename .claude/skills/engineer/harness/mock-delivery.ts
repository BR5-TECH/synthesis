/*
 * Delivery source stand-in (DSR-delivery-source.md), for UI auditing only.
 *
 * The delivery is the seeded `flows/release.flow.md` plan ("Payments v2") until
 * the Delivery section of Project settings changes the source. The milestones
 * are the plan's two milestones, Beta (2027-03-31) and GA (2027-06-30), with
 * the leaf counts the fixed leaf states of `mock-flow-plan.ts` give, and one
 * undated milestone so the No date group of the Dashboard can be seen.
 *
 * Flags: `?deliveryNone` starts with no delivery, `?deliveryStale` marks the
 * milestones answer (and the GitHub plan) stale, `?deliveryFails` refuses the
 * milestones read, `?deliveryGithub` starts with the GitHub Project "Roadmap"
 * as the source (see `mock-github-plan.ts`), and `?deliveryPublishFails` makes
 * the first Publish to GitHub stop after one created issue. `?deliveryUndated`
 * removes the due date of every milestone, and `?deliveryPastDue` moves the
 * due date of Beta before today.
 *
 * Publish to GitHub makes GitHub the source; Export to file writes the GitHub
 * plan to the chosen path and makes that file the source.
 */

import { NOT_HANDLED } from "./mock-github-polling";
import { GITHUB_PROJECT, applyGithubEdit, githubIssueRead, githubPlanAnswer, githubPlanBody } from "./mock-github-plan";

const FLOW_ID = "flows/release.flow.md";

const flag = (name: string) => new URLSearchParams(location.search).has(name);

type Settings = {
  source: { kind: "automatic" } | { kind: "github" } | { kind: "file"; flowId: string };
  createTypes: { epic: string; feature: string; task: string; defect: string };
  readEntries: { type: string; kind: string }[];
  untyped: string | null;
};

let settings: Settings = {
  source: flag("deliveryNone")
    ? { kind: "automatic" }
    : flag("deliveryGithub")
      ? { kind: "github" }
      : { kind: "file", flowId: FLOW_ID },
  createTypes: { epic: "Epic", feature: "Feature", task: "Task", defect: "Bug" },
  readEntries: [{ type: "Spike", kind: "task" }],
  untyped: null,
};

/** The Flow files the harness can serve as the delivery: the seeded one and each export. */
const knownFlows = new Set([FLOW_ID]);
/** A GitHub Project is selected once `?deliveryGithub` or a publish chose one. */
let githubSelected = flag("deliveryGithub");
let publishAttempts = 0;

const githubSource = () => ({
  kind: "github",
  project: GITHUB_PROJECT,
  repository: { host: "github.com", owner: "acme", name: "platform" },
  // MDL-FR-WAOZ: `?deliveryGithubUnchecked` gives the state after a restart, which the lens loads too.
  configuration: {
    state: flag("deliveryGithubUnchecked") ? "unchecked" : "valid",
    errorCode: null,
    error: null,
    missingStatus: null,
    projectTitle: GITHUB_PROJECT.title,
  },
});

/** DSR-FR-AECK: Automatic resolves to the selected GitHub Project, or to none. */
function resolvedSource(): any {
  switch (settings.source.kind) {
    case "file":
      return knownFlows.has(settings.source.flowId)
        ? { kind: "file", flowId: settings.source.flowId, path: settings.source.flowId }
        : { kind: "none", reason: "delivery_file_missing", flowId: settings.source.flowId };
    case "github":
      return githubSelected ? githubSource() : { kind: "none", reason: "github_project_unset", flowId: null };
    default:
      return githubSelected ? githubSource() : { kind: "none", reason: "no_source", flowId: null };
  }
}

const counts = (notStarted: number, drafting: number, inProgress: number, done: number, dropped = 0) => ({
  notStarted,
  drafting,
  inProgress,
  done,
  dropped,
});

/** DSR-FR-FQZD: the plan's milestones, in the order of FLO-FR-PLUZ. */
const MILESTONES = [
  {
    itemId: "m1",
    name: "Beta",
    due: "2027-03-31",
    state: "in_progress",
    leafCounts: counts(4, 3, 1, 0),
    blockedLeaves: 4,
    current: true,
  },
  {
    itemId: "m2",
    name: "GA",
    due: "2027-06-30",
    state: "in_progress",
    leafCounts: counts(0, 1, 0, 0),
    blockedLeaves: 1,
    current: false,
  },
  {
    itemId: "m3",
    name: "v2.1",
    due: null,
    state: "not_started",
    leafCounts: counts(0, 0, 0, 0),
    blockedLeaves: 0,
    current: false,
  },
];

/** DSR-FR-NFFP / DSR-FR-CPYM: the checks the backend makes on a write. */
function refusal(next: Settings): { code: string; message: string } | null {
  if (next.source.kind === "file" && next.source.flowId !== FLOW_ID) {
    return { code: "not_a_flow", message: "The chosen file is not a Flow file of this project." };
  }
  const creates = Object.values(next.createTypes);
  const reads = next.readEntries.map((e) => e.type);
  const names = [...creates, ...reads];
  if (names.some((n) => n.trim() === "") || new Set(names).size !== names.length) {
    return {
      code: "invalid_delivery_settings",
      message: "Each Type name must not be empty, and one Type can map to one kind only.",
    };
  }
  return null;
}

function fire(name: string, payload: unknown) {
  queueMicrotask(() => {
    const emit = (globalThis as Record<string, unknown>).__fireBusEvent as
      | ((n: string, p?: unknown) => void)
      | undefined;
    emit?.(name, payload);
  });
}

/**
 * The commands of the delivery source and of the GitHub plan; NOT_HANDLED for
 * every other one. `writeBody` stores the body of a file an export writes.
 */
export function deliveryInvoke(
  cmd: string,
  a: Record<string, unknown>,
  writeBody?: (path: string, body: string) => void,
): unknown {
  switch (cmd) {
    case "load_github_delivery_plan":
      if (resolvedSource().kind !== "github") {
        return Promise.reject({ code: "not_github_source", message: "The delivery source is not GitHub." });
      }
      return githubPlanAnswer(settings);
    case "read_github_plan_issue":
      return githubIssueRead(Number(a.number));
    case "apply_github_delivery_edit": {
      const refused = applyGithubEdit(a.edit);
      if (refused) return Promise.reject(refused);
      return githubPlanAnswer(settings);
    }
    case "publish_delivery_to_github": {
      if (resolvedSource().kind !== "file") {
        return Promise.reject({ code: "not_file_source", message: "The delivery source is not a Flow file." });
      }
      publishAttempts += 1;
      if (flag("deliveryPublishFails") && publishAttempts === 1) {
        return Promise.reject({
          code: "request_failed",
          message: "GitHub refused to create the milestone GA.",
          createdUrls: ["https://github.com/acme/platform/milestone/3"],
        });
      }
      githubSelected = true;
      settings = { ...settings, source: { kind: "github" } };
      fire("delivery-source-changed", resolvedSource());
      return { milestones: 2, issues: 14, relations: 4, createdUrls: [] };
    }
    case "export_delivery_to_file": {
      const path = String(a.path ?? "");
      if (resolvedSource().kind !== "github") {
        return Promise.reject({ code: "not_github_source", message: "The delivery source is not GitHub." });
      }
      if (knownFlows.has(path)) return Promise.reject({ code: "path_exists", message: `A file is at ${path}.` });
      knownFlows.add(path);
      writeBody?.(path, githubPlanBody());
      settings = { ...settings, source: { kind: "file", flowId: path } };
      fire("delivery-source-changed", resolvedSource());
      return { flowId: path, path };
    }
    case "get_delivery_settings":
      return structuredClone(settings);
    case "set_delivery_settings": {
      const next = a.settings as Settings;
      const refused = refusal(next);
      if (refused) return Promise.reject(refused);
      settings = structuredClone(next);
      // DSR-FR-EGFF: each stored change emits the resolved source.
      fire("delivery-source-changed", resolvedSource());
      return structuredClone(settings);
    }
    case "get_delivery_source":
      return resolvedSource();
    case "load_delivery_milestones": {
      const source = resolvedSource();
      if (source.kind === "none") {
        return { source, planName: null, milestones: [], readAt: null, stale: false };
      }
      if (flag("deliveryFails")) {
        return Promise.reject({ code: "flow_invalid", message: "The delivery Flow file is not valid." });
      }
      return {
        source,
        planName: "Payments v2",
        // `?deliveryUndated`: no milestone has a due date (DSH-FR-NZXN).
        // `?deliveryPastDue`: Beta is past due, so its card holds every line.
        milestones: flag("deliveryUndated")
          ? MILESTONES.map((m) => ({ ...m, due: null }))
          : flag("deliveryPastDue")
            ? MILESTONES.map((m) => (m.itemId === "m1" ? { ...m, due: "2026-09-30" } : m))
            : MILESTONES,
        readAt: flag("deliveryStale") ? new Date(Date.now() - 20 * 60_000).toISOString() : null,
        stale: flag("deliveryStale"),
      };
    }
    default:
      return NOT_HANDLED;
  }
}

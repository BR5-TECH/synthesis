/*
 * GitHub polling stand-in (GPP-github-polling.md), for UI auditing only.
 *
 * Seeds one ready task, one pending claim, and one GitHub-shadow draft (`gh-1`,
 * also listed by `list_drafts` in mock-core.ts), so the Git panel's Ready tasks
 * section, the Drafts panel's GitHub row, and the read-only New Artifact tab
 * are all reachable without a network.
 *
 * `?githubPollingInvalid` starts with a configuration error, and
 * `?githubPollingUnset` with no Project selected. The Roadmap Project's
 * `Status` field says `In progress`, so the default selection of the status
 * selectors is case-insensitive (GPP-FR-VXSB).
 */

export const NOT_HANDLED = Symbol("not handled");

function flag(name: string): boolean {
  return new URLSearchParams(location.search).has(name);
}

function fire(name: string, payload: unknown) {
  queueMicrotask(() => {
    const emit = (globalThis as Record<string, unknown>).__fireBusEvent as
      | ((n: string, p?: unknown) => void)
      | undefined;
    emit?.(name, payload);
  });
}

const PROJECTS = [
  { nodeId: "PVT_roadmap", title: "Roadmap", ownerLogin: "acme", number: 3 },
  { nodeId: "PVT_ops", title: "Ops board", ownerLogin: "kira", number: 1 },
];

/** GPP-FR-NNSJ: the `Status` options of each Project; Ops board has none. */
const STATUS_OPTIONS: Record<string, { id: string; name: string }[]> = {
  PVT_roadmap: [
    { id: "s1", name: "Todo" },
    { id: "s2", name: "Ready" },
    { id: "s3", name: "In progress" },
    { id: "s4", name: "Done" },
  ],
};

/** GPP-FR-VXSB: a stored name matches exactly; an unset one by its default. */
function resolveStatus(projectNodeId: string, stored: string | null, fallback: string) {
  const options = STATUS_OPTIONS[projectNodeId] ?? [];
  return stored !== null
    ? options.find((o) => o.name === stored)
    : options.find((o) => o.name.toLowerCase() === fallback.toLowerCase());
}

/** The shadow draft the seeded drafts list carries as `gh-1`. */
export const SHADOW_ISSUE = {
  repositoryOwner: "acme",
  repositoryName: "platform",
  issueNumber: 9,
  issueUrl: "https://github.com/acme/platform/issues/9",
  projectNodeId: "PVT_roadmap",
  claimState: "claimed",
};

const state: any = {
  settings: {
    projectNodeId: flag("githubPollingUnset") ? null : "PVT_roadmap",
    intervalMinutes: flag("githubPollingUnset") ? null : 5,
    readyStatus: flag("githubPollingUnset") ? null : flag("githubPollingInvalid") ? "Queued" : "Ready",
    inProgressStatus: flag("githubPollingUnset") ? null : "In progress",
  },
  configuration: flag("githubPollingUnset")
    ? { state: "unset", errorCode: null, error: null, missingStatus: null, projectTitle: null }
    : flag("githubPollingInvalid")
      ? {
          state: "invalid",
          errorCode: "ready_option_missing",
          error:
            "The Ready status “Queued” is not an option of the “Status” field of the GitHub Project “Roadmap”. Choose an option in the GitHub Project settings.",
          missingStatus: "Queued",
          projectTitle: "Roadmap",
        }
      : { state: "unchecked", errorCode: null, error: null, missingStatus: null, projectTitle: "Roadmap" },
  repository: { owner: "acme", name: "platform" },
  polling: false,
  tasks: [] as any[],
  stale: false,
  lastErrorCode: null,
  lastError: null,
  lastSuccessAt: null,
  pendingClaims: [
    {
      repositoryOwner: "acme",
      repositoryName: "platform",
      issueNumber: 17,
      issueUrl: "https://github.com/acme/platform/issues/17",
      projectNodeId: "PVT_roadmap",
      draftId: null,
      claimedAt: "2026-09-30T10:00:00Z",
    },
  ],
  shadows: [
    {
      draftId: "gh-1",
      name: "Cache invalidation",
      status: "github_shadow",
      ...SHADOW_ISSUE,
      locked: false,
    },
  ],
};

/** The issues GitHub "holds" as Ready; a poll reads them. */
const remoteReady = [
  { issueNumber: 42, title: "Add retry to the sync job" },
  { issueNumber: 43, title: "Show the queue depth in the status bar" },
];
const reported = new Set<number>();
let shadowSeq = 2;

const view = () => JSON.parse(JSON.stringify(state));

function poll() {
  if (state.settings.projectNodeId === null) throw "polling_unconfigured";
  if (state.configuration.state === "invalid") throw "polling_configuration_invalid";
  const claimed = new Set([
    ...state.pendingClaims.map((c: any) => c.issueNumber),
    ...state.shadows.map((s: any) => s.issueNumber),
  ]);
  state.tasks = remoteReady
    .filter((i) => !claimed.has(i.issueNumber))
    .map((i) => ({
      repositoryOwner: "acme",
      repositoryName: "platform",
      issueNumber: i.issueNumber,
      title: i.title,
      url: `https://github.com/acme/platform/issues/${i.issueNumber}`,
      status: "Ready",
    }));
  state.stale = false;
  state.lastErrorCode = null;
  state.lastError = null;
  state.lastSuccessAt = new Date().toISOString();
  state.configuration = { ...state.configuration, state: "valid" };
  const fresh = state.tasks.filter((t: any) => !reported.has(t.issueNumber));
  fresh.forEach((t: any) => reported.add(t.issueNumber));
  fire("github-polling-changed", {
    newIssues: fresh.map((t: any) => ({ issueNumber: t.issueNumber, title: t.title })),
  });
  return view();
}

/**
 * Handle one polling command, or answer NOT_HANDLED. `addShadowDraft` lets
 * mock-core list a shadow draft a claim created.
 */
export function githubPollingInvoke(
  cmd: string,
  a: Record<string, any>,
  addShadowDraft: (id: string, name: string, issue: typeof SHADOW_ISSUE) => void,
): unknown {
  switch (cmd) {
    case "list_github_projects":
      return PROJECTS;
    case "list_github_project_statuses": {
      const options = STATUS_OPTIONS[String(a.projectNodeId)];
      if (!options) throw "status_field_missing";
      return options;
    }
    case "get_github_polling_state":
      return view();
    case "set_github_polling_settings": {
      const interval = a.intervalMinutes ?? null;
      if (interval !== null && ![1, 5, 15, 30, 60].includes(interval))
        throw "invalid_interval";
      const selected = a.projectNodeId ?? null;
      const ready = selected === null ? null : (a.readyStatus || null);
      const inProgress = selected === null ? null : (a.inProgressStatus || null);
      if (ready !== null && ready === inProgress) throw "duplicate_status_option";
      state.settings = {
        projectNodeId: selected,
        intervalMinutes: interval,
        readyStatus: ready,
        inProgressStatus: inProgress,
      };
      const project = PROJECTS.find((p) => p.nodeId === selected);
      const title = project?.title ?? null;
      const missing =
        selected === null || selected === "PVT_ops"
          ? null
          : !resolveStatus(selected, ready, "Ready")
            ? { code: "ready_option_missing", label: "Ready", name: ready ?? "Ready" }
            : !resolveStatus(selected, inProgress, "In Progress")
              ? { code: "in_progress_option_missing", label: "In Progress", name: inProgress ?? "In Progress" }
              : null;
      state.configuration =
        selected === null
          ? { state: "unset", errorCode: null, error: null, missingStatus: null, projectTitle: null }
          : selected === "PVT_ops"
            ? {
                state: "invalid",
                errorCode: "status_field_missing",
                error:
                  "“Ops board” has no single-select field named “Status”. Add it, or select another Project in Project settings.",
                missingStatus: null,
                projectTitle: title,
              }
            : missing
              ? {
                  state: "invalid",
                  errorCode: missing.code,
                  error: `The ${missing.label} status “${missing.name}” is not an option of the “Status” field of the GitHub Project “${title}”. Choose an option in the GitHub Project settings.`,
                  missingStatus: missing.name,
                  projectTitle: title,
                }
              : { state: "valid", errorCode: null, error: null, missingStatus: null, projectTitle: title };
      fire("github-polling-changed", { newIssues: [] });
      return view();
    }
    case "poll_github_ready_tasks":
      return poll();
    case "claim_github_task":
    case "retry_github_claim": {
      const n = Number(a.issueNumber);
      const pending = state.pendingClaims.find((c: any) => c.issueNumber === n);
      if (cmd === "claim_github_task" && pending) throw "claim_pending";
      if (cmd === "retry_github_claim" && !pending) throw "no_pending_claim";
      const remote = remoteReady.find((i) => i.issueNumber === n);
      const title = remote?.title ?? `Issue ${n}`;
      const existing = state.shadows.find((s: any) => s.issueNumber === n);
      const draftId = existing?.draftId ?? `gh-${shadowSeq++}`;
      const issue = {
        ...SHADOW_ISSUE,
        issueNumber: n,
        issueUrl: `https://github.com/acme/platform/issues/${n}`,
      };
      if (!existing) {
        state.shadows.push({ draftId, name: title, status: "github_shadow", ...issue, locked: false });
        addShadowDraft(draftId, title, issue);
      }
      state.tasks = state.tasks.filter((t: any) => t.issueNumber !== n);
      if (!pending)
        state.pendingClaims.push({
          repositoryOwner: "acme",
          repositoryName: "platform",
          issueNumber: n,
          issueUrl: issue.issueUrl,
          projectNodeId: issue.projectNodeId,
          draftId,
          claimedAt: new Date().toISOString(),
        });
      else pending.draftId = draftId;
      fire("github-polling-changed", { newIssues: [] });
      return { draftId, draftName: title };
    }
    case "acknowledge_github_claim": {
      const n = Number(a.issueNumber);
      state.pendingClaims = state.pendingClaims.filter((c: any) => c.issueNumber !== n);
      fire("github-polling-changed", { newIssues: [] });
      return null;
    }
    case "open_github_task_issue":
      return null;
    default:
      return NOT_HANDLED;
  }
}

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Dashboard, hasPendingGitContent, pendingGitRows, runTone } from "./Dashboard";
import type {
  ActiveDraftItem,
  AgentRunItem,
  OpenableArtifact,
  PendingGitActivity,
  RecentlyEditedArtifact,
} from "../types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/**
 * The four refresh channels and the failure channel, captured so a test can
 * deliver one the way the backend would (DSH-FR-15, DSH-FR-16, DSH-FR-18).
 *
 * The real `listen` is mocked rather than stubbed away: what these scenarios are
 * about is that the surface **re-invokes the loader the event names**, which
 * cannot be observed if the subscription never happens.
 */
const listeners: Record<string, ((payload: unknown) => void)[]> = {};
function emitEvent(name: string, payload: unknown = {}) {
  (listeners[name] ?? []).forEach((handler) => handler(payload));
}

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (name: string, handler: (event: { payload: unknown }) => void) => {
      const wrapped = (payload: unknown) => handler({ payload });
      (listeners[name] ??= []).push(wrapped);
      return () => {
        listeners[name] = (listeners[name] ?? []).filter((h) => h !== wrapped);
      };
    },
  ),
}));

const RECENTLY_EDITED = "dashboard-recently-edited-changed";
const ACTIVE_DRAFTS = "dashboard-active-drafts-changed";
const AGENT_ACTIVITY = "dashboard-agent-activity-changed";
const PENDING_GIT = "dashboard-pending-git-changed";
const REFRESH_FAILED = "dashboard-refresh-failed";
const WORKTREE_CHANGED = "worktree-context-changed";

function markdown(id: string, name: string): RecentlyEditedArtifact {
  return { id, name, kind: "markdown", modifiedAt: "2026-05-15T09:00:00.000Z" };
}
function flowItem(id: string, name: string): RecentlyEditedArtifact {
  return { id, name, kind: "flow", modifiedAt: "2026-05-15T09:00:00.000Z" };
}
function draft(draftId: string, name: string, activityAt: string): ActiveDraftItem {
  return { draftId, name, status: "active", activityAt };
}
function run(
  runId: string,
  overrides: Partial<AgentRunItem> = {},
): AgentRunItem {
  return {
    runId,
    draftId: `${runId}-draft`,
    draftName: `${runId} draft`,
    streamId: "w1",
    streamName: "editor work",
    state: "completed",
    stage: "done",
    stageCondition: "complete",
    updatedAt: "2026-05-15T08:00:00.000Z",
    ...overrides,
  };
}

const NO_GIT: PendingGitActivity = {
  modifiedArtifacts: null,
  modifiedSourceFiles: null,
  unpushedCommits: null,
  fetchableCommits: null,
};

interface Fixture {
  recent?: RecentlyEditedArtifact[];
  drafts?: ActiveDraftItem[];
  runs?: AgentRunItem[];
  git?: PendingGitActivity;
}

/**
 * Serve the four loaders and render the Dashboard.
 *
 * Every loader answers, so a widget that is absent below is absent because
 * DSH-FR-04 hid it rather than because its call never resolved.
 */
function renderDashboard(fixture: Fixture = {}) {
  const onOpenArtifact = vi.fn<(a: OpenableArtifact) => void>();
  const onOpenRuns = vi.fn();
  const onOpenGit = vi.fn();
  const onOpenDraft = vi.fn<(id: string) => void>();
  const onOpenRun = vi.fn<(id: string) => void>();
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "list_recently_edited_artifacts":
        return fixture.recent ?? [];
      case "list_active_drafts":
        return fixture.drafts ?? [];
      case "list_recent_agent_runs":
        return fixture.runs ?? [];
      case "list_pending_git_activity":
        return fixture.git ?? NO_GIT;
      default:
        return undefined;
    }
  });
  render(
    <Dashboard
      onOpenArtifact={onOpenArtifact}
      onOpenRuns={onOpenRuns}
      onOpenGit={onOpenGit}
      onOpenDraft={onOpenDraft}
      onOpenRun={onOpenRun}
    />,
  );
  return { onOpenArtifact, onOpenRuns, onOpenGit, onOpenDraft, onOpenRun };
}

/** Which loaders have been invoked, and how often. */
function calls(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd).length;
}

/// The live indicator is a CSS-only element with no text and no role, so it is
/// queried structurally. That is deliberate: what this suite is protecting is
/// the *presence of the animated node*, not a label.
function liveIndicator(): Element | null {
  return document.querySelector(".dot--live");
}

beforeEach(() => {
  invokeMock.mockReset();
  for (const k in listeners) delete listeners[k];
});

afterEach(cleanup);

describe("Dashboard — Recently edited widget (DSH-FR-09)", () => {
  it("loads the list via list_recently_edited_artifacts and renders the names in order", async () => {
    renderDashboard({
      recent: [
        markdown(".claude/skills/design-review.md", "design-review.md"),
        flowItem("flows/code-review.flow", "code-review.flow"),
      ],
    });

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("list_recently_edited_artifacts"),
    );
    expect(await screen.findByText("Recently edited")).toBeInTheDocument();
    expect(screen.getByText("design-review.md")).toBeInTheDocument();
    expect(screen.getByText("code-review.flow")).toBeInTheDocument();
  });

  it("DSH-FR-06: clicking a markdown item routes to the Editor (no flow kind)", async () => {
    const { onOpenArtifact } = renderDashboard({
      recent: [markdown(".claude/skills/design-review.md", "design-review.md")],
    });

    await userEvent.click(await screen.findByText("design-review.md"));

    // This asserts the Dashboard's contract at the onOpenArtifact boundary:
    // markdown -> artifactType left undefined. The downstream rule that turns
    // that into an Editor tab (kind !== "flow") lives in App.openArtifact and
    // is covered by App-level routing tests, not here.
    expect(onOpenArtifact).toHaveBeenCalledWith({
      id: ".claude/skills/design-review.md",
      name: "design-review.md",
      artifactType: undefined,
    });
  });

  it("DSH-FR-06: clicking a Flow item routes to the Flow tab", async () => {
    const { onOpenArtifact } = renderDashboard({
      recent: [flowItem("flows/code-review.flow", "code-review.flow")],
    });

    await userEvent.click(await screen.findByText("code-review.flow"));

    expect(onOpenArtifact).toHaveBeenCalledWith({
      id: "flows/code-review.flow",
      name: "code-review.flow",
      artifactType: "flow",
    });
  });

  it("DSH-FR-09: an empty list keeps the widget visible with an empty state", async () => {
    // While the Dashboard has content, the Recently-edited widget stays visible
    // and renders an empty state rather than hiding (DSH-FR-09's exception to
    // DSH-FR-04). Another widget supplies that content.
    renderDashboard({ runs: [run("r1")] });

    expect(await screen.findByText("Recently edited")).toBeInTheDocument();
    expect(await screen.findByText("No recent edits yet")).toBeInTheDocument();
  });

  it("does not flash the empty state before the load resolves", async () => {
    // DSH-FR-17: a widget with no result yet shows a loading state. The empty
    // state must NOT appear during the in-flight load — only after it resolves
    // to []. A regression to an initial `[]` would render the empty state on
    // first paint and fail the pre-resolve assertion below.
    let resolveLoad!: (v: RecentlyEditedArtifact[]) => void;
    const pending = new Promise<RecentlyEditedArtifact[]>((res) => {
      resolveLoad = res;
    });
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_recently_edited_artifacts") return pending;
      if (cmd === "list_pending_git_activity") return Promise.resolve(NO_GIT);
      // Another widget with content, so the Dashboard is not empty as a whole —
      // this scenario is about the Recently edited widget's own loading state,
      // not about DSH-FR-08.
      if (cmd === "list_recent_agent_runs") return Promise.resolve([run("r1")]);
      return Promise.resolve([]);
    });
    render(
      <Dashboard
        onOpenArtifact={vi.fn()}
        onOpenRuns={vi.fn()}
        onOpenGit={vi.fn()}
      />,
    );

    expect(await screen.findByText("Recently edited")).toBeInTheDocument();
    expect(screen.queryByText("No recent edits yet")).not.toBeInTheDocument();

    resolveLoad([]);
    expect(await screen.findByText("No recent edits yet")).toBeInTheDocument();
  });

  it("DSH-FR-09: renders exactly the five the loader served, in the order it served them", async () => {
    // DSH-FR-09 / PST-FR-31: the five-item limit and the source-file ordering
    // are the loader's. This surface renders what it is given and reorders
    // nothing — a project holding twelve modified artifacts reaches it as five.
    //
    // Served deliberately UNSORTED: with an alphabetical fixture a surface that
    // sorted its rows — the exact thing "the order is the order the loader
    // returns" forbids — would pass this test.
    renderDashboard({
      recent: [
        markdown("z.md", "z.md"),
        markdown("a.md", "a.md"),
        markdown("m.md", "m.md"),
        markdown("c.md", "c.md"),
        markdown("b.md", "b.md"),
      ],
    });

    const rows = await screen.findAllByText(/^[a-z]\.md$/);
    expect(rows.map((r) => r.textContent)).toEqual([
      "z.md",
      "a.md",
      "m.md",
      "c.md",
      "b.md",
    ]);
  });
});

describe("Dashboard — Active workstreams widget (DSH-FR-11 / DSH-FR-12)", () => {
  it("DSH-FR-11: renders the drafts the loader served, under their stored names", async () => {
    // Served in the loader's prompt-activity order, which is deliberately NOT
    // alphabetical: a surface that sorted by name would otherwise pass.
    renderDashboard({
      drafts: [
        draft("d3", "zebra-migration", "2026-05-15T12:00:00.000Z"),
        draft("d1", "alpha-rewrite", "2026-05-15T11:00:00.000Z"),
        draft("d2", "onboarding", "2026-05-15T10:00:00.000Z"),
      ],
    });

    expect(await screen.findByText("Active workstreams")).toBeInTheDocument();
    const rows = await screen.findAllByText(/zebra-migration|alpha-rewrite|onboarding/);
    expect(rows.map((r) => r.textContent)).toEqual([
      "zebra-migration",
      "alpha-rewrite",
      "onboarding",
    ]);
  });

  it("DSH-FR-12: activating a row opens that draft in a New Artifact tab, by id", async () => {
    // NAW-FR-03: by **id**, so the name the widget last read cannot decide which
    // draft opens. The single-tab rule (jump focus rather than open a second)
    // belongs to the shell's `openDraft`, which this route composes.
    const { onOpenDraft, onOpenArtifact, onOpenGit, onOpenRuns } = renderDashboard({
      drafts: [draft("d3", "checkout-v2", "2026-05-15T12:00:00.000Z")],
    });

    await userEvent.click(await screen.findByText("checkout-v2"));

    expect(onOpenDraft).toHaveBeenCalledWith("d3");
    // DSH-FR-12: it opens no Project panel filter and no other surface.
    expect(onOpenArtifact).not.toHaveBeenCalled();
    expect(onOpenGit).not.toHaveBeenCalled();
    expect(onOpenRuns).not.toHaveBeenCalled();
  });

  it("DSH-FR-04: hides itself when the project has no active draft", async () => {
    renderDashboard({ recent: [markdown("a.md", "a.md")], drafts: [] });

    expect(await screen.findByText("Recently edited")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByText("Active workstreams")).not.toBeInTheDocument(),
    );
  });
});

describe("Dashboard — Last / current agent activity (DSH-FR-13 / DSH-FR-07)", () => {
  it("DSH-FR-13: lists the runs the loader served, each with its state and stage", async () => {
    renderDashboard({
      runs: [
        run("r1", { draftName: "checkout-v2", state: "queued", stage: "queued" }),
        run("r2", { draftName: "onboarding", state: "working", stage: "working" }),
      ],
    });

    expect(
      await screen.findByText("Last / current agent activity"),
    ).toBeInTheDocument();
    expect(screen.getByText("checkout-v2")).toBeInTheDocument();
    // A queued run is listed like any other, so work waiting to start is
    // visible rather than absent. Its state and its stage are both `queued`,
    // which is what makes two nodes the right count here.
    expect(screen.getAllByText("queued")).toHaveLength(2);
    // The second run's state and stage are both `working`, so two nodes carry
    // that word exactly as the queued run's two carry theirs.
    expect(screen.getAllByText("working")).toHaveLength(2);
  });

  it("DSH-FR-07: the live indicator is on the running run's row alone", async () => {
    const { onOpenRun } = renderDashboard({
      runs: [
        run("r1", { draftName: "finished-one", state: "completed" }),
        run("r2", { draftName: "running-one", state: "working" }),
      ],
    });

    await screen.findByText("running-one");
    // Exactly one, and it is the running run's — a widget-level indicator would
    // claim liveness for the finished run beside it (DSH-FR-07).
    expect(document.querySelectorAll(".dot--live")).toHaveLength(1);

    await userEvent.click(screen.getByText("running-one"));
    expect(onOpenRun).toHaveBeenCalledWith("r2");
  });

  it("shows no live indicator while nothing is in flight", async () => {
    // The headline regression this widget once carried: it hardcoded the live
    // indicator, so it animated from the moment a project opened — on the tab
    // that opens automatically (DSH-FR-01) and cannot be closed while it is the
    // only one. An infinite CSS animation on an always-mounted element holds the
    // compositor at the display's refresh rate for the life of the window.
    renderDashboard({ runs: [run("r1"), run("r2", { state: "failed" })] });

    await screen.findByText("r1 draft");
    expect(liveIndicator()).toBeNull();
  });

  it("routes a terminated run to the Runs panel too", async () => {
    const { onOpenRun } = renderDashboard({
      // Not named for a stage word: `done` is now the stage a completed run
      // stands at, and a fixture that borrowed it would match two nodes.
      runs: [run("r1", { draftName: "finished-run", state: "completed" })],
    });

    await userEvent.click(await screen.findByText("finished-run"));
    expect(onOpenRun).toHaveBeenCalledWith("r1");
  });

  it("names a run whose source draft no longer resolves by its draft id", async () => {
    // PST-FR-33: such a run is still returned, carrying its `draft_id` and no
    // name — so the widget must have something to render for it.
    renderDashboard({ runs: [run("r1", { draftName: undefined, draftId: "d-gone" })] });

    expect(await screen.findByText("d-gone")).toBeInTheDocument();
  });

  it("DSH-FR-04: hides itself when the project has no run to report", async () => {
    renderDashboard({ recent: [markdown("a.md", "a.md")], runs: [] });

    expect(await screen.findByText("Recently edited")).toBeInTheDocument();
    await waitFor(() =>
      expect(
        screen.queryByText("Last / current agent activity"),
      ).not.toBeInTheDocument(),
    );
  });
});

describe("Dashboard — Pending Git activity (DSH-FR-14)", () => {
  it("DSH-FR-14: shows the four counts, and renders an unavailable one as unavailable", async () => {
    renderDashboard({
      git: {
        modifiedArtifacts: 3,
        modifiedSourceFiles: 4,
        unpushedCommits: 2,
        fetchableCommits: 1,
      },
    });

    expect(await screen.findByText("Pending Git activity")).toBeInTheDocument();
    for (const label of [
      "modified artifacts",
      "modified source files",
      "commits to push",
      "commits to fetch",
    ]) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
    expect(screen.getByText("3")).toBeInTheDocument();
    expect(screen.getByText("4")).toBeInTheDocument();
    expect(screen.getByText("2")).toBeInTheDocument();
    expect(screen.getByText("1")).toBeInTheDocument();

    cleanup();

    // A branch with no upstream: the two commit counts are unavailable rather
    // than zero, because "level with the remote" and "never published" are the
    // opposite answers.
    renderDashboard({
      git: {
        modifiedArtifacts: 3,
        modifiedSourceFiles: 4,
        unpushedCommits: null,
        fetchableCommits: null,
      },
    });
    await screen.findByText("Pending Git activity");
    expect(screen.getAllByText("unavailable")).toHaveLength(2);
    expect(screen.queryByText("0")).not.toBeInTheDocument();
  });

  it("routes a count to the Git bottom panel", async () => {
    const { onOpenGit } = renderDashboard({
      git: { ...NO_GIT, modifiedArtifacts: 3 },
    });

    await userEvent.click(await screen.findByText("modified artifacts"));
    expect(onOpenGit).toHaveBeenCalled();
  });

  it("DSH-FR-04: hides itself when nothing is pending", async () => {
    renderDashboard({
      recent: [markdown("a.md", "a.md")],
      git: {
        modifiedArtifacts: 0,
        modifiedSourceFiles: 0,
        unpushedCommits: 0,
        fetchableCommits: 0,
      },
    });

    expect(await screen.findByText("Recently edited")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByText("Pending Git activity")).not.toBeInTheDocument(),
    );
  });
});

describe("Dashboard — empty and loading (DSH-FR-08 / DSH-FR-17)", () => {
  it("DSH-FR-08: a project with no activity shows the empty state and no widgets", async () => {
    renderDashboard();

    expect(await screen.findByText("Nothing here yet")).toBeInTheDocument();
    expect(screen.queryByText("Recently edited")).not.toBeInTheDocument();
    expect(screen.queryByText("Active workstreams")).not.toBeInTheDocument();
    expect(screen.queryByText("Pending Git activity")).not.toBeInTheDocument();
  });

  it("DSH-FR-04: exactly the widgets with content are shown", async () => {
    renderDashboard({
      recent: [markdown("a.md", "a.md")],
      git: { ...NO_GIT, modifiedArtifacts: 2 },
    });

    expect(await screen.findByText("Recently edited")).toBeInTheDocument();
    expect(await screen.findByText("Pending Git activity")).toBeInTheDocument();
    expect(screen.queryByText("Active workstreams")).not.toBeInTheDocument();
    expect(
      screen.queryByText("Last / current agent activity"),
    ).not.toBeInTheDocument();
  });

  it("DSH-FR-17: a slow loader leaves its own widget loading while the others render", async () => {
    // DSH-FR-17: no widget's result is waited for, and widgets render as their
    // results arrive in whatever order they arrive.
    let resolveGit!: (v: PendingGitActivity) => void;
    const slowGit = new Promise<PendingGitActivity>((res) => {
      resolveGit = res;
    });
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_pending_git_activity") return slowGit;
      if (cmd === "list_recently_edited_artifacts")
        return Promise.resolve([markdown("a.md", "a.md")]);
      return Promise.resolve([]);
    });
    render(
      <Dashboard
        onOpenArtifact={vi.fn()}
        onOpenRuns={vi.fn()}
        onOpenGit={vi.fn()}
      />,
    );

    // The other widget has its data while the slow one is still outstanding.
    expect(await screen.findByText("a.md")).toBeInTheDocument();
    expect(screen.getByText("Pending Git activity")).toBeInTheDocument();
    expect(screen.getByText("Loading…")).toBeInTheDocument();

    resolveGit({ ...NO_GIT, modifiedArtifacts: 1 });
    expect(await screen.findByText("modified artifacts")).toBeInTheDocument();
  });
});

describe("Dashboard — refresh events (DSH-FR-15 / DSH-FR-16)", () => {
  it("DSH-FR-15: a 5-minute tick re-invokes only the two widgets it names", async () => {
    renderDashboard({
      recent: [markdown("a.md", "a.md")],
      runs: [run("r1")],
      git: { ...NO_GIT, modifiedArtifacts: 1 },
    });
    await screen.findByText("a.md");

    const before = {
      recent: calls("list_recently_edited_artifacts"),
      drafts: calls("list_active_drafts"),
      runs: calls("list_recent_agent_runs"),
      git: calls("list_pending_git_activity"),
    };

    emitEvent(AGENT_ACTIVITY);
    emitEvent(PENDING_GIT);

    await waitFor(() =>
      expect(calls("list_recent_agent_runs")).toBe(before.runs + 1),
    );
    expect(calls("list_pending_git_activity")).toBe(before.git + 1);
    // The two filesystem-driven widgets are not re-invoked by that tick: this
    // surface starts no timer and polls nothing.
    expect(calls("list_recently_edited_artifacts")).toBe(before.recent);
    expect(calls("list_active_drafts")).toBe(before.drafts);
  });

  it("DSH-FR-16: an external artifact write reaches the widget without a manual refresh", async () => {
    let served = [markdown("a.md", "a.md")];
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_recently_edited_artifacts") return served;
      if (cmd === "list_pending_git_activity") return NO_GIT;
      return [];
    });
    render(
      <Dashboard
        onOpenArtifact={vi.fn()}
        onOpenRuns={vi.fn()}
        onOpenGit={vi.fn()}
      />,
    );
    await screen.findByText("a.md");

    // An external editor wrote `b.md`, so it now leads the loader's order.
    served = [markdown("b.md", "b.md"), markdown("a.md", "a.md")];
    emitEvent(RECENTLY_EDITED);

    expect(await screen.findByText("b.md")).toBeInTheDocument();
  });

  it("DSH-FR-16, DSH-FR-11: an external prompt write reorders the Active workstreams widget", async () => {
    let served = [
      draft("d1", "onboarding", "2026-05-15T11:00:00.000Z"),
      draft("d2", "checkout-v2", "2026-05-15T10:00:00.000Z"),
    ];
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_active_drafts") return served;
      if (cmd === "list_pending_git_activity") return NO_GIT;
      return [];
    });
    render(
      <Dashboard
        onOpenArtifact={vi.fn()}
        onOpenRuns={vi.fn()}
        onOpenGit={vi.fn()}
      />,
    );
    await screen.findByText("onboarding");

    served = [
      draft("d2", "checkout-v2", "2026-05-15T12:00:00.000Z"),
      draft("d1", "onboarding", "2026-05-15T11:00:00.000Z"),
    ];
    emitEvent(ACTIVE_DRAFTS);

    await waitFor(() => {
      const rows = screen.getAllByText(/checkout-v2|onboarding/);
      expect(rows.map((r) => r.textContent)).toEqual([
        "checkout-v2",
        "onboarding",
      ]);
    });
    // The row's name and status are the record's, and neither moved.
    expect(screen.getAllByText("active")).toHaveLength(2);
  });
});

describe("Dashboard — error and stale states (DSH-FR-18 / DSH-FR-19)", () => {
  it("DSH-FR-18: a failing loader shows an error state and hides nothing else", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_pending_git_activity") throw new Error("git is unwell");
      if (cmd === "list_recently_edited_artifacts")
        return [markdown("a.md", "a.md")];
      return [];
    });
    render(
      <Dashboard
        onOpenArtifact={vi.fn()}
        onOpenRuns={vi.fn()}
        onOpenGit={vi.fn()}
      />,
    );

    expect(
      await screen.findByText("Pending Git activity could not be read"),
    ).toBeInTheDocument();
    // An error is content: the widget is not hidden by DSH-FR-04, and the rest
    // of the grid renders.
    expect(screen.getByText("Pending Git activity")).toBeInTheDocument();
    expect(screen.getByText("a.md")).toBeInTheDocument();
  });

  it("DSH-FR-18: a widget refreshing keeps its rows, marked stale, rather than emptying", async () => {
    let resolveSecond!: (v: ActiveDraftItem[]) => void;
    let first = true;
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_active_drafts") {
        if (first) {
          first = false;
          return Promise.resolve([
            draft("d1", "checkout-v2", "2026-05-15T11:00:00.000Z"),
          ]);
        }
        return new Promise<ActiveDraftItem[]>((res) => {
          resolveSecond = res;
        });
      }
      if (cmd === "list_pending_git_activity") return Promise.resolve(NO_GIT);
      return Promise.resolve([]);
    });
    render(
      <Dashboard
        onOpenArtifact={vi.fn()}
        onOpenRuns={vi.fn()}
        onOpenGit={vi.fn()}
      />,
    );
    await screen.findByText("checkout-v2");

    emitEvent(ACTIVE_DRAFTS);

    // The rows the author is reading are still there, marked stale — neither a
    // refresh nor an error blanks a widget.
    expect(await screen.findByText("Refreshing…")).toBeInTheDocument();
    expect(screen.getByText("checkout-v2")).toBeInTheDocument();

    resolveSecond([draft("d1", "checkout-v3", "2026-05-15T12:00:00.000Z")]);
    expect(await screen.findByText("checkout-v3")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.queryByText("Refreshing…")).not.toBeInTheDocument(),
    );
  });

  it("DSH-FR-18: a background failure reaches the widget, and its next success clears it", async () => {
    renderDashboard({
      recent: [markdown("a.md", "a.md")],
      git: { ...NO_GIT, modifiedArtifacts: 1 },
    });
    await screen.findByText("modified artifacts");

    // A refresh the author never asked for failed. It names one widget and
    // touches no other.
    emitEvent(REFRESH_FAILED, { widget: "pending_git", error: "git is unwell" });

    expect(
      await screen.findByText("Pending Git activity could not be read"),
    ).toBeInTheDocument();
    expect(screen.getByText("a.md")).toBeInTheDocument();

    // A later tick refreshes it successfully, which is what clears the error.
    emitEvent(PENDING_GIT);
    expect(await screen.findByText("modified artifacts")).toBeInTheDocument();
    expect(
      screen.queryByText("Pending Git activity could not be read"),
    ).not.toBeInTheDocument();
  });

  it("DSH-FR-19: an earlier invocation returning last is discarded", async () => {
    const settle: ((v: RecentlyEditedArtifact[]) => void)[] = [];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_recently_edited_artifacts")
        return new Promise<RecentlyEditedArtifact[]>((res) => settle.push(res));
      if (cmd === "list_pending_git_activity") return Promise.resolve(NO_GIT);
      return Promise.resolve([]);
    });
    render(
      <Dashboard
        onOpenArtifact={vi.fn()}
        onOpenRuns={vi.fn()}
        onOpenGit={vi.fn()}
      />,
    );
    await waitFor(() => expect(settle).toHaveLength(1));

    // A second invocation while the first is still outstanding.
    emitEvent(RECENTLY_EDITED);
    await waitFor(() => expect(settle).toHaveLength(2));

    // The later one returns first, then the earlier one.
    settle[1]([markdown("newer.md", "newer.md")]);
    expect(await screen.findByText("newer.md")).toBeInTheDocument();
    settle[0]([markdown("older.md", "older.md")]);

    await waitFor(() =>
      expect(screen.queryByText("older.md")).not.toBeInTheDocument(),
    );
    expect(screen.getByText("newer.md")).toBeInTheDocument();
  });

  it("DSH-FR-19: a result outstanding across a worktree change is discarded", async () => {
    const settle: ((v: RecentlyEditedArtifact[]) => void)[] = [];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_recently_edited_artifacts")
        return new Promise<RecentlyEditedArtifact[]>((res) => settle.push(res));
      if (cmd === "list_pending_git_activity") return Promise.resolve(NO_GIT);
      return Promise.resolve([]);
    });
    render(
      <Dashboard
        onOpenArtifact={vi.fn()}
        onOpenRuns={vi.fn()}
        onOpenGit={vi.fn()}
      />,
    );
    await waitFor(() => expect(settle).toHaveLength(1));

    emitEvent(WORKTREE_CHANGED, { activeWorktreePath: "~/dev/acme-main" });
    await waitFor(() => expect(settle).toHaveLength(2));

    // The INCOMING worktree's call settles first, and the outgoing root's call
    // returns after it. That order is what makes this test able to fail:
    // resolving the outgoing one first would let the incoming result overwrite
    // it and the assertion would hold with the sequence guard deleted.
    settle[1]([markdown("incoming.md", "incoming.md")]);
    expect(await screen.findByText("incoming.md")).toBeInTheDocument();

    settle[0]([markdown("outgoing.md", "outgoing.md")]);
    await waitFor(() =>
      expect(screen.queryByText("outgoing.md")).not.toBeInTheDocument(),
    );
    expect(screen.getByText("incoming.md")).toBeInTheDocument();
  });
});

describe("Dashboard — presentation rules", () => {
  it("tells a run that is working from one that ended well and one that did not", () => {
    // DSH-FR-13: collapsing the last two onto one token would report a failure
    // in the same colour as a clean finish.
    expect(runTone("working")).toBe("live");
    expect(runTone("completed")).toBe("ok");
    expect(runTone("failed")).toBe("danger");
    expect(runTone("discarded")).toBe("warn");
    expect(runTone("queued")).toBe("accent");
  });

  it("names the four Pending Git counts in the order DSH-FR-14 names them", () => {
    const rows = pendingGitRows({
      modifiedArtifacts: 1,
      modifiedSourceFiles: 2,
      unpushedCommits: 3,
      fetchableCommits: 4,
    });
    expect(rows.map((r) => r.value)).toEqual([1, 2, 3, 4]);
    expect(rows.map((r) => r.label)).toEqual([
      "modified artifacts",
      "modified source files",
      "commits to push",
      "commits to fetch",
    ]);
  });

  it("treats an unavailable count and a zero as no content", () => {
    // DSH-FR-04: a project with nothing pending has nothing to say, and four
    // zeroes said out loud is noise on the one surface meant to be read at a
    // glance.
    expect(hasPendingGitContent(null)).toBe(false);
    expect(hasPendingGitContent(NO_GIT)).toBe(false);
    expect(
      hasPendingGitContent({
        modifiedArtifacts: 0,
        modifiedSourceFiles: 0,
        unpushedCommits: 0,
        fetchableCommits: 0,
      }),
    ).toBe(false);
    expect(hasPendingGitContent({ ...NO_GIT, fetchableCommits: 1 })).toBe(true);
  });
});

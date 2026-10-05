import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import { BottomPanel } from "./BottomPanel";
import { COALESCE_MS } from "../state/graduation";

// The History surface scopes to the active artifact entity (distinct from the
// Notes panel's override), and SNV-FR-47: the panel names where it is and
// offers no way to go anywhere else.

// The Git surface subscribes to four backend event streams and reads through
// `invoke` on mount. Rendering it unmocked leaves those promises to reject
// after the test has finished, which Vitest flags as a false-positive risk.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(async () => () => {}) }));

afterEach(cleanup);

describe("BottomPanel History scoping", () => {
  it("labels the History surface with the active entity when one is set", () => {
    render(
      <BottomPanel
        surface="history"
        onHide={() => {}}
        activeEntity="design-review"
        onSwitchWorktree={async () => ({ ok: true })}
        canCheckOutBranches
      />,
    );
    // SNV-FR-57: "HISTORY" is a label and keeps its eyebrow casing; the entity
    // beside it is a file name, rendered in its own element so no display
    // treatment case-transforms it.
    const header = screen.getByText(/HISTORY —/);
    expect(header).toHaveTextContent("HISTORY — design-review");
    // The entity sits in its own element carrying the modifier that turns the
    // eyebrow's uppercasing off — the same mechanism the Diff tab header and
    // the Notes group headers use, rather than an inline style here.
    const entity = screen.getByText("design-review");
    expect(entity).toHaveClass("t-eyebrow__literal");
  });

  it("falls back to 'no entity scope' when no entity is active", () => {
    render(
      <BottomPanel
        surface="history"
        onHide={() => {}}
        activeEntity={null}
        onSwitchWorktree={async () => ({ ok: true })}
        canCheckOutBranches
      />,
    );
    expect(screen.getByText("HISTORY — no entity scope")).toBeInTheDocument();
  });

  // SNV-FR-47: the panel names where it is and offers no way to go
  // anywhere else. The activity bar's bottom cluster is the only switcher.
  it("presents no surface switcher — only a title and the hide control", async () => {
    const onHide = vi.fn();
    render(
      <BottomPanel
        surface="history"
        onHide={onHide}
        activeEntity={null}
        onSwitchWorktree={async () => ({ ok: true })}
        canCheckOutBranches
      />,
    );

    expect(screen.getByTestId("bottom-panel-title")).toHaveTextContent(
      "History",
    );
    // No Runs or Git affordance anywhere in the panel to switch to.
    expect(screen.queryByText("Runs")).toBeNull();
    expect(screen.queryByText("Git")).toBeNull();
    // The hide control is the panel's only navigation affordance.
    const buttons = screen.getAllByRole("button");
    expect(buttons).toHaveLength(1);

    await userEvent.click(screen.getByRole("button", { name: "Hide" }));
    expect(onHide).toHaveBeenCalledOnce();
  });

  it("titles itself from the surface it renders", () => {
    for (const [surface, label] of [
      ["git", "Git"],
      ["history", "History"],
    ] as const) {
      cleanup();
      render(
        <BottomPanel
          surface={surface}
          onHide={() => {}}
          activeEntity={null}
          onSwitchWorktree={async () => ({ ok: true })}
          canCheckOutBranches
        />,
      );
      expect(screen.getByTestId("bottom-panel-title")).toHaveTextContent(label);
    }
  });
});

describe("BottomPanel Runs surface (RUN-FR-04, RUN-FR-06 / SNV-FR-47)", () => {
  const runsProps = {
    surface: "runs" as const,
    onHide: () => {},
    activeEntity: null,
    onSwitchWorktree: async () => ({ ok: true }) as const,
    canCheckOutBranches: true,
  };
  const renderRuns = () => render(<BottomPanel {...runsProps} />);

  it("SNV-FR-47: the title names the surface and carries no run-state dot", () => {
    // The chrome used to carry a live indicator gated on `surface === "runs"`
    // rather than on a run being live, so it animated — infinitely — for as
    // long as the panel stayed open on an otherwise idle window. The header
    // names where the panel is; run state belongs to the surface.
    renderRuns();
    const title = screen.getByTestId("bottom-panel-title");
    expect(title).toHaveTextContent("Runs");
    expect(title.querySelector(".dot")).toBeNull();
  });

  // RUN-FR-05, RUN-FR-10 / RUN-FR-09: nothing in the window starts a run, so a project in
  // which none has started finds the panel at its empty state — no output, no
  // live indicator, and no affordance to begin one.
  it("RUN-FR-02, RUN-FR-10: opens on the graduation section, which starts nothing either", () => {
    // RUN-FR-10: the panel hosts two sections and chooses between them itself.
    // Graduation leads, because it is the section carrying decisions the author
    // is being waited on for (GRU-FR-MYFA).
    renderRuns();
    const control = screen.getByRole("tablist", { name: "Runs sections" });
    expect(
      within(control).getByRole("tab", { name: "Graduation" }),
    ).toHaveAttribute("aria-selected", "true");
    expect(
      within(control).getByRole("tab", { name: "Agent output" }),
    ).toHaveAttribute("aria-selected", "false");
    // GRU-FR-HKBD / RUN-FR-09: an empty queue says so and offers no control to
    // begin one.
    expect(screen.getByText("Nothing graduating")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Graduate$/ })).toBeNull();
  });

  it("RUN-FR-02, RUN-FR-10: a route to a run selects the graduation section once, not for good", async () => {
    // RUN-FR-10: routing to a run is a *request*, taken once. Deriving what is
    // showing from the request instead would leave the author unable to choose
    // the other section again for as long as it stood — which one click on a
    // draft's graduation marker would do for the rest of the session.
    const { rerender } = render(
      <BottomPanel
        {...runsProps}
        selectGraduationRun={{ runId: "run-1", nonce: 1 }}
      />,
    );
    expect(
      await screen.findByRole("tab", { name: "Graduation" }),
    ).toHaveAttribute("aria-selected", "true");

    await userEvent.click(screen.getByRole("tab", { name: "Agent output" }));
    expect(screen.getByRole("tab", { name: "Agent output" })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    // A re-render with the same standing request must not drag it back.
    rerender(
      <BottomPanel
        {...runsProps}
        selectGraduationRun={{ runId: "run-1", nonce: 1 }}
      />,
    );
    expect(screen.getByRole("tab", { name: "Agent output" })).toHaveAttribute(
      "aria-selected",
      "true",
    );

    // A *new* request routes again, which is what a second click means.
    rerender(
      <BottomPanel
        {...runsProps}
        selectGraduationRun={{ runId: "run-1", nonce: 2 }}
      />,
    );
    await waitFor(() =>
      expect(screen.getByRole("tab", { name: "Graduation" })).toHaveAttribute(
        "aria-selected",
        "true",
      ),
    );
  });

  it("GRU-FR-MYFA: reports whether the graduation section is the one showing", async () => {
    // The shell needs this at the instant a graduation is started, to decide
    // whether the run that starts may take the author's selection. The panel is
    // what knows it: a run is showing only while Runs is the active surface and
    // the section control has chosen graduation.
    const reported: boolean[] = [];
    // `.at` is outside this project's target lib, so the last entry is read by
    // index rather than by it.
    const latest = () => reported[reported.length - 1];
    const { rerender, unmount } = render(
      <BottomPanel
        {...runsProps}
        onGraduationSectionActive={(active) => reported.push(active)}
      />,
    );
    await screen.findByRole("tab", { name: "Graduation" });
    expect(latest()).toBe(true);

    // Moving the section control off graduation says so at once.
    await userEvent.click(screen.getByRole("tab", { name: "Agent output" }));
    expect(latest()).toBe(false);
    await userEvent.click(screen.getByRole("tab", { name: "Graduation" }));
    expect(latest()).toBe(true);

    // So does the panel showing another surface altogether.
    rerender(
      <BottomPanel
        {...runsProps}
        surface="logs"
        onGraduationSectionActive={(active) => reported.push(active)}
      />,
    );
    await waitFor(() => expect(latest()).toBe(false));

    // And a panel that is unmounted is showing nothing, rather than leaving
    // the shell holding the last answer it was given.
    rerender(
      <BottomPanel
        {...runsProps}
        onGraduationSectionActive={(active) => reported.push(active)}
      />,
    );
    await waitFor(() => expect(latest()).toBe(true));
    unmount();
    expect(latest()).toBe(false);
  });

  it("RUN-FR-09, RUN-FR-05, RUN-FR-10: the agent-output section opens at its empty state, with nothing that starts a run", async () => {
    renderRuns();
    await userEvent.click(screen.getByRole("tab", { name: "Agent output" }));
    expect(screen.getByText("No run")).toBeInTheDocument();
    expect(screen.queryByText("running")).toBeNull();
    expect(document.querySelectorAll(".dot--live")).toHaveLength(0);
    expect(document.querySelectorAll(".runs-line")).toHaveLength(0);
    // RUN-FR-05: Cancel and Stop act on a run in flight and are absent here.
    expect(screen.queryByRole("button", { name: /Stop/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Cancel/ })).toBeNull();
    // And no control begins one.
    expect(screen.queryByRole("button", { name: /Re-run/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Run/ })).toBeNull();
  });

  // Scoped to this describe rather than wrapped in a try/finally inside the one
  // test that needs them: the sibling describes drive `userEvent`, which hangs
  // under fake timers, and a leak into `afterEach(cleanup)` would surface as a
  // hung suite rather than a clear failure. Matches Runs.test.tsx's pattern.
  describe("timers", () => {
    beforeEach(() => vi.useFakeTimers());
    afterEach(() => vi.useRealTimers());

    it("arms no timer by being opened", () => {
      // The idle-CPU property, expressed as the one thing jsdom can observe
      // directly. Mounting the panel live restarted the canned stream's 600ms
      // interval on every open. Asserted on the timer count rather than on the
      // rendered line count, so a timer that ticks without appending anything —
      // a poll, say — is caught too.
      renderRuns();
      expect(vi.getTimerCount()).toBe(0);
    });
  });
});

// LOG-logs.md LOG-FR-11 / LOG-FR-21. Both are claims about the *seam* rather
// than about the Logs surface itself, so neither is observable from
// `Logs.test.tsx`: one is about what happens while Logs is NOT the active
// surface, the other about the two surfaces staying separate.
describe("the Logs surface at the bottom panel's seam", () => {
  const props = {
    onHide: () => {},
    activeEntity: null,
    onSwitchWorktree: async () => ({ ok: true }) as const,
    canCheckOutBranches: true,
  };

  /**
   * Long enough for the panel's own debounce and its `setTimeout(…, 0)` initial
   * load to have fired. Awaiting a bare microtask would pass whether or not
   * `<Logs />` were mounted, since the first query is behind a macrotask — the
   * assertion has to outlast the thing it claims does not happen.
   */
  const settle = () => new Promise((r) => setTimeout(r, 60));

  it("LOG-FR-11: issues no query while another surface is showing (LOG-FR-12)", async () => {
    const invoked = vi.mocked(invoke);
    // Runs, Git, and History in turn — none of them may wake the log channel.
    for (const surface of ["runs", "git", "history"] as const) {
      cleanup();
      invoked.mockClear();
      render(<BottomPanel {...props} surface={surface} />);
      await settle();
      expect(
        invoked.mock.calls.some(([name]) => name === "query_logs"),
        `${surface} must not query the log buffer`,
      ).toBe(false);
    }

    // The positive control, under the identical wait: without it the assertion
    // above proves only that the wait was too short.
    cleanup();
    invoked.mockClear();
    render(<BottomPanel {...props} surface="logs" />);
    await settle();
    const queries = invoked.mock.calls.filter(([name]) => name === "query_logs");
    expect(queries.length).toBeGreaterThan(0);
    // LOG-FR-12: becoming visible takes ONE page, not a burst of redundant ones.
    expect(queries).toHaveLength(1);
  });

  it("LOG-FR-11 / LOG-FR-12: queries as soon as Logs becomes the active surface", async () => {
    const invoked = vi.mocked(invoke);
    invoked.mockClear();
    render(<BottomPanel {...props} surface="logs" />);
    await waitFor(() =>
      expect(invoked.mock.calls.some(([name]) => name === "query_logs")).toBe(true),
    );
    // The header names where the panel is (SNV-FR-47), and there is still no
    // switcher inside it.
    expect(screen.getByTestId("bottom-panel-title")).toHaveTextContent("Logs");
  });

  it("LOG-FR-21: renders no run output, and leaves the Runs stream alone (RUN-FR-03)", async () => {
    render(<BottomPanel {...props} surface="logs" />);
    await waitFor(() => expect(document.querySelector(".logs")).toBeTruthy());
    // A run's stream belongs to Runs; opening Logs neither replaces nor
    // duplicates it.
    expect(document.querySelectorAll(".runs-line")).toHaveLength(0);
    expect(screen.queryByRole("button", { name: /Re-run/ })).toBeNull();

    cleanup();
    render(<BottomPanel {...props} surface="runs" />);
    // Runs is the surface it always was — at its empty state, since nothing in
    // the window started a run (RUN-FR-09) — and Logs is gone. It opens on its
    // graduation section (RUN-FR-10), which is likewise empty.
    expect(await screen.findByText("Nothing graduating")).toBeInTheDocument();
    expect(document.querySelector(".logs")).toBeNull();
  });
});

describe("the graduation section's count (RUN-FR-10)", () => {
  const runsProps = {
    surface: "runs" as const,
    onHide: () => {},
    activeEntity: null,
    onSwitchWorktree: async () => ({ ok: true }) as const,
    canCheckOutBranches: true,
  };

  /** One run of the project's queue, in whichever queue it is assigned to. */
  function run(
    id: string,
    state: string,
    queue: "graduation" | "implementation" | null,
  ) {
    return {
      id,
      projectKey: "/dev/acme",
      draftId: `draft-${id}`,
      mode: "git",
      state,
      queue,
      input: {
        draftId: `draft-${id}`,
        draftName: id,
        prompt: "p",
        promptChecksum: "sha",
        capturedAt: "2026-08-01T00:00:00Z",
      },
      source: {
        kind: "git",
        sourceWorktreePath: "~/dev/acme",
        sourceBranch: "main",
        sourceRevision: "abc",
        graduationBranch: `synthesis/graduation/${id}`,
        graduationWorktreePath: `/store/${id}/worktree`,
      },
      iteration: 1,
      enqueuedAt: "2026-08-01T00:00:00Z",
      updatedAt: "2026-08-01T00:00:00Z",
    };
  }

  it("RUN-FR-10: the label counts the runs the project's streams hold between them", async () => {
    vi.mocked(invoke).mockImplementation(async (cmd: string) => {
      if (cmd === "list_graduation_queue")
        return {
          projectKey: "/dev/acme",
          runs: [
            run("a", "queued", "graduation"),
            // A run of another stream. Both are the project's work in flight
            // and both are counted, once each.
            run("b", "working", "graduation"),
            // Terminal, and counted in neither.
            run("c", "completed", null),
            run("d", "discarded", null),
          ],
        };
      return undefined;
    });
    render(<BottomPanel {...runsProps} />);

    const control = await screen.findByRole("tablist", {
      name: "Runs sections",
    });
    await waitFor(() =>
      expect(
        within(control).getByRole("tab", { name: /Graduation/ }).textContent,
      ).toBe("Graduation · 2"),
    );
  });
});

describe("the graduation section's reloads (GRD-FR-EFAU)", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockClear();
    vi.mocked(listen).mockClear();
  });
  afterEach(() => vi.useRealTimers());

  /** Every callback the panel registered for one backend event. */
  const listenersFor = (event: string): Array<(payload: unknown) => void> =>
    vi
      .mocked(listen)
      .mock.calls.filter(([name]) => name === event)
      .map(([, handler]) => handler as (payload: unknown) => void);

  const queueReads = () =>
    vi.mocked(invoke).mock.calls.filter(([name]) => name === "list_graduation_queue")
      .length;

  it("a burst of run events reads the queue once, not once each", async () => {
    render(
      <BottomPanel
        surface="runs"
        onHide={() => {}}
        activeEntity={null}
        onSwitchWorktree={async () => ({ ok: true })}
        canCheckOutBranches
      />,
    );
    // The mount read, which the coalescer has nothing to do with.
    await waitFor(() => expect(queueReads()).toBeGreaterThan(0));
    const handlers = listenersFor("graduation-run-changed");
    expect(handlers.length).toBeGreaterThan(0);

    vi.useFakeTimers();
    const before = queueReads();
    for (let i = 0; i < 5; i += 1) handlers[0]({ payload: {} });
    expect(queueReads()).toBe(before);

    vi.advanceTimersByTime(COALESCE_MS);
    await vi.waitFor(() => expect(queueReads()).toBe(before + 1));
  });

  it("an event arriving after the panel goes reads nothing", async () => {
    const { unmount } = render(
      <BottomPanel
        surface="runs"
        onHide={() => {}}
        activeEntity={null}
        onSwitchWorktree={async () => ({ ok: true })}
        canCheckOutBranches
      />,
    );
    await waitFor(() => expect(queueReads()).toBeGreaterThan(0));
    const handlers = listenersFor("graduation-queue-changed");
    expect(handlers.length).toBeGreaterThan(0);

    vi.useFakeTimers();
    unmount();
    const after = queueReads();
    handlers[0]({ payload: {} });
    vi.advanceTimersByTime(COALESCE_MS * 5);
    expect(queueReads()).toBe(after);
  });
});

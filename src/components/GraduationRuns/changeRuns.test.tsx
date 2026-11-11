/**
 * Apply runs and rework runs in the Runs panel
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-LTWN,
 * GRU-FR-YHFQ, GRU-FR-WDWB, GRU-FR-GLSO, GRU-FR-SOFY, GRU-FR-PXCW,
 * GRU-FR-HZLV, GRU-FR-HABG, GRU-FR-IKEQ, GRU-FR-NGQE;
 * `GRH-graduation-history.md` GRH-FR-FYTH; `GRT-graduation-restart.md`
 * GRT-FR-GLLN; `RUN-runs.md` RUN-FR-09).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../../logging";
import { configureOpenSpecChangeRoutes } from "../../state/openspec/changeRoutes";
import { makeObservability, makeQueue, makeRun, type RunOverrides } from "../../test/graduationFixtures";
import type { ApplyProgress, GraduationRun, GraduationRunState } from "../../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (e: { payload: unknown }) => void) => {
    const set = listeners.get(event) ?? new Set();
    set.add(cb);
    listeners.set(event, set);
    return () => set.delete(cb);
  }),
  emit: () => Promise.resolve(),
}));

import { RunActionRow } from "./actionRow";
import { GraduationRail, type GraduationRailProps } from "./rail";
import { RunRegion } from "./region";
import { RestartRegionControl } from "./restart";

const CHANGE = "add-logout-3f2a";
const open = vi.fn();
let progress: ApplyProgress | { reject: string };

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function emit(event: string, payload: unknown) {
  for (const cb of listeners.get(event) ?? []) cb({ payload });
}

beforeEach(() => {
  resetLogBufferForTest();
  listeners.clear();
  open.mockReset();
  configureOpenSpecChangeRoutes({ open });
  progress = { state: "ready", total: 4, complete: 2, nextTask: "Add the logout route", missingArtifacts: [] };
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "read_apply_progress") {
      if ("reject" in progress) throw progress.reject;
      return progress;
    }
    if (cmd === "restart_graduation_run") return makeRun("r9", "queued");
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
});
afterEach(() => {
  configureOpenSpecChangeRoutes(null);
  cleanup();
});

/** An apply run or a rework run of the change, with no draft. */
function changeRun(
  phase: "apply" | "rework",
  state: GraduationRunState,
  over: RunOverrides = {},
): GraduationRun {
  return makeRun("r1", state, {
    streamId: "s-7",
    streamName: "auth-work",
    input: {
      draftId: "",
      draftName: "",
      prompt: `Apply the OpenSpec change "${CHANGE}": implement its tasks.`,
      promptChecksum: "sha",
      capturedAt: "2026-09-06T09:00:00Z",
    },
    openspec:
      phase === "apply"
        ? { phase, change: CHANGE }
        : {
            phase,
            change: CHANGE,
            instruction: "Split <b>task 2</b> in two.",
            threadId: "t-req",
            requester: { kind: "agent", agentId: "synthesis", handle: "Synthesis", title: "" },
          },
    ...over,
  });
}

const act$ = vi.fn(async () => true);
const pressedRef = { current: null };

function region(run: GraduationRun) {
  return render(
    <RunRegion
      run={run}
      runs={[run]}
      queue={makeQueue([run])}
      capacity={null}
      busy={false}
      act={act$}
      onActivateStage={() => {}}
      onAnswered={() => {}}
      onRestarted={() => {}}
      refusal={null}
      pressedRef={pressedRef}
      openRestart={false}
      onRestartOpened={() => {}}
    />,
  );
}

function row(run: GraduationRun) {
  render(
    <RunActionRow
      run={run}
      busy={false}
      act={act$}
      onRestarted={() => {}}
      refusal={null}
      pressedRef={pressedRef}
      openRestart={false}
      onRestartOpened={() => {}}
    />,
  );
}

const labels = () =>
  [...document.querySelectorAll(".run-progress__label")].map((el) => el.textContent);

const ALL_STATES: GraduationRunState[] = [
  "queued",
  "working",
  "reviewing",
  "awaiting_author",
  "blocked",
  "interrupted",
  "completed",
  "failed",
  "discarded",
];

describe("the stage row of an OpenSpec change run", () => {
  it("GRU-FR-SOFY, GOB-FR-CAXV: an apply run's stages are Queued, Tasks, Verify, and Landed", () => {
    region(changeRun("apply", "reviewing", { observability: makeObservability({ currentStage: "review" }) }));
    expect(labels()).toEqual(["Queued", "Tasks", "Verify", "Landed"]);
  });

  it("GRU-FR-SOFY, GOB-FR-CAXV: a rework run's stages are Queued, Reworking, Reviewing, and Proposed", () => {
    region(changeRun("rework", "working"));
    expect(labels()).toEqual(["Queued", "Reworking", "Reviewing", "Proposed"]);
  });

  it("GOB-FR-CAXV: a stopped done stage reads Not landed for an apply run and Not proposed for a rework run", () => {
    const stopped = makeObservability({ currentStage: "done", stageCondition: "stopped" });
    region(changeRun("apply", "discarded", { observability: stopped }));
    expect(labels()).toEqual(["Queued", "Tasks", "Verify", "Not landed"]);
    cleanup();
    region(changeRun("rework", "discarded", { observability: stopped }));
    expect(labels()).toEqual(["Queued", "Reworking", "Reviewing", "Not proposed"]);
  });

  it("GOB-FR-CAXV: a working rework run's sentence says Reworking with its pass, never Working", async () => {
    region(changeRun("rework", "working"));
    const row = screen.getByTestId("run-progress");
    expect(row).toHaveTextContent(/Reworking · pass 1 of/);
    expect(calls("read_apply_progress")).toHaveLength(0);
  });

  it("GRU-FR-PXCW: the Tasks stage of a working apply run states the task counts and the next open task", async () => {
    region(changeRun("apply", "working"));
    await waitFor(() =>
      expect(screen.getByTestId("run-progress")).toHaveTextContent("2 of 4 tasks · next: Add the logout route"),
    );
    expect(calls("read_apply_progress")[0][1]).toEqual({ streamId: "s-7", change: CHANGE });
  });

  it("GRU-FR-PXCW: it reads the progress again on each change of the run, and not for another run", async () => {
    region(changeRun("apply", "working"));
    await waitFor(() => expect(calls("read_apply_progress")).toHaveLength(1));
    progress = { ...(progress as ApplyProgress), complete: 3, nextTask: "Test the route" };
    act(() => emit("graduation-run-changed", { runId: "other", state: "working" }));
    act(() => emit("graduation-run-changed", { runId: "r1", state: "working" }));
    await waitFor(() =>
      expect(screen.getByTestId("run-progress")).toHaveTextContent("3 of 4 tasks · next: Test the route"),
    );
    expect(calls("read_apply_progress")).toHaveLength(2);
  });

  it("GRU-FR-PXCW: a read that fails after a read that succeeded puts the stage back on its ordinary sentence", async () => {
    region(changeRun("apply", "working"));
    await waitFor(() =>
      expect(screen.getByTestId("run-progress")).toHaveTextContent("2 of 4 tasks · next: Add the logout route"),
    );
    progress = { reject: "cli_failed" };
    act(() => emit("graduation-run-changed", { runId: "r1", state: "working" }));
    await waitFor(() => expect(screen.getByTestId("run-progress")).not.toHaveTextContent("of 4 tasks"));
    expect(calls("read_apply_progress")).toHaveLength(2);
    expect(screen.getByTestId("run-progress")).toHaveTextContent(/Tasks · pass 1 of/);
  });

  it("GRU-FR-HZLV: the detail slot states checks running while a work turn is in flight", () => {
    region(changeRun("apply", "working"));
    expect(screen.getByTestId("graduation-check-state")).toHaveTextContent("checks running");
  });

  it("GRU-FR-HZLV: the detail slot states correcting checks while the checkpoint holds a check gate", () => {
    region(changeRun("apply", "working", { checkpoint: { checkGate: { cause: "red", next: "correct" } } }));
    expect(screen.getByTestId("graduation-check-state")).toHaveTextContent("correcting checks");
  });

  it("GRU-FR-HZLV: the detail slot states checks red while the run rests on checks_not_passed", () => {
    region(
      changeRun("apply", "awaiting_author", {
        rest: { reason: "checks_not_passed", cause: "checks_failed", checks: [] } as never,
      }),
    );
    expect(screen.getByTestId("graduation-check-state")).toHaveTextContent("checks red");
  });

  it.each<GraduationRunState>(["queued", "reviewing", "completed", "blocked"])(
    "GRU-FR-HZLV: an apply run in %s states no check state",
    (state) => {
      region(changeRun("apply", state));
      expect(screen.queryByTestId("graduation-check-state")).toBeNull();
    },
  );

  it("GRU-FR-HZLV: a rework run states no check state", () => {
    region(changeRun("rework", "working"));
    expect(screen.queryByTestId("graduation-check-state")).toBeNull();
  });
});

describe("the region of an OpenSpec change run", () => {
  it.each(ALL_STATES)(
    "GRU-FR-YHFQ, GRU-FR-HABG: the region of an apply run in %s names its change in the place of a draft, and no draft",
    (state) => {
      region(changeRun("apply", state));
      expect(screen.getAllByText(`Apply ${CHANGE}`).length).toBeGreaterThan(0);
      expect(screen.getByTestId("graduation-openspec-change")).toHaveTextContent(`Change ${CHANGE}`);
      expect(screen.queryByRole("button", { name: "Open draft" })).toBeNull();
      expect(document.body).not.toHaveTextContent(/draft/i);
      expect(screen.queryByText(/graduated/i)).toBeNull();
    },
  );

  it("GRU-FR-YHFQ: a rework run's region renders the instruction as escaped plain text", () => {
    region(changeRun("rework", "working"));
    const instruction = screen.getByTestId("graduation-rework-instruction");
    expect(instruction).toHaveTextContent("Split <b>task 2</b> in two.");
    expect(instruction.querySelector("b")).toBeNull();
    expect(screen.getAllByText(`Rework ${CHANGE}`).length).toBeGreaterThan(0);
  });

  it("GRU-FR-IKEQ: a completed rework run with a proposal says its result is a proposal in the discussion of the change", () => {
    const run = changeRun("rework", "completed", {
      observability: makeObservability({ currentStage: "done", stageCondition: "complete" }),
    });
    run.openspec = { ...run.openspec!, proposalId: "p-1" };
    region(run);
    expect(screen.getByTestId("run-progress")).toHaveTextContent(
      "The result is a proposal in the discussion of the change.",
    );
    expect(screen.queryByTestId("graduation-commits")).toBeNull();
  });

  it("GRU-FR-IKEQ: a completed rework run with no proposal says the pass found nothing to change", () => {
    region(
      changeRun("rework", "completed", {
        observability: makeObservability({ currentStage: "done", stageCondition: "complete" }),
      }),
    );
    expect(screen.getByTestId("run-progress")).toHaveTextContent("The rework pass found nothing to change.");
  });

  it.each([
    ["proposal_pending", "The change holds a pending edit proposal"],
    ["discussion_resolved", "The request thread of the rework is resolved"],
    ["write_failed", "The proposal could not be written."],
  ])(
    "GRU-FR-NGQE: a rework run blocked at its finish on %s says why in words, and offers Continue",
    (code, words) => {
      region(
        changeRun("rework", "blocked", {
          blocker: { code, message: "internal", clearsBy: "Continue the run.", attempt: 1 },
          observability: makeObservability({ currentStage: "review", stageCondition: "blocked" }),
        }),
      );
      expect(screen.getByTestId("graduation-blocker")).toHaveTextContent(words);
      expect(screen.getByRole("button", { name: "Continue" })).toBeInTheDocument();
    },
  );

  it("GRU-FR-GLSO, RUN-FR-09: the region of an OpenSpec change run offers no affordance to start a run", () => {
    region(changeRun("apply", "completed"));
    expect(screen.queryByRole("button", { name: /^Apply/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Rework/ })).toBeNull();
    expect(screen.queryByRole("button", { name: /Graduate/ })).toBeNull();
  });
});

describe("Open change of an OpenSpec change run", () => {
  it.each(ALL_STATES)(
    "GRU-FR-LTWN: an apply run in %s offers Open change, which opens its change on its stream",
    async (state) => {
      row(changeRun("apply", state));
      await userEvent.click(screen.getByRole("button", { name: "Open change" }));
      expect(open).toHaveBeenCalledWith({ kind: "stream", streamId: "s-7" }, CHANGE, {
        streamName: "auth-work",
      });
    },
  );

  it.each(ALL_STATES)(
    "GRU-FR-LTWN, GRU-FR-IKEQ: a rework run in %s offers Open change, which selects its request thread",
    async (state) => {
      row(changeRun("rework", state));
      await userEvent.click(screen.getByRole("button", { name: "Open change" }));
      expect(open).toHaveBeenCalledWith({ kind: "stream", streamId: "s-7" }, CHANGE, {
        streamName: "auth-work",
        thread: "t-req",
      });
    },
  );

  it("GRU-FR-HABG: an apply run offers no Open draft in any state", () => {
    for (const state of ALL_STATES) {
      row(changeRun("apply", state));
      expect(screen.queryByRole("button", { name: "Open draft" })).toBeNull();
      cleanup();
    }
  });

  it("GRU-FR-WDWB: a discarded apply run and a discarded rework run offer Restart", () => {
    row(changeRun("apply", "discarded"));
    expect(screen.getByRole("button", { name: `Restart “Apply ${CHANGE}”` })).toBeInTheDocument();
    cleanup();
    row(changeRun("rework", "discarded"));
    expect(screen.getByRole("button", { name: `Restart “Rework ${CHANGE}”` })).toBeInTheDocument();
  });
});

describe("the rail row of an OpenSpec change run", () => {
  function rail(runs: GraduationRun[]) {
    const props: GraduationRailProps = {
      runs,
      listed: runs,
      selected: null,
      view: "in-flight",
      onView: vi.fn(),
      filter: "",
      onFilter: vi.fn(),
      busy: false,
      onSelect: vi.fn(),
      positionOf: () => null,
      onPause: vi.fn(),
      onContinue: vi.fn(),
      onAutoStart: vi.fn(),
      onArchive: vi.fn(),
      onRestart: vi.fn(),
    };
    render(<GraduationRail {...props} />);
  }

  it("GRH-FR-FYTH: the row of an apply run and of a rework run names its title in the place of a draft", () => {
    rail([changeRun("apply", "working"), { ...changeRun("rework", "queued"), id: "r2" }]);
    const names = screen.getAllByTestId("graduation-row").map((b) => b.getAttribute("aria-label"));
    expect(names).toEqual([`Apply ${CHANGE}`, `Rework ${CHANGE}`]);
  });
});

describe("restarting a discarded OpenSpec change run", () => {
  function confirmOf(run: GraduationRun) {
    const onRestarted = vi.fn();
    render(<RestartRegionControl run={run} busy={false} onRestarted={onRestarted} />);
    return onRestarted;
  }

  it("GRT-FR-GLLN: the confirmation of an apply run names its title and stream, and asks no stream and no standing-work choice", async () => {
    confirmOf(changeRun("apply", "discarded"));
    await userEvent.click(screen.getByTestId("graduation-restart"));
    const confirm = await screen.findByTestId("graduation-restart-confirm");
    expect(confirm).toHaveTextContent(`Apply ${CHANGE}`);
    expect(confirm).toHaveTextContent("auth-work");
    expect(confirm).toHaveTextContent("It works on the same change there.");
    expect(within(confirm).queryByRole("combobox")).toBeNull();
    expect(within(confirm).queryByRole("radio")).toBeNull();
    expect(calls("list_work_streams")).toHaveLength(0);
  });

  it("GRT-FR-GLLN: the confirmation of a rework run states the same instruction, and the restart sends the run's stream and resting values", async () => {
    const onRestarted = confirmOf(changeRun("rework", "discarded"));
    await userEvent.click(screen.getByTestId("graduation-restart"));
    const confirm = await screen.findByTestId("graduation-restart-confirm");
    expect(confirm).toHaveTextContent("with the same instruction");
    await userEvent.click(within(confirm).getByRole("button", { name: "Restart run" }));
    await waitFor(() => expect(onRestarted).toHaveBeenCalled());
    expect(calls("restart_graduation_run")[0][1]).toEqual({
      runId: "r1",
      streamId: "s-7",
      standingWork: "commit",
      standingWorkMessage: null,
    });
  });
});

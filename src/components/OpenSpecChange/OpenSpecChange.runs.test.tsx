/**
 * Apply and the run strip of a change workspace
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-DMGL,
 * OCW-FR-QSZP, OCW-FR-DTJD, OCW-FR-NNRY, OCW-FR-EKYF, OCW-FR-JHAB,
 * OCW-FR-YTGJ, OCW-FR-BTSI, OCW-FR-FAEL, OCW-FR-LYIX, OCW-FR-SFYL,
 * OCW-FR-NWRS).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

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
}));

import { configureRunRoute } from "../../state/graduation/runRoute";
import { rereadChangeProposals } from "../../state/openspec/changeProposals";
import {
  configureOpenSpecChangeRoutes,
  openOpenSpecChange,
} from "../../state/openspec/changeRoutes";
import { makeObservability, makeRun, type RunOverrides } from "../../test/graduationFixtures";
import {
  CHANGE,
  STREAM_ID,
  applyProgress,
  changeBackend,
  changeProposal,
  drawChange,
  resetChangeWorkspaceStores,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";
import type { GraduationRun, GraduationRunState } from "../../types";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function emit(event: string, payload: unknown) {
  for (const cb of listeners.get(event) ?? []) cb({ payload });
}

let backend: ChangeBackend;
const openRun = vi.fn();

beforeEach(() => {
  resetChangeWorkspaceStores();
  listeners.clear();
  openRun.mockReset();
  configureRunRoute(openRun);
  invokeMock.mockReset();
  backend = changeBackend({ proposals: [], discussions: [] });
  wireChangeBackend(invokeMock, backend);
  const rest = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "open_settings_window") return undefined;
    return rest(cmd, args);
  });
});
afterEach(() => {
  configureRunRoute(null);
  configureOpenSpecChangeRoutes(null);
  cleanup();
  expect(backend.unknown).toEqual([]);
});

/** A run of the change on its stream. */
function changeRun(
  id: string,
  phase: "propose" | "apply" | "rework",
  state: GraduationRunState,
  over: RunOverrides = {},
): GraduationRun {
  return makeRun(id, state, {
    streamId: STREAM_ID,
    streamName: "auth-work",
    input: { draftId: "", draftName: "", prompt: "p", promptChecksum: "c", capturedAt: "2026-01-01T00:00:00Z" },
    openspec: { phase, change: CHANGE },
    ...over,
  });
}

const apply = () => screen.getByRole("button", { name: "Apply" });
/** The discussion tab of a thread of the fixture: t2 is the passage "# Why". */
const threadTab = (column: HTMLElement, thread: "t1" | "t2") =>
  thread === "t2"
    ? within(column).getByTitle("Discussion 2: # Why")
    : within(column).getByTitle(/^Discussion 1/);
const strip = () => screen.getByTestId("openspec-run-strip");

async function opened() {
  drawChange();
  await screen.findAllByTestId("openspec-artifact");
  await waitFor(() => expect(calls("list_graduation_queue").length).toBeGreaterThan(0));
}

describe("Apply (OCW-FR-DTJD, OCW-FR-NNRY, OCW-FR-EKYF)", () => {
  it("OCW-FR-DTJD: Apply invokes start change apply with the stream and the change, asks nothing, and the strip shows the new run", async () => {
    backend.startApply = () => {
      const run = changeRun("g-1", "apply", "queued", {
        observability: makeObservability({ currentStage: "queued", stageCondition: "waiting" }),
      });
      backend.runs = [run];
      return run;
    };
    await opened();
    expect(screen.queryByTestId("openspec-run-strip")).toBeNull();
    await userEvent.click(apply());
    await waitFor(() => expect(calls("start_change_apply")).toHaveLength(1));
    expect(calls("start_change_apply")[0][1]).toEqual({ streamId: STREAM_ID, change: CHANGE });
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(await screen.findByTestId("openspec-run-strip")).toHaveTextContent(`Apply ${CHANGE}`);
  });

  it("OCW-FR-NNRY: a tab whose location is not a stream offers no Apply and no Rework…", async () => {
    drawChange({ location: { kind: "active_worktree" } });
    await screen.findAllByTestId("openspec-artifact");
    expect(screen.queryByRole("button", { name: "Apply" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Rework…" })).toBeNull();
    expect(calls("read_apply_progress")).toHaveLength(0);
    expect(calls("list_graduation_queue")).toHaveLength(0);
  });

  it("OCW-FR-NNRY: Apply is unavailable with a sentence while a start is in flight", async () => {
    let release: (run: GraduationRun) => void = () => {};
    const rest = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "start_change_apply") return new Promise<GraduationRun>((resolve) => (release = resolve));
      return rest(cmd, args);
    });
    await opened();
    await userEvent.click(apply());
    await waitFor(() => expect(apply()).toHaveAttribute("aria-disabled", "true"));
    expect(screen.getByTestId("openspec-apply-block")).toHaveTextContent("The apply run is starting.");
    await userEvent.click(apply());
    expect(calls("start_change_apply")).toHaveLength(1);
    await act(async () => release(changeRun("g-1", "apply", "completed")));
    await waitFor(() => expect(apply()).not.toHaveAttribute("aria-disabled"));
  });

  it("OCW-FR-NNRY: Apply is unavailable while a run of the change on this stream is not terminal, and names that run", async () => {
    backend.runs = [changeRun("g-2", "propose", "working", { input: { ...makeRun("x").input, draftName: "Logout" } })];
    await opened();
    await waitFor(() => expect(apply()).toHaveAttribute("aria-disabled", "true"));
    expect(screen.getByTestId("openspec-apply-block")).toHaveTextContent("The run “Logout” of this change has not ended.");
    expect(apply()).toHaveAccessibleDescription(/The run “Logout” of this change has not ended./);
    await userEvent.click(apply());
    expect(calls("start_change_apply")).toHaveLength(0);
  });

  it("OCW-FR-NNRY: a run of another change or another stream leaves Apply available", async () => {
    backend.runs = [
      changeRun("g-1", "apply", "completed", { createdAt: "2025-12-01T00:00:00Z" }),
      changeRun("g-2", "apply", "working", { openspec: { phase: "apply", change: "other" } }),
      changeRun("g-3", "apply", "working", { streamId: "s-9" }),
    ];
    await opened();
    // The strip of the terminal run shows that the answer of the queue read is drawn.
    await waitFor(() => expect(screen.getByTestId("openspec-run-strip-state")).toHaveTextContent("Completed"));
    expect(apply()).not.toHaveAttribute("aria-disabled");
    expect(screen.queryByTestId("openspec-apply-block")).toBeNull();
  });

  it("OCW-FR-NNRY: Apply is unavailable with a sentence while the change holds a pending edit proposal", async () => {
    backend.proposals = [changeProposal()];
    await opened();
    await waitFor(() => expect(apply()).toHaveAttribute("aria-disabled", "true"));
    expect(screen.getByTestId("openspec-apply-block")).toHaveTextContent(
      "Apply and Rework… are unavailable. The change holds a pending edit proposal. Decide it first.",
    );
    expect(apply()).toHaveAccessibleDescription(/The change holds a pending edit proposal. Decide it first./);
    await userEvent.click(apply());
    expect(calls("start_change_apply")).toHaveLength(0);
  });

  it("OCW-FR-NNRY: Apply is unavailable with a sentence while a closed proposal's commit is owed, and available once it is not", async () => {
    backend.proposals = [changeProposal(["accepted", "accepted"], { state: "accepted", commitOwed: true })];
    await opened();
    await waitFor(() => expect(apply()).toHaveAttribute("aria-disabled", "true"));
    expect(screen.getByTestId("openspec-apply-block")).toHaveTextContent(
      "Apply is unavailable. The accepted edits of an edit proposal are not committed yet.",
    );
    // A commit owed holds Apply alone: Rework… stays available.
    expect(screen.getByRole("button", { name: "Rework…" })).not.toHaveAttribute("aria-disabled");
    await userEvent.click(apply());
    expect(calls("start_change_apply")).toHaveLength(0);

    backend.proposals = [changeProposal(["accepted", "accepted"], { state: "accepted", commitOwed: false })];
    await act(async () => rereadChangeProposals(STREAM_ID, CHANGE));
    await waitFor(() => expect(apply()).not.toHaveAttribute("aria-disabled"));
    expect(screen.queryByTestId("openspec-apply-block")).toBeNull();
  });

  it("OCW-FR-NNRY: Apply is unavailable while the last apply progress states blocked, naming the missing artifacts", async () => {
    backend.applyProgress = applyProgress({ state: "blocked", missingArtifacts: ["design", "tasks"] });
    await opened();
    await waitFor(() => expect(apply()).toHaveAttribute("aria-disabled", "true"));
    expect(screen.getByTestId("openspec-apply-block")).toHaveTextContent("The change is missing artifacts: design, tasks.");
    expect(calls("read_apply_progress")[0][1]).toEqual({ streamId: STREAM_ID, change: CHANGE });
  });

  it("OCW-FR-NNRY: Refresh reads the apply progress again, and a change that became ready makes Apply available", async () => {
    backend.applyProgress = applyProgress({ state: "blocked", missingArtifacts: ["tasks"] });
    await opened();
    await waitFor(() => expect(apply()).toHaveAttribute("aria-disabled", "true"));
    const before = calls("read_apply_progress").length;
    backend.applyProgress = applyProgress();
    await userEvent.click(screen.getByRole("button", { name: "Refresh" }));
    await waitFor(() => expect(apply()).not.toHaveAttribute("aria-disabled"));
    expect(calls("read_apply_progress")).toHaveLength(before + 1);
  });

  it("OCW-FR-EKYF: openspec_run_active names the run, with a route to it", async () => {
    backend.runs = [changeRun("g-7", "apply", "completed")];
    backend.startApply = () => ({ reject: "openspec_run_active: g-7" });
    await opened();
    await userEvent.click(apply());
    const refusal = await screen.findByTestId("openspec-apply-refusal");
    expect(refusal).toHaveTextContent(`The run “Apply ${CHANGE}” of this change on this stream has not ended, so no run started.`);
    await userEvent.click(within(refusal).getByRole("button", { name: "Open run" }));
    expect(openRun).toHaveBeenCalledWith("g-7");
  });

  it("OCW-FR-EKYF: proposal_pending from Apply states the proposal to decide or the edits to commit, and that no run started", async () => {
    // The backend saw a proposal that the tab did not read yet.
    backend.startApply = () => ({ reject: "proposal_pending" });
    await opened();
    await userEvent.click(apply());
    expect(await screen.findByTestId("openspec-apply-refusal")).toHaveTextContent(
      "The change holds a pending edit proposal, or accepted edits that are not committed. Decide the proposal or commit the edits first. No run started.",
    );
  });

  it("OCW-FR-EKYF, OFN-FR-RNWG: a refusal that names no change takes the change of the tab in its sentence", async () => {
    backend.startApply = () => ({ reject: { code: "tasks_incomplete", diagnostics: [] } });
    await opened();
    await userEvent.click(apply());
    const refusal = await screen.findByTestId("openspec-apply-refusal");
    expect(within(refusal).getByRole("alert")).toHaveTextContent(
      `The change "${CHANGE}" has tasks that are not done. No run started.`,
    );
    expect(refusal).not.toHaveTextContent("this change");
  });

  it("OCW-FR-EKYF, OFN-FR-IHSQ: change_not_ready renders its notice with the missing artifacts, that no run started, and its Details", async () => {
    backend.startApply = () => ({
      reject: {
        code: "change_not_ready",
        detail: "design,tasks",
        diagnostics: [
          { severity: "error", message: "Missing artifact: design" },
          { severity: "info", message: "Write the design first." },
        ],
      },
    });
    await opened();
    await userEvent.click(apply());
    const refusal = await screen.findByTestId("openspec-apply-refusal");
    expect(within(refusal).getByRole("alert")).toHaveTextContent(
      "The change misses these artifacts: design, tasks. Complete them before Apply. No run started.",
    );
    // OFN-FR-RLSO: the tab of the change offers no Open change.
    expect(within(refusal).queryByRole("button", { name: "Open change" })).toBeNull();
    await userEvent.click(within(refusal).getByRole("button", { name: /Details/ }));
    expect(within(refusal).getAllByTestId("openspec-failure-diagnostic").map((d) => d.textContent)).toEqual([
      "error: Missing artifact: design",
      "info: Write the design first.",
    ]);
  });

  it.each<[unknown]>([
    [{ code: "openspec_mode_inactive", diagnostics: [] }],
    [{ code: "cli_unavailable", detail: "unsupported", diagnostics: [] }],
  ])(
    "OCW-FR-EKYF, OFN-FR-EPIG: an environment refusal %# routes to the OpenSpec section of Project settings",
    async (reject) => {
      backend.startApply = () => ({ reject });
      await opened();
      await userEvent.click(apply());
      const refusal = await screen.findByTestId("openspec-apply-refusal");
      expect(refusal).toHaveTextContent("No run started.");
      await userEvent.click(within(refusal).getByRole("button", { name: "Open OpenSpec settings" }));
      await waitFor(() => expect(calls("open_settings_window")).toHaveLength(1));
      expect(calls("open_settings_window")[0][1]).toEqual({ section: "project/openspec" });
    },
  );

  it("OCW-FR-EKYF, OFN-FR-CBLG, OFP-FR-BYYT: apply_progress_unavailable takes the class of its cause, and Retry re-checks the CLI and starts again", async () => {
    let starts = 0;
    backend.startApply = () => {
      starts += 1;
      return starts === 1
        ? { reject: { code: "apply_progress_unavailable", cause: "cli_timed_out", diagnostics: [] } }
        : changeRun("g-9", "apply", "working");
    };
    await opened();
    await userEvent.click(apply());
    const refusal = await screen.findByTestId("openspec-apply-refusal");
    expect(within(refusal).getByRole("alert")).toHaveTextContent(
      "The OpenSpec CLI did not finish in time, also after one more try. No run started.",
    );
    await userEvent.click(within(refusal).getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(starts).toBe(2));
    expect(backend.statusChecks).toEqual([true]);
    await waitFor(() => expect(screen.queryByTestId("openspec-apply-refusal")).toBeNull());
  });

  it("OCW-FR-EKYF: an image preflight refusal states what the author configures", async () => {
    backend.startApply = () => ({ reject: "vendor_image_unconfigured" });
    await opened();
    await userEvent.click(apply());
    expect(await screen.findByTestId("openspec-apply-refusal")).toHaveTextContent(
      "Configure an agent image for the active agent in Project settings",
    );
  });

  it("OCW-FR-EKYF: every other code states the code and that no run started", async () => {
    backend.startApply = () => ({ reject: "invalid_change_name" });
    await opened();
    await userEvent.click(apply());
    expect(await screen.findByTestId("openspec-apply-refusal")).toHaveTextContent(
      "The run could not start (invalid_change_name). No run started.",
    );
  });
});

describe("the run strip (OCW-FR-JHAB, OCW-FR-YTGJ, OCW-FR-BTSI, OCW-FR-FAEL)", () => {
  it("OCW-FR-JHAB: the strip names the newest apply or rework run of the change, its state in words, and its stage labels", async () => {
    backend.runs = [
      changeRun("g-1", "apply", "completed", { createdAt: "2026-01-01T00:00:00Z" }),
      changeRun("g-2", "rework", "working", { createdAt: "2026-01-02T00:00:00Z" }),
      changeRun("g-3", "propose", "completed", { createdAt: "2026-01-03T00:00:00Z" }),
      changeRun("g-4", "apply", "working", { createdAt: "2026-01-04T00:00:00Z", openspec: { phase: "apply", change: "other" } }),
    ];
    await opened();
    const shown = await screen.findByTestId("openspec-run-strip");
    expect(shown).toHaveTextContent(`Rework ${CHANGE}`);
    expect(within(shown).getByTestId("openspec-run-strip-state")).toHaveTextContent("Working, pass 1");
    const labels = [...shown.querySelectorAll(".run-progress__label")].map((el) => el.textContent);
    expect(labels).toEqual(["Queued", "Reworking", "Reviewing", "Proposed"]);
  });

  it("OCW-FR-YTGJ: a tab with no apply or rework run of the change shows no strip", async () => {
    backend.runs = [changeRun("g-3", "propose", "completed")];
    await opened();
    expect(screen.queryByTestId("openspec-run-strip")).toBeNull();
  });

  it("OCW-FR-YTGJ: a run that is not terminal offers no Close", async () => {
    backend.runs = [changeRun("g-1", "apply", "working")];
    await opened();
    await screen.findByTestId("openspec-run-strip");
    expect(within(strip()).queryByRole("button", { name: "Close" })).toBeNull();
  });

  it("OCW-FR-YTGJ: a terminal run stays with its outcome until Close, and a newer run shows the strip again", async () => {
    backend.runs = [
      changeRun("g-1", "apply", "completed", {
        commits: ["c1"],
        observability: makeObservability({ currentStage: "done", stageCondition: "complete" }),
      }),
    ];
    await opened();
    expect(await screen.findByTestId("openspec-run-strip")).toHaveTextContent("Committed one change on the work stream.");
    await userEvent.click(within(strip()).getByRole("button", { name: "Close" }));
    expect(screen.queryByTestId("openspec-run-strip")).toBeNull();

    backend.runs = [...backend.runs, changeRun("g-2", "apply", "queued", { createdAt: "2026-09-07T00:00:00Z" })];
    act(() => emit("graduation-queue-changed", { projectKey: "k", streamId: STREAM_ID, worktreePath: "" }));
    expect(await screen.findByTestId("openspec-run-strip")).toBeInTheDocument();
  });

  it("OCW-FR-BTSI: the Tasks stage of a working apply run states the counts and the next open task, from read apply progress", async () => {
    backend.runs = [changeRun("g-1", "apply", "working")];
    await opened();
    await waitFor(() => expect(strip()).toHaveTextContent("2 of 4 tasks"));
    expect(within(strip()).getByTestId("openspec-run-strip-next")).toHaveTextContent("next: Add the logout route");
  });

  it("OCW-FR-BTSI: it reads the progress again on each change of the run, and a read that fails keeps the last answer", async () => {
    backend.runs = [changeRun("g-1", "apply", "working")];
    await opened();
    await waitFor(() => expect(strip()).toHaveTextContent("2 of 4 tasks"));
    const before = calls("read_apply_progress").length;
    backend.applyProgress = applyProgress({ complete: 3, nextTask: "Test the route" });
    act(() => emit("graduation-run-changed", { runId: "g-1", state: "working" }));
    await waitFor(() => expect(strip()).toHaveTextContent("3 of 4 tasks"));
    expect(calls("read_apply_progress").length).toBeGreaterThan(before);

    backend.applyProgress = { reject: "cli_failed" };
    const failedFrom = calls("read_apply_progress").length;
    act(() => emit("graduation-run-changed", { runId: "g-1", state: "working" }));
    await waitFor(() => expect(calls("read_apply_progress").length).toBeGreaterThan(failedFrom));
    expect(strip()).toHaveTextContent("3 of 4 tasks");
    expect(within(strip()).getByTestId("openspec-run-strip-next")).toHaveTextContent("Test the route");
  });

  it("OCW-FR-BTSI, OFN-FR-QZEC: a read that fails keeps the last answer and adds the short form of its notice, with no action and no Details", async () => {
    backend.runs = [changeRun("g-1", "apply", "working")];
    await opened();
    await waitFor(() => expect(strip()).toHaveTextContent("2 of 4 tasks"));
    backend.applyProgress = { reject: { code: "cli_timed_out", diagnostics: [{ message: "slow" }] } as never };
    act(() => emit("graduation-run-changed", { runId: "g-1", state: "working" }));
    const SHORT = "The OpenSpec CLI did not finish in time, also after one more try.";
    // The short form stands on one line of its own under the stage row, with
    // its whole text as its tooltip, so the sentence of the stage stays short.
    const note = await within(strip()).findByTestId("run-progress-note");
    expect(note.textContent).toBe(SHORT);
    expect(note).toHaveAttribute("title", SHORT);
    expect(strip().querySelector(".run-progress__stage[data-status='current'] .run-progress__condition")).toHaveTextContent(
      /^2 of 4 tasks$/,
    );
    // The announcement reads the sentence, then the short form, each closed once.
    const spoken = within(strip()).getByRole("status").textContent ?? "";
    expect(spoken).toContain(`2 of 4 tasks. ${SHORT}`);
    expect(spoken).not.toContain("..");
    expect(within(strip()).queryByTestId("openspec-failure-notice")).toBeNull();
    expect(within(strip()).queryByRole("button", { name: /Details|Retry/ })).toBeNull();
    // A read that answers again takes the short form away.
    backend.applyProgress = applyProgress({ complete: 3 });
    act(() => emit("graduation-run-changed", { runId: "g-1", state: "working" }));
    await waitFor(() => expect(strip()).toHaveTextContent("3 of 4 tasks"));
    expect(strip()).not.toHaveTextContent("did not finish in time");
    expect(within(strip()).queryByTestId("run-progress-note")).toBeNull();
  });

  it("OCW-FR-FAEL: the detail slot of an apply run states the check state in words", async () => {
    backend.runs = [changeRun("g-1", "apply", "working", { checkpoint: { checkGate: { next: "correct" } } })];
    await opened();
    expect(await screen.findByTestId("openspec-run-strip-checks")).toHaveTextContent("correcting checks");
  });

  it("OCW-FR-FAEL: the detail slot of a rework run states nothing", async () => {
    backend.runs = [changeRun("g-1", "rework", "working")];
    await opened();
    await screen.findByTestId("openspec-run-strip");
    expect(within(strip()).queryByTestId("openspec-run-strip-checks")).toBeNull();
    expect(within(strip()).queryByTestId("openspec-run-strip-next")).toBeNull();
  });

  it("OCW-FR-LYIX: Open run opens the Runs panel on the run", async () => {
    backend.runs = [changeRun("g-1", "apply", "working")];
    await opened();
    await screen.findByTestId("openspec-run-strip");
    await userEvent.click(within(strip()).getByRole("button", { name: "Open run" }));
    expect(openRun).toHaveBeenCalledWith("g-1");
  });

  it("OCW-FR-NWRS: the strip stands in the document column, its head holds the title, the state, and the actions, and the next task carries its whole text", async () => {
    backend.runs = [changeRun("g-1", "apply", "working")];
    backend.applyProgress = applyProgress({ nextTask: "Add the logout route and a test that covers every page of the app" });
    await opened();
    await waitFor(() => expect(screen.getByTestId("openspec-run-strip-next")).toBeInTheDocument());
    expect(strip().closest(".openspec-change__doc")).not.toBeNull();
    expect(strip().closest("[data-testid='openspec-change-discussion']")).toBeNull();
    const head = strip().querySelector(".openspec-runstrip__head")!;
    expect(head).toHaveTextContent(`Apply ${CHANGE}`);
    expect(within(head as HTMLElement).getByTestId("openspec-run-strip-state")).toBeInTheDocument();
    expect(within(head as HTMLElement).getByRole("button", { name: "Open run" })).toBeInTheDocument();
    const next = screen.getByTestId("openspec-run-strip-next");
    const whole = "Add the logout route and a test that covers every page of the app";
    expect(next).toHaveAttribute("title", whole);
    expect(next).toHaveAccessibleName(`Next task: ${whole}`);
  });

  it("OCW-FR-NWRS, OCW-FR-JHAB: the state in words carries its whole text as its tooltip, and the title carries its own", async () => {
    backend.runs = [changeRun("g-1", "apply", "working")];
    await opened();
    const state = await screen.findByTestId("openspec-run-strip-state");
    expect(state).toHaveTextContent("Working, pass 1");
    expect(state).toHaveAttribute("title", state.textContent);
    expect(strip().querySelector(".openspec-runstrip__title")).toHaveAttribute("title", `Apply ${CHANGE}`);
  });
});

describe("following the runs (OCW-FR-SFYL)", () => {
  it("OCW-FR-SFYL: a queue event for its stream reads the queue again and settles Apply; one for another stream does not", async () => {
    backend.runs = [changeRun("g-1", "apply", "working")];
    await opened();
    await waitFor(() => expect(apply()).toHaveAttribute("aria-disabled", "true"));
    const before = calls("list_graduation_queue").length;
    act(() => emit("graduation-queue-changed", { projectKey: "k", streamId: "s-9", worktreePath: "" }));
    expect(calls("list_graduation_queue")).toHaveLength(before);

    backend.runs = [changeRun("g-1", "apply", "completed")];
    act(() => emit("graduation-queue-changed", { projectKey: "k", streamId: STREAM_ID, worktreePath: "" }));
    await waitFor(() => expect(apply()).not.toHaveAttribute("aria-disabled"));
    expect(screen.getByTestId("openspec-run-strip-state")).toHaveTextContent("Completed");
  });

  it("OCW-FR-SFYL: a run event for a run of its stream reads the queue again and updates the strip", async () => {
    backend.runs = [changeRun("g-1", "apply", "working")];
    await opened();
    await screen.findByTestId("openspec-run-strip");
    const before = calls("list_graduation_queue").length;
    act(() => emit("graduation-run-changed", { runId: "unknown", state: "working" }));
    expect(calls("list_graduation_queue")).toHaveLength(before);
    backend.runs = [
      changeRun("g-1", "apply", "reviewing", { observability: makeObservability({ currentStage: "review" }) }),
    ];
    act(() => emit("graduation-run-changed", { runId: "g-1", state: "reviewing" }));
    await waitFor(() => expect(screen.getByTestId("openspec-run-strip-state")).toHaveTextContent("In review, pass 1"));
  });
});

describe("an open that names a thread (OCW-FR-QSZP, OCW-FR-DMGL)", () => {
  it("OCW-FR-QSZP, OCW-FR-DMGL: an open from a rework run selects its request thread in the discussion column", async () => {
    backend.discussions = changeBackend().discussions;
    // The route draws the tab from what it is given, as the shell does.
    configureOpenSpecChangeRoutes({ open: (location, name) => void drawChange({ location, name }) });
    openOpenSpecChange({ kind: "stream", streamId: STREAM_ID }, CHANGE, { thread: "t2" });
    const column = await screen.findByTestId("openspec-change-discussion");
    await waitFor(() => expect(within(column).getAllByRole("tab")).toHaveLength(2));
    await waitFor(() => expect(threadTab(column, "t2")).toHaveAttribute("aria-selected", "true"));
    expect(threadTab(column, "t1")).toHaveAttribute("aria-selected", "false");
  });

  it("OCW-FR-QSZP: an open that names no thread does not select the request thread", async () => {
    backend.discussions = changeBackend().discussions;
    configureOpenSpecChangeRoutes({ open: (location, name) => void drawChange({ location, name }) });
    openOpenSpecChange({ kind: "stream", streamId: STREAM_ID }, CHANGE);
    const column = await screen.findByTestId("openspec-change-discussion");
    await waitFor(() => expect(within(column).getAllByRole("tab")).toHaveLength(2));
    await waitFor(() => expect(threadTab(column, "t1")).toHaveAttribute("aria-selected", "true"));
    expect(threadTab(column, "t2")).toHaveAttribute("aria-selected", "false");
  });
});

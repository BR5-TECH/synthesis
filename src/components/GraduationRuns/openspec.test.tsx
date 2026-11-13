/**
 * The propose run in the Runs panel
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-LTWN,
 * GRU-FR-YHFQ).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../../logging";
import { configureOpenSpecChangeRoutes } from "../../state/openspec/changeRoutes";
import { makeQueue, makeRun } from "../../test/graduationFixtures";
import type { GraduationRun, GraduationRunState } from "../../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
  emit: () => Promise.resolve(),
}));

import { RunActionRow } from "./actionRow";
import { RunRegion } from "./region";

const open = vi.fn();

beforeEach(() => {
  resetLogBufferForTest();
  open.mockReset();
  configureOpenSpecChangeRoutes({ open });
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
});
afterEach(() => {
  configureOpenSpecChangeRoutes(null);
  cleanup();
});

const propose = (state: GraduationRunState): GraduationRun =>
  makeRun("r1", state, {
    streamId: "s-7",
    streamName: "auth-work",
    openspec: { phase: "propose", change: "add-logout-3f2a" },
  });

const act = vi.fn(async () => true);
const pressedRef = { current: null };

function row(run: GraduationRun) {
  render(
    <RunActionRow
      run={run}
      busy={false}
      act={act}
      onRestarted={() => {}}
      refusal={null}
      pressedRef={pressedRef}
      openRestart={false}
      onRestartOpened={() => {}}
    />,
  );
}

function region(run: GraduationRun) {
  render(
    <RunRegion
      run={run}
      runs={[run]}
      queue={makeQueue([run])}
      capacity={null}
      busy={false}
      act={act}
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

describe("a propose run", () => {
  it("GRU-FR-LTWN: a completed propose run offers Open change, which opens its change on its stream", async () => {
    row(propose("completed"));
    await userEvent.click(screen.getByRole("button", { name: "Open change" }));
    expect(open).toHaveBeenCalledWith(
      { kind: "stream", streamId: "s-7" },
      "add-logout-3f2a",
      { streamName: "auth-work" },
    );
  });

  it.each<GraduationRunState>([
    "queued",
    "working",
    "reviewing",
    "awaiting_author",
    "blocked",
    "interrupted",
    "failed",
    "discarded",
  ])(
    "GRU-FR-LTWN: a propose run in %s offers no Open change",
    (state) => {
      row(propose(state));
      expect(screen.queryByRole("button", { name: "Open change" })).toBeNull();
    },
  );

  it("GRU-FR-LTWN: a completed draft run offers no Open change", () => {
    row(makeRun("r2", "completed"));
    expect(screen.queryByRole("button", { name: "Open change" })).toBeNull();
  });

  it.each<GraduationRunState>([
    "queued",
    "working",
    "reviewing",
    "awaiting_author",
    "blocked",
    "interrupted",
    "completed",
    "failed",
    "discarded",
  ])(
    "GRU-FR-YHFQ: the region of a propose run in %s names the change beside its source draft",
    (state) => {
      region(propose(state));
      expect(screen.getByText("Run r1")).toBeInTheDocument();
      expect(screen.getByTestId("graduation-openspec-change")).toHaveTextContent(
        "Change add-logout-3f2a",
      );
    },
  );

  it("GRU-FR-YHFQ: the region of a draft run names no change", () => {
    region(makeRun("r2", "working"));
    expect(screen.queryByTestId("graduation-openspec-change")).toBeNull();
  });
});

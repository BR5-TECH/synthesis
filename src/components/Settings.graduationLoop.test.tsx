import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetLogBufferForTest } from "../logging";
import { Settings } from "../test/settingsHarness";
import { graduationFocusOf, resolveSelection } from "./SettingsBody";

// SET-FR-PSLD through SET-FR-OVTW: the pass and time limit sliders of the
// Project settings Graduation section, and the routes to them from a run that
// stopped on a time limit (GRU-FR-QLRQ, GRU-FR-TTLR).
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const HALF_HOUR = 1_800_000;

interface Stored {
  graduationPassBudget: number | null;
  graduationStageTimeLimitMs: number | null;
  graduationTotalTimeLimitMs: number | null;
}

interface Backend {
  saves: Record<string, unknown>[];
  /** Make the next saves wait until the test releases them. */
  gate: (() => void)[];
  holdSaves: boolean;
  refuse: string | null;
}

function backend(stored: Partial<Stored> = {}): Backend {
  const state: Stored = {
    graduationPassBudget: null,
    graduationStageTimeLimitMs: null,
    graduationTotalTimeLimitMs: null,
    ...stored,
  };
  const b: Backend = { saves: [], gate: [], holdSaves: false, refuse: null };
  invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "append_log_records":
        return Promise.resolve();
      case "load_project_config":
        return Promise.resolve({
          lineEndings: "crlf",
          draftTemplate: null,
          graduationConcurrencyLimit: 1,
          executionTimeoutMs: null,
          ...state,
        });
      case "save_project_config": {
        const config = (args as { config: Record<string, unknown> }).config;
        const write = () => {
          if (b.refuse) throw b.refuse;
          b.saves.push(config);
          Object.assign(state, config);
        };
        if (!b.holdSaves) {
          try {
            write();
            return Promise.resolve();
          } catch (e) {
            return Promise.reject(e);
          }
        }
        return new Promise<void>((resolve, reject) => {
          b.gate.push(() => {
            try {
              write();
              resolve();
            } catch (e) {
              reject(e);
            }
          });
        });
      }
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
  return b;
}

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
});
afterEach(cleanup);

async function openGraduation(section = "project/graduation", nonce = 1) {
  const view = render(
    <Settings
      contentRoot="/p#0"
      lineEndings="crlf"
      onSelectLineEndings={vi.fn()}
      sectionRequest={{ section, nonce }}
    />,
  );
  const sliders = await screen.findByTestId("graduation-loop-settings");
  await waitFor(() =>
    expect(screen.getByTestId("graduation-pass-budget")).not.toBeDisabled(),
  );
  return { view, sliders };
}

const slider = (item: string) => screen.getByTestId(`graduation-${item}`) as HTMLInputElement;

/** Drag a slider to a value and let it go. */
function drag(item: string, value: number) {
  const input = slider(item);
  fireEvent.change(input, { target: { value: String(value) } });
  fireEvent.pointerUp(input);
}

describe("Project settings — Graduation loop sliders", () => {
  it("SET-FR-PSLD, SET-FR-ISAV: three range sliders in order, with their bounds and steps", async () => {
    backend();
    const { sliders } = await openGraduation();
    const ranges = within(sliders).getAllByRole("slider") as HTMLInputElement[];
    expect(ranges.map((r) => r.id)).toEqual([
      "graduation-pass-budget",
      "graduation-stage-time-limit",
      "graduation-total-time-limit",
    ]);
    expect(screen.getByLabelText(/^Passes per run/)).toBe(ranges[0]);
    expect(screen.getByLabelText(/^Time per turn/)).toBe(ranges[1]);
    expect(screen.getByLabelText(/^Total time per run/)).toBe(ranges[2]);
    expect([ranges[0].min, ranges[0].max, ranges[0].step]).toEqual(["1", "10", "1"]);
    expect([ranges[1].min, ranges[1].max, ranges[1].step]).toEqual([
      String(HALF_HOUR),
      String(48 * 2 * HALF_HOUR),
      String(HALF_HOUR),
    ]);
    expect([ranges[2].min, ranges[2].max, ranges[2].step]).toEqual([
      String(HALF_HOUR),
      String(96 * 2 * HALF_HOUR),
      String(HALF_HOUR),
    ]);
    // SET-FR-ISAV: a native range control, full width in the accent colour,
    // with the range beneath it. The arrow keys, Home and End are its own.
    for (const r of ranges) {
      expect(r.type).toBe("range");
      expect(r.style.width).toBe("100%");
      expect(r.style.accentColor).toBe("var(--accent)");
    }
    expect(sliders).toHaveTextContent("1 pass – 10 passes");
    expect(sliders).toHaveTextContent("30 min – 48 h");
    expect(sliders).toHaveTextContent("30 min – 96 h");
  });

  it("SET-FR-RTDF: an unset slider shows its default as value text", async () => {
    backend();
    await openGraduation();
    expect(slider("pass-budget").value).toBe("2");
    expect(slider("pass-budget")).toHaveAttribute("aria-valuetext", "2 passes");
    expect(slider("stage-time-limit")).toHaveAttribute("aria-valuetext", "3 h");
    expect(slider("total-time-limit")).toHaveAttribute("aria-valuetext", "5 h");
    expect(screen.getByText("Passes per run — 2 passes")).toBeInTheDocument();
    expect(screen.getByText("Time per turn — 3 h")).toBeInTheDocument();
    expect(screen.getByText("Total time per run — 5 h")).toBeInTheDocument();
  });

  it("SET-FR-RTDF: a stored value shows as text beside the label and as value text", async () => {
    backend({
      graduationPassBudget: 1,
      graduationStageTimeLimitMs: 7 * HALF_HOUR,
      graduationTotalTimeLimitMs: HALF_HOUR,
    });
    await openGraduation();
    expect(slider("pass-budget")).toHaveAttribute("aria-valuetext", "1 pass");
    expect(slider("stage-time-limit")).toHaveAttribute("aria-valuetext", "3 h 30 min");
    expect(slider("total-time-limit")).toHaveAttribute("aria-valuetext", "30 min");
    expect(screen.getByText("Time per turn — 3 h 30 min")).toBeInTheDocument();
  });

  it("SET-FR-HLPL: each slider says what it bounds", async () => {
    backend();
    await openGraduation();
    expect(screen.getByTestId("graduation-pass-budget-note")).toHaveTextContent(
      "It does not apply to merge runs.",
    );
    expect(screen.getByTestId("graduation-stage-time-limit-note")).toHaveTextContent(
      "How long one graduation turn can run.",
    );
    expect(screen.getByTestId("graduation-total-time-limit-note")).toHaveTextContent(
      "It counts only working and reviewing time.",
    );
    expect(slider("total-time-limit")).toHaveAttribute(
      "aria-describedby",
      "graduation-total-time-limit-note",
    );
  });

  it("SET-FR-WTOS: a release saves at once, with the stored line endings and the one setting", async () => {
    const b = backend();
    await openGraduation();
    // A drag writes nothing until the slider is let go.
    fireEvent.change(slider("stage-time-limit"), { target: { value: String(8 * HALF_HOUR) } });
    expect(screen.getByText("Time per turn — 4 h")).toBeInTheDocument();
    expect(b.saves).toHaveLength(0);
    fireEvent.pointerUp(slider("stage-time-limit"));
    await waitFor(() => expect(b.saves).toHaveLength(1));
    expect(b.saves[0]).toEqual({
      lineEndings: "crlf",
      graduationStageTimeLimitMs: 8 * HALF_HOUR,
    });
    expect(slider("stage-time-limit")).toHaveAttribute("aria-valuetext", "4 h");
    // SET-FR-WTOS: outside the section dirty state — no save control.
    expect(screen.queryByRole("button", { name: /^save/i })).toBeNull();
  });

  it("SET-FR-WTOS, SET-FR-ISAV: a key release saves the value the key moved to", async () => {
    const b = backend();
    await openGraduation();
    const input = slider("pass-budget");
    fireEvent.change(input, { target: { value: "3" } });
    fireEvent.keyUp(input, { key: "ArrowRight" });
    await waitFor(() => expect(b.saves).toHaveLength(1));
    expect(b.saves[0]).toEqual({ lineEndings: "crlf", graduationPassBudget: 3 });
  });

  it("SET-FR-WTOS: quick changes go in sequence and are all kept", async () => {
    const b = backend();
    b.holdSaves = true;
    await openGraduation();
    drag("pass-budget", 4);
    drag("total-time-limit", 12 * HALF_HOUR);
    await waitFor(() => expect(b.gate).toHaveLength(1));
    // The second write waits for the first.
    expect(slider("pass-budget")).toHaveAttribute("aria-valuetext", "4 passes");
    expect(slider("total-time-limit")).toHaveAttribute("aria-valuetext", "6 h");
    b.gate.shift()!();
    await waitFor(() => expect(b.gate).toHaveLength(1));
    b.gate.shift()!();
    await waitFor(() => expect(b.saves).toHaveLength(2));
    expect(b.saves).toEqual([
      { lineEndings: "crlf", graduationPassBudget: 4 },
      { lineEndings: "crlf", graduationTotalTimeLimitMs: 12 * HALF_HOUR },
    ]);
    expect(slider("pass-budget")).toHaveAttribute("aria-valuetext", "4 passes");
    expect(slider("total-time-limit")).toHaveAttribute("aria-valuetext", "6 h");
  });

  it("SET-FR-WTOS: two quick moves of one slider both save, and the last one stays", async () => {
    const b = backend();
    b.holdSaves = true;
    await openGraduation();
    drag("pass-budget", 5);
    drag("pass-budget", 7);
    await waitFor(() => expect(b.gate).toHaveLength(1));
    b.gate.shift()!();
    // The first write landed; the control still shows the later value.
    await waitFor(() => expect(b.gate).toHaveLength(1));
    expect(slider("pass-budget")).toHaveAttribute("aria-valuetext", "7 passes");
    b.gate.shift()!();
    await waitFor(() => expect(b.saves).toHaveLength(2));
    expect(b.saves.map((s) => s.graduationPassBudget)).toEqual([5, 7]);
    expect(slider("pass-budget")).toHaveAttribute("aria-valuetext", "7 passes");
  });

  it("SET-FR-WTOS: a refused write keeps the previous value and says why inline", async () => {
    const b = backend({ graduationTotalTimeLimitMs: 8 * HALF_HOUR });
    b.refuse = "graduation_total_time_limit_invalid: the value is not a whole half hour";
    await openGraduation();
    drag("total-time-limit", 20 * HALF_HOUR);
    const error = await screen.findByTestId("graduation-total-time-limit-error");
    expect(error).toHaveAttribute("role", "alert");
    expect(error).toHaveTextContent(
      "The total time per run must be a whole number of half hours from 30 min to 96 h. Nothing was saved.",
    );
    expect(slider("total-time-limit")).toHaveAttribute("aria-valuetext", "4 h");
    expect(slider("total-time-limit").value).toBe(String(8 * HALF_HOUR));
    expect(b.saves).toHaveLength(0);
  });

  it("SET-FR-WTOS: a write that fails for another reason says the reason", async () => {
    const b = backend();
    b.refuse = "project.toml could not be written";
    await openGraduation();
    drag("pass-budget", 6);
    expect(await screen.findByTestId("graduation-pass-budget-error")).toHaveTextContent(
      "The setting was not saved: project.toml could not be written",
    );
    expect(slider("pass-budget")).toHaveAttribute("aria-valuetext", "2 passes");
  });

  it("SET-FR-KAXB: Reset to default clears the setting and the slider shows the default", async () => {
    const b = backend({ graduationStageTimeLimitMs: 4 * HALF_HOUR });
    await openGraduation();
    expect(slider("stage-time-limit")).toHaveAttribute("aria-valuetext", "2 h");
    const reset = screen.getByRole("button", { name: "Reset Time per turn to default" });
    expect(reset).toHaveTextContent("Reset to default");
    await userEvent.click(reset);
    await waitFor(() => expect(b.saves).toHaveLength(1));
    expect(b.saves[0]).toEqual({ lineEndings: "crlf", graduationStageTimeLimitMs: null });
    await waitFor(() =>
      expect(slider("stage-time-limit")).toHaveAttribute("aria-valuetext", "3 h"),
    );
    // Nothing more to reset.
    expect(reset).toBeDisabled();
  });

  it("SET-FR-OVTW: a stage limit higher than the total limit keeps both and warns", async () => {
    backend({ graduationStageTimeLimitMs: 20 * HALF_HOUR });
    await openGraduation();
    const warning = screen.getByTestId("graduation-stage-over-total");
    expect(warning).toHaveTextContent("The total time limit ends the run first.");
    expect(slider("stage-time-limit")).toHaveAttribute("aria-valuetext", "10 h");
    expect(slider("total-time-limit")).toHaveAttribute("aria-valuetext", "5 h");
    // The warning follows the shown values, also while a slider moves.
    fireEvent.change(slider("total-time-limit"), { target: { value: String(24 * HALF_HOUR) } });
    expect(screen.queryByTestId("graduation-stage-over-total")).toBeNull();
  });

  it("SET-FR-OVTW: equal limits do not warn", async () => {
    backend({ graduationStageTimeLimitMs: 10 * HALF_HOUR });
    await openGraduation();
    expect(screen.queryByTestId("graduation-stage-over-total")).toBeNull();
  });

  it("GRU-FR-QLRQ, SWN-FR-HZWD: the stage-time-limit address opens the section with Time per turn focused", async () => {
    backend();
    await openGraduation("project/graduation:stage-time-limit");
    await waitFor(() => expect(slider("stage-time-limit")).toHaveFocus());
  });

  it("GRU-FR-TTLR, SWN-FR-HZWD: the total-time-limit address opens the section with Total time per run focused", async () => {
    backend();
    await openGraduation("project/graduation:total-time-limit");
    await waitFor(() => expect(slider("total-time-limit")).toHaveFocus());
  });

  it("GRU-FR-TTLR: a later route to the section already on screen moves the focus", async () => {
    backend();
    const { view } = await openGraduation("project/graduation:stage-time-limit");
    await waitFor(() => expect(slider("stage-time-limit")).toHaveFocus());
    view.rerender(
      <Settings
        contentRoot="/p#0"
        lineEndings="crlf"
        onSelectLineEndings={vi.fn()}
        sectionRequest={{ section: "project/graduation:total-time-limit", nonce: 2 }}
      />,
    );
    await waitFor(() => expect(slider("total-time-limit")).toHaveFocus());
  });

  it("SWN-FR-HZWD: an address with a Graduation item presents the Graduation section", () => {
    expect(resolveSelection("project/graduation:total-time-limit", ["project"])).toEqual({
      scope: "project",
      section: "graduation",
    });
    expect(
      graduationFocusOf({ section: "project/graduation:stage-time-limit", nonce: 3 }, ["project"]),
    ).toEqual({ item: "stage-time-limit", nonce: 3 });
    expect(graduationFocusOf({ section: "project/graduation", nonce: 3 }, ["project"])).toBeNull();
    expect(
      graduationFocusOf({ section: "project/graduation:total-time-limit", nonce: 3 }, ["global"]),
    ).toBeNull();
    expect(graduationFocusOf({ section: "project/agents:new", nonce: 3 }, ["project"])).toBeNull();
  });

  it("SET-FR-RTDF: settings that cannot be read say so instead of sliders", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "append_log_records") return Promise.resolve();
      if (cmd === "load_project_config") return Promise.reject("project.toml is not valid TOML");
      return Promise.reject(new Error(`unexpected command ${cmd}`));
    });
    render(
      <Settings
        contentRoot="/p#0"
        lineEndings="crlf"
        onSelectLineEndings={vi.fn()}
        sectionRequest={{ section: "project/graduation", nonce: 1 }}
      />,
    );
    expect(await screen.findByTestId("graduation-loop-load-error")).toHaveTextContent(
      "project.toml is not valid TOML",
    );
    expect(screen.queryByTestId("graduation-loop-settings")).toBeNull();
  });
});

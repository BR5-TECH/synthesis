import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { invoke } from "@tauri-apps/api/core";

import { logError } from "../../logging";
import {
  loadAppPreferences,
  resetAppPreferencesCache,
} from "../../state/appPreferences";
import {
  makeObservability,
  makePass,
  makeRun,
} from "../../test/graduationFixtures";
import type { GraduationRun } from "../../types";
import { RunColumns } from "./columns";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

const invoked = vi.mocked(invoke);
/** What the backend holds. A save writes it, as the real store does. */
let stored: number | undefined;
let refuseSave: boolean;
/** A load the test releases itself, or null to answer at once. */
let heldLoad: Promise<void> | null;

beforeEach(() => {
  stored = undefined;
  refuseSave = false;
  heldLoad = null;
  invoked.mockReset();
  invoked.mockImplementation(async (command: string, args?: unknown) => {
    if (command === "load_app_preferences") {
      if (heldLoad) await heldLoad;
      return { graduationPathsWidthFraction: stored };
    }
    if (command === "save_app_preferences") {
      if (refuseSave) throw "storage_unavailable";
      stored = (args as { preferences: { graduationPathsWidthFraction?: number } })
        .preferences.graduationPathsWidthFraction;
    }
    return undefined;
  });
  vi.mocked(logError).mockClear();
  resetAppPreferencesCache();
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const commands = () => invoked.mock.calls.map(([command]) => command);
/** Every paths width a save carried, in order. */
const saves = () =>
  invoked.mock.calls
    .filter(([command]) => command === "save_app_preferences")
    .map(
      ([, args]) =>
        (args as { preferences: { graduationPathsWidthFraction: number } }).preferences
          .graduationPathsWidthFraction,
    );
/** Let every queued preference write settle, so a "no save" check can fail. */
const settle = () => act(async () => new Promise((resolve) => setTimeout(resolve, 0)));
const divider = () => screen.getByTestId("graduation-columns-divider");
const columns = () => document.querySelector<HTMLElement>(".graduation__columns")!;

/** A run with both columns: one changed path and one pass. */
const both = (id = "r1") =>
  makeRun(id, "working", {
    checkpoint: { changedPaths: ["src/a.ts"] },
    observability: makeObservability({ passes: [makePass({ pass: 1 })] }),
  });
const pathsOnly = () =>
  makeRun("r1", "working", {
    checkpoint: { changedPaths: ["a.ts"] },
    observability: makeObservability({ passes: [] }),
  });

/** Columns 1000px wide, starting 100px from the window's left edge. */
function layOut() {
  vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    left: 100,
    width: 1000,
    top: 0,
    right: 1100,
    bottom: 100,
    height: 100,
    x: 100,
    y: 0,
    toJSON: () => ({}),
  } as DOMRect);
}
function pointer(type: string, clientX: number, button = 0) {
  divider().dispatchEvent(
    Object.assign(new Event(type, { bubbles: true }), { clientX, button, pointerId: 1 }),
  );
}
async function mount(run: GraduationRun = both()) {
  const view = render(<RunColumns run={run} />);
  await waitFor(() => expect(commands()).toContain("load_app_preferences"));
  await settle();
  return view;
}

describe("the paths column's width", () => {
  it("GRU-FR-KWRB, GRU-FR-ZPTE: a named separator, at 30 % where nothing is stored", async () => {
    await mount();
    expect(divider()).toHaveAttribute("role", "separator");
    expect(divider()).toHaveAttribute("aria-orientation", "vertical");
    expect(divider()).toHaveAttribute("aria-label", "Paths width");
    expect(divider()).toHaveAttribute("aria-valuenow", "30");
    expect(divider()).toHaveAttribute("aria-valuemin", "15");
    expect(divider()).toHaveAttribute("aria-valuemax", "70");
    expect(divider()).toHaveAttribute("aria-valuetext", "Paths width 30 percent");
    expect(columns().style.getPropertyValue("--graduation-paths")).toBe("30%");
    // Between the two columns, in reading order.
    expect(divider().previousElementSibling).toHaveClass("graduation__columns-paths");
    expect(divider().nextElementSibling).toHaveAttribute("data-testid", "graduation-passes");
  });

  it.each([
    { value: 0.95, shown: "70" },
    { value: 0.42, shown: "42" },
    { value: 0, shown: "15" },
    { value: 0.15, shown: "15" },
    { value: 0.7, shown: "70" },
  ])("GRU-FR-KWRB: a stored $value opens at $shown %", async ({ value, shown }) => {
    stored = value;
    await mount();
    expect(divider()).toHaveAttribute("aria-valuenow", shown);
    expect(columns().style.getPropertyValue("--graduation-paths")).toBe(`${shown}%`);
  });

  it("GRU-FR-KWRB: a width already read paints at once, with no frame at the default", async () => {
    stored = 0.5;
    await loadAppPreferences();
    render(<RunColumns run={both()} />);
    expect(divider()).toHaveAttribute("aria-valuenow", "50");
  });

  it("GRU-FR-KWRB, GSS-FR-QDNV: what the author sets is what the next mount reads", async () => {
    stored = 0.42;
    await mount();
    divider().focus();
    await userEvent.keyboard("{ArrowRight}");
    await settle();
    expect(stored).toBeCloseTo(0.43, 5);
    cleanup();
    resetAppPreferencesCache();
    await mount();
    expect(divider()).toHaveAttribute("aria-valuenow", "43");
  });

  it("GRU-FR-KWRB: the width is user-global, so a change of run keeps it", async () => {
    stored = 0.5;
    const view = await mount(both("r1"));
    view.rerender(<RunColumns run={both("r2")} />);
    expect(divider()).toHaveAttribute("aria-valuenow", "50");
    view.rerender(<RunColumns run={pathsOnly()} />);
    view.rerender(<RunColumns run={both("r3")} />);
    expect(divider()).toHaveAttribute("aria-valuenow", "50");
    await settle();
    expect(commands().filter((c) => c === "load_app_preferences")).toHaveLength(1);
    expect(saves()).toEqual([]);
  });

  it("GRU-FR-ZPTE: a width the author sets while the record loads is not taken back", async () => {
    stored = 0.42;
    let release!: () => void;
    heldLoad = new Promise((resolve) => (release = resolve));
    render(<RunColumns run={both()} />);
    divider().focus();
    await userEvent.keyboard("{ArrowRight}");
    await act(async () => release());
    await settle();
    expect(divider()).toHaveAttribute("aria-valuenow", "31");
    expect(saves()).toEqual([expect.closeTo(0.31, 5)]);
  });
});

describe("the column divider", () => {
  it("GRU-FR-ZPTE: the keyboard sets the width, and each change is stored", async () => {
    await mount();
    divider().focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(divider()).toHaveAttribute("aria-valuenow", "31");
    expect(columns().style.getPropertyValue("--graduation-paths")).toBe("31%");
    await userEvent.keyboard("{ArrowLeft}{ArrowLeft}");
    expect(divider()).toHaveAttribute("aria-valuenow", "29");
    await userEvent.keyboard("{Home}");
    expect(divider()).toHaveAttribute("aria-valuenow", "15");
    await userEvent.keyboard("{End}");
    expect(divider()).toHaveAttribute("aria-valuenow", "70");
    await settle();
    expect(saves()).toEqual([
      expect.closeTo(0.31, 5),
      expect.closeTo(0.3, 5),
      expect.closeTo(0.29, 5),
      expect.closeTo(0.15, 5),
      expect.closeTo(0.7, 5),
    ]);
  });

  it("GRU-FR-ZPTE: a step starts from the width shown, not from a stored value out of bounds", async () => {
    stored = 0.95;
    await mount();
    divider().focus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(divider()).toHaveAttribute("aria-valuenow", "69");
  });

  it("GRU-FR-ZPTE: a key the divider does not answer to changes and stores nothing", async () => {
    await mount();
    divider().focus();
    await userEvent.keyboard("{ArrowUp}");
    await settle();
    expect(divider()).toHaveAttribute("aria-valuenow", "30");
    expect(saves()).toEqual([]);
  });

  it("GRU-FR-ZPTE: a drag follows the pointer from where it took hold, and stores once, when it ends", async () => {
    await mount();
    layOut();
    // The boundary stands at 100 + 0.30 × 1000 = 400. The author takes hold
    // of the divider 8px right of it, and the boundary keeps that distance
    // from the pointer rather than jumping under it.
    pointer("pointerdown", 408);
    pointer("pointermove", 408);
    expect(divider()).toHaveAttribute("aria-valuenow", "30");
    for (const x of [420, 450, 480, 508]) pointer("pointermove", x);
    await waitFor(() => expect(divider()).toHaveAttribute("aria-valuenow", "40"));
    await settle();
    expect(saves()).toEqual([]);
    pointer("pointerup", 508);
    await settle();
    expect(saves()).toEqual([expect.closeTo(0.4, 5)]);
  });

  it("GRU-FR-ZPTE: a move with no drag under way changes and stores nothing", async () => {
    await mount();
    layOut();
    pointer("pointermove", 600);
    pointer("pointerup", 600);
    await settle();
    expect(divider()).toHaveAttribute("aria-valuenow", "30");
    expect(saves()).toEqual([]);
  });

  it("GRU-FR-ZPTE: a cancelled drag goes back to the stored width and stores nothing", async () => {
    await mount();
    layOut();
    pointer("pointerdown", 400);
    pointer("pointermove", 600);
    await waitFor(() => expect(divider()).toHaveAttribute("aria-valuenow", "50"));
    pointer("pointercancel", 600);
    await waitFor(() => expect(divider()).toHaveAttribute("aria-valuenow", "30"));
    pointer("pointermove", 700);
    pointer("pointerup", 700);
    await settle();
    expect(divider()).toHaveAttribute("aria-valuenow", "30");
    expect(saves()).toEqual([]);
  });

  it("GRU-FR-ZPTE: a button other than the primary one starts no drag", async () => {
    await mount();
    layOut();
    pointer("pointerdown", 400, 2);
    pointer("pointermove", 600);
    pointer("pointerup", 600);
    await settle();
    expect(divider()).toHaveAttribute("aria-valuenow", "30");
    expect(saves()).toEqual([]);
  });

  it("GRU-FR-ZPTE: a drag ends with the divider it was on", async () => {
    const view = await mount();
    layOut();
    pointer("pointerdown", 400);
    view.rerender(<RunColumns run={pathsOnly()} />);
    view.rerender(<RunColumns run={both("r2")} />);
    pointer("pointermove", 600);
    pointer("pointerup", 600);
    await settle();
    expect(divider()).toHaveAttribute("aria-valuenow", "30");
    expect(saves()).toEqual([]);
  });

  it("GRU-FR-ZPTE: a width that cannot be stored is logged, kept, and the next one still stored", async () => {
    await mount();
    refuseSave = true;
    divider().focus();
    await userEvent.keyboard("{ArrowRight}");
    await settle();
    expect(logError).toHaveBeenCalledWith(
      ["frontend"],
      "the paths column width could not be stored",
      {},
    );
    expect(divider()).toHaveAttribute("aria-valuenow", "31");
    refuseSave = false;
    await userEvent.keyboard("{ArrowRight}");
    await settle();
    expect(stored).toBeCloseTo(0.32, 5);
  });

  it.each([
    { name: "paths alone", run: pathsOnly },
    {
      name: "passes alone",
      run: () =>
        makeRun("r1", "working", {
          observability: makeObservability({ passes: [makePass({ pass: 1 })] }),
        }),
    },
  ])("GRU-FR-MCYF: a run with $name renders no divider", async ({ run }) => {
    await mount(run());
    expect(screen.queryByTestId("graduation-columns-divider")).toBeNull();
  });
});

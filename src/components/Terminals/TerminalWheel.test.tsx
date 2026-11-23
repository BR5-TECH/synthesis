// The mouse wheel of a terminal (`TRM-terminals.md` TRM-FR-QMXG, TRM-FR-PACR).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { fakeTerminals } from "../../test/ghosttyMock";
import { refreshTerminals } from "../../state/terminals";
import { terminalViewStarted } from "../../state/terminalViews";
import { MAX_WHEEL_REPORTS, handleTerminalWheel, newWheelTravel, wheelReport } from "../../state/terminalWheel";
import type { TerminalSummary } from "../../types";
import { TerminalTab } from "./TerminalTab";
import { commands, resetTerminalFixtures, serve, session } from "./terminalFixtures";

vi.mock("@tauri-apps/api/core", async () => {
  const f = await import("./terminalFixtures");
  return { invoke: f.invokeMock, Channel: f.FakeChannel };
});
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./terminalFixtures")).listenMock,
}));
vi.mock("../../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

/** An 80×24 grid of 10×20 pixel cells at (100, 50). */
const RECT = { left: 100, top: 50, width: 800, height: 480 };

function fakeTerm(over: { tracking?: boolean; sgr?: boolean } = {}) {
  return {
    cols: 80,
    rows: 24,
    hasMouseTracking: vi.fn(() => over.tracking ?? true),
    getMode: vi.fn((mode: number) => mode === 1006 && (over.sgr ?? true)),
    scrollLines: vi.fn(),
  };
}

function wheel(init: WheelEventInit & { clientX?: number; clientY?: number }) {
  return new WheelEvent("wheel", { clientX: 105, clientY: 55, deltaMode: 0, ...init });
}

describe("wheel reports", () => {
  it("TRM-FR-QMXG: the SGR form names the button, the 1-based cell, and Alt and Ctrl", () => {
    expect(wheelReport(true, 1, 1, { alt: false, ctrl: false }, true)).toBe("\x1b[<64;1;1M");
    expect(wheelReport(false, 80, 24, { alt: false, ctrl: false }, true)).toBe("\x1b[<65;80;24M");
    expect(wheelReport(true, 3, 4, { alt: true, ctrl: false }, true)).toBe("\x1b[<72;3;4M");
    expect(wheelReport(false, 3, 4, { alt: false, ctrl: true }, true)).toBe("\x1b[<81;3;4M");
    expect(wheelReport(true, 3, 4, { alt: true, ctrl: true }, true)).toBe("\x1b[<88;3;4M");
  });

  it("TRM-FR-QMXG: the legacy form is ESC [ M and three single bytes, and a cell past 95 sends nothing", () => {
    expect(wheelReport(true, 1, 1, { alt: false, ctrl: false }, false)).toBe("\x1b[M" + String.fromCharCode(96, 33, 33));
    const edge = wheelReport(false, 95, 2, { alt: true, ctrl: true }, false)!;
    expect(edge).toBe("\x1b[M" + String.fromCharCode(32 + 89, 32 + 95, 34));
    // Each value stays one byte in the UTF-8 that goes to the session.
    expect(new TextEncoder().encode(edge)).toHaveLength(6);
    expect(wheelReport(true, 96, 1, { alt: false, ctrl: false }, false)).toBeNull();
    expect(wheelReport(true, 1, 96, { alt: false, ctrl: false }, false)).toBeNull();
  });

  it("TRM-FR-QMXG: a wheel step goes to a program with mouse reporting, at the cell under the pointer", () => {
    const send = vi.fn();
    const term = fakeTerm();
    // Pointer at pixel (100+35, 50+45) is column 4, row 3.
    expect(handleTerminalWheel(wheel({ deltaY: -20, clientX: 135, clientY: 95 }), term, RECT, newWheelTravel(), send)).toBe(true);
    expect(send.mock.calls).toEqual([["\x1b[<64;4;3M"]]);
    expect(term.scrollLines).not.toHaveBeenCalled();
  });

  it("TRM-FR-QMXG: a pointer outside the cells reports the nearest edge cell", () => {
    const send = vi.fn();
    handleTerminalWheel(wheel({ deltaY: 20, clientX: 2000, clientY: -10 }), fakeTerm(), RECT, newWheelTravel(), send);
    expect(send.mock.calls).toEqual([["\x1b[<65;80;1M"]]);
  });

  it("TRM-FR-QMXG: without mouse reporting the library keeps the wheel", () => {
    const send = vi.fn();
    expect(handleTerminalWheel(wheel({ deltaY: 60 }), fakeTerm({ tracking: false }), RECT, newWheelTravel(), send)).toBe(false);
    expect(send).not.toHaveBeenCalled();
  });

  it("TRM-FR-PACR: small pixel deltas add up to one report for each cell height", () => {
    const send = vi.fn();
    const travel = newWheelTravel();
    const term = fakeTerm();
    for (let i = 0; i < 4; i += 1) expect(handleTerminalWheel(wheel({ deltaY: 4 }), term, RECT, travel, send)).toBe(true);
    expect(send).not.toHaveBeenCalled();
    handleTerminalWheel(wheel({ deltaY: 4 }), term, RECT, travel, send);
    expect(send.mock.calls).toEqual([["\x1b[<65;1;1M"]]);
  });

  it("TRM-FR-PACR: a change of direction drops the travel of the other direction", () => {
    const send = vi.fn();
    const travel = newWheelTravel();
    const term = fakeTerm();
    handleTerminalWheel(wheel({ deltaY: 15 }), term, RECT, travel, send);
    handleTerminalWheel(wheel({ deltaY: -15 }), term, RECT, travel, send);
    expect(send).not.toHaveBeenCalled();
    handleTerminalWheel(wheel({ deltaY: -5 }), term, RECT, travel, send);
    expect(send.mock.calls).toEqual([["\x1b[<64;1;1M"]]);
  });

  it("TRM-FR-PACR: one wheel event sends at most five reports, in lines and in pages too", () => {
    const send = vi.fn();
    const term = fakeTerm();
    handleTerminalWheel(wheel({ deltaY: 2000 }), term, RECT, newWheelTravel(), send);
    expect(send.mock.calls).toEqual([["\x1b[<65;1;1M".repeat(MAX_WHEEL_REPORTS)]]);
    send.mockClear();
    handleTerminalWheel(wheel({ deltaY: -3, deltaMode: 1 }), term, RECT, newWheelTravel(), send);
    expect(send.mock.calls).toEqual([["\x1b[<64;1;1M".repeat(3)]]);
    send.mockClear();
    handleTerminalWheel(wheel({ deltaY: 1, deltaMode: 2 }), term, RECT, newWheelTravel(), send);
    expect(send.mock.calls).toEqual([["\x1b[<65;1;1M".repeat(MAX_WHEEL_REPORTS)]]);
  });

  it("TRM-FR-PACR: Shift with the wheel scrolls the scrollback, also while mouse reporting is on", () => {
    const send = vi.fn();
    const term = fakeTerm();
    const travel = newWheelTravel();
    expect(handleTerminalWheel(wheel({ deltaY: -60, shiftKey: true }), term, RECT, travel, send)).toBe(true);
    // WebKit turns a Shift+wheel into a horizontal delta.
    expect(handleTerminalWheel(wheel({ deltaX: 40, shiftKey: true }), term, RECT, travel, send)).toBe(true);
    expect(term.scrollLines.mock.calls).toEqual([[-3], [2]]);
    expect(send).not.toHaveBeenCalled();
  });
});

describe("wheel in a terminal tab", () => {
  beforeEach(() => {
    resetTerminalFixtures();
    serve("attach_terminal", { state: "running", historyBytes: 0 });
    serve("acknowledge_terminal_output", null);
    serve("resize_terminal", null);
    serve("detach_terminal", null);
    serve("write_terminal_input", null);
  });
  afterEach(() => cleanup());

  async function open(list: TerminalSummary[]) {
    serve("list_terminals", () => list);
    await act(() => refreshTerminals());
    render(<TerminalTab terminalId="a" />);
    await act(() => terminalViewStarted("a"));
    return fakeTerminals[fakeTerminals.length - 1];
  }

  it("TRM-FR-QMXG: the tab sends the wheel report of a mouse-aware program to the session", async () => {
    const term = await open([session({ id: "a" })]);
    term.mouseTracking = true;
    term.modes.add(1006);
    expect(term.wheelHandler?.(wheel({ deltaY: -1, deltaMode: 1 }))).toBe(true);
    // jsdom lays out nothing, so the cell is a clamped edge cell.
    await waitFor(() => expect(commands("write_terminal_input")).toHaveLength(1));
    expect(commands("write_terminal_input")[0].args.data).toMatch(/^\x1b\[<64;\d+;\d+M$/);
  });

  it("TRM-FR-PACR: an ended session receives no wheel report", async () => {
    const term = await open([session({ id: "a", state: "exited", exitCode: 0, hasHistory: true })]);
    term.mouseTracking = true;
    term.wheelHandler?.(wheel({ deltaY: -1, deltaMode: 1 }));
    expect(commands("write_terminal_input")).toHaveLength(0);
  });
});

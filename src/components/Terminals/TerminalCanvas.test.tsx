// The text size of an open terminal tab after a lost canvas state (`TRM-terminals.md` TRM-FR-KZPU, TRM-FR-LNOZ).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { fakeNoRenderHook, fakeNoRenderer, fakeTerminals, type FakeTerminal } from "../../test/ghosttyMock";
import { refreshTerminals, upsertTerminal } from "../../state/terminals";
import { terminalViewStarted } from "../../state/terminalViews";
import { TerminalTab } from "./TerminalTab";
import { commands, resetTerminalFixtures, serve, session } from "./terminalFixtures";
import { logInfo, logWarn } from "../../logging";

vi.mock("@tauri-apps/api/core", async () => {
  const f = await import("./terminalFixtures");
  return { invoke: f.invokeMock, Channel: f.FakeChannel };
});
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./terminalFixtures")).listenMock,
}));
vi.mock("../../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

async function open(id: string, theme: string) {
  serve("list_terminals", () => [session({ id, theme: theme as never })]);
  await act(() => refreshTerminals());
  render(<TerminalTab terminalId={id} />);
  await act(() => terminalViewStarted(id));
}

function termsOf(id: string): FakeTerminal[] {
  return fakeTerminals.filter((t) => (t.opened as HTMLElement | null)?.dataset.terminalId === id);
}

const ORIGINAL_RATIO = Object.getOwnPropertyDescriptor(window, "devicePixelRatio");

function setRatio(ratio: number) {
  Object.defineProperty(window, "devicePixelRatio", { configurable: true, value: ratio });
}

function restoreRatio() {
  if (ORIGINAL_RATIO) Object.defineProperty(window, "devicePixelRatio", ORIGINAL_RATIO);
  else delete (window as { devicePixelRatio?: number }).devicePixelRatio;
}

const repairs = () => vi.mocked(logInfo).mock.calls.filter((c) => c[1] === "terminal canvas repaired");

describe("the canvas of an open terminal tab", () => {
  beforeEach(() => {
    resetTerminalFixtures();
    serve("attach_terminal", { state: "running", historyBytes: 0 });
    serve("acknowledge_terminal_output", null);
    serve("resize_terminal", null);
    serve("detach_terminal", null);
    serve("write_terminal_input", null);
    vi.mocked(logInfo).mockClear();
    vi.mocked(logWarn).mockClear();
    // A Retina display: a lost state draws the text at half its size.
    setRatio(2);
  });
  afterEach(() => {
    cleanup();
    restoreRatio();
  });

  it("TRM-FR-KZPU, TRM-FR-LNOZ: a terminal whose canvas state is lost draws at its scale again on the next frame", async () => {
    await open("a", "paper");
    const [term] = termsOf("a");
    const renderer = term.renderer!;
    renderer.frame(term);
    renderer.ctx.resetTransform();
    renderer.frame(term);
    expect(renderer.ctx.getTransform()).toEqual({ a: 2, d: 2 });
    expect(renderer.rendered[renderer.rendered.length - 1]).toBe(true);
    expect(repairs()).toEqual([[["frontend"], "terminal canvas repaired", { terminalId: "a", reason: "state_lost", ratio: 2 }]]);
  });

  it("TRM-FR-KZPU: a terminal made again for a theme change keeps its scale too", async () => {
    await open("a", "paper");
    act(() => upsertTerminal(session({ id: "a", theme: "sand" })));
    await waitFor(() => expect(commands("attach_terminal")).toHaveLength(2));
    await act(() => terminalViewStarted("a"));
    const terms = termsOf("a");
    expect(terms).toHaveLength(2);
    const renderer = terms[1].renderer!;
    renderer.frame(terms[1]);
    renderer.ctx.resetTransform();
    renderer.frame(terms[1]);
    expect(renderer.ctx.getTransform()).toEqual({ a: 2, d: 2 });
    expect(renderer.resized[renderer.resized.length - 1]).toEqual({ cols: terms[1].cols, rows: terms[1].rows });
    expect(repairs()).toHaveLength(1);
  });

  it("TRM-FR-LNOZ: a terminal library without the render hook gives one WARN for the terminal, also over remakes", async () => {
    fakeNoRenderHook.on = true;
    await open("a", "paper");
    act(() => upsertTerminal(session({ id: "a", theme: "sand" })));
    await waitFor(() => expect(commands("attach_terminal")).toHaveLength(2));
    await act(() => terminalViewStarted("a"));
    const warns = vi.mocked(logWarn).mock.calls.filter((c) => c[1] === "terminal canvas scale not kept");
    expect(warns).toEqual([[["frontend"], "terminal canvas scale not kept", { terminalId: "a", reason: "no_renderer_hook" }]]);
    expect(vi.mocked(logWarn).mock.calls.filter((c) => c[1] === "terminal contrast not applied")).toEqual([]);
  });

  it("TRM-FR-LNOZ: a terminal library with no renderer gives one WARN for each requirement and each terminal", async () => {
    fakeNoRenderer.on = true;
    await open("a", "paper");
    cleanup();
    await open("b", "paper");
    const warns = vi.mocked(logWarn).mock.calls.map((c) => [c[1], (c[2] as { terminalId: string }).terminalId]);
    expect(warns.sort()).toEqual([
      ["terminal canvas scale not kept", "a"],
      ["terminal canvas scale not kept", "b"],
      ["terminal contrast not applied", "a"],
      ["terminal contrast not applied", "b"],
    ]);
  });
});

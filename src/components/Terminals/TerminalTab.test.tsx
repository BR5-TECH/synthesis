// The Terminal tab and its terminal (`TRM-terminals.md` TRM-FR-XBVB …
// TRM-FR-XVFQ). `ghostty-web` is the recording fake of `src/test/ghosttyMock.ts`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { fakeTerminals } from "../../test/ghosttyMock";
import { refreshTerminals } from "../../state/terminals";
import {
  TERMINAL_SCROLLBACK_BYTES,
  TERMINAL_SCROLLBACK_LINES,
  handleTerminalKey,
  hasTerminalView,
  inputParts,
  keyStaysInTerminal,
  releaseTerminalView,
  terminalViewStarted,
} from "../../state/terminalViews";
import type { TerminalSummary } from "../../types";
import { TerminalTab } from "./TerminalTab";
import { FakeChannel, commands, resetTerminalFixtures, serve, session } from "./terminalFixtures";

vi.mock("@tauri-apps/api/core", async () => {
  const f = await import("./terminalFixtures");
  return { invoke: f.invokeMock, Channel: f.FakeChannel };
});
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./terminalFixtures")).listenMock,
}));
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

beforeEach(() => {
  resetTerminalFixtures();
  serve("attach_terminal", { state: "running", historyBytes: 0 });
  serve("acknowledge_terminal_output", null);
  serve("resize_terminal", null);
  serve("detach_terminal", null);
  serve("write_terminal_input", null);
});
afterEach(() => {
  // The page-wide values a test sets are put back even when its assertions fail.
  const root = document.documentElement;
  for (const name of ["--font-source-family", "--font-source-size", "--bg-canvas"]) root.style.removeProperty(name);
  delete root.dataset.theme;
  vi.unstubAllGlobals();
  cleanup();
});

async function seed(list: TerminalSummary[]) {
  serve("list_terminals", () => list);
  await act(() => refreshTerminals());
}

async function mount(id = "a") {
  const view = render(<TerminalTab terminalId={id} />);
  await act(() => terminalViewStarted(id));
  return view;
}

describe("Terminal tab", () => {
  it("TRM-FR-XBVB: ghostty-web is pinned to an exact version", () => {
    const pkg = JSON.parse(readFileSync(resolve(__dirname, "../../../package.json"), "utf8"));
    expect(pkg.dependencies["ghostty-web"]).toMatch(/^\d+\.\d+\.\d+$/);
  });

  it("TRM-FR-AULS, TRM-FR-XBVB: opening the tab makes a terminal, attaches, writes frames and acknowledges them", async () => {
    await seed([session({ id: "a" })]);
    await mount();
    expect(fakeTerminals).toHaveLength(1);
    expect(commands("attach_terminal")[0].args.terminalId).toBe("a");
    act(() => FakeChannel.last!.push(new TextEncoder().encode("hello").buffer));
    expect(fakeTerminals[0].writtenText()).toBe("hello");
    await waitFor(() => expect(commands("acknowledge_terminal_output")[0]?.args).toEqual({ terminalId: "a", bytes: 5 }));
  });

  it("TRM-FR-WRVB, TRM-FR-DTCA: the terminal uses the Source code role, the app theme, and a 10,000-line scrollback", async () => {
    document.documentElement.style.setProperty("--font-source-family", "Fira Code");
    document.documentElement.style.setProperty("--font-source-size", "15px");
    document.documentElement.style.setProperty("--bg-canvas", "#101010");
    await seed([session({ id: "a" })]);
    await mount();
    const options = fakeTerminals[0].options as Record<string, any>;
    expect(options.fontFamily).toBe("Fira Code");
    expect(options.fontSize).toBe(15);
    expect(TERMINAL_SCROLLBACK_LINES).toBe(10_000);
    expect(options.scrollback).toBe(TERMINAL_SCROLLBACK_BYTES);
    expect(TERMINAL_SCROLLBACK_BYTES).toBeGreaterThanOrEqual(TERMINAL_SCROLLBACK_LINES * 160 * 13);
    expect(options.theme.background).toBe("#101010");
    document.documentElement.style.removeProperty("--font-source-family");
    document.documentElement.style.removeProperty("--font-source-size");
    document.documentElement.style.removeProperty("--bg-canvas");
  });

  it("TRM-FR-WRVB: a font-role change applies to an open tab at once", async () => {
    await seed([session({ id: "a" })]);
    await mount();
    await act(async () => {
      document.documentElement.style.setProperty("--font-source-size", "18px");
    });
    await waitFor(() => expect(fakeTerminals[0].options.fontSize).toBe(18));
    document.documentElement.style.removeProperty("--font-source-size");
  });

  it("TRM-FR-WRVB: a theme change makes the terminal again, and the new one attaches", async () => {
    await seed([session({ id: "a" })]);
    await mount();
    await act(async () => {
      document.documentElement.dataset.theme = "dark";
    });
    await waitFor(() => expect(commands("attach_terminal")).toHaveLength(2));
    expect(fakeTerminals[0].disposed).toBe(true);
    delete document.documentElement.dataset.theme;
  });

  it("TRM-FR-ILRQ: the tab resizes the session once for each new size", async () => {
    await seed([session({ id: "a" })]);
    await mount();
    expect(commands("resize_terminal").map((c) => c.args)).toEqual([{ terminalId: "a", cols: 80, rows: 24 }]);
    act(() => fakeTerminals[0].resize(100, 30));
    act(() => fakeTerminals[0].resize(100, 30));
    expect(commands("resize_terminal").map((c) => c.args)).toEqual([
      { terminalId: "a", cols: 80, rows: 24 },
      { terminalId: "a", cols: 100, rows: 30 },
    ]);
  });

  it("TRM-FR-ILRQ: a session that starts in a container gets the fitted size when it runs", async () => {
    await seed([session({ id: "a", state: "starting", runsIn: "docker" })]);
    await mount();
    expect(commands("resize_terminal")).toHaveLength(0);
    await seed([session({ id: "a", state: "running", runsIn: "docker" })]);
    await waitFor(() =>
      expect(commands("resize_terminal").map((c) => c.args)).toEqual([{ terminalId: "a", cols: 80, rows: 24 }]),
    );
  });

  it("TRM-FR-ITAK: a hidden tab stays attached and keeps writing output", async () => {
    await seed([session({ id: "a" })]);
    const view = await mount();
    view.unmount();
    expect(hasTerminalView("a")).toBe(true);
    expect(commands("detach_terminal")).toHaveLength(0);
    act(() => FakeChannel.last!.push(new TextEncoder().encode("later").buffer));
    expect(fakeTerminals[0].writtenText()).toBe("later");
    await mount();
    expect(fakeTerminals).toHaveLength(1);
    expect(commands("attach_terminal")).toHaveLength(1);
  });

  it("TRM-FR-JXLA: releasing the tab detaches the session and ends nothing", async () => {
    await seed([session({ id: "a" })]);
    await mount();
    releaseTerminalView("a");
    await waitFor(() => expect(commands("detach_terminal")).toHaveLength(1));
    expect(commands("close_terminal")).toHaveLength(0);
    expect(fakeTerminals[0].disposed).toBe(true);
  });

  it("TRM-FR-AULS, TRM-FR-FFPL: typing goes to a running session, and an exited one receives nothing", async () => {
    await seed([session({ id: "a" })]);
    await mount();
    act(() => fakeTerminals[0].type("ls\r"));
    expect(commands("write_terminal_input").map((c) => c.args)).toEqual([{ terminalId: "a", data: "ls\r" }]);
    await seed([session({ id: "a", state: "exited", exitCode: 3 })]);
    act(() => fakeTerminals[0].type("x"));
    expect(commands("write_terminal_input")).toHaveLength(1);
    expect(screen.getByRole("status")).toHaveTextContent("The process exited with code 3.");
  });

  it("TRM-FR-FFPL: a failed start shows its error line", async () => {
    await seed([session({ id: "a", state: "exited", endReason: "start_failed", endDetail: "image missing" })]);
    await mount();
    expect(screen.getByRole("status")).toHaveTextContent("The terminal did not start: image missing");
  });

  it("TRM-FR-GMGF: a starting session shows Starting over the start output", async () => {
    await seed([session({ id: "a", state: "starting" })]);
    await mount();
    expect(screen.getByRole("status")).toHaveTextContent("Starting");
    expect(document.querySelector("[data-fake-terminal]")).not.toBeNull();
  });

  it("TRM-FR-XVJT: a gone target keeps the tab as a read-only view with a banner", async () => {
    await seed([session({ id: "a", state: "target_gone", endReason: "target_gone" })]);
    await mount();
    expect(screen.getByText("Target gone.")).toBeInTheDocument();
    act(() => fakeTerminals[0].type("x"));
    expect(commands("write_terminal_input")).toHaveLength(0);
  });

  it("TRM-FR-HQDM: an exited session with no history says its output is gone", async () => {
    await seed([session({ id: "a", state: "exited", exitCode: 0, hasHistory: false })]);
    render(<TerminalTab terminalId="a" />);
    expect(screen.getByText("The output of this terminal is gone.")).toBeInTheDocument();
    expect(commands("attach_terminal")).toHaveLength(0);
  });

  it("TRM-FR-XVFQ: a busy target shows a persistent warning and input still goes through", async () => {
    await seed([session({ id: "a", busyRunId: "run-1" })]);
    await mount();
    expect(screen.getByRole("note")).toHaveTextContent("A graduation run is working in this target.");
    act(() => fakeTerminals[0].type("y"));
    expect(commands("write_terminal_input")).toHaveLength(1);
  });
});

describe("terminal keys and input", () => {
  const term = () => ({
    hasSelection: vi.fn(() => true),
    getSelection: vi.fn(() => "sel"),
    paste: vi.fn(),
    scrollPages: vi.fn(),
  });
  const key = (init: KeyboardEventInit) => new KeyboardEvent("keydown", init);

  it("TRM-FR-DHEM: Cmd+C copies the selection on macOS, Ctrl+Shift+C on Linux", () => {
    const writeText = vi.fn(() => Promise.resolve());
    vi.stubGlobal("navigator", { ...navigator, clipboard: { writeText, readText: vi.fn(() => Promise.resolve("")) } });
    const t = term();
    expect(handleTerminalKey(key({ code: "KeyC", metaKey: true }), t, vi.fn(), true)).toBe(true);
    expect(handleTerminalKey(key({ code: "KeyC", ctrlKey: true, shiftKey: true }), t, vi.fn(), false)).toBe(true);
    expect(writeText).toHaveBeenCalledTimes(2);
    expect(handleTerminalKey(key({ code: "KeyC", ctrlKey: true }), t, vi.fn(), false)).toBe(false);
    vi.unstubAllGlobals();
  });

  it("TRM-FR-DHEM: Cmd+V and Ctrl+Shift+V paste the clipboard", async () => {
    const readText = vi.fn(() => Promise.resolve("pasted"));
    vi.stubGlobal("navigator", { ...navigator, clipboard: { readText, writeText: vi.fn() } });
    const t = term();
    expect(handleTerminalKey(key({ code: "KeyV", metaKey: true }), t, vi.fn(), true)).toBe(true);
    expect(handleTerminalKey(key({ code: "KeyV", ctrlKey: true, shiftKey: true }), t, vi.fn(), false)).toBe(true);
    await waitFor(() => expect(t.paste).toHaveBeenCalledTimes(2));
    vi.unstubAllGlobals();
  });

  it("TRM-FR-RFVP: on Linux Ctrl+V goes to the shell, and Ctrl+letter keys stay in the terminal", () => {
    const send = vi.fn();
    expect(handleTerminalKey(key({ code: "KeyV", ctrlKey: true }), term(), send, false)).toBe(true);
    expect(send).toHaveBeenCalledWith("\x16");
    expect(keyStaysInTerminal(key({ code: "KeyK", ctrlKey: true }), false)).toBe(true);
    expect(keyStaysInTerminal(key({ code: "KeyW", ctrlKey: true }), false)).toBe(true);
    expect(keyStaysInTerminal(key({ code: "KeyC", ctrlKey: true, shiftKey: true }), false)).toBe(false);
    expect(keyStaysInTerminal(key({ code: "KeyC", ctrlKey: true }), true)).toBe(true);
    expect(keyStaysInTerminal(key({ code: "KeyK", metaKey: true }), true)).toBe(false);
  });

  it("TRM-FR-DTCA: Shift+PageUp and Shift+PageDown scroll the scrollback", () => {
    const t = term();
    expect(handleTerminalKey(key({ code: "PageUp", shiftKey: true }), t, vi.fn(), false)).toBe(true);
    expect(handleTerminalKey(key({ code: "PageDown", shiftKey: true }), t, vi.fn(), false)).toBe(true);
    expect(t.scrollPages.mock.calls).toEqual([[-1], [1]]);
  });

  it("TRM-FR-DHEM: a paste larger than 64 KiB goes in parts of at most 64 KiB", () => {
    const big = "é".repeat(40_000);
    const parts = inputParts(big);
    expect(parts.join("")).toBe(big);
    expect(parts.every((p) => new TextEncoder().encode(p).length <= 64 * 1024)).toBe(true);
    expect(inputParts("ls")).toEqual(["ls"]);
  });
});

describe("Terminal tab marker and logging", () => {
  it("TRM-FR-DPWA: an exited session and a gone target show a state marker; a running one shows none", async () => {
    const { TerminalTabMark } = await import("./TerminalTabMark");
    await seed([session({ id: "a", state: "exited" }), session({ id: "b", state: "target_gone" }), session({ id: "c" })]);
    render(
      <>
        <TerminalTabMark terminalId="a" />
        <TerminalTabMark terminalId="b" />
        <TerminalTabMark terminalId="c" />
      </>,
    );
    expect(screen.getByLabelText("exited")).toBeInTheDocument();
    expect(screen.getByLabelText("target gone")).toBeInTheDocument();
    expect(document.querySelectorAll(".tab__terminal-state")).toHaveLength(2);
  });

  it("TRM-FR-LNOZ: attach and detach emit DEBUG, a failed call a WARN, and no record carries input or output", async () => {
    const logging = await import("../../logging");
    vi.mocked(logging.logDebug).mockClear();
    vi.mocked(logging.logWarn).mockClear();
    await seed([session({ id: "a" })]);
    serve("write_terminal_input", () => {
      throw { code: "terminal_not_running" };
    });
    await mount();
    act(() => fakeTerminals[0].type("secret-input"));
    act(() => FakeChannel.last!.push(new TextEncoder().encode("secret-output").buffer));
    releaseTerminalView("a");
    await waitFor(() =>
      expect(logging.logDebug).toHaveBeenCalledWith(["frontend"], "terminal detached", { terminalId: "a" }),
    );
    expect(logging.logDebug).toHaveBeenCalledWith(["frontend"], "terminal attached", { terminalId: "a" });
    await waitFor(() =>
      expect(logging.logWarn).toHaveBeenCalledWith(["frontend"], "terminal input failed", {
        terminalId: "a",
        code: "terminal_not_running",
      }),
    );
    const all = JSON.stringify([...vi.mocked(logging.logDebug).mock.calls, ...vi.mocked(logging.logWarn).mock.calls]);
    expect(all).not.toContain("secret");
  });
});

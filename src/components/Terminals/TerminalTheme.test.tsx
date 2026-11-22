// A theme change in the Terminal tab (`TRM-terminals.md` TRM-FR-WRVB) and the
// acknowledgement of a frame that the terminal fails to write (TRM-FR-AULS).
// `ghostty-web` is the recording fake of `src/test/ghosttyMock.ts`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { fakeTerminals, fakeWriteFailure } from "../../test/ghosttyMock";
import { refreshTerminals } from "../../state/terminals";
import { releaseTerminalView, terminalViewStarted } from "../../state/terminalViews";
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

const DARK_BACKGROUND = "#16181d";
const LIGHT_BACKGROUND = "#ffffff";

beforeEach(() => {
  resetTerminalFixtures();
  serve("attach_terminal", { state: "running", historyBytes: 0 });
  serve("acknowledge_terminal_output", null);
  serve("resize_terminal", null);
  serve("detach_terminal", null);
  serve("write_terminal_input", null);
});
afterEach(() => {
  // The theme goes back before cleanup, and both run before the observer's
  // microtask. Each entry is then out of the page, so it only becomes stale
  // and starts no attach that reaches the next test.
  delete document.documentElement.dataset.theme;
  fakeWriteFailure.error = null;
  cleanup();
});

async function seed(list: TerminalSummary[]) {
  serve("list_terminals", () => list);
  await act(() => refreshTerminals());
}

async function mount(id: string) {
  const view = render(<TerminalTab terminalId={id} />);
  await act(() => terminalViewStarted(id));
  return view;
}

async function setTheme(theme: "dark" | "light") {
  await act(async () => {
    document.documentElement.dataset.theme = theme;
  });
}

function attaches(id: string) {
  return commands("attach_terminal").filter((c) => c.args.terminalId === id);
}

/** The channel of the `index`-th attach of `id`. */
function channel(id: string, index: number): FakeChannel<ArrayBuffer> {
  return attaches(id)[index].args.output as FakeChannel<ArrayBuffer>;
}

function terminalsOf(id: string) {
  return fakeTerminals.filter((t) => (t.opened as HTMLElement | null)?.dataset.terminalId === id);
}

function bytes(text: string): ArrayBuffer {
  return new TextEncoder().encode(text).buffer as ArrayBuffer;
}

describe("Terminal tab theme change", () => {
  it("TRM-FR-WRVB: a tab that shows is made again at once, in the page, with the new theme", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    await setTheme("dark");
    await waitFor(() => expect(attaches("a")).toHaveLength(2));
    const [old, next] = terminalsOf("a");
    expect(old.disposed).toBe(true);
    expect(next.openedConnected).toBe(true);
    expect((next.options.theme as Record<string, string>).background).toBe(DARK_BACKGROUND);
  });

  it("TRM-FR-WRVB, TRM-FR-ITAK: a theme change keeps the terminal of a hidden tab, which stays attached", async () => {
    await seed([session({ id: "a" }), session({ id: "b" })]);
    await mount("a");
    const b = await mount("b");
    b.unmount();
    await setTheme("dark");
    await waitFor(() => expect(attaches("a")).toHaveLength(2));
    expect(attaches("b")).toHaveLength(1);
    const [hidden] = terminalsOf("b");
    expect(terminalsOf("b")).toHaveLength(1);
    expect(hidden.disposed).toBe(false);
    expect((hidden.options.theme as Record<string, string>).background).toBe(LIGHT_BACKGROUND);
    act(() => channel("b", 0).push(bytes("still")));
    expect(hidden.writtenText()).toBe("still");
    await waitFor(() =>
      expect(commands("acknowledge_terminal_output").map((c) => c.args)).toContainEqual({ terminalId: "b", bytes: 5 }),
    );
  });

  it("TRM-FR-WRVB: a hidden tab gets the new theme when it shows next, before it writes more output", async () => {
    await seed([session({ id: "a" }), session({ id: "b" })]);
    await mount("a");
    const b = await mount("b");
    b.unmount();
    await setTheme("dark");
    await waitFor(() => expect(attaches("a")).toHaveLength(2));
    await mount("b");
    await waitFor(() => expect(attaches("b")).toHaveLength(2));
    const [old, next] = terminalsOf("b");
    expect(old.disposed).toBe(true);
    expect(next.openedConnected).toBe(true);
    expect((next.options.theme as Record<string, string>).background).toBe(DARK_BACKGROUND);
    act(() => channel("b", 1).push(bytes("fresh")));
    act(() => channel("b", 0).push(bytes("stale")));
    expect(next.writtenText()).toBe("fresh");
    expect(old.writtenText()).toBe("");
  });

  it("TRM-FR-WRVB: two theme changes while hidden make the terminal again once, with the current theme", async () => {
    await seed([session({ id: "a" }), session({ id: "b" })]);
    await mount("a");
    const b = await mount("b");
    b.unmount();
    await setTheme("dark");
    await waitFor(() => expect(attaches("a")).toHaveLength(2));
    await setTheme("light");
    await waitFor(() => expect(attaches("a")).toHaveLength(3));
    await mount("b");
    await waitFor(() => expect(attaches("b")).toHaveLength(2));
    expect(terminalsOf("b")).toHaveLength(2);
    expect((terminalsOf("b")[1].options.theme as Record<string, string>).background).toBe(LIGHT_BACKGROUND);
  });

  it("TRM-FR-WRVB, TRM-FR-ILRQ: the new terminal starts at the size of the old one, and an unchanged size is not sent again", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    act(() => fakeTerminals[0].resize(100, 30));
    await setTheme("dark");
    await waitFor(() => expect(attaches("a")).toHaveLength(2));
    const next = terminalsOf("a")[1];
    expect(next.options.cols).toBe(100);
    expect(next.options.rows).toBe(30);
    expect(commands("resize_terminal").map((c) => c.args)).toEqual([
      { terminalId: "a", cols: 80, rows: 24 },
      { terminalId: "a", cols: 100, rows: 30 },
    ]);
  });

  it("TRM-FR-WRVB: a hidden tab made again on show starts at its old size", async () => {
    await seed([session({ id: "a" }), session({ id: "b" })]);
    await mount("a");
    const b = await mount("b");
    act(() => terminalsOf("b")[0].resize(120, 40));
    b.unmount();
    await setTheme("dark");
    await waitFor(() => expect(attaches("a")).toHaveLength(2));
    await mount("b");
    await waitFor(() => expect(attaches("b")).toHaveLength(2));
    const next = terminalsOf("b")[1];
    expect([next.options.cols, next.options.rows]).toEqual([120, 40]);
    expect(commands("resize_terminal").filter((c) => c.args.terminalId === "b").map((c) => c.args)).toEqual([
      { terminalId: "b", cols: 80, rows: 24 },
      { terminalId: "b", cols: 120, rows: 40 },
    ]);
  });

  it("TRM-FR-WRVB, TRM-FR-JXLA: a hidden tab released after a theme change is not made again", async () => {
    await seed([session({ id: "a" }), session({ id: "b" })]);
    await mount("a");
    const b = await mount("b");
    b.unmount();
    await setTheme("dark");
    await waitFor(() => expect(attaches("a")).toHaveLength(2));
    releaseTerminalView("b");
    await waitFor(() => expect(commands("detach_terminal")).toHaveLength(1));
    expect(terminalsOf("b")[0].disposed).toBe(true);
    await setTheme("light");
    await waitFor(() => expect(attaches("a")).toHaveLength(3));
    expect(attaches("b")).toHaveLength(1);
    expect(terminalsOf("b")).toHaveLength(1);
  });
});

describe("Terminal tab write failure", () => {
  it("TRM-FR-AULS, TRM-FR-LNOZ: a frame the terminal fails to write is acknowledged, with one WARN for each terminal", async () => {
    const logging = await import("../../logging");
    for (const log of [logging.logDebug, logging.logInfo, logging.logWarn, logging.logError]) vi.mocked(log).mockClear();
    await seed([session({ id: "a" })]);
    await mount("a");
    fakeWriteFailure.error = new Error("Out of bounds memory access");
    act(() => FakeChannel.last!.push(bytes("secret-one")));
    act(() => FakeChannel.last!.push(bytes("secret-two!")));
    await waitFor(() =>
      expect(commands("acknowledge_terminal_output").map((c) => c.args)).toEqual([
        { terminalId: "a", bytes: 10 },
        { terminalId: "a", bytes: 11 },
      ]),
    );
    const warns = () => vi.mocked(logging.logWarn).mock.calls.filter((c) => c[1] === "terminal write failed");
    expect(warns()).toEqual([
      [["frontend"], "terminal write failed", { terminalId: "a", error: "Error: Out of bounds memory access" }],
    ]);

    fakeWriteFailure.error = null;
    act(() => FakeChannel.last!.push(bytes("ok")));
    expect(fakeTerminals[0].writtenText()).toBe("ok");
    await waitFor(() => expect(commands("acknowledge_terminal_output")).toHaveLength(3));

    await setTheme("dark");
    await waitFor(() => expect(attaches("a")).toHaveLength(2));
    fakeWriteFailure.error = new Error("Out of bounds memory access");
    act(() => channel("a", 1).push(bytes("secret-three")));
    await waitFor(() => expect(warns()).toHaveLength(2));
    await waitFor(() =>
      expect(commands("acknowledge_terminal_output").map((c) => c.args)).toContainEqual({ terminalId: "a", bytes: 12 }),
    );
    const all = [logging.logDebug, logging.logInfo, logging.logWarn, logging.logError].flatMap(
      (log) => vi.mocked(log).mock.calls,
    );
    expect(JSON.stringify(all)).not.toContain("secret");
  });
});

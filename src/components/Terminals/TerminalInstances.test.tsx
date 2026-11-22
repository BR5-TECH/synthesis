// Each terminal has its own `ghostty-web` WebAssembly instance
// (`TRM-terminals.md` TRM-FR-CGRR). `ghostty-web` is the recording fake of
// `src/test/ghosttyMock.ts`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { fakeInitCalls, fakeInstances, fakeLoadFailure, fakeLoadGate, fakeTerminals } from "../../test/ghosttyMock";
import { refreshTerminals } from "../../state/terminals";
import { releaseTerminalView, terminalViewStarted } from "../../state/terminalViews";
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
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

const DARK_BACKGROUND = "#16181d";

beforeEach(async () => {
  resetTerminalFixtures();
  serve("attach_terminal", { state: "running", historyBytes: 0 });
  serve("acknowledge_terminal_output", null);
  serve("resize_terminal", null);
  serve("detach_terminal", null);
  serve("write_terminal_input", null);
  const logging = await import("../../logging");
  for (const log of [logging.logDebug, logging.logInfo, logging.logWarn, logging.logError]) vi.mocked(log).mockClear();
});
afterEach(() => {
  delete document.documentElement.dataset.theme;
  fakeLoadFailure.error = null;
  fakeLoadGate.promise = null;
  fakeLoadGate.waiting = 0;
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

function terminalsOf(id: string) {
  return fakeTerminals.filter((t) => (t.opened as HTMLElement | null)?.dataset.terminalId === id);
}

/** Holds every `Ghostty.load()` that starts from now on, until `open()`. */
function holdLoads() {
  let open!: () => void;
  fakeLoadGate.promise = new Promise<void>((resolve) => {
    open = resolve;
  });
  return () => act(async () => open());
}

describe("Terminal instances", () => {
  it("TRM-FR-CGRR: each tab's terminal gets its own instance", async () => {
    await seed([session({ id: "a" }), session({ id: "b" })]);
    await mount("a");
    await mount("b");
    expect(fakeInstances).toHaveLength(2);
    const [a] = terminalsOf("a");
    const [b] = terminalsOf("b");
    expect(a.options.ghostty).toBe(fakeInstances[0]);
    expect(b.options.ghostty).toBe(fakeInstances[1]);
    expect(a.options.ghostty).not.toBe(b.options.ghostty);
    expect(fakeInitCalls.count).toBe(0);
  });

  it("TRM-FR-CGRR: a tab opened after another tab closed does not get the closed tab's instance", async () => {
    await seed([session({ id: "a" }), session({ id: "b" })]);
    const a = await mount("a");
    const [old] = terminalsOf("a");
    a.unmount();
    act(() => releaseTerminalView("a"));
    expect(old.disposed).toBe(true);
    await mount("b");
    const [next] = terminalsOf("b");
    expect(next.options.ghostty).toBeDefined();
    expect(next.options.ghostty).not.toBe(old.options.ghostty);
  });

  it("TRM-FR-CGRR, TRM-FR-WRVB: the terminal made again on a theme change gets a new instance", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    await setTheme("dark");
    await waitFor(() => expect(attaches("a")).toHaveLength(2));
    const [old, next] = terminalsOf("a");
    expect(old.disposed).toBe(true);
    expect(fakeInstances).toHaveLength(2);
    expect(next.options.ghostty).toBe(fakeInstances[1]);
    expect(next.options.ghostty).not.toBe(old.options.ghostty);
  });

  it("TRM-FR-CGRR, TRM-FR-LNOZ: a failed instance load makes no terminal, attaches nothing, and logs one WARN", async () => {
    const logging = await import("../../logging");
    fakeLoadFailure.error = new Error("WebAssembly.instantiate(): out of memory");
    await seed([session({ id: "a" })]);
    await mount("a");
    expect(terminalsOf("a")).toHaveLength(0);
    expect(attaches("a")).toHaveLength(0);
    expect(vi.mocked(logging.logWarn).mock.calls).toEqual([
      [
        ["frontend"],
        "terminal instance failed to load",
        { terminalId: "a", error: "Error: WebAssembly.instantiate(): out of memory" },
      ],
    ]);
  });

  it("TRM-FR-CGRR: a failed load does not stop the next tab, and a theme change makes the failed tab's terminal", async () => {
    fakeLoadFailure.error = new Error("load failed");
    await seed([session({ id: "a" }), session({ id: "b" })]);
    await mount("a");
    expect(terminalsOf("a")).toHaveLength(0);
    fakeLoadFailure.error = null;
    const b = await mount("b");
    expect(terminalsOf("b")).toHaveLength(1);
    b.unmount();
    await setTheme("dark");
    await waitFor(() => expect(attaches("a")).toHaveLength(1));
    expect(terminalsOf("a")).toHaveLength(1);
  });

  it("TRM-FR-CGRR, TRM-FR-JXLA: a tab released while its load is pending makes no terminal", async () => {
    await seed([session({ id: "a" })]);
    const open = holdLoads();
    const view = render(<TerminalTab terminalId="a" />);
    const started = terminalViewStarted("a");
    await waitFor(() => expect(fakeLoadGate.waiting).toBe(1));
    view.unmount();
    act(() => releaseTerminalView("a"));
    await open();
    await act(() => started);
    expect(fakeTerminals).toHaveLength(0);
    expect(attaches("a")).toHaveLength(0);
  });

  it("TRM-FR-CGRR, TRM-FR-WRVB: a theme change while the load is pending makes one terminal, with the new theme", async () => {
    await seed([session({ id: "a" })]);
    const open = holdLoads();
    render(<TerminalTab terminalId="a" />);
    await waitFor(() => expect(fakeLoadGate.waiting).toBe(1));
    await setTheme("dark");
    await waitFor(() => expect(fakeLoadGate.waiting).toBe(2));
    await open();
    await act(() => terminalViewStarted("a"));
    await waitFor(() => expect(attaches("a")).toHaveLength(1));
    const made = terminalsOf("a");
    expect(made).toHaveLength(1);
    expect((made[0].options.theme as Record<string, string>).background).toBe(DARK_BACKGROUND);
  });
});

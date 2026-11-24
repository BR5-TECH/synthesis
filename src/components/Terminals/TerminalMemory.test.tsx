// The memory cap of a terminal (`TRM-terminals.md` TRM-FR-VBOY, TRM-FR-PLUX)
// and what a disposed terminal keeps (TRM-FR-HQZN). `ghostty-web` is the
// recording fake of `src/test/ghosttyMock.ts`; a test sets the size of an
// instance's memory with its `memory` hook, so no test grows real memory.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import * as logging from "../../logging";
import {
  FakeGhostty,
  FakeTerminal,
  fakeInstances,
  fakeLoadGate,
  fakeTerminals,
  fakeWriteFailure,
} from "../../test/ghosttyMock";
import { refreshTerminals } from "../../state/terminals";
import {
  TERMINAL_MEMORY_CAP_BYTES,
  TERMINAL_REMAKE_MIN_OUTPUT_BYTES,
  hasTerminalView,
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

const MiB = 1024 * 1024;
const OVER = TERMINAL_MEMORY_CAP_BYTES + 64 * 1024;
// The buffers are made once: a test never allocates megabytes per frame.
const ENOUGH = new ArrayBuffer(TERMINAL_REMAKE_MIN_OUTPUT_BYTES);
const ONE_MIB = new ArrayBuffer(MiB);
const DARK_BACKGROUND = "#16181d";

beforeEach(() => {
  resetTerminalFixtures();
  for (const log of [logging.logDebug, logging.logInfo, logging.logWarn, logging.logError]) vi.mocked(log).mockClear();
  serve("attach_terminal", { state: "running", historyBytes: 0 });
  serve("acknowledge_terminal_output", null);
  serve("resize_terminal", null);
  serve("detach_terminal", null);
  serve("write_terminal_input", null);
});
afterEach(() => {
  delete document.documentElement.dataset.theme;
  fakeWriteFailure.error = null;
  fakeLoadGate.promise = null;
  vi.restoreAllMocks();
  cleanup();
  document.body.replaceChildren();
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

function channel(id: string, index: number): FakeChannel<ArrayBuffer> {
  return attaches(id)[index].args.output as FakeChannel<ArrayBuffer>;
}

function terminalsOf(id: string): FakeTerminal[] {
  return fakeTerminals.filter((t) => (t.opened as HTMLElement | null)?.dataset.terminalId === id);
}

function bytes(text: string): ArrayBuffer {
  return new TextEncoder().encode(text).buffer as ArrayBuffer;
}

function acks(id: string): number[] {
  return commands("acknowledge_terminal_output")
    .filter((c) => c.args.terminalId === id)
    .map((c) => c.args.bytes as number);
}

function remakeRecords() {
  return vi.mocked(logging.logInfo).mock.calls.filter((c) => c[1] === "terminal remade for memory");
}

/** Pushes a frame, then waits for any terminal that it made. */
async function push(id: string, index: number, frame: ArrayBuffer) {
  await act(async () => channel(id, index).push(frame));
  await act(() => terminalViewStarted(id));
}

function setMemory(instance: FakeGhostty, size: number) {
  instance.memory.buffer.byteLength = size;
}

describe("Terminal memory cap of a shown tab", () => {
  it("TRM-FR-VBOY, TRM-FR-AULS, TRM-FR-CGRR: a shown terminal past the cap is made again with its size and theme", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    const [old] = terminalsOf("a");
    act(() => old.resize(120, 40));
    const resizes = commands("resize_terminal").length;
    setMemory(fakeInstances[0], OVER);
    await push("a", 0, ENOUGH);

    expect(attaches("a")).toHaveLength(2);
    const [, next] = terminalsOf("a");
    expect(old.disposed).toBe(true);
    expect(next.openedConnected).toBe(true);
    expect([next.options.cols, next.options.rows]).toEqual([120, 40]);
    expect(next.options.theme).toEqual(old.options.theme);
    expect(fakeInstances).toHaveLength(2);
    expect(fakeInstances[1]).not.toBe(fakeInstances[0]);
    // TRM-FR-ILRQ: the session keeps its size, so the size is not sent again.
    expect(commands("resize_terminal")).toHaveLength(resizes);
    await push("a", 1, bytes("fresh"));
    expect(next.writtenText()).toBe("fresh");
  });

  it("TRM-FR-VBOY, TRM-FR-AULS: the frame that passes the cap is written and acknowledged before the remake", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    const [old] = terminalsOf("a");
    setMemory(fakeInstances[0], OVER);
    await push("a", 0, ENOUGH);
    expect(old.written).toHaveLength(1);
    expect(old.written[0].byteLength).toBe(ENOUGH.byteLength);
    expect(acks("a")).toEqual([ENOUGH.byteLength]);
  });

  it("TRM-FR-VBOY: one INFO record names the terminal and the memory size, and no output", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    setMemory(fakeInstances[0], OVER);
    await push("a", 0, bytes("secret ".repeat(10)));
    await push("a", 0, ENOUGH);
    expect(remakeRecords()).toEqual([
      [["frontend"], "terminal remade for memory", { terminalId: "a", memoryBytes: OVER, shown: true }],
    ]);
    const all = [logging.logDebug, logging.logInfo, logging.logWarn, logging.logError].flatMap(
      (log) => vi.mocked(log).mock.calls,
    );
    expect(JSON.stringify(all)).not.toContain("secret");
    expect(vi.mocked(logging.logDebug).mock.calls).toContainEqual([
      ["frontend"],
      "terminal rebuilt",
      { terminalId: "a", reason: "memory" },
    ]);
  });

  it("TRM-FR-VBOY: a memory of exactly the cap is kept; one page more makes the terminal again", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    setMemory(fakeInstances[0], TERMINAL_MEMORY_CAP_BYTES);
    await push("a", 0, ENOUGH);
    expect(attaches("a")).toHaveLength(1);
    expect(remakeRecords()).toHaveLength(0);
    setMemory(fakeInstances[0], TERMINAL_MEMORY_CAP_BYTES + 64 * 1024);
    await push("a", 0, bytes("x"));
    expect(attaches("a")).toHaveLength(2);
  });

  it("TRM-FR-VBOY: the cap applies only when the terminal received 8 MiB of output", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    setMemory(fakeInstances[0], OVER);
    for (let i = 0; i < 7; i += 1) await push("a", 0, ONE_MIB);
    await push("a", 0, new ArrayBuffer(MiB - 1));
    expect(attaches("a")).toHaveLength(1);
    await push("a", 0, bytes("x"));
    expect(attaches("a")).toHaveLength(2);
  });

  it("TRM-FR-VBOY: the history that a new terminal replays never makes it again", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    setMemory(fakeInstances[0], OVER);
    await push("a", 0, ENOUGH);
    setMemory(fakeInstances[1], OVER);
    for (let i = 0; i < 4; i += 1) await push("a", 1, ONE_MIB);
    expect(attaches("a")).toHaveLength(2);
  });

  it("TRM-FR-VBOY, TRM-FR-WRVB: a theme rebuild starts the output count of the new terminal at zero", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    for (let i = 0; i < 7; i += 1) await push("a", 0, ONE_MIB);
    await setTheme("dark");
    await act(() => terminalViewStarted("a"));
    expect(attaches("a")).toHaveLength(2);
    setMemory(fakeInstances[1], OVER);
    await push("a", 1, ONE_MIB);
    await push("a", 1, ONE_MIB);
    expect(attaches("a")).toHaveLength(2);
  });

  it("TRM-FR-VBOY, TRM-FR-AULS: a frame after the one that passes the cap is dropped by the old terminal", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    const [old] = terminalsOf("a");
    setMemory(fakeInstances[0], OVER);
    await act(async () => {
      channel("a", 0).push(ENOUGH);
      channel("a", 0).push(bytes("late"));
    });
    await act(() => terminalViewStarted("a"));
    expect(old.written).toHaveLength(1);
    expect(acks("a")).toEqual([ENOUGH.byteLength]);
  });

  it("TRM-FR-VBOY, TRM-FR-AULS: a terminal that fails its writes is made again too", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    setMemory(fakeInstances[0], OVER);
    fakeWriteFailure.error = new Error("wasm trap");
    await push("a", 0, ENOUGH);
    expect(acks("a")).toEqual([ENOUGH.byteLength]);
    expect(attaches("a")).toHaveLength(2);
    const warns = vi.mocked(logging.logWarn).mock.calls.filter((c) => c[1] === "terminal write failed");
    expect(warns).toHaveLength(1);
  });

  it("TRM-FR-VBOY: only the terminal that passes the cap is made again", async () => {
    await seed([session({ id: "a" }), session({ id: "b" })]);
    await mount("a");
    await mount("b");
    setMemory(fakeInstances[0], OVER);
    setMemory(fakeInstances[1], OVER);
    await push("b", 0, ENOUGH);
    expect(attaches("b")).toHaveLength(2);
    expect(attaches("a")).toHaveLength(1);
    expect(terminalsOf("a")[0].disposed).toBe(false);
  });

  it("TRM-FR-VBOY: a terminal whose instance has no readable memory is never made again", async () => {
    vi.spyOn(FakeGhostty, "load").mockImplementation(async () => {
      const instance = new FakeGhostty();
      delete (instance as Partial<FakeGhostty>).memory;
      fakeInstances.push(instance);
      return instance;
    });
    await seed([session({ id: "a" })]);
    await mount("a");
    await push("a", 0, ENOUGH);
    expect(attaches("a")).toHaveLength(1);
    expect(vi.mocked(logging.logDebug).mock.calls).toContainEqual([
      ["frontend"],
      "terminal memory not readable",
      { terminalId: "a" },
    ]);
  });

  it("TRM-FR-VBOY: a remake keeps the focus on an element outside the terminal", async () => {
    // The real library's open() puts the focus on its own text area.
    const open = FakeTerminal.prototype.open;
    vi.spyOn(FakeTerminal.prototype, "open").mockImplementation(function (this: FakeTerminal, parent: HTMLElement) {
      open.call(this, parent);
      const input = document.createElement("textarea");
      parent.appendChild(input);
      input.focus();
    });
    await seed([session({ id: "a" })]);
    await mount("a");
    const field = document.createElement("input");
    document.body.appendChild(field);
    field.focus();
    setMemory(fakeInstances[0], OVER);
    await push("a", 0, ENOUGH);
    expect(attaches("a")).toHaveLength(2);
    expect(document.activeElement).toBe(field);
  });
});

describe("Terminal memory cap of a hidden tab", () => {
  async function hiddenOverCap() {
    await seed([session({ id: "a" }), session({ id: "b" })]);
    await mount("a");
    const b = await mount("b");
    const [old] = terminalsOf("b");
    act(() => old.resize(100, 30));
    b.unmount();
    setMemory(fakeInstances[1], OVER);
    await push("b", 0, ENOUGH);
    return old;
  }

  it("TRM-FR-PLUX, TRM-FR-HQZN: a hidden terminal past the cap is disposed at once and made only when it shows", async () => {
    const old = await hiddenOverCap();
    expect(old.disposed).toBe(true);
    expect((old.opened as HTMLElement).children).toHaveLength(0);
    expect(attaches("b")).toHaveLength(1);
    expect(fakeInstances).toHaveLength(2);
    expect(remakeRecords()).toEqual([
      [["frontend"], "terminal remade for memory", { terminalId: "b", memoryBytes: OVER, shown: false }],
    ]);
  });

  it("TRM-FR-PLUX: frames that arrive while the terminal is disposed are acknowledged and not written", async () => {
    const old = await hiddenOverCap();
    const written = old.written.length;
    await push("b", 0, bytes("one"));
    await push("b", 0, bytes("three"));
    await push("b", 0, ENOUGH);
    expect(acks("b")).toEqual([ENOUGH.byteLength, 3, 5, ENOUGH.byteLength]);
    expect(old.written).toHaveLength(written);
    expect(remakeRecords()).toHaveLength(1);
    expect(vi.mocked(logging.logWarn)).not.toHaveBeenCalled();
  });

  it("TRM-FR-PLUX, TRM-FR-XBAP: a colour query that arrives while the terminal is disposed still gets an answer", async () => {
    await hiddenOverCap();
    await push("b", 0, bytes("\x1b]11;?\x07"));
    const replies = commands("write_terminal_input").filter((c) => c.args.terminalId === "b").map((c) => c.args.data);
    expect(replies).toHaveLength(1);
    expect(String(replies[0])).toMatch(/^\x1b\]11;rgb:[0-9a-f]{4}\/[0-9a-f]{4}\/[0-9a-f]{4}\x07$/);
  });

  it("TRM-FR-PLUX, TRM-FR-VBOY, TRM-FR-WRVB: the show makes the terminal again with its size and the current theme", async () => {
    await hiddenOverCap();
    await setTheme("dark");
    expect(attaches("b")).toHaveLength(1);
    await mount("b");
    expect(attaches("b")).toHaveLength(2);
    const [, next] = terminalsOf("b");
    expect(next.openedConnected).toBe(true);
    expect([next.options.cols, next.options.rows]).toEqual([100, 30]);
    expect((next.options.theme as Record<string, string>).background).toBe(DARK_BACKGROUND);
    expect(vi.mocked(logging.logDebug).mock.calls).toContainEqual([
      ["frontend"],
      "terminal rebuilt",
      { terminalId: "b", reason: "memory" },
    ]);
    const before = acks("b").length;
    await push("b", 1, bytes("fresh"));
    await push("b", 0, bytes("stale"));
    expect(next.writtenText()).toBe("fresh");
    expect(acks("b").slice(before)).toEqual([5]);
    expect(remakeRecords()).toHaveLength(1);
  });

  it("TRM-FR-PLUX, TRM-FR-JXLA: a tab closed while its terminal is disposed detaches once and makes nothing", async () => {
    await hiddenOverCap();
    act(() => releaseTerminalView("b"));
    expect(commands("detach_terminal").filter((c) => c.args.terminalId === "b")).toHaveLength(1);
    expect(hasTerminalView("b")).toBe(false);
    const before = acks("b").length;
    await push("b", 0, bytes("late"));
    expect(acks("b")).toHaveLength(before);
    await setTheme("dark");
    expect(attaches("b")).toHaveLength(1);
  });

  it("TRM-FR-PLUX, TRM-FR-WRVB: a tab hidden again while its terminal loads makes no terminal out of the page", async () => {
    await hiddenOverCap();
    let open!: () => void;
    fakeLoadGate.promise = new Promise((resolve) => (open = resolve));
    const shown = render(<TerminalTab terminalId="b" />);
    await waitFor(() => expect(fakeLoadGate.waiting).toBe(1));
    shown.unmount();
    fakeLoadGate.promise = null;
    await act(async () => open());
    await act(() => terminalViewStarted("b"));
    expect(terminalsOf("b").filter((t) => !t.openedConnected)).toHaveLength(0);
    expect(attaches("b")).toHaveLength(1);
    await mount("b");
    expect(attaches("b")).toHaveLength(2);
    const made = terminalsOf("b");
    expect(made[made.length - 1].openedConnected).toBe(true);
  });

  it("TRM-FR-WRVB: a tab hidden during its first load makes its terminal and attaches only when it shows", async () => {
    await seed([session({ id: "a" })]);
    let open!: () => void;
    fakeLoadGate.promise = new Promise((resolve) => (open = resolve));
    const first = render(<TerminalTab terminalId="a" />);
    await waitFor(() => expect(fakeLoadGate.waiting).toBe(1));
    first.unmount();
    fakeLoadGate.promise = null;
    await act(async () => open());
    await act(() => terminalViewStarted("a"));
    expect(terminalsOf("a")).toHaveLength(0);
    expect(attaches("a")).toHaveLength(0);
    await setTheme("dark");
    expect(attaches("a")).toHaveLength(0);
    await mount("a");
    expect(attaches("a")).toHaveLength(1);
    const made = terminalsOf("a");
    expect(made).toHaveLength(1);
    expect(made[0].openedConnected).toBe(true);
    expect((made[0].options.theme as Record<string, string>).background).toBe(DARK_BACKGROUND);
  });

  it("TRM-FR-HQZN: a disposed terminal leaves no key listener on its host", async () => {
    await seed([session({ id: "a" }), session({ id: "b" })]);
    await mount("a");
    const b = await mount("b");
    const [old] = terminalsOf("b");
    const host = old.opened as HTMLElement;
    b.unmount();
    document.body.appendChild(host);
    const reached = vi.fn();
    document.addEventListener("keydown", reached);
    try {
      host.dispatchEvent(new KeyboardEvent("keydown", { key: "a", code: "KeyA", bubbles: true }));
      expect(reached).not.toHaveBeenCalled();
      host.remove();
      setMemory(fakeInstances[1], OVER);
      await push("b", 0, ENOUGH);
      document.body.appendChild(host);
      host.dispatchEvent(new KeyboardEvent("keydown", { key: "a", code: "KeyA", bubbles: true }));
      expect(reached).toHaveBeenCalledTimes(1);
    } finally {
      document.removeEventListener("keydown", reached);
      host.remove();
    }
  });
});

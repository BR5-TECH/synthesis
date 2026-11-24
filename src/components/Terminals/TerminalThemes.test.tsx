// The five terminal themes and the theme that a terminal shows
// (`TRM-terminals.md` TRM-FR-CFXM, TRM-FR-PXDQ, TRM-FR-IUMC).
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";
import { act, cleanup, render, waitFor } from "@testing-library/react";
import { fakeTerminals } from "../../test/ghosttyMock";
import { refreshTerminals, upsertTerminal } from "../../state/terminals";
import { terminalViewStarted } from "../../state/terminalViews";
import {
  TERMINAL_THEMES,
  adoptStoredTerminalTheme,
  getTerminalThemePreference,
  resolveTerminalTheme,
  setTerminalThemePreference,
  subscribeTerminalThemePreference,
} from "../../state/terminalThemes";
import { contrastRatio, parseColor } from "../../state/terminalContrast";
import type { TerminalSummary } from "../../types";
import { TerminalTab } from "./TerminalTab";
import { FakeChannel, commands, resetTerminalFixtures, serve, session } from "./terminalFixtures";
import { fakeNoRenderer, type FakeTerminal } from "../../test/ghosttyMock";
import { logDebug, logWarn } from "../../logging";

vi.mock("@tauri-apps/api/core", async () => {
  const f = await import("./terminalFixtures");
  return { invoke: f.invokeMock, Channel: f.FakeChannel };
});
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./terminalFixtures")).listenMock,
}));
vi.mock("../../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

const ANSI = [
  "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
  "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightMagenta", "brightCyan", "brightWhite",
] as const;

function contrast(a: string, b: string): number {
  return contrastRatio(parseColor(a)!, parseColor(b)!);
}

describe("the five terminal themes", () => {
  it("TRM-FR-CFXM: three light themes and two dark themes, by id and name, the light ones first", () => {
    expect(TERMINAL_THEMES.map((t) => [t.id, t.name, t.dark])).toEqual([
      ["paper", "Paper", false],
      ["sand", "Sand", false],
      ["mist", "Mist", false],
      ["night", "Night", true],
      ["ember", "Ember", true],
    ]);
  });

  it("TRM-FR-PXDQ: each theme sets the four surface colours and the 16 ANSI colours", () => {
    for (const theme of TERMINAL_THEMES) {
      for (const key of ["background", "foreground", "cursor", "selectionBackground", ...ANSI] as const)
        expect(theme.palette[key], `${theme.id}.${key}`).toMatch(/^#[0-9a-f]{6}$/);
    }
  });

  it("TRM-FR-PXDQ: the foreground has 7:1 on the background, and each ANSI colour that is not exempt has 4.5:1", () => {
    for (const theme of TERMINAL_THEMES) {
      const bg = theme.palette.background;
      expect(contrast(theme.palette.foreground, bg), `${theme.id} foreground`).toBeGreaterThanOrEqual(7);
      const exempt = theme.dark ? ["black", "brightBlack"] : ["white", "brightWhite"];
      for (const key of ANSI) {
        if (exempt.includes(key)) continue;
        expect(contrast(theme.palette[key], bg), `${theme.id}.${key}`).toBeGreaterThanOrEqual(4.5);
      }
    }
  });
});

describe("the theme a terminal shows", () => {
  afterEach(() => {
    delete document.documentElement.dataset.theme;
  });

  it("TRM-FR-IUMC: the override comes first, then the Terminal theme of Settings, then the app theme", () => {
    expect(resolveTerminalTheme("ember", "paper").key).toBe("ember");
    expect(resolveTerminalTheme("ember", "paper").theme.background).toBe("#1b1512");
    expect(resolveTerminalTheme(null, "paper").key).toBe("paper");
    expect(resolveTerminalTheme(undefined, "app").key).toMatch(/^app:light:/);
    document.documentElement.dataset.theme = "dark";
    expect(resolveTerminalTheme(null, "app").key).toMatch(/^app:dark:/);
  });

  it("GLS-FR-VTVK, GSS-FR-FRBK: a stored value sets the preference, and a missing or unknown one reads as app", () => {
    const heard = vi.fn();
    const stop = subscribeTerminalThemePreference(heard);
    onTestFinished(stop);
    adoptStoredTerminalTheme({ terminalTheme: "sand" });
    expect(getTerminalThemePreference()).toBe("sand");
    adoptStoredTerminalTheme({ terminalTheme: "sand" });
    expect(heard).toHaveBeenCalledTimes(1);
    adoptStoredTerminalTheme({});
    expect(getTerminalThemePreference()).toBe("app");
    setTerminalThemePreference("neon" as never);
    expect(getTerminalThemePreference()).toBe("app");
    stop();
  });
});

describe("a theme change of an open terminal tab", () => {
  beforeEach(() => {
    resetTerminalFixtures();
    serve("attach_terminal", { state: "running", historyBytes: 0 });
    serve("acknowledge_terminal_output", null);
    serve("resize_terminal", null);
    serve("detach_terminal", null);
    serve("write_terminal_input", null);
  });
  afterEach(() => {
    delete document.documentElement.dataset.theme;
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

  const terminalsOf = (id: string) =>
    fakeTerminals.filter((t) => (t.opened as HTMLElement | null)?.dataset.terminalId === id);
  const background = (id: string) => {
    const all = terminalsOf(id);
    return (all[all.length - 1].options.theme as Record<string, string>).background;
  };
  const attaches = (id: string) => commands("attach_terminal").filter((c) => c.args.terminalId === id);

  it("TRM-FR-IUMC: a new terminal shows its override", async () => {
    await seed([session({ id: "a", theme: "night" })]);
    await mount("a");
    expect(background("a")).toBe("#10141c");
  });

  it("TRM-FR-IUMC: the tab's surface around the terminal has the terminal's background", async () => {
    await seed([session({ id: "a", theme: "sand" })]);
    const view = await mount("a");
    const surround = view.container.querySelector<HTMLElement>(".terminal-view")!;
    expect(surround.style.background).toBe("rgb(245, 238, 223)");
  });

  it("TRM-FR-IUMC, GLS-FR-VTVK: a change of the Terminal theme of Settings makes a shown terminal again with it", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    act(() => setTerminalThemePreference("mist"));
    await waitFor(() => expect(attaches("a")).toHaveLength(2));
    expect(background("a")).toBe("#edf1f4");
    expect(terminalsOf("a")[0].disposed).toBe(true);
  });

  it("TRM-FR-IUMC: a change of the override makes the terminal again, and an unrelated change of the list does not", async () => {
    const a = session({ id: "a" });
    await seed([a]);
    await mount("a");
    act(() => upsertTerminal({ ...a, label: "renamed" }));
    // A remake disposes the old terminal at once, before the new one is made.
    expect(terminalsOf("a")[0].disposed).toBe(false);
    expect(attaches("a")).toHaveLength(1);
    act(() => upsertTerminal({ ...a, theme: "ember" }));
    await waitFor(() => expect(attaches("a")).toHaveLength(2));
    expect(background("a")).toBe("#1b1512");
  });

  it("TRM-FR-IUMC: a terminal with an override keeps its theme when the Settings theme or the app theme changes", async () => {
    await seed([session({ id: "a", theme: "paper" })]);
    await mount("a");
    act(() => setTerminalThemePreference("night"));
    await act(async () => {
      document.documentElement.dataset.theme = "dark";
    });
    expect(terminalsOf("a")[0].disposed).toBe(false);
    expect(attaches("a")).toHaveLength(1);
    expect(terminalsOf("a")).toHaveLength(1);
  });

  it("TRM-FR-IUMC: a terminal that follows a Settings theme keeps it when the app theme changes", async () => {
    await seed([session({ id: "a" })]);
    await mount("a");
    act(() => setTerminalThemePreference("night"));
    await waitFor(() => expect(attaches("a")).toHaveLength(2));
    await act(async () => {
      document.documentElement.dataset.theme = "dark";
    });
    expect(terminalsOf("a")[1].disposed).toBe(false);
    expect(attaches("a")).toHaveLength(2);
  });

  it("TRM-FR-IUMC, TRM-FR-WRVB: a hidden tab gets the new theme when it shows next, not before", async () => {
    const b = session({ id: "b" });
    await seed([session({ id: "a" }), b]);
    await mount("a");
    const view = await mount("b");
    view.unmount();
    act(() => upsertTerminal({ ...b, theme: "night" }));
    expect(attaches("b")).toHaveLength(1);
    expect(terminalsOf("b")[0].disposed).toBe(false);
    await mount("b");
    await waitFor(() => expect(attaches("b")).toHaveLength(2));
    expect(background("b")).toBe("#10141c");
  });
});

describe("the colours that a program in an open terminal tab gets", () => {
  beforeEach(() => {
    resetTerminalFixtures();
    serve("acknowledge_terminal_output", null);
    serve("resize_terminal", null);
    serve("detach_terminal", null);
    serve("write_terminal_input", null);
  });
  afterEach(() => {
    delete document.documentElement.dataset.theme;
    cleanup();
  });

  async function open(id: string, theme: string, historyBytes = 0) {
    serve("attach_terminal", { state: "running", historyBytes });
    serve("list_terminals", () => [session({ id, theme: theme as never })]);
    await act(() => refreshTerminals());
    render(<TerminalTab terminalId={id} />);
    await act(() => terminalViewStarted(id));
    const all = commands("attach_terminal").filter((c) => c.args.terminalId === id);
    const attach = all[all.length - 1];
    const terms = fakeTerminals.filter((t) => (t.opened as HTMLElement | null)?.dataset.terminalId === id);
    const term = terms[terms.length - 1];
    return { channel: attach.args.output as FakeChannel<ArrayBuffer>, term };
  }

  const frame = (text: string) => new TextEncoder().encode(text).buffer as ArrayBuffer;
  const inputs = (id: string) =>
    commands("write_terminal_input").filter((c) => c.args.terminalId === id).map((c) => c.args.data);

  it("TRM-FR-XBAP: a live OSC 11 query gets the background of the theme as input of the session", async () => {
    const { channel } = await open("a", "paper");
    act(() => channel.push(frame("\x1b]11;?\x1b\\")));
    await waitFor(() => expect(inputs("a")).toEqual(["\x1b]11;rgb:fbfb/fafa/f7f7\x1b\\"]));
  });

  it("TRM-FR-XBAP: a query in the history that the attach sends first gets no answer", async () => {
    const history = "prompt\x1b]11;?\x07";
    const { channel } = await open("a", "night", history.length);
    act(() => channel.push(frame(history)));
    act(() => channel.push(frame("\x1b]10;?\x07")));
    await waitFor(() => expect(inputs("a")).toEqual(["\x1b]10;rgb:d9d9/dfdf/e8e8\x07"]));
  });

  it("TRM-FR-XBAP: the answer has the colours of the terminal's own theme, also a dark override on the light app theme", async () => {
    document.documentElement.dataset.theme = "light";
    const { channel } = await open("a", "ember");
    act(() => channel.push(frame("\x1b]11;?\x07\x1b]10;?\x07")));
    await waitFor(() =>
      expect(inputs("a")).toEqual(["\x1b]11;rgb:1b1b/1515/1212\x07", "\x1b]10;rgb:ecec/e1e1/d4d4\x07"]),
    );
  });

  it("TRM-FR-XBAP, TRM-FR-LNOZ: an ended session gets no answer and no DEBUG record of one", async () => {
    const { channel, term } = await open("a", "paper");
    await act(() => upsertTerminal(session({ id: "a", theme: "paper", state: "exited" })));
    vi.mocked(logDebug).mockClear();
    act(() => channel.push(frame("\x1b]11;?\x07")));
    // The frame reached the terminal, and still no answer went out.
    expect(term.writtenText()).toContain("\x1b]11;?\x07");
    expect(inputs("a")).toEqual([]);
    expect(vi.mocked(logDebug).mock.calls.filter((c) => c[1] === "terminal colour query answered")).toEqual([]);
  });

  async function openGated(id: string, theme: string) {
    let finish!: (value: unknown) => void;
    serve("attach_terminal", () => new Promise((resolve) => (finish = resolve)));
    serve("list_terminals", () => [session({ id, theme: theme as never })]);
    await act(() => refreshTerminals());
    render(<TerminalTab terminalId={id} />);
    await waitFor(() => expect(commands("attach_terminal")).toHaveLength(1));
    const channel = commands("attach_terminal")[0].args.output as FakeChannel<ArrayBuffer>;
    return { channel, finish: (value: unknown) => finish(value) };
  }

  it("TRM-FR-XBAP: queries that come before the attach answers wait for the history size", async () => {
    const { channel, finish } = await openGated("a", "paper");
    const history = "old\x1b]11;?\x07";
    act(() => channel.push(frame(history)));
    act(() => channel.push(frame("\x1b]10;?\x07")));
    expect(inputs("a")).toEqual([]);
    await act(async () => finish({ state: "running", historyBytes: history.length }));
    await waitFor(() => expect(inputs("a")).toEqual(["\x1b]10;rgb:1f1f/2121/2424\x07"]));
  });

  it("TRM-FR-XBAP: a terminal remade while its attach waits does not answer the queries of the old attach", async () => {
    const { channel, finish } = await openGated("a", "paper");
    act(() => channel.push(frame("\x1b]11;?\x07")));
    serve("attach_terminal", { state: "running", historyBytes: 0 });
    act(() => upsertTerminal(session({ id: "a", theme: "night" })));
    await waitFor(() => expect(commands("attach_terminal")).toHaveLength(2));
    await act(async () => finish({ state: "running", historyBytes: 0 }));
    const second = commands("attach_terminal")[1].args.output as FakeChannel<ArrayBuffer>;
    act(() => second.push(frame("\x1b]11;?\x07")));
    await waitFor(() => expect(inputs("a")).toEqual(["\x1b]11;rgb:1010/1414/1c1c\x07"]));
  });

  it("TRM-FR-XBAP: after a theme change the new attach's history replay gets no answer", async () => {
    const replay = "\x1b]11;?\x07";
    const { channel } = await open("a", "paper", replay.length);
    act(() => channel.push(frame(replay)));
    serve("attach_terminal", { state: "running", historyBytes: replay.length });
    act(() => upsertTerminal(session({ id: "a", theme: "night" })));
    await waitFor(() => expect(commands("attach_terminal")).toHaveLength(2));
    const second = commands("attach_terminal")[1].args.output as FakeChannel<ArrayBuffer>;
    await act(async () => {});
    act(() => second.push(frame(replay)));
    act(() => second.push(frame(replay)));
    await waitFor(() => expect(inputs("a")).toEqual(["\x1b]11;rgb:1010/1414/1c1c\x07"]));
  });

  it("TRM-FR-LNOZ: a terminal library without the contrast hook gives one WARN for the terminal, also over remakes", async () => {
    fakeNoRenderer.on = true;
    vi.mocked(logWarn).mockClear();
    await open("a", "paper");
    act(() => upsertTerminal(session({ id: "a", theme: "sand" })));
    await waitFor(() => expect(commands("attach_terminal")).toHaveLength(2));
    const warns = vi.mocked(logWarn).mock.calls.filter((c) => c[1] === "terminal contrast not applied");
    expect(warns).toEqual([[["frontend"], "terminal contrast not applied", { terminalId: "a", reason: "no_renderer_hook" }]]);
  });

  it("TRM-FR-DLJP: the terminal of a tab draws a light-blue cell with 4.5:1 on a light theme", async () => {
    const { term } = await open("a", "sand");
    const renderer = (term as FakeTerminal).renderer!;
    const cell = { fg_r: 177, fg_g: 185, fg_b: 249, bg_r: 0, bg_g: 0, bg_b: 0, flags: 0 };
    renderer.renderCellText(cell, 0, 0);
    const [drawn] = renderer.drawn;
    const hex = (c: number) => c.toString(16).padStart(2, "0");
    expect(contrast(`#${hex(drawn.fg_r)}${hex(drawn.fg_g)}${hex(drawn.fg_b)}`, "#f5eedf")).toBeGreaterThanOrEqual(4.5);
  });
});

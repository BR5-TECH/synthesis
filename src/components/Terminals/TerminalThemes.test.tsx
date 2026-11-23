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

const ANSI = [
  "black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
  "brightBlack", "brightRed", "brightGreen", "brightYellow", "brightBlue", "brightMagenta", "brightCyan", "brightWhite",
] as const;

function luminance(hex: string): number {
  const [r, g, b] = [1, 3, 5].map((i) => Number.parseInt(hex.slice(i, i + 2), 16) / 255);
  const channel = (c: number) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

function contrast(a: string, b: string): number {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
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

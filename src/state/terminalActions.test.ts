// The rename and the theme change of a terminal (`TRM-terminals.md`
// TRM-FR-LIRN, TRM-FR-QXIY, TRM-FR-LNOZ).
import { beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "@testing-library/react";
import { commands, resetTerminalFixtures, serve, session } from "../components/Terminals/terminalFixtures";
import { findTerminal, refreshTerminals } from "./terminals";
import { renameTerminal, setTerminalThemeOverride } from "./terminalActions";

vi.mock("@tauri-apps/api/core", async () => {
  const f = await import("../components/Terminals/terminalFixtures");
  return { invoke: f.invokeMock, Channel: f.FakeChannel };
});
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("../components/Terminals/terminalFixtures")).listenMock,
}));
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

beforeEach(async () => {
  resetTerminalFixtures();
  serve("list_terminals", () => [session({ id: "a", label: "bash" })]);
  await act(() => refreshTerminals());
});

describe("renameTerminal", () => {
  it("TRM-FR-LIRN: unchanged text after the trim makes no call", async () => {
    expect(await renameTerminal("a", "  bash  ")).toBeNull();
    expect(await renameTerminal("a", "\t ")).toBeNull();
    expect(commands("set_terminal_label")).toHaveLength(0);
  });

  it("TRM-FR-LIRN: each refusal gives a line for the field", async () => {
    serve("set_terminal_label", () => {
      throw { code: "unknown_terminal" };
    });
    expect(await renameTerminal("a", "x")).toBe("This terminal is gone.");
    serve("set_terminal_label", () => {
      throw new Error("boom");
    });
    expect(await renameTerminal("a", "x")).toBe("The terminal could not be renamed.");
    expect(findTerminal("a")?.label).toBe("bash");
  });

  it("TRM-FR-LIRN: a terminal that is not in the list is still renamed", async () => {
    serve("set_terminal_label", (args) => session({ id: "zz", label: String(args.label) }));
    expect(await renameTerminal("zz", "new")).toBeNull();
    expect(commands("set_terminal_label").map((c) => c.args)).toEqual([{ terminalId: "zz", label: "new" }]);
  });
});

describe("setTerminalThemeOverride", () => {
  it("TRM-FR-QXIY, TRM-FR-LNOZ: a theme change updates the list and emits DEBUG, with default for null", async () => {
    const logging = await import("../logging");
    vi.mocked(logging.logDebug).mockClear();
    serve("set_terminal_theme", (args) => session({ id: "a", label: "bash", theme: args.theme as never }));
    await setTerminalThemeOverride("a", "night");
    expect(findTerminal("a")?.theme).toBe("night");
    await setTerminalThemeOverride("a", null);
    expect(findTerminal("a")?.theme).toBeNull();
    expect(vi.mocked(logging.logDebug).mock.calls).toEqual([
      [["frontend"], "terminal theme changed", { terminalId: "a", theme: "night" }],
      [["frontend"], "terminal theme changed", { terminalId: "a", theme: "default" }],
    ]);
  });

  it("TRM-FR-LNOZ: a failed theme change emits WARN, throws nothing, and keeps the list", async () => {
    const logging = await import("../logging");
    vi.mocked(logging.logWarn).mockClear();
    serve("set_terminal_theme", () => {
      throw { code: "invalid_theme" };
    });
    await expect(setTerminalThemeOverride("a", "ember")).resolves.toBeUndefined();
    expect(findTerminal("a")?.theme).toBeUndefined();
    expect(logging.logWarn).toHaveBeenCalledWith(["frontend"], "terminal theme change failed", {
      terminalId: "a",
      code: "invalid_theme",
    });
  });
});

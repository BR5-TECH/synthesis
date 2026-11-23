// The Terminal theme selector of Settings → Appearance (`GLS-global-settings.md`
// GLS-FR-SOJD, GLS-FR-VTVK, `GSS-global-settings-storage.md` GSS-FR-FRBK).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { getTerminalThemePreference } from "../state/terminalThemes";
import type { AppPreferences } from "../types";
import { TerminalThemeSetting } from "./TerminalThemeSetting";

const saves: AppPreferences[] = [];
let stored: Partial<AppPreferences> = {};
let saveAnswer: (prefs: AppPreferences) => Promise<void> = () => Promise.resolve();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args: Record<string, unknown> = {}) => {
    if (cmd === "load_app_preferences") return Promise.resolve({ theme: "dark", editorZoomPercent: 120, ...stored });
    if (cmd === "save_app_preferences") {
      const prefs = args.preferences as AppPreferences;
      saves.push(prefs);
      return saveAnswer(prefs);
    }
    if (cmd === "append_log_records") return Promise.resolve();
    return Promise.reject(new Error(`unexpected ${cmd}`));
  },
}));
vi.mock("@tauri-apps/api/event", () => ({ emit: async () => {}, listen: async () => () => {} }));
vi.mock("../logging", () => ({ logDebug: vi.fn(), logInfo: vi.fn(), logWarn: vi.fn(), logError: vi.fn() }));

beforeEach(() => {
  resetAppPreferencesCache();
  saves.length = 0;
  stored = {};
  saveAnswer = () => Promise.resolve();
});
afterEach(cleanup);

const radio = (name: string | RegExp) => screen.getByRole("radio", { name });

async function renderLoaded() {
  render(<TerminalThemeSetting />);
  await waitFor(() => expect(radio(/Match app theme/)).toBeEnabled());
}

function deferred() {
  let resolve!: () => void;
  let reject!: (err: Error) => void;
  const promise = new Promise<void>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("Terminal theme selector", () => {
  it("GLS-FR-SOJD: Match app theme and the five themes, light themes first, each with a swatch; the default is checked", async () => {
    await renderLoaded();
    const group = screen.getByRole("radiogroup", { name: "Terminal theme" });
    const radios = screen.getAllByRole("radio");
    expect(radios.map((r) => r.textContent?.replace("~ $ ls", ""))).toEqual([
      "Match app theme",
      "Paper",
      "Sand",
      "Mist",
      "Night",
      "Ember",
    ]);
    expect(group.querySelectorAll(".terminal-theme-tile__screen")).toHaveLength(6);
    expect(radio(/Match app theme/)).toHaveAttribute("aria-checked", "true");
  });

  it("GLS-FR-SOJD: the selector reads the stored theme on mount", async () => {
    stored = { terminalTheme: "mist" };
    await renderLoaded();
    expect(radio(/Mist/)).toHaveAttribute("aria-checked", "true");
    expect(getTerminalThemePreference()).toBe("mist");
  });

  it("GSS-FR-FRBK: an unknown stored value reads as Match app theme", async () => {
    stored = { terminalTheme: "solarized" as never };
    await renderLoaded();
    expect(radio(/Match app theme/)).toHaveAttribute("aria-checked", "true");
    expect(getTerminalThemePreference()).toBe("app");
  });

  it("GLS-FR-VTVK, GSS-FR-20: a choice applies at once and saves at once, with the other fields unchanged", async () => {
    await renderLoaded();
    await userEvent.click(radio(/Night/));
    expect(getTerminalThemePreference()).toBe("night");
    expect(radio(/Night/)).toHaveAttribute("aria-checked", "true");
    await waitFor(() => expect(saves).toHaveLength(1));
    expect(saves[0]).toMatchObject({ terminalTheme: "night", theme: "dark", editorZoomPercent: 120 });
  });

  it("GLS-FR-VTVK: a failed save goes back to the stored theme and shows the error", async () => {
    const logging = await import("../logging");
    vi.mocked(logging.logWarn).mockClear();
    stored = { terminalTheme: "paper" };
    saveAnswer = () => Promise.reject(new Error("disk full"));
    await renderLoaded();
    await userEvent.click(radio(/Ember/));
    expect(await screen.findByRole("alert")).toHaveTextContent("disk full");
    expect(radio(/Paper/)).toHaveAttribute("aria-checked", "true");
    expect(getTerminalThemePreference()).toBe("paper");
    expect(logging.logWarn).toHaveBeenCalledWith(["frontend"], "terminal theme save failed", { theme: "ember" });
  });

  it("GLS-FR-VTVK: a failed older save does not undo a later choice that saved", async () => {
    const sand = deferred();
    const night = deferred();
    const answers = [sand.promise, night.promise];
    saveAnswer = () => answers.shift()!;
    await renderLoaded();
    await userEvent.click(radio(/Sand/));
    await userEvent.click(radio(/Night/));
    await act(async () => sand.reject(new Error("busy")));
    await waitFor(() => expect(saves).toHaveLength(2));
    await act(async () => night.resolve());
    expect(radio(/Night/)).toHaveAttribute("aria-checked", "true");
    expect(getTerminalThemePreference()).toBe("night");
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("GLS-FR-SOJD: the arrow keys move the choice and wrap at the ends", async () => {
    await renderLoaded();
    const group = screen.getByRole("radiogroup", { name: "Terminal theme" });
    fireEvent.keyDown(group, { key: "ArrowLeft" });
    expect(radio(/Ember/)).toHaveAttribute("aria-checked", "true");
    expect(radio(/Ember/)).toHaveFocus();
    fireEvent.keyDown(group, { key: "ArrowRight" });
    expect(radio(/Match app theme/)).toHaveAttribute("aria-checked", "true");
    await waitFor(() => expect(saves.map((s) => s.terminalTheme)).toEqual(["ember", "app"]));
  });
});

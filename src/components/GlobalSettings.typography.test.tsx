import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

import { GlobalSettings, resolveTheme } from "./GlobalSettings";
import { resetAppPreferencesCache } from "../state/appPreferences";
import {
  resetSettingsSections,
  runSettingsSaveSweep,
  settingsSectionsPending,
} from "../state/settingsSweep";
import type {
  AppPreferences,
  FontFamily,
  InstalledAdapter,
  InstalledPlugin,
  RecentProject,
} from "../types";

const invokeMock = vi.fn();
const promptMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

// `formatRelative` (imported transitively via ProjectPicker) pulls in
// @tauri-apps/api/window through that module; mock it so the import resolves
// under jsdom.
vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  }
  return {
    LogicalSize,
    getCurrentWindow: () => ({
      setResizable: async () => {},
      setMaximizable: async () => {},
      setSize: async () => {},
      isMaximized: async () => false,
      unmaximize: async () => {},
    }),
  };
});
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));

interface Backend {
  prefs?: AppPreferences;
  recents?: RecentProject[];
  plugins?: InstalledPlugin[];
  adapters?: InstalledAdapter[];
  /** FNT-FR-02: the installed families the Appearance controls offer. */
  fonts?: FontFamily[];
  overrides?: Record<string, (args?: Record<string, unknown>) => unknown>;
}

function wireBackend(b: Backend) {
  const state: Backend = {
    prefs: b.prefs ?? { theme: "system" },
    recents: b.recents ?? [],
    plugins: b.plugins ?? [],
    adapters: b.adapters ?? [],
    fonts: b.fonts ?? [],
  };
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) => {
      if (b.overrides?.[cmd]) return b.overrides[cmd](args);
      switch (cmd) {
        case "load_app_preferences":
          return state.prefs;
        case "save_app_preferences":
          // GSS-FR-20 is a whole-record write, so the stub keeps what it was
          // sent — a test that then re-reads sees what really landed rather
          // than what it hoped would.
          state.prefs = (args?.preferences as AppPreferences) ?? state.prefs;
          return undefined;
        case "list_system_fonts":
          return state.fonts;
        case "list_recent_projects":
          return state.recents;
        case "remove_recent_project":
        case "clear_recent_projects":
        case "pin_recent_project":
        case "unpin_recent_project":
          return undefined;
        case "list_installed_plugins":
          return state.plugins;
        case "install_plugin":
        case "uninstall_plugin":
          return { id: "x", name: "x", source: "s" };
        case "list_agent_adapters":
          return state.adapters;
        case "install_adapter":
          return { id: "x", name: "x", source: "s" };
        default:
          return undefined;
      }
    },
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  // The app-preferences record is cached at module scope (GSS-FR-20); reset it
  // so one test's persisted theme is not served to the next.
  resetAppPreferencesCache();
  promptMock.mockReset();
  vi.stubGlobal("prompt", promptMock);
  // Deterministic matchMedia for resolveTheme("system").
  vi.stubGlobal(
    "matchMedia",
    vi.fn().mockReturnValue({ matches: true }) as unknown,
  );
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

function calls(cmd: string) {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

// ---------------------------------------------------------------------------
// Typography (GLS-FR-17 – GLS-FR-23)
// ---------------------------------------------------------------------------

/** A machine carrying both kinds of family, ordered as FNT-FR-03 returns them. */
const INSTALLED: FontFamily[] = [
  { family: "Fira Code", monospace: true },
  { family: "Georgia", monospace: false },
  { family: "Helvetica Neue", monospace: false },
  { family: "JetBrains Mono", monospace: true },
];

/** The last record `save_app_preferences` was sent. */
function lastSavedPrefs(): AppPreferences | undefined {
  const saves = calls("save_app_preferences");
  return saves.length
    ? ((saves[saves.length - 1][1] as { preferences: AppPreferences })
        .preferences)
    : undefined;
}

const familySelect = (role: string) =>
  screen.getByLabelText(`${role} font family`) as HTMLSelectElement;
const sizeField = (role: string) =>
  screen.getByLabelText(`${role} font size`) as HTMLInputElement;
const lineHeightField = (role: string) =>
  screen.getByLabelText(`${role} line height`) as HTMLInputElement;

describe("Appearance typography (GLS-FR-17, GLS-FR-18 – GLS-FR-21)", () => {
  it("GLS-FR-17, GLS-FR-18: presents nine controls grouped as three roles, each reflecting its stored value", async () => {
    wireBackend({
      prefs: {
        theme: "dark",
        fonts: {
          ui: { family: "Helvetica Neue", sizePx: 12, lineHeight: 1.4 },
          rich: { family: "Georgia", sizePx: 17, lineHeight: 1.8 },
          source: { family: "Fira Code", sizePx: 14, lineHeight: 1.6 },
        },
      },
      fonts: INSTALLED,
    });
    render(<GlobalSettings />);

    // Three role groups (GLS-FR-17), and nothing else claiming to be one.
    for (const role of ["ui", "rich", "source"]) {
      expect(await screen.findByTestId(`font-role-${role}`)).toBeInTheDocument();
    }

    // Nine controls, each holding its stored value.
    await waitFor(() =>
      expect(familySelect("UI").value).toBe("Helvetica Neue"),
    );
    expect(sizeField("UI").value).toBe("12");
    expect(lineHeightField("UI").value).toBe("1.4");

    expect(familySelect("Rich Markdown").value).toBe("Georgia");
    expect(sizeField("Rich Markdown").value).toBe("17");
    expect(lineHeightField("Rich Markdown").value).toBe("1.8");

    expect(familySelect("Source code").value).toBe("Fira Code");
    expect(sizeField("Source code").value).toBe("14");
    expect(lineHeightField("Source code").value).toBe("1.6");

    // GLS-FR-18 / FNT-FR-01: the family controls are populated from
    // "list system fonts" — every enumerated family is offerable, alongside the
    // built-in face (FNT-FR-08), which is never enumerated.
    expect(calls("list_system_fonts")).toHaveLength(1);
    const options = within(familySelect("UI"))
      .getAllByRole("option")
      .map((o) => o.textContent);
    for (const f of INSTALLED) expect(options).toContain(f.family);
    expect(options.some((o) => /built-in/i.test(o ?? ""))).toBe(true);
  });

  it("GLS-FR-17, GLS-FR-18: a role with nothing stored shows the built-in default", async () => {
    // GSS-FR-04, GSS-FR-19, GSS-FR-21, GSS-FR-24, GSS-FR-25, GSS-FR-29, GSS-FR-32, GSS-FR-33, GSS-FR-34: a fresh machine has all nine unset, and "unset" must present
    // as the built-in face rather than as an empty control.
    wireBackend({ prefs: { theme: "system" }, fonts: INSTALLED });
    render(<GlobalSettings />);
    await waitFor(() => expect(familySelect("UI").value).toBe(""));
    expect(familySelect("UI").selectedOptions[0].textContent).toMatch(
      /built-in/i,
    );
    // The size and line-height fields show the built-in numbers, so the user
    // reads what the role *is* rather than a blank.
    expect(sizeField("Source code").value).toBe("13");
    expect(lineHeightField("Rich Markdown").value).toBe("1.65");
  });

  it("GLS-FR-20, GLS-FR-13: a family change persists at once, reports up for app-wide application, and leaves no dirty state", async () => {
    const onFontsChange = vi.fn();
    wireBackend({ prefs: { theme: "dark" }, fonts: INSTALLED });
    render(
      <GlobalSettings
        onFontsChange={onFontsChange}
      />,
    );
    await waitFor(() => expect(familySelect("Rich Markdown")).toBeEnabled());

    await userEvent.selectOptions(familySelect("Rich Markdown"), "Georgia");

    // Persisted at once — no Save control was involved (GLS-FR-20).
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    expect(lastSavedPrefs()?.fonts?.rich?.family).toBe("Georgia");
    expect(screen.queryByRole("button", { name: /^save$/i })).toBeNull();

    // Reported up so the app root re-typesets every open surface (OVW-FR-14).
    expect(onFontsChange).toHaveBeenLastCalledWith(
      expect.objectContaining({ rich: { family: "Georgia" } }),
    );

    // GLS-FR-20 / GLS-FR-13: the section never reports a dirty state, so
    // closing the tab shows no discard-unsaved-changes confirmation on its
    // account.
    // SWN-FR-08 / GLS-FR-13: nothing is saved on account of this section when
    // the window closes, because it holds nothing for the sweep to write.
    expect(settingsSectionsPending()).toEqual([]);
  });

  it("GLS-FR-20, GLS-FR-13 / GLS-FR-14: a font change carries every other field of the record through", async () => {
    // GSS-FR-20 is a whole-record write, so a bare `{ fonts }` save would clear
    // the window's full-screen state, the query mode, and both diff modes —
    // none of which this section offers a control for.
    wireBackend({
      prefs: {
        theme: "dark",
        mainWindowFullscreen: true,
        searchQueryMode: "regex",
        diffVisualizationMode: "side_by_side",
        diffRenderingMode: "rich",
        changesCommitAction: "commit_and_push",
        notificationsEnabled: true,
        selectionFollowsTab: true,
      graduationRailWidthFraction: 0.2,
      graduationPathsWidthFraction: 0.3,
      gitFilesWidthFraction: 0.3,
      },
      fonts: INSTALLED,
    });
    render(<GlobalSettings />);
    await waitFor(() => expect(familySelect("UI")).toBeEnabled());

    await userEvent.selectOptions(familySelect("UI"), "Helvetica Neue");

    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    const saved = lastSavedPrefs()!;
    expect(saved.fonts?.ui?.family).toBe("Helvetica Neue");
    expect(saved.theme).toBe("dark");
    expect(saved.mainWindowFullscreen).toBe(true);
    expect(saved.searchQueryMode).toBe("regex");
    expect(saved.diffVisualizationMode).toBe("side_by_side");
    expect(saved.diffRenderingMode).toBe("rich");
    expect(saved.changesCommitAction).toBe("commit_and_push");
  });

  it("GLS-FR-18: the Source code role presents fixed-width families first and marked, and still offers the rest", async () => {
    wireBackend({ prefs: { theme: "dark" }, fonts: INSTALLED });
    render(<GlobalSettings />);
    const select = await waitFor(() => familySelect("Source code"));

    const groups = select.querySelectorAll("optgroup");
    expect(groups).toHaveLength(2);
    // Marked as such, and first.
    expect(groups[0].label).toMatch(/fixed/i);
    expect(
      [...groups[0].querySelectorAll("option")].map((o) => o.value),
    ).toEqual(["Fira Code", "JetBrains Mono"]);
    // The proportional families are present below rather than withheld — a
    // family the platform describes imprecisely is still choosable for code.
    expect(
      [...groups[1].querySelectorAll("option")].map((o) => o.value),
    ).toEqual(["Georgia", "Helvetica Neue"]);

    // And selecting one is accepted and persisted like any other choice.
    await userEvent.selectOptions(select, "Georgia");
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    expect(lastSavedPrefs()?.fonts?.source?.family).toBe("Georgia");
  });

  it("GLS-FR-18: the other two roles do not group by width, because it says nothing about the choice", async () => {
    wireBackend({ prefs: { theme: "dark" }, fonts: INSTALLED });
    render(<GlobalSettings />);
    const select = await waitFor(() => familySelect("UI"));
    expect(select.querySelectorAll("optgroup")).toHaveLength(0);
    expect(
      within(select)
        .getAllByRole("option")
        .map((o) => (o as HTMLOptionElement).value)
        .filter(Boolean),
    ).toEqual(["Fira Code", "Georgia", "Helvetica Neue", "JetBrains Mono"]);
  });

  it("GLS-FR-19: a size or line height beyond the permitted range is not committed", async () => {
    const onFontsChange = vi.fn();
    wireBackend({ prefs: { theme: "dark" }, fonts: INSTALLED });
    render(<GlobalSettings onFontsChange={onFontsChange} />);
    const size = await waitFor(() => sizeField("UI"));
    // Mount reports the persisted record up so the root can apply it; that is
    // initialisation, not a commit, so clear it before asserting on commits.
    await waitFor(() => expect(onFontsChange).toHaveBeenCalled());
    onFontsChange.mockClear();

    await userEvent.clear(size);
    await userEvent.type(size, "400");

    // GLS-FR-19 has three clauses and this is all of them: the value is not
    // committed, `save_app_preferences` is not invoked with it, and the
    // application's text is unchanged.
    //
    // Asserted as "no save happened at all", not as "no save carried a value
    // over the maximum" — an implementation that silently clamped 400 to 32 and
    // persisted *that* would satisfy the weaker form while failing every clause
    // of the requirement, because the app's text would visibly change. The
    // digits typed on the way to 400 ("4", "40") are equally out of range, so a
    // correct implementation makes no call here whatsoever.
    expect(calls("save_app_preferences")).toHaveLength(0);
    expect(onFontsChange).not.toHaveBeenCalled();
    expect(size).toHaveAttribute("aria-invalid", "true");

    const lineHeight = lineHeightField("Source code");
    await userEvent.clear(lineHeight);
    await userEvent.type(lineHeight, "9");
    expect(calls("save_app_preferences")).toHaveLength(0);
    expect(onFontsChange).not.toHaveBeenCalled();
    expect(lineHeight).toHaveAttribute("aria-invalid", "true");
  });

  it("GLS-FR-19: a value passing through the invalid range on its way to a valid one is not committed", async () => {
    // Typing `18` puts `1` in the field first, which is below the minimum. A
    // field that committed each keystroke would re-typeset the whole app at 1px
    // — briefly unreadable, and persisted if the user stopped there.
    const onFontsChange = vi.fn();
    wireBackend({ prefs: { theme: "dark" }, fonts: INSTALLED });
    render(<GlobalSettings onFontsChange={onFontsChange} />);
    const size = await waitFor(() => sizeField("UI"));

    await userEvent.clear(size);
    await userEvent.type(size, "1");
    expect(calls("save_app_preferences")).toHaveLength(0);

    await userEvent.type(size, "8");
    await waitFor(() => expect(lastSavedPrefs()?.fonts?.ui?.sizePx).toBe(18));
    // One commit, for the value that was actually in range.
    expect(calls("save_app_preferences")).toHaveLength(1);
  });

  it("GLS-FR-19: the boundary values themselves are accepted", async () => {
    // The other edge of the bound: a range enforced one step too tight would
    // make the documented minimum and maximum unreachable.
    wireBackend({ prefs: { theme: "dark" }, fonts: INSTALLED });
    render(<GlobalSettings />);
    const size = await waitFor(() => sizeField("UI"));

    await userEvent.clear(size);
    await userEvent.type(size, "9");
    await waitFor(() => expect(lastSavedPrefs()?.fonts?.ui?.sizePx).toBe(9));
    expect(size).not.toHaveAttribute("aria-invalid");

    await userEvent.clear(size);
    await userEvent.type(size, "32");
    await waitFor(() => expect(lastSavedPrefs()?.fonts?.ui?.sizePx).toBe(32));
    expect(size).not.toHaveAttribute("aria-invalid");
  });

  it("GLS-FR-19: a value inside the range is committed", async () => {
    // The other half of the bound: the field must not be so strict that a
    // legitimate value cannot be set.
    wireBackend({ prefs: { theme: "dark" }, fonts: INSTALLED });
    render(<GlobalSettings />);
    const size = await waitFor(() => sizeField("Rich Markdown"));

    await userEvent.clear(size);
    await userEvent.type(size, "18");

    await waitFor(() =>
      expect(lastSavedPrefs()?.fonts?.rich?.sizePx).toBe(18),
    );
    expect(size).not.toHaveAttribute("aria-invalid");
  });

  it("GLS-FR-22: a stored family the machine no longer has stays the selection and is marked unavailable", async () => {
    // The font was chosen while installed and has since been uninstalled, so
    // it is absent from "list system fonts" (FNT-FR-09).
    const withoutFira = INSTALLED.filter((f) => f.family !== "Fira Code");
    wireBackend({
      prefs: { theme: "dark", fonts: { source: { family: "Fira Code" } } },
      fonts: withoutFira,
    });
    render(<GlobalSettings />);

    const select = await waitFor(() => familySelect("Source code"));
    // Still the stored selection — the absence alone rewrites nothing.
    await waitFor(() => expect(select.value).toBe("Fira Code"));
    expect(select.selectedOptions[0].textContent).toMatch(/not installed/i);
    expect(select).toHaveAttribute("data-unavailable", "true");
    expect(screen.getByText(/is not installed on this machine/i)).toBeVisible();

    // And nothing was persisted on account of the absence: reinstalling the
    // font has to restore the role with no reselection.
    expect(calls("save_app_preferences")).toHaveLength(0);
  });

  it("GLS-FR-22: an available stored family is not marked unavailable", async () => {
    wireBackend({
      prefs: { theme: "dark", fonts: { source: { family: "Fira Code" } } },
      fonts: INSTALLED,
    });
    render(<GlobalSettings />);
    const select = await waitFor(() => familySelect("Source code"));
    await waitFor(() => expect(select.value).toBe("Fira Code"));
    expect(select).not.toHaveAttribute("data-unavailable");
    expect(screen.queryByText(/not installed/i)).toBeNull();
  });

  it("FNT-FR-07, FNT-FR-08: a machine whose font set cannot be enumerated still renders the controls on the built-in faces", async () => {
    // The command yields an empty list rather than an error (FNT-FR-07), and
    // an empty list must not be read as "every stored family is missing" —
    // that would report our own blindness as the user's uninstalled font.
    wireBackend({
      prefs: { theme: "dark", fonts: { source: { family: "Fira Code" } } },
      fonts: [],
    });
    render(<GlobalSettings />);

    const select = await waitFor(() => familySelect("Source code"));
    expect(
      within(select)
        .getAllByRole("option")
        .some((o) => /built-in/i.test(o.textContent ?? "")),
    ).toBe(true);
    // The stored family is STILL the selection. A `<select>` whose value
    // matches no option falls back to the first one, so without an option of
    // its own the control would quietly read "built-in" while the record says
    // Fira Code — the control lying about what is stored, which is exactly what
    // GLS-FR-22 forbids.
    await waitFor(() => expect(select.value).toBe("Fira Code"));
    // But it is not accused of being uninstalled: we could not enumerate, which
    // is not the same as the font being gone.
    expect(select).not.toHaveAttribute("data-unavailable");
    expect(screen.queryByText(/not installed/i)).toBeNull();
    expect(screen.queryByText(/✗/)).not.toBeInTheDocument();
    // And nothing was rewritten on account of it.
    expect(calls("save_app_preferences")).toHaveLength(0);
  });

  it("GLS-FR-23, GLS-FR-20: resetting one role returns its three values and leaves the other two untouched", async () => {
    wireBackend({
      prefs: {
        theme: "dark",
        fonts: {
          ui: { family: "Helvetica Neue", sizePx: 12, lineHeight: 1.4 },
          rich: { family: "Georgia", sizePx: 17, lineHeight: 1.8 },
          source: { family: "Fira Code", sizePx: 14, lineHeight: 1.6 },
        },
      },
      fonts: INSTALLED,
    });
    render(<GlobalSettings />);
    await waitFor(() =>
      expect(familySelect("Rich Markdown").value).toBe("Georgia"),
    );

    await userEvent.click(
      screen.getByRole("button", { name: "Reset Rich Markdown font" }),
    );

    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    const saved = lastSavedPrefs()!;
    expect(saved.fonts?.rich).toEqual({});
    expect(saved.fonts?.ui).toEqual({
      family: "Helvetica Neue",
      sizePx: 12,
      lineHeight: 1.4,
    });
    expect(saved.fonts?.source).toEqual({
      family: "Fira Code",
      sizePx: 14,
      lineHeight: 1.6,
    });

    // The controls now read the built-in defaults for that role alone.
    await waitFor(() => expect(familySelect("Rich Markdown").value).toBe(""));
    expect(sizeField("Rich Markdown").value).toBe("15");
    expect(lineHeightField("Rich Markdown").value).toBe("1.65");
    expect(familySelect("UI").value).toBe("Helvetica Neue");
  });

  it("GLS-FR-23, GLS-FR-20: resetting all roles returns all nine values", async () => {
    wireBackend({
      prefs: {
        theme: "dark",
        fonts: {
          ui: { family: "Helvetica Neue", sizePx: 12 },
          rich: { family: "Georgia", sizePx: 17 },
          source: { family: "Fira Code", sizePx: 14 },
        },
      },
      fonts: INSTALLED,
    });
    render(<GlobalSettings />);
    await waitFor(() => expect(familySelect("UI").value).toBe("Helvetica Neue"));

    await userEvent.click(
      screen.getByRole("button", { name: /reset all fonts/i }),
    );

    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    expect(lastSavedPrefs()?.fonts).toEqual({});
    await waitFor(() => expect(familySelect("UI").value).toBe(""));
    expect(familySelect("Rich Markdown").value).toBe("");
    expect(familySelect("Source code").value).toBe("");
    expect(sizeField("UI").value).toBe("13");
  });

  it("GLS-FR-23: a role already on the built-in defaults offers nothing to reset, and keeps its focus when used", async () => {
    wireBackend({
      prefs: { theme: "dark", fonts: { ui: { sizePx: 16 } } },
      fonts: INSTALLED,
    });
    render(<GlobalSettings />);

    // A role that carries nothing has nothing to undo, and the control says so.
    const richReset = await screen.findByRole("button", {
      name: "Reset Rich Markdown font",
    });
    expect(richReset).toHaveAttribute("aria-disabled", "true");
    await userEvent.click(richReset);
    expect(calls("save_app_preferences")).toHaveLength(0);

    // The role that does carry something resets — and the button is still the
    // focused element afterwards. `disabled` would have removed it from the
    // tree's focusable set at the moment it was activated, dropping a keyboard
    // user back to the top of the document.
    const uiReset = screen.getByRole("button", { name: "Reset UI font" });
    expect(uiReset).not.toHaveAttribute("aria-disabled");
    uiReset.focus();
    await userEvent.click(uiReset);

    await waitFor(() => expect(lastSavedPrefs()?.fonts?.ui).toEqual({}));
    expect(uiReset).toHaveAttribute("aria-disabled", "true");
    expect(document.activeElement).toBe(uiReset);
  });

  it("GLS-FR-21: each role's specimen is set in that role's own values", async () => {
    wireBackend({
      prefs: {
        theme: "dark",
        fonts: {
          rich: { family: "Georgia", sizePx: 17, lineHeight: 1.8 },
          source: { family: "Fira Code", sizePx: 14 },
        },
      },
      fonts: INSTALLED,
    });
    render(<GlobalSettings />);

    const source = await screen.findByTestId("font-specimen-source");
    const rich = screen.getByTestId("font-specimen-rich");
    const ui = screen.getByTestId("font-specimen-ui");
    await waitFor(() => expect(source.style.fontFamily).toContain("Fira Code"));
    expect(source.style.fontSize).toBe("14px");
    expect(rich.style.fontFamily).toContain("Georgia");
    expect(rich.style.fontSize).toBe("17px");
    expect(rich.style.lineHeight).toBe("1.8");
    // The UI role carries nothing of its own, so its specimen shows the
    // built-in face at the built-in size.
    expect(ui.style.fontSize).toBe("13px");
    expect(ui.style.fontFamily).not.toContain("Georgia");

    // GLS-FR-21: changing one role re-renders that role's specimen and leaves
    // the other two alone.
    const size = sizeField("Source code");
    await userEvent.clear(size);
    await userEvent.type(size, "20");
    await waitFor(() =>
      expect(screen.getByTestId("font-specimen-source").style.fontSize).toBe(
        "20px",
      ),
    );
    expect(screen.getByTestId("font-specimen-rich").style.fontSize).toBe("17px");
    expect(screen.getByTestId("font-specimen-ui").style.fontSize).toBe("13px");
  });

  it("GLS-FR-20: a failed write rolls back to the record that is really stored", async () => {
    // The section holds no dirty state, so a value left applied but unsaved
    // would silently disagree with what a relaunch renders.
    //
    // The fixture carries a real prior selection rather than an empty record:
    // rolling back to `undefined` is indistinguishable from a correct rollback
    // when there was nothing stored, so an implementation that discarded the
    // user's existing fonts on every failed write would pass that version.
    const stored = {
      ui: { family: "Helvetica Neue", sizePx: 14, lineHeight: 1.45 },
      source: { family: "Fira Code" },
    };
    const onFontsChange = vi.fn();
    wireBackend({
      prefs: { theme: "dark" },
      fonts: INSTALLED,
      overrides: {
        load_app_preferences: () => ({ theme: "dark", fonts: stored }),
        list_system_fonts: () => INSTALLED,
        save_app_preferences: () => {
          throw "disk full";
        },
      },
    });
    render(<GlobalSettings onFontsChange={onFontsChange} />);
    await waitFor(() =>
      expect(familySelect("UI").value).toBe("Helvetica Neue"),
    );

    await userEvent.selectOptions(familySelect("UI"), "Georgia");

    expect(await screen.findByText(/disk full/)).toBeInTheDocument();
    // Rolled back to what is really stored — in the control, and in what the
    // app root was told to apply.
    await waitFor(() =>
      expect(familySelect("UI").value).toBe("Helvetica Neue"),
    );
    expect(onFontsChange).toHaveBeenLastCalledWith(stored);
    // And the role the user never touched is intact.
    expect(familySelect("Source code").value).toBe("Fira Code");
  });

  it("FNT-FR-07: an enumeration that rejects outright still leaves the controls usable", async () => {
    // The command is specified never to fail, but the bridge underneath it can
    // be absent entirely — a non-Tauri mount rejects before the command is
    // reached. The section must read that the same way it reads an empty list:
    // no family beyond the built-in faces is offerable.
    wireBackend({
      prefs: { theme: "dark", fonts: { rich: { family: "Georgia" } } },
      overrides: {
        load_app_preferences: () => ({
          theme: "dark",
          fonts: { rich: { family: "Georgia" } },
        }),
        list_system_fonts: () => {
          throw "no bridge";
        },
      },
    });
    render(<GlobalSettings />);

    const select = await waitFor(() => familySelect("Rich Markdown"));
    // The stored family is still the selection and is not accused of being
    // uninstalled — we could not enumerate, which is not the same as it being
    // gone.
    await waitFor(() => expect(select.value).toBe("Georgia"));
    expect(select).not.toHaveAttribute("data-unavailable");
    // The built-in face is offerable regardless (FNT-FR-08).
    expect(
      within(select)
        .getAllByRole("option")
        .some((o) => /built-in/i.test(o.textContent ?? "")),
    ).toBe(true);
    // And the failure is not reported as an error to the user.
    expect(screen.queryByText(/✗/)).not.toBeInTheDocument();
    expect(screen.queryByText(/no bridge/)).not.toBeInTheDocument();
  });

  it("GLS NFR: the typography controls render with no intermediate loading state", async () => {
    // "A machine carrying a large font set does not delay the section's first
    // paint." Asserted before any promise settles: every other test in this
    // file awaits, so the pre-resolution render is otherwise never observed.
    wireBackend({ prefs: { theme: "dark" }, fonts: INSTALLED });
    render(<GlobalSettings />);

    // Synchronously, on the very first paint — all nine controls are present
    // and offering the built-in faces.
    expect(screen.getByTestId("font-role-ui")).toBeInTheDocument();
    expect(screen.getByTestId("font-role-rich")).toBeInTheDocument();
    expect(screen.getByTestId("font-role-source")).toBeInTheDocument();
    expect(familySelect("UI")).toBeInTheDocument();
    expect(sizeField("Source code")).toBeInTheDocument();
    expect(lineHeightField("Rich Markdown")).toBeInTheDocument();
    expect(screen.getByTestId("font-specimen-rich")).toBeInTheDocument();

    // And the enumeration arriving populates them rather than replacing a
    // spinner.
    await waitFor(() =>
      expect(
        within(familySelect("UI")).getAllByRole("option").length,
      ).toBeGreaterThan(1),
    );
  });

  it("GLS-FR-22: choosing another family while the stored one is unavailable replaces the selection", async () => {
    const withoutFira = INSTALLED.filter((f) => f.family !== "Fira Code");
    wireBackend({
      prefs: { theme: "dark", fonts: { source: { family: "Fira Code" } } },
      fonts: withoutFira,
    });
    render(<GlobalSettings />);
    const select = await waitFor(() => familySelect("Source code"));
    await waitFor(() => expect(select.value).toBe("Fira Code"));

    await userEvent.selectOptions(select, "JetBrains Mono");

    await waitFor(() =>
      expect(lastSavedPrefs()?.fonts?.source?.family).toBe("JetBrains Mono"),
    );
    // The absence no longer applies to the new selection, so the marking goes.
    await waitFor(() => expect(select).not.toHaveAttribute("data-unavailable"));
    expect(screen.queryByText(/is not installed on this machine/i)).toBeNull();
  });
});

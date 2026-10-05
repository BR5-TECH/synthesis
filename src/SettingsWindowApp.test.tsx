import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { SettingsWindowApp } from "./SettingsWindowApp";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetSettingsSections } from "./state/settingsSweep";

/**
 * A settings **child window** end to end
 * (`../specifications/ui/SWN-settings-windows.md`).
 *
 * This is the window's own half of the contract: the save-before-close sweep it
 * runs when the backend asks (SWN-FR-08 through SWN-FR-12), the section it
 * presents when routed or when a save fails (SWN-FR-11, SWN-FR-13), the absence
 * of any discard prompt (SWN-FR-10), and the two things it holds that the main
 * window used to — the GitHub token picker of GHA-FR-20 and the project's
 * line-ending convention of SET-FR-11.
 *
 * The frame itself — the fixed size, the centring, the modality, the title, and
 * the one-window rule — is the backend's, and is pinned in
 * `src-tauri/src/settings_window.rs`.
 */

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const { eventHandlers, emitMock } = vi.hoisted(() => ({
  eventHandlers: {} as Record<string, Array<(e: { payload?: unknown }) => void>>,
  emitMock: vi.fn(),
}));
vi.mock("@tauri-apps/api/event", () => ({
  emit: (...args: unknown[]) => emitMock(...args),
  listen: vi.fn(
    async (name: string, handler: (e: { payload?: unknown }) => void) => {
      (eventHandlers[name] ??= []).push(handler);
      return () => {
        eventHandlers[name] = (eventHandlers[name] ?? []).filter(
          (h) => h !== handler,
        );
      };
    },
  ),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));

function fireBusEvent(name: string, payload?: unknown) {
  for (const h of [...(eventHandlers[name] ?? [])]) h({ payload });
}

/** SWN-FR-08: the backend asking this window to save and report. */
const requestSaveAndClose = () =>
  act(() => {
    fireBusEvent("settings-window:save-and-close");
  });

const calls = (name: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === name);

const answers = () =>
  calls("finish_settings_close").map(
    (c) => c[1] as { ok: boolean; section: string | null },
  );

/** The project behind the window, as `get_settings_window_context` answers. */
const CONTEXT = {
  projectName: "acme",
  projectKey: "~/dev/acme",
  contentRoot: "~/dev/acme",
};

let projectConfig: { lineEndings: string; draftTemplate: string | null };
let saveConfigOutcome: "ok" | "fail";
let savedConfigs: unknown[];
let binding: unknown;

function backend() {
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) => {
      switch (cmd) {
        case "get_settings_window_context":
          return CONTEXT;
        case "load_app_preferences":
          return { theme: "dark" };
        case "save_app_preferences":
          return undefined;
        case "list_system_fonts":
          return [];
        case "get_notification_permission":
          return "granted";
        case "load_project_config":
          return { ...projectConfig };
        case "save_project_config":
          savedConfigs.push(args?.config);
          if (saveConfigOutcome === "fail") throw "disk full";
          return undefined;
        case "get_project_github_token_binding":
          return binding;
        case "list_github_tokens":
          return [
            {
              id: "t1",
              label: "work laptop",
              accountLogin: "octocat",
              scopes: ["repo"],
              maskedHint: "aaaa",
              addedAt: "2026-06-01T10:00:00Z",
              lastVerifiedAt: "2026-06-01T10:00:00Z",
              state: "verified",
            },
            {
              id: "t2",
              label: "personal",
              accountLogin: "octocat",
              scopes: ["repo"],
              maskedHint: "bbbb",
              addedAt: "2026-06-02T10:00:00Z",
              lastVerifiedAt: "2026-06-02T10:00:00Z",
              state: "verified",
            },
          ];
        case "set_project_github_token_binding":
          binding = { tokenId: args?.tokenId, resolution: "bound" };
          return undefined;
        case "list_recent_projects":
        case "list_installed_plugins":
        case "list_agent_adapters":
        case "list_agents":
        case "list_project_agents":
        case "list_ai_api_integrations":
        case "list_agentic_integrations":
        case "list_ai_api_catalogs":
          return [];
        case "get_active_ai_api_catalog":
          return { resolution: "none_configured", catalog: null };
        default:
          return undefined;
      }
    },
  );
}

beforeEach(() => {
  invokeMock.mockReset();
  emitMock.mockReset();
  for (const k in eventHandlers) delete eventHandlers[k];
  resetAppPreferencesCache();
  resetSettingsSections();
  projectConfig = { lineEndings: "lf", draftTemplate: null };
  saveConfigOutcome = "ok";
  savedConfigs = [];
  binding = { tokenId: "t2", resolution: "bound" };
  backend();
});

afterEach(() => {
  cleanup();
  resetAppPreferencesCache();
  resetSettingsSections();
});

const renderGlobal = (section: string | null = null) =>
  render(<SettingsWindowApp kind="global" initialSection={section} />);
const renderProject = (section: string | null = null) =>
  render(<SettingsWindowApp kind="project" initialSection={section} />);

describe("the window renders its own settings and nothing of the shell (SWN-FR-01)", () => {
  it("renders Global settings with none of the main window's chrome", async () => {
    renderGlobal();
    await screen.findByTestId("global-settings");
    // SWN-FR-01 / SWN-FR-18: no tab strip, no activity bar, no status bar — the
    // frame is the platform's own and the body is one section-navigated surface.
    expect(document.querySelector(".tabstrip")).toBeNull();
    expect(document.querySelector(".activity-bar")).toBeNull();
    expect(screen.queryByTestId("status-bar")).toBeNull();
  });

  it("renders Project settings, and reads the project behind it", async () => {
    renderProject();
    await screen.findByText("Draft template");
    await waitFor(() =>
      expect(calls("get_settings_window_context")).toHaveLength(1),
    );
    expect(document.querySelector(".tabstrip")).toBeNull();
  });
});

describe("the window follows what the other one wrote (GLS-FR-05 / GLS-FR-20)", () => {
  it("adopts a theme the main window persisted, and does not read it as unsaved", async () => {
    // SNV-FR-15: one shared preference with two equal entry points — the
    // top-chrome selector in the main window and this section. Since SWN-FR-01
    // they are in two windows, so a change made over there arrives as an
    // announcement. Adopting it as the SAVED baseline as well as the displayed
    // one is what keeps this section from reporting a change it did not make as
    // one of its own pending changes (SWN-FR-08).
    let stored: Record<string, unknown> = { theme: "dark" };
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") return stored;
      if (cmd === "get_settings_window_context") return CONTEXT;
      if (cmd === "list_system_fonts") return [];
      if (cmd === "get_notification_permission") return "granted";
      return [];
    });

    renderGlobal();
    const body = await screen.findByTestId("global-settings");
    await waitFor(() =>
      expect(within(body).getByRole("radio", { name: "Dark" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );

    stored = { theme: "light" };
    act(() => {
      fireBusEvent("app-preferences:changed");
    });

    await waitFor(() =>
      expect(within(body).getByRole("radio", { name: "Light" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );
    // The main window persisted it, so this section holds nothing to write.
    expect(within(body).queryByText(/Unsaved theme change/)).toBeNull();
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("adopts font roles the main window persisted (OVW-FR-14)", async () => {
    let stored: Record<string, unknown> = { theme: "dark" };
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") return stored;
      if (cmd === "get_settings_window_context") return CONTEXT;
      if (cmd === "list_system_fonts") return [];
      if (cmd === "get_notification_permission") return "granted";
      return [];
    });

    renderGlobal();
    await screen.findByTestId("global-settings");

    stored = { theme: "dark", fonts: { rich: { sizePx: 21 } } };
    act(() => {
      fireBusEvent("app-preferences:changed");
    });

    await waitFor(() =>
      expect(
        document.documentElement.style.getPropertyValue("--font-rich-size"),
      ).toBe("21px"),
    );
  });
});

describe("the save-before-close sweep (SWN-FR-08 / SWN-FR-10)", () => {
  it("SWN-FR-08, SWN-FR-10: answers straight away when nothing is pending, with no prompt", async () => {
    const confirmMock = vi.fn();
    vi.stubGlobal("confirm", confirmMock);
    renderGlobal();
    await screen.findByTestId("global-settings");

    await requestSaveAndClose();

    await waitFor(() => expect(answers()).toEqual([{ ok: true, section: null }]));
    // SWN-FR-10: no discard-unsaved-changes prompt exists for a settings window.
    expect(confirmMock).not.toHaveBeenCalled();
    vi.unstubAllGlobals();
  });

  it("SWN-FR-08, SWN-FR-07: writes a section that is not the one on screen", async () => {
    // The Draft template section holds the window's one pending change in this
    // build. Open it, type, navigate away to another section — leaving it
    // mounted-and-gone — and close from there.
    renderProject("template");
    const source = await screen.findByRole("button", {
      name: "Edit as Markdown source",
    });
    await userEvent.click(source);
    const area = await screen.findByLabelText("Markdown source");
    fireEvent.change(area, { target: { value: "# Context" } });

    // Away to another section, so the one holding the change is no longer the
    // one on screen — which is the whole of what SWN-FR-08, SWN-FR-07 is about.
    await userEvent.click(screen.getByRole("tab", { name: "Plugins" }));
    await screen.findByRole("heading", { name: "Plugins" });
    expect(screen.queryByLabelText("Markdown source")).toBeNull();

    await requestSaveAndClose();

    // SWN-FR-08: the pending change was written before the window answered.
    await waitFor(() => expect(answers()).toEqual([{ ok: true, section: null }]));
    // Exactly once, whichever route wrote it: SET-FR-17's leave-the-section
    // write and the window's sweep must not each send the same text.
    expect(savedConfigs).toEqual([
      { lineEndings: "lf", draftTemplate: "# Context" },
    ]);
  });
});

describe("a failed save cancels the close (SWN-FR-11)", () => {
  it("SWN-FR-08, SWN-FR-11: reports the failure, keeps the edits, and presents the section", async () => {
    saveConfigOutcome = "fail";
    renderProject("template");
    await userEvent.click(
      await screen.findByRole("button", { name: "Edit as Markdown source" }),
    );
    const area = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    fireEvent.change(area, { target: { value: "# Context" } });

    await requestSaveAndClose();

    // The backend is told the transition is cancelled, and which section to
    // present so the error and the retry are visible.
    await waitFor(() =>
      expect(answers()).toEqual([{ ok: false, section: "template" }]),
    );
    // The author's text is exactly as they left it — nothing reverted, reloaded
    // or replaced (SET-FR-18).
    expect(
      (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
    ).toBe("# Context");
    // …and the section states the failure and offers its retry.
    expect(screen.getByTestId("draft-template-status")).toHaveTextContent(
      /could not be saved/i,
    );
    expect(
      screen.getByRole("button", { name: "Retry" }),
    ).toBeInTheDocument();
  });

  it("presents the failed section even when the author is looking at another", async () => {
    saveConfigOutcome = "fail";
    renderProject("template");
    await userEvent.click(
      await screen.findByRole("button", { name: "Edit as Markdown source" }),
    );
    const area = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    fireEvent.change(area, { target: { value: "# Context" } });

    await requestSaveAndClose();
    await waitFor(() =>
      expect(answers()).toEqual([{ ok: false, section: "template" }]),
    );

    // The backend routes the window to the failed section (SWN-FR-11); the
    // author had navigated to Plugins in the meantime.
    await userEvent.click(screen.getByText("Plugins"));
    act(() => {
      fireBusEvent("settings-window:present-failure", { section: "template" });
    });

    expect(
      await screen.findByRole("button", { name: "Edit as Markdown source" }),
    ).toBeInTheDocument();
  });
});

describe("failure paths that would otherwise wedge the window", () => {
  it("logs rather than wedging when the close answer cannot be delivered", async () => {
    // The one failure this window cannot recover from on its own: the backend
    // goes on holding the sweep, every later request is inert (SWN-FR-12), and
    // the parent stays blocked behind a window that will not close
    // (SWN-FR-02). Nothing here can retry it usefully, so the record is the
    // whole of what it can do — and without one the wedge has no explanation.
    backend();
    const original = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "finish_settings_close") throw new Error("bridge is gone");
      return original(cmd, args as Record<string, unknown>);
    });

    renderGlobal();
    await screen.findByTestId("global-settings");

    await requestSaveAndClose();

    await waitFor(() => expect(calls("finish_settings_close")).toHaveLength(1));
    // It did not throw into an unhandled rejection, and it is ready to answer a
    // second request rather than believing one is still running.
    await requestSaveAndClose();
    await waitFor(() => expect(calls("finish_settings_close")).toHaveLength(2));
  });

  it("SWN-FR-15: renders anyway when the project behind it cannot be read", async () => {
    // Global settings is reachable with no project open at all, so a context
    // read that fails must not take the window with it — the sections that need
    // a project disable themselves instead.
    const original = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "get_settings_window_context") throw new Error("no project");
      return original(cmd, args as Record<string, unknown>);
    });

    renderGlobal("notifications");

    expect(
      await screen.findByRole("heading", { name: "Notifications" }),
    ).toBeInTheDocument();
    // GLS-FR-27: with no project to address, the rehearsal is greyed out rather
    // than sending a notification nothing could route.
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /test notification/i }),
      ).toBeDisabled(),
    );
  });
});

describe("section routing (SWN-FR-06 / SWN-FR-13)", () => {
  it("opens on the section the request that created the window named", async () => {
    renderGlobal("github");
    // The GitHub section of GHA-FR-01, rather than Appearance.
    expect(
      await screen.findByRole("heading", { name: "GitHub" }),
    ).toBeInTheDocument();
  });

  it("SWN-FR-06: presents a section a later request names, without reloading", async () => {
    renderGlobal();
    await screen.findByRole("heading", { name: "Appearance" });

    act(() => {
      fireBusEvent("settings-window:route", { section: "notifications" });
    });

    expect(
      await screen.findByRole("heading", { name: "Notifications" }),
    ).toBeInTheDocument();
    // The window was not remounted: its sections keep their state, so nothing
    // was read a second time.
    expect(calls("get_settings_window_context")).toHaveLength(1);
  });

  it("AGT-FR-06, AGT-FR-07: an agents address opens the Agents section with an editor", async () => {
    // AGT-FR-06 / AGT-FR-07: the roster's two actions travel as one section
    // address — `agents:new` for a fresh persona, `agents:<id>` for one that
    // exists — because the window they reach is a window of its own and shares
    // no state with the one the roster is in.
    renderGlobal("agents:new");
    expect(
      await screen.findByRole("heading", { name: "Agents" }),
    ).toBeInTheDocument();
    await screen.findByTestId("agents-section");
    // The editor is open on a fresh persona rather than on an existing one.
    expect(await screen.findByLabelText(/nickname/i)).toHaveValue("");
  });

  it("AGT-FR-06: an honoured editor request is not reopened by the section's own work", async () => {
    // The defect this pins: the request is derived from the section address, so
    // a section that re-rendered for its own reasons — its agent list landing,
    // an agent created, one deleted — would find the same request standing and
    // reopen the editor the author had just closed.
    renderGlobal("agents:new");
    const field = await screen.findByLabelText(/nickname/i);
    expect(field).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(screen.queryByLabelText(/nickname/i)).toBeNull(),
    );

    // Nothing brings it back on its own.
    act(() => {
      fireBusEvent("settings-window:route", { section: "appearance" });
      fireBusEvent("settings-window:route", { section: "agents" });
    });
    await screen.findByTestId("agents-section");
    expect(screen.queryByLabelText(/nickname/i)).toBeNull();
  });

  it("ignores a section this window does not have", async () => {
    // A `project` address routed at the Global window, or the reverse, names
    // nothing here — and must not blank the body.
    renderGlobal("mcp");
    expect(
      await screen.findByRole("heading", { name: "Appearance" }),
    ).toBeInTheDocument();
  });
});

describe("the GitHub token picker lives in this window (GHA-FR-20 / GHA-FR-21)", () => {
  it("GHA-FR-20: preselects the project's binding, and rebinds on confirm", async () => {
    // GHA-FR-20: the picker is presented **within the Project settings window**
    // rather than over the main window, that window being modal to it. Supplying
    // no preselection is not a cosmetic slip: the picker falls back to the first
    // row, so confirming without touching anything silently rebinds the project.
    renderProject("project");
    await userEvent.click(await screen.findByRole("button", { name: "Change…" }));

    const picker = await screen.findByTestId("github-token-picker");
    await waitFor(() =>
      expect(
        within(picker).getByRole("radio", { name: "personal" }),
      ).toHaveAttribute("aria-checked", "true"),
    );
    expect(
      within(picker).getByRole("radio", { name: "work laptop" }),
    ).toHaveAttribute("aria-checked", "false");

    await userEvent.click(
      within(picker).getByRole("radio", { name: "work laptop" }),
    );
    await userEvent.click(
      within(picker).getByRole("button", { name: /^Use/ }),
    );

    await waitFor(() =>
      expect(calls("set_project_github_token_binding")).toHaveLength(1),
    );
    expect(calls("set_project_github_token_binding")[0][1]).toEqual({
      tokenId: "t1",
    });
    await waitFor(() =>
      expect(screen.queryByTestId("github-token-picker")).toBeNull(),
    );
  });

  it("GHA-FR-20: cancelling leaves the binding untouched", async () => {
    renderProject("project");
    await userEvent.click(await screen.findByRole("button", { name: "Change…" }));
    const picker = await screen.findByTestId("github-token-picker");

    await userEvent.click(
      within(picker).getByRole("button", { name: "Cancel" }),
    );

    await waitFor(() =>
      expect(screen.queryByTestId("github-token-picker")).toBeNull(),
    );
    expect(calls("set_project_github_token_binding")).toHaveLength(0);
  });
});

describe("the project's line-ending convention (SET-FR-10 / SET-FR-11)", () => {
  it("SET-FR-10, PST-FR-22: persists at once and announces it to the other window", async () => {
    renderProject("project");
    const section = await screen.findByTestId("settings-line-endings");
    await waitFor(() =>
      expect(within(section).getByRole("radio", { name: "LF" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );

    await userEvent.click(within(section).getByRole("radio", { name: "CRLF" }));

    await waitFor(() => expect(savedConfigs).toEqual([{ lineEndings: "crlf" }]));
    // SET-FR-11 / SWN-FR-01: the status bar edits the identical value from the
    // main window, and hears about this one.
    await waitFor(() =>
      expect(
        emitMock.mock.calls.filter(
          (c) => c[0] === "project-config:line-endings-changed",
        ),
      ).toEqual([["project-config:line-endings-changed", { value: "crlf" }]]),
    );
    // SET-FR-10: no Save control was involved.
    expect(screen.queryByRole("button", { name: /^Save/ })).toBeNull();
  });

  it("SET-FR-11: adopts a convention the status bar persisted", async () => {
    renderProject("project");
    const section = await screen.findByTestId("settings-line-endings");
    await waitFor(() =>
      expect(within(section).getByRole("radio", { name: "LF" })).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );

    act(() => {
      fireBusEvent("project-config:line-endings-changed", { value: "crlf" });
    });

    await waitFor(() =>
      expect(
        within(screen.getByTestId("settings-line-endings")).getByRole("radio", {
          name: "CRLF",
        }),
      ).toHaveAttribute("aria-checked", "true"),
    );
    // It reflected the write rather than repeating it.
    expect(savedConfigs).toEqual([]);
  });
});

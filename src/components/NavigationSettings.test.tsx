import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { NavigationSettings } from "./NavigationSettings";
import { resetAppPreferencesCache } from "../state/appPreferences";
import {
  resetSelectionFollowsTab,
  selectionFollowsTabEnabled,
} from "../state/selectionFollowsTab";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

/** The stored record, or a record with no such field at all. */
function backend(prefs: Record<string, unknown>) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_app_preferences") return prefs;
    return undefined;
  });
}

const theSwitch = () =>
  screen.getByRole("checkbox", { name: "Selection follows tab" });

beforeEach(() => {
  invokeMock.mockReset();
  // The preferences record is a module-level cache shared across the process.
  resetAppPreferencesCache();
  resetSelectionFollowsTab();
});
afterEach(cleanup);

describe("Navigation section (GLS-FR-28)", () => {
  it("GLS-FR-03, GLS-FR-28, GLS-FR-10, GLS-FR-13, GLS-FR-14, GSS-FR-33, GSS-FR-20: reads on, on a machine that has never saved a preference", async () => {
    backend({ theme: "system" });
    render(<NavigationSettings />);

    await waitFor(() => expect(theSwitch()).toBeChecked());
    expect(calls("load_app_preferences").length).toBeGreaterThan(0);
    // Nothing was written by merely looking at the section.
    expect(calls("save_app_preferences")).toHaveLength(0);
  });

  it("GLS-FR-03, GLS-FR-28, GLS-FR-10, GLS-FR-13, GLS-FR-14, GSS-FR-33, GSS-FR-20: reads on for a stored record written before the field existed", async () => {
    // The upgrade case, which is the one the default exists for: an author who
    // has been using the application has a record with every other field set and
    // this one absent, and must not be presented as opted out.
    backend({
      theme: "dark",
      mainWindowFullscreen: true,
      searchQueryMode: "regex",
      notificationsEnabled: false,
    });
    render(<NavigationSettings />);

    await waitFor(() => expect(theSwitch()).toBeChecked());
  });

  it("GLS-FR-03, GLS-FR-28, GLS-FR-10, GLS-FR-13, GLS-FR-14, GSS-FR-33, GSS-FR-20: reads off once the author has turned it off", async () => {
    backend({ theme: "system", selectionFollowsTab: false });
    render(<NavigationSettings />);

    await waitFor(() => expect(theSwitch()).not.toBeChecked());
  });

  it("GLS-FR-03, GLS-FR-28, GLS-FR-10, GLS-FR-13, GLS-FR-14, GSS-FR-33, GSS-FR-20: persists through the whole-record write, carrying every other field", async () => {
    backend({
      theme: "dark",
      mainWindowFullscreen: true,
      searchQueryMode: "regex",
      notificationsEnabled: false,
      fonts: { ui: { family: "Inter" }, rich: {}, source: {} },
    });
    render(<NavigationSettings />);
    await waitFor(() => expect(theSwitch()).toBeChecked());

    await userEvent.click(theSwitch());

    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    const written = calls("save_app_preferences")[0][1] as {
      preferences: Record<string, unknown>;
    };
    expect(written.preferences.selectionFollowsTab).toBe(false);
    // GSS-FR-20 / GLS-FR-14: a write of this field resets nothing else.
    expect(written.preferences).toMatchObject({
      theme: "dark",
      mainWindowFullscreen: true,
      searchQueryMode: "regex",
      notificationsEnabled: false,
      fonts: { ui: { family: "Inter" }, rich: {}, source: {} },
    });
    expect(theSwitch()).not.toBeChecked();
  });

  it("GLS-FR-28: takes effect at once rather than at the next relaunch", async () => {
    backend({ theme: "system" });
    render(<NavigationSettings />);
    await waitFor(() => expect(selectionFollowsTabEnabled()).toBe(true));

    await userEvent.click(theSwitch());

    // The shell reads this gate at every activation, so the next one already
    // sees the new value — with no relaunch and no prop threaded down to it.
    await waitFor(() => expect(selectionFollowsTabEnabled()).toBe(false));

    await userEvent.click(theSwitch());
    await waitFor(() => expect(selectionFollowsTabEnabled()).toBe(true));
  });

  it("GLS-FR-28: every toggle writes at once, so there is never a pending change", async () => {
    // Deliberately NOT "there is no Save button": this component renders no
    // buttons at all, so that assertion cannot fail and reads as coverage it is
    // not. What GLS-FR-10 actually claims is that no gesture leaves an unsaved
    // change behind — so drive several and count the writes.
    backend({ theme: "system" });
    render(<NavigationSettings />);
    await waitFor(() => expect(theSwitch()).toBeChecked());

    await userEvent.click(theSwitch());
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    await userEvent.click(theSwitch());
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(2));

    // Each write carried the value shown at that moment, so the switch is never
    // ahead of what is stored.
    const values = calls("save_app_preferences").map(
      (c) => (c[1] as { preferences: { selectionFollowsTab: boolean } })
        .preferences.selectionFollowsTab,
    );
    expect(values).toEqual([false, true]);
    expect(theSwitch()).toBeChecked();
  });

  it("rolls the switch back and says so when the write fails", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_app_preferences") return { theme: "system" };
      if (cmd === "save_app_preferences") throw new Error("disk is full");
      return undefined;
    });
    render(<NavigationSettings />);
    await waitFor(() => expect(theSwitch()).toBeChecked());

    await userEvent.click(theSwitch());

    // Rolled back rather than left asserting a value that never reached disk,
    // and the gate rolled back with it so the shell and the switch agree.
    await waitFor(() => expect(theSwitch()).toBeChecked());
    expect(selectionFollowsTabEnabled()).toBe(true);
    expect(await screen.findByRole("alert")).toHaveTextContent("disk is full");
  });
});

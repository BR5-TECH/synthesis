import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

import { NotificationSettings, REHEARSAL_DELAY_MS } from "./NotificationSettings";
import { resetAppPreferencesCache } from "../state/appPreferences";
import {
  configureNotifications,
  notificationGate,
  resetNotifications,
} from "../state/notifications";
import type { NotificationPermissionState } from "../types";

/**
 * The Global settings Notifications section (`GLS-global-settings.md`
 * GLS-FR-25 / GLS-FR-26 / GLS-FR-27), covering GLS-FR-03, GLS-FR-10, GLS-FR-13, GLS-FR-11, NTF-FR-29, GLS-FR-26 and
 * GLS-FR-25, GLS-FR-14, GSS-FR-20, GSS-FR-32, plus the rehearsal half of NTF-FR-25, NTF-FR-17, GLS-FR-27 / NTF-FR-08.
 */

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

// SWN-FR-01: the section is in the Global settings child window, which is a
// webview of its own — so the rehearsal reaches the main window as an event
// rather than as a call. The bus stands in for that window.
const { emitMock } = vi.hoisted(() => ({ emitMock: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  emit: (...args: unknown[]) => emitMock(...args),
  listen: vi.fn(async () => () => {}),
}));

const emitted = (name: string) =>
  emitMock.mock.calls.filter((c) => c[0] === name);

const TARGET = { projectKey: "/dev/acme", worktree: "/dev/acme" };

const calls = (name: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === name);

const saved = () =>
  calls("save_app_preferences").map(
    (c) => (c[1] as { preferences: Record<string, unknown> }).preferences,
  );

function wireBackend({
  permission = "granted" as NotificationPermissionState,
  stored = {} as Record<string, unknown>,
  grantOnRequest = "granted" as NotificationPermissionState,
} = {}) {
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "load_app_preferences":
        return { theme: "system", ...stored };
      case "save_app_preferences":
        return undefined;
      case "get_notification_permission":
        return permission;
      case "request_notification_permission":
        return grantOnRequest;
      case "post_notification":
        return { id: "notif-0" };
      default:
        return undefined;
    }
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  resetAppPreferencesCache();
  resetNotifications();
  wireBackend();
});

afterEach(() => {
  cleanup();
  resetAppPreferencesCache();
  resetNotifications();
  vi.useRealTimers();
});

const renderSection = (target = TARGET) =>
  render(<NotificationSettings rehearsalTarget={target} />);

describe("the switch (GLS-FR-25)", () => {
  it("initialises from the stored record", async () => {
    // GLS-FR-03, GLS-FR-25, GLS-FR-26, GLS-FR-27, GLS-FR-10, GLS-FR-13, GLS-FR-11, NTF-FR-29.
    wireBackend({ stored: { notificationsEnabled: false } });
    renderSection();
    const box = await screen.findByRole("checkbox");
    await waitFor(() => expect(box).not.toBeChecked());
  });

  it("reads an absent field as ON rather than off (GSS-FR-32)", async () => {
    // The upgrade path: every record written before the field existed omits it,
    // and reading `undefined` as `false` would present every existing install
    // as opted out of a feature they never turned off.
    wireBackend({ stored: {} });
    renderSection();
    const box = await screen.findByRole("checkbox");
    await waitFor(() => expect(box).toBeChecked());
  });

  it("persists at once, through a patch that carries the rest through", async () => {
    // GLS-FR-25, GSS-FR-32 / GLS-FR-14 / GSS-FR-20: a whole-record write, so the theme and
    // every other field survive a switch change.
    wireBackend({
      stored: { theme: "dark", searchQueryMode: "regex", fonts: {} },
    });
    renderSection();
    const box = await screen.findByRole("checkbox");
    await waitFor(() => expect(box).toBeChecked());

    fireEvent.click(box);
    await waitFor(() => expect(saved()).toHaveLength(1));
    expect(saved()[0]).toMatchObject({
      notificationsEnabled: false,
      theme: "dark",
      searchQueryMode: "regex",
    });
  });

  it("rolls back when the write fails, rather than lying about what is stored", async () => {
    renderSection();
    const box = await screen.findByRole("checkbox");
    await waitFor(() => expect(box).toBeChecked());

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_app_preferences") throw new Error("disk is full");
      if (cmd === "load_app_preferences") return { theme: "system" };
      if (cmd === "get_notification_permission") return "granted";
      return undefined;
    });
    fireEvent.click(box);
    await waitFor(() => expect(box).toBeChecked());
    expect(await screen.findByText(/disk is full/)).toBeInTheDocument();
  });

  it("holds no dirty state — it never asks anything to be saved", async () => {
    // GLS-FR-10 / GLS-FR-13: every action applies immediately, so the section
    // never contributes to the tab's discard-on-close confirmation. The
    // observable is that there is no Save control at all.
    renderSection();
    await screen.findByRole("checkbox");
    expect(screen.queryByRole("button", { name: /save/i })).toBeNull();
  });
});

describe("the operating system's disposition (GLS-FR-26)", () => {
  it("reads the platform on mount without asking for anything", async () => {
    // NTF-FR-13: the state is read; nothing here requests it, so no permission
    // prompt appears merely because the section was opened.
    renderSection();
    await waitFor(() => expect(calls("get_notification_permission")).toHaveLength(1));
    expect(calls("request_notification_permission")).toHaveLength(0);
  });

  it("offers a request only while the platform has not been asked", async () => {
    // GLS-FR-26, GLS-FR-27.
    wireBackend({ permission: "not_requested" });
    renderSection();
    const allow = await screen.findByRole("button", {
      name: "Allow notifications",
    });
    expect(screen.getByText(/has not been asked/)).toBeInTheDocument();

    fireEvent.click(allow);
    await waitFor(() =>
      expect(calls("request_notification_permission")).toHaveLength(1),
    );
    // Once granted the control is gone, because there is nothing left to ask.
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: "Allow notifications" }),
      ).toBeNull(),
    );
    // GLS-FR-26, GLS-FR-27's last clause, and the one that matters most: granting is what
    // ENABLES the rehearsal. Without it the author grants permission precisely
    // in order to try the feature and finds the button still greyed out.
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /test notification/i }),
      ).toBeEnabled(),
    );
    // And the facility itself must know, or the rehearsal they now press would
    // be suppressed for want of a permission they just gave (NTF-FR-12).
    expect(notificationGate().permissionGranted).toBe(true);
  });

  it("tells the facility about every gate change it makes (NTF-FR-12)", async () => {
    // The defect this pins: the gate used to be read once into the shell's
    // React state at startup, so nothing the author did in this section
    // reached a raise until the application was relaunched.
    wireBackend({ stored: { notificationsEnabled: true } });
    renderSection();
    const box = await screen.findByRole("checkbox");
    await waitFor(() => expect(notificationGate().enabled).toBe(true));
    await waitFor(() => expect(notificationGate().permissionGranted).toBe(true));

    fireEvent.click(box);
    await waitFor(() => expect(notificationGate().enabled).toBe(false));

    fireEvent.click(box);
    await waitFor(() => expect(notificationGate().enabled).toBe(true));
  });

  it("puts the gate back when the write that would have changed it fails", async () => {
    // A rolled-back switch that left the facility disabled would stop every
    // notification for the rest of the session, with the checkbox insisting
    // they were on.
    renderSection();
    const box = await screen.findByRole("checkbox");
    await waitFor(() => expect(notificationGate().enabled).toBe(true));

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "save_app_preferences") throw new Error("disk is full");
      if (cmd === "load_app_preferences") return { theme: "system" };
      if (cmd === "get_notification_permission") return "granted";
      return undefined;
    });
    fireEvent.click(box);
    await waitFor(() => expect(box).toBeChecked());
    expect(notificationGate().enabled).toBe(true);
  });

  it("states a refusal without offering a control that cannot succeed", async () => {
    // GLS-FR-26: a refusal is reversed in the operating system's own settings,
    // so a button here would do nothing and say something false by existing.
    wireBackend({ permission: "denied" });
    renderSection();
    expect(
      await screen.findByText(/turned off for Synthesis in your operating/),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "Allow notifications" }),
    ).toBeNull();
  });

  it("GLS-FR-26: disables the switch entirely on a platform with no notification centre", async () => {
    wireBackend({ permission: "unsupported" });
    renderSection();
    const box = await screen.findByRole("checkbox");
    await waitFor(() => expect(box).toBeDisabled());
    expect(screen.getByText(/no notification centre/)).toBeInTheDocument();
    // GLS-FR-26: the statement also names a run outside an application bundle.
    expect(
      screen.getByText(/cannot post notifications/),
    ).toHaveTextContent(/not running from an application bundle/);
  });

  it("treats a platform that cannot answer as unsupported", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_notification_permission") throw new Error("no backend");
      if (cmd === "load_app_preferences") return { theme: "system" };
      return undefined;
    });
    renderSection();
    expect(await screen.findByText(/no notification centre/)).toBeInTheDocument();
  });

  it("asks for permission when the author turns the switch on, and never when off", async () => {
    // NTF-FR-13: enabling the switch is the author asking for notifications,
    // which is one of exactly two places a prompt may originate.
    wireBackend({
      permission: "not_requested",
      stored: { notificationsEnabled: false },
    });
    renderSection();
    const box = await screen.findByRole("checkbox");
    await waitFor(() => expect(box).not.toBeChecked());

    fireEvent.click(box); // off -> on
    await waitFor(() =>
      expect(calls("request_notification_permission")).toHaveLength(1),
    );

    fireEvent.click(box); // on -> off
    await waitFor(() => expect(saved()).toHaveLength(2));
    expect(calls("request_notification_permission")).toHaveLength(1);
  });
});

describe("the rehearsal (GLS-FR-27)", () => {
  it("is disabled unless there is something for it to demonstrate", async () => {
    for (const [label, opts] of [
      ["switch off", { stored: { notificationsEnabled: false } }],
      ["permission denied", { permission: "denied" as const }],
    ] as const) {
      cleanup();
      resetAppPreferencesCache();
      invokeMock.mockReset();
      wireBackend(opts);
      renderSection();
      const button = await screen.findByRole("button", { name: /test notification/i });
      await waitFor(() => expect(button, label).toBeDisabled());
    }
  });

  it("is disabled with no project open, having nowhere to land", async () => {
    renderSection(null as never);
    const button = await screen.findByRole("button", {
      name: /test notification/i,
    });
    await waitFor(() => expect(button).toBeDisabled());
  });

  it("states its delay so the author knows how long they have", async () => {
    renderSection();
    const seconds = Math.round(REHEARSAL_DELAY_MS / 1000);
    expect(
      await screen.findByRole("button", {
        name: new RegExp(`${seconds}\\s*s`, "i"),
      }),
    ).toBeInTheDocument();
  });

  it("asks the main window for the raise, addressing the Global settings window", async () => {
    // NTF-FR-17, GLS-FR-27 / NTF-FR-25 / SWN-FR-01: the raise goes through the ordinary
    // facility, so its address must be one that actually resolves — the arrival
    // end of the path has to be reachable every time. The facility lives in the
    // main window, and the delay with it: the whole point of the rehearsal is
    // that the author leaves before it fires, and closing THIS window is one of
    // the ways they may leave. A timer held here would go with the window and
    // take the notification with it.
    vi.useFakeTimers();
    renderSection();
    const button = await vi.waitFor(() =>
      screen.getByRole("button", { name: /test notification/i }),
    );
    await vi.waitFor(() => expect(button).toBeEnabled());

    fireEvent.click(button);

    // Nothing is posted from here, now or later.
    expect(calls("post_notification")).toHaveLength(0);
    const asks = emitted("settings-window:rehearsal-requested");
    expect(asks).toHaveLength(1);
    expect((asks[0][1] as { address: string }).address).toBe(
      `synthesis://${encodeURIComponent(TARGET.projectKey)}/${encodeURIComponent(
        TARGET.worktree,
      )}/settings/global`,
    );

    await act(async () => {
      vi.advanceTimersByTime(REHEARSAL_DELAY_MS);
    });
    expect(calls("post_notification")).toHaveLength(0);
  });

  it("clears its timer when the section unmounts mid-delay", async () => {
    // The raise itself is allowed to be in flight, but the timer must not fire
    // `setPendingRehearsal` into an unmounted component.
    vi.useFakeTimers();
    const { unmount } = renderSection();
    const button = await vi.waitFor(() =>
      screen.getByRole("button", { name: /test notification/i }),
    );
    await vi.waitFor(() => expect(button).toBeEnabled());
    fireEvent.click(button);
    unmount();
    const errors: unknown[] = [];
    const spy = vi.spyOn(console, "error").mockImplementation((...a) => {
      errors.push(a);
    });
    await act(async () => {
      vi.advanceTimersByTime(REHEARSAL_DELAY_MS * 2);
    });
    expect(errors).toEqual([]);
    spy.mockRestore();
  });

  it("is suppressed when the author stays on the very tab it addresses", async () => {
    // NTF-FR-25, NTF-FR-08: the rehearsal is not a special case of the policy, it is
    // subject to it — which is exactly what a dedicated test-only post would
    // have hidden.
    vi.useFakeTimers();
    configureNotifications({
      snapshot: () => ({
        focused: true,
        projectKey: TARGET.projectKey,
        worktree: TARGET.worktree,
        activeTarget: { kind: "settings", which: "global" },
        visiblePanelSurface: null,
        visibleBottomSurface: null,
    visibleRun: null,
    // SWN-FR-05 / NTF-FR-08: no settings child window is open.
    openSettingsWindow: null,
      }),
      resolveTab: () => null,
    });
    renderSection();
    const button = await vi.waitFor(() =>
      screen.getByRole("button", { name: /test notification/i }),
    );
    await vi.waitFor(() => expect(button).toBeEnabled());

    fireEvent.click(button);
    await act(async () => {
      vi.advanceTimersByTime(REHEARSAL_DELAY_MS);
    });
    expect(calls("post_notification")).toHaveLength(0);
  });
});

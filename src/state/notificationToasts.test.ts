import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  configureNotifications,
  decideDelivery,
  notifyArrived,
  raiseNotification,
  resetNotifications,
  retractNotification,
  setNotificationPermissionGranted,
  setNotificationsEnabled,
  withdrawForRoot,
  type Raise,
  type WindowSnapshot,
} from "./notifications";
import { mintAddress } from "./notificationAddress";
import { resetLogBufferForTest } from "../logging";
import { resetTabIndications } from "./tabIndications";
import { queuedToasts, showToast, visibleToasts } from "./toasts";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const PROJECT = "/dev/acme";
const WORKTREE = "/dev/acme";

const posts = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "post_notification")
    .map((c) => (c[1] as { request: Record<string, unknown> }).request);

const base: WindowSnapshot = {
  focused: true,
  mainFocused: true,
  projectKey: PROJECT,
  worktree: WORKTREE,
  activeTarget: null,
  visiblePanelSurface: null,
  visibleBottomSurface: null,
  visibleRun: null,
  openSettingsWindow: null,
};

const runAddress = (id: string) =>
  mintAddress(PROJECT, WORKTREE, { kind: "run", runId: id });

const raiseFor = (key: string, over: Partial<Raise> = {}): Raise => ({
  key,
  level: "Info",
  title: `title ${key}`,
  body: `body ${key}`,
  address: runAddress(key),
  ...over,
});

function useWindow(snapshot: WindowSnapshot) {
  configureNotifications({ snapshot: () => snapshot, resolveTab: () => null });
}

let postCounter = 0;
beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  postCounter = 0;
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "post_notification") return { id: `n-${postCounter++}` };
    return undefined;
  });
  resetNotifications();
  resetTabIndications();
  setNotificationsEnabled(true);
  setNotificationPermissionGranted(true);
});

describe("the channel of a raise (NTF-FR-WMBD)", () => {
  const options = { enabled: true, permissionGranted: true };

  it("NTF-FR-WMBD: chooses a toast while the main window holds focus", () => {
    expect(decideDelivery(raiseFor("a"), base, options)).toEqual({
      channel: "toast",
    });
  });

  it("NTF-FR-WMBD: chooses the OS while no window holds focus", () => {
    expect(
      decideDelivery(
        raiseFor("a"),
        { ...base, focused: false, mainFocused: false },
        options,
      ),
    ).toEqual({ channel: "os" });
  });

  it("NTF-FR-WMBD: chooses the OS while only a settings window holds focus", () => {
    expect(
      decideDelivery(raiseFor("a"), { ...base, mainFocused: false }, options),
    ).toEqual({ channel: "os" });
  });

  it("NTF-FR-WMBD: shows a toast and posts nothing to the OS", async () => {
    useWindow(base);
    await raiseNotification(raiseFor("a"));
    expect(visibleToasts().map((t) => t.key)).toEqual(["a"]);
    expect(posts()).toEqual([]);
  });

  it("NTF-FR-WMBD: posts to the OS and shows no toast while the window is in the background", async () => {
    useWindow({ ...base, focused: false, mainFocused: false });
    await raiseNotification(raiseFor("a"));
    expect(posts()).toHaveLength(1);
    expect(visibleToasts()).toEqual([]);
  });

  it("NTF-FR-KTRQ: a toast carries the level the raise set, and the OS notification carries none", async () => {
    useWindow(base);
    await raiseNotification(raiseFor("a", { level: "Error" }));
    expect(visibleToasts()[0].level).toBe("Error");

    useWindow({ ...base, focused: false, mainFocused: false });
    await raiseNotification(raiseFor("b", { level: "Warn" }));
    expect(posts()[0]).not.toHaveProperty("level");
  });
});

describe("the channel is gated only where it should be (NTF-FR-PCVX, NTF-FR-11)", () => {
  it("NTF-FR-PCVX, NTF-FR-11: shows a toast with the switch off", async () => {
    setNotificationsEnabled(false);
    useWindow(base);
    await raiseNotification(raiseFor("a"));
    expect(visibleToasts()).toHaveLength(1);
  });

  it("NTF-FR-PCVX, NTF-FR-11: shows a toast while the OS permission is not granted", async () => {
    setNotificationPermissionGranted(false);
    useWindow(base);
    await raiseNotification(raiseFor("a"));
    expect(visibleToasts()).toHaveLength(1);
  });

  it("NTF-FR-11: posts nothing to the OS with the switch off or permission refused", async () => {
    useWindow({ ...base, focused: false, mainFocused: false });
    setNotificationsEnabled(false);
    await raiseNotification(raiseFor("a"));
    setNotificationsEnabled(true);
    setNotificationPermissionGranted(false);
    await raiseNotification(raiseFor("b"));
    expect(posts()).toEqual([]);
    expect(visibleToasts()).toEqual([]);
  });
});

describe("suppression applies to a toast (NTF-FR-08, NTF-FR-10)", () => {
  it("NTF-FR-08, NTF-FR-10: shows no toast for the visible active tab", async () => {
    useWindow({ ...base, activeTarget: { kind: "dashboard" } });
    await raiseNotification(
      raiseFor("a", {
        address: mintAddress(PROJECT, WORKTREE, { kind: "dashboard" }),
      }),
    );
    expect(visibleToasts()).toEqual([]);
  });

  it("NTF-FR-08: shows no toast for the visible active panel surface", async () => {
    useWindow({ ...base, visiblePanelSurface: "comments" });
    await raiseNotification(
      raiseFor("a", {
        address: mintAddress(PROJECT, WORKTREE, {
          kind: "panel",
          surface: "comments",
        }),
      }),
    );
    expect(visibleToasts()).toEqual([]);
  });

  it("NTF-FR-08: shows no toast for the open settings window", async () => {
    useWindow({ ...base, openSettingsWindow: "global" });
    await raiseNotification(
      raiseFor("a", {
        address: mintAddress(PROJECT, WORKTREE, {
          kind: "settings",
          which: "global",
        }),
      }),
    );
    expect(visibleToasts()).toEqual([]);
  });

  it("NTF-FR-08, NTF-FR-39: shows no toast for the selected run in Runs, and shows one for another run", async () => {
    useWindow({ ...base, visibleBottomSurface: "runs", visibleRun: "r1" });
    await raiseNotification(raiseFor("r1"));
    expect(visibleToasts()).toEqual([]);
    await raiseNotification(raiseFor("r2"));
    expect(visibleToasts().map((t) => t.key)).toEqual(["r2"]);
  });

  it("NTF-FR-10: a suppressed raise replaces and withdraws nothing", async () => {
    useWindow({ ...base, visibleBottomSurface: "runs", visibleRun: "r1" });
    showToast({
      key: "r1",
      level: "Info",
      title: "old",
      body: "",
      address: runAddress("r1"),
    });
    await raiseNotification(raiseFor("r1", { title: "new" }));
    expect(visibleToasts()[0].title).toBe("old");
  });

  it("NTF-FR-08: an unparseable address shows nothing", async () => {
    useWindow(base);
    await raiseNotification(raiseFor("a", { address: "nonsense" }));
    expect(visibleToasts()).toEqual([]);
  });
});

describe("a toast leaves the stack (NTF-FR-HZNF, NTF-FR-ZSIK, NTF-FR-14, NTF-FR-15, NTF-FR-38)", () => {
  it("NTF-FR-HZNF: a raise with the key of a showing toast replaces it in place", async () => {
    useWindow(base);
    await raiseNotification(raiseFor("a"));
    await raiseNotification(raiseFor("b"));
    await raiseNotification(raiseFor("a", { level: "Warn", title: "again" }));
    expect(visibleToasts().map((t) => t.key)).toEqual(["b", "a"]);
    expect(visibleToasts()[1]).toMatchObject({ level: "Warn", title: "again" });
  });

  it("NTF-FR-HZNF, NTF-FR-SXTI: a fourth raise waits in the queue", async () => {
    useWindow(base);
    for (const k of ["a", "b", "c", "d"]) await raiseNotification(raiseFor(k));
    expect(visibleToasts()).toHaveLength(3);
    expect(queuedToasts().map((t) => t.key)).toEqual(["d"]);
  });

  it("NTF-FR-ZSIK: an OS raise replaces the showing toast of its key", async () => {
    useWindow(base);
    await raiseNotification(raiseFor("a"));
    await raiseNotification(raiseFor("b"));
    useWindow({ ...base, focused: false, mainFocused: false });
    await raiseNotification(raiseFor("a"));
    expect(visibleToasts().map((t) => t.key)).toEqual(["b"]);
    expect(posts()).toHaveLength(1);
  });

  it("NTF-FR-14: reaching the address removes its toast and no other", async () => {
    useWindow(base);
    await raiseNotification(raiseFor("a"));
    await raiseNotification(raiseFor("b"));
    notifyArrived(runAddress("a"));
    expect(visibleToasts().map((t) => t.key)).toEqual(["b"]);
  });

  it("NTF-FR-38: a retraction removes the toast of its key and no other", async () => {
    useWindow(base);
    await raiseNotification(raiseFor("a"));
    await raiseNotification(raiseFor("b"));
    retractNotification("a");
    expect(visibleToasts().map((t) => t.key)).toEqual(["b"]);
    retractNotification("a");
    retractNotification("never-raised");
    expect(visibleToasts().map((t) => t.key)).toEqual(["b"]);
  });

  it("NTF-FR-38, NTF-FR-SXTI: a retraction drops a queued toast", async () => {
    useWindow(base);
    for (const k of ["a", "b", "c", "d"]) await raiseNotification(raiseFor(k));
    retractNotification("d");
    expect(queuedToasts()).toEqual([]);
  });

  it("NTF-FR-38: a retraction works with the switch off and the permission refused", async () => {
    setNotificationsEnabled(false);
    setNotificationPermissionGranted(false);
    useWindow(base);
    await raiseNotification(raiseFor("a"));
    retractNotification("a");
    expect(visibleToasts()).toEqual([]);
  });

  it("NTF-FR-15, NTF-FR-22: a root change removes every toast, also one without an address", async () => {
    useWindow(base);
    await raiseNotification(raiseFor("a"));
    showToast({
      key: "unreachable-address",
      level: "Warn",
      title: "x",
      body: "",
      address: null,
    });
    withdrawForRoot(PROJECT, WORKTREE);
    expect(visibleToasts()).toEqual([]);
    expect(queuedToasts()).toEqual([]);
  });

  it("NTF-FR-14: dismissing a toast withdraws no OS notification", async () => {
    useWindow({ ...base, focused: false, mainFocused: false });
    await raiseNotification(raiseFor("os"));
    useWindow(base);
    await raiseNotification(raiseFor("t"));
    const before = invokeMock.mock.calls.filter(
      (c) => c[0] === "withdraw_notification",
    ).length;
    retractNotification("t");
    const after = invokeMock.mock.calls.filter(
      (c) => c[0] === "withdraw_notification",
    ).length;
    expect(after).toBe(before);
  });
});

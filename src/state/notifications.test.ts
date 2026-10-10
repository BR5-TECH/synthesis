import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  configureNotifications,
  decideIndicate,
  decidePost,
  isAlreadyVisible,
  notifyArrived,
  postedAddresses,
  notificationGate,
  projectSubtitle,
  raiseNotification,
  readNotificationPermission,
  resetNotifications,
  retractNotification,
  setNotificationPermissionGranted,
  setNotificationsEnabled,
  withdrawForRoot,
  type Raise,
  type WindowSnapshot,
} from "./notifications";
import { mintAddress, parseAddress } from "./notificationAddress";
import { resetLogBufferForTest } from "../logging";
import {
  hasRunIndication,
  indicatedAddresses,
  resetTabIndications,
  retainIndications,
} from "./tabIndications";

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

const withdrawals = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "withdraw_notification")
    .map((c) => (c[1] as { id: string }).id);

/** A window with nothing open and no focus — every raise posts from here. */
const backgrounded: WindowSnapshot = {
  focused: false,
  projectKey: PROJECT,
  worktree: WORKTREE,
  activeTarget: null,
  visiblePanelSurface: null,
  visibleBottomSurface: null,
  visibleRun: null,
  // SWN-FR-05 / NTF-FR-08: no settings child window is open.
  openSettingsWindow: null,
};

const raiseFor = (address: string): Raise => ({
  key: "run:abc",
  title: "A run finished",
  body: "It took an hour.",
  address,
});

const dashboard = mintAddress(PROJECT, WORKTREE, { kind: "dashboard" });
const fileAddress = mintAddress(PROJECT, WORKTREE, {
  kind: "file",
  path: "a.md",
});

let postCounter = 0;
beforeEach(() => {
  // A successful raise logs, and the log flush timer must not reach the next
  // test.
  resetLogBufferForTest();
  invokeMock.mockReset();
  postCounter = 0;
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "post_notification") return { id: `notif-${postCounter++}` };
    return undefined;
  });
  resetNotifications();
});

describe("the post policy (NTF-FR-08 through NTF-FR-11)", () => {
  const granted = { enabled: true, permissionGranted: true };

  it("posts whenever the window does not hold focus", () => {
    // NTF-FR-01, NTF-FR-08, NTF-FR-06. Even when the address names the tab that happens to be active
    // — an unfocused window has shown the author nothing.
    expect(decidePost(raiseFor(dashboard), backgrounded, granted)).toEqual({
      post: true,
    });
    expect(
      decidePost(
        raiseFor(dashboard),
        { ...backgrounded, activeTarget: { kind: "dashboard" } },
        granted,
      ),
    ).toEqual({ post: true });
  });

  it("suppresses only what the author is already looking at", () => {
    // NTF-FR-08: focused, and the address names the active tab.
    const focusedOnFile: WindowSnapshot = {
      ...backgrounded,
      focused: true,
      activeTarget: { kind: "file", path: "a.md" },
    };
    expect(decidePost(raiseFor(fileAddress), focusedOnFile, granted)).toEqual({
      post: false,
      reason: "already-looking",
    });

    // A different file, same window: posted.
    const other = mintAddress(PROJECT, WORKTREE, {
      kind: "file",
      path: "b.md",
    });
    expect(decidePost(raiseFor(other), focusedOnFile, granted)).toEqual({
      post: true,
    });

    // And the same address once the author moves to another tab.
    expect(
      decidePost(
        raiseFor(fileAddress),
        { ...focusedOnFile, activeTarget: { kind: "dashboard" } },
        granted,
      ),
    ).toEqual({ post: true });
  });

  it("counts a panel surface as visible only while its panel is showing", () => {
    // NTF-FR-08. A selected-but-hidden surface has told the author nothing, so
    // treating it as "already looking" would swallow the notification.
    const runs = mintAddress(PROJECT, WORKTREE, {
      kind: "bottom",
      surface: "runs",
    });
    const showing: WindowSnapshot = {
      ...backgrounded,
      focused: true,
      visibleBottomSurface: "runs",
    };
    expect(decidePost(raiseFor(runs), showing, granted)).toEqual({
      post: false,
      reason: "already-looking",
    });
    expect(
      decidePost(
        raiseFor(runs),
        { ...showing, visibleBottomSurface: null },
        granted,
      ),
    ).toEqual({ post: true });
    // A vertical-panel address is judged against the vertical panel, not the
    // bottom one — the two surfaces share names in neither direction, but the
    // lookup must not cross zones.
    const comments = mintAddress(PROJECT, WORKTREE, {
      kind: "panel",
      surface: "comments",
    });
    expect(
      decidePost(
        raiseFor(comments),
        { ...showing, visiblePanelSurface: "comments" },
        granted,
      ),
    ).toEqual({ post: false, reason: "already-looking" });
    // And the negative direction, which is where the App wiring was wrong:
    // `s.panelSurface` keeps naming the SELECTED surface while the panel is
    // hidden, so feeding it in unconditionally suppressed every raise about
    // whichever panel happened to be selected — silently (NTF-FR-10).
    expect(
      decidePost(
        raiseFor(comments),
        { ...showing, visiblePanelSurface: null },
        granted,
      ),
    ).toEqual({ post: true });
  });

  it("posts a raise made while no project is open (NTF-FR-09)", () => {
    // NTF-FR-09: the picker is never where a target lives, so nothing there can
    // be "already visible" — even with the window focused.
    const picker: WindowSnapshot = {
      focused: true,
      projectKey: null,
      worktree: null,
      activeTarget: null,
      visiblePanelSurface: null,
      visibleBottomSurface: null,
    visibleRun: null,
    // SWN-FR-05 / NTF-FR-08: no settings child window is open.
    openSettingsWindow: null,
    };
    expect(decidePost(raiseFor(dashboard), picker, granted)).toEqual({
      post: true,
    });
  });

  it("posts nothing at all while the switch is off (NTF-FR-11)", () => {
    // NTF-FR-11, NTF-FR-12: absolute, so being in the background does not override it.
    expect(
      decidePost(raiseFor(dashboard), backgrounded, {
        enabled: false,
        permissionGranted: true,
      }),
    ).toEqual({ post: false, reason: "disabled" });
  });

  it("posts nothing while the OS has not granted permission (NTF-FR-11)", () => {
    // NTF-FR-11, NTF-FR-13: likewise absolute, and checked without asking for anything.
    expect(
      decidePost(raiseFor(dashboard), backgrounded, {
        enabled: true,
        permissionGranted: false,
      }),
    ).toEqual({ post: false, reason: "permission" });
  });

  it("checks the switch before the permission, and both before focus", () => {
    // The order is the contract's, and it is observable through the reason:
    // an author who turned notifications off should be told that, not that the
    // OS refused.
    expect(
      decidePost(raiseFor(dashboard), backgrounded, {
        enabled: false,
        permissionGranted: false,
      }),
    ).toEqual({ post: false, reason: "disabled" });
  });

  it("declines to interrupt for an address nobody could be routed from", () => {
    expect(
      decidePost(raiseFor("not an address"), backgrounded, granted),
    ).toEqual({ post: false, reason: "unparseable-address" });
  });

  it("treats an address in another root as never already-visible", () => {
    // A file open in worktree A does not make a raise about the same path in
    // worktree B redundant.
    const inOther = parseAddress(
      mintAddress(PROJECT, "/dev/acme-other", { kind: "file", path: "a.md" }),
    )!;
    expect(
      isAlreadyVisible(inOther, {
        ...backgrounded,
        focused: true,
        activeTarget: { kind: "file", path: "a.md" },
      }),
    ).toBe(false);
  });
});

describe("a settings address is judged against the window (NTF-FR-08, SWN-FR-05)", () => {
  /** Looking at the Global settings window, which is open and holds focus. */
  const inGlobalSettings: WindowSnapshot = {
    ...backgrounded,
    focused: true,
    openSettingsWindow: "global",
  };
  const settingsAddress = (which: "global" | "project") =>
    mintAddress(PROJECT, WORKTREE, { kind: "settings", which });

  it("NTF-FR-25, NTF-FR-08: suppresses a raise naming the settings window that is open", () => {
    // The author has already been told by the thing itself. A settings surface
    // is a window rather than a tab (SWN-FR-01), so this is answered from the
    // window snapshot — the active TAB says nothing about it.
    expect(
      decidePost(raiseFor(settingsAddress("global")), inGlobalSettings, {
        enabled: true,
        permissionGranted: true,
      }),
    ).toEqual({ post: false, reason: "already-looking" });
  });

  it("posts a raise naming the OTHER settings window", () => {
    // At most one is ever open (SWN-FR-05), so the one that is not open is
    // somewhere the author is not.
    expect(
      decidePost(raiseFor(settingsAddress("project")), inGlobalSettings, {
        enabled: true,
        permissionGranted: true,
      }),
    ).toEqual({ post: true });
  });

  it("posts when no settings window is open, whatever the active tab claims", () => {
    // The defect this pins: the suppression used to read the active tab's
    // target, and the two settings surfaces used to BE tabs. A snapshot still
    // carrying a settings `activeTarget` must not suppress anything now — the
    // window is what decides, and none is open.
    const noWindow: WindowSnapshot = {
      ...inGlobalSettings,
      openSettingsWindow: null,
      activeTarget: { kind: "settings", which: "global" },
    };
    expect(
      decidePost(raiseFor(settingsAddress("global")), noWindow, {
        enabled: true,
        permissionGranted: true,
      }),
    ).toEqual({ post: true });
  });

  it("posts while no window of the application holds focus", () => {
    // NTF-FR-08: an unfocused application always posts, even for the window
    // that happens to be open behind it.
    expect(
      decidePost(
        raiseFor(settingsAddress("global")),
        { ...inGlobalSettings, focused: false },
        { enabled: true, permissionGranted: true },
      ),
    ).toEqual({ post: true });
  });

  it("NTF-FR-27 / NTF-FR-28: marks nothing anywhere, whatever the strip holds", () => {
    // A settings address is reached through its notification alone. Asserted
    // against a resolver that would happily hand back a tab, so the rule is the
    // reason rather than an accident of nothing matching.
    const parsed = parseAddress(settingsAddress("project"))!;
    expect(
      decideIndicate(parsed, () => ({ active: false }), backgrounded),
    ).toBe(false);
  });
});

describe("a run address is judged one step finer (NTF-FR-08, NTF-FR-39)", () => {
  const granted = { enabled: true, permissionGranted: true };
  const runAddress = mintAddress(PROJECT, WORKTREE, {
    kind: "run",
    runId: "artifact-window",
  });
  const looking: WindowSnapshot = {
    ...backgrounded,
    focused: true,
    visibleBottomSurface: "runs",
    visibleRun: "artifact-window",
  };

  it("suppresses only where Runs is showing that very run", () => {
    // NTF-FR-39, NTF-FR-26, NTF-FR-28, NTF-FR-31, NTF-FR-14, GRU-FR-BLSS (NTF-FR-08): a queue the author is reading is not the run that
    // has stopped.
    expect(decidePost(raiseFor(runAddress), looking, granted)).toEqual({
      post: false,
      reason: "already-looking",
    });
    // The panel hidden altogether.
    expect(
      decidePost(
        raiseFor(runAddress),
        { ...looking, visibleBottomSurface: null, visibleRun: null },
        granted,
      ),
    ).toEqual({ post: true });
    // Another bottom surface active.
    expect(
      decidePost(
        raiseFor(runAddress),
        { ...looking, visibleBottomSurface: "logs", visibleRun: null },
        granted,
      ),
    ).toEqual({ post: true });
    // Runs active on a different run.
    expect(
      decidePost(
        raiseFor(runAddress),
        { ...looking, visibleRun: "graduation-flow" },
        granted,
      ),
    ).toEqual({ post: true });
  });

  it("marks the Runs toggle rather than any tab, whenever the author is not on it", () => {
    // NTF-FR-08, NTF-FR-31, NTF-FR-14, GRU-FR-BLSS (NTF-FR-26, NTF-FR-28, NTF-FR-39): a run has no tab of its own,
    // so nothing in the strip is marked and the toggle carries it instead.
    const parsed = parseAddress(runAddress);
    expect(parsed).not.toBeNull();
    const noTab = () => null;
    expect(
      decideIndicate(parsed!, noTab, {
        ...looking,
        visibleBottomSurface: null,
        visibleRun: null,
      }),
    ).toBe(true);
    expect(
      decideIndicate(parsed!, noTab, {
        ...looking,
        visibleRun: "graduation-flow",
      }),
    ).toBe(true);
    // Its equivalent of the active tab: Runs showing, with that run selected.
    expect(decideIndicate(parsed!, noTab, looking)).toBe(false);
  });

  it("marks with the switch off and permission refused, exactly as a tab is", async () => {
    // NTF-FR-08, NTF-FR-39, NTF-FR-26, NTF-FR-28, NTF-FR-31, NTF-FR-14, GRU-FR-BLSS (NTF-FR-29): the two surfaces answer the same question for
    // authors in two places, and the gates that stop one do not stop the other.
    configureNotifications({
      snapshot: () => ({
        ...backgrounded,
        focused: true,
        visibleBottomSurface: "logs",
      }),
      resolveTab: () => null,
    });
    setNotificationsEnabled(false);
    setNotificationPermissionGranted(false);

    await raiseNotification(raiseFor(runAddress));
    expect(posts()).toHaveLength(0);
    expect(indicatedAddresses()).toEqual([runAddress]);
    expect(hasRunIndication({ addresses: new Set([runAddress]), pulsing: new Set() })).toBe(
      true,
    );
  });

  it("keeps one emphasis for two runs and clears each as it is reached", async () => {
    // NTF-FR-08, NTF-FR-39, NTF-FR-26, NTF-FR-28, GRU-FR-BLSS (NTF-FR-31, NTF-FR-14): reaching one run leaves the other's
    // mark standing.
    configureNotifications({
      snapshot: () => backgrounded,
      resolveTab: () => null,
    });
    const second = mintAddress(PROJECT, WORKTREE, {
      kind: "run",
      runId: "graduation-flow",
    });
    await raiseNotification({ ...raiseFor(runAddress), key: "run:one" });
    await raiseNotification({ ...raiseFor(second), key: "run:two" });
    expect(indicatedAddresses().sort()).toEqual([runAddress, second].sort());

    notifyArrived(runAddress);
    expect(indicatedAddresses()).toEqual([second]);
    notifyArrived(second);
    expect(indicatedAddresses()).toEqual([]);
  });

  it("survives the tab reconciliation, which is about tabs (NTF-FR-39)", async () => {
    // A run indication has no tab lifecycle: the Runs toggle is always present,
    // so reaping it against the strip would clear it the instant it was set.
    configureNotifications({
      snapshot: () => backgrounded,
      resolveTab: () => null,
    });
    await raiseNotification(raiseFor(runAddress));
    expect(indicatedAddresses()).toEqual([runAddress]);
    retainIndications(() => false);
    expect(indicatedAddresses()).toEqual([runAddress]);
  });
});

describe("raising (NTF-FR-01 / NTF-FR-02 / NTF-FR-23)", () => {
  const configure = (snapshot: WindowSnapshot) => {
    configureNotifications({
      snapshot: () => snapshot,
      resolveTab: () => null,
    });
    setNotificationsEnabled(true);
    setNotificationPermissionGranted(true);
  };

  it("carries the raise to the backend with the address as the payload", async () => {
    // NTF-FR-01, NTF-FR-08, NTF-FR-06: the address travels as the opaque payload, and the key is
    // passed through so the backend can coalesce on it (NTF-FR-07).
    configure(backgrounded);
    await expect(raiseNotification(raiseFor(dashboard))).resolves.toBe(true);
    expect(posts()).toEqual([
      {
        key: "run:abc",
        title: "A run finished",
        // NTF-FR-QGSV: the facility adds the project's name.
        subtitle: "acme",
        body: "It took an hour.",
        payload: dashboard,
      },
    ]);
  });

  it("posts nothing when the policy suppresses, and says so to nobody", async () => {
    // NTF-FR-10: silent to the author. The return value is the only signal, and
    // callers are free to ignore it.
    configure({
      ...backgrounded,
      focused: true,
      activeTarget: { kind: "dashboard" },
    });
    await expect(raiseNotification(raiseFor(dashboard))).resolves.toBe(false);
    expect(posts()).toEqual([]);
  });

  it("never throws when the backend refuses (NTF-FR-23)", async () => {
    // NTF-FR-23, NTF-FR-02: the raising surface is unaffected and no error is rendered.
    configure(backgrounded);
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "post_notification") throw "permission_denied";
      return undefined;
    });
    await expect(raiseNotification(raiseFor(dashboard))).resolves.toBe(false);
  });

  it("never throws when the window snapshot itself fails", async () => {
    // A raise must not be able to take down the surface that made it.
    configureNotifications({
      snapshot: () => {
        throw new Error("shell is mid-teardown");
      },
      resolveTab: () => null,
    });
    setNotificationsEnabled(true);
    setNotificationPermissionGranted(true);
    await expect(raiseNotification(raiseFor(dashboard))).resolves.toBe(false);
    expect(posts()).toEqual([]);
  });

  it("does nothing at all before the shell has configured it", async () => {
    // The picker mounts before the shell does; a raise there must be inert
    // rather than a crash.
    resetNotifications();
    await expect(raiseNotification(raiseFor(dashboard))).resolves.toBe(false);
    expect(posts()).toEqual([]);
  });
});

describe("withdrawal (NTF-FR-14 / NTF-FR-15)", () => {
  beforeEach(() => {
    // No tab is open on anything these raises address, so nothing here marks
    // one (NTF-FR-28) and every assertion below is about the posting alone.
    configureNotifications({
      snapshot: () => backgrounded,
      resolveTab: () => null,
    });
    setNotificationsEnabled(true);
    setNotificationPermissionGranted(true);
  });

  it("withdraws a notification once the author reaches what it addresses", async () => {
    // NTF-FR-14: reaching it by a route of the author's own, without ever
    // activating the notification.
    await raiseNotification(raiseFor(fileAddress));
    expect(postedAddresses()).toEqual([fileAddress]);

    notifyArrived(fileAddress);
    expect(withdrawals()).toEqual(["notif-0"]);
    expect(postedAddresses()).toEqual([]);

    // Idempotent: arriving again withdraws nothing a second time.
    notifyArrived(fileAddress);
    expect(withdrawals()).toEqual(["notif-0"]);
  });

  it("ignores arrival at somewhere nothing was raised about", () => {
    notifyArrived(dashboard);
    expect(withdrawals()).toEqual([]);
  });

  it("withdraws everything pointing into a root no longer being read", async () => {
    // NTF-FR-15: a worktree change, a project switch, or a project close.
    const otherRoot = mintAddress(PROJECT, "/dev/acme-other", {
      kind: "dashboard",
    });
    await raiseNotification({ ...raiseFor(fileAddress), key: "a" });
    await raiseNotification({ ...raiseFor(otherRoot), key: "b" });
    expect(postedAddresses()).toHaveLength(2);

    withdrawForRoot(PROJECT, WORKTREE);
    expect(withdrawals()).toEqual(["notif-0"]);
    expect(postedAddresses()).toEqual([otherRoot]);

    withdrawForRoot(PROJECT, "/dev/acme-other");
    expect(withdrawals()).toEqual(["notif-0", "notif-1"]);
    expect(postedAddresses()).toEqual([]);
  });

  it("survives a withdrawal the backend rejects", async () => {
    // The notification centre is not something the author can be shown an
    // error about, so a failed withdrawal must not reject into the shell.
    await raiseNotification(raiseFor(fileAddress));
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "withdraw_notification") throw new Error("gone");
      return { id: "notif-x" };
    });
    expect(() => notifyArrived(fileAddress)).not.toThrow();
  });
});

// ---------------------------------------------------------------------------
// NTF-FR-24, PCR-FR-26 … NTF-FR-01 — retraction (NTF-FR-38)
// ---------------------------------------------------------------------------

describe("retraction (NTF-FR-38)", () => {
  const first = mintAddress(PROJECT, WORKTREE, {
    kind: "file",
    path: "prompts/one.md",
  });
  const second = mintAddress(PROJECT, WORKTREE, {
    kind: "file",
    path: "prompts/two.md",
  });

  beforeEach(() => {
    resetTabIndications();
    configureNotifications({
      snapshot: () => backgrounded,
      // Each file address names an open tab that is not the active one, so
      // those raises mark their tab as well as posting (NTF-FR-26). The
      // unrelated raise addresses the Dashboard, which no strip entry stands
      // for here.
      resolveTab: (address) =>
        address.target.kind === "file" ? { active: false } : null,
    });
    setNotificationsEnabled(true);
    setNotificationPermissionGranted(true);
  });

  it("withdraws what one key posted and clears what it marked, and nothing else", async () => {
    await raiseNotification({ ...raiseFor(first), key: "prompt-proposal:p1" });
    await raiseNotification({ ...raiseFor(second), key: "prompt-proposal:p2" });
    await raiseNotification({ ...raiseFor(dashboard), key: "run:unrelated" });
    expect(posts()).toHaveLength(3);
    expect(indicatedAddresses().sort()).toEqual([first, second].sort());

    retractNotification("prompt-proposal:p1");

    expect(withdrawals()).toEqual(["notif-0"]);
    expect(indicatedAddresses()).toEqual([second]);
    // The second proposal's notification and the unrelated one are untouched.
    expect(postedAddresses().sort()).toEqual([second, dashboard].sort());
  });

  it("is a no-op for a key that posted nothing and marked nothing", () => {
    // NTF-FR-38, NTF-FR-29, NTF-FR-10: the raise was suppressed because its tab was active, so
    // nothing was posted and no tab was marked.
    expect(() => retractNotification("prompt-proposal:never")).not.toThrow();
    expect(withdrawals()).toEqual([]);
    expect(indicatedAddresses()).toEqual([]);
  });

  it("is a no-op for a key whose notification the author already reached", async () => {
    await raiseNotification({ ...raiseFor(first), key: "prompt-proposal:p1" });
    notifyArrived(first);
    expect(withdrawals()).toEqual(["notif-0"]);

    retractNotification("prompt-proposal:p1");
    expect(withdrawals()).toEqual(["notif-0"]);
  });

  it("clears the indication even where nothing was ever posted", async () => {
    // NTF-FR-38, NTF-FR-29, NTF-FR-10: the enable switch is off, so a marked tab stands with nothing
    // posted — and the retraction still clears the mark.
    setNotificationsEnabled(false);
    await raiseNotification({ ...raiseFor(first), key: "prompt-proposal:p1" });
    expect(posts()).toEqual([]);
    expect(indicatedAddresses()).toEqual([first]);

    retractNotification("prompt-proposal:p1");
    expect(indicatedAddresses()).toEqual([]);
    expect(withdrawals()).toEqual([]);
  });

  it("clears one key's mark and leaves another key's on the same tab", async () => {
    // NTF-FR-31 / NTF-FR-38: two proposals against one file address the same
    // tab, and deciding one must not clear the other's mark.
    await raiseNotification({ ...raiseFor(first), key: "prompt-proposal:p1" });
    await raiseNotification({ ...raiseFor(first), key: "prompt-proposal:p2" });
    expect(indicatedAddresses()).toEqual([first]);

    retractNotification("prompt-proposal:p1");
    expect(indicatedAddresses()).toEqual([first]);

    retractNotification("prompt-proposal:p2");
    expect(indicatedAddresses()).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// NTF-FR-01 — the facility is the only route (NTF-FR-01, NTF-FR-38)
// ---------------------------------------------------------------------------

describe("the one route in and the one route out (NTF-FR-01, NTF-FR-38)", () => {
  it("keeps every call site of the two commands inside this facility", async () => {
    const modules = import.meta.glob("../**/*.{ts,tsx}", {
      query: "?raw",
      import: "default",
      eager: true,
    }) as Record<string, string>;
    for (const [path, source] of Object.entries(modules)) {
      if (path.includes(".test.")) continue;
      if (path.endsWith("notifications.ts")) continue;
      // `api.ts` declares the two wrappers and `types.ts` names their shapes in
      // prose; the facility is the only thing that calls either.
      if (path.endsWith("/api.ts") || path.endsWith("/types.ts")) continue;
      for (const command of ["postNotification", "withdrawNotification"]) {
        expect(source, `${path} reaches ${command} directly`).not.toContain(
          `${command}(`,
        );
      }
    }
  });

  it("has exactly one surface that retracts", () => {
    // NTF-FR-01, NTF-FR-38: the prompt change review, and nothing else. That it retracts
    // **only on a decision** is a behaviour rather than a shape, and is asserted
    // against the shell subscriber itself in `App.promptProposal.test.tsx`.
    //
    // The surface is `useAppNotifications`, the main window's own notification
    // surface — split out of `App.tsx` so the file stays readable.
    const modules = import.meta.glob("../**/*.{ts,tsx}", {
      query: "?raw",
      import: "default",
      eager: true,
    }) as Record<string, string>;
    const callers = Object.entries(modules)
      .filter(([path]) => !path.includes(".test."))
      .filter(([path]) => !path.endsWith("notifications.ts"))
      .filter(([, source]) => source.includes("retractNotification("))
      .map(([path]) => path);
    expect(callers).toEqual(["../hooks/useAppNotifications.ts"]);
  });
});

describe("the gate is live, not a snapshot (NTF-FR-12)", () => {
  // The defect this pins: the switch and the platform's disposition used to be
  // read once into the shell's React state at startup, so turning the switch on
  // — or granting permission in order to try the rehearsal — did not take
  // effect until the application was relaunched. NTF-FR-12 requires the
  // opposite, and GLS-FR-26, GLS-FR-27's last clause depends on it.

  beforeEach(() => {
    // No tab is open on anything these raises address, so nothing here marks
    // one (NTF-FR-28) and every assertion below is about the posting alone.
    configureNotifications({
      snapshot: () => backgrounded,
      resolveTab: () => null,
    });
  });

  it("defaults to notifications on, permission not assumed", () => {
    // GSS-FR-32 for the switch; the safe direction for the platform, which has
    // not answered yet at the moment the module loads.
    expect(notificationGate()).toEqual({
      enabled: true,
      permissionGranted: false,
    });
  });

  it("stops posting the moment the switch is turned off, with no relaunch", async () => {
    // NTF-FR-11, NTF-FR-12.
    setNotificationsEnabled(true);
    setNotificationPermissionGranted(true);
    await expect(raiseNotification(raiseFor(dashboard))).resolves.toBe(true);
    expect(posts()).toHaveLength(1);

    setNotificationsEnabled(false);
    await expect(raiseNotification(raiseFor(dashboard))).resolves.toBe(false);
    expect(posts()).toHaveLength(1);

    setNotificationsEnabled(true);
    await expect(raiseNotification(raiseFor(dashboard))).resolves.toBe(true);
    expect(posts()).toHaveLength(2);
  });

  it("starts posting the moment permission is granted, with no relaunch", async () => {
    // NTF-FR-11, NTF-FR-13 / GLS-FR-26, GLS-FR-27: the author grants permission specifically so they
    // can press the rehearsal, and the very next raise must honour it.
    setNotificationsEnabled(true);
    setNotificationPermissionGranted(false);
    await expect(raiseNotification(raiseFor(dashboard))).resolves.toBe(false);
    expect(posts()).toHaveLength(0);

    setNotificationPermissionGranted(true);
    await expect(raiseNotification(raiseFor(dashboard))).resolves.toBe(true);
    expect(posts()).toHaveLength(1);
  });
});

describe("the subtitle names the project (NTF-FR-QGSV)", () => {
  beforeEach(() => {
    resetNotifications();
    resetTabIndications();
    configureNotifications({
      snapshot: () => backgrounded,
      resolveTab: () => null,
    });
    setNotificationPermissionGranted(true);
  });

  it("NTF-FR-QGSV: names the project by the last segment of its key", async () => {
    expect(await raiseNotification(raiseFor(dashboard))).toBe(true);
    expect(posts()[0].subtitle).toBe("acme");
    // The raise's own words reach the backend unchanged beside it.
    expect(posts()[0].title).toBe("A run finished");
    expect(posts()[0].body).toBe("It took an hour.");
  });

  it("NTF-FR-QGSV: names the project the address names, not the open one", async () => {
    const elsewhere = mintAddress("/dev/other-project", "/dev/other-project", {
      kind: "dashboard",
    });
    await raiseNotification({ ...raiseFor(elsewhere), key: "elsewhere" });
    expect(posts()[0].subtitle).toBe("other-project");
  });

  it("NTF-FR-QGSV: every surface's raise gets the same subtitle for one project", async () => {
    await raiseNotification({ ...raiseFor(dashboard), key: "a" });
    await raiseNotification({ ...raiseFor(fileAddress), key: "b" });
    await raiseNotification({
      ...raiseFor(mintAddress(PROJECT, WORKTREE, { kind: "run", runId: "r1" })),
      key: "c",
    });
    expect(posts().map((p) => p.subtitle)).toEqual(["acme", "acme", "acme"]);
  });

  it("NTF-FR-QGSV: a subtitle a surface tries to send is ignored", async () => {
    const raise = { ...raiseFor(dashboard), subtitle: "something else" } as Raise;
    await raiseNotification(raise);
    expect(posts()[0].subtitle).toBe("acme");
  });

  it.each([
    ["/dev/acme", "acme"],
    ["/dev/acme/", "acme"],
    ["/dev/acme//", "acme"],
    ["C:\\dev\\acme", "acme"],
    ["C:\\dev\\acme\\", "acme"],
    ["C:\\dev/acme", "acme"],
    ["~/dev/acme", "acme"],
    ["/dev/проект", "проект"],
    ["acme", "acme"],
    ["/", ""],
  ])("NTF-FR-QGSV: the key %s gives the name %s", (projectKey, name) => {
    const address = parseAddress(mintAddress(projectKey, projectKey, { kind: "dashboard" }));
    expect(address).not.toBeNull();
    expect(projectSubtitle(address!)).toBe(name);
  });
});

describe("reading the permission (NTF-FR-13, NTF-FR-WUUY)", () => {
  type Answer = { resolve: (state: string) => void; reject: (e: unknown) => void };

  /** Each permission read waits for the test to answer it. */
  const deferReads = () => {
    const answers: Answer[] = [];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "get_notification_permission") {
        return new Promise((resolve, reject) => answers.push({ resolve, reject }));
      }
      return Promise.resolve(undefined);
    });
    return answers;
  };

  beforeEach(() => {
    resetNotifications();
  });

  it("NTF-FR-WUUY: the read started last decides, whatever order the answers come in", async () => {
    const answers = deferReads();
    const startup = readNotificationPermission("startup");
    const focus = readNotificationPermission("window focus");
    answers[1].resolve("denied");
    await focus;
    answers[0].resolve("granted");
    await startup;
    expect(notificationGate().permissionGranted).toBe(false);
    expect(invokeMock.mock.calls.some((c) => c[0] === "request_notification_permission")).toBe(
      false,
    );
  });

  it("NTF-FR-WUUY: a read that fails closes the gate", async () => {
    setNotificationPermissionGranted(true);
    const answers = deferReads();
    const read = readNotificationPermission("window focus");
    answers[0].reject(new Error("no backend"));
    await read;
    expect(notificationGate().permissionGranted).toBe(false);
  });

  it("NTF-FR-13: a read still in flight at a reset does not apply", async () => {
    const answers = deferReads();
    const read = readNotificationPermission("startup");
    resetNotifications();
    answers[0].resolve("granted");
    await read;
    expect(notificationGate().permissionGranted).toBe(false);
  });
});

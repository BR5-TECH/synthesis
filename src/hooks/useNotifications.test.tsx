import { describe, expect, it } from "vitest";

import { resolveActivation, type ActivationContext } from "./useNotifications";
import { mintAddress } from "../state/notificationAddress";

/**
 * Activation routing (`NTF-notifications.md` NTF-FR-17 / NTF-FR-19), covering
 * NTF-FR-16, NTF-FR-17 through NTF-FR-20.
 */

const PROJECT = "/dev/acme";
const WORKTREE = "/dev/acme";

const context = (over: Partial<ActivationContext> = {}): ActivationContext => ({
  projectKey: PROJECT,
  worktree: WORKTREE,
  fileExists: () => true,
  draftExists: () => true,
  ...over,
});

const address = (
  target: Parameters<typeof mintAddress>[2],
  project = PROJECT,
  worktree = WORKTREE,
) => mintAddress(project, worktree, target);

describe("routing an address that resolves (NTF-FR-17)", () => {
  it("opens each target kind the grammar admits", () => {
    // NTF-FR-16, NTF-FR-17 / TAB-FR-17 / NTF-FR-18, SNV-FR-45, SNV-FR-47 / NTF-FR-25, GLS-FR-27, and the routing half of
    // NTF-FR-03, NTF-FR-17.
    const cases: [Parameters<typeof mintAddress>[2], unknown][] = [
      [{ kind: "dashboard" }, { kind: "dashboard" }],
      [
        { kind: "file", path: "specifications/ui/RUN-runs.md" },
        { kind: "file", path: "specifications/ui/RUN-runs.md" },
      ],
      [
        { kind: "draft", draftId: "d-1" },
        { kind: "draft", draftId: "d-1" },
      ],
      [
        { kind: "panel", surface: "comments" },
        { kind: "panel", surface: "comments" },
      ],
      [
        { kind: "bottom", surface: "git" },
        { kind: "bottom", surface: "git" },
      ],
      [
        { kind: "settings", which: "global" },
        { kind: "settings", which: "global" },
      ],
      [
        { kind: "settings", which: "project" },
        { kind: "settings", which: "project" },
      ],
    ];

    for (const [target, expected] of cases) {
      const outcome = resolveActivation(address(target), context());
      expect(outcome, JSON.stringify(target)).toEqual({
        kind: "open",
        target: expected,
      });
    }
  });
});

describe("routing an address that does not resolve (NTF-FR-19)", () => {
  const isRefusal = (outcome: ReturnType<typeof resolveActivation>) => {
    expect(outcome.kind).toBe("state");
    // The statement never leaks the address into what the author reads.
    if (outcome.kind === "state") {
      expect(outcome.message).not.toContain("synthesis://");
      expect(outcome.message.length).toBeGreaterThan(0);
    }
  };

  it("changes nothing when the address names another project", () => {
    // NTF-FR-19, NTF-FR-20: no project is switched, and the distinction is named so the
    // author knows why nothing happened.
    const outcome = resolveActivation(
      address({ kind: "dashboard" }, "/dev/other"),
      context(),
    );
    isRefusal(outcome);
    expect(outcome).toEqual({
      kind: "state",
      message: "That notification points into a different project.",
    });
  });

  it("changes nothing when the address names another worktree", () => {
    // NTF-FR-19, NTF-FR-20: distinct from the project case, because they are different
    // situations to be in.
    const outcome = resolveActivation(
      address({ kind: "dashboard" }, PROJECT, "/dev/acme-other"),
      context(),
    );
    isRefusal(outcome);
    expect(outcome).toEqual({
      kind: "state",
      message: "That notification points into a different worktree.",
    });
  });

  it("changes nothing when no project is open", () => {
    // NTF-FR-19: the Project picker is left exactly as it is.
    const outcome = resolveActivation(
      address({ kind: "dashboard" }),
      context({ projectKey: null, worktree: null }),
    );
    isRefusal(outcome);
  });

  it("names a file that is no longer in the project", () => {
    // NTF-FR-19, NTF-FR-20: and names it by basename, which is what the author would
    // recognise from the tab strip.
    const outcome = resolveActivation(
      address({ kind: "file", path: "specifications/ui/RUN-runs.md" }),
      context({ fileExists: () => false }),
    );
    isRefusal(outcome);
    if (outcome.kind === "state") {
      expect(outcome.message).toContain("RUN-runs.md");
      expect(outcome.message).not.toContain("specifications/ui");
    }
  });

  it("refuses a draft that no longer exists", () => {
    const outcome = resolveActivation(
      address({ kind: "draft", draftId: "gone" }),
      context({ draftExists: () => false }),
    );
    isRefusal(outcome);
  });

  it("treats an unparseable payload exactly as an unreachable one", () => {
    // NTF-FR-19, NTF-FR-20: the difference is not one the author can act on, so it is not
    // one they are told about.
    for (const payload of ["", "gibberish", "synthesis://p/w/nope"]) {
      isRefusal(resolveActivation(payload, context()));
    }
  });

  it("checks the root before the target, so a stale file elsewhere is a root problem", () => {
    // A file address naming another worktree must report the worktree rather
    // than "no longer in the project" — the file may well still be there, just
    // not in the root being read.
    const outcome = resolveActivation(
      address({ kind: "file", path: "a.md" }, PROJECT, "/dev/other"),
      context({ fileExists: () => false }),
    );
    expect(outcome).toEqual({
      kind: "state",
      message: "That notification points into a different worktree.",
    });
  });

  it("never consults existence for a target that has none", () => {
    // A panel, a bottom surface, the Dashboard, and the settings tabs always
    // exist while a project is open — asking a file predicate about them would
    // be a category error, and a `false`-returning one must not block them.
    const ctx = context({
      fileExists: () => false,
      draftExists: () => false,
    });
    for (const target of [
      { kind: "dashboard" } as const,
      { kind: "panel", surface: "drafts" } as const,
      { kind: "bottom", surface: "logs" } as const,
      { kind: "settings", which: "global" } as const,
    ]) {
      expect(resolveActivation(address(target), ctx).kind).toBe("open");
    }
  });
});

// ---------------------------------------------------------------------------
// The hook itself (NTF-FR-16 / NTF-FR-17 / NTF-FR-18), not just its decision
// function. The dispatch switch is where a routed target actually reaches a
// surface, and NTF-FR-17, NTF-FR-18, SNV-FR-45, SNV-FR-47's "the vertical panel is untouched" is a claim about
// which handlers DON'T run.
// ---------------------------------------------------------------------------

import { act, cleanup, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, vi } from "vitest";
import { useNotifications, type ActivationHandlers } from "./useNotifications";
import { NOTIFICATION_ACTIVATED } from "../events";
import {
  configureNotifications,
  postedAddresses,
  raiseNotification,
  resetNotifications,
  setNotificationPermissionGranted,
  setNotificationsEnabled,
} from "../state/notifications";

const listeners: ((e: { payload: unknown }) => void)[] = [];

vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, cb: (e: { payload: unknown }) => void) => {
    if (name === "notification-activated") listeners.push(cb);
    return Promise.resolve(() => {
      const i = listeners.indexOf(cb);
      if (i >= 0) listeners.splice(i, 1);
    });
  },
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string) =>
    Promise.resolve(cmd === "post_notification" ? { id: "notif-0" } : undefined),
}));

const fire = async (payload: string) => {
  await act(async () => {
    for (const l of [...listeners]) l({ payload: { id: "n1", key: "k", payload } });
  });
};

const handlers = (): ActivationHandlers & { calls: string[] } => {
  const calls: string[] = [];
  return {
    calls,
    openFile: (p) => calls.push(`file:${p}`),
    openDraft: (d) => calls.push(`draft:${d}`),
    openDashboard: () => calls.push("dashboard"),
    openPanelSurface: (s) => calls.push(`panel:${s}`),
    openBottomSurface: (s) => calls.push(`bottom:${s}`),
    openGlobalSettings: () => calls.push("settings:global"),
    openRun: (r) => calls.push(`run:${r}`),
    openProjectSettings: () => calls.push("settings:project"),
  };
};

beforeEach(() => {
  listeners.length = 0;
});
afterEach(cleanup);

describe("the hook dispatches an activation (NTF-FR-17 / NTF-FR-18)", () => {
  it("routes each target to exactly its own handler and no other", async () => {
    // NTF-FR-17, NTF-FR-18, SNV-FR-45, SNV-FR-47: "the vertical panel is untouched" / "the bottom panel is
    // untouched" are assertions about what did NOT run, which is why every case
    // checks the whole call list rather than just its own entry.
    const cases: [Parameters<typeof mintAddress>[2], string][] = [
      [{ kind: "dashboard" }, "dashboard"],
      [{ kind: "file", path: "a.md" }, "file:a.md"],
      [{ kind: "draft", draftId: "d1" }, "draft:d1"],
      [{ kind: "panel", surface: "comments" }, "panel:comments"],
      [{ kind: "bottom", surface: "git" }, "bottom:git"],
      [{ kind: "settings", which: "global" }, "settings:global"],
      [{ kind: "settings", which: "project" }, "settings:project"],
    ];
    for (const [target, expected] of cases) {
      listeners.length = 0;
      const h = handlers();
      const { result, unmount } = renderHook(() =>
        useNotifications(context(), h),
      );
      await act(async () => {});
      await fire(address(target));
      expect(h.calls, expected).toEqual([expected]);
      expect(result.current.statement).toBeNull();
      unmount();
    }
  });

  it("invokes no handler at all when the address cannot be reached", async () => {
    // NTF-FR-19: "changes nothing" is the requirement, so the observable is an
    // empty call list alongside the statement.
    const h = handlers();
    const { result } = renderHook(() =>
      useNotifications(context({ fileExists: () => false }), h),
    );
    await act(async () => {});
    await fire(address({ kind: "file", path: "gone.md" }));
    expect(h.calls).toEqual([]);
    expect(result.current.statement).toContain("gone.md");
  });

  it("replaces a statement rather than stacking, and clears on dismissal", async () => {
    const h = handlers();
    const { result } = renderHook(() =>
      useNotifications(context({ projectKey: null, worktree: null }), h),
    );
    await act(async () => {});
    await fire(address({ kind: "dashboard" }));
    const first = result.current.statement;
    expect(first).toBeTruthy();

    await fire("not an address at all");
    expect(result.current.statement).toBeTruthy();
    act(() => result.current.dismissStatement());
    expect(result.current.statement).toBeNull();
  });

  it("keeps one subscription across a project change rather than resubscribing", async () => {
    // The comment in the hook calls this load-bearing: an activation landing
    // while the listener was being torn down and rebuilt would be lost, and a
    // notification clicked once has no second chance.
    const h = handlers();
    const { rerender } = renderHook(
      ({ key }: { key: string }) =>
        useNotifications(context({ projectKey: key, worktree: key }), h),
      { initialProps: { key: "/dev/a" } },
    );
    await act(async () => {});
    expect(listeners).toHaveLength(1);

    rerender({ key: "/dev/b" });
    await act(async () => {});
    expect(listeners).toHaveLength(1);

    // And it routes against the CURRENT project, not the one it subscribed with.
    await fire(mintAddress("/dev/b", "/dev/b", { kind: "dashboard" }));
    expect(h.calls).toEqual(["dashboard"]);
  });

  it("stops tracking the notification the activation consumed", async () => {
    // NTD-FR-10 already withdrew it backend-side, so the facility must forget
    // it too — otherwise NTF-FR-14 would later ask the backend to withdraw an
    // id that is long gone the next time the author reached that target.
    const addr = address({ kind: "dashboard" });
    resetNotifications();
    configureNotifications({
      snapshot: () => ({
        focused: false,
        projectKey: PROJECT,
        worktree: WORKTREE,
        activeTarget: null,
        visiblePanelSurface: null,
        visibleBottomSurface: null,
    visibleRun: null,
    // SWN-FR-05 / NTF-FR-08: no settings child window is open.
    openSettingsWindow: null,
      }),
      resolveTab: () => null,
    });
    setNotificationsEnabled(true);
    setNotificationPermissionGranted(true);
    await raiseNotification({
      key: "k",
      title: "t",
      body: "b",
      address: addr,
    });
    expect(postedAddresses()).toEqual([addr]);

    const h = handlers();
    renderHook(() => useNotifications(context(), h));
    await act(async () => {});
    await fire(addr);
    expect(h.calls).toEqual(["dashboard"]);
    expect(postedAddresses()).toEqual([]);
  });

  it("tears the subscription down on unmount", async () => {
    const h = handlers();
    const { unmount } = renderHook(() => useNotifications(context(), h));
    await act(async () => {});
    expect(listeners).toHaveLength(1);
    unmount();
    expect(listeners).toHaveLength(0);
  });
});

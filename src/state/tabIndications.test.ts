/**
 * The in-app tab indication (`NTF-notifications.md` NTF-FR-26 through
 * NTF-FR-36).
 *
 * Two halves are exercised here: the store's own lifecycle — aggregation,
 * per-key clearing, the bounded pulse, reconciliation against the strip — and
 * the eligibility decision the facility makes at raise time, including the part
 * that matters most, that the enable switch and the operating system's
 * permission govern the posting alone (NTF-FR-29).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  configureNotifications,
  decideIndicate,
  notifyArrived,
  raiseNotification,
  postedAddresses,
  resetNotifications,
  setNotificationPermissionGranted,
  setNotificationsEnabled,
  withdrawForRoot,
  type Raise,
  type WindowSnapshot,
} from "./notifications";
import {
  mintAddress,
  parseAddress,
  resolveTabForAddress,
  sameTarget,
  targetForTab,
  type NotificationAddress,
} from "./notificationAddress";
import {
  clearIndicationsForAddress,
  hasRunIndication,
  indicatedAddresses,
  markIndication,
  pulsingAddresses,
  PULSE_MS,
  retainIndications,
  subscribe,
  resetTabIndications,
} from "./tabIndications";
import type { Tab } from "../types";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const PROJECT = "/dev/acme";
const WORKTREE = "/dev/acme";

const posts = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "post_notification");
const permissionRequests = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "request_notification_permission");

const fileAddress = (path: string, worktree = WORKTREE) =>
  mintAddress(PROJECT, worktree, { kind: "file", path });
const draftAddress = (draftId: string) =>
  mintAddress(PROJECT, WORKTREE, { kind: "draft", draftId });

/** The tabs a test says are open, in the shape the strip holds them. */
const editorTab = (path: string): Tab => ({
  id: `art:${path}`,
  label: path,
  kind: "editor",
  artifactId: path,
});
const draftTab = (draftId: string): Tab => ({
  id: `draft:${draftId}`,
  label: draftId,
  kind: "draft",
  draftId,
});
const diffTab = (path: string): Tab => ({
  id: `diff:${path}`,
  label: path,
  kind: "diff",
  diff: { path } as Tab["diff"],
});
const historyTab = (path: string): Tab => ({
  id: `hist:${path}`,
  label: path,
  kind: "editor",
  // A History detail tab shows an old revision and owns no artifact to address
  // (NTF-FR-03), so it carries no `artifactId`.
});
const dashboardTab: Tab = { id: "dashboard", label: "Dashboard" };

/**
 * Wire the facility to a strip. `active` names the active tab's id; every other
 * tab in the list is a background tab.
 */
function openStrip(tabs: Tab[], active: string, worktree = WORKTREE) {
  const snapshot: WindowSnapshot = {
    // Focused, so the post policy is doing real work rather than waving
    // everything through — the indication must hold up beside it.
    focused: true,
    projectKey: PROJECT,
    worktree,
    activeTarget: targetForTab(tabs.find((t) => t.id === active) ?? tabs[0]),
    visiblePanelSurface: null,
    visibleBottomSurface: null,
    visibleRun: null,
    // SWN-FR-05 / NTF-FR-08: no settings child window is open.
    openSettingsWindow: null,
  };
  configureNotifications({
    snapshot: () => snapshot,
    resolveTab: (address: NotificationAddress) => {
      if (address.projectKey !== PROJECT || address.worktree !== worktree) {
        return null;
      }
      const tab = tabs.find((t) => sameTarget(targetForTab(t), address.target));
      return tab ? { active: tab.id === active } : null;
    },
  });
  return { snapshot };
}

const raise = (key: string, address: string): Raise => ({
  key,
  title: "Something happened",
  body: "in a tab you are not reading",
  address,
});

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "post_notification") return Promise.resolve({ id: "n1" });
    if (cmd === "get_notification_permission")
      return Promise.resolve("granted");
    return Promise.resolve(undefined);
  });
  resetNotifications();
  resetTabIndications();
  setNotificationsEnabled(true);
  setNotificationPermissionGranted(true);
});

afterEach(() => {
  vi.useRealTimers();
  resetTabIndications();
  resetNotifications();
});

// ---------------------------------------------------------------------------
// Eligibility
// ---------------------------------------------------------------------------

describe("what is eligible for an indication (NTF-FR-26 / NTF-FR-27 / NTF-FR-28)", () => {
  // NTF-FR-08, NTF-FR-26, NTF-FR-37.
  it("marks an open background tab and leaves the active tab alone", async () => {
    openStrip([editorTab("a.md"), editorTab("b.md")], "art:a.md");

    await raiseNotification(raise("k1", fileAddress("b.md")));

    expect(indicatedAddresses()).toEqual([fileAddress("b.md")]);
    // NTF-FR-29 / NTF-FR-37: the active tab is untouched and the raise posted
    // exactly as the policy says it should, the window being focused on
    // something else.
    expect(posts()).toHaveLength(1);
  });

  // NTF-FR-26, NTF-FR-29, second clause: the active tab is never the marked tab, even when
  // the window is unfocused and the raise therefore posts.
  it("never marks the active tab", async () => {
    openStrip([editorTab("a.md"), editorTab("b.md")], "art:a.md");

    await raiseNotification(raise("k1", fileAddress("a.md")));

    expect(indicatedAddresses()).toEqual([]);
  });

  // NTF-FR-28.
  it("marks nothing when no tab is open on the address, and opens none", async () => {
    openStrip([editorTab("a.md")], "art:a.md");

    await raiseNotification(raise("k1", fileAddress("z.md")));

    expect(indicatedAddresses()).toEqual([]);
    // The notification remains the whole route to a target the author has not
    // opened (NTF-FR-28).
    expect(posts()).toHaveLength(1);
  });

  // NTF-FR-28, second clause: the Home affordance is a control of the strip
  // rather than a tab, so a Dashboard address with the tab closed marks nothing.
  it("marks nothing for the Dashboard while its tab is closed", async () => {
    openStrip([editorTab("a.md")], "art:a.md");

    await raiseNotification(
      raise("k1", mintAddress(PROJECT, WORKTREE, { kind: "dashboard" })),
    );

    expect(indicatedAddresses()).toEqual([]);
  });

  // NTF-FR-28.
  it("marks nothing for a panel or bottom-panel address", async () => {
    openStrip([editorTab("a.md"), dashboardTab], "art:a.md");

    await raiseNotification(
      raise(
        "k1",
        mintAddress(PROJECT, WORKTREE, { kind: "panel", surface: "comments" }),
      ),
    );
    await raiseNotification(
      raise(
        "k2",
        mintAddress(PROJECT, WORKTREE, { kind: "bottom", surface: "git" }),
      ),
    );

    expect(indicatedAddresses()).toEqual([]);
    expect(posts()).toHaveLength(2);
  });

  // NTF-FR-27, TAB-FR-06: the editing tab is the one an activation lands on, so it is the
  // one marked — a Diff tab on the same file is not (TAB-FR-06).
  it("marks the editing tab rather than a Diff tab on the same file", async () => {
    openStrip(
      [dashboardTab, editorTab("a.md"), diffTab("a.md")],
      "dashboard",
    );

    await raiseNotification(raise("k1", fileAddress("a.md")));

    expect(indicatedAddresses()).toEqual([fileAddress("a.md")]);
    // One address, and it is the editing tab's — `targetForTab` yields nothing
    // for a Diff tab at all.
    expect(targetForTab(diffTab("a.md"))).toBeNull();
  });

  /**
   * The root check, exercised through the function production actually uses
   * rather than through this file's own resolver — the strip at
   * `/dev/acme-wt2` holds a tab at the same project-relative path, and an
   * address into the worktree the application is not reading must still find
   * nothing.
   */
  it("resolves no tab for an address into another root", () => {
    const strip = {
      tabs: [editorTab("a.md"), editorTab("b.md")],
      activeTab: "art:a.md",
      projectKey: PROJECT,
      worktree: WORKTREE,
    };

    expect(
      resolveTabForAddress(parseAddress(fileAddress("b.md"))!, strip),
    ).toEqual({ tabId: "art:b.md", active: false });
    expect(
      resolveTabForAddress(parseAddress(fileAddress("a.md"))!, strip),
    ).toEqual({ tabId: "art:a.md", active: true });

    // Same path, other worktree.
    expect(
      resolveTabForAddress(
        parseAddress(fileAddress("b.md", "/dev/acme-wt2"))!,
        strip,
      ),
    ).toBeNull();
    // Same path, other project.
    expect(
      resolveTabForAddress(
        parseAddress(mintAddress("/dev/other", WORKTREE, {
          kind: "file",
          path: "b.md",
        }))!,
        strip,
      ),
    ).toBeNull();
    // Nothing open on it at all.
    expect(
      resolveTabForAddress(parseAddress(fileAddress("z.md"))!, strip),
    ).toBeNull();
  });

  // NTF-FR-27 / NTF-FR-28 / SWN-FR-18: a settings address marks nothing
  // anywhere, a settings surface being a native child window rather than a tab
  // of this strip (SWN-FR-01), so no tab can stand in for one.
  it("resolves the Dashboard by its own tab, and neither settings window", () => {
    const strip = {
      tabs: [dashboardTab, editorTab("a.md")] as Tab[],
      activeTab: "art:a.md",
      projectKey: PROJECT,
      worktree: WORKTREE,
    };
    const at = (target: Parameters<typeof mintAddress>[2]) =>
      resolveTabForAddress(
        parseAddress(mintAddress(PROJECT, WORKTREE, target))!,
        strip,
      );

    expect(at({ kind: "dashboard" })).toEqual({
      tabId: "dashboard",
      active: false,
    });
    // NTF-FR-27 / NTF-FR-28 / SWN-FR-18: a settings address marks nothing
    // anywhere and is reached through its notification alone, a settings
    // surface being a native child window rather than a tab of this strip
    // (SWN-FR-01). No tab can stand in for one, whatever the strip holds.
    expect(at({ kind: "settings", which: "global" })).toBeNull();
    expect(at({ kind: "settings", which: "project" })).toBeNull();
  });

  /**
   * The store holds the address the *raiser* minted; the strip re-mints one
   * from the tab it is drawing and looks it up. The two have to agree
   * byte-for-byte or an indication is stored that nothing can ever render and
   * nothing can ever clear — a silent failure no assertion on the store alone
   * would catch.
   */
  it("mints the same address from a raise and from the tab it names", () => {
    for (const tab of [
      editorTab("docs/a b.md"),
      editorTab("docs/100% done.md"),
      draftTab("d/1"),
      dashboardTab,
    ]) {
      const target = targetForTab(tab)!;
      const raised = mintAddress(PROJECT, WORKTREE, target);
      const drawn = mintAddress(PROJECT, WORKTREE, targetForTab(tab)!);
      expect(drawn).toBe(raised);
      // And it survives the round trip the facility puts it through.
      expect(parseAddress(raised)!.target).toEqual(target);
    }
  });

  it("marks nothing for an address into another root", async () => {
    openStrip([editorTab("a.md"), editorTab("b.md")], "art:a.md");

    await raiseNotification(
      raise("k1", fileAddress("b.md", "/dev/acme-wt2")),
    );

    expect(indicatedAddresses()).toEqual([]);
  });

  /**
   * NTF-FR-27 / TAB-FR-06: an Editor tab and a Diff tab on one file, and only
   * the Editor tab is what an activation lands on.
   *
   * Resolved rather than marked, because both tabs would store the identical
   * address — an assertion on the stored address cannot tell the two apart, so
   * it has to be the resolution that is checked, and the rendering half is
   * covered in `TabStrip.test.tsx`.
   */
  it("resolves a file address to the editing tab, never a Diff or History tab", () => {
    const strip = {
      tabs: [dashboardTab, diffTab("a.md"), editorTab("a.md")],
      activeTab: "dashboard",
      projectKey: PROJECT,
      worktree: WORKTREE,
    };
    expect(
      resolveTabForAddress(parseAddress(fileAddress("a.md"))!, strip),
    ).toEqual({ tabId: "art:a.md", active: false });

    // With the Diff tab alone in the strip, the address resolves to nothing —
    // it is not a lesser match, it is not a match.
    expect(
      resolveTabForAddress(parseAddress(fileAddress("a.md"))!, {
        ...strip,
        tabs: [dashboardTab, diffTab("a.md"), historyTab("a.md")],
      }),
    ).toBeNull();
  });

  // The decision itself, without a facility around it.
  it("decides from the target kind and the tab alone", () => {
    const address = parseAddress(fileAddress("b.md"))!;
    expect(decideIndicate(address, () => ({ active: false }))).toBe(true);
    expect(decideIndicate(address, () => ({ active: true }))).toBe(false);
    expect(decideIndicate(address, () => null)).toBe(false);

    const panel = parseAddress(
      mintAddress(PROJECT, WORKTREE, { kind: "panel", surface: "notes" }),
    )!;
    // A panel address is refused before the resolver is even consulted.
    const resolver = vi.fn(() => ({ active: false }));
    expect(decideIndicate(panel, resolver)).toBe(false);
    expect(resolver).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// The switch governs the OS alone
// ---------------------------------------------------------------------------

describe("the enable switch governs the posting alone (NTF-FR-29)", () => {
  // NTF-FR-29, NTF-FR-11, NTF-FR-13.
  it("marks the tab with notifications off and permission never requested", async () => {
    openStrip([editorTab("a.md"), editorTab("b.md")], "art:a.md");
    setNotificationsEnabled(false);
    setNotificationPermissionGranted(false);

    await raiseNotification(raise("k1", fileAddress("b.md")));

    expect(indicatedAddresses()).toEqual([fileAddress("b.md")]);
    expect(posts()).toEqual([]);
    // No prompt appeared because a background tab needed attention.
    expect(permissionRequests()).toEqual([]);
  });

  it("marks the tab when the platform refuses to deliver at all", async () => {
    openStrip([editorTab("a.md"), editorTab("b.md")], "art:a.md");
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "post_notification") {
        return Promise.reject(new Error("unsupported"));
      }
      return Promise.resolve(undefined);
    });

    await expect(
      raiseNotification(raise("k1", fileAddress("b.md"))),
    ).resolves.toBe(false);

    expect(indicatedAddresses()).toEqual([fileAddress("b.md")]);
  });

  // NTF-FR-37: an indication changes nothing else — a resolver that throws must
  // not cost the author the notification they would otherwise have received.
  it("still posts when the tab resolver throws", async () => {
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
      resolveTab: () => {
        throw new Error("strip is mid-teardown");
      },
    });

    await expect(
      raiseNotification(raise("k1", fileAddress("b.md"))),
    ).resolves.toBe(true);
    expect(posts()).toHaveLength(1);
    expect(indicatedAddresses()).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// Aggregation, superseding, clearing
// ---------------------------------------------------------------------------

describe("aggregation and per-key clearing (NTF-FR-31 / NTF-FR-32)", () => {
  // NTF-FR-26, NTF-FR-32.
  it("marks two tabs independently and clears one without the other", async () => {
    openStrip(
      [editorTab("a.md"), editorTab("b.md"), editorTab("c.md")],
      "art:a.md",
    );

    await raiseNotification(raise("k1", fileAddress("b.md")));
    await raiseNotification(raise("k2", fileAddress("c.md")));
    expect(new Set(indicatedAddresses())).toEqual(
      new Set([fileAddress("b.md"), fileAddress("c.md")]),
    );

    notifyArrived(fileAddress("b.md"));
    expect(indicatedAddresses()).toEqual([fileAddress("c.md")]);
  });

  // NTF-FR-31, NTF-FR-32, NTF-FR-14: two keys, one tab, one emphasis — and clearing is all-or-nothing
  // only because reaching the tab reaches both.
  it("holds one address for two keys and clears both on arrival", async () => {
    openStrip([editorTab("a.md"), editorTab("b.md")], "art:a.md");

    await raiseNotification(raise("k1", fileAddress("b.md")));
    await raiseNotification(raise("k2", fileAddress("b.md")));
    // One entry, not two: the tab renders one emphasis (NTF-FR-31).
    expect(indicatedAddresses()).toEqual([fileAddress("b.md")]);

    notifyArrived(fileAddress("b.md"));
    expect(indicatedAddresses()).toEqual([]);
  });

  /**
   * The one path where clearing a key could wrongly clear the tab: two keys on
   * one address, and one of them moves away.
   *
   * `b.md` must keep its emphasis on `k2`'s account while `k1` goes on to mark
   * `c.md` — the whole reason the state is per-key behind one emphasis rather
   * than a set of marked tabs.
   */
  it("keeps a tab marked when one of two keys moves off it", () => {
    markIndication("k1", fileAddress("b.md"));
    markIndication("k2", fileAddress("b.md"));
    expect(indicatedAddresses()).toEqual([fileAddress("b.md")]);

    markIndication("k1", fileAddress("c.md"));

    expect(new Set(indicatedAddresses())).toEqual(
      new Set([fileAddress("b.md"), fileAddress("c.md")]),
    );

    // And `b.md` goes only once the key still holding it does.
    clearIndicationsForAddress(fileAddress("b.md"));
    expect(indicatedAddresses()).toEqual([fileAddress("c.md")]);
  });

  it("keeps a tab marked when reconciliation spares it", () => {
    markIndication("k1", fileAddress("b.md"));
    markIndication("k2", fileAddress("b.md"));

    // `c.md` never existed; `b.md` does. Only the first is reaped, and the tab
    // that survives keeps both of its keys.
    markIndication("k3", fileAddress("c.md"));
    retainIndications((address) => address === fileAddress("b.md"));
    expect(indicatedAddresses()).toEqual([fileAddress("b.md")]);

    clearIndicationsForAddress(fileAddress("b.md"));
    expect(indicatedAddresses()).toEqual([]);
  });

  // NTF-FR-31, NTF-FR-32, NTF-FR-14, first clause: a notification dismissed in the notification
  // centre reaches this store not at all, so nothing clears.
  it("clears nothing on a withdrawal that is not an arrival", async () => {
    openStrip([editorTab("a.md"), editorTab("b.md")], "art:a.md");
    await raiseNotification(raise("k1", fileAddress("b.md")));

    // Reaching a *different* target is the only other thing the shell reports.
    notifyArrived(fileAddress("a.md"));

    expect(indicatedAddresses()).toEqual([fileAddress("b.md")]);
  });

  // NTF-FR-31, NTF-FR-35: a key marks at most one tab at a time.
  it("moves an indication when a key's address moves", async () => {
    openStrip(
      [editorTab("a.md"), editorTab("b.md"), editorTab("c.md")],
      "art:a.md",
    );

    await raiseNotification(raise("run:abc", fileAddress("b.md")));
    await raiseNotification(raise("run:abc", fileAddress("c.md")));

    expect(indicatedAddresses()).toEqual([fileAddress("c.md")]);
  });

  it("replaces rather than stacks for a repeated key", async () => {
    openStrip([editorTab("a.md"), draftTab("d1")], "art:a.md");

    await raiseNotification(raise("draft-proposal:d1", draftAddress("d1")));
    await raiseNotification(raise("draft-proposal:d1", draftAddress("d1")));
    await raiseNotification(raise("draft-proposal:d1", draftAddress("d1")));

    expect(indicatedAddresses()).toEqual([draftAddress("d1")]);
  });
});

// ---------------------------------------------------------------------------
// The pulse
// ---------------------------------------------------------------------------

describe("the arrival pulse is bounded and rate-limited (NTF-FR-35)", () => {
  it("pulses once on arrival and settles static", async () => {
    vi.useFakeTimers();
    const seen: number[] = [];
    const stop = subscribe(() => seen.push(1));

    markIndication("k1", fileAddress("b.md"));
    // Marked and pulsing.
    expect(indicatedAddresses()).toEqual([fileAddress("b.md")]);

    await vi.advanceTimersByTimeAsync(PULSE_MS + 10);
    // The indication stands; only the pulse ended — which is a second
    // notification to the strip, and the last one. Nothing is animating now.
    expect(indicatedAddresses()).toEqual([fileAddress("b.md")]);
    // Exactly two: the mark, and the pulse settling. Not "at least", which
    // would pass for an implementation that woke the strip on a timer.
    expect(seen.length).toBe(2);

    await vi.advanceTimersByTimeAsync(60_000);
    const settled = seen.length;
    await vi.advanceTimersByTimeAsync(60_000);
    expect(seen.length).toBe(settled);

    stop();
  });

  // NTF-FR-34, NTF-FR-35, NTF-FR-37: a burst against one tab produces one pulse, not one per raise.
  it("does not restart the pulse for a burst against one tab", async () => {
    vi.useFakeTimers();
    let notifications = 0;
    const stop = subscribe(() => {
      notifications += 1;
    });

    markIndication("k1", fileAddress("b.md"));
    const afterFirst = notifications;
    markIndication("k2", fileAddress("b.md"));
    markIndication("k3", fileAddress("b.md"));
    markIndication("k1", fileAddress("b.md"));
    // Nothing the strip renders changed — one address, still pulsing — so it
    // was not woken again.
    expect(notifications).toBe(afterFirst);

    // And the pulse ends when the *first* one was due, not extended by the
    // three that followed it.
    await vi.advanceTimersByTimeAsync(PULSE_MS + 10);
    const afterSettle = notifications;
    await vi.advanceTimersByTimeAsync(PULSE_MS * 4);
    expect(notifications).toBe(afterSettle);

    stop();
  });

  /**
   * NTF-FR-35, as the strip actually reads it: `pulsing` is the set the
   * emphasis's two states are chosen from, so it is what has to be asserted —
   * a subscriber count says only that *something* changed.
   */
  it("reports the address as pulsing, then not, and the indication throughout", async () => {
    vi.useFakeTimers();

    markIndication("k1", fileAddress("b.md"));
    expect(pulsingAddresses()).toEqual([fileAddress("b.md")]);

    await vi.advanceTimersByTimeAsync(PULSE_MS + 10);

    expect(pulsingAddresses()).toEqual([]);
    // Settled, not cleared: the emphasis stands until it is reached.
    expect(indicatedAddresses()).toEqual([fileAddress("b.md")]);
  });

  it("stops reporting a cleared address as pulsing", async () => {
    vi.useFakeTimers();
    markIndication("k1", fileAddress("b.md"));
    expect(pulsingAddresses()).toEqual([fileAddress("b.md")]);

    clearIndicationsForAddress(fileAddress("b.md"));

    expect(pulsingAddresses()).toEqual([]);
    // And the timer went with it rather than firing into an empty store: a
    // later mark of the same address gets a pulse of its own, full length.
    markIndication("k2", fileAddress("b.md"));
    expect(pulsingAddresses()).toEqual([fileAddress("b.md")]);
  });

  /**
   * A key that moves off an address no other key holds takes that address's
   * pulse with it — otherwise the leftover timer settles a *later* indication
   * early, and a tab just marked stops pulsing before the author looks up.
   */
  it("does not leave a stale pulse behind when a key moves away", async () => {
    vi.useFakeTimers();
    markIndication("k1", fileAddress("b.md"));
    await vi.advanceTimersByTimeAsync(PULSE_MS / 2);

    markIndication("k1", fileAddress("c.md"));
    expect(indicatedAddresses()).toEqual([fileAddress("c.md")]);

    // `b.md` is marked afresh, by a different key, and must pulse for its own
    // full duration rather than for what was left of the first one.
    markIndication("k2", fileAddress("b.md"));
    expect(pulsingAddresses()).toContain(fileAddress("b.md"));

    await vi.advanceTimersByTimeAsync(PULSE_MS / 2 + 10);
    expect(pulsingAddresses()).toContain(fileAddress("b.md"));

    await vi.advanceTimersByTimeAsync(PULSE_MS);
    expect(pulsingAddresses()).toEqual([]);
  });

  /**
   * NTF-FR-35 pulses on being marked and on a superseding raise — a raise
   * carrying a key that is already indicating. A raise from a *different* key
   * against a tab already marked is neither, and must not re-pulse: NTF-FR-31
   * says a second thing happening in a marked tab makes it no more insistent.
   */
  it("pulses for a superseding raise but not for a second key", async () => {
    vi.useFakeTimers();

    markIndication("k1", fileAddress("b.md"));
    await vi.advanceTimersByTimeAsync(PULSE_MS + 10);
    expect(pulsingAddresses()).toEqual([]);

    // Same key again, once settled: superseding, so it draws the eye again.
    markIndication("k1", fileAddress("b.md"));
    expect(pulsingAddresses()).toEqual([fileAddress("b.md")]);
    await vi.advanceTimersByTimeAsync(PULSE_MS + 10);
    expect(pulsingAddresses()).toEqual([]);

    // A different key on the same, still-marked tab: no new pulse.
    markIndication("k2", fileAddress("b.md"));
    expect(pulsingAddresses()).toEqual([]);
    expect(indicatedAddresses()).toEqual([fileAddress("b.md")]);
  });

  // NTF-FR-35, NTF-FR-34, NTF-FR-36: reduced motion means static from the moment it appears.
  it("never pulses where the platform asks for reduced motion", async () => {
    vi.useFakeTimers();
    const original = window.matchMedia;
    window.matchMedia = ((query: string) => ({
      matches: query.includes("prefers-reduced-motion"),
      media: query,
      addEventListener: () => {},
      removeEventListener: () => {},
    })) as unknown as typeof window.matchMedia;

    try {
      let notifications = 0;
      const stop = subscribe(() => {
        notifications += 1;
      });
      markIndication("k1", fileAddress("b.md"));
      const afterMark = notifications;

      // Never pulsing, not even for an instant — the emphasis is static from
      // the moment it appears.
      expect(pulsingAddresses()).toEqual([]);

      // And no timer was ever scheduled, so nothing settles later either.
      await vi.advanceTimersByTimeAsync(PULSE_MS * 4);
      expect(notifications).toBe(afterMark);
      // The indication itself stands, undiminished by not having pulsed.
      expect(indicatedAddresses()).toEqual([fileAddress("b.md")]);

      stop();
    } finally {
      // In a `finally`, or a failing assertion above leaks reduced motion into
      // every test that runs after it in this file.
      window.matchMedia = original;
    }
  });
});

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

describe("indications do not outlive their tabs (NTF-FR-15 / NTF-FR-33)", () => {
  // NTF-FR-33, NTF-FR-15, NTF-FR-30.
  it("drops an indication whose tab has closed and does not restore it", () => {
    markIndication("k1", fileAddress("b.md"));
    markIndication("k2", fileAddress("c.md"));

    // The strip now holds `c.md` alone: `b.md`'s tab closed.
    const open = [editorTab("c.md")];
    const stillOpen = (address: string) => {
      const parsed = parseAddress(address);
      return (
        !!parsed &&
        open.some((t) => sameTarget(targetForTab(t), parsed.target))
      );
    };
    retainIndications(stillOpen);
    expect(indicatedAddresses()).toEqual([fileAddress("c.md")]);

    // Reopening the target opens an unmarked tab: the indication is gone, and
    // reconciliation does not bring back what it dropped.
    open.push(editorTab("b.md"));
    retainIndications(stillOpen);
    expect(indicatedAddresses()).toEqual([fileAddress("c.md")]);
  });

  it("clears every indication in a root the application stops reading", async () => {
    markIndication("k1", fileAddress("b.md"));
    markIndication("k2", fileAddress("c.md"));
    markIndication("k3", fileAddress("d.md", "/dev/acme-wt2"));

    withdrawForRoot(PROJECT, WORKTREE);

    expect(indicatedAddresses()).toEqual([
      fileAddress("d.md", "/dev/acme-wt2"),
    ]);
  });

  /**
   * NTF-FR-33, second sentence: closing a tab withdraws nothing from the
   * notification centre — the target is still reachable and the posted
   * notification is still the route to it.
   */
  it("takes the indication when a tab closes and leaves the notification standing", async () => {
    openStrip([editorTab("a.md"), editorTab("b.md")], "art:a.md");
    await raiseNotification(raise("k1", fileAddress("b.md")));
    expect(indicatedAddresses()).toEqual([fileAddress("b.md")]);
    expect(postedAddresses()).toEqual([fileAddress("b.md")]);

    // `b.md`'s tab closes: nothing in the strip is a view onto it any more.
    retainIndications(() => false);

    expect(indicatedAddresses()).toEqual([]);
    expect(postedAddresses()).toEqual([fileAddress("b.md")]);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "withdraw_notification"),
    ).toEqual([]);
  });

  /**
   * NTF-FR-30: the store reaches no backend operation and writes no preference,
   * so there is nothing for a relaunch to read back.
   */
  it("touches no backend operation and carries nothing across a relaunch", () => {
    markIndication("k1", fileAddress("b.md"));
    markIndication("k2", fileAddress("c.md"));
    clearIndicationsForAddress(fileAddress("b.md"));

    // Not "no save_app_preferences" — no call of any kind.
    expect(invokeMock.mock.calls).toEqual([]);

    // The store is module state with no persistence behind it, so a fresh
    // module is a fresh session.
    resetTabIndications();
    expect(indicatedAddresses()).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// The tab -> target mapping
// ---------------------------------------------------------------------------

describe("the Runs toggle carries a run's attention (NTF-FR-39 / SNV-FR-70)", () => {
  const runAddress = (runId: string) =>
    mintAddress(PROJECT, WORKTREE, { kind: "run", runId });

  it("marks whenever the author is not on that run, and clears only on reaching it", async () => {
    // NTF-FR-08, NTF-FR-39, NTF-FR-26, NTF-FR-28, NTF-FR-31, NTF-FR-14, GRU-FR-BLSS / NTF-FR-07, NTF-FR-24: the toggle renders one emphasis however many runs
    // are indicating it, and reaching one leaves the other's mark standing.
    configureNotifications({
      snapshot: () => ({
        focused: true,
        projectKey: PROJECT,
        worktree: WORKTREE,
        activeTarget: null,
        visiblePanelSurface: null,
        // Runs is showing, on a *different* run — which is the case that most
        // easily reads as "already looking at it" and is not.
        visibleBottomSurface: "runs",
        visibleRun: "graduation-flow",
        // SWN-FR-05 / NTF-FR-08: no settings child window is open.
        openSettingsWindow: null,
      }),
      resolveTab: () => null,
    });

    await raiseNotification({
      key: "run:artifact-window",
      title: "Graduation · Waiting on you",
      body: "It has a question.",
      address: runAddress("artifact-window"),
    });
    await raiseNotification({
      key: "run:notes-cleanup",
      title: "Graduation · Ready to review",
      body: "It finished.",
      address: runAddress("notes-cleanup"),
    });

    const snapshot = () => ({
      addresses: new Set(indicatedAddresses()),
      pulsing: new Set(pulsingAddresses()),
    });
    expect(hasRunIndication(snapshot())).toBe(true);
    // NTF-FR-28: and no tab is marked for either of them.
    expect(
      indicatedAddresses().every(
        (address) => parseAddress(address)?.target.kind === "run",
      ),
    ).toBe(true);

    // Reaching one run clears that run's mark and no other's; the toggle still
    // carries its one emphasis while any run stands.
    notifyArrived(runAddress("artifact-window"));
    expect(indicatedAddresses()).toEqual([runAddress("notes-cleanup")]);
    expect(hasRunIndication(snapshot())).toBe(true);

    notifyArrived(runAddress("notes-cleanup"));
    expect(indicatedAddresses()).toEqual([]);
    expect(hasRunIndication(snapshot())).toBe(false);
  });

  it("does not mark while Runs is showing that very run", async () => {
    // NTF-FR-39: the run's equivalent of the active tab.
    configureNotifications({
      snapshot: () => ({
        focused: true,
        projectKey: PROJECT,
        worktree: WORKTREE,
        activeTarget: null,
        visiblePanelSurface: null,
        visibleBottomSurface: "runs",
        visibleRun: "artifact-window",
        // SWN-FR-05 / NTF-FR-08: no settings child window is open.
        openSettingsWindow: null,
      }),
      resolveTab: () => null,
    });
    await raiseNotification({
      key: "run:artifact-window",
      title: "t",
      body: "b",
      address: runAddress("artifact-window"),
    });
    expect(indicatedAddresses()).toEqual([]);
  });
});

describe("what a tab is a view onto (NTF-FR-27)", () => {
  it("maps the addressable tab kinds and refuses the rest", () => {
    expect(targetForTab(dashboardTab)).toEqual({ kind: "dashboard" });
    expect(targetForTab(editorTab("a.md"))).toEqual({
      kind: "file",
      path: "a.md",
    });
    expect(
      targetForTab({ id: "flow:f", label: "f", kind: "flow", artifactId: "f.flow" }),
    ).toEqual({ kind: "file", path: "f.flow" });
    expect(targetForTab(draftTab("d1"))).toEqual({
      kind: "draft",
      draftId: "d1",
    });

    // NTF-FR-03: none of these is addressable, so none is ever marked.
    expect(targetForTab(diffTab("a.md"))).toBeNull();
    expect(targetForTab({ id: "search:1", label: "q", kind: "search" })).toBeNull();
    expect(
      targetForTab({ id: "conv:1", label: "c", kind: "conversation" }),
    ).toBeNull();
    // A Dashboard canned row carries no artifact id, so there is nothing to
    // address (SNV-FR-67's reasoning, applied here).
    expect(targetForTab({ id: "art:x", label: "x", kind: "editor" })).toBeNull();
    // SWN-FR-01 / SWN-FR-18: no tab is a view onto a settings window, so a tab
    // that happens to carry the old settings id names nothing.
    expect(
      targetForTab({ id: "global-settings", label: "Global settings" }),
    ).toBeNull();
    expect(
      targetForTab({ id: "project-settings", label: "Project settings" }),
    ).toBeNull();
  });

  it("compares targets by what they name", () => {
    expect(
      sameTarget({ kind: "file", path: "a.md" }, { kind: "file", path: "a.md" }),
    ).toBe(true);
    expect(
      sameTarget({ kind: "file", path: "a.md" }, { kind: "file", path: "b.md" }),
    ).toBe(false);
    expect(sameTarget({ kind: "dashboard" }, { kind: "dashboard" })).toBe(true);
    expect(
      sameTarget({ kind: "draft", draftId: "d" }, { kind: "file", path: "d" }),
    ).toBe(false);
    expect(sameTarget(null, { kind: "dashboard" })).toBe(false);
    expect(
      sameTarget(
        { kind: "settings", which: "global" },
        { kind: "settings", which: "project" },
      ),
    ).toBe(false);
  });
});

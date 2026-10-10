/**
 * The notification facility (`NTF-notifications.md`).
 *
 * One API every surface asks for the author's attention through (NTF-FR-01), and
 * the policy that decides whether a raise becomes a toast, an OS notification,
 * or nothing (NTF-FR-08 through NTF-FR-11, NTF-FR-WMBD). There is no second route to the notification
 * centre: `api.postNotification` is reached from here and nowhere else, which is
 * what makes the policy structural rather than a convention each caller
 * remembers.
 *
 * The decision itself is [`decideDelivery`] — a pure function of the raise and a
 * snapshot of the window, so every branch of the policy is testable without a
 * window, a backend, or a notification centre.
 */
import * as api from "../api";
import { logDebug, logInfo, logWarn } from "../logging";
import {
  mintAddress,
  parseAddress,
  type NotificationAddress,
  type NotificationTarget,
} from "./notificationAddress";
import {
  clearToasts,
  removeToastsForAddress,
  removeToastsForKey,
  resetToasts,
  showToast,
  type NotificationLevel,
} from "./toasts";
import {
  clearIndicationsForAddress,
  clearIndicationsForKey,
  clearIndicationsForRoot,
  markIndication,
} from "./tabIndications";

/**
 * What a surface hands the facility (NTF-FR-01).
 *
 * `key` is the identity of the *thing* being raised about, not of the raise: a
 * surface reporting repeatedly about one run passes one key, and each post
 * replaces the last rather than stacking (NTF-FR-07).
 */
export interface Raise {
  key: string;
  /**
   * NTF-FR-KTRQ: set by the raising surface. The facility never derives it, and
   * the OS notification does not carry it.
   */
  level: NotificationLevel;
  title: string;
  body: string;
  /** Where the author lands on activation — mint it with `mintAddress`. */
  address: string;
}

/**
 * What the facility knows about the window when it decides (NTF-FR-08).
 *
 * Deliberately a plain snapshot rather than a live reference: the decision is a
 * pure function of it, so a test states the situation instead of constructing a
 * shell.
 */
export interface WindowSnapshot {
  /**
   * NTF-FR-08: whether **any window of the application** holds OS focus — the
   * main window or the settings child window open over it (SWN-FR-01). Not the
   * main window's own focus alone: a settings window taking focus does not put
   * the author anywhere else.
   */
  focused: boolean;
  /**
   * NTF-FR-WMBD: whether the **main window** holds OS focus. A toast shows only
   * then. When only a settings window holds focus, `focused` is true and this is
   * false, so the raise goes to the operating system.
   */
  mainFocused: boolean;
  /** The open project's key, or null while the Project picker is showing. */
  projectKey: string | null;
  /** The active worktree's path, or null with no project open. */
  worktree: string | null;
  /** What the main viewport's active tab is showing, if it is addressable. */
  activeTarget: NotificationTarget | null;
  /**
   * The vertical panel's active surface, when the panel is actually visible.
   * Null when it is hidden — a surface the author cannot see has told them
   * nothing.
   */
  visiblePanelSurface: string | null;
  /** The same for the bottom panel. */
  visibleBottomSurface: string | null;
  /**
   * NTF-FR-08 / SWN-FR-05: which settings child window is open, or null when
   * none is.
   *
   * A settings surface is a window rather than a tab (SWN-FR-01), so "is the
   * author already looking at it" cannot be answered from the tab strip. At
   * most one is ever open, which is why this is one value rather than a set.
   */
  openSettingsWindow: string | null;
  /**
   * NTF-FR-08 / NTF-FR-39: the graduation run the author is looking at, when
   * the Runs surface is the visible active bottom surface and its graduation
   * section is showing one. Null otherwise.
   *
   * A run address is held to the same "already looking at it" test one step
   * finer than a panel address is, because a project holds a queue of runs and
   * a queue the author is reading is not the run that has stopped.
   */
  visibleRun: string | null;
}

/** Everything that can stop a raise from becoming a notification. */
export type SuppressionReason =
  | "disabled"
  | "permission"
  | "already-looking"
  | "unparseable-address";

/** NTF-FR-WMBD: where a raise goes. Never both. */
export type DeliveryDecision =
  | { channel: "toast" }
  | { channel: "os" }
  | { channel: "none"; reason: SuppressionReason };

/**
 * Whether the author is already looking at what `address` names (NTF-FR-08).
 *
 * "Looking at" is deliberately strict about panels: a panel surface counts only
 * while its panel is *visible*, because a selected-but-hidden surface has shown
 * the author nothing. A file or draft counts only when it is the **active** tab
 * — one open behind three others is no more visible than one not open at all.
 */
export function isAlreadyVisible(
  address: NotificationAddress,
  snapshot: WindowSnapshot,
): boolean {
  if (
    snapshot.projectKey !== address.projectKey ||
    snapshot.worktree !== address.worktree
  ) {
    return false;
  }
  const { target } = address;
  switch (target.kind) {
    case "panel":
      return snapshot.visiblePanelSurface === target.surface;
    case "bottom":
      return snapshot.visibleBottomSurface === target.surface;
    // NTF-FR-08: suppressed only where Runs is that moment's visible active
    // bottom surface **with that run selected**. Posted where the panel is
    // hidden, where another bottom surface is active, and where Runs is active
    // on a different run.
    case "run":
      return (
        snapshot.visibleBottomSurface === "runs" &&
        snapshot.visibleRun === target.runId
      );
    // NTF-FR-08 / SWN-FR-05: suppressed where the address names the settings
    // window that is at that moment open — the author has already been told by
    // the thing itself. A window rather than a tab, so this is answered from
    // the window snapshot rather than from the active tab.
    case "settings":
      return snapshot.openSettingsWindow === target.which;
    default:
      break;
  }
  const active = snapshot.activeTarget;
  if (!active || active.kind !== target.kind) return false;
  switch (target.kind) {
    case "dashboard":
      return true;
    case "file":
      return active.kind === "file" && active.path === target.path;
    case "draft":
      return active.kind === "draft" && active.draftId === target.draftId;
    default:
      return false;
  }
}

/**
 * NTF-FR-08 through NTF-FR-11, NTF-FR-WMBD, NTF-FR-PCVX: the whole delivery
 * policy, as one pure function.
 *
 * Order matters and is the contract's. Suppression comes first and governs both
 * channels (NTF-FR-08). Then the main window's focus picks the channel. Only the
 * OS channel is gated by the switch and the permission (NTF-FR-11), so a toast
 * shows with notifications switched off.
 */
export function decideDelivery(
  raise: Raise,
  snapshot: WindowSnapshot,
  options: { enabled: boolean; permissionGranted: boolean },
): DeliveryDecision {
  const address = parseAddress(raise.address);
  // A raise nobody could ever be routed from is not worth interrupting for.
  if (!address) return { channel: "none", reason: "unparseable-address" };

  // NTF-FR-08: an unfocused application is never suppressed. NTF-FR-09: nor is a
  // raise made while no project is open, the picker never being where a target
  // lives — which falls out of `isAlreadyVisible`, since a null project key
  // matches no address.
  if (snapshot.focused && isAlreadyVisible(address, snapshot)) {
    return { channel: "none", reason: "already-looking" };
  }

  if (snapshot.mainFocused) return { channel: "toast" };

  if (!options.enabled) return { channel: "none", reason: "disabled" };
  if (!options.permissionGranted) {
    return { channel: "none", reason: "permission" };
  }
  return { channel: "os" };
}

/**
 * NTF-FR-26 through NTF-FR-29: whether this raise marks a tab.
 *
 * Deliberately independent of [`decideDelivery`] rather than a branch of it. The two
 * surfaces answer the same question for authors in two different places, and the
 * gates that stop one do not stop the other: the enable switch and the operating
 * system's permission withhold the **posting** alone (NTF-FR-11), so an eligible
 * raise marks its tab with notifications switched off, with permission refused,
 * and on a platform with no notification centre to reach at all (NTF-FR-29).
 *
 * What it does take is an open tab that is not the active one. A panel or
 * bottom-panel address marks nothing and is reached through its notification
 * alone; an address with no tab open marks nothing, and nothing is opened to
 * carry a mark (NTF-FR-28). The already-viewing suppression of NTF-FR-08 needs
 * no special case here and gets none — it means the address names the active tab
 * or a visible panel surface, and each of those fails a test below on its own.
 */
export function decideIndicate(
  address: NotificationAddress,
  resolveTab: (address: NotificationAddress) => { active: boolean } | null,
  snapshot?: WindowSnapshot,
): boolean {
  const { kind } = address.target;
  if (kind === "panel" || kind === "bottom") return false;
  // NTF-FR-27 / NTF-FR-28: a `settings` address marks nothing anywhere and is
  // reached through its notification alone, a settings window being a window
  // rather than a tab (SWN-FR-18). Stated here rather than left to fall out of
  // "no tab matches": the tab lookup would answer the same today, and would
  // stop answering it the moment anything named a tab after a settings window.
  if (kind === "settings") return false;
  // NTF-FR-28 / NTF-FR-39: a `run` address marks no tab — a run is work the
  // author watches in a panel rather than a thing a tab is a view onto. It
  // marks the activity bar's Runs toggle instead, on the same terms every other
  // indication is held under: whenever the author is not already looking at
  // that run's surface, and whether or not the window holds focus.
  if (kind === "run") {
    if (!snapshot) return true;
    return !isAlreadyVisible(address, snapshot);
  }
  const tab = resolveTab(address);
  if (!tab) return false;
  // NTF-FR-29: a tab the author is on is one they can already see.
  return !tab.active;
}

// ---------------------------------------------------------------------------
// The live facility
// ---------------------------------------------------------------------------

/** What the facility needs to know at the moment of a raise. */
export interface FacilityContext {
  snapshot: () => WindowSnapshot;
  /**
   * NTF-FR-27: the open tab this address's own routing would activate, and
   * whether it is the active one — or null when no tab is open on it.
   *
   * Supplied by the shell rather than computed here, because the strip is what
   * knows which tabs are open. It reads state the shell already holds, so
   * deciding an indication costs no backend round-trip.
   */
  resolveTab: (address: NotificationAddress) => { active: boolean } | null;
}

let context: FacilityContext | null = null;

/**
 * NTF-FR-QGSV: the subtitle every posted notification carries — the display
 * name of the project the address names, which is the last segment of its key.
 * The backend names an open project by the same rule, so a notification about
 * any project, open or not, reads as the shell names it.
 */
export function projectSubtitle(address: NotificationAddress): string {
  const segments = address.projectKey.split(/[\\/]/).filter((s) => s !== "");
  return segments[segments.length - 1] ?? "";
}

/**
 * NTF-FR-11 / NTF-FR-12: the switch and the platform's disposition.
 *
 * Held **here** rather than in the shell's React state, because both are read at
 * the moment of a raise and written from a settings section several components
 * away. Threading them back up as props would mean the section's change reached
 * the facility only on the next relaunch — which is exactly what NTF-FR-12
 * forbids ("turning it off stops the next raise without a relaunch"), and which
 * would make the rehearsal a no-op for the very author who just granted
 * permission in order to try it.
 *
 * Defaults are the safe direction in each case: notifications are on until the
 * author says otherwise (GSS-FR-32), and permission is *not* assumed until the
 * platform has actually answered.
 */
let enabled = true;
let permissionGranted = false;

/** GLS-FR-25: the Notifications section's switch, and the startup read. */
export function setNotificationsEnabled(next: boolean): void {
  enabled = next;
}

/**
 * Counts the writes to `permissionGranted`. A read applies its answer only
 * while no later read or write started, so a slow read never overwrites
 * newer information.
 */
let permissionWrites = 0;

/** NTD-FR-02: what the platform last told us, from startup or a request. */
export function setNotificationPermissionGranted(next: boolean): void {
  permissionWrites += 1;
  permissionGranted = next;
}

/**
 * NTF-FR-13 / NTF-FR-WUUY: read the platform's disposition into the gate.
 * Reading is not requesting, so this never shows a prompt. A read that fails
 * closes the gate. When reads overlap, the one started last decides.
 */
export async function readNotificationPermission(reason: string): Promise<void> {
  permissionWrites += 1;
  const write = permissionWrites;
  let granted = false;
  try {
    granted = (await api.getNotificationPermission()) === "granted";
  } catch (e) {
    logWarn(["frontend"], "notification permission read failed", {
      reason,
      error: String(e),
    });
  }
  if (write === permissionWrites) permissionGranted = granted;
}

/** Tests and diagnostics: the gate as it currently stands. */
export function notificationGate(): {
  enabled: boolean;
  permissionGranted: boolean;
} {
  return { enabled, permissionGranted };
}

/**
 * Ids of notifications this session posted, by the address they carry, so
 * NTF-FR-14 can withdraw one the moment the author reaches what it addresses by
 * a route of their own.
 *
 * Keyed by address rather than by key: withdrawal is triggered by *arrival* at a
 * target, and the address is what names a target. Two raises about different
 * things that happen to address the same tab both become stale when the author
 * opens it.
 */
const postedByAddress = new Map<string, string>();

/**
 * NTF-FR-38: the notifications this session posted, by the **key** they were
 * raised under, so a retraction withdraws exactly what that key put up.
 *
 * Kept beside `postedByAddress` rather than instead of it, because the two
 * answer different questions. Withdrawal on *arrival* is triggered by a target
 * and the address is what names a target (NTF-FR-14); withdrawal on a
 * **retraction** is triggered by the thing being settled, and the key is what
 * names the thing. Two proposals against one file address the same tab, and
 * deciding one must not withdraw the other's notification.
 */
const postedByKey = new Map<string, string>();

/** Wire the facility to the running shell. Called once, as the shell mounts. */
export function configureNotifications(next: FacilityContext): void {
  context = next;
}

/** Tests only — production configures once for the application's life. */
export function resetNotifications(): void {
  context = null;
  postedByAddress.clear();
  postedByKey.clear();
  resetToasts();
  enabled = true;
  permissionGranted = false;
  // A read still in flight from before the reset must not apply.
  permissionWrites += 1;
}

/**
 * NTF-FR-01 / NTF-FR-02: ask for the author's attention.
 *
 * A request rather than an instruction — the facility decides whether anything
 * is posted, and a raise that becomes nothing is the ordinary case rather than a
 * failure. Never throws and never blocks the caller (NTF-FR-23): the returned
 * promise resolves to whether an OS notification was posted, which callers are free
 * to ignore and which nothing in the UI surfaces.
 */
export async function raiseNotification(raise: Raise): Promise<boolean> {
  if (!context) return false;

  // NTF-FR-26 / NTF-FR-29: the in-app half, decided independently of the
  // posting — and so reached even when the posting is withheld, which is the
  // whole point of the switch governing the operating system alone.
  //
  // In its own try, and *first*, so that NTF-FR-37 holds in the direction that
  // is easy to get wrong: an indication changes nothing else about the
  // application, which includes a resolver that throws not costing the author
  // the notification they would otherwise have been sent.
  try {
    const address = parseAddress(raise.address);
    if (
      address &&
      decideIndicate(address, context.resolveTab, context.snapshot())
    ) {
      markIndication(raise.key, raise.address);
    }
  } catch (e) {
    // A strip that could not answer marks nothing — but says so, because this
    // is the only record of it. "Why was that tab not marked?" is a question
    // asked from the Logs panel, and a handled failure that logs nothing is
    // exactly the one nobody can answer.
    logWarn(["frontend"], "tab indication skipped", {
      key: raise.key,
      error: String(e),
    });
  }

  let decision: DeliveryDecision;
  try {
    decision = decideDelivery(raise, context.snapshot(), {
      enabled,
      permissionGranted,
    });
  } catch {
    // A snapshot that threw must not take down the surface that raised.
    return false;
  }

  if (decision.channel === "none") {
    // NTF-FR-10: suppression is silent to the author. It is not silent to the
    // Logs panel, which is where someone asking "why didn't it notify me?" will
    // look — and the reason is the whole answer to that question.
    logDebug(["frontend"], "notification suppressed", {
      key: raise.key,
      reason: decision.reason,
    });
    return false;
  }

  if (decision.channel === "toast") {
    // NTF-FR-WMBD: the main window holds focus, so the toast is the whole
    // delivery and nothing is posted to the operating system.
    showToast({
      key: raise.key,
      level: raise.level,
      title: raise.title,
      body: raise.body,
      address: raise.address,
    });
    logInfo(["frontend"], "toast raised", {
      key: raise.key,
      level: raise.level,
    });
    return false;
  }

  // NTF-FR-ZSIK: a raise of the OS channel is the newer word on its key, so a
  // toast still showing for that key is stale.
  removeToastsForKey(raise.key);

  try {
    // NTF-FR-QGSV: derived here and nowhere else, so every notification has
    // one shape whatever surface raised it. `decideDelivery` posts nothing for an
    // address that does not parse, so this address parses.
    const parsed = parseAddress(raise.address);
    const subtitle = parsed ? projectSubtitle(parsed) : "";
    const posted = await api.postNotification({
      key: raise.key,
      title: raise.title,
      subtitle,
      body: raise.body,
      payload: raise.address,
    });
    postedByAddress.set(raise.address, posted.id);
    postedByKey.set(raise.key, posted.id);
    // The title, subtitle and body are author-facing text and the address
    // names a path, so none is logged by value (LGC-FR-16).
    logInfo(["frontend"], "notification raised", { key: raise.key });
    return true;
  } catch (e) {
    // NTF-FR-23: a post the backend refuses is surfaced nowhere, because the
    // author asked no question that an error would answer.
    logWarn(["frontend"], "notification post refused", {
      key: raise.key,
      error: String(e),
    });
    return false;
  }
}

/**
 * NTF-FR-14: the author **reached** `address`, so nothing about it outlives the
 * thing having been seen — the posted notification is withdrawn and every
 * indication naming it is cleared, on one trigger rather than two.
 *
 * Called by the shell whenever a tab becomes active, a panel surface is shown,
 * or the window regains focus on a tab that is a view onto this address. The two
 * halves clear together here and separately nowhere: a notification dismissed in
 * the notification centre reaches this function not at all, and rightly, because
 * the author read nothing by dismissing it (NTF-FR-32).
 */
export function notifyArrived(address: string): void {
  clearIndicationsForAddress(address);
  removeToastsForAddress(address);
  const id = postedByAddress.get(address);
  if (!id) return;
  postedByAddress.delete(address);
  for (const [key, held] of [...postedByKey]) {
    if (held === id) postedByKey.delete(key);
  }
  void api.withdrawNotification(id).catch(() => {
    // Withdrawal is best-effort: the notification centre is not something the
    // author can be shown an error about.
  });
}

/**
 * NTF-FR-38: the surface that raised under `key` **retracts** it, because the
 * thing that key was raised about is settled and needs the author's attention no
 * longer.
 *
 * The counterpart of the raise and the only route by which attention is taken
 * back: every posted notification carrying that key is withdrawn and every
 * indication derived from it is cleared, in one act and without any reload,
 * re-read, or reopening of anything.
 *
 * It exists because reaching a target (NTF-FR-14) is not the only way a thing
 * stops being worth the author's attention — a decision they made in another
 * window settles it just as finally as reading it does, and a notification that
 * outlives what it was about sends the author to a surface with nothing left on
 * it (PCR-FR-26).
 *
 * A key that never posted and never marked anything retracts to a **no-op**: the
 * raise may have been suppressed, the author may already have reached it, or the
 * notifications switch may have been off throughout. That is what lets a caller
 * retract unconditionally on a decision rather than first working out what
 * became of what it raised.
 */
export function retractNotification(key: string): void {
  clearIndicationsForKey(key);
  removeToastsForKey(key);
  const id = postedByKey.get(key);
  if (!id) return;
  postedByKey.delete(key);
  for (const [address, held] of [...postedByAddress]) {
    if (held === id) postedByAddress.delete(address);
  }
  void api.withdrawNotification(id).catch(() => {
    // Withdrawal is best-effort: the notification centre is not something the
    // author can be shown an error about.
  });
}

/**
 * NTF-FR-15: a content root the application is no longer reading — a project
 * close, a project switch, or a worktree change — leaves neither a notification
 * nor an indication pointing into it.
 */
export function withdrawForRoot(projectKey: string, worktree: string): void {
  clearIndicationsForRoot(projectKey, worktree);
  // NTF-FR-15: no toast survives a project or worktree change, whatever its
  // address, the unreachable-address toast having none.
  clearToasts();
  for (const [address, id] of [...postedByAddress]) {
    const parsed = parseAddress(address);
    if (!parsed) continue;
    if (parsed.projectKey === projectKey && parsed.worktree === worktree) {
      postedByAddress.delete(address);
      for (const [key, held] of [...postedByKey]) {
        if (held === id) postedByKey.delete(key);
      }
      void api.withdrawNotification(id).catch(() => {});
    }
  }
}

/** An activation consumed the notification, so the shell stops tracking it. */
export function forgetPosted(address: string): void {
  const id = postedByAddress.get(address);
  postedByAddress.delete(address);
  if (id === undefined) return;
  for (const [key, held] of [...postedByKey]) {
    if (held === id) postedByKey.delete(key);
  }
}

/** Tests only: what the facility believes is showing. */
export function postedAddresses(): string[] {
  return [...postedByAddress.keys()];
}

export { mintAddress, parseAddress };
export type { NotificationAddress, NotificationLevel, NotificationTarget };

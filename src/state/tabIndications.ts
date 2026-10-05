/**
 * The in-app half of a raise (`NTF-notifications.md` NTF-FR-26 through
 * NTF-FR-36): the tabs that need the author's attention.
 *
 * Owned **here**, at the notifications layer, rather than inside any tab
 * component (NTF-FR-30). A tab is a view onto a target, and it mounts,
 * unmounts, and shifts along the strip as its neighbours open and close; state
 * held in one would be lost on the first of those and would belong to the wrong
 * target after the second. An indication is therefore held by the raise's
 * **key** together with the **address** it resolved — both stable text — and the
 * strip renders whatever this store says of the target each tab is a view onto.
 *
 * Two consequences worth naming, because they are requirements rather than
 * incidental:
 *
 * - **One emphasis per tab, per-key state behind it** (NTF-FR-31). Several keys
 *   may indicate one address; the strip reads the address, so a second thing
 *   happening in a marked tab makes it no more insistent, while clearing one key
 *   leaves the others — and the tab — standing.
 * - **Nothing here is persisted** (NTF-FR-30). It reaches no backend operation,
 *   forms no part of any notification record, and does not survive a relaunch.
 */
import { useSyncExternalStore } from "react";

import { logDebug } from "../logging";
import { parseAddress } from "./notificationAddress";

/**
 * How long the arrival pulse runs before the emphasis settles static
 * (NTF-FR-35). Long enough to catch the eye across the strip, short enough that
 * a tab left indicated is quiet by the time the author looks up.
 */
export const PULSE_MS = 1200;

/** What the strip needs in order to draw the strip. */
export interface IndicationSnapshot {
  /** Addresses currently indicating — one entry however many keys hold it. */
  addresses: ReadonlySet<string>;
  /** The subset still running their arrival pulse (NTF-FR-35). */
  pulsing: ReadonlySet<string>;
}

const EMPTY: IndicationSnapshot = {
  addresses: new Set(),
  pulsing: new Set(),
};

/** key -> the address that key is indicating. A key marks at most one tab. */
const byKey = new Map<string, string>();
/** Addresses mid-pulse, with the timer that will settle each one. */
const pulseTimers = new Map<string, ReturnType<typeof setTimeout>>();

const subscribers = new Set<() => void>();
let snapshot: IndicationSnapshot = EMPTY;

/**
 * Rebuild the snapshot and wake the strip.
 *
 * `useSyncExternalStore` compares snapshots by identity, so this must produce a
 * new object only when something actually changed — otherwise every raise, of
 * which a busy session makes many, re-renders the strip whether or not it looks
 * any different.
 */
function publish(): void {
  const addresses = new Set(byKey.values());
  const pulsing = new Set(
    [...pulseTimers.keys()].filter((address) => addresses.has(address)),
  );
  if (
    addresses.size === snapshot.addresses.size &&
    pulsing.size === snapshot.pulsing.size &&
    [...addresses].every((a) => snapshot.addresses.has(a)) &&
    [...pulsing].every((a) => snapshot.pulsing.has(a))
  ) {
    return;
  }
  snapshot = { addresses, pulsing };
  for (const notify of subscribers) notify();
}

/**
 * NTF-FR-35: whether the platform has asked us not to animate.
 *
 * Read at mark time rather than cached, because the preference can change while
 * the application runs and the next tab to be marked should honour the answer as
 * it is then. The CSS carries the same guard, so a platform this cannot see
 * still gets a static emphasis.
 */
function prefersReducedMotion(): boolean {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return false;
  }
  try {
    return window.matchMedia("(prefers-reduced-motion: reduce)").matches;
  } catch {
    // A matchMedia that throws on an unsupported query tells us nothing about
    // the author's preference; motion is the documented default.
    return false;
  }
}

function settle(address: string): void {
  const timer = pulseTimers.get(address);
  if (timer === undefined) return;
  clearTimeout(timer);
  pulseTimers.delete(address);
}

/**
 * NTF-FR-26: mark the tab `address` names as needing attention, on behalf of
 * `key`.
 *
 * Replaces whatever that key was indicating rather than adding a second
 * (NTF-FR-31), so a key whose address has moved clears the tab it was marking
 * and marks the new one.
 *
 * The pulse is rate-limited to one per address (NTF-FR-35): a burst of raises
 * against one tab produces one pulse rather than one per raise, and a raise
 * arriving against a tab already indicating does not restart an emphasis the
 * author may be halfway through noticing.
 */
export function markIndication(key: string, address: string): void {
  const previous = byKey.get(key);
  if (previous === address && pulseTimers.has(address)) {
    // Already indicating this exact address and already pulsing for it: there
    // is nothing to change and nothing to restart.
    return;
  }

  const alreadyIndicated = [...byKey].some(
    ([k, a]) => k !== key && a === address,
  );
  byKey.set(key, address);

  // A key that has moved off an address no other key holds takes that address's
  // pulse with it. Without this the timer outlives the indication it belonged
  // to, and a later raise against that same address inherits whatever is left
  // of it — a pulse cut short, for a tab that has only just been marked.
  if (previous !== undefined && previous !== address) {
    const stillHeld = [...byKey.values()].some((a) => a === previous);
    if (!stillHeld) settle(previous);
  }

  if (!pulseTimers.has(address) && !alreadyIndicated && !prefersReducedMotion()) {
    pulseTimers.set(
      address,
      setTimeout(() => {
        pulseTimers.delete(address);
        publish();
      }, PULSE_MS),
    );
  }

  // The address names a path and the title is author-facing text, so neither is
  // logged by value (LGC-FR-16); the key is what a reader correlates on.
  logDebug(["frontend"], "tab indication marked", { key });
  publish();
}

/**
 * NTF-FR-32: the author reached what `address` names, so every key indicating it
 * is cleared — and no others.
 *
 * Keys indicating a *different* address stand, including keys on tabs the author
 * has not been to. Returns how many were cleared, which is what tells the caller
 * whether anything happened worth logging.
 */
export function clearIndicationsForAddress(address: string): number {
  let cleared = 0;
  for (const [key, held] of [...byKey]) {
    if (held !== address) continue;
    byKey.delete(key);
    cleared += 1;
  }
  if (cleared === 0) return 0;
  settle(address);
  logDebug(["frontend"], "tab indication cleared", { cleared });
  publish();
  return cleared;
}

/**
 * NTF-FR-32 / NTF-FR-38: the surface that raised under `key` has **retracted**,
 * so the indication that key was holding is cleared — and no others.
 *
 * Keyed rather than addressed, unlike [`clearIndicationsForAddress`], because a
 * retraction says that *the thing this key was raised about* is settled rather
 * than that the author reached a tab: two proposals against one file address the
 * same tab, and deciding one must not clear the other's mark.
 *
 * A key that is indicating nothing — because its raise was suppressed, because
 * the author already reached it, or because notifications never marked a tab at
 * all — retracts to a no-op (NTF-FR-38).
 */
export function clearIndicationsForKey(key: string): number {
  const address = byKey.get(key);
  if (address === undefined) return 0;
  byKey.delete(key);
  // The pulse belongs to the address rather than to the key, so it is settled
  // only once no other key is still holding that address.
  if (![...byKey.values()].some((held) => held === address)) settle(address);
  logDebug(["frontend"], "tab indication retracted", { key });
  publish();
  return 1;
}

/**
 * NTF-FR-15: a content root the application is no longer reading leaves no
 * indication pointing into it.
 */
export function clearIndicationsForRoot(
  projectKey: string,
  worktree: string,
): void {
  let cleared = 0;
  for (const [key, address] of [...byKey]) {
    const parsed = parseAddress(address);
    if (!parsed) continue;
    if (parsed.projectKey !== projectKey || parsed.worktree !== worktree) {
      continue;
    }
    byKey.delete(key);
    settle(address);
    cleared += 1;
  }
  if (cleared === 0) return;
  logDebug(["frontend"], "tab indications cleared for root", { cleared });
  publish();
}

/**
 * NTF-FR-33: drop every indication whose tab is gone.
 *
 * Reconciliation rather than a hook on each close path, because a tab leaves the
 * strip by more routes than the author closing it — a removed file takes its tab
 * (`TAB-tabs.md` TAB-FR-19), a commit takes its Diff tabs (TAB-FR-22), and a
 * worktree change takes all of them (TAB-FR-14). Asking "is this still open?"
 * once, against the strip as it now stands, covers every one of them and cannot
 * be forgotten by a route added later.
 *
 * That an indication does not come back with the tab is the point: it was the
 * tab's way of saying the author had not been there, and a tab they have just
 * opened is not that.
 */
export function retainIndications(isOpen: (address: string) => boolean): void {
  let dropped = 0;
  for (const [key, address] of [...byKey]) {
    // NTF-FR-39: a `run` indication marks the activity bar's Runs toggle
    // rather than a tab, and that toggle is always present — so it has none of
    // the tab-closing lifecycle this reconciliation is about. Reaping it here
    // would clear it the instant it was set, the strip holding no tab for it to
    // find.
    if (isRunAddress(address)) continue;
    if (isOpen(address)) continue;
    byKey.delete(key);
    settle(address);
    dropped += 1;
  }
  if (dropped === 0) return;
  logDebug(["frontend"], "tab indications dropped with their tabs", { dropped });
  publish();
}

/** Whether `address` names a graduation run rather than a tab. */
function isRunAddress(address: string): boolean {
  return parseAddress(address)?.target.kind === "run";
}

/**
 * NTF-FR-39: whether any run is asking for the author's attention.
 *
 * The activity bar renders **one** emphasis however many runs are indicating
 * it, on the terms NTF-FR-31 sets for a tab — so what the strip needs is this
 * one answer rather than the set behind it.
 */
export function hasRunIndication(snapshot: IndicationSnapshot): boolean {
  for (const address of snapshot.addresses) {
    if (isRunAddress(address)) return true;
  }
  return false;
}

/** NTF-FR-39: whether that one emphasis is still running its arrival pulse. */
export function runIndicationPulsing(snapshot: IndicationSnapshot): boolean {
  for (const address of snapshot.pulsing) {
    if (isRunAddress(address)) return true;
  }
  return false;
}

export function subscribe(notify: () => void): () => void {
  subscribers.add(notify);
  return () => {
    subscribers.delete(notify);
  };
}

function getSnapshot(): IndicationSnapshot {
  return snapshot;
}

/** The live set. The strip reads through this, so one raise re-renders it once. */
export function useTabIndications(): IndicationSnapshot {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/** Tests and diagnostics: which addresses are indicating. */
export function indicatedAddresses(): string[] {
  return [...new Set(byKey.values())];
}

/**
 * Tests and diagnostics: which of them are still running their arrival pulse.
 *
 * Read from the published snapshot rather than from the timer map, so it is the
 * same answer the strip is rendering from — a test asserting on it is asserting
 * on what the author sees rather than on bookkeeping behind it.
 */
export function pulsingAddresses(): string[] {
  return [...snapshot.pulsing];
}

/** Tests only — production holds these for the application's life. */
export function resetTabIndications(): void {
  for (const timer of pulseTimers.values()) clearTimeout(timer);
  pulseTimers.clear();
  byKey.clear();
  subscribers.clear();
  snapshot = EMPTY;
}

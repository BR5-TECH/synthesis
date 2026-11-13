/**
 * The route that opens a change workspace tab
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-DMGL).
 *
 * Registered by the shell rather than passed as props, on the terms of
 * `decompositionRoutes.ts`: the Runs panel, the Drafts panel row menu and the
 * New Artifact tab sit several layers below the tab strip.
 */
import { logDebug } from "../../logging";
import { discussionTargetKey, type ChangeLocation } from "../../types";

export interface OpenChangeOptions {
  /** OCW-FR-IAPK: the change-relative path of the file to select. */
  file?: string | null;
  /** OCW-FR-XUJN: the stream name, where the caller knows it. */
  streamName?: string;
  /**
   * OCW-FR-QSZP: the thread of the change to select in the discussion column.
   * A stream location alone has one.
   */
  thread?: string | null;
}

export interface OpenSpecChangeRoutes {
  open: (location: ChangeLocation, name: string, options?: OpenChangeOptions) => void;
}

let routes: OpenSpecChangeRoutes | null = null;

/** Wire the route to the running shell. Called as the shell mounts. */
export function configureOpenSpecChangeRoutes(next: OpenSpecChangeRoutes | null): void {
  routes = next;
}

/** OCW-FR-DMGL: open the change workspace tab of one change in one location. */
export function openOpenSpecChange(
  location: ChangeLocation,
  name: string,
  options: OpenChangeOptions = {},
): void {
  if (!routes) {
    logDebug(["frontend"], "change workspace route is not configured", {
      locationKind: location.kind,
    });
    return;
  }
  // OCW-FR-QSZP: the tab takes the thread when it mounts or, where it is
  // open, at once.
  if (options.thread && location.kind === "stream") {
    requestChangeDiscussion(
      discussionTargetKey({ kind: "change", streamId: location.streamId, change: name }),
      options.thread,
    );
  }
  routes.open(location, name, options);
}

/**
 * CVP-FR-TFRD: the discussion each change workspace is asked to select, by the
 * change's target key. The route sets it before the tab opens, and the tab
 * takes it when it mounts or, where it is open, at once.
 */
const wanted = new Map<string, string>();
const wantedListeners = new Set<() => void>();
let wantedVersion = 0;

/** CVP-FR-TFRD: ask the change workspace of `key` to select a discussion. */
export function requestChangeDiscussion(key: string, discussionId: string): void {
  wanted.set(key, discussionId);
  wantedVersion += 1;
  wantedListeners.forEach((l) => l());
}

/** The discussion a change workspace is asked to select, taken once. */
export function takeChangeDiscussion(key: string): string | null {
  const id = wanted.get(key) ?? null;
  wanted.delete(key);
  return id;
}

export function subscribeChangeDiscussion(listener: () => void): () => void {
  wantedListeners.add(listener);
  return () => {
    wantedListeners.delete(listener);
  };
}

export function changeDiscussionVersion(): number {
  return wantedVersion;
}

/** Tests only: forget every request. */
export function resetChangeDiscussionRequests(): void {
  wanted.clear();
  wantedVersion = 0;
}

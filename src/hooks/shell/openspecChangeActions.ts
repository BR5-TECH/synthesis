/**
 * The tab work of the change workspace (`TAB-tabs.md` TAB-FR-KOTW, TAB-FR-DRXS;
 * `OCW-openspec-change-workspace.md` OCW-FR-DMGL, OCW-FR-IAPK, OCW-FR-XUJN).
 *
 * A change workspace tab is read-only. It holds no editing session and no
 * savable buffer, so no write is brought forward when it closes. It is
 * identified by its location and its change name.
 *
 * Plain closures rather than a hook: the strip stays owned by `useShellSession`.
 */
import {
  useEffect,
  useRef,
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
} from "react";

import * as api from "../../api";
import { onWorkStreamsChanged } from "../../events";
import { logDebug, logWarn } from "../../logging";
import {
  configureOpenSpecChangeRoutes,
  type OpenChangeOptions,
} from "../../state/openspec/changeRoutes";
import { pruneChangeViews } from "../../state/openspec/changeView";
import type { ChangeLocation, Tab, WorktreeEntry } from "../../types";
import { neverEmpty } from "./tabRecords";
import { noteOwnerDeleted, voiceOwner } from "../../voice/voiceController";

/** TAB-FR-KOTW: the tab id of one change in one location. */
export function changeTabId(location: ChangeLocation, name: string): string {
  const where = location.kind === "stream" ? `stream:${location.streamId}` : "worktree";
  return `openspec:${where}:${name}`;
}

/**
 * OCW-FR-PLDG / WTC-FR-RDVK: where an open in the active worktree reads the
 * change. A worktree that is the working copy of a stream is that stream's
 * location, so the tab carries the stream's discussion. Any other worktree is
 * the active worktree location.
 */
export function worktreeChangeLocation(entry: WorktreeEntry | null): {
  location: ChangeLocation;
  streamName?: string;
} {
  const stream = entry?.stream;
  if (stream) {
    return {
      location: { kind: "stream", streamId: stream.streamId },
      streamName: stream.streamName,
    };
  }
  return { location: { kind: "active_worktree" } };
}

/** OCW-FR-XUJN: the tooltip of a change workspace tab. */
function changeTooltip(name: string, location: ChangeLocation, streamName?: string): string {
  if (location.kind === "active_worktree") return `${name} — active worktree`;
  return `${name} — stream ${streamName ?? location.streamId}`;
}

export interface OpenSpecChangeActionDeps {
  tabsRef: MutableRefObject<Tab[]>;
  setTabs: Dispatch<SetStateAction<Tab[]>>;
  setActiveTab: Dispatch<SetStateAction<string>>;
  activateTab: (id: string) => void;
}

export interface OpenSpecChangeActions {
  openChange: (location: ChangeLocation, name: string, options?: OpenChangeOptions) => void;
  closeChangeTabsForStreams: (liveStreamIds: ReadonlySet<string>) => void;
}

export function createOpenSpecChangeActions(
  deps: OpenSpecChangeActionDeps,
): OpenSpecChangeActions {
  const { tabsRef, setTabs, setActiveTab, activateTab } = deps;

  /** OCW-FR-XUJN: put a stream name on every open tab of that stream. */
  const nameStream = (streamId: string, streamName: string) =>
    setTabs((ts) =>
      ts.map((t) => {
        const target = t.openspecChange;
        if (
          !target ||
          target.location.kind !== "stream" ||
          target.location.streamId !== streamId ||
          target.streamName === streamName
        )
          return t;
        return {
          ...t,
          tooltip: changeTooltip(target.name, target.location, streamName),
          openspecChange: { ...target, streamName },
        };
      }),
    );

  /** OCW-FR-XUJN: read the stream name where the opener did not give it. */
  const resolveStreamName = (streamId: string) => {
    void api
      .listWorkStreams()
      .then((found) => {
        const stream = found.find((s) => s.stream.id === streamId)?.stream;
        if (stream) nameStream(streamId, stream.name);
      })
      .catch((e) => {
        logWarn(["frontend"], "work streams could not be listed to name a change tab", {
          error: e instanceof Error ? e.message : String(e),
        });
      });
  };

  /**
   * OCW-FR-DMGL / TAB-FR-KOTW: open the change, or focus the tab that shows
   * it. Each open increments `openSeq`, so the tab applies the selection of
   * this open (OCW-FR-IAPK). The presence test runs inside the updater, so two
   * requests before a render cannot add a second tab.
   */
  const openChange = (
    location: ChangeLocation,
    name: string,
    options: OpenChangeOptions = {},
  ) => {
    const id = changeTabId(location, name);
    const file = options.file ?? null;
    const knownName =
      options.streamName ??
      tabsRef.current.find((t) => t.id === id)?.openspecChange?.streamName;
    setTabs((ts) => {
      const open = ts.find((t) => t.id === id);
      if (open?.openspecChange) {
        const target = open.openspecChange;
        return ts.map((t) =>
          t.id === id
            ? {
                ...t,
                tooltip: changeTooltip(name, location, knownName),
                openspecChange: {
                  ...target,
                  streamName: knownName ?? target.streamName,
                  file,
                  openSeq: target.openSeq + 1,
                },
              }
            : t,
        );
      }
      return [
        ...ts,
        {
          id,
          label: name,
          tooltip: changeTooltip(name, location, knownName),
          kind: "openspecChange",
          openspecChange: { location, name, streamName: knownName, file, openSeq: 1 },
        },
      ];
    });
    activateTab(id);
    logDebug(["frontend"], "change workspace opened", {
      locationKind: location.kind,
      namesFile: file !== null,
    });
    if (location.kind === "stream" && knownName === undefined) {
      resolveStreamName(location.streamId);
    }
  };

  /**
   * TAB-FR-DRXS: a tab whose stream left the project's streams closes. The
   * close needs no prompt and writes nothing. Focus that lands on a neighbour
   * is a consequence rather than a navigation, so the raw setter is used
   * (SNV-FR-65).
   */
  const closeChangeTabsForStreams = (liveStreamIds: ReadonlySet<string>) => {
    const closing = tabsRef.current.filter((t) => {
      const location = t.openspecChange?.location;
      return location?.kind === "stream" && !liveStreamIds.has(location.streamId);
    });
    if (closing.length === 0) return;
    const closingIds = new Set(closing.map((t) => t.id));
    const remaining = neverEmpty(tabsRef.current.filter((t) => !closingIds.has(t.id)));
    tabsRef.current = remaining;
    setTabs((ts) => neverEmpty(ts.filter((t) => !closingIds.has(t.id))));
    setActiveTab((current) =>
      remaining.some((t) => t.id === current)
        ? current
        : (remaining[remaining.length - 1]?.id ?? current),
    );
    logDebug(["frontend"], "change workspace tabs closed: stream left the project", {
      count: closing.length,
      rule: "TAB-FR-DRXS",
    });
  };

  return { openChange, closeChangeTabsForStreams };
}

/**
 * VOM-FR-NTKT: a stream that left the project's streams deletes the owner of
 * a change discussion, so voice mode on it ends.
 */
export function endVoiceForGoneStream(liveStreamIds: ReadonlySet<string>): void {
  const owner = voiceOwner();
  if (owner?.kind !== "change" || liveStreamIds.has(owner.streamId)) return;
  logDebug(["frontend"], "voice mode ends: the stream of its change left the project", {
    rule: "VOM-FR-NTKT",
  });
  noteOwnerDeleted(owner);
}

/**
 * The shell's half of the change workspace: the route the panels take, the
 * close of TAB-FR-DRXS on `"work streams changed"`, and the pruning of what a
 * closed tab kept.
 */
export function useOpenSpecChangeTabs(
  tabs: Tab[],
  tabsRef: MutableRefObject<Tab[]>,
  actions: OpenSpecChangeActions,
): void {
  const actionsRef = useRef(actions);
  actionsRef.current = actions;

  useEffect(() => {
    configureOpenSpecChangeRoutes({
      open: (location, name, options) => actionsRef.current.openChange(location, name, options),
    });
    return () => configureOpenSpecChangeRoutes(null);
  }, []);

  // TAB-FR-DRXS: subscribed once. Each event lists the streams again, and only
  // when a tab of a stream location is open.
  useEffect(() => {
    let stop: (() => void) | null = null;
    let cancelled = false;
    void onWorkStreamsChanged(() => {
      const anyStreamTab = tabsRef.current.some(
        (t) => t.openspecChange?.location.kind === "stream",
      );
      // VOM-FR-NTKT: voice mode on a change discussion also needs the list.
      const voiceOnChange = voiceOwner()?.kind === "change";
      if (!anyStreamTab && !voiceOnChange) return;
      void api
        .listWorkStreams()
        .then((found) => {
          if (cancelled) return;
          const live = new Set(found.map((s) => s.stream.id));
          endVoiceForGoneStream(live);
          actionsRef.current.closeChangeTabsForStreams(live);
        })
        .catch((e) => {
          logWarn(["frontend"], "work streams could not be listed for change tabs", {
            error: e instanceof Error ? e.message : String(e),
          });
        });
    }).then((fn) => {
      if (cancelled) fn();
      else stop = fn;
    });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [tabsRef]);

  useEffect(() => {
    pruneChangeViews(new Set(tabs.filter((t) => t.openspecChange).map((t) => t.id)));
  }, [tabs]);
}

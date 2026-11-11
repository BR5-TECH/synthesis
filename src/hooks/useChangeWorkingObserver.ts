/**
 * The working-activity observer of OpenSpec changes
 * (`specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-MIEY).
 *
 * It is the editing-activity observer of a draft (`../draftEditingObserver`,
 * DFI-FR-XKRM, DFI-FR-UKFR, DFI-FR-JDWS) with a change in place of a draft:
 * at most one open interval per change, opened by an interaction while the
 * application has focus and a related surface of the change is visible, and
 * settled on a loss of focus, on the last related surface going, or five
 * minutes after the last interaction. Each settled interval goes whole to
 * `"record change working interval"`.
 *
 * The related surfaces of a change are its tab, which holds its discussion
 * column, and a conversation tab whose origin kind is `change_comment` or
 * `change_discussion` for that change. The caller reduces them to the changes
 * they are about, so two surfaces of one change hold one interval.
 */
import { useEffect, useMemo } from "react";

import { recordChangeWorkingInterval } from "../api";
import { DraftEditingObserver } from "../draftEditingObserver";
import { logWarn } from "../logging";
import type { Tab } from "../types";

/** One change on its stream. */
export interface ObservedChange {
  streamId: string;
  change: string;
}

/**
 * OCW-FR-MIEY: the changes a related surface is visible for. The active tab
 * counts where it is a change workspace on a stream, which holds the change's
 * discussion column, or a conversation tab whose owner is a change. No other
 * tab is a surface the author is at.
 */
export function visibleChangesOf(tabs: readonly Tab[], activeTab: string | null): ObservedChange[] {
  const tab = tabs.find((t) => t.id === activeTab);
  if (!tab) return [];
  const target = tab.openspecChange;
  if (tab.kind === "openspecChange" && target?.location.kind === "stream") {
    return [{ streamId: target.location.streamId, change: target.name }];
  }
  if (tab.kind === "conversation" && tab.ownerTarget?.kind === "change") {
    return [{ streamId: tab.ownerTarget.streamId, change: tab.ownerTarget.change }];
  }
  return [];
}

/** The key the observer counts a change by. */
export function changeObserverKey(target: ObservedChange): string {
  return JSON.stringify([target.streamId, target.change]);
}

function fromKey(key: string): ObservedChange | null {
  try {
    const [streamId, change] = JSON.parse(key) as [unknown, unknown];
    return typeof streamId === "string" && typeof change === "string" ? { streamId, change } : null;
  } catch {
    return null;
  }
}

/** OCW-FR-MIEY: report one settled interval. Fire-and-forget, never shown. */
function report(key: string, startedAt: string, endedAt: string): void {
  const target = fromKey(key);
  if (!target) return;
  void recordChangeWorkingInterval(target.streamId, target.change, startedAt, endedAt).catch((e) =>
    logWarn(["frontend"], "a change working interval could not be recorded", {
      streamId: target.streamId,
      reason: String(e).slice(0, 80),
    }),
  );
}

/** Make the observer of changes. Exported so a test can drive its clock. */
export function createChangeWorkingObserver(now?: () => number): DraftEditingObserver {
  return new DraftEditingObserver({ report, now });
}

export function useChangeWorkingObserver(visibleChanges: readonly ObservedChange[]): void {
  const observer = useMemo(() => createChangeWorkingObserver(), []);

  // A stable key, so a render that changed nothing settles nothing.
  const visible = useMemo(
    () => JSON.stringify([...new Set(visibleChanges.map(changeObserverKey))].sort()),
    [visibleChanges],
  );

  useEffect(() => {
    observer.setVisibleDrafts(JSON.parse(visible) as string[]);
  }, [observer, visible]);

  useEffect(() => {
    const onFocus = () => observer.setFocused(true);
    const onBlur = () => observer.setFocused(false);
    // DFI-FR-XKRM: an interaction is the author doing something with the
    // application. Agent processing is not one.
    const onInteract = () => observer.interacted();
    window.addEventListener("focus", onFocus);
    window.addEventListener("blur", onBlur);
    window.addEventListener("pointerdown", onInteract, true);
    window.addEventListener("keydown", onInteract, true);
    window.addEventListener("wheel", onInteract, { capture: true, passive: true });
    return () => {
      window.removeEventListener("focus", onFocus);
      window.removeEventListener("blur", onBlur);
      window.removeEventListener("pointerdown", onInteract, true);
      window.removeEventListener("keydown", onInteract, true);
      window.removeEventListener("wheel", onInteract, true);
      // DFI-FR-UKFR: an interval that never settled is reported not at all.
      observer.dispose();
    };
  }, [observer]);
}

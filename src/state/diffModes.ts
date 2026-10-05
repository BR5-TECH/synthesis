/**
 * The Diff tab's two modes (`DFV-diff-viewer.md` DFV-FR-23 / DFV-FR-24).
 *
 * Both are **one user-global choice**, not a property of a tab: activating a
 * toggle anywhere re-renders every open Diff tab into that mode immediately,
 * and a tab opened afterwards opens in it too. That is why the value lives in a
 * module-level store with subscribers rather than in each tab's `useState` —
 * per-tab state would leave the other tabs on the mode the user just left.
 *
 * Persistence is a **patch**, not a replace. `save_app_preferences` writes the
 * record whole (GSS-FR-20), so sending a bare `{ diffVisualizationMode }` would
 * clear the theme, the full-screen flag, and the search bar's query mode, which
 * share the record and are edited elsewhere entirely (GLS-FR-14).
 */
import { useSyncExternalStore } from "react";
import {
  loadAppPreferences,
  patchAppPreferences,
} from "./appPreferences";
import type { DiffRenderingMode, DiffVisualizationMode } from "../types";

export interface DiffModes {
  visualization: DiffVisualizationMode;
  rendering: DiffRenderingMode;
}

/** DFV-FR-23: what a user who has never chosen a mode starts in. */
export const DEFAULT_DIFF_MODES: DiffModes = {
  visualization: "unified",
  rendering: "source",
};

/**
 * DFV-FR-07 / DFV-FR-08: the visualization group, in toolbar order. `hint` is
 * what the toggle's tooltip says the mode does — three one-word labels cannot
 * carry that on their own.
 */
export const VISUALIZATION_MODES: ReadonlyArray<{
  value: DiffVisualizationMode;
  label: string;
  hint: string;
}> = [
  {
    value: "unified",
    label: "Unified",
    hint: "The changed regions in one column, every row numbered on both sides.",
  },
  {
    value: "side_by_side",
    label: "Side-by-side",
    hint: "The whole file twice — the old revision left, the new right, scrolling together.",
  },
  {
    value: "final",
    label: "Final",
    hint: "The new revision alone, with no change marking.",
  },
];

/** DFV-FR-16: the rendering group, in toolbar order. */
export const RENDERING_MODES: ReadonlyArray<{
  value: DiffRenderingMode;
  label: string;
  hint: string;
}> = [
  {
    value: "source",
    label: "Source",
    hint: "The file's literal text, unrendered.",
  },
  {
    value: "rich",
    label: "Rich",
    hint: "Markdown rendered as its formatted document.",
  },
];

let modes: DiffModes = DEFAULT_DIFF_MODES;
const subscribers = new Set<() => void>();
/** One shared read of the stored record, however many tabs mount at once. */
let hydration: Promise<void> | null = null;
/**
 * Whether the user has activated a toggle since the app started. The toolbar
 * renders before the stored record arrives, so a click can land mid-read; the
 * click is then the newer fact and the arriving record must not overwrite it.
 */
let chosen = false;

function publish(next: DiffModes) {
  // Identity is the change signal `useSyncExternalStore` compares on, so an
  // equal-valued update must not produce a new object or every subscriber
  // re-renders on every hydration.
  if (next.visualization === modes.visualization && next.rendering === modes.rendering) {
    return;
  }
  modes = next;
  for (const notify of subscribers) notify();
}

/** Listen for mode changes. Returns the unsubscribe. */
export function subscribe(notify: () => void): () => void {
  subscribers.add(notify);
  return () => {
    subscribers.delete(notify);
  };
}

function getSnapshot(): DiffModes {
  return modes;
}

/**
 * DFV-FR-23: seed from `"load app preferences"`. Idempotent — the first Diff
 * tab to mount pays the round-trip and the rest read what it published. A read
 * that fails leaves the defaults in place rather than blocking the tab.
 */
export function hydrateDiffModes(): Promise<void> {
  if (!hydration) {
    hydration = loadAppPreferences()
      .then((prefs) => {
        // A toggle activated while this read was in flight has already been
        // written; letting the stored record land on top would revert the
        // user's choice on screen while persisting it to disk.
        if (chosen) return;
        publish({
          visualization:
            prefs?.diffVisualizationMode ?? DEFAULT_DIFF_MODES.visualization,
          rendering: prefs?.diffRenderingMode ?? DEFAULT_DIFF_MODES.rendering,
        });
      })
      .catch(() => {
        // Nothing better to do with it: the toggles show the defaults and the
        // next activation writes a full record anyway.
      });
  }
  return hydration;
}

/**
 * DFV-FR-24: activate a visualization mode everywhere at once.
 *
 * The value is applied optimistically and rolled back if the write fails —
 * the toggles have no error affordance of their own, and leaving them showing a
 * choice that will not survive relaunch is worse than reverting it.
 */
export function setVisualizationMode(next: DiffVisualizationMode): void {
  // DFV-FR-23: the record is written when the user activates a *different*
  // toggle. Re-activating the one already active changes nothing to persist.
  if (next === modes.visualization) return;
  chosen = true;
  const previous = modes;
  publish({ ...modes, visualization: next });
  patchAppPreferences({ diffVisualizationMode: next }).catch(() => {
    publish(previous);
  });
}

/** DFV-FR-24: the same, for the rendering group. */
export function setRenderingMode(next: DiffRenderingMode): void {
  if (next === modes.rendering) return;
  chosen = true;
  const previous = modes;
  publish({ ...modes, rendering: next });
  patchAppPreferences({ diffRenderingMode: next }).catch(() => {
    publish(previous);
  });
}

/** The current pair without subscribing. */
export function peekDiffModes(): DiffModes {
  return modes;
}

/** Drop the store. Tests only — production holds one pair for the app's life. */
export function resetDiffModes(): void {
  modes = DEFAULT_DIFF_MODES;
  hydration = null;
  chosen = false;
  subscribers.clear();
}

/**
 * The live pair. Every Diff tab reads through this, so one activation re-renders
 * all of them (DFV-FR-24).
 */
export function useDiffModes(): DiffModes {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

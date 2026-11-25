/**
 * EDT-FR-MSCV, EDT-FR-NGCW, EDT-FR-DNVJ, EDT-FR-BHJR: the legend state of the
 * diagram overlays — whether the syntax hints of the diagram editor overlay and
 * the details panel of a CALM interactive canvas (`CAD-calm-architecture-
 * diagrams.md` CAD-FR-RLDM) show.
 *
 * One value for each window, so a toggle on one surface applies at once to
 * every surface of the window. It is stored per project in the layout record
 * (`SNV-shell-navigation.md` SNV-FR-08), never in a file of the project.
 *
 * A module store rather than React state, because the surfaces that read it do
 * not share a React parent: the overlays of the window and the Diagram view of
 * an Architecture tab.
 */
import { useSyncExternalStore } from "react";

import { logDebug, logWarn } from "../logging";
import {
  loadLayoutPreferences,
  patchLayoutPreferences,
  reloadLayoutPreferences,
} from "./layoutPreferences";

type Listener = () => void;

let hidden = false;
/** The project whose record holds the value. `null` stores nothing. */
let projectKey: string | null = null;
/**
 * The number of changes the author made. A read that started before a change
 * returns an older value than the one on screen, so it is discarded.
 */
let changes = 0;
/**
 * EDT-FR-BHJR: the last write failed, so the record holds an older value than
 * the one on screen. A read does not apply that older value.
 */
let unsaved = false;
/** EDT-FR-DNVJ: the window listens for its focus once a project opens. */
let focusListening = false;
const listeners = new Set<Listener>();

function emit(): void {
  for (const listener of [...listeners]) listener();
}

function subscribe(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** EDT-FR-MSCV: whether the legend is hidden now. */
export function diagramLegendHidden(): boolean {
  return hidden;
}

/** The legend state, for a component. It renders again at each change. */
export function useDiagramLegendHidden(): boolean {
  return useSyncExternalStore(subscribe, diagramLegendHidden);
}

/**
 * Read the stored value for `key`, unless the author changed it meanwhile.
 * `fresh` reads the backend again and does not use the cache of this window.
 *
 * A record with no value changes nothing: the project never stored one, or
 * the read failed (`loadLayoutPreferences` answers `{}` then). Either way the
 * legend keeps the state on screen.
 */
async function read(key: string, fresh: boolean): Promise<void> {
  const before = changes;
  const prefs = await (fresh ? reloadLayoutPreferences(key) : loadLayoutPreferences(key));
  // A different project opened, or the author chose while the read ran.
  if (projectKey !== key || changes !== before || unsaved) return;
  if (prefs.diagramLegendHidden === undefined) return;
  const next = prefs.diagramLegendHidden;
  if (next === hidden) return;
  hidden = next;
  logDebug(["frontend"], "the stored legend state of the diagram overlays applies", {
    legend: next ? "hidden" : "shown",
  });
  emit();
}

/**
 * EDT-FR-DNVJ: the Settings window is modal, so the main window gets its focus
 * back when the Settings window closes. A read then puts what that window
 * stored into the cache of this window, so a later write of an unrelated
 * layout field does not write an older legend state back.
 */
function onWindowFocus(): void {
  void refreshDiagramLegend();
}

/**
 * EDT-FR-NGCW: restore the legend state of the project that opens. Until the
 * read returns, and for a project with no stored value, the legend shows.
 */
export function restoreDiagramLegend(key: string): Promise<void> {
  if (projectKey !== key) {
    projectKey = key;
    unsaved = false;
    if (hidden) {
      hidden = false;
      logDebug(["frontend"], "the legend of the diagram overlays shows for the new project", {
        legend: "shown",
      });
      emit();
    }
  }
  if (!key) return Promise.resolve();
  if (!focusListening && typeof window !== "undefined") {
    window.addEventListener("focus", onWindowFocus);
    focusListening = true;
  }
  return read(key, false);
}

/**
 * EDT-FR-DNVJ: read the stored value again before an overlay opens, because
 * the other window can have changed it.
 */
export function refreshDiagramLegend(): Promise<void> {
  if (!projectKey) return Promise.resolve();
  return read(projectKey, true);
}

/**
 * EDT-FR-JFGN, EDT-FR-NGCW, EDT-FR-BHJR: the author hides or shows the legend.
 * The change applies at once. A write that fails keeps what the author chose.
 */
export function setDiagramLegendHidden(next: boolean): void {
  if (next === hidden) return;
  hidden = next;
  changes += 1;
  emit();
  logDebug(["frontend"], "the legend of the diagram overlays changed", {
    legend: next ? "hidden" : "shown",
  });
  if (!projectKey) return;
  const key = projectKey;
  const change = changes;
  void patchLayoutPreferences(key, { diagramLegendHidden: next }).then(
    () => {
      // Only the newest change says what the record holds now.
      if (projectKey === key && changes === change) unsaved = false;
    },
    (error: unknown) => {
      if (projectKey === key) unsaved = true;
      logWarn(["frontend"], "the legend state of the diagram overlays was not stored", {
        error: String(error),
      });
    },
  );
}

/** Tests: put the store back to its first state. */
export function resetDiagramLegendForTest(): void {
  hidden = false;
  projectKey = null;
  changes = 0;
  unsaved = false;
  if (focusListening && typeof window !== "undefined") {
    window.removeEventListener("focus", onWindowFocus);
  }
  focusListening = false;
  listeners.clear();
}

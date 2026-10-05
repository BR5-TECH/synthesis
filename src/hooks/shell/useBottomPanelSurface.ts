/**
 * The bottom panel's visibility, its active surface, and the per-project
 * persistence of both (SNV-FR-08 / SNV-FR-46 / SNV-FR-47).
 *
 * Split out of `useShellSession` whole — state, restore effect and the three
 * controls together — because the persistence is what ties them: every control
 * writes the same record the restore effect reads, and separating them would
 * leave a control that changes the panel without recording it.
 *
 * Named for the *surface* to keep it apart from `../useBottomPanel`, which is a
 * different hook about a different thing: that one owns the panel's height, its
 * drag, and the ceiling that keeps it from crowding the viewport out
 * (SNV-FR-50 .. SNV-FR-53).
 */
import { useEffect, useRef, useState } from "react";
import {
  loadLayoutPreferences,
  patchLayoutPreferences,
  setActiveLayoutProject,
} from "../../state/layoutPreferences";
import { nextPanelToggleState } from "../../state/panelToggle";
import { restoreDraftRatios } from "../../state/draftDiscussion";
import type { BottomSurface } from "../../types";

export interface BottomPanelSurface {
  bottomVisible: boolean;
  bottomSurface: BottomSurface;
  setBottomVisible: (visible: boolean) => void;
  showBottom: (surface: BottomSurface) => void;
  toggleBottomSurface: (surface: BottomSurface) => void;
  hideBottom: () => void;
}

export function useBottomPanelSurface(
  screen: "picker" | "ide",
  projectPath: string,
): BottomPanelSurface {
  const [bottomVisible, setBottomVisible] = useState(false);
  const [bottomSurface, setBottomSurface] = useState<BottomSurface>("runs");
  /**
   * SNV-FR-08: the bottom panel's visibility and active surface are persisted
   * per project, so a relaunch returns the author to the surface they were last
   * working in rather than always to Runs.
   *
   * Writes go through `patchLayoutPreferences`, which merges into the record as
   * it currently stands — the same record `useVerticalPanel` and
   * `useMainWindowState` write their own fields into, on different triggers.
   *
   * Held in a ref rather than read from `projectPath` directly so the persist
   * helpers below are stable and do not have to be re-created (and re-bound by
   * every caller) each time the path changes.
   */
  const layoutKeyRef = useRef("");

  // SNV-FR-08 / SNV-FR-46: restore this project's bottom-panel state. Keyed by
  // path, so switching projects (OVW-FR-11) reads the incoming project's slot
  // rather than leaving B showing A's panel.
  useEffect(() => {
    layoutKeyRef.current = projectPath;
    // Tell the layout store which project is live, so a patch issued for the
    // outgoing project and still in flight across a switch is dropped rather
    // than written into the incoming project's slot.
    setActiveLayoutProject(projectPath);
    if (screen !== "ide" || !projectPath) return;
    let cancelled = false;
    // DDS-FR-PNXR: how each draft's tab is split, restored per draft. It goes
    // through its own store rather than being read out here, because that store
    // is also what writes it — and a ratio the author sets is written against
    // the same record this read returns.
    void restoreDraftRatios(projectPath);
    loadLayoutPreferences(projectPath)
      .then((prefs) => {
        if (cancelled) return;
        setBottomSurface(prefs.bottomPanelSurface ?? "runs");
        // Only an explicit `false` opens the panel. A record that carries no
        // decision — never persisted, or written for some unrelated field —
        // leaves it shut, which is the bottom panel's default (unlike the
        // vertical panel's).
        setBottomVisible(prefs.bottomPanelHidden === false);
      })
      .catch(() => {
        // No persisted layout (or no backend): the Runs default stands, with
        // the panel closed.
      });
    return () => {
      cancelled = true;
    };
  }, [screen, projectPath]);

  /** Persist a bottom-panel field. A write with no project open is a no-op. */
  const persistBottom = (patch: {
    bottomPanelSurface?: BottomSurface;
    bottomPanelHidden?: boolean;
  }) => {
    const key = layoutKeyRef.current;
    if (!key) return;
    void patchLayoutPreferences(key, patch).catch(() => {
      // A failed write leaves the panel where the user left it for this session.
    });
  };

  /**
   * SNV-FR-47: open the bottom panel on `surface` and mark its toggle active.
   * The route every programmatic entry point takes — a Dashboard widget
   * (DSH-FR-06), a Search result (SCH-FR-08), an activated notification — so all
   * of them agree with the strip about where the panel is.
   */
  const showBottom = (surface: BottomSurface) => {
    setBottomVisible(true);
    setBottomSurface(surface);
    persistBottom({ bottomPanelSurface: surface, bottomPanelHidden: false });
  };

  /**
   * SNV-FR-46: one click on a bottom-panel toggle. Switches surface while the
   * panel is open, hides it when the clicked surface is the one already
   * showing, and reopens it on the clicked surface when hidden.
   */
  const toggleBottomSurface = (surface: BottomSurface) => {
    const next = nextPanelToggleState(
      { hidden: !bottomVisible, surface: bottomSurface },
      surface,
    );
    setBottomVisible(!next.hidden);
    setBottomSurface(next.surface);
    persistBottom({
      bottomPanelSurface: next.surface,
      bottomPanelHidden: next.hidden,
    });
  };

  /** SNV-FR-47: the panel's own hide control, and the File-menu-less routes. */
  const hideBottom = () => {
    setBottomVisible(false);
    persistBottom({ bottomPanelHidden: true });
  };

  return {
    bottomVisible,
    bottomSurface,
    setBottomVisible,
    showBottom,
    toggleBottomSurface,
    hideBottom,
  };
}

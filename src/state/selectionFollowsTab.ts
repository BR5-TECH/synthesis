/**
 * Selection follows tab — which vertical panel a newly-activated tab selects
 * in, and the gate that decides whether any of it happens
 * (`../../specifications/ui/SNV-shell-navigation.md` SNV-FR-64 … SNV-FR-69).
 *
 * The behaviour is one rule with two halves. This module owns the half that is
 * pure — mapping a tab to the item it is a view onto (SNV-FR-66) — plus the
 * user-global switch that gates it (GLS-FR-28 / GSS-FR-33). The other half, the
 * trigger (SNV-FR-65), lives in `useShellSession`, which is the only place that
 * can tell an activation from a tab being removed under the author.
 *
 * The gate is a module-level value rather than a prop for the reason
 * `state/notifications.ts` holds its own: the Navigation section that edits it
 * sits inside a tab, several levels below the shell that reads it, and a change
 * has to reach the next activation without a relaunch (GLS-FR-28). Seeded at
 * startup and updated by that section.
 */
import type { PanelSurface, Tab } from "../types";

/**
 * The item a tab is a view onto, and the panel it lives in (SNV-FR-66).
 *
 * `id` is whatever that panel reveals by: the Project panel's path-derived
 * stable key (ASC-FR-13), the Drafts panel's draft id, the Changes panel's
 * project-relative path.
 */
export interface FollowTarget {
  panel: PanelSurface;
  id: string;
}

/**
 * SNV-FR-66: which panel a tab selects in, decided by the kind of tab and by the
 * identity that tab already carries — never by re-deriving one.
 *
 * Returns `null` for every tab with no item in any vertical panel to name: a
 * Dashboard, Search results, History detail, conversation, Document, PDF
 * Viewer, Global settings, or Search results tab. A `null` here is what SNV-FR-66's "selects nowhere and
 * changes nothing" is made of, and it is deliberately indistinguishable from
 * "this tab has an item but it cannot be resolved" (SNV-FR-67) at the call site,
 * because both leave the panel exactly as it was.
 */
export function followTargetForTab(tab: Tab | undefined): FollowTarget | null {
  if (!tab) return null;

  // A New Artifact tab selects by the id of the draft it is bound to
  // (TAB-FR-17) rather than by the draft's display name or the path of its
  // prompt, so a draft renamed or moved since is still the row that is reached.
  if (tab.kind === "draft") {
    return tab.draftId ? { panel: "drafts", id: tab.draftId } : null;
  }

  // A Diff tab selects by the file half of the pair identifying it (DFV-FR-04).
  // The comparison half names which change set the tab is about, which the
  // Changes panel decides for itself and a reveal never changes (CHG-FR-54).
  if (tab.kind === "diff") {
    return tab.diff?.path ? { panel: "changes", id: tab.diff.path } : null;
  }

  // An Editor tab and a Flow tab both select in the Project panel, by the tab's
  // own file (TAB-FR-04). A tab with no `artifactId` is one opened on something
  // that is not a project file — a Dashboard canned row — so there is no node to
  // reveal and nothing happens (SNV-FR-67).
  if (tab.kind === "editor" || tab.kind === "flow") {
    return tab.artifactId ? { panel: "library", id: tab.artifactId } : null;
  }

  // SNV-FR-66 / SMP-FR-KQTD: the Map tab is a view onto the whole corpus, which
  // no vertical panel holds as an item.
  if (tab.kind === "map") return null;

  // SNV-FR-66: a Document tab and a PDF Viewer tab name a reference file that
  // is identified by a document id, and no panel selects by one. The Documents
  // panel shows no selection, so activating one changes nothing.
  if (tab.kind === "document" || tab.kind === "pdf") return null;

  return null;
}

/**
 * GSS-FR-33: on until the author turns it off. The default is asserted here as
 * well as in `DEFAULT_APP_PREFERENCES` because this value is read before the
 * preference load resolves — a tab activated in that window follows, rather than
 * silently not following for the first moments of a session.
 */
let enabled = true;

/** GLS-FR-28: seeded at startup and re-set whenever the Navigation switch moves. */
export function setSelectionFollowsTab(next: boolean): void {
  enabled = next;
}

/** SNV-FR-64: the gate, read at every qualifying activation rather than captured. */
export function selectionFollowsTabEnabled(): boolean {
  return enabled;
}

/** Restore the default. Tests only — production holds one gate for the app's life. */
export function resetSelectionFollowsTab(): void {
  enabled = true;
}

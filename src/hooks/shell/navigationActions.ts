/**
 * The click-throughs that land the author on the artifact a panel row names
 * (NTS-FR-11, LCM-FR-05). Revealing a discussion is a different route, in
 * `./discussionActions`.
 *
 * Plain closures rather than a hook: the session store and the panel state stay
 * owned by `useShellSession`.
 */
import type {
  NotesEntity,
  OpenableArtifact,
  PanelSurface,
} from "../../types";

export interface NavigationActionDeps {
  openArtifact: (item: OpenableArtifact) => void;
  setPanelSurface: (surface: PanelSurface) => void;
  setRevealArtifactId: (id: string | null) => void;
  setNotesEntityOverride: (entity: NotesEntity | null) => void;
}

export interface NavigationActions {
  revealArtifact: (item: OpenableArtifact) => void;
  showNotesFor: (artifact: OpenableArtifact) => void;
}

export function createNavigationActions(
  deps: NavigationActionDeps,
): NavigationActions {
  const {
    openArtifact,
    setPanelSurface,
    setRevealArtifactId,
    setNotesEntityOverride,
  } = deps;

  /**
   * NTS-FR-11: follow a Notes group header to the artifact it names — reveal it
   * in the Library and open it in its natural surface, by the same routing the
   * Library click-through uses. The reveal half is the pair the search's
   * workstream route performs (SCH-FR-09); the open half is `openArtifact`, so
   * a Flow lands in a Flow tab rather than an Editor one (LIB-FR-03).
   */
  const revealArtifact = (item: OpenableArtifact) => {
    openArtifact(item);
    if (item.id) {
      setPanelSurface("library");
      setRevealArtifactId(item.id);
    }
  };

  // The Library "Notes" context-menu action: reveal the Notes surface scoped to
  // the chosen artifact, without opening it in a tab.
  const showNotesFor = (artifact: OpenableArtifact) => {
    // NTS-FR-02: the panel binds to the entity's id (its path-derived key,
    // ASC-FR-13) — the name alone would not name a note's scope. A canned row
    // carrying no id cannot be scoped to, so the panel stays project-wide.
    setNotesEntityOverride(
      artifact.id ? { id: artifact.id, name: artifact.name } : null,
    );
    setPanelSurface("notes");
  };

  return { revealArtifact, showNotesFor };
}

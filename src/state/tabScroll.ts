/**
 * EDT-FR-UERT / NAW-FR-IHDU / DFV-FR-YMJA: where each open tab is scrolled to.
 *
 * The viewport mounts the active tab alone, so a tab's scroller unmounts
 * whenever the author activates another tab. The offsets therefore live here,
 * outside the component, keyed by the tab and by the surface inside it (one
 * document of a draft, one editing mode). The shell drops a tab's offsets when
 * the tab leaves the strip, so a new tab for the same content starts at the top.
 * It is memory only: nothing here is written to disk.
 *
 * Nothing subscribes to this store. A scroll event writes an offset and no
 * component renders again because of it.
 */

const offsets = new Map<string, Map<string, number>>();

/** The offset the surface was left at, or 0 when it has none. */
export function tabScrollOf(tabId: string, surface: string): number {
  return offsets.get(tabId)?.get(surface) ?? 0;
}

export function holdTabScroll(
  tabId: string,
  surface: string,
  top: number,
): void {
  let surfaces = offsets.get(tabId);
  if (!surfaces) {
    surfaces = new Map();
    offsets.set(tabId, surfaces);
  }
  surfaces.set(surface, top);
}

/** The surface of one document in one editing mode, as the Editor names it. */
export function documentSurface(documentId: string, mode: string): string {
  return `${documentId}\u0000${mode}`;
}

/**
 * NAW-FR-IHDU: a document renamed while its tab stays open keeps its offsets
 * under the new name, in every tab that holds them. `renamed` gives the new id
 * of a document, or null for a document the rename does not move.
 */
export function rekeyDocumentScroll(
  renamed: (documentId: string) => string | null,
): void {
  for (const surfaces of offsets.values()) {
    for (const [surface, top] of [...surfaces]) {
      const cut = surface.lastIndexOf("\u0000");
      if (cut === -1) continue;
      const next = renamed(surface.slice(0, cut));
      if (next === null) continue;
      surfaces.delete(surface);
      surfaces.set(documentSurface(next, surface.slice(cut + 1)), top);
    }
  }
}

/**
 * Forget the offsets of every tab that is not in the strip any more. Called by
 * the shell with the ids of the tabs its strip still holds.
 */
export function pruneTabScroll(openTabIds: ReadonlySet<string>): void {
  for (const id of [...offsets.keys()]) {
    if (!openTabIds.has(id)) offsets.delete(id);
  }
}

/** Forget everything. Tests only. */
export function resetTabScrollForTest(): void {
  offsets.clear();
}

/**
 * The tab work of the Documents collection: opening a document in its viewer
 * tab, and closing the viewer tabs of documents that a source removal took out
 * of the collection.
 *
 * A viewer tab is read-only. It holds no editing session and no savable buffer,
 * so no write is brought forward when it closes and no close is ever refused
 * (TAB-FR-LKCT, TAB-FR-FJXM). It is identified by the document id (TAB-FR-QXRF).
 *
 * Plain closures rather than a hook: the strip stays owned by `useShellSession`.
 */
import { useEffect, type Dispatch, type MutableRefObject, type SetStateAction } from "react";
import { logDebug } from "../../logging";
import { pruneViewState } from "../../state/documentViewState";
import type { DocumentEntry, Tab } from "../../types";
import { documentTabId, neverEmpty } from "./tabRecords";

export interface DocumentActionDeps {
  tabsRef: MutableRefObject<Tab[]>;
  setTabs: Dispatch<SetStateAction<Tab[]>>;
  setActiveTab: Dispatch<SetStateAction<string>>;
  activateTab: (id: string) => void;
}

export interface DocumentActions {
  openDocument: (entry: DocumentEntry) => void;
  closeTabsForRemovedDocuments: (documentIds: string[]) => void;
}

export function createDocumentActions(deps: DocumentActionDeps): DocumentActions {
  const { tabsRef, setTabs, setActiveTab, activateTab } = deps;

  /**
   * DPN-FR-CDFO / TAB-FR-QXRF: open a document in a **Document tab** (Markdown
   * and text) or a **PDF Viewer tab** (PDF), or jump focus to the tab that
   * already shows it. The presence test runs inside the updater, so two requests
   * landing before a render cannot add a second tab. An unavailable document
   * opens its tab on the same terms: the tab shows its unavailable state.
   */
  const openDocument = (entry: DocumentEntry) => {
    const id = documentTabId(entry.id);
    setTabs((ts) =>
      ts.some((t) => t.id === id)
        ? ts
        : [
            ...ts,
            {
              id,
              label: entry.name,
              // DTV-FR-UGVG / PDV-FR-IWDK: the tooltip is the full path.
              tooltip: entry.path,
              kind: entry.format === "pdf" ? "pdf" : "document",
              documentId: entry.id,
            },
          ],
    );
    activateTab(id);
  };

  /**
   * TAB-FR-KUIN: a viewer tab closes when a source removal took its document out
   * of the collection. The close needs no prompt and writes nothing. A document
   * that stays through another source is not in `documentIds`, so it keeps its
   * tab. Focus landing on a neighbour is a consequence rather than a navigation,
   * so the raw setter is used (SNV-FR-65).
   */
  const closeTabsForRemovedDocuments = (documentIds: string[]) => {
    if (documentIds.length === 0) return;
    const gone = new Set(documentIds);
    const closing = tabsRef.current.filter(
      (t) => t.documentId !== undefined && gone.has(t.documentId),
    );
    if (closing.length === 0) return;
    const closingIds = new Set(closing.map((t) => t.id));

    let remaining: Tab[] = [];
    setTabs((ts) => {
      remaining = neverEmpty(ts.filter((t) => !closingIds.has(t.id)));
      tabsRef.current = remaining;
      return remaining;
    });
    setActiveTab((current) =>
      remaining.some((t) => t.id === current)
        ? current
        : (remaining[remaining.length - 1]?.id ?? current),
    );
    for (const t of closing) {
      // TAB-FR-ZMGX: one DEBUG record per closed viewer tab. No path, no name.
      logDebug(["frontend"], "tab closed: document source removed", {
        documentId: t.documentId,
        tabKind: t.kind,
        rule: "TAB-FR-KUIN",
      });
    }
  };

  return { openDocument, closeTabsForRemovedDocuments };
}

/**
 * DTV-FR-XMRL / PDV-FR-DXUX: what a viewer tab keeps belongs to the tab, so it
 * is dropped when the tab leaves the strip, by whichever route. A new tab for
 * the same document then starts from the defaults.
 */
export function useViewerStatePruning(tabs: Tab[]): void {
  useEffect(() => {
    pruneViewState(
      new Set(tabs.flatMap((t) => (t.documentId ? [t.documentId] : []))),
    );
  }, [tabs]);
}

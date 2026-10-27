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
import * as api from "../../api";
import { logDebug, logWarn } from "../../logging";
import { pruneViewState } from "../../state/documentViewState";
import type { DocumentEntry, Tab } from "../../types";
import { documentTabId, neverEmpty } from "./tabRecords";

export interface DocumentActionDeps {
  tabsRef: MutableRefObject<Tab[]>;
  setTabs: Dispatch<SetStateAction<Tab[]>>;
  setActiveTab: Dispatch<SetStateAction<string>>;
  activateTab: (id: string) => void;
  /**
   * TAB-FR-UEKW: open a file of the active worktree in its Editor tab, by its
   * project-relative path.
   */
  openProjectFile: (path: string, name: string) => void;
}

export interface DocumentActions {
  openDocument: (entry: DocumentEntry) => void;
  openDocumentById: (documentId: string) => void;
  openDocumentByProjectPath: (path: string) => void;
  closeTabsForRemovedDocuments: (documentIds: string[]) => void;
}

export function createDocumentActions(deps: DocumentActionDeps): DocumentActions {
  const { tabsRef, setTabs, setActiveTab, activateTab, openProjectFile } = deps;

  /**
   * DPN-FR-CDFO / TAB-FR-QXRF: open a document in a **Document tab** (Markdown
   * and text) or a **PDF Viewer tab** (PDF), or jump focus to the tab that
   * already shows it. The presence test runs inside the updater, so two requests
   * landing before a render cannot add a second tab. An unavailable document
   * opens its tab on the same terms: the tab shows its unavailable state.
   */
  const openDocument = (entry: DocumentEntry) => {
    // TAB-FR-UEKW: a Markdown or text document of the active worktree opens in
    // the Editor, where it can be edited and discussed. A PDF always opens in
    // its viewer.
    if (entry.format !== "pdf" && entry.project_path) {
      logDebug(["frontend"], "document opened in the editor", { documentId: entry.id });
      openProjectFile(entry.project_path, entry.name);
      return;
    }
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
   * SCH-FR-09: open a document named by its id, as the Documents panel opens
   * it. The entry is read through `"list documents"`, so the tab carries the
   * collection's own name, path, and format. A document that left the
   * collection opens nothing.
   */
  const openDocumentById = (documentId: string) => {
    void api
      .listDocuments()
      .then((snapshot) => {
        const entry = snapshot.documents.find((d) => d.id === documentId);
        if (entry) openDocument(entry);
        else logWarn(["frontend"], "search hit names a document no longer in the collection", { documentId });
      })
      .catch((e) => {
        logWarn(["frontend"], "documents could not be listed to open a search hit", {
          error: e instanceof Error ? e.message : String(e),
        });
      });
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

  /**
   * TAB-FR-GVOS: a Document-typed PDF clicked in the Project panel opens the
   * PDF Viewer tab of its document. The document is found by its project path.
   * A file that is not in the collection opens nothing.
   */
  const openDocumentByProjectPath = (path: string) => {
    void api
      .listDocuments()
      .then((snapshot) => {
        const entry = snapshot.documents.find((d) => d.project_path === path);
        if (entry) openDocument(entry);
        else logWarn(["frontend"], "project file is not a document of the collection", { path });
      })
      .catch((e) => {
        logWarn(["frontend"], "documents could not be listed to open a project file", {
          error: e instanceof Error ? e.message : String(e),
        });
      });
  };

  return {
    openDocument,
    openDocumentById,
    openDocumentByProjectPath,
    closeTabsForRemovedDocuments,
  };
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

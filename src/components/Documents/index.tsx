/**
 * The Documents vertical panel (`specifications/ui/DPN-documents-panel.md`).
 *
 * The panel lists the reference documents of the project as a tree, adds a file
 * or a folder through the OS picker, shows every selected source with its state,
 * and removes a source. It owns nothing on disk: the collection of
 * `DCL-documents-collection.md` is the single owner of what is selected, and the
 * panel renders the snapshot it gets from `"list documents"`, from the commands
 * it calls, and from `"documents changed"`.
 *
 * The filter text and the expansion of the folders live in a session store
 * (`documentsPanelState`), because a switch to another panel unmounts this
 * component and a change of worktree remounts the whole shell subtree
 * (DPN-FR-AREM).
 */
import { useEffect, useMemo, useRef, useState } from "react";
import * as api from "../../api";
import { logError, logInfo, logWarn } from "../../logging";
import { documentErrorCode } from "../../state/documentErrors";
import { useDocumentsPanelState } from "../../state/documentsPanelState";
import type {
  DocumentEntry,
  DocumentSource,
  DocumentsSnapshot,
  PickDocumentSourcesMode,
} from "../../types";
import { Icon } from "../icons";
import {
  EMPTY_BODY_CLASS,
  FILTERED_BODY_CLASS,
  PanelEmptyState,
  PanelFilteredState,
} from "../PanelEmptyState";
import { AddDocumentsMenu } from "./AddDocumentsMenu";
import { DocumentTree } from "./DocumentTree";
import { SourcesSection } from "./SourcesSection";
import { buildTree, documentIds, filterTree, visibleRows } from "./tree";
import { useDocumentsSnapshot } from "./useDocumentsSnapshot";

export interface DocumentsPanelProps {
  /** DPN-FR-CDFO: open the viewer tab of a document, or focus the open one. */
  onOpenDocument: (entry: DocumentEntry) => void;
  /**
   * DPN-FR-ZHEJ: the ids of the documents that a removal took out of the
   * collection. The shell closes their viewer tabs.
   */
  onDocumentsRemoved: (documentIds: string[]) => void;
  /** SNV-FR-56: called before the choice opens, so other overlays close first. */
  onOverlayOpening?: () => void;
}

const ADD_MARK = "data-documents-add";

function ignoredLine(count: number): string {
  return count === 1
    ? "1 selected file was ignored because its type is not supported."
    : `${count} selected files were ignored because their type is not supported.`;
}

export function DocumentsPanel({
  onOpenDocument,
  onDocumentsRemoved,
  onOverlayOpening,
}: DocumentsPanelProps) {
  const panel = useDocumentsPanelState();
  const { status, snapshot, latest, replace, retry } = useDocumentsSnapshot();
  const rootRef = useRef<HTMLDivElement | null>(null);
  const mounted = useRef(true);
  const [menu, setMenu] = useState<{ anchor: HTMLElement } | null>(null);
  const [picking, setPicking] = useState(false);
  const [removing, setRemoving] = useState<string | null>(null);
  const [notice, setNotice] = useState<{
    kind: "status" | "alert";
    text: string;
  } | null>(null);
  const [activeKey, setActiveKey] = useState<string | null>(null);
  const focusAfter = useRef<{ index: number } | "add" | null>(null);

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  const documents = snapshot?.documents;
  const built = useMemo(() => buildTree(documents ?? []), [documents]);
  const filtered = useMemo(
    () => filterTree(built, panel.filter),
    [built, panel.filter],
  );
  const filtering = panel.filter.trim() !== "";

  // DPN-FR-AREM: a folder that has not been toggled starts expanded. While the
  // filter holds text, a folder with a match renders expanded (DPN-FR-TELU).
  const isExpanded = (folderKey: string): boolean =>
    filtering && filtered.open.has(folderKey)
      ? !panel.isFilterCollapsed(folderKey)
      : panel.isExpanded(folderKey);
  const rows = visibleRows(filtered.nodes, isExpanded);

  const toggle = (folderKey: string, expanded: boolean) => {
    // A folder closed while the filter shows it open is closed for that filter
    // text only, and the choice is session memory like the rest (DPN-FR-AREM).
    if (filtering && filtered.open.has(folderKey)) {
      panel.setFilterCollapsed(folderKey, !expanded);
      return;
    }
    panel.setExpanded(folderKey, expanded);
  };

  // DPN-FR-RTLG: after a removal the focus goes to the next source row, or to
  // Add documents when none is left.
  useEffect(() => {
    const target = focusAfter.current;
    if (target === null || !rootRef.current) return;
    focusAfter.current = null;
    const removes = rootRef.current.querySelectorAll<HTMLButtonElement>(
      ".documents-source__remove",
    );
    if (target !== "add" && removes[target.index]) {
      removes[target.index].focus();
      return;
    }
    rootRef.current
      .querySelector<HTMLButtonElement>(`[${ADD_MARK}], .panel-empty__action`)
      ?.focus();
  }, [snapshot, picking, removing]);

  const openMenu = (anchor: HTMLElement) => {
    // SNV-FR-56: at most one floating overlay, so the others close first.
    onOverlayOpening?.();
    setMenu({ anchor });
  };

  const pick = (mode: PickDocumentSourcesMode) => {
    setMenu(null);
    setNotice(null);
    setPicking(true);
    const before = latest();
    api
      .pickDocumentSources(mode)
      .then((result) => {
        if (!mounted.current) return;
        setPicking(false);
        // A cancelled picker changes nothing and shows nothing (DPN-FR-BADJ).
        if (!result.cancelled) {
          replace(result.snapshot);
          logInfo(["frontend"], "document source added", {
            kind: mode === "files" ? "file" : "folder",
            added: Math.max(
              0,
              result.snapshot.sources.length - (before?.sources.length ?? 0),
            ),
            sources: result.snapshot.sources.length,
            documents: result.snapshot.documents.length,
          });
          if (result.ignored_count > 0) {
            logWarn(["frontend"], "pick ignored unsupported files", {
              ignored: result.ignored_count,
            });
            setNotice({ kind: "status", text: ignoredLine(result.ignored_count) });
          }
        }
        focusAfter.current = "add";
      })
      .catch((error: unknown) => {
        logError(["frontend"], "document source pick failed", {
          mode,
          reason: documentErrorCode(error) ?? "unknown",
        });
        if (!mounted.current) return;
        setPicking(false);
        setNotice({ kind: "alert", text: "Documents could not be added." });
        focusAfter.current = "add";
      });
  };

  const remove = (source: DocumentSource, index: number) => {
    setNotice(null);
    setRemoving(source.path);
    const before = latest();
    api
      .removeDocumentSource(source.path)
      .then((next: DocumentsSnapshot) => {
        if (!mounted.current) return;
        setRemoving(null);
        focusAfter.current = { index };
        replace(next);
        logInfo(["frontend"], "document source removed", {
          kind: source.kind,
          sources: next.sources.length,
          documents: next.documents.length,
        });
        // DPN-FR-ZHEJ: a document that another source still includes stays.
        const kept = documentIds(next.documents);
        const gone = (before?.documents ?? [])
          .map((d) => d.id)
          .filter((id) => !kept.has(id));
        if (gone.length > 0) onDocumentsRemoved(gone);
      })
      .catch((error: unknown) => {
        logError(["frontend"], "document source removal failed", {
          reason: documentErrorCode(error) ?? "unknown",
        });
        if (!mounted.current) return;
        setRemoving(null);
        setNotice({ kind: "alert", text: "The source could not be removed." });
      });
  };

  const ready = status === "ready" && snapshot !== null;
  const nothingAtAll =
    ready && snapshot.sources.length === 0 && snapshot.documents.length === 0;
  const noDocuments = ready && !nothingAtAll && snapshot.documents.length === 0;
  const filteredToNothing =
    ready && snapshot.documents.length > 0 && rows.length === 0;

  const addButton = (
    <button
      type="button"
      className="btn btn--sm documents-add"
      {...{ [ADD_MARK]: "" }}
      aria-haspopup="menu"
      aria-expanded={menu !== null}
      aria-busy={picking}
      disabled={picking}
      onClick={(event) => {
        if (menu) setMenu(null);
        else openMenu(event.currentTarget);
      }}
    >
      <Icon.Plus size={12} aria-hidden="true" />
      Add documents
    </button>
  );

  let bodyClass = "vpanel__body documents-panel__body";
  if (nothingAtAll) bodyClass = EMPTY_BODY_CLASS;
  else if (filteredToNothing) bodyClass = FILTERED_BODY_CLASS;

  return (
    <div ref={rootRef} className="documents-panel">
      <div className="panel-header">
        <span className="panel-header__title">Documents</span>
        {/* DPN-FR-UKXW: the empty state carries the one Add documents action. */}
        {!nothingAtAll && addButton}
      </div>

      {/* DPN-FR-BADJ: one status line, announced politely. The region stays
          mounted so an announcement made into it is heard. */}
      <p
        className="documents-notice"
        role={notice?.kind === "alert" ? "alert" : "status"}
        aria-live={notice?.kind === "alert" ? "assertive" : "polite"}
        data-kind={notice?.kind}
      >
        {notice?.text}
      </p>

      {/* DPN-FR-UKXW: with nothing at all there is nothing to narrow. */}
      {ready && !nothingAtAll && (
        <div className="panel-controls">
          <div className="search-input" style={{ height: 24 }}>
            <Icon.Search size={12} aria-hidden="true" />
            <input
              placeholder="Filter documents…"
              aria-label="Filter documents"
              value={panel.filter}
              onChange={(event) => panel.setFilter(event.target.value)}
            />
          </div>
        </div>
      )}

      <div className={bodyClass}>
        {status === "loading" && (
          <p className="notes__message" role="status">
            Loading documents…
          </p>
        )}
        {status === "error" && (
          <div className="notes__message documents-error" role="alert">
            <p className="documents-error__text">Documents could not be loaded.</p>
            <button type="button" className="btn btn--sm" onClick={retry}>
              Retry
            </button>
          </div>
        )}
        {nothingAtAll && (
          <PanelEmptyState
            line="No documents"
            action={{
              label: "Add documents",
              icon: <Icon.Plus size={12} aria-hidden="true" />,
              onClick: (event) => openMenu(event.currentTarget),
            }}
          >
            Documents are reference files. They stay where they are on disk.
          </PanelEmptyState>
        )}
        {noDocuments && (
          <p className="notes__message">
            No supported documents in the selected sources.
          </p>
        )}
        {filteredToNothing && (
          <PanelFilteredState>
            No documents match{" "}
            <span className="documents-filtered__text">{panel.filter}</span>
          </PanelFilteredState>
        )}
        {ready && rows.length > 0 && (
          <DocumentTree
            rows={rows}
            activeKey={activeKey}
            onActiveChange={setActiveKey}
            onToggle={toggle}
            onOpenDocument={onOpenDocument}
          />
        )}
      </div>

      {ready && !nothingAtAll && (
        <SourcesSection
          sources={snapshot.sources}
          removing={removing}
          onRemove={remove}
        />
      )}

      {menu && (
        <AddDocumentsMenu
          anchor={menu.anchor}
          onChoose={pick}
          onClose={(restoreFocus) => {
            setMenu(null);
            if (restoreFocus) menu.anchor.focus();
          }}
        />
      )}
    </div>
  );
}

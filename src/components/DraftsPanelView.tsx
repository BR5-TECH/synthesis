/**
 * Everything the Drafts panel draws: the filter chrome, the tree, the pinned
 * create affordance, the empty and filtered-to-nothing states, and the modals
 * over the lot (`../../specifications/ui/DRP-drafts-panel.md`).
 *
 * A presentational component over the panel's own state — it owns none of it
 * and holds no hook of its own. Split out so `DraftsPanel.tsx` reads as the
 * tree the panel is, with the markup that renders it beside it.
 */
import type React from "react";

import { DraftInformationModal } from "./DraftInformationModal";
import { Icon } from "./icons";
import { SelectorRow } from "./SelectorRow";
import {
  EMPTY_BODY_CLASS,
  FILTERED_BODY_CLASS,
  PanelEmptyState,
  PanelFilteredState,
} from "./PanelEmptyState";
import { AnchoredMenu, Modal, MovePicker } from "./DraftsPanelParts";
import {
  countPhrase,
  folderName,
  ROOT,
  STATUS_POSITIONS,
} from "./draftsTree";
import type { MenuAnchor, TreeItem } from "./draftsTree";
import type { Inline, Overlay } from "./draftsRows";
import type { DraftsPanel as DraftsPanelState } from "../hooks/useDraftsPanelState";
import type { DraftFolder, DraftHierarchy, DraftSummary } from "../types";

export interface DraftsPanelViewProps {
  panel: DraftsPanelState;
  hierarchy: DraftHierarchy | null;
  error: string | null;
  folders: DraftFolder[];
  rootFolders: DraftFolder[];
  rootDrafts: DraftSummary[];
  rootIsTarget: boolean;
  rootRef: React.RefObject<HTMLDivElement | null>;
  nothingAtAll: boolean;
  filteredToNothing: boolean;
  announcement: string;

  overlay: Overlay | null;
  setOverlay: (next: Overlay | null) => void;
  anchor: MenuAnchor | null;
  menuOpener: React.MutableRefObject<HTMLElement | null>;
  openRowMenu: (
    e: React.MouseEvent | React.KeyboardEvent,
    next: Overlay,
    row: HTMLElement,
  ) => void;
  inline: Inline | null;
  closeInline: () => void;

  dragging: TreeItem | null;
  setDragging: (item: TreeItem | null) => void;
  setDropTarget: React.Dispatch<React.SetStateAction<string | null>>;
  moveRefusal: (item: TreeItem, destination: string) => string | null;
  commitMove: (item: TreeItem, destination: string) => Promise<void>;

  /** DRP-FR-25: the folder the confirmation is about, and what it holds. */
  deleteTarget: {
    path: string;
    drafts: number;
    folders: number;
    parent: string;
  } | null;
  draftDeleteTarget: DraftSummary | null;
  draftInformationTarget: DraftSummary | null;
  doDeleteDraft: (draft: DraftSummary) => Promise<void>;
  doDeleteFolder: (path: string) => Promise<void>;
  doNewDraft: (folder: string) => Promise<void>;
  doNewFolder: (parent: string) => void;

  menuEntry: ReturnType<
    typeof import("./draftsRows").makeRowRenderers
  >["menuEntry"];
  inlineField: ReturnType<
    typeof import("./draftsRows").makeRowRenderers
  >["inlineField"];
  renderDraft: (draft: DraftSummary, depth: number) => React.ReactNode;
  renderFolder: (folder: DraftFolder, depth: number) => React.ReactNode;
}

export function DraftsPanelView(props: DraftsPanelViewProps) {
  const {
    panel,
    hierarchy,
    error,
    folders,
    rootFolders,
    rootDrafts,
    rootIsTarget,
    rootRef,
    nothingAtAll,
    filteredToNothing,
    announcement,
    overlay,
    setOverlay,
    anchor,
    menuOpener,
    openRowMenu,
    inline,
    closeInline,
    dragging,
    setDragging,
    setDropTarget,
    moveRefusal,
    commitMove,
    deleteTarget,
    draftDeleteTarget,
    draftInformationTarget,
    doDeleteDraft,
    doDeleteFolder,
    doNewDraft,
    doNewFolder,
    menuEntry,
    inlineField,
    renderDraft,
    renderFolder,
  } = props;

  return (
    <div
      ref={rootRef}
      style={{ display: "flex", flexDirection: "column", height: "100%" }}
    >
      <div className="panel-header">
        <span className="panel-header__title">Drafts</span>
        {/* DRP-FR-22: the implicit root offers New Draft and New Folder alone,
            having nothing to rename, move, or delete. It is reached through the
            panel's own chrome rather than through a row of its own — the root is
            the tree's container, never a folder in it (DRP-FR-20). */}
        {!nothingAtAll && (
          <span className="drafts-tree__root-actions">
            <button
              className="icon-btn icon-btn--xs drafts-tree__menu-btn"
              aria-label="Drafts actions"
              aria-haspopup="menu"
              aria-expanded={overlay?.kind === "root-menu"}
              onClick={(e) => {
                if (overlay?.kind === "root-menu") {
                  setOverlay(null);
                  return;
                }
                openRowMenu(e, { kind: "root-menu" }, e.currentTarget);
              }}
            >
              ⋯
            </button>
            {overlay?.kind === "root-menu" && (
              <AnchoredMenu anchor={anchor} label="Drafts actions">
                {menuEntry("New Draft", <Icon.Diamond size={12} />, () =>
                  void doNewDraft(ROOT),
                )}
                {menuEntry("New Folder", <Icon.Folder size={12} />, () => {
                  setOverlay(null);
                  doNewFolder(ROOT);
                })}
              </AnchoredMenu>
            )}
          </span>
        )}
      </div>

      {/* DRP-FR-15: with nothing at all the filters are not rendered — there is
          one thing to do, and a control to narrow an empty tree is not it.

          SNV-FR-58: the text filter first, directly beneath the panel header,
          and the status selector beneath it and directly above the tree. */}
      {!nothingAtAll && (
        <div className="panel-controls">
          <div className="search-input" style={{ height: 24 }}>
            <Icon.Search size={12} />
            <input
              placeholder="Filter name or contents…"
              aria-label="Filter drafts"
              value={panel.text}
              onChange={(e) => panel.setText(e.target.value)}
            />
          </div>
          <SelectorRow
            label="Draft status"
            positions={STATUS_POSITIONS}
            value={panel.filter}
            onChange={panel.setFilter}
          />
        </div>
      )}

      <div
        className={
          nothingAtAll
            ? EMPTY_BODY_CLASS
            : filteredToNothing
              ? FILTERED_BODY_CLASS
              : "vpanel__body"
        }
        data-drop-target={rootIsTarget ? "true" : undefined}
        onDragOver={(e) => {
          if (!dragging) return;
          if (moveRefusal(dragging, ROOT) !== null) {
            setDropTarget(null);
            return;
          }
          e.preventDefault();
          if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
          setDropTarget(ROOT);
        }}
        onDragLeave={() => setDropTarget((c) => (c === ROOT ? null : c))}
        onDrop={(e) => {
          e.preventDefault();
          const item = dragging;
          setDragging(null);
          setDropTarget(null);
          if (item && moveRefusal(item, ROOT) === null) void commitMove(item, ROOT);
        }}
        // DRP-FR-22: the implicit root's own menu. It has no row to right-click
        // — it is the tree's container — so the panel's background is what
        // stands in for one. Rows stop propagation, so this only fires on the
        // space around them.
        onContextMenu={(e) =>
          openRowMenu(e, { kind: "root-menu" }, e.currentTarget)
        }
      >
        {error && <div className="notes__message">{error}</div>}

        {/* DRP-FR-15 / SNV-FR-60: the block every vertical panel renders when it
            has nothing at all, carrying the create affordance as its single
            button. The pinned affordance is not rendered alongside it. */}
        {nothingAtAll && (
          <PanelEmptyState
            line="No drafts yet."
            action={{
              label: "New draft",
              icon: <Icon.Plus size={12} />,
              onClick: () => void doNewDraft(ROOT),
            }}
          >
            A draft is where a new artifact is developed — with AI, by hand, or
            both — before it enters the project.
          </PanelEmptyState>
        )}

        {/* DRP-FR-15 / SNV-FR-61: a filter matching nothing stays in the tree's
            own region with both controls above still present, because the
            author's next move is to change them rather than to make a draft. */}
        {filteredToNothing && (
          <PanelFilteredState>No draft matches this filter.</PanelFilteredState>
        )}

        {!nothingAtAll && hierarchy !== null && (
          <div className="drafts-tree" role="tree" aria-label="Drafts">
            {/* DRP-FR-23: a folder created at the root gets its field here. */}
            {inline?.kind === "new-folder" && inline.parent === ROOT && (
              // Explicitly zero, like every other depth-0 row: the stylesheet's
              // own horizontal padding would otherwise indent the provisional
              // row past the folders it is about to sit among.
              <div className="drafts-tree__folder" style={{ paddingLeft: 0 }}>
                <span className="drafts-tree__caret" aria-hidden="true">
                  <Icon.CaretRight size={10} />
                </span>
                <span className="drafts-tree__icon" aria-hidden="true">
                  <Icon.Folder size={12} />
                </span>
                {inlineField("New folder name", closeInline)}
              </div>
            )}
            {rootFolders.map((folder) => renderFolder(folder, 0))}
            {rootDrafts.map((draft) => renderDraft(draft, 0))}
          </div>
        )}
      </div>

      {/* DRP-FR-06: pinned below the tree. It asks for no name, because a draft
          is named by working in it rather than before it exists, and it creates
          the draft at the tree's root. */}
      {!nothingAtAll && (
        <div className="drafts__footer">
          <button className="btn btn--sm" onClick={() => void doNewDraft(ROOT)}>
            <Icon.Plus size={12} /> Draft
          </button>
        </div>
      )}

      {/* DRP-FR-12: the draft confirmation names the draft and says what goes
          with it. */}
      {draftDeleteTarget && (
        <Modal
          label="Delete draft"
          returnFocus={menuOpener.current}
          onClose={() => setOverlay(null)}
        >
          <>
            <div className="modal__head">
              <div className="modal__title">Delete draft</div>
            </div>
            <div className="modal__body">
              <p className="t-ui-sm">
                Delete “{draftDeleteTarget.name}”? Its prompt, its version
                history, its conversation, and its review threads go with it.
                Nothing in the project is touched.
              </p>
            </div>
            <div className="modal__actions">
              <button className="btn btn--ghost" onClick={() => setOverlay(null)}>
                Cancel
              </button>
              <button
                className="btn btn--danger"
                onClick={() => void doDeleteDraft(draftDeleteTarget)}
              >
                Delete
              </button>
            </div>
          </>
        </Modal>
      )}

      {/* DRP-FR-KDVX: Information opens that draft's statistics modal and does
          nothing else — it invokes no operation of this panel's own, changes no
          draft, and leaves the row, the selection, and the tree exactly as they
          were (per `DFI-draft-information.md` DFI-FR-ZGBU). */}
      {draftInformationTarget && (
        <DraftInformationModal
          draftId={draftInformationTarget.id}
          draftName={draftInformationTarget.name}
          returnFocus={menuOpener.current}
          onClose={() => setOverlay(null)}
        />
      )}

      {/* DRP-FR-25: the folder confirmation states that what it holds moves to
          its parent rather than being deleted, and counts it. */}
      {deleteTarget && (
        <Modal
          label="Delete folder"
          returnFocus={menuOpener.current}
          onClose={() => setOverlay(null)}
        >
          <>
            <div className="modal__head">
              <div className="modal__title">Delete folder</div>
            </div>
            <div className="modal__body">
              <p className="t-ui-sm">Delete “{folderName(deleteTarget.path)}”?</p>
              <p className="t-ui-sm">
                {/* Counted so the author knows the size of what is about to
                    move — and an empty count is elided rather than rendered as
                    "0 drafts", which reads as a mistake. */}
                {countPhrase(deleteTarget.drafts, deleteTarget.folders)} to{" "}
                {deleteTarget.parent === ROOT
                  ? "Drafts"
                  : folderName(deleteTarget.parent)}
                , not to the wastebasket. Nothing is deleted.
              </p>
            </div>
            <div className="modal__actions">
              <button className="btn btn--ghost" onClick={() => setOverlay(null)}>
                Cancel
              </button>
              <button
                className="btn btn--danger"
                onClick={() => void doDeleteFolder(deleteTarget.path)}
              >
                Delete
              </button>
            </div>
          </>
        </Modal>
      )}

      {/* DRP-FR-28: every move available by dragging is available without a
          pointer. Destinations the move would be refused on are listed as
          unavailable rather than omitted, so the reason a folder cannot be a
          destination is legible. */}
      {overlay?.kind === "move" && (
        <MovePicker
          item={overlay.item}
          folders={folders}
          returnFocus={menuOpener.current}
          refusalFor={(destination) => moveRefusal(overlay.item, destination)}
          onCancel={() => setOverlay(null)}
          onMove={(destination) => {
            setOverlay(null);
            void commitMove(overlay.item, destination);
          }}
        />
      )}

      {/* DRP-FR-28: what has just happened and where the item now sits. */}
      <div className="sr-only" role="status" aria-live="polite">
        {announcement}
      </div>
    </div>
  );
}

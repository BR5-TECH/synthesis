/**
 * `MIV-map-issue-view.md`: when the issue view of the board opens and closes.
 * The open view is kept apart from the selection, because a selection made by
 * a reveal or a marker opens no view (MIV-FR-TTZU).
 */
import { useEffect, useRef, useState } from "react";

import { logInfo } from "../../../logging";
import type { FlowCanvasState } from "../useFlowCanvas";
import type { IssueViewHost } from "./host";

interface OpenView {
  id: string;
  /** MIV-FR-HOAE: a view the author opened takes the focus; a restored one does not. */
  focus: boolean;
}

export function useIssueView(
  c: FlowCanvasState,
  host: IssueViewHost | undefined,
  /** Whether a click on the item opens its view: not a ghost card (MIV-FR-TTZU). */
  opens: (itemId: string) => boolean,
) {
  const enabled = c.board;
  const blocked = host?.blocked === true;
  const [open, setOpen] = useState<OpenView | null>(() =>
    host?.initialOpen ? { id: host.initialOpen, focus: false } : null,
  );
  const only = c.selection.length === 1 ? c.selection[0] : null;

  // MIV-FR-HULC: another selection, no selection, or a drag closes the view.
  // MIV-FR-ITSA: so does the review sheet.
  const dragging = c.lifted !== null;
  const closes = open !== null && (only !== open.id || dragging || blocked);
  const closing = closes ? open.id : null;
  const cause = blocked ? "review" : dragging ? "drag" : "selection";
  useEffect(() => {
    if (closing === null) return;
    // A click on another item may open its view before this runs: keep it.
    setOpen((prev) => (prev?.id === closing ? null : prev));
    logInfo(["frontend"], "issue view closed", { cause });
  }, [closing, cause]);

  // MIV-FR-RPAU: the host keeps the open view for the next mount.
  const report = useRef(host?.onOpenChange);
  report.current = host?.onOpenChange;
  const openId = open?.id ?? null;
  useEffect(() => {
    report.current?.(openId);
  }, [openId]);

  /** MIV-FR-SVXH: open the view of an item, selecting it. */
  const openFor = (id: string): boolean => {
    if (!enabled || blocked || !opens(id)) return false;
    c.setSelection([id]);
    c.setSelectedEdge(null);
    c.setEditingName(null);
    setOpen({ id, focus: true });
    logInfo(["frontend"], "issue view opened", {});
    return true;
  };

  /** MIV-FR-YPWK: close the view; the item stays selected and takes the focus. */
  const close = () => {
    if (open === null) return;
    const id = open.id;
    setOpen(null);
    logInfo(["frontend"], "issue view closed", { cause: "close" });
    const card = c.surfaceRef.current?.querySelector<HTMLElement>(
      `[data-testid="flow-item-${id.replace(/["\\]/g, "\\$&")}"]`,
    );
    card?.focus();
  };

  const shown =
    enabled && open !== null && !closes && c.selected?.id === open.id ? open : null;
  return { shown, openFor, close };
}

export type IssueViewState = ReturnType<typeof useIssueView>;

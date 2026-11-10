/**
 * CAD-FR-RBYK – CAD-FR-QLQZ: the two views of an Editor tab on an
 * Architecture artifact. The Source view is the source surface of the file
 * (`ESH-editor-source-files.md` ESH-FR-SSDV). The Diagram view is the
 * interactive CALM canvas of the buffer, read-only in every state of the tab.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { logDebug } from "../../logging";
import {
  CalmFlowSelect,
  CalmStage,
  CalmZoomControls,
  useCalmInteraction,
} from "../CalmDiagram/CalmInteractive";
import { useCalmRead } from "../CalmDiagram/useCalmRead";
import type { CalmLayout } from "../../calm/types";
import { NO_DIAGRAM } from "../DiagramEditor/DiagramPreview";

export type ArchitectureMode = "diagram" | "source";

/** CAD-FR-OJTD: the name and the tooltip of the toggle, by the view it shows. */
export const SHOW_ARCHITECTURE_SOURCE = "Show architecture source";
export const SHOW_ARCHITECTURE_DIAGRAM = "Show architecture diagram";
/** CAD-FR-OZST: the control that leaves the Diagram view for the source. */
export const SHOW_SOURCE = "Show source";

export interface ArchitectureViewState {
  /** The tab is an Architecture tab and has a view. */
  active: boolean;
  view: ArchitectureMode;
  toggle: () => void;
  /** CAD-FR-OZST: shows the Source view, with the caret at `line` when given. */
  showSource: (line?: number) => void;
}

/**
 * CAD-FR-NQXR, CAD-FR-PVCT: the view of an Architecture tab. It is decided
 * once, when the buffer has loaded: the Diagram view, or the Source view for an
 * empty buffer, or the Source view when the type came after the tab opened.
 */
export function useArchitectureView({
  architecture,
  loaded,
  buffer,
  findActive,
  focusedThread,
  reviewing,
  sourceRef,
}: {
  architecture: boolean;
  loaded: boolean;
  /** The buffer of the artifact, which holds the loaded text before the surface does. */
  buffer: string;
  findActive: boolean;
  focusedThread: string | null;
  /** A proposed change is under review, which shows on the source surface. */
  reviewing: boolean;
  sourceRef: React.RefObject<HTMLTextAreaElement | null>;
}): ArchitectureViewState {
  const [view, setView] = useState<ArchitectureMode | null>(null);
  const viewRef = useRef(view);
  viewRef.current = view;
  const openedUntyped = useRef(false);

  useEffect(() => {
    if (!loaded || view !== null) return;
    if (!architecture) {
      openedUntyped.current = true;
      return;
    }
    const empty = buffer.trim() === "";
    setView(openedUntyped.current || empty ? "source" : "diagram");
  }, [architecture, loaded, buffer, view]);

  const switchTo = useCallback((next: ArchitectureMode) => {
    if (viewRef.current === next) return;
    viewRef.current = next;
    // CAD-FR-QLQZ: the name of the new view, and no text of the document.
    logDebug(["frontend"], "the Architecture view changed", { view: next });
    setView(next);
  }, []);

  // CAD-FR-PVCT: opening Find and activating a comment thread switch to the
  // source text, which they work on. The start of a review of a proposed
  // change does too, because the review shows on the source surface. Each is
  // the moment it starts: the author can then switch back to the diagram.
  const opened = useRef({ findActive, focusedThread, reviewing });
  useEffect(() => {
    const last = opened.current;
    opened.current = { findActive, focusedThread, reviewing };
    const started =
      (findActive && !last.findActive) ||
      (focusedThread !== null && focusedThread !== last.focusedThread) ||
      (reviewing && !last.reviewing);
    if (started && viewRef.current === "diagram") switchTo("source");
  }, [findActive, focusedThread, reviewing, switchTo]);

  const showSource = useCallback(
    (line?: number) => {
      switchTo("source");
      if (line === undefined) return;
      requestAnimationFrame(() => {
        const el = sourceRef.current;
        if (!el) return;
        let offset = 0;
        for (let at = 1; at < line; at++) {
          const next = el.value.indexOf("\n", offset);
          if (next < 0) break;
          offset = next + 1;
        }
        el.focus();
        el.setSelectionRange(offset, offset);
      });
    },
    [switchTo, sourceRef],
  );

  const active = architecture && view !== null;
  return {
    active,
    view: active ? (view ?? "source") : "source",
    toggle: () => switchTo(view === "diagram" ? "source" : "diagram"),
    showSource,
  };
}

/** CAD-FR-RBYK, CAD-FR-OPTU – CAD-FR-PDVE: the Diagram view of the buffer. */
export function ArchitectureDiagramView({
  sourceText,
  onShowSource,
}: {
  sourceText: string;
  onShowSource: (line?: number) => void;
}) {
  const { empty, settled, lastRendered } = useCalmRead(sourceText);
  const message = (summary: string, detail?: string, line?: number) => (
    <div className="architecture-view" data-testid="architecture-diagram">
      <div className="architecture-view__message" role="status">
        <p className="architecture-view__summary">{summary}</p>
        {detail !== undefined && <pre className="architecture-view__detail">{detail}</pre>}
        <button type="button" className="btn btn--sm" onClick={() => onShowSource(line)}>
          {SHOW_SOURCE}
        </button>
      </div>
    </div>
  );
  // CAD-FR-PDVE
  if (empty) return message(NO_DIAGRAM);
  // CAD-FR-OZST
  if (settled?.state === "error") {
    return message(`Diagram error: ${settled.summary}`, settled.detail, settled.line);
  }
  const layout = settled?.state === "rendered" ? settled.layout : lastRendered?.layout;
  if (!layout) {
    return (
      <div className="architecture-view" data-testid="architecture-diagram">
        <p className="architecture-view__message" role="status">
          Reading the architecture…
        </p>
      </div>
    );
  }
  return <ArchitectureCanvas layout={layout} />;
}

function ArchitectureCanvas({ layout }: { layout: CalmLayout }) {
  const ix = useCalmInteraction(layout);
  return (
    <div className="architecture-view" data-testid="architecture-diagram">
      <div className="architecture-view__bar">
        <CalmFlowSelect ix={ix} />
        <CalmZoomControls ix={ix} />
      </div>
      <CalmStage ix={ix} />
    </div>
  );
}

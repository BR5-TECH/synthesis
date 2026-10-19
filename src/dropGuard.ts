/**
 * SNV-FR-UKER: a file or a link released in the main window over a place that
 * takes no drop changes nothing. DRP-FR-PXMK: a drag from a Drafts panel row is
 * a move within that panel and nothing else.
 *
 * The main window hands every drag to its web content (SNV-FR-RQLF). A web
 * page that does not accept a dropped file or link lets the engine load it, and
 * that load would take the whole window away from the application. This guard
 * refuses such a drop where nothing else accepted it, so the window stays where
 * it is.
 *
 * It stands aside for every drop that a surface takes: a drop target cancels
 * `dragover` and `drop` itself. A text field accepts a dropped link as text on
 * its own terms, but no text field takes a file, so a file over a text field
 * is accepted here and its `drop` reaches the field first — the editor takes an
 * image there (EDT-FR-86), and a drop that nothing took is cancelled.
 *
 * A row drag of the Drafts panel is stopped before any surface sees it outside
 * the panel's own region, so no editor or composer shows a drop affordance for
 * it.
 */
import { DRAG_TYPE, DRAG_ZONE_ATTRIBUTE } from "./components/draftsTree";
import { logDebug } from "./logging";

/** Input types that take dropped text. */
const TEXT_INPUT_TYPES = ["", "text", "search", "url", "email", "tel", "password"];

function hasType(e: DragEvent, type: string): boolean {
  const types = e.dataTransfer?.types;
  return !!types && Array.from(types).includes(type);
}

const carriesFiles = (e: DragEvent) => hasType(e, "Files");
const carriesLink = (e: DragEvent) => hasType(e, "text/uri-list");
const carriesText = (e: DragEvent) => hasType(e, "text/plain");

/** Whether the drop lands in a field that takes dropped text. */
function isTextField(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  // The nearest `contenteditable` decides, so an island marked `false` inside
  // an editable region is not a text field.
  const host = target.closest("[contenteditable]");
  if (host && host.getAttribute("contenteditable")?.toLowerCase() !== "false") return true;
  if (target instanceof HTMLTextAreaElement) return !target.readOnly && !target.disabled;
  if (target instanceof HTMLInputElement)
    return (
      TEXT_INPUT_TYPES.includes(target.getAttribute("type")?.toLowerCase() ?? "") &&
      !target.readOnly &&
      !target.disabled
    );
  return false;
}

/** DRP-FR-PXMK: a Drafts panel row drag over a place outside the panel. */
function strayRowDrag(e: DragEvent): boolean {
  if (!hasType(e, DRAG_TYPE)) return false;
  const target = e.target;
  return !(target instanceof Element && target.closest(`[${DRAG_ZONE_ATTRIBUTE}]`));
}

/** Whether the guard must refuse this `dragover` (no surface took it). */
function unclaimedOver(e: DragEvent): boolean {
  if (e.defaultPrevented) return false;
  if (carriesFiles(e)) return true;
  return carriesLink(e) && !isTextField(e.target);
}

/**
 * Whether the guard must cancel this `drop`. A text field inserts a link from
 * its plain-text form alone; a link without one would be loaded instead.
 */
function unclaimedDrop(e: DragEvent): boolean {
  if (e.defaultPrevented) return false;
  if (carriesFiles(e)) return true;
  return carriesLink(e) && !(isTextField(e.target) && carriesText(e));
}

/**
 * Installs the guard on `target` and returns the function that removes it.
 *
 * The refusal of a file or a link sits in the bubble phase, so every surface
 * below has seen the event first and `defaultPrevented` says whether one took
 * it. The refusal of a stray row drag sits in the capture phase, so no surface
 * outside the panel sees that drag at all.
 */
export function installDropGuard(target: Window = window): () => void {
  // One record per drag rather than one per `dragover`, which fires many times
  // a second.
  let logged = false;
  const refuse = (e: DragEvent, reason: string) => {
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = "none";
    if (logged) return;
    logged = true;
    logDebug(["frontend"], "drag refused", {
      reason,
      types: Array.from(e.dataTransfer?.types ?? []).join(","),
    });
  };

  const onCaptureOver = (event: Event) => {
    const e = event as DragEvent;
    if (!strayRowDrag(e)) return;
    e.stopPropagation();
    refuse(e, "drafts row outside the panel");
  };
  const onCaptureDrop = (event: Event) => {
    const e = event as DragEvent;
    if (!strayRowDrag(e)) return;
    e.stopPropagation();
    e.preventDefault();
  };
  const onDragOver = (event: Event) => {
    const e = event as DragEvent;
    if (!unclaimedOver(e)) return;
    if (carriesFiles(e) && isTextField(e.target)) {
      // A cancelled `dragover` tells the engine that the page handles the
      // drag, so it loads nothing; the field still sees the `drop`.
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = "copy";
      return;
    }
    // A cancelled `dragover` with the effect `none` refuses the drop outright.
    refuse(e, "no drop target");
  };
  const onDrop = (event: Event) => {
    const e = event as DragEvent;
    if (!unclaimedDrop(e)) return;
    e.preventDefault();
    logDebug(["frontend"], "drop refused: no target took it", {
      types: Array.from(e.dataTransfer?.types ?? []).join(","),
    });
  };
  /** A new drag may be logged once its predecessor has ended or left. */
  const onEnd = (event: Event) => {
    if (event.type === "dragleave" && (event as DragEvent).relatedTarget !== null) return;
    logged = false;
  };

  target.addEventListener("dragover", onCaptureOver, true);
  target.addEventListener("drop", onCaptureDrop, true);
  target.addEventListener("dragover", onDragOver);
  target.addEventListener("drop", onDrop);
  for (const type of ["dragend", "dragleave", "drop"]) target.addEventListener(type, onEnd);
  return () => {
    target.removeEventListener("dragover", onCaptureOver, true);
    target.removeEventListener("drop", onCaptureDrop, true);
    target.removeEventListener("dragover", onDragOver);
    target.removeEventListener("drop", onDrop);
    for (const type of ["dragend", "dragleave", "drop"]) target.removeEventListener(type, onEnd);
  };
}

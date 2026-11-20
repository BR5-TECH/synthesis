import { useEffect } from "react";
import { bindCanvasGestures } from "../components/canvasGestures";
import {
  endEditorZoomPinch,
  pinchEditorZoom,
  startEditorZoomPinch,
} from "../state/editorZoom";

/** EZM-FR-QGOU: the class of a zoom region. */
export const EDITOR_ZOOM_REGION = "editor-zoom";
/**
 * EZM-FR-BNFJ: a surface inside a region that has its own zoom: the diagram
 * area of the diagram viewer and of an Architecture tab, and any surface that
 * carries `data-own-zoom`.
 */
const OWN_ZOOM_SURFACE = ".diagram-viewer__area, [data-own-zoom]";

/**
 * EZM-FR-QGOU / EZM-FR-BNFJ / EZM-FR-COUH: a pinch over `target` changes the
 * editor zoom level only inside a region, and not over a surface with its own
 * zoom.
 */
export function acceptsEditorZoomTarget(target: EventTarget | null): boolean {
  if (!(target instanceof Element)) return false;
  return target.closest(`.${EDITOR_ZOOM_REGION}`) !== null && target.closest(OWN_ZOOM_SURFACE) === null;
}

/**
 * EZM-FR-GMSX / EZM-FR-VQHI: a trackpad pinch, a Ctrl or Cmd wheel, and a
 * two-finger touchscreen pinch over a zoom region change the editor zoom level.
 * One set of listeners on the document root serves every tab, and the target
 * filter leaves every gesture outside the regions alone.
 */
export function useEditorZoomGestures(): void {
  useEffect(() => {
    return bindCanvasGestures(document.documentElement, {
      accepts: acceptsEditorZoomTarget,
      touch: true,
      onPinchStart: startEditorZoomPinch,
      onPinchEnd: endEditorZoomPinch,
      onPinch: (factor, clientX, clientY) => pinchEditorZoom(factor, { clientX, clientY }),
    });
  }, []);
}

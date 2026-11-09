/**
 * EXD-FR-UKLK, EXD-FR-ZQMX: the CALM pictures of an export.
 *
 * Before the export, the dialog lists the CALM sources of the saved file and
 * makes the print drawing of each (CAD-FR-QHEJ) from the same read that the
 * rich editor shows. A source with no drawing gets no picture, so the export
 * keeps its fence as code and states it (DXP-FR-VMPP).
 */
import { listExportCalmSources } from "../../api";
import { readCalm } from "../../calm/read";
import { logDebug, logWarn } from "../../logging";
import type { CalmPicture } from "../../types";

/** EXD-FR-QWZO: the count of sources drawn, of all sources. */
export type DrawProgress = (done: number, total: number) => void;

/**
 * EXD-FR-UKLK: the print drawing of each CALM source of the saved file at
 * `path`. A list call that fails rejects with its typed error.
 */
export async function drawCalmPictures(path: string, progress: DrawProgress): Promise<CalmPicture[]> {
  const started = Date.now();
  const sources = await listExportCalmSources(path);
  const pictures: CalmPicture[] = [];
  progress(0, sources.length);
  if (sources.length > 0) {
    // The drawing needs the canvas library, so it loads only for a CALM file.
    const { calmPrintSvg } = await import("../CalmDiagram/printSvg");
    for (const [index, source] of sources.entries()) {
      let step: string | null = null;
      try {
        const result = await readCalm(source);
        if (result.state === "error") {
          step = result.step;
        } else {
          // CAD-FR-VBDT: a graph with no nodes gives no drawing.
          const svg = calmPrintSvg(result.layout);
          if (svg === null) step = "no_nodes";
          else pictures.push({ source, svg });
        }
      } catch {
        step = "draw";
      }
      if (step !== null) {
        // EXD-FR-ZQMX: the step alone, never CALM text.
        logWarn(["frontend"], "a CALM diagram of a document export has no picture", { step });
      }
      progress(index + 1, sources.length);
    }
  }
  logDebug(["frontend"], "the CALM pictures of a document export were drawn", {
    sources: sources.length,
    pictures: pictures.length,
    durationMs: Date.now() - started,
  });
  return pictures;
}

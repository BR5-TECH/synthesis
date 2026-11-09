/**
 * CAD-FR-QHEJ, DXP-FR-DMUH: the print drawings of the CALM templates are the
 * fixtures that the backend rasterises in its own tests
 * (`src-tauri/src/document_export/convert/tests_calm.rs`). Thus the backend
 * draws each SVG feature that the drawing uses. Run with
 * `UPDATE_CALM_FIXTURES=1` to write the fixtures again.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";

import { beforeAll, describe, expect, it, vi } from "vitest";

vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { readCalm } from "../../calm/read";
import { CALM_TEMPLATES } from "../../calm/templates";
import { estimateWidth } from "./printMeasure";
import { calmPrintSvg } from "./printSvg";

const FIXTURES = resolve(__dirname, "../../../src-tauri/src/document_export/convert/fixtures");

beforeAll(async () => {
  // The first read loads the CALM libraries.
  await readCalm(CALM_TEMPLATES[0].source);
}, 30_000);

describe("CALM print drawing fixtures", () => {
  it.each(CALM_TEMPLATES.map((t) => [t.key] as const))(
    "CAD-FR-QHEJ: the fixture of the %s template is the current print drawing",
    async (key) => {
      const template = CALM_TEMPLATES.find((t) => t.key === key)!;
      const result = await readCalm(template.source);
      if (result.state !== "rendered") throw new Error(`${key}: ${result.summary}`);
      const svg = calmPrintSvg(result.layout, estimateWidth);
      expect(svg).not.toBeNull();
      const file = resolve(FIXTURES, `calm-${key}.svg`);
      if (process.env.UPDATE_CALM_FIXTURES === "1") writeFileSync(file, `${svg}\n`);
      expect(readFileSync(file, "utf8")).toBe(`${svg}\n`);
    },
    15_000,
  );
});

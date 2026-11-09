import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("@xyflow/react", async (importOriginal) =>
  (await import("../../test/calmMock")).xyflowModule(await importOriginal()),
);
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { readCalm, resetCalmForTest } from "../../calm/read";
import { CALM_TEMPLATES } from "../../calm/templates";
import { CalmStatic } from "./CalmStatic";

const SCHEMA = "https://calm.finos.org/release/1.3/meta/calm.json";
const VALID = JSON.stringify({
  $schema: SCHEMA,
  nodes: [{ "unique-id": "a", "node-type": "service", name: "Alpha", description: "A." }],
});
const fallback = (source: string) => (
  <pre data-testid="fallback">
    <code>{source}</code>
  </pre>
);


/**
 * The first read of a test file loads the schema validator and the layout
 * engine. That load happens here, under the hook limit, so that no test waits
 * for it under the shorter limit of a `findBy` on a slow runner.
 */
beforeAll(async () => {
  await readCalm(CALM_TEMPLATES[0].source);
});

beforeEach(() => resetCalmForTest());
afterEach(() => cleanup());

describe("an inert CALM fence outside an editing surface", () => {
  it("DFV-FR-EGFA, CAD-FR-PFNM: the source shows while the read runs, then the inert canvas", async () => {
    render(<CalmStatic source={VALID}>{fallback(VALID)}</CalmStatic>);
    expect(screen.getByTestId("fallback")).toBeInTheDocument();
    const figure = await screen.findByRole("img", { name: "CALM architecture diagram" });
    expect(figure).toHaveTextContent("Alpha");
    expect(figure).not.toHaveAttribute("tabindex");
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.queryByTestId("fallback")).toBeNull();
  });

  it("DFV-FR-EGFA, CAD-FR-ITUS: a fence that does not read shows its source and its error", async () => {
    render(<CalmStatic source="{ nope">{fallback("{ nope")}</CalmStatic>);
    await screen.findByText("Diagram error: The CALM source is not valid JSON.");
    expect(screen.getByTestId("fallback")).toBeInTheDocument();
  });
});

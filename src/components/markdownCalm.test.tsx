import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act } from "@testing-library/react";

vi.mock("@xyflow/react", async (importOriginal) =>
  (await import("../test/calmMock")).xyflowModule(await importOriginal()),
);
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { readCalm, resetCalmForTest } from "../calm/read";
import { CALM_TEMPLATES } from "../calm/templates";
import type { CalmLayout } from "../calm/types";
import { mountCalmFigure, unmountCalmFigures } from "./markdownCalm";

let layout: CalmLayout;

beforeEach(async () => {
  resetCalmForTest();
  const result = await readCalm(CALM_TEMPLATES[0].source);
  if (result.state !== "rendered") throw new Error("expected a graph");
  layout = result.layout;
});

afterEach(() => {
  document.body.replaceChildren();
});

describe("the inline canvas of a CALM fence widget", () => {
  it("CAD-FR-IPRR: a widget mounts its canvas, and its destroy unmounts it", async () => {
    const head = document.createElement("span");
    const figure = document.createElement("span");
    head.appendChild(figure);
    document.body.appendChild(head);
    await act(async () => mountCalmFigure(figure, layout));
    expect(figure).toHaveClass("calm-figure");
    expect(figure.querySelector(".calm-inline")).not.toBeNull();
    await act(async () => {
      unmountCalmFigures(head);
      await Promise.resolve();
    });
    expect(figure.childElementCount).toBe(0);
  });

  it("CAD-FR-IPRR: a second destroy of the same widget does nothing", async () => {
    const figure = document.createElement("span");
    document.body.appendChild(figure);
    await act(async () => mountCalmFigure(figure, layout));
    await act(async () => {
      unmountCalmFigures(figure);
      unmountCalmFigures(figure);
      await Promise.resolve();
    });
    expect(figure.childElementCount).toBe(0);
  });
});

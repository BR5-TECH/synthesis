/**
 * The zoom control of the status bar (`STB-status-bar.md` STB-FR-VRUS,
 * STB-FR-OQTE, STB-FR-LBHP).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const patchMock = vi.fn<(patch: Record<string, unknown>) => Promise<unknown>>();
vi.mock("../state/appPreferences", () => ({
  patchAppPreferences: (patch: Record<string, unknown>) => patchMock(patch),
  resetAppPreferencesCache: () => {},
}));
vi.mock("../logging", () => ({ logDebug: vi.fn(), logWarn: vi.fn() }));

import { StatusBarZoom } from "./StatusBarZoom";
import {
  EDITOR_ZOOM_STEPS,
  getEditorZoom,
  pinchEditorZoom,
  resetEditorZoomForTest,
  setEditorZoom,
  startEditorZoomPinch,
} from "../state/editorZoom";

beforeEach(() => {
  resetEditorZoomForTest();
  patchMock.mockReset();
  patchMock.mockResolvedValue({});
});

afterEach(() => {
  cleanup();
  resetEditorZoomForTest();
});

function optionLabels(): string[] {
  return within(screen.getByTestId("zoom-select"))
    .getAllByRole("option")
    .map((o) => o.textContent ?? "");
}

describe("StatusBarZoom", () => {
  it("STB-FR-VRUS: shows the level as a whole percent and offers each step", () => {
    render(<StatusBarZoom zoomable />);
    const select = screen.getByRole("combobox", { name: "Editor zoom" });
    expect(select).toHaveValue("100");
    expect(optionLabels()).toEqual(EDITOR_ZOOM_STEPS.map((s) => `${s}%`));
  });

  it("STB-FR-VRUS, EZM-FR-VPYL: selecting a step sets the level and saves it", async () => {
    render(<StatusBarZoom zoomable />);
    await userEvent.selectOptions(screen.getByTestId("zoom-select"), "150");
    expect(getEditorZoom()).toBe(150);
    expect(screen.getByTestId("zoom-select")).toHaveValue("150");
    expect(patchMock).toHaveBeenCalledWith({ editorZoomPercent: 150 });
  });

  it("STB-FR-VRUS, EZM-FR-MXAB: follows a change of the level from elsewhere", () => {
    render(<StatusBarZoom zoomable />);
    act(() => setEditorZoom(250));
    expect(screen.getByTestId("zoom-select")).toHaveValue("250");
  });

  it("STB-FR-OQTE: a level between steps shows as one more option in its place", () => {
    render(<StatusBarZoom zoomable />);
    act(() => {
      startEditorZoomPinch();
      pinchEditorZoom(1.374);
    });
    expect(screen.getByTestId("zoom-select")).toHaveValue("137");
    const labels = optionLabels();
    expect(labels).toHaveLength(EDITOR_ZOOM_STEPS.length + 1);
    expect(labels.slice(7, 10)).toEqual(["125%", "137%", "150%"]);
  });

  it("STB-FR-OQTE: the extra option goes away when the level becomes a step", () => {
    render(<StatusBarZoom zoomable />);
    act(() => {
      startEditorZoomPinch();
      pinchEditorZoom(1.374);
    });
    act(() => setEditorZoom(150));
    expect(optionLabels()).toEqual(EDITOR_ZOOM_STEPS.map((s) => `${s}%`));
  });

  it("STB-FR-LBHP: on a tab that is not zoomable it shows a disabled Zoom with no level", () => {
    act(() => setEditorZoom(175));
    render(<StatusBarZoom zoomable={false} />);
    const inert = screen.getByTestId("zoom-inert");
    expect(inert).toHaveTextContent("Zoom");
    expect(inert).not.toHaveTextContent("%");
    expect(inert).toHaveAttribute("aria-disabled", "true");
    expect(screen.queryByRole("combobox")).toBeNull();
  });
});

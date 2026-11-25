/**
 * The diagram editor overlay in the Settings window
 * (`../specifications/ui/EDT-editor.md` EDT-FR-TOVY, EDT-FR-SJGJ, EDT-FR-SRXG,
 * and `SET-project-settings.md` SET-FR-16).
 *
 * The window mounts its own host, so the Edit button of a Draft template fence
 * opens the overlay inside this window. SNV-FR-56 does not apply here: the
 * window sets no closer of other overlays.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

vi.mock("mermaid", async () => (await import("./test/mermaidMock")).mermaidModule);

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  emit: vi.fn(async () => undefined),
  listen: vi.fn(async () => () => {}),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));

const { openingSpy } = vi.hoisted(() => ({ openingSpy: vi.fn() }));
vi.mock("./components/DiagramEditor/store", async (importOriginal) => {
  const actual = await importOriginal<typeof import("./components/DiagramEditor/store")>();
  return {
    ...actual,
    setDiagramEditorOpening: (next: (() => void) | null) => {
      openingSpy(next);
      actual.setDiagramEditorOpening(next);
    },
  };
});

import { SettingsWindowApp } from "./SettingsWindowApp";
import { isDiagramEditorOpen, resetDiagramEditorForTest } from "./components/DiagramEditor/store";
import { resetLogBufferForTest } from "./logging";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetSettingsSections } from "./state/settingsSweep";
import { resetMermaidMock } from "./test/mermaidMock";
import { resetMermaidForTest } from "./components/markdownMermaidRender";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import type { LayoutPreferences } from "./types";

const TEMPLATE = "# Draft\n\n```mermaid\ngraph TD\nA-->B\n```\n";

/** EDT-FR-DNVJ: the layout record that this window shares with the main window. */
let layout: LayoutPreferences | null = null;
let savedLayouts: LayoutPreferences[] = [];

function backend() {
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "load_layout_preferences":
        return layout && { ...layout };
      case "save_layout_preferences":
        layout = args?.preferences as LayoutPreferences;
        savedLayouts.push(layout);
        return undefined;
      case "get_settings_window_context":
        return { projectName: "acme", projectKey: "~/dev/acme", contentRoot: "~/dev/acme" };
      case "load_app_preferences":
        return { theme: "dark" };
      case "load_project_config":
        return { lineEndings: "lf", draftTemplate: TEMPLATE };
      case "save_project_config":
      case "save_app_preferences":
      case "append_log_records":
        return undefined;
      case "list_system_fonts":
        return [];
      default:
        return Promise.reject(new Error(`unexpected invoke ${cmd}`));
    }
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  openingSpy.mockReset();
  resetLogBufferForTest();
  resetAppPreferencesCache();
  resetSettingsSections();
  resetMermaidForTest();
  resetMermaidMock();
  resetDiagramEditorForTest();
  resetLayoutPreferencesCache();
  layout = null;
  savedLayouts = [];
  backend();
});

afterEach(() => {
  cleanup();
  resetAppPreferencesCache();
  resetSettingsSections();
});

const dialog = () => screen.getByRole("dialog", { name: "Edit Mermaid diagram" });

/** Render the window on the Draft template, and open the overlay there. */
async function openOverlay() {
  render(<SettingsWindowApp initialSection="project/template" />);
  // The fence draws again once its diagram has rendered, so the button is
  // taken from that drawing.
  await screen.findByRole("button", { name: "Mermaid diagram, edit source" });
  const settings = document.querySelector(".ProseMirror") as HTMLElement;
  fireEvent.click(within(settings).getByRole("button", { name: "Edit diagram" }));
  expect(dialog()).toBeInTheDocument();
  return settings;
}

describe("the diagram viewer overlay in the Settings window", () => {
  it("SET-FR-16, EDT-FR-TOVY, EDT-FR-YVAI: View diagram on a Draft template fence opens the viewer inside this window", async () => {
    render(<SettingsWindowApp initialSection="project/template" />);
    await screen.findByRole("button", { name: "Mermaid diagram, edit source" });
    const settings = document.querySelector(".ProseMirror") as HTMLElement;
    fireEvent.click(await within(settings).findByRole("button", { name: "View diagram" }));
    const viewer = screen.getByRole("dialog", { name: "View Mermaid diagram" });
    expect(settings.contains(viewer)).toBe(false);
    expect(openingSpy).not.toHaveBeenCalledWith(expect.any(Function));
    fireEvent.keyDown(viewer, { key: "Escape" });
    expect(isDiagramEditorOpen()).toBe(false);
  });
});

describe("the diagram editor overlay in the Settings window", () => {
  it("EDT-FR-TOVY, SET-FR-16: Edit diagram on a Draft template fence opens the overlay inside this window", async () => {
    const settings = await openOverlay();
    expect(within(dialog()).getByRole("textbox", { name: "Diagram source" })).toHaveValue(
      "graph TD\nA-->B",
    );
    // The overlay is drawn over the window, outside the settings surface.
    expect(settings.contains(dialog())).toBe(false);
    expect(screen.getByTestId("diagram-editor-scrim")).toContainElement(dialog());
  });

  it("EDT-FR-TOVY, SNV-FR-56: the window sets no closer of other overlays, so opening calls none", async () => {
    await openOverlay();
    expect(openingSpy).not.toHaveBeenCalledWith(expect.any(Function));
  });

  it("EDT-FR-SRXG: Escape closes the overlay, and the Draft template stays", async () => {
    await openOverlay();
    fireEvent.keyDown(within(dialog()).getByRole("textbox", { name: "Diagram source" }), {
      key: "Escape",
    });
    expect(screen.queryByRole("dialog", { name: "Edit Mermaid diagram" })).toBeNull();
    expect(isDiagramEditorOpen()).toBe(false);
    expect(screen.getByRole("button", { name: "Edit diagram" })).toBeInTheDocument();
  });

  it("EDT-FR-SRXG: Close closes the overlay, and the window can open it again", async () => {
    await openOverlay();
    fireEvent.click(within(dialog()).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog", { name: "Edit Mermaid diagram" })).toBeNull();
    expect(isDiagramEditorOpen()).toBe(false);

    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Edit diagram" })).toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "Edit diagram" }));
    expect(dialog()).toBeInTheDocument();
  });
});

describe("the legend state in the Settings window", () => {
  it("EDT-FR-DNVJ, EDT-FR-NGCW: the overlay shows the stored legend state of the project, and a toggle stores it", async () => {
    layout = { mainWindowMaximized: true, diagramLegendHidden: true };
    await openOverlay();
    expect(
      await within(dialog()).findByRole("button", { name: "Show syntax hints" }),
    ).toBeInTheDocument();
    expect(within(dialog()).queryByRole("region", { name: "Mermaid syntax hints" })).toBeNull();
    fireEvent.click(within(dialog()).getByRole("button", { name: "Show syntax hints" }));
    expect(within(dialog()).getByRole("region", { name: "Mermaid syntax hints" })).toBeInTheDocument();
    await waitFor(() =>
      expect(savedLayouts[savedLayouts.length - 1]).toEqual({ mainWindowMaximized: true, diagramLegendHidden: false }),
    );
  });

  it("EDT-FR-NGCW: with no project behind the window, the legend state reads and writes nothing", async () => {
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) =>
      cmd === "get_settings_window_context"
        ? Promise.resolve({ projectName: "", projectKey: "", contentRoot: "" })
        : base(cmd, args),
    );
    render(<SettingsWindowApp initialSection="global/appearance" />);
    await waitFor(() =>
      expect(invokeMock.mock.calls.some(([cmd]) => cmd === "get_settings_window_context")).toBe(true),
    );
    await screen.findByRole("navigation");
    expect(invokeMock.mock.calls.filter(([cmd]) => String(cmd).includes("layout"))).toEqual([]);
  });
});

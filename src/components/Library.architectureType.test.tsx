/**
 * The Architecture type in the Project panel (`LIB-library.md` LIB-FR-04,
 * LIB-FR-08, LIB-FR-19): a `*.calm.json` file is tagged with its own chip, and
 * the lens offers the type when the tree holds one.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
// No log flush timer may outlive its test.
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

import {
  expandedAll,
  file,
  folder,
  makeTreeStubs,
  renderLibrary,
} from "../test/libraryFixtures";
import { resetPanelReveals } from "../state/panelReveal";

const { mockInvoke } = makeTreeStubs(invokeMock);

function architectureTree() {
  return folder("", true, [
    folder("models", true, [
      file("models/system.calm.json", "architecture", "inferred"),
    ]),
    file("review.flow", "flow", "inferred"),
    file("CLAUDE.md", "instructions", "inferred"),
  ]);
}

beforeEach(() => {
  resetPanelReveals();
  invokeMock.mockReset();
});
afterEach(cleanup);

describe("the Architecture type in the Project panel", () => {
  it("LIB-FR-08, ASC-FR-QVCN: a calm.json file carries the ARC tag", async () => {
    const tree = architectureTree();
    mockInvoke({ load_project_tree: () => tree }, expandedAll(tree));
    renderLibrary();

    const row = (await screen.findByText("system.calm.json")).closest(
      ".tree-row",
    ) as HTMLElement;
    expect(within(row).getByText("ARC")).toHaveAttribute("data-type", "architecture");
  });

  it("LIB-FR-04, LIB-FR-19: the lens offers Architecture between Flow and Instructions and narrows to it", async () => {
    const tree = architectureTree();
    mockInvoke({ load_project_tree: () => tree }, expandedAll(tree));
    renderLibrary();

    await screen.findByText("system.calm.json");
    const radios = screen
      .getAllByRole("radio")
      .map((r) => r.getAttribute("aria-label") ?? r.textContent);
    const flow = radios.findIndex((r) => r?.includes("Flow"));
    expect(radios[flow + 1]).toContain("Architecture");
    expect(radios[flow + 2]).toContain("Instructions");

    fireEvent.click(screen.getByRole("radio", { name: "Architecture" }));
    await waitFor(() => expect(screen.queryByText("review.flow")).not.toBeInTheDocument());
    expect(screen.getByText("system.calm.json")).toBeInTheDocument();
    expect(screen.queryByText("CLAUDE.md")).not.toBeInTheDocument();
  });
});

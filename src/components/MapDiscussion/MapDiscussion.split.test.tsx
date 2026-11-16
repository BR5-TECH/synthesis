/**
 * The split of the Map tab between the canvas side and the Discussion column
 * (`SMP-specification-map.md` SMP-FR-SWCL, SMP-FR-VDND): the presets of the
 * toolbar, the accelerator, and the splitter of the draft column
 * (`DDS-draft-discussion.md` DDS-FR-PNXR).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
// No log flush timer may outlive its test.
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

import { clearAllDiscussionSessions } from "../../state/discussionSession";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import { resetAgentRegistry } from "../../state/agentRegistry";
import { resetProjectIdentity } from "../../state/projectIdentity";
import { SpecMapSessionStore } from "../../state/specMap/session";
import { CONTEXT_ESTIMATE, otherCommand } from "../../test/invokeMocks";
import { HUMAN } from "../../test/mapPlanFixtures";
import { DiscussionPane, DiscussionToggle } from ".";

let store: SpecMapSessionStore;

function wire() {
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "list_map_discussions":
      case "list_project_agents":
      case "list_ai_api_catalogs":
      case "list_agent_turns":
        return Promise.resolve([]);
      case "find_map_discussion":
        return Promise.resolve(null);
      case "resolve_comment_author_identity":
        return Promise.resolve(HUMAN);
      case "get_active_ai_api_catalog":
        return Promise.resolve({ resolution: "none_configured", catalog: null });
      case "estimate_discussion_context":
        return Promise.resolve(CONTEXT_ESTIMATE);
      default:
        return otherCommand(cmd);
    }
  });
}

function renderPane() {
  // The body of a lens is 1000px wide and starts at the left edge.
  const body = document.createElement("div");
  document.body.appendChild(body);
  vi.spyOn(body, "getBoundingClientRect").mockReturnValue({
    left: 0,
    width: 1000,
    top: 0,
    height: 600,
    right: 1000,
    bottom: 600,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  });
  return render(<DiscussionPane store={store} selected={null} planTool planHost={null} />, {
    container: body,
  });
}

const cycle = () => fireEvent.keyDown(window, { key: "\\", metaKey: true });

beforeEach(() => {
  invokeMock.mockReset();
  wire();
  store = new SpecMapSessionStore();
  resetAgentRegistry();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetProjectIdentity();
});

afterEach(() => {
  cleanup();
  document.body.innerHTML = "";
});

describe("the split (SMP-FR-SWCL)", () => {
  it("SMP-FR-SWCL: the column opens at canvas 70, and the splitter names the share of the map", async () => {
    renderPane();
    expect(await screen.findByTestId("map-discussion")).toHaveStyle({ width: "30%" });
    const splitter = screen.getByTestId("map-discussion-splitter");
    expect(splitter).toHaveAttribute("role", "separator");
    expect(splitter).toHaveAttribute("aria-valuenow", "70");
    expect(splitter).toHaveAttribute("aria-valuemin", "25");
    expect(splitter).toHaveAttribute("aria-valuemax", "75");
    expect(splitter).toHaveAttribute("aria-valuetext", "Map 70 percent");
  });

  it("SMP-FR-SWCL: the accelerator cycles canvas 70, even, and discussion 70", async () => {
    renderPane();
    const pane = await screen.findByTestId("map-discussion");
    act(cycle);
    expect(pane).toHaveStyle({ width: "50%" });
    act(cycle);
    expect(pane).toHaveStyle({ width: "70%" });
    act(cycle);
    expect(pane).toHaveStyle({ width: "30%" });
    // Ctrl takes the place of Cmd, and Alt makes it another key.
    act(() => fireEvent.keyDown(window, { key: "\\", ctrlKey: true }));
    expect(store.snapshot().discussion.ratio).toBe("even");
    act(() => fireEvent.keyDown(window, { key: "\\", ctrlKey: true, altKey: true }));
    expect(store.snapshot().discussion.ratio).toBe("even");
  });

  it("SMP-FR-SWCL: the accelerator does nothing while the column is hidden or the tab is gone", async () => {
    const view = renderPane();
    await screen.findByTestId("map-discussion");
    act(() => store.setDiscussionVisible(false));
    act(cycle);
    expect(store.snapshot().discussion.ratio).toBe("doc70");
    act(() => store.setDiscussionVisible(true));
    view.unmount();
    act(cycle);
    expect(store.snapshot().discussion.ratio).toBe("doc70");
  });

  it("SMP-FR-SWCL: the splitter moves by keyboard within the limits of the draft column", async () => {
    renderPane();
    const splitter = await screen.findByTestId("map-discussion-splitter");
    splitter.focus();
    const user = userEvent.setup();
    await user.keyboard("{ArrowLeft}");
    expect(store.snapshot().discussion.ratio).toBeCloseTo(0.68);
    expect(screen.getByTestId("map-discussion")).toHaveStyle({ width: "32%" });
    await user.keyboard("{Home}");
    expect(store.snapshot().discussion.ratio).toBe(0.25);
    await user.keyboard("{End}");
    expect(store.snapshot().discussion.ratio).toBe(0.75);
    expect(screen.getByTestId("map-discussion")).toHaveStyle({ width: "25%" });
  });

  it("SMP-FR-SWCL: a drag sets the split from the place of the pointer in the body of the lens", async () => {
    renderPane();
    const splitter = await screen.findByTestId("map-discussion-splitter");
    fireEvent.pointerDown(splitter, { clientX: 700 });
    act(() => {
      window.dispatchEvent(new MouseEvent("pointermove", { clientX: 600 }));
    });
    expect(store.snapshot().discussion.ratio).toBe(0.6);
    expect(screen.getByTestId("map-discussion")).toHaveStyle({ width: "40%" });
    act(() => {
      window.dispatchEvent(new MouseEvent("pointermove", { clientX: 50 }));
    });
    expect(store.snapshot().discussion.ratio).toBe(0.25);
    act(() => {
      window.dispatchEvent(new MouseEvent("pointerup"));
    });
    act(() => {
      window.dispatchEvent(new MouseEvent("pointermove", { clientX: 600 }));
    });
    expect(store.snapshot().discussion.ratio).toBe(0.25);
  });
});

describe("the presets in the toolbar (SMP-FR-VDND)", () => {
  it("SMP-FR-VDND: the presets and the accelerator hint lead the toggle, and choosing a preset sets the split", async () => {
    render(<DiscussionToggle store={store} labelled />);
    const split = screen.getByTestId("map-discussion-split");
    const toggle = screen.getByRole("button", { name: "Discussion" });
    expect(split.compareDocumentPosition(toggle) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(split).toHaveTextContent("⌘\\");
    const user = userEvent.setup();
    expect(screen.getByRole("radio", { name: "Document" })).toHaveAttribute("aria-checked", "true");
    await user.click(screen.getByRole("radio", { name: "Discussion" }));
    expect(store.snapshot().discussion.ratio).toBe("chat70");
  });

  it("SMP-FR-VDND: the presets stand down while the column is hidden", async () => {
    render(<DiscussionToggle store={store} labelled />);
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Discussion" }));
    expect(screen.queryByTestId("map-discussion-split")).toBeNull();
    expect(screen.queryByRole("radiogroup")).toBeNull();
    await user.click(screen.getByRole("button", { name: "Discussion" }));
    expect(screen.getByTestId("map-discussion-split")).toBeInTheDocument();
  });

  it("SMP-FR-VDND: a narrow tab hides the hint, and a narrower one the presets, and the toggle stays", async () => {
    const view = render(<DiscussionToggle store={store} labelled presets hint={false} />);
    expect(screen.getByRole("radiogroup")).toBeInTheDocument();
    expect(screen.queryByText("⌘\\")).toBeNull();
    view.rerender(<DiscussionToggle store={store} labelled presets={false} hint={false} />);
    expect(screen.queryByTestId("map-discussion-split")).toBeNull();
    expect(screen.getByRole("button", { name: "Discussion" })).toBeInTheDocument();
  });
});

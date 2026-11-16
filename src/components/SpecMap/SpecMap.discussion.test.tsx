/**
 * The Discussion column and the **Discuss** entry on the Specifications lens
 * (`SMP-specification-map.md` SMP-FR-WNLR, SMP-FR-SWCL, SMP-FR-DUUU;
 * `SMN-specification-map-nodes.md` SMN-FR-JHQR, SMN-FR-BSZV;
 * `LCM-library-context-menu.md` LCM-FR-07).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

import { SpecMap } from ".";
import { clearAllDiscussionSessions } from "../../state/discussionSession";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import { resetAgentRegistry } from "../../state/agentRegistry";
import { resetProjectIdentity } from "../../state/projectIdentity";
import { CONTEXT_ESTIMATE, otherCommand } from "../../test/invokeMocks";
import { HUMAN } from "../../test/mapPlanFixtures";
import { openspecIndex, smallIndex } from "../../test/specMapFixtures";
import { NO_DELIVERY_SOURCE, makeStore, mockCanvasRect, nodeButton, setView, setWindowWidth } from "../../test/specMapRender";
import { SpecMapSessionStore } from "../../state/specMap/session";

function wire() {
  invokeMock.mockImplementation((cmd: string) => {
    switch (cmd) {
      case "list_map_discussions":
        return Promise.resolve([]);
      case "find_map_discussion":
        return Promise.resolve(null);
      case "list_project_agents":
        return Promise.resolve([]);
      case "resolve_comment_author_identity":
        return Promise.resolve(HUMAN);
      case "get_active_ai_api_catalog":
        return Promise.resolve({ resolution: "none_configured", catalog: null });
      case "estimate_discussion_context":
        return Promise.resolve(CONTEXT_ESTIMATE);
      case "list_ai_api_catalogs":
      case "list_agent_turns":
        return Promise.resolve([]);
      default:
        return otherCommand(cmd);
    }
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  wire();
  resetAgentRegistry();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetProjectIdentity();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

/** Renders the tab with the column shown, as a first open of a map session does. */
async function renderShown(options: { width?: number; windowWidth?: number; index?: Parameters<typeof makeStore>[0] } = {}) {
  setWindowWidth(options.windowWidth ?? 1600);
  mockCanvasRect({ width: options.width ?? 1400, height: 800 });
  const { store } = makeStore(options.index);
  store.setDiscussionVisible(true);
  render(<SpecMap store={store} onOpenArtifact={vi.fn()} onOverlayOpening={vi.fn()} />);
  await screen.findByTestId("spec-map-canvas");
  await waitFor(() => expect(store.snapshot().status).toBe("ready"));
  return store;
}

const menu = () => screen.queryByRole("menu", { name: "Node actions" });

describe("the column in the tab (SMP-FR-WNLR, SMP-FR-SWCL)", () => {
  it("SMP-FR-WNLR: the Discussion column stands on the right of the canvas, and the toolbar toggle shows and hides it", async () => {
    await renderShown();
    expect(await screen.findByTestId("map-discussion-column")).toBeInTheDocument();
    const toggle = screen.getByRole("button", { name: "Discussion" });
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    const body = document.querySelector(".smap-body") as HTMLElement;
    expect(body.lastElementChild).toBe(screen.getByTestId("map-discussion"));
    fireEvent.click(toggle);
    expect(screen.queryByTestId("map-discussion-column")).toBeNull();
    expect(toggle).toHaveAttribute("aria-pressed", "false");
  });

  it("SMP-FR-WNLR: a session opens with the column shown", async () => {
    setWindowWidth(1600);
    mockCanvasRect({ width: 1400, height: 800 });
    const store = new SpecMapSessionStore({
      load: vi.fn(async () => smallIndex()),
      getDeliverySource: vi.fn(async () => NO_DELIVERY_SOURCE),
    });
    render(<SpecMap store={store} onOpenArtifact={vi.fn()} />);
    expect(await screen.findByTestId("map-discussion-column")).toBeInTheDocument();
  });

  it("SMP-FR-SWCL: the inspector stays with the column from 900px of tab width", async () => {
    const store = await renderShown({ width: 900 });
    act(() => store.select({ kind: "index", id: "g1" }));
    expect(screen.getByRole("complementary", { name: "Inspector" })).toBeInTheDocument();
    expect(screen.getByTestId("map-discussion-column")).toBeInTheDocument();
  });

  it("SMP-FR-SWCL: below 900px of tab width the column replaces the inspector while it is shown", async () => {
    const store = await renderShown({ width: 899 });
    act(() => store.select({ kind: "index", id: "g1" }));
    expect(screen.queryByRole("complementary", { name: "Inspector" })).toBeNull();
    expect(screen.getByTestId("map-discussion-column")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "inspector" })).toBeNull();
    act(() => store.setDiscussionVisible(false));
    expect(screen.getByRole("complementary", { name: "Inspector" })).toBeInTheDocument();
  });

  it("SMP-FR-SWCL: the canvas takes the width that the session holds for the column", async () => {
    const store = await renderShown();
    act(() => store.setDiscussionWidth(500));
    expect(screen.getByTestId("map-discussion")).toHaveStyle({ width: "500px" });
  });

  it("SMP-FR-DUUU: the node selected on the Specifications canvas is the node that Use selected node takes", async () => {
    const store = await renderShown();
    act(() => store.select({ kind: "index", id: "g1" }));
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Use selected node" }));
    expect(screen.getByTestId("map-focus-chip")).toHaveTextContent("g1 label");
  });
});

async function levelFor(store: SpecMapSessionStore, level: number) {
  setView(store, { level });
}

describe("the Discuss entry of a node (SMN-FR-JHQR, SMN-FR-BSZV, LCM-FR-07)", () => {
  it("SMN-FR-JHQR: a right-click on a root card opens a menu that holds a Discuss entry with a leading icon", async () => {
    await renderShown();
    fireEvent.contextMenu(nodeButton("d1 label, domain"), { clientX: 20, clientY: 20 });
    const entry = within(screen.getByRole("menu", { name: "Node actions" })).getByRole("menuitem", { name: "Discuss" });
    // LCM-FR-07: the entry carries a leading icon.
    expect(entry.firstElementChild?.tagName.toLowerCase()).toBe("svg");
  });

  it("SMN-FR-JHQR: a branch box, a leaf-group box, and a spec chip have the menu too", async () => {
    const store = await renderShown();
    await levelFor(store, 1);
    fireEvent.contextMenu(nodeButton("f1 label, feature"), { clientX: 20, clientY: 20 });
    expect(menu()).not.toBeNull();
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    await levelFor(store, 2);
    fireEvent.contextMenu(nodeButton("g1 label, group"), { clientX: 20, clientY: 20 });
    expect(menu()).not.toBeNull();
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    await levelFor(store, 3);
    fireEvent.contextMenu(nodeButton("A1 a1 label"), { clientX: 20, clientY: 20 });
    expect(menu()).not.toBeNull();
  });

  it("SMN-FR-JHQR: the context-menu key and Shift+F10 on a focused node open the menu, and Escape returns the focus to the node", async () => {
    await renderShown();
    const root = nodeButton("d1 label, domain");
    root.focus();
    fireEvent.keyDown(root, { key: "ContextMenu" });
    expect(menu()).not.toBeNull();
    fireEvent.keyDown(screen.getByRole("menu"), { key: "Escape" });
    expect(menu()).toBeNull();
    expect(root).toHaveFocus();
    fireEvent.keyDown(root, { key: "F10", shiftKey: true });
    expect(menu()).not.toBeNull();
  });

  it("SMN-FR-JHQR: a change chip has no menu", async () => {
    const store = await renderShown({ index: openspecIndex() });
    await levelFor(store, 3);
    fireEvent.contextMenu(nodeButton("change add-logout, active worktree, on Authentication"), { clientX: 20, clientY: 20 });
    expect(menu()).toBeNull();
  });

  it("SMN-FR-BSZV: Discuss hands the node reference of a spec chip to the column, closes the menu, and focuses the composer", async () => {
    const store = await renderShown();
    await levelFor(store, 3);
    fireEvent.contextMenu(nodeButton("A1 a1 label"), { clientX: 20, clientY: 20 });
    await userEvent.setup().click(screen.getByRole("menuitem", { name: "Discuss" }));
    expect(menu()).toBeNull();
    expect(store.snapshot().discussion.node).toEqual({
      lens: "specifications",
      kind: "spec",
      id: "A1",
      label: "a1 label",
    });
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Discuss this node" })).toHaveFocus());
  });

  it("SMN-FR-BSZV: Discuss on a root card hands a node of the kind index, and shows the column where it was hidden", async () => {
    const store = await renderShown();
    act(() => store.setDiscussionVisible(false));
    fireEvent.contextMenu(nodeButton("d1 label, domain"), { clientX: 20, clientY: 20 });
    await userEvent.setup().click(screen.getByRole("menuitem", { name: "Discuss" }));
    expect(store.snapshot().discussion).toMatchObject({
      visible: true,
      node: { lens: "specifications", kind: "index", id: "d1" },
    });
    expect(await screen.findByTestId("map-discussion-column")).toBeInTheDocument();
  });
});

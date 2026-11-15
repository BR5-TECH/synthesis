/**
 * The Discussion column of the Map tab (`SMP-specification-map.md`
 * SMP-FR-WNLR .. SMP-FR-BOBB, `CMT-comments.md` CMT-FR-LVNQ). The backend is
 * mocked at `invoke`, so every assertion is about what the column renders and
 * which operation it sends.
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
import { resetToasts } from "../../state/toasts";
import { sameMapNode } from "../../state/specMap/discussion";
import { SpecMapSessionStore } from "../../state/specMap/session";
import { CONTEXT_ESTIMATE, otherCommand } from "../../test/invokeMocks";
import { expectBackendOrigin } from "../../test/origins";
import {
  AGENT,
  HUMAN,
  ITEM_NODE,
  SPEC_NODE,
  mapComment,
  mapThread,
  proposal,
  createRow,
} from "../../test/mapPlanFixtures";
import type { Discussion, MapNodeRef } from "../../types";
import { DiscussionPane, DiscussionToggle, type MapColumnProps } from ".";
import { MapDiscussionColumn } from "./Column";

interface Backend {
  threads: Discussion[];
  calls: { cmd: string; args: unknown }[];
  /** Answers `get_or_create_map_discussion` with a thread another window made. */
  racing?: Discussion;
}

function nodeOf(args: unknown): MapNodeRef | null {
  return (args as { mapNode?: MapNodeRef | null }).mapNode ?? null;
}

function wire(b: Backend) {
  invokeMock.mockImplementation((cmd: string, args: unknown) => {
    b.calls.push({ cmd, args });
    switch (cmd) {
      case "list_map_discussions":
        return Promise.resolve([...b.threads]);
      case "find_map_discussion":
        return Promise.resolve(b.threads.find((t) => sameMapNode(t.mapNode ?? null, nodeOf(args))) ?? null);
      case "get_or_create_map_discussion": {
        const a = args as { body: string; attachments: unknown[] };
        const held =
          b.racing ?? b.threads.find((t) => sameMapNode(t.mapNode ?? null, nodeOf(args)));
        if (held) {
          if (!b.threads.includes(held)) b.threads.push(held);
          return Promise.resolve(held);
        }
        const made = mapThread(`t${b.threads.length + 1}`, [], nodeOf(args));
        made.comments = [
          mapComment(`${made.id}-c0`, HUMAN, a.body, { attachments: a.attachments as never }),
        ];
        b.threads.push(made);
        return Promise.resolve(made);
      }
      case "add_comment": {
        const a = args as { discussionId: string; body: string; attachments: unknown[] };
        const thread = b.threads.find((t) => t.id === a.discussionId) as Discussion;
        thread.comments = [
          ...thread.comments,
          mapComment(`${thread.id}-c${thread.comments.length}`, HUMAN, a.body, {
            attachments: a.attachments as never,
          }),
        ];
        return Promise.resolve(thread);
      }
      case "list_project_agents":
        return Promise.resolve([{ agent: { id: "a1", nickname: "arch" }, availability: "ready" }]);
      case "resolve_comment_author_identity":
        return Promise.resolve(HUMAN);
      case "get_active_ai_api_catalog":
        return Promise.resolve({ resolution: "none_configured", catalog: null });
      case "estimate_discussion_context":
        return Promise.resolve(CONTEXT_ESTIMATE);
      case "list_ai_api_catalogs":
      case "list_agent_turns":
        return Promise.resolve([]);
      case "list_map_plan_proposals":
        return Promise.resolve([proposal([createRow("r1", "Payout schedule")])]);
      case "dispatch_agent_turn":
        return Promise.resolve({
          id: "turn-1",
          agentId: "a1",
          nickname: "arch",
          origin: expectBackendOrigin((args as { origin: unknown }).origin),
          triggerCommentId: "c1",
          state: "running",
          failure: null,
          startedAt: "2026-01-01T00:00:00Z",
          endedAt: null,
        });
      default:
        return otherCommand(cmd);
    }
  });
}

const callsOf = (b: Backend, cmd: string) =>
  b.calls.filter((c) => c.cmd === cmd).map((c) => c.args);

let store: SpecMapSessionStore;

function backend(threads: Discussion[] = []): Backend {
  const b: Backend = { threads, calls: [] };
  wire(b);
  return b;
}

function Column(props: Partial<MapColumnProps> = {}) {
  return (
    <MapDiscussionColumn
      store={store}
      selected={null}
      planTool
      planHost={null}
      {...props}
    />
  );
}

async function renderColumn(props: Partial<MapColumnProps> = {}) {
  render(<Column {...props} />);
  await screen.findByTestId("map-discussion-column");
}

const composer = (name: RegExp | string) => screen.findByRole("textbox", { name });

beforeEach(() => {
  invokeMock.mockReset();
  store = new SpecMapSessionStore();
  resetAgentRegistry();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetProjectIdentity();
  resetToasts();
});

afterEach(cleanup);

describe("the column (SMP-FR-WNLR, CMT-FR-LVNQ)", () => {
  it("SMP-FR-WNLR: shows one thread with a header, the transcript, and a composer", async () => {
    backend([mapThread("t1", ["What is left of the plan?"])]);
    await renderColumn();
    const column = screen.getByTestId("map-discussion-column");
    expect(within(column).getByRole("combobox", { name: "Thread" })).toBeInTheDocument();
    expect(await within(column).findByTestId("comment-thread-t1")).toHaveTextContent("What is left of the plan?");
    expect(await composer("Reply to thread t1")).toBeInTheDocument();
  });

  it("SMP-FR-WNLR: the pane draws the column and a splitter while it is shown, and nothing while it is hidden", async () => {
    backend();
    render(<DiscussionPane store={store} selected={null} planTool planHost={null} />);
    expect(await screen.findByTestId("map-discussion")).toBeInTheDocument();
    expect(screen.getByRole("separator", { name: "Resize the discussion column" })).toBeInTheDocument();
    act(() => store.setDiscussionVisible(false));
    expect(screen.queryByTestId("map-discussion")).toBeNull();
    expect(screen.queryByRole("separator")).toBeNull();
  });

  it("SMP-FR-WNLR: the Discussion toggle in the toolbar shows and hides the column", async () => {
    backend();
    render(<DiscussionToggle store={store} labelled />);
    const toggle = screen.getByRole("button", { name: "Discussion" });
    expect(toggle).toHaveAttribute("aria-pressed", "true");
    const user = userEvent.setup();
    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-pressed", "false");
    expect(store.snapshot().discussion.visible).toBe(false);
    await user.click(toggle);
    expect(store.snapshot().discussion.visible).toBe(true);
  });

  it("CMT-FR-LVNQ: a Map thread renders in the column, and a node thread names its node in the header", async () => {
    backend([mapThread("t2", ["About this spec"], SPEC_NODE)]);
    store.setDiscussionNode(SPEC_NODE);
    await renderColumn();
    expect(await screen.findByTestId("comment-thread-t2")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Specification map/ })).toBeInTheDocument();
  });

  it("CMT-FR-LVNQ: the composer can attach a focus", async () => {
    backend([mapThread("t1", ["hello"])]);
    await renderColumn({ selected: SPEC_NODE });
    await screen.findByTestId("comment-thread-t1");
    expect(screen.getByRole("group", { name: "Focus" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use selected node" })).toBeEnabled();
  });
});

describe("the thread chooser (SMP-FR-EIUP)", () => {
  it("SMP-FR-EIUP: lists the Map thread and every node thread, each with its node label", async () => {
    backend([mapThread("t1", ["a"]), mapThread("t2", ["b"], SPEC_NODE), mapThread("t3", ["c"], ITEM_NODE)]);
    await renderColumn();
    const select = await screen.findByRole("combobox", { name: "Thread" });
    await waitFor(() => expect(within(select).getAllByRole("option")).toHaveLength(3));
    expect(within(select).getAllByRole("option").map((o) => o.textContent)).toEqual([
      "Map",
      "Specification map",
      "Decline message",
    ]);
  });

  it("SMP-FR-EIUP: choosing a thread shows it", async () => {
    backend([mapThread("t1", ["on the map"]), mapThread("t2", ["on the node"], SPEC_NODE)]);
    await renderColumn();
    await screen.findByTestId("comment-thread-t1");
    const user = userEvent.setup();
    const select = screen.getByRole("combobox", { name: "Thread" });
    await waitFor(() => expect(within(select).getAllByRole("option")).toHaveLength(2));
    await user.selectOptions(select, "Specification map");
    expect(store.snapshot().discussion.node).toEqual(SPEC_NODE);
  });

  it("SMP-FR-EIUP: the header names the node of a node thread, and selecting that name selects the node on the canvas", async () => {
    backend([mapThread("t2", ["b"], SPEC_NODE)]);
    store.setDiscussionNode(SPEC_NODE);
    const show = vi.spyOn(store, "showMapNode");
    await renderColumn();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /Specification map/ }));
    expect(show).toHaveBeenCalledWith(SPEC_NODE);
  });

  it("SMP-FR-EIUP: the Map thread has no node name in the header", async () => {
    backend([mapThread("t1", ["a"])]);
    await renderColumn();
    await screen.findByTestId("comment-thread-t1");
    expect(screen.queryByTitle("Select this node on the canvas")).toBeNull();
  });
});

describe("the Focus chip (SMP-FR-DUUU, SMP-FR-BOBB)", () => {
  it("SMP-FR-DUUU: Use selected node takes the selected node of the active lens as the focus, and the clear control drops it", async () => {
    backend([mapThread("t1", ["hello"])]);
    await renderColumn({ selected: ITEM_NODE });
    await screen.findByTestId("comment-thread-t1");
    const user = userEvent.setup();
    expect(screen.getByText("The whole map")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Use selected node" }));
    expect(screen.getByTestId("map-focus-chip")).toHaveTextContent("Decline message");
    expect(screen.getByRole("button", { name: "Use selected node" })).toBeDisabled();
    await user.click(screen.getByRole("button", { name: "Clear the focus" }));
    expect(screen.queryByTestId("map-focus-chip")).toBeNull();
  });

  it("SMP-FR-DUUU: Use selected node is off, with its reason, where nothing is selected", async () => {
    backend([mapThread("t1", ["hello"])]);
    await renderColumn({ selected: null });
    await screen.findByTestId("comment-thread-t1");
    const use = screen.getByRole("button", { name: "Use selected node" });
    expect(use).toBeDisabled();
    expect(use).toHaveAttribute("title", "Select a node on the canvas first");
  });

  it("SMP-FR-DUUU: a message with a focus sends one map_node attachment", async () => {
    const b = backend([mapThread("t1", ["hello"])]);
    await renderColumn({ selected: ITEM_NODE });
    const user = userEvent.setup();
    await screen.findByTestId("comment-thread-t1");
    await user.click(screen.getByRole("button", { name: "Use selected node" }));
    await user.type(await composer("Reply to thread t1"), "what about this?");
    await user.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(callsOf(b, "add_comment")).toHaveLength(1));
    const sent = callsOf(b, "add_comment")[0] as { attachments: { kind: string; node: MapNodeRef }[] };
    expect(sent.attachments.filter((a) => a.kind === "mapNode")).toEqual([{ kind: "mapNode", node: ITEM_NODE }]);
  });

  it("SMP-FR-DUUU: a message with no focus sends no map_node attachment", async () => {
    const b = backend([mapThread("t1", ["hello"])]);
    await renderColumn();
    const user = userEvent.setup();
    await user.type(await composer("Reply to thread t1"), "about the whole map");
    await user.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(callsOf(b, "add_comment")).toHaveLength(1));
    expect((callsOf(b, "add_comment")[0] as { attachments: unknown[] }).attachments).toEqual([]);
  });

  it("SMP-FR-DUUU: in a node thread the node of the thread is the default focus, and it returns there after a post", async () => {
    const b = backend([mapThread("t2", ["b"], SPEC_NODE)]);
    store.setDiscussionNode(SPEC_NODE);
    await renderColumn();
    await screen.findByTestId("comment-thread-t2");
    expect(screen.getByTestId("map-focus-chip")).toHaveTextContent("Specification map");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "Clear the focus" }));
    expect(screen.queryByTestId("map-focus-chip")).toBeNull();
    await user.type(await composer("Reply to thread t2"), "one more");
    await user.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(callsOf(b, "add_comment")).toHaveLength(1));
    await waitFor(() => expect(screen.getByTestId("map-focus-chip")).toHaveTextContent("Specification map"));
  });

  it("SMP-FR-DUUU: the first message of an empty node thread carries the node of the thread as its focus", async () => {
    const b = backend([]);
    store.discuss(ITEM_NODE);
    await renderColumn();
    const user = userEvent.setup();
    await waitFor(() => expect(screen.getByTestId("map-focus-chip")).toHaveTextContent("Decline message"));
    await user.type(await composer("Discuss this node"), "first");
    await user.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(callsOf(b, "get_or_create_map_discussion")).toHaveLength(1));
    const sent = callsOf(b, "get_or_create_map_discussion")[0] as { attachments: unknown[] };
    expect(sent.attachments).toEqual([{ kind: "mapNode", node: ITEM_NODE }]);
  });

  it("SMP-FR-BOBB: a message that addresses an agent dispatches the turn of that agent for the Map thread", async () => {
    const b = backend([mapThread("t1", ["hello"])]);
    await renderColumn({ selected: ITEM_NODE });
    const user = userEvent.setup();
    await screen.findByTestId("comment-thread-t1");
    await user.click(screen.getByRole("button", { name: "Use selected node" }));
    await user.type(await composer("Reply to thread t1"), "@arch please review");
    await user.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(callsOf(b, "dispatch_agent_turn")).toHaveLength(1));
    expect(callsOf(b, "dispatch_agent_turn")[0]).toMatchObject({
      nickname: "arch",
      origin: { discussionId: "t1", target: { kind: "map" }, fragmentTarget: null },
    });
  });
});

describe("Discuss and the first message (SMP-FR-IHFM, SMP-FR-KHKC)", () => {
  it("SMP-FR-IHFM: Discuss on a node that has a thread shows that thread, and creates none", async () => {
    const b = backend([mapThread("t2", ["about the node"], SPEC_NODE)]);
    store.setDiscussionVisible(false);
    store.discuss(SPEC_NODE);
    await renderColumn();
    expect(await screen.findByTestId("comment-thread-t2")).toHaveTextContent("about the node");
    expect(callsOf(b, "find_map_discussion")[0]).toEqual({ mapNode: SPEC_NODE });
    expect(callsOf(b, "get_or_create_map_discussion")).toHaveLength(0);
  });

  it("SMP-FR-IHFM: where none exists the column shows an empty thread for the node with the composer focused, and creates none", async () => {
    const b = backend([]);
    store.discuss(ITEM_NODE);
    await renderColumn();
    expect(await screen.findByTestId("map-discussion-empty")).toHaveTextContent("No one has discussed this node yet");
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Discuss this node" })).toHaveFocus());
    expect(callsOf(b, "find_map_discussion")).toHaveLength(1);
    expect(callsOf(b, "get_or_create_map_discussion")).toHaveLength(0);
    expect(callsOf(b, "open_discussion")).toHaveLength(0);
  });

  it("SMP-FR-IHFM: a second Discuss puts the caret in the composer again", async () => {
    backend([]);
    store.discuss(ITEM_NODE);
    await renderColumn();
    const field = await screen.findByRole("textbox", { name: "Discuss this node" });
    await waitFor(() => expect(field).toHaveFocus());
    field.blur();
    act(() => store.discuss(ITEM_NODE));
    await waitFor(() => expect(field).toHaveFocus());
  });

  it("SMP-FR-KHKC: the first message of an empty node thread creates it with the node, and later messages are added to it", async () => {
    const b = backend([]);
    store.discuss(ITEM_NODE);
    await renderColumn();
    const user = userEvent.setup();
    await user.type(await composer("Discuss this node"), "first");
    await user.click(screen.getByRole("button", { name: "Post" }));
    expect(await screen.findByTestId("comment-thread-t1")).toHaveTextContent("first");
    expect(callsOf(b, "get_or_create_map_discussion")).toHaveLength(1);
    expect(callsOf(b, "get_or_create_map_discussion")[0]).toMatchObject({ mapNode: ITEM_NODE, body: "first" });
    await user.type(await composer("Reply to thread t1"), "second");
    await user.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(callsOf(b, "add_comment")).toHaveLength(1));
    expect(callsOf(b, "get_or_create_map_discussion")).toHaveLength(1);
  });

  it("SMP-FR-KHKC: Discuss on that node afterwards opens the same thread, and the chooser holds it once", async () => {
    const b = backend([]);
    store.discuss(ITEM_NODE);
    await renderColumn();
    const user = userEvent.setup();
    await user.type(await composer("Discuss this node"), "first");
    await user.click(screen.getByRole("button", { name: "Post" }));
    await screen.findByTestId("comment-thread-t1");
    act(() => store.setDiscussionNode(null));
    act(() => store.discuss(ITEM_NODE));
    expect(await screen.findByTestId("comment-thread-t1")).toBeInTheDocument();
    const select = screen.getByRole("combobox", { name: "Thread" });
    expect(within(select).getAllByRole("option").filter((o) => o.textContent === "Decline message")).toHaveLength(1);
    expect(b.threads).toHaveLength(1);
  });

  it("SMP-FR-KHKC: a thread for the same node found by another window is shown, not duplicated", async () => {
    const other = mapThread("t-other", ["from another window"], ITEM_NODE);
    const b = backend([]);
    b.racing = other;
    store.discuss(ITEM_NODE);
    await renderColumn();
    const user = userEvent.setup();
    await user.type(await composer("Discuss this node"), "mine");
    await user.click(screen.getByRole("button", { name: "Post" }));
    expect(await screen.findByTestId("comment-thread-t-other")).toHaveTextContent("from another window");
    expect(b.threads).toHaveLength(1);
  });
});

describe("the empty Map thread (SMP-FR-XGBI)", () => {
  it("SMP-FR-XGBI: tells the author that the agent reads the plan and proposes changes only while a GitHub Project is the delivery source", async () => {
    backend();
    await renderColumn({ planTool: true });
    const note = await screen.findByTestId("map-plan-tool-note");
    expect(note).toHaveTextContent("The agent reads the plan and proposes changes only while a GitHub Project is the delivery source.");
    expect(note).not.toHaveTextContent("has no plan tool");
  });

  it("SMP-FR-XGBI: with another delivery source the column says that the agent has no plan tool", async () => {
    backend();
    await renderColumn({ planTool: false });
    expect(await screen.findByTestId("map-plan-tool-note")).toHaveTextContent("the agent has no plan tool");
  });
});

describe("the transcript (SMP-FR-PPJB)", () => {
  it("SMP-FR-PPJB: an agent comment that carries a map_plan_proposal attachment renders the review card", async () => {
    const thread = mapThread("t1", ["plan it"]);
    thread.comments.push(
      mapComment("t1-c1", AGENT, "I propose one step.", {
        attachments: [{ kind: "mapPlanProposal", proposalId: "p1" }],
      }),
    );
    const b = backend([thread]);
    await renderColumn();
    const card = await screen.findByTestId("map-plan-proposal");
    expect(card).toHaveTextContent("Plan proposal");
    expect(await within(card).findByText("Payout schedule")).toBeInTheDocument();
    expect(callsOf(b, "list_map_plan_proposals")[0]).toEqual({ discussionId: "t1" });
  });

  it("SMP-FR-PPJB: a human comment that carries a focus shows it as a chip", async () => {
    const thread = mapThread("t1", []);
    thread.comments = [mapComment("t1-c0", HUMAN, "look here", { attachments: [{ kind: "mapNode", node: ITEM_NODE }] })];
    backend([thread]);
    await renderColumn();
    expect(await screen.findByTestId("comment-attachment-map-node")).toHaveTextContent("Decline message");
  });

  it("MPV-FR-FESO: the composer stays usable while a card has a row that waits", async () => {
    const thread = mapThread("t1", ["plan it"]);
    thread.comments.push(
      mapComment("t1-c1", AGENT, "I propose one step.", {
        attachments: [{ kind: "mapPlanProposal", proposalId: "p1" }],
      }),
    );
    backend([thread]);
    await renderColumn();
    await screen.findByTestId("map-plan-proposal");
    expect(await composer("Reply to thread t1")).toBeEnabled();
  });
});

describe("the session of the column (SMP-FR-AUFA, SMP-FR-SWCL)", () => {
  it("SMP-FR-AUFA: the unsent text of the composer is kept while the column is hidden and shown again", async () => {
    backend([]);
    render(<DiscussionPane store={store} selected={null} planTool planHost={null} />);
    const user = userEvent.setup();
    await user.type(await composer("Discuss the map"), "half a thought");
    act(() => store.setDiscussionVisible(false));
    expect(screen.queryByTestId("map-discussion")).toBeNull();
    act(() => store.setDiscussionVisible(true));
    expect(await composer("Discuss the map")).toHaveValue("half a thought");
  });

  it("SMP-FR-AUFA: a project switch discards the text with the session", async () => {
    backend([]);
    const first = render(<DiscussionPane store={store} selected={null} planTool planHost={null} />);
    const user = userEvent.setup();
    await user.type(await composer("Discuss the map"), "half a thought");
    first.unmount();
    // The shell clears the map session and the discussion sessions in one step.
    store.clear();
    clearAllDiscussionSessions();
    render(<DiscussionPane store={store} selected={null} planTool planHost={null} />);
    expect(await composer("Discuss the map")).toHaveValue("");
  });

  it("SMP-FR-SWCL: the pane is as wide as the session says, and the splitter shows the range", async () => {
    backend();
    render(<DiscussionPane store={store} selected={null} planTool planHost={null} />);
    const pane = await screen.findByTestId("map-discussion");
    expect(pane).toHaveStyle({ width: "360px" });
    const splitter = screen.getByRole("separator", { name: "Resize the discussion column" });
    expect(splitter).toHaveAttribute("aria-valuemin", "280");
    expect(splitter).toHaveAttribute("aria-valuemax", "560");
    expect(splitter).toHaveAttribute("aria-valuenow", "360");
  });

  it("SMP-FR-SWCL: the splitter resizes the column by keyboard within 280px and 560px", async () => {
    backend();
    render(<DiscussionPane store={store} selected={null} planTool planHost={null} />);
    const splitter = await screen.findByRole("separator", { name: "Resize the discussion column" });
    const user = userEvent.setup();
    splitter.focus();
    await user.keyboard("{ArrowLeft}");
    expect(store.snapshot().discussion.width).toBe(376);
    await user.keyboard("{ArrowRight}{ArrowRight}");
    expect(store.snapshot().discussion.width).toBe(344);
    await user.keyboard("{End}");
    expect(store.snapshot().discussion.width).toBe(560);
    await user.keyboard("{ArrowLeft}");
    expect(store.snapshot().discussion.width).toBe(560);
    await user.keyboard("{Home}");
    expect(store.snapshot().discussion.width).toBe(280);
    expect(screen.getByTestId("map-discussion")).toHaveStyle({ width: "280px" });
  });

  it("SMP-FR-SWCL: a drag with the pointer makes the column wider when it moves to the left", async () => {
    backend();
    render(<DiscussionPane store={store} selected={null} planTool planHost={null} />);
    const splitter = await screen.findByRole("separator", { name: "Resize the discussion column" });
    fireEvent.pointerDown(splitter, { button: 0, clientX: 600, pointerId: 1 });
    fireEvent.pointerMove(splitter, { clientX: 540, pointerId: 1 });
    expect(store.snapshot().discussion.width).toBe(420);
    fireEvent.pointerUp(splitter, { pointerId: 1 });
    fireEvent.pointerMove(splitter, { clientX: 100, pointerId: 1 });
    expect(store.snapshot().discussion.width).toBe(420);
  });
});

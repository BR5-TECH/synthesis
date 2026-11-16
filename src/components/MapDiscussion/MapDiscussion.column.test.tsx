/**
 * The Discussion column of the Map tab (`SMP-specification-map.md`
 * SMP-FR-WNLR .. SMP-FR-BOBB, `CMT-comments.md` CMT-FR-LVNQ): the threads, the
 * Focus chip, Discuss, and the transcript. The split of the column is in
 * `MapDiscussion.split.test.tsx`. The backend is mocked at `invoke`, so every
 * assertion is about what the column renders and which operation it sends.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
// CMT-FR-54: the `"discussion changed"` event, which a test emits by hand.
const discussionChanged = new Set<(thread: Discussion) => void>();
vi.mock("../../events", async (importOriginal) => ({
  ...(await importOriginal<typeof import("../../events")>()),
  onDiscussionChanged: async (handler: (thread: Discussion) => void) => {
    discussionChanged.add(handler);
    return () => discussionChanged.delete(handler);
  },
}));
// No log flush timer may outlive its test.
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

import { logDebug, logWarn } from "../../logging";
import { clearConversationThreads } from "../../state/conversationThreads";
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
}

function nodeOf(args: unknown): MapNodeRef | null {
  return (args as { mapNode?: MapNodeRef | null }).mapNode ?? null;
}

/** MPR-FR-MYLA: the open thread of the node with the latest `updatedAt`. */
function latestOpen(threads: Discussion[], node: MapNodeRef | null): Discussion | null {
  const open = threads.filter((t) => !t.resolved && sameMapNode(t.mapNode ?? null, node));
  return [...open].sort((a, b) => a.updatedAt.localeCompare(b.updatedAt)).pop() ?? null;
}

function wire(b: Backend) {
  invokeMock.mockImplementation((cmd: string, args: unknown) => {
    b.calls.push({ cmd, args });
    switch (cmd) {
      case "list_map_discussions":
        return Promise.resolve([...b.threads]);
      case "find_map_discussion":
        return Promise.resolve(latestOpen(b.threads, nodeOf(args)));
      case "open_map_discussion": {
        // MPR-FR-MAXH: each opening is a new thread.
        const a = args as { body: string; attachments: unknown[] };
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
      case "set_discussion_resolution": {
        const a = args as { discussionId: string; resolved: boolean };
        const at = b.threads.findIndex((t) => t.id === a.discussionId);
        b.threads[at] = { ...b.threads[at], resolved: a.resolved };
        return Promise.resolve(b.threads[at]);
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
/** SMP-FR-IHFM: the look-up of a Discuss has answered, with a thread or none. */
const lookedUp = (found: boolean) =>
  waitFor(() =>
    expect(logDebug).toHaveBeenCalledWith(["frontend"], "map discussion of a node looked up", { found }),
  );
const tabNames = () => screen.getAllByRole("tab").map((t) => t.textContent?.trim());

beforeEach(() => {
  invokeMock.mockReset();
  vi.mocked(logDebug).mockClear();
  vi.mocked(logWarn).mockClear();
  discussionChanged.clear();
  clearConversationThreads();
  store = new SpecMapSessionStore();
  resetAgentRegistry();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetProjectIdentity();
  resetToasts();
});

afterEach(cleanup);

describe("the column (SMP-FR-WNLR, CMT-FR-LVNQ)", () => {
  it("SMP-FR-WNLR: shows the discussion column of a draft: header, tabs, transcript, and composer", async () => {
    backend([mapThread("t1", ["What is left of the plan?"])]);
    await renderColumn();
    const column = screen.getByTestId("map-discussion-column");
    expect(within(column).getByText("Discussion")).toBeInTheDocument();
    expect(await within(column).findByRole("tablist", { name: "Discussions" })).toBeInTheDocument();
    expect(await within(column).findByTestId("comment-thread-t1")).toHaveTextContent("What is left of the plan?");
    expect(await composer("Reply to thread t1")).toBeInTheDocument();
  });

  it("SMP-FR-WNLR: the pane draws the column and a splitter while it is shown, and nothing while it is hidden", async () => {
    backend();
    render(<DiscussionPane store={store} selected={null} planTool planHost={null} />);
    expect(await screen.findByTestId("map-discussion")).toBeInTheDocument();
    expect(screen.getByTestId("map-discussion-splitter")).toBeInTheDocument();
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

  it("CMT-FR-LVNQ: a node thread renders in the column and names its node in the header", async () => {
    backend([mapThread("t2", ["About this spec"], SPEC_NODE)]);
    await renderColumn();
    expect(await screen.findByTestId("comment-thread-t2")).toBeInTheDocument();
    expect(screen.getByTestId("map-discussion-node")).toHaveTextContent("Specification map");
  });

  it("CMT-FR-LVNQ: the composer can attach a focus", async () => {
    backend([mapThread("t1", ["hello"])]);
    await renderColumn({ selected: SPEC_NODE });
    await screen.findByTestId("comment-thread-t1");
    expect(screen.getByRole("group", { name: "Focus" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Use selected node" })).toBeEnabled();
  });
});

describe("the tabs (SMP-FR-WYZA, SMP-FR-EIUP)", () => {
  it("SMP-FR-WYZA, SMP-FR-EIUP: one tab for each open thread in the order of the list, each named Map or by its node", async () => {
    backend([
      mapThread("t1", ["a"]),
      mapThread("t2", ["b"], SPEC_NODE),
      mapThread("t3", ["c"], ITEM_NODE),
      mapThread("t4", ["d"], ITEM_NODE),
      mapThread("t5", ["e"]),
    ]);
    await renderColumn();
    await waitFor(() => expect(screen.getAllByRole("tab")).toHaveLength(5));
    expect(tabNames()).toEqual(["Map", "Specification map", "Decline message", "Decline message", "Map"]);
    expect(screen.getAllByRole("tab")[1]).toHaveAttribute("title", "Specification map");
    // The tabs, then the control that begins another, then the disclosure.
    const row = screen.getByRole("tablist", { name: "Discussions" });
    const last = screen.getAllByRole("tab")[4];
    const begin = screen.getByTestId("discussion-new");
    expect(last.compareDocumentPosition(begin) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(row).toContainElement(begin);
  });

  it("SMP-FR-WYZA: the column shows the first tab where the session holds no active thread", async () => {
    backend([mapThread("t1", ["first"]), mapThread("t2", ["second"], SPEC_NODE)]);
    await renderColumn();
    expect(await screen.findByTestId("comment-thread-t1")).toBeInTheDocument();
    expect(screen.getAllByRole("tab")[0]).toHaveAttribute("aria-selected", "true");
  });

  it("SMP-FR-WYZA: the column shows the active thread of the session", async () => {
    backend([mapThread("t1", ["first"]), mapThread("t2", ["second"], SPEC_NODE)]);
    store.selectDiscussionThread("t2");
    await renderColumn();
    expect(await screen.findByTestId("comment-thread-t2")).toBeInTheDocument();
  });

  it("SMP-FR-WYZA: where the Map holds no open thread, the column shows the opening composer of a whole-map thread", async () => {
    backend([]);
    await renderColumn();
    expect(await composer("Discuss the map")).toBeInTheDocument();
    expect(screen.queryByRole("tablist")).toBeNull();
  });

  it("SMP-FR-EIUP: choosing a tab shows its thread and makes it the active thread", async () => {
    backend([mapThread("t1", ["on the map"]), mapThread("t2", ["on the node"], SPEC_NODE)]);
    await renderColumn();
    await screen.findByTestId("comment-thread-t1");
    const user = userEvent.setup();
    await user.click(screen.getByRole("tab", { name: "Specification map" }));
    expect(await screen.findByTestId("comment-thread-t2")).toBeInTheDocument();
    expect(store.snapshot().discussion.threadId).toBe("t2");
  });

  it("SMP-FR-EIUP: the header names the node of a node thread, and selecting that name selects the node on the canvas", async () => {
    backend([mapThread("t2", ["b"], SPEC_NODE)]);
    const show = vi.spyOn(store, "showMapNode");
    await renderColumn();
    const user = userEvent.setup();
    await user.click(await screen.findByTestId("map-discussion-node"));
    expect(show).toHaveBeenCalledWith(SPEC_NODE);
  });

  it("SMP-FR-EIUP: a whole-map thread has no node name in the header", async () => {
    backend([mapThread("t1", ["a"])]);
    await renderColumn();
    await screen.findByTestId("comment-thread-t1");
    expect(screen.queryByTestId("map-discussion-node")).toBeNull();
  });
});

describe("another discussion (SMP-FR-KHKC)", () => {
  it("SMP-FR-KHKC: the control that begins another discussion opens a new whole-map thread beside the others", async () => {
    const b = backend([mapThread("t1", ["about the node"], ITEM_NODE)]);
    await renderColumn();
    await screen.findByTestId("comment-thread-t1");
    const user = userEvent.setup();
    await user.click(screen.getByTestId("discussion-new"));
    await user.type(await composer("Discuss the map"), "a new topic");
    await user.click(screen.getByRole("button", { name: "Post" }));
    expect(await screen.findByTestId("comment-thread-t2")).toHaveTextContent("a new topic");
    expect(callsOf(b, "open_map_discussion")[0]).toMatchObject({ mapNode: null, body: "a new topic" });
    expect(store.snapshot().discussion.threadId).toBe("t2");
    await waitFor(() => expect(tabNames()).toEqual(["Decline message", "Map"]));
    expect(callsOf(b, "add_comment")).toHaveLength(0);
  });

  it("SMP-FR-KHKC: a later message is a reply in the shown thread", async () => {
    const b = backend([]);
    await renderColumn();
    const user = userEvent.setup();
    await user.type(await composer("Discuss the map"), "first");
    await user.click(screen.getByRole("button", { name: "Post" }));
    await screen.findByTestId("comment-thread-t1");
    await user.type(await composer("Reply to thread t1"), "second");
    await user.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(callsOf(b, "add_comment")).toHaveLength(1));
    expect(callsOf(b, "open_map_discussion")).toHaveLength(1);
  });

  it("SMP-FR-KHKC: a thread that another window opened appears as one tab", async () => {
    const b = backend([mapThread("t1", ["mine"])]);
    await renderColumn();
    await screen.findByTestId("comment-thread-t1");
    const other = mapThread("t-other", ["from another window"], ITEM_NODE);
    b.threads.push(other);
    act(() => discussionChanged.forEach((emit) => emit(other)));
    await waitFor(() => expect(screen.getAllByRole("tab")).toHaveLength(2));
    // The same thread again is still one tab.
    act(() => discussionChanged.forEach((emit) => emit({ ...other })));
    await waitFor(() => expect(tabNames().filter((n) => n === "Decline message")).toHaveLength(1));
    expect(screen.getAllByRole("tab")).toHaveLength(2);
  });
});

describe("resolve and reopen (SMP-FR-HSZM, SMP-FR-UHZK)", () => {
  async function resolveShown(user: ReturnType<typeof userEvent.setup>, id: string) {
    await user.click(await screen.findByRole("button", { name: `Thread actions for ${id}` }));
    await user.click(screen.getByRole("menuitem", { name: "Mark resolved" }));
  }

  it("SMP-FR-HSZM: the header menu resolves the shown thread, which leaves the tabs for the disclosure, and the first open tab shows", async () => {
    const b = backend([mapThread("t1", ["a"]), mapThread("t2", ["b"], SPEC_NODE), mapThread("t3", ["c"], ITEM_NODE)]);
    store.selectDiscussionThread("t2");
    await renderColumn();
    await screen.findByTestId("comment-thread-t2");
    const header = screen.getByTestId("discussion-header-actions");
    expect(within(header).getByRole("button", { name: "Thread actions for t2" })).toBeInTheDocument();
    const user = userEvent.setup();
    await resolveShown(user, "t2");
    await waitFor(() => expect(callsOf(b, "set_discussion_resolution")).toHaveLength(1));
    expect(callsOf(b, "set_discussion_resolution")[0]).toMatchObject({ discussionId: "t2", resolved: true });
    expect(await screen.findByTestId("comment-thread-t1")).toBeInTheDocument();
    await waitFor(() => expect(tabNames()).toEqual(["Map", "Decline message"]));
    expect(screen.getByRole("button", { name: "1 resolved" })).toBeInTheDocument();
  });

  it("SMP-FR-HSZM: resolving the last open thread shows the opening composer", async () => {
    backend([mapThread("t1", ["a"])]);
    await renderColumn();
    const user = userEvent.setup();
    await resolveShown(user, "t1");
    expect(await composer("Discuss the map")).toBeInTheDocument();
    expect(screen.getByText("The discussion about the map is resolved.")).toBeInTheDocument();
  });

  it("SMP-FR-UHZK: a resolved thread reads resolved, with no composer, and Reopen returns it to the tabs", async () => {
    const b = backend([mapThread("t1", ["a"], null, { resolved: true }), mapThread("t2", ["b"])]);
    await renderColumn();
    await screen.findByTestId("comment-thread-t2");
    const user = userEvent.setup();
    await user.click(screen.getByRole("button", { name: "1 resolved" }));
    expect(store.snapshot().discussion.showResolved).toBe(true);
    await user.click(screen.getByRole("tab", { name: "Map", selected: false }));
    expect(await screen.findByTestId("comment-thread-t1")).toBeInTheDocument();
    expect(screen.getByTestId("discussion-resolved-state")).toHaveTextContent("resolved");
    expect(screen.queryByRole("textbox", { name: "Reply to thread t1" })).toBeNull();
    expect(screen.queryByRole("button", { name: /Quote comment/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Thread actions for t1" }));
    await user.click(screen.getByRole("menuitem", { name: "Reopen discussion" }));
    await waitFor(() =>
      expect(callsOf(b, "set_discussion_resolution")[0]).toMatchObject({ discussionId: "t1", resolved: false }),
    );
    expect(await composer("Reply to thread t1")).toBeInTheDocument();
  });

  it("SMP-FR-UHZK: with the disclosure expanded and no open thread, the resolved thread shows with its node in the header", async () => {
    backend([mapThread("t1", ["old"], ITEM_NODE, { resolved: true })]);
    store.setDiscussionShowResolved(true);
    await renderColumn();
    expect(await screen.findByTestId("comment-thread-t1")).toBeInTheDocument();
    expect(screen.getByTestId("map-discussion-node")).toHaveTextContent("Decline message");
  });

  it("SMP-FR-UHZK: a resolved thread of a node blocks nothing: Discuss on the node opens a new one", async () => {
    const b = backend([mapThread("t1", ["old"], ITEM_NODE, { resolved: true })]);
    store.discuss(ITEM_NODE);
    await renderColumn();
    await lookedUp(false);
    const user = userEvent.setup();
    await user.type(await composer("Discuss this node"), "again");
    await user.click(screen.getByRole("button", { name: "Post" }));
    expect(await screen.findByTestId("comment-thread-t2")).toHaveTextContent("again");
    expect(callsOf(b, "open_map_discussion")[0]).toMatchObject({ mapNode: ITEM_NODE });
    expect(b.threads.find((t) => t.id === "t1")?.resolved).toBe(true);
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

  it("SMP-FR-DUUU: the first message of a new node thread carries the node as its focus", async () => {
    const b = backend([]);
    store.discuss(ITEM_NODE);
    await renderColumn();
    const user = userEvent.setup();
    await waitFor(() => expect(screen.getByTestId("map-focus-chip")).toHaveTextContent("Decline message"));
    await user.type(await composer("Discuss this node"), "first");
    await user.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(callsOf(b, "open_map_discussion")).toHaveLength(1));
    const sent = callsOf(b, "open_map_discussion")[0] as { attachments: unknown[] };
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

describe("Discuss (SMP-FR-IHFM)", () => {
  it("SMP-FR-IHFM: Discuss on a node shows its latest open thread, and creates none", async () => {
    const older = mapThread("t1", ["older"], SPEC_NODE, { updatedAt: "2026-01-01T00:00:00Z" });
    const newer = mapThread("t2", ["newer"], SPEC_NODE, { updatedAt: "2026-01-02T00:00:00Z" });
    // The latest is listed first, so only the answer of the find picks it.
    const b = backend([newer, older, mapThread("t3", ["map"])]);
    store.setDiscussionVisible(false);
    store.discuss(SPEC_NODE);
    await renderColumn();
    expect(await screen.findByTestId("comment-thread-t2")).toHaveTextContent("newer");
    // The caret moves from the opening composer to the reply composer of the thread.
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Reply to thread t2" })).toHaveFocus());
    expect(callsOf(b, "find_map_discussion")[0]).toEqual({ mapNode: SPEC_NODE });
    expect(callsOf(b, "open_map_discussion")).toHaveLength(0);
    expect(store.snapshot().discussion.visible).toBe(true);
  });

  it("SMP-FR-IHFM: where the node has no open thread the column shows the focused opening composer of the node, and no tab is selected", async () => {
    const b = backend([mapThread("t1", ["map"])]);
    store.discuss(ITEM_NODE);
    await renderColumn();
    await lookedUp(false);
    expect(await screen.findByText("This begins a discussion about Decline message.")).toBeInTheDocument();
    // SMP-FR-XGBI: the composer of a node says nothing about the plan tools.
    expect(screen.queryByText(/proposes changes only while/)).toBeNull();
    await waitFor(() => expect(screen.getByRole("textbox", { name: "Discuss this node" })).toHaveFocus());
    expect(screen.getAllByRole("tab").every((t) => t.getAttribute("aria-selected") === "false")).toBe(true);
    expect(screen.getByTestId("map-discussion-node")).toHaveTextContent("Decline message");
    expect(callsOf(b, "open_map_discussion")).toHaveLength(0);
    expect(callsOf(b, "open_discussion")).toHaveLength(0);
  });

  it("SMP-FR-IHFM: Discuss leaves the thread the column was reading for the node", async () => {
    backend([mapThread("t1", ["map"]), mapThread("t2", ["node"], ITEM_NODE)]);
    await renderColumn();
    await screen.findByTestId("comment-thread-t1");
    act(() => store.discuss(ITEM_NODE));
    expect(await screen.findByTestId("comment-thread-t2")).toBeInTheDocument();
  });

  it("SMP-FR-IHFM: a late answer of the find does not undo a tab the author chose meanwhile", async () => {
    const b = backend([mapThread("t1", ["map"]), mapThread("t2", ["node"], ITEM_NODE)]);
    let answer: (thread: Discussion | null) => void = () => {};
    const wired = invokeMock.getMockImplementation() as (cmd: string, args: unknown) => Promise<unknown>;
    invokeMock.mockImplementation((cmd: string, args: unknown) =>
      cmd === "find_map_discussion"
        ? new Promise((resolve) => {
            b.calls.push({ cmd, args });
            answer = resolve;
          })
        : wired(cmd, args),
    );
    store.discuss(ITEM_NODE);
    await renderColumn();
    await waitFor(() => expect(callsOf(b, "find_map_discussion")).toHaveLength(1));
    const user = userEvent.setup();
    await user.click(await screen.findByRole("tab", { name: "Map" }));
    expect(await screen.findByTestId("comment-thread-t1")).toBeInTheDocument();
    await act(async () => answer(b.threads[1]));
    await lookedUp(true);
    expect(store.snapshot().discussion.threadId).toBe("t1");
    expect(screen.getByTestId("comment-thread-t1")).toBeInTheDocument();
  });

  it("SMP-FR-IHFM: a refused find keeps the opening composer of the node", async () => {
    const b = backend([]);
    const wired = invokeMock.getMockImplementation() as (cmd: string, args: unknown) => Promise<unknown>;
    invokeMock.mockImplementation((cmd: string, args: unknown) =>
      cmd === "find_map_discussion" ? Promise.reject(new Error("no_project_open")) : wired(cmd, args),
    );
    store.discuss(ITEM_NODE);
    await renderColumn();
    expect(await composer("Discuss this node")).toBeInTheDocument();
    await waitFor(() =>
      expect(logWarn).toHaveBeenCalledWith(["frontend"], "map discussion look-up of a node failed", expect.anything()),
    );
    expect(store.snapshot().discussion.composing).toBe(true);
    expect(callsOf(b, "open_map_discussion")).toHaveLength(0);
  });

  it("SMP-FR-IHFM: Discuss on the node of the thread on screen leaves the composer for that thread", async () => {
    backend([mapThread("t2", ["node"], ITEM_NODE)]);
    store.selectDiscussionThread("t2");
    await renderColumn();
    await screen.findByTestId("comment-thread-t2");
    act(() => store.discuss(ITEM_NODE));
    await lookedUp(true);
    expect(await screen.findByTestId("comment-thread-t2")).toBeInTheDocument();
    expect(store.snapshot().discussion.composing).toBe(false);
  });

  it("SMP-FR-IHFM: a column that mounts again does not ask again, and keeps the composer of the node", async () => {
    const b = backend([]);
    store.discuss(ITEM_NODE);
    const view = render(<Column />);
    await lookedUp(false);
    // Another window opens a thread of the node meanwhile.
    b.threads.push(mapThread("t9", ["elsewhere"], ITEM_NODE));
    view.unmount();
    render(<Column />);
    expect(await composer("Discuss this node")).toBeInTheDocument();
    expect(callsOf(b, "find_map_discussion")).toHaveLength(1);
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
});

describe("the opening composer of a whole-map thread (SMP-FR-XGBI)", () => {
  it("SMP-FR-XGBI: tells the author that the agent reads the plan and proposes changes only while a GitHub Project is the delivery source", async () => {
    backend();
    await renderColumn({ planTool: true });
    const note = await screen.findByText(/The agent reads the plan and proposes changes only while a GitHub Project is the delivery source\./);
    expect(note).not.toHaveTextContent("has no plan tool");
  });

  it("SMP-FR-XGBI: with another delivery source the column says that the agent has no plan tool", async () => {
    backend();
    await renderColumn({ planTool: false });
    expect(await screen.findByText(/the agent has no plan tool/)).toBeInTheDocument();
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

describe("the session of the column (SMP-FR-AUFA)", () => {
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

  it("SMP-FR-AUFA: the unsent text of a node composer is its own", async () => {
    backend([]);
    store.discuss(ITEM_NODE);
    render(<DiscussionPane store={store} selected={null} planTool planHost={null} />);
    const user = userEvent.setup();
    await user.type(await composer("Discuss this node"), "about the node");
    act(() => store.cancelDiscussionComposer());
    expect(await composer("Discuss the map")).toHaveValue("");
    act(() => store.discuss(ITEM_NODE));
    expect(await composer("Discuss this node")).toHaveValue("about the node");
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
});

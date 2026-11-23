/**
 * The surface panel (`ASF-agent-surfaces.md`, `CMT-comments.md` CMT-FR-DBQC).
 *
 * The backend is a strict `invoke` mock routed by command name, and the
 * events are delivered by hand.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
function emitEvent(name: string, payload: unknown) {
  [...(listeners.get(name) ?? [])].forEach((h) => h({ payload }));
}
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (e: { payload: unknown }) => void) => {
    const set = listeners.get(event) ?? new Set();
    set.add(cb);
    listeners.set(event, set);
    return () => set.delete(cb);
  }),
}));

const dispatchMock = vi.fn(async () => {});
vi.mock("../DraftDiscussion/decisionDispatch", () => ({
  dispatchDecision: (...args: unknown[]) => dispatchMock(...(args as [])),
}));

import { AgentSurfacePanel } from ".";
import { CommentAttachmentList } from "../CommentAttachments";
import { resetSurfaceModelsForTest } from "./model";
import { clearConversationThreads, publishThread } from "../../state/conversationThreads";
import { flushLogs, resetLogBufferForTest } from "../../logging";
import type { AgentSurface, Discussion, SurfaceComponent } from "../../types";

type Handler = (args: Record<string, unknown>) => unknown;

type LoggedRecord = { message: string; fields?: Record<string, unknown> };

/** Every log record the module has flushed through the mock. */
function logRecordsForTest(): LoggedRecord[] {
  flushLogs();
  return invokeMock.mock.calls
    .filter(([cmd]) => cmd === "append_log_records")
    .flatMap(([, args]) => (args as { records: LoggedRecord[] }).records);
}
let handlers: Record<string, Handler> = {};

beforeEach(() => {
  resetLogBufferForTest();
  resetSurfaceModelsForTest();
  clearConversationThreads();
  listeners.clear();
  dispatchMock.mockReset();
  invokeMock.mockReset();
  handlers = {};
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    if (cmd === "append_log_records") return undefined;
    const handler = handlers[cmd];
    if (!handler) throw new Error(`unexpected command ${cmd}`);
    return handler(args ?? {});
  });
});

afterEach(() => {
  cleanup();
});

const THREAD = "thread-1";

function publish(resolved = false) {
  publishThread({
    id: THREAD,
    target: { kind: "draft", draftId: "draft-1" },
    resolved,
    comments: [],
  } as unknown as Discussion);
}

function form(): SurfaceComponent[] {
  return [
    { id: "root", component: "Column", children: ["q", "echo", "pick", "note", "send"] },
    { id: "q", component: "Text", text: "Which storage do you want?" },
    { id: "echo", component: "Text", text: { "@call": "formatString", args: { value: "Note: ${/note}" } } },
    {
      id: "pick",
      component: "ChoicePicker",
      label: "Storage",
      options: [
        { label: "SQLite", value: "sqlite" },
        { label: "Files", value: "files" },
      ],
      value: { "@path": "/choice" },
    },
    { id: "note", component: "TextField", label: "Note", value: { "@path": "/note" } },
    {
      id: "send",
      component: "Button",
      variant: "primary",
      child: "send_label",
      action: {
        event: {
          name: "choose_storage",
          context: { choice: { "@path": "/choice" }, note: { "@path": "/note" }, kind: "storage" },
        },
      },
      "@checks": [
        { condition: { "@call": "required", args: { value: { "@path": "/choice" } } }, message: "Pick one." },
      ],
    },
    { id: "send_label", component: "Text", text: "Send" },
  ];
}

function surface(over: Partial<AgentSurface> = {}): AgentSurface {
  return {
    id: "s-1",
    discussionId: THREAD,
    target: { kind: "draft", draftId: "draft-1" },
    commentId: "c-1",
    catalogId: "synthesis:discussion/v1",
    components: form(),
    dataModel: { choice: [], note: "" },
    authoredBy: { kind: "agent", agentId: "a-1", handle: "arch" },
    createdAt: "2026-01-01T00:00:00Z",
    state: "open",
    ...over,
  } as AgentSurface;
}

function submitted(): AgentSurface {
  return surface({
    state: "submitted",
    submission: {
      name: "choose_storage",
      sourceComponentId: "send",
      context: { choice: ["files"], note: "fast", kind: "storage" },
      dataModel: { choice: ["files"], note: "fast" },
      by: { kind: "human", login: "ada" },
      at: "2026-01-02T10:00:00Z",
      commentId: "c-2",
    },
  });
}

function mount(record: AgentSurface = surface()) {
  handlers.get_agent_surface = () => record;
  return render(
    <AgentSurfacePanel attachment={{ kind: "agentSurface", surfaceId: record.id }} threadId={THREAD} />,
  );
}

async function loaded() {
  await waitFor(() =>
    expect(screen.getByTestId("agent-surface").dataset.state).not.toBe("loading"),
  );
  return screen.getByTestId("agent-surface");
}

describe("AgentSurfacePanel", () => {
  it("ASF-FR-RRXP / ASF-FR-MGVD: a placeholder while loading, and Retry after a failed read", async () => {
    let release: (s: AgentSurface) => void = () => {};
    handlers.get_agent_surface = () => new Promise<AgentSurface>((res) => (release = res));
    publish();
    render(<AgentSurfacePanel attachment={{ kind: "agentSurface", surfaceId: "s-1" }} threadId={THREAD} />);
    expect(await screen.findByText("Loading the panel…")).toBeInTheDocument();
    await act(async () => release(surface()));
    expect(await screen.findByText("Which storage do you want?")).toBeInTheDocument();
    cleanup();

    let reads = 0;
    handlers.get_agent_surface = () => {
      reads += 1;
      if (reads === 1) throw new Error("surface_not_found");
      return surface();
    };
    render(<AgentSurfacePanel attachment={{ kind: "agentSurface", surfaceId: "s-1" }} threadId={THREAD} />);
    expect(await screen.findByText("This panel could not be loaded.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Which storage do you want?")).toBeInTheDocument();
    const warn = logRecordsForTest().find((r) => r.message === "agent surface could not be read");
    expect(warn?.fields).toEqual({ surfaceId: "s-1" });
  });

  it("ASF-FR-ENRG: an input writes the local model, and every bound component shows it at once", async () => {
    publish();
    mount();
    await loaded();
    fireEvent.change(screen.getByLabelText("Note"), { target: { value: "fast" } });
    expect(screen.getByText("Note: fast")).toBeInTheDocument();
    expect(screen.getByLabelText("Note")).toHaveValue("fast");
  });

  it("ASF-FR-JLUJ: the local model stays when the panel mounts again, and each surface has its own", async () => {
    publish();
    const first = mount();
    await loaded();
    fireEvent.change(screen.getByLabelText("Note"), { target: { value: "kept" } });
    first.unmount();
    mount();
    await loaded();
    expect(screen.getByLabelText("Note")).toHaveValue("kept");
    cleanup();
    mount(surface({ id: "s-2" }));
    await loaded();
    expect(screen.getByLabelText("Note")).toHaveValue("");
  });

  it("ASF-FR-GKYM: a false check disables the button and shows its message", async () => {
    publish();
    mount();
    await loaded();
    const send = screen.getByRole("button", { name: "Send" });
    expect(send).toBeDisabled();
    expect(screen.getByText("Pick one.")).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("Files"));
    expect(send).toBeEnabled();
    expect(screen.queryByText("Pick one.")).not.toBeInTheDocument();
  });

  it("ASF-FR-QBTR / ASF-FR-XOSG / ASF-FR-DHGO: a press sends the action once, disables the panel while it runs, and brings the agents back", async () => {
    publish();
    let release: (r: unknown) => void = () => {};
    handlers.submit_surface_action = () => new Promise((res) => (release = res));
    mount();
    await loaded();
    fireEvent.click(screen.getByLabelText("Files"));
    fireEvent.change(screen.getByLabelText("Note"), { target: { value: "fast" } });
    const send = screen.getByRole("button", { name: "Send" });
    fireEvent.click(send);
    fireEvent.click(send);
    const calls = invokeMock.mock.calls.filter(([cmd]) => cmd === "submit_surface_action");
    expect(calls).toHaveLength(1);
    expect(calls[0][1]).toEqual({
      surfaceId: "s-1",
      action: {
        name: "choose_storage",
        sourceComponentId: "send",
        context: { choice: ["files"], note: "fast", kind: "storage" },
        dataModel: { choice: ["files"], note: "fast" },
      },
    });
    expect(screen.getByTestId("agent-surface").dataset.state).toBe("sending");
    expect(screen.getByLabelText("Note")).toBeDisabled();
    expect(send).toBeDisabled();
    await act(async () => release({ surface: submitted(), commentId: "c-2" }));
    await waitFor(() => expect(dispatchMock).toHaveBeenCalledTimes(1));
    expect(dispatchMock.mock.calls[0]).toEqual([
      { id: "s-1", threadId: THREAD, target: { kind: "draft", draftId: "draft-1" } },
      { commentId: "c-2" },
      expect.objectContaining({ id: THREAD }),
      [],
      "",
    ]);
    expect(screen.getByTestId("agent-surface").dataset.state).toBe("submitted");
  });

  it("ASF-FR-YCTE: a submitted panel is read-only, shows the sent values, and names what was sent", async () => {
    publish();
    mount(submitted());
    await loaded();
    expect(screen.getByLabelText("Note")).toBeDisabled();
    expect(screen.getByLabelText("Note")).toHaveValue("fast");
    expect(screen.getByLabelText("Files")).toBeChecked();
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Send" })).toHaveAttribute("data-pressed", "true");
    expect(screen.getByText(/Sent “Send” by ada on/)).toBeInTheDocument();
  });

  it("ASF-FR-CUEI: the panel draws the record its own event carries, and no other", async () => {
    publish();
    mount();
    await loaded();
    act(() => emitEvent("agent-surface-changed", { ...submitted(), id: "other" }));
    expect(screen.getByTestId("agent-surface").dataset.state).toBe("open");
    act(() => emitEvent("agent-surface-changed", submitted()));
    expect(screen.getByTestId("agent-surface").dataset.state).toBe("submitted");
  });

  it("ASF-FR-NLJW / ASF-FR-MGVD: already submitted reads the record again; another error shows under the panel and frees it", async () => {
    publish();
    mount();
    await loaded();
    handlers.submit_surface_action = () => {
      throw new Error("comment_failed");
    };
    fireEvent.click(screen.getByLabelText("Files"));
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The values were not sent. Try again.");
    expect(screen.getByRole("button", { name: "Send" })).toBeEnabled();
    expect(screen.getByLabelText("Files")).toBeChecked();
    expect(dispatchMock).not.toHaveBeenCalled();

    handlers.submit_surface_action = () => {
      throw new Error("already_submitted");
    };
    // Another window sent it, so the record now reads as submitted.
    handlers.get_agent_surface = () => submitted();
    fireEvent.click(screen.getByRole("button", { name: "Send" }));
    await waitFor(() => expect(screen.getByTestId("agent-surface").dataset.state).toBe("submitted"));
    const warns = logRecordsForTest().filter((r) => r.message === "agent surface action failed");
    expect(warns.map((w) => w.fields)).toEqual([
      { surfaceId: "s-1", error: "comment_failed" },
      { surfaceId: "s-1", error: "already_submitted" },
    ]);
    const text = JSON.stringify(logRecordsForTest());
    expect(text).not.toContain("Which storage");
    expect(text).not.toContain("files");
  });

  it("ASF-FR-JSTI: a panel in a resolved discussion is read-only with no sent line", async () => {
    publish(true);
    mount();
    await loaded();
    expect(screen.getByTestId("agent-surface").dataset.state).toBe("resolved");
    expect(screen.getByLabelText("Note")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Send" })).toBeDisabled();
    expect(screen.queryByText(/^Sent /)).not.toBeInTheDocument();
  });

  it("ASF-FR-HRQL / ASF-FR-SERF / ASF-FR-JPNA: every component draws, an unknown one draws its line, and the panel has no fixed height", async () => {
    publish();
    mount(
      surface({
        components: [
          { id: "root", component: "Column", children: ["row", "card", "list", "unknown", "slider", "date", "check", "chips"] },
          { id: "row", component: "Row", children: ["icon", "div", "cap"] },
          { id: "icon", component: "Icon", name: "info" },
          { id: "div", component: "Divider", axis: "vertical" },
          { id: "cap", component: "Text", text: "A caption", variant: "caption" },
          { id: "card", component: "Card", child: "inside" },
          { id: "inside", component: "Text", text: "In a card" },
          { id: "list", component: "List", children: { componentId: "item", path: "/items" } },
          {
            id: "item",
            component: "Text",
            text: { "@call": "formatString", args: { value: "${name}" } },
          },
          { id: "unknown", component: "Video", url: "https://example.com/v.mp4" },
          { id: "slider", component: "Slider", label: "Level", max: 10, value: { "@path": "/level" } },
          { id: "date", component: "DateTimeInput", label: "When", value: { "@path": "/when" }, enableDate: true },
          { id: "check", component: "CheckBox", label: "Agree", value: { "@path": "/ok" } },
          {
            id: "chips",
            component: "ChoicePicker",
            label: "Tags",
            variant: "multipleSelection",
            displayStyle: "chips",
            options: [
              { label: "Fast", value: "fast" },
              { label: "Small", value: "small" },
            ],
            value: { "@path": "/tags" },
          },
        ],
        dataModel: { items: [{ name: "one" }, { name: "two" }, { name: "three" }], level: 4, when: "", ok: false, tags: [] },
      }),
    );
    await loaded();
    expect(screen.getByText("A caption")).toHaveClass("agent-surface__text--caption");
    expect(screen.getByText("In a card").closest(".agent-surface__card")).not.toBeNull();
    const panel = screen.getByTestId("agent-surface");
    expect(panel.style.height).toBe("");
    expect(panel.style.maxHeight).toBe("");
    const items = within(screen.getByRole("list")).getAllByText(/^(one|two|three)$/);
    expect(items.map((i) => i.textContent)).toEqual(["one", "two", "three"]);
    expect(screen.getByText("This part cannot be shown.")).toBeInTheDocument();
    expect(screen.getByLabelText(/Level/)).toHaveValue("4");
    expect(screen.getByLabelText("When")).toHaveAttribute("type", "date");
    fireEvent.click(screen.getByLabelText("Agree"));
    expect(screen.getByLabelText("Agree")).toBeChecked();
    const fast = screen.getByRole("button", { name: "Fast" });
    fireEvent.click(fast);
    expect(fast).toHaveAttribute("aria-pressed", "true");
    expect(document.querySelector("video, img, iframe")).toBeNull();
  });

  it("ASF-FR-GZFO / ASF-FR-TAVD: a table has header cells and a row per object; tabs follow the tabs pattern", async () => {
    publish();
    mount(
      surface({
        components: [
          { id: "root", component: "Tabs", tabs: [{ title: "Compare", child: "table" }, { title: "Notes", child: "notes" }] },
          {
            id: "table",
            component: "Table",
            caption: "Options",
            columns: [
              { key: "name", title: "Name" },
              { key: "cost", title: "Cost" },
            ],
            rows: { "@path": "/rows" },
          },
          { id: "notes", component: "Text", text: "<img src=x onerror=alert(1)>" },
        ],
        dataModel: { rows: [{ name: "SQLite", cost: 0 }, { name: "Postgres" }] },
      }),
    );
    await loaded();
    expect(screen.getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Name", "Cost"]);
    const rows = screen.getAllByRole("row").slice(1);
    expect(rows.map((r) => within(r).getAllByRole("cell").map((c) => c.textContent))).toEqual([
      ["SQLite", "0"],
      ["Postgres", ""],
    ]);
    const tabs = screen.getAllByRole("tab");
    expect(tabs[0]).toHaveAttribute("aria-selected", "true");
    fireEvent.keyDown(tabs[0], { key: "ArrowRight" });
    expect(screen.getAllByRole("tab")[1]).toHaveAttribute("aria-selected", "true");
    expect(screen.getByRole("tabpanel")).toHaveTextContent("<img src=x onerror=alert(1)>");
    expect(document.querySelector("img")).toBeNull();
  });

  it("ASF-FR-ASQV / CMT-FR-DBQC: an agent_surface attachment draws the panel under the comment, with no chip", async () => {
    publish();
    handlers.get_agent_surface = () => surface();
    render(
      <CommentAttachmentList threadId={THREAD} attachments={[{ kind: "agentSurface", surfaceId: "s-1" }]} />,
    );
    const text = await screen.findByText("Which storage do you want?");
    expect(text.closest("[data-testid='agent-surface']")).not.toBeNull();
    expect(screen.queryByTestId("attachment-chip")).not.toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("get_agent_surface", { surfaceId: "s-1" });
  });
});

/**
 * The cards of the surface panel (`ASF-agent-surfaces.md` ASF-FR-EKZW to
 * ASF-FR-MLKG), and the parts of the panel the first file leaves open.
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


function cardSurface(card: Record<string, unknown>, extra: Partial<AgentSurface> = {}): AgentSurface {
  return surface({ components: [{ id: "root", ...card } as SurfaceComponent], dataModel: {}, ...extra });
}

function submitCalls() {
  return invokeMock.mock.calls.filter(([cmd]) => cmd === "submit_surface_action").map(([, a]) => a);
}

const CHOICE = {
  component: "ChoiceCard",
  question: "Which storage?",
  options: [
    { value: "sqlite", label: "SQLite", description: "One file." },
    { value: "files", label: "Plain files" },
  ],
  allowOther: true,
  action: { name: "choose" },
};

describe("surface cards", () => {
  it("ASF-FR-EKZW / ASF-FR-MLKG: a choice card sends its selection, and Send waits for a choice or an own answer", async () => {
    publish();
    handlers.submit_surface_action = () => new Promise(() => {});
    mount(cardSurface(CHOICE));
    await loaded();
    expect(screen.getByText("Which storage?")).toBeInTheDocument();
    expect(screen.getByText("One file.")).toBeInTheDocument();
    const send = screen.getByRole("button", { name: "Send" });
    expect(send).toBeDisabled();
    fireEvent.change(screen.getByLabelText("Own answer"), { target: { value: "Redis" } });
    expect(send).toBeEnabled();
    fireEvent.click(screen.getByRole("radio", { name: /Plain files/ }));
    fireEvent.click(send);
    expect(submitCalls()).toEqual([
      {
        surfaceId: "s-1",
        action: {
          name: "choose",
          sourceComponentId: "root",
          context: { selected: ["files"], other: "Redis" },
          dataModel: {},
        },
      },
    ]);
  });

  it("ASF-FR-EKZW: a multiple choice card takes several options", async () => {
    publish();
    handlers.submit_surface_action = () => new Promise(() => {});
    mount(cardSurface({ ...CHOICE, multiple: true, allowOther: false, sendLabel: "Use these" }));
    await loaded();
    fireEvent.click(screen.getByRole("checkbox", { name: /SQLite/ }));
    fireEvent.click(screen.getByRole("checkbox", { name: /Plain files/ }));
    expect(screen.queryByLabelText("Own answer")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Use these" }));
    expect((submitCalls()[0] as { action: { context: unknown } }).action.context).toEqual({
      selected: ["sqlite", "files"],
    });
  });

  it("ASF-FR-IBTY: a confirm card has a danger confirm button and a decline button, each with its own name", async () => {
    publish();
    handlers.submit_surface_action = () => new Promise(() => {});
    mount(
      cardSurface({
        component: "ConfirmCard",
        title: "Delete the drafts?",
        body: "Their discussions go too.",
        confirm: { label: "Delete", name: "delete" },
        decline: { label: "Keep", name: "keep" },
        tone: "danger",
      }),
    );
    await loaded();
    expect(screen.getByText("Their discussions go too.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Delete" })).toHaveClass("btn--danger");
    fireEvent.click(screen.getByRole("button", { name: "Keep" }));
    expect((submitCalls()[0] as { action: { name: string; context: unknown } }).action).toMatchObject({
      name: "keep",
      context: {},
    });
  });

  it("ASF-FR-EOLY: a comparison card marks the recommended option in words and sends the pick", async () => {
    publish();
    handlers.submit_surface_action = () => new Promise(() => {});
    mount(
      cardSurface({
        component: "ComparisonCard",
        title: "Queues",
        criteria: [
          { key: "ops", title: "Operations" },
          { key: "managed", title: "Managed" },
        ],
        options: [
          { value: "redis", label: "Redis", values: { ops: "One more service", managed: false }, recommended: true },
          { value: "pg", label: "Postgres", values: { ops: "Deployed", managed: true } },
        ],
        pick: { name: "pick_queue" },
      }),
    );
    await loaded();
    expect(screen.getAllByRole("columnheader").map((h) => h.textContent)).toEqual([
      "Pick",
      "Option",
      "Operations",
      "Managed",
    ]);
    expect(screen.getByRole("rowheader", { name: /Redis/ })).toHaveTextContent("Recommended");
    expect(screen.getAllByRole("cell").map((c) => c.textContent)).toContain("No");
    const pick = screen.getByRole("button", { name: "Pick" });
    expect(pick).toBeDisabled();
    fireEvent.click(screen.getByRole("radio", { name: "Postgres" }));
    fireEvent.click(pick);
    expect((submitCalls()[0] as { action: { context: unknown } }).action.context).toEqual({ picked: "pg" });
  });

  it("ASF-FR-MXGW: a checklist card starts with the declared items checked and sends with none checked", async () => {
    publish();
    handlers.submit_surface_action = () => new Promise(() => {});
    mount(
      cardSurface({
        component: "ChecklistCard",
        title: "Files",
        items: [
          { value: "api", label: "src/api.ts", checked: true },
          { value: "readme", label: "README.md", description: "Usage only." },
        ],
        action: { name: "pick_files" },
      }),
    );
    await loaded();
    const api = screen.getByRole("checkbox", { name: /src\/api.ts/ });
    expect(api).toBeChecked();
    fireEvent.click(api);
    const send = screen.getByRole("button", { name: "Send" });
    expect(send).toBeEnabled();
    fireEvent.click(send);
    expect((submitCalls()[0] as { action: { context: unknown } }).action.context).toEqual({ checked: [] });
  });

  it("ASF-FR-MLKG / ASF-FR-YCTE: a submitted card shows the sent selection read-only and names its button", async () => {
    publish();
    mount(
      cardSurface(CHOICE, {
        state: "submitted",
        submission: {
          name: "choose",
          sourceComponentId: "root",
          context: { selected: ["sqlite"], other: "" },
          dataModel: {},
          by: { kind: "human", login: "ada" },
          at: "2026-01-02T10:00:00Z",
          commentId: "c-2",
        },
      }),
    );
    await loaded();
    expect(screen.getByRole("radio", { name: /SQLite/ })).toBeChecked();
    expect(screen.getByRole("radio", { name: /SQLite/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Send" })).toHaveAttribute("data-pressed", "true");
    expect(screen.getByText(/Sent “Send” by ada on/)).toBeInTheDocument();
  });

  it("ASF-FR-MLKG: a card keeps its selection when the panel mounts again", async () => {
    publish();
    const first = mount(cardSurface(CHOICE));
    await loaded();
    fireEvent.click(screen.getByRole("radio", { name: /SQLite/ }));
    first.unmount();
    mount(cardSurface(CHOICE));
    await loaded();
    expect(screen.getByRole("radio", { name: /SQLite/ })).toBeChecked();
  });
});

describe("the panel parts the first file leaves open", () => {
  it("ASF-FR-GKYM / ASF-FR-TAVD: an input shows its own check messages, and takes its name from its accessibility label", async () => {
    publish();
    mount(
      surface({
        components: [
          { id: "root", component: "Column", children: ["field", "go"] },
          {
            id: "field",
            component: "TextField",
            label: "",
            "@accessibility": { label: "Release name" },
            value: { "@path": "/name" },
            "@checks": [
              { condition: { "@call": "length", args: { value: { "@path": "/name" }, max: 3 } }, message: "At most 3." },
            ],
          },
          { id: "go", component: "Button", child: "go_label", action: { event: { name: "go" } } },
          { id: "go_label", component: "Text", text: "Go" },
        ],
        dataModel: { name: "" },
      }),
    );
    await loaded();
    const field = screen.getByLabelText("Release name");
    expect(screen.queryByText("At most 3.")).not.toBeInTheDocument();
    fireEvent.change(field, { target: { value: "toolong" } });
    expect(screen.getByText("At most 3.")).toBeInTheDocument();
  });

  it("ASF-FR-TURS / ASF-FR-GHUZ: a Row template draws each item with its index, and a failing call draws an empty value", async () => {
    publish();
    mount(
      surface({
        components: [
          { id: "root", component: "Column", children: ["row", "broken"] },
          { id: "row", component: "Row", children: { componentId: "cell", path: "/items" } },
          {
            id: "cell",
            component: "Text",
            text: {
              "@call": "formatString",
              args: { value: { "@call": "formatNumber", args: { value: { "@call": "@index", args: { offset: 1 } } } } },
            },
          },
          { id: "broken", component: "Text", text: { "@call": "formatDate", args: { value: "never", format: "yyyy" } } },
        ],
        dataModel: { items: ["a", "b", "c"] },
      }),
    );
    await loaded();
    const row = document.querySelector(".agent-surface__stack--row") as HTMLElement;
    expect(within(row).getAllByText(/^[123]$/).map((n) => n.textContent)).toEqual(["1", "2", "3"]);
    const texts = [...document.querySelectorAll(".agent-surface__text")].map((n) => n.textContent);
    expect(texts[texts.length - 1]).toBe("");
  });
});

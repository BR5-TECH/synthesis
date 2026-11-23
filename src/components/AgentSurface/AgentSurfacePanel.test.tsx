/**
 * The surface panel with the official A2UI renderer (`ASF-agent-surfaces.md`,
 * `CMT-comments.md` CMT-FR-DBQC).
 *
 * The backend is a strict `invoke` mock routed by command name, and the
 * events are delivered by hand. The official controls are custom elements, so
 * each check waits for the state it reads.
 */
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

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

const openUrlMock = vi.fn(async () => {});
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: (...args: unknown[]) => openUrlMock(...(args as [])),
}));

const dispatchMock = vi.fn(async () => {});
vi.mock("../DraftDiscussion/decisionDispatch", () => ({
  dispatchDecision: (...args: unknown[]) => dispatchMock(...(args as [])),
}));

import { AgentSurfacePanel } from ".";
import { CommentAttachmentList } from "../CommentAttachments";
import { resetSurfaceModelsForTest } from "./model";
import { resetSurfaceRenderersForTest } from "./a2ui/SurfaceRenderer";
import { clearConversationThreads, publishThread } from "../../state/conversationThreads";
import { flushLogs, resetLogBufferForTest } from "../../logging";
import {
  THREAD,
  loadedPanel,
  officialButton,
  routeInvoke,
  surfaceOf,
  textFieldInput,
  findTextField,
  type Handler,
} from "../../test/agentSurfaceSupport";
import type { AgentSurface, Discussion, SurfaceComponent } from "../../types";

type LoggedRecord = { message: string; fields?: Record<string, unknown> };

function logRecords(): LoggedRecord[] {
  flushLogs();
  return invokeMock.mock.calls
    .filter(([cmd]) => cmd === "append_log_records")
    .flatMap(([, args]) => (args as { records: LoggedRecord[] }).records);
}

let handlers: Record<string, Handler> = {};

// ASF-FR-RTRX: the renderer is a chunk of its own; load it once before the
// tests, so no test waits on its first transform.
beforeAll(async () => {
  await import("./a2ui/SurfaceRenderer");
  // The catalog and the custom elements are built once, outside every wait.
  const { discussionCatalog } = await import("./a2ui/catalog");
  discussionCatalog();
});

beforeEach(() => {
  resetLogBufferForTest();
  resetSurfaceModelsForTest();
  resetSurfaceRenderersForTest();
  clearConversationThreads();
  listeners.clear();
  dispatchMock.mockReset();
  openUrlMock.mockClear();
  invokeMock.mockReset();
  handlers = {};
  routeInvoke(invokeMock, handlers);
});

afterEach(() => {
  cleanup();
});

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
    { id: "root", component: "Column", children: ["q", "echo", "note", "send"] },
    { id: "q", component: "Text", text: "Which storage do you want?" },
    { id: "echo", component: "Text", text: { "@call": "formatString", args: { value: "Note: ${/note}" } } },
    { id: "note", component: "TextField", label: "Note", value: { "@path": "/note" } },
    {
      id: "send",
      component: "Button",
      variant: "primary",
      child: "send_label",
      action: { event: { name: "save_note", context: { note: { "@path": "/note" }, kind: "storage" } } },
      checks: [
        { condition: { "@call": "required", args: { value: { "@path": "/note" } } }, message: "Write a note." },
      ],
    },
    { id: "send_label", component: "Text", text: "Send" },
  ] as SurfaceComponent[];
}

function submitted(): AgentSurface {
  return surfaceOf(form(), { note: "" }, {
    state: "submitted",
    submission: {
      name: "save_note",
      sourceComponentId: "send",
      context: { note: "fast", kind: "storage" },
      dataModel: { note: "fast" },
      by: { kind: "human", login: "ada" },
      at: "2026-01-02T10:00:00Z",
      commentId: "c-2",
    },
  });
}

function mount(record: AgentSurface = surfaceOf(form(), { note: "" })) {
  handlers.get_agent_surface = () => record;
  return render(
    <AgentSurfacePanel attachment={{ kind: "agentSurface", surfaceId: record.id }} threadId={THREAD} />,
  );
}

const loaded = () => loadedPanel(screen.getByTestId);

describe("AgentSurfacePanel with the official renderer", () => {
  it("ASF-FR-RRXP / ASF-FR-MGVD: a failed read offers Retry, and the log names the id alone", async () => {
    publish();
    let reads = 0;
    handlers.get_agent_surface = () => {
      reads += 1;
      if (reads === 1) throw new Error("surface_not_found");
      return surfaceOf(form(), { note: "" });
    };
    render(<AgentSurfacePanel attachment={{ kind: "agentSurface", surfaceId: "s-1" }} threadId={THREAD} />);
    expect(await screen.findByText("This panel could not be loaded.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    const panel = await loaded();
    await waitFor(() => expect(panel.textContent).toContain("Which storage do you want?"));
    const warn = logRecords().find((r) => r.message === "agent surface could not be read");
    expect(warn?.fields).toEqual({ surfaceId: "s-1" });
  });

  it("ASF-FR-HRQL / ASF-FR-RTRX: the official renderer draws the log after its chunk loads", async () => {
    publish();
    mount();
    const panel = await loaded();
    await waitFor(() => {
      expect(panel.querySelector("a2ui-basic-column")).not.toBeNull();
      expect(panel.querySelector("a2ui-basic-text")).not.toBeNull();
      expect(panel.querySelector("a2ui-basic-textfield")).not.toBeNull();
    });
    await waitFor(() => expect(panel.textContent).toContain("Which storage do you want?"));
  });

  it("ASF-FR-ENRG / ASF-FR-GKYM: an input writes the data model, a bound text follows, and a check frees the button", async () => {
    publish();
    mount();
    const panel = await loaded();
    await waitFor(() => expect(officialButton(panel, "Send")).toBeDisabled());
    fireEvent.input(await findTextField(panel, "Note"), { target: { value: "fast" } });
    await waitFor(() => expect(panel.textContent).toContain("Note: fast"));
    await waitFor(() => expect(officialButton(panel, "Send")).toBeEnabled());
    expect(panel.textContent).not.toContain("Write a note.");
  });

  it("ASF-FR-GKYM: the message of a false check shows under its button", async () => {
    publish();
    mount();
    const panel = await loaded();
    await waitFor(() => expect(officialButton(panel, "Send")).toBeDisabled());
    await waitFor(() => expect(panel.textContent).toContain("Write a note."));
  });

  it("ASF-FR-JLUJ: the data model stays when the panel mounts again, and each surface has its own", async () => {
    publish();
    const first = mount();
    let panel = await loaded();
    fireEvent.input(await findTextField(panel, "Note"), { target: { value: "kept" } });
    await waitFor(() => expect(panel.textContent).toContain("Note: kept"));
    first.unmount();
    mount();
    panel = await loaded();
    await waitFor(() => expect(panel.textContent).toContain("Note: kept"));
    cleanup();
    mount(surfaceOf(form(), { note: "" }, { id: "s-2" }));
    panel = await loaded();
    await waitFor(() => expect(panel.textContent).toContain("Note:"));
    expect(panel.textContent).not.toContain("Note: kept");
  });

  it("ASF-FR-QBTR / ASF-FR-XOSG / ASF-FR-DHGO: a press sends the resolved action once, the panel waits, and the agents come back", async () => {
    publish();
    let release: (r: unknown) => void = () => {};
    handlers.submit_surface_action = () => new Promise((res) => (release = res));
    mount();
    const panel = await loaded();
    fireEvent.input(await findTextField(panel, "Note"), { target: { value: "fast" } });
    await waitFor(() => expect(officialButton(panel, "Send")).toBeEnabled());
    fireEvent.click(officialButton(panel, "Send"));
    await waitFor(() => expect(invokeMock.mock.calls.some(([c]) => c === "submit_surface_action")).toBe(true));
    fireEvent.click(officialButton(panel, "Send"));
    const calls = invokeMock.mock.calls.filter(([cmd]) => cmd === "submit_surface_action");
    expect(calls).toHaveLength(1);
    expect(calls[0][1]).toEqual({
      surfaceId: "s-1",
      action: {
        name: "save_note",
        sourceComponentId: "send",
        context: { note: "fast", kind: "storage" },
        dataModel: { note: "fast" },
      },
    });
    expect(screen.getByTestId("agent-surface").dataset.state).toBe("sending");
    expect(panel.querySelector("fieldset")).toBeDisabled();
    expect(textFieldInput(panel, "Note")).toBeDisabled();
    expect(officialButton(panel, "Send")).toBeDisabled();
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
    const panel = await loaded();
    expect(panel.querySelector("fieldset")).toBeDisabled();
    await waitFor(() => expect(panel.textContent).toContain("Note: fast"));
    expect(textFieldInput(panel, "Note")).toBeDisabled();
    expect(officialButton(panel, "Send")).toBeDisabled();
    expect(screen.getByText(/Sent “Send” by ada on/)).toBeInTheDocument();
  });

  it("ASF-FR-CUEI / AUI-FR-TYBX: a record whose log grows draws its new components", async () => {
    publish();
    const record = surfaceOf([{ id: "root", component: "Text", text: "First" } as SurfaceComponent]);
    mount(record);
    const panel = await loaded();
    await waitFor(() => expect(panel.textContent).toContain("First"));
    act(() =>
      emitEvent("agent-surface-changed", {
        ...record,
        messages: [
          ...record.messages,
          { version: "1.0", updateComponents: { surfaceId: "s-1", components: [{ id: "root", component: "Text", text: "Second" }] } },
        ],
      }),
    );
    await waitFor(() => expect(screen.getByTestId("agent-surface").textContent).toContain("Second"));
  });

  it("ASF-FR-CUEI: the panel draws the record its own event carries, and no other", async () => {
    publish();
    mount();
    await loaded();
    act(() => emitEvent("agent-surface-changed", { ...submitted(), id: "other" }));
    expect(screen.getByTestId("agent-surface").dataset.state).toBe("open");
    act(() => emitEvent("agent-surface-changed", submitted()));
    await waitFor(() => expect(screen.getByTestId("agent-surface").dataset.state).toBe("submitted"));
  });

  it("ASF-FR-NLJW / ASF-FR-MGVD: an error shows under the panel and frees it; already submitted reads the record again", async () => {
    publish();
    mount();
    const panel = await loaded();
    handlers.submit_surface_action = () => {
      throw new Error("comment_failed");
    };
    fireEvent.input(await findTextField(panel, "Note"), { target: { value: "fast" } });
    await waitFor(() => expect(officialButton(panel, "Send")).toBeEnabled());
    fireEvent.click(officialButton(panel, "Send"));
    expect(await screen.findByRole("alert")).toHaveTextContent("The values were not sent. Try again.");
    expect(panel.querySelector("fieldset")).not.toBeDisabled();
    expect(dispatchMock).not.toHaveBeenCalled();
    handlers.submit_surface_action = () => {
      throw new Error("already_submitted");
    };
    handlers.get_agent_surface = () => submitted();
    fireEvent.click(officialButton(panel, "Send"));
    await waitFor(() => expect(screen.getByTestId("agent-surface").dataset.state).toBe("submitted"));
    const warns = logRecords().filter((r) => r.message === "agent surface action failed");
    expect(warns.map((w) => w.fields)).toEqual([
      { surfaceId: "s-1", error: "comment_failed" },
      { surfaceId: "s-1", error: "already_submitted" },
    ]);
    expect(JSON.stringify(logRecords())).not.toContain("fast");
  });

  it("ASF-FR-JSTI: a panel in a resolved discussion is read-only with no sent line", async () => {
    publish(true);
    mount();
    const panel = await loaded();
    expect(screen.getByTestId("agent-surface").dataset.state).toBe("resolved");
    expect(panel.querySelector("fieldset")).toBeDisabled();
    await waitFor(() => expect(textFieldInput(panel, "Note")).toBeDisabled());
    expect(screen.queryByText(/^Sent /)).not.toBeInTheDocument();
  });

  it("ASF-FR-QXYW: Text renders Markdown with no raw HTML, and a link opens through the opener", async () => {
    publish();
    mount(
      surfaceOf([
        {
          id: "root",
          component: "Text",
          text: "**Bold** and [the spec](https://a2ui.org) <img src=x onerror=alert(1)>",
        } as SurfaceComponent,
      ]),
    );
    const panel = await loaded();
    await waitFor(() => expect(panel.querySelector("strong")?.textContent).toBe("Bold"));
    expect(panel.querySelector("img")).toBeNull();
    fireEvent.click(panel.querySelector("a[title]") as HTMLAnchorElement);
    expect(openUrlMock).toHaveBeenCalledWith("https://a2ui.org");
  });

  it("ASF-FR-NOBK / ASF-FR-IKTX: media load https as it is, project files through the backend, and refuse other sources", async () => {
    publish();
    handlers.read_surface_media = (args) =>
      args.source === "project:docs/a.png"
        ? { mediaType: "image/png", data: "iVBORw0K" }
        : Promise.reject(new Error("media_not_found"));
    mount(
      surfaceOf([
        { id: "root", component: "Column", children: ["web", "local", "gone", "clip"] },
        { id: "web", component: "Image", url: "https://example.com/a.png" },
        { id: "local", component: "Image", url: "project:docs/a.png" },
        { id: "gone", component: "Image", url: "project:docs/missing.png" },
        { id: "clip", component: "Video", url: "https://example.com/a.mp4" },
      ] as SurfaceComponent[]),
    );
    const panel = await loaded();
    await waitFor(() => expect(panel.querySelectorAll("img")).toHaveLength(2));
    const sources = [...panel.querySelectorAll("img")].map((i) => i.getAttribute("src"));
    expect(sources).toEqual(["https://example.com/a.png", "data:image/png;base64,iVBORw0K"]);
    expect(await screen.findByText("This media cannot be shown.")).toBeInTheDocument();
    const video = panel.querySelector("video") as HTMLVideoElement;
    expect(video).toHaveAttribute("controls");
    expect(video.autoplay).toBe(false);
    expect(invokeMock).toHaveBeenCalledWith("read_surface_media", { surfaceId: "s-1", source: "project:docs/a.png" });
  });

  it("ASF-FR-NEPE: a Modal opens its content in the application's dialog, and Escape closes it", async () => {
    publish();
    mount(
      surfaceOf([
        { id: "root", component: "Modal", trigger: "open", content: "body" },
        { id: "open", component: "Button", child: "open_label", action: { event: { name: "noop" } } },
        { id: "open_label", component: "Text", text: "Details" },
        { id: "body", component: "Text", text: "The detail text." },
      ] as SurfaceComponent[]),
    );
    const panel = await loaded();
    await waitFor(() => expect(officialButton(panel, "Details")).toBeInTheDocument());
    fireEvent.click(officialButton(panel, "Details"));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("The detail text.");
    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(invokeMock.mock.calls.some(([c]) => c === "submit_surface_action")).toBe(false);
    // The focus is inside the dialog, and goes back to the trigger.
    await waitFor(() => expect(dialog.contains(document.activeElement)).toBe(true));
    fireEvent.keyDown(dialog, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await waitFor(() => expect(document.activeElement).toBe(officialButton(panel, "Details")));
    fireEvent.click(officialButton(panel, "Details"));
    fireEvent.click(await screen.findByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("ASF-FR-VZCC: a Button with openUrl opens the address through the opener and sends nothing", async () => {
    publish();
    mount(
      surfaceOf([
        {
          id: "root",
          component: "Button",
          child: "l",
          action: { functionCall: { "@call": "openUrl", args: { url: "https://a2ui.org" } } },
        },
        { id: "l", component: "Text", text: "Open the spec" },
      ] as SurfaceComponent[]),
    );
    const panel = await loaded();
    await waitFor(() => expect(officialButton(panel, "Open the spec")).toBeInTheDocument());
    fireEvent.click(officialButton(panel, "Open the spec"));
    await waitFor(() => expect(openUrlMock).toHaveBeenCalledWith("https://a2ui.org"));
    expect(invokeMock.mock.calls.some(([c]) => c === "submit_surface_action")).toBe(false);
  });

  it("ASF-FR-GZFO / ASF-FR-TAVD: a table has header cells and a row per object", async () => {
    publish();
    mount(
      surfaceOf(
        [
          {
            id: "root",
            component: "Table",
            caption: "Options",
            columns: [
              { key: "name", title: "Name" },
              { key: "cost", title: "Cost" },
            ],
            rows: { "@path": "/rows" },
          } as SurfaceComponent,
        ],
        { rows: [{ name: "SQLite", cost: 0 }, { name: "Postgres" }] },
      ),
    );
    await loaded();
    await waitFor(() =>
      expect(screen.getAllByRole("columnheader").map((h) => h.textContent)).toEqual(["Name", "Cost"]),
    );
    expect(screen.getAllByRole("cell").map((c) => c.textContent)).toEqual(["SQLite", "0", "Postgres", ""]);
  });

  it("ASF-FR-ASQV / CMT-FR-DBQC: an agent_surface attachment draws the panel under the comment, with no chip", async () => {
    publish();
    handlers.get_agent_surface = () => surfaceOf(form(), { note: "" });
    render(<CommentAttachmentList threadId={THREAD} attachments={[{ kind: "agentSurface", surfaceId: "s-1" }]} />);
    const panel = await loaded();
    await waitFor(() => expect(panel.textContent).toContain("Which storage do you want?"));
    expect(screen.queryByTestId("attachment-chip")).not.toBeInTheDocument();
  });

  it("ASF-FR-TURS / ASF-FR-GHUZ: a template draws each item in order, and a call that fails gives an empty value while the panel draws", async () => {
    publish();
    mount(
      surfaceOf(
        [
          { id: "root", component: "Column", children: ["list", "broken"] },
          { id: "list", component: "List", children: { componentId: "item", path: "/items" } },
          { id: "item", component: "Text", text: { "@path": "name" } },
          {
            id: "broken",
            component: "Text",
            text: { "@call": "formatDate", args: { value: "never", format: "yyyy" } },
          },
        ] as SurfaceComponent[],
        { items: [{ name: "one" }, { name: "two" }, { name: "three" }] },
      ),
    );
    const panel = await loaded();
    await waitFor(() => expect(panel.textContent).toMatch(/one[\s\S]*two[\s\S]*three/));
    expect(panel.querySelectorAll("a2ui-basic-text").length).toBeGreaterThanOrEqual(4);
  });

  it("ASF-FR-SERF: a component the renderer does not know draws its line, and the panel still shows", async () => {
    publish();
    mount(
      surfaceOf([
        { id: "root", component: "Column", children: ["known", "strange"] },
        { id: "known", component: "Text", text: "Still here" },
        { id: "strange", component: "Html", html: "<b>x</b>" },
      ] as SurfaceComponent[]),
    );
    const panel = await loaded();
    await waitFor(() => expect(panel.textContent).toContain("This part cannot be shown."));
    expect(panel.textContent).toContain("Still here");
    expect(panel.querySelector("b")).toBeNull();
  });

  it("ASF-FR-QBTR: a bound context value with no value in the data model is sent as null", async () => {
    publish();
    handlers.submit_surface_action = () => new Promise(() => {});
    mount(
      surfaceOf([
        {
          id: "root",
          component: "Button",
          child: "l",
          action: { event: { name: "go", context: { missing: { "@path": "/nothing" }, kind: "x" } } },
        },
        { id: "l", component: "Text", text: "Go" },
      ] as SurfaceComponent[]),
    );
    const panel = await loaded();
    await waitFor(() => expect(officialButton(panel, "Go")).toBeEnabled());
    fireEvent.click(officialButton(panel, "Go"));
    await waitFor(() => expect(invokeMock.mock.calls.some(([c]) => c === "submit_surface_action")).toBe(true));
    const call = invokeMock.mock.calls.find(([c]) => c === "submit_surface_action");
    expect((call?.[1] as { action: { context: unknown } }).action.context).toEqual({ missing: null, kind: "x" });
  });
});

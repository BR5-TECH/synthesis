/**
 * The parts of the surface panel that the first panel file leaves open
 * (`ASF-agent-surfaces.md`): links that are refused, media sources, the
 * dialog's focus and overlay rule, templates, failed calls, and one submission
 * for two presses in one task.
 *
 * The backend is a strict `invoke` mock routed by command name. The official
 * controls are custom elements, so each check waits for the state it reads.
 */
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
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
import { resetSurfaceModelsForTest } from "./model";
import { resetSurfaceRenderersForTest } from "./a2ui/SurfaceRenderer";
import { clearConversationThreads, publishThread } from "../../state/conversationThreads";
import { flushLogs, resetLogBufferForTest } from "../../logging";
import { setModalOpening } from "../../state/modalOpening";
import {
  THREAD,
  findButton,
  loadedPanel,
  routeInvoke,
  surfaceOf,
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

// ASF-FR-RTRX: the chunk and the catalog are loaded once, outside every wait.
beforeAll(async () => {
  await import("./a2ui/SurfaceRenderer");
  const { discussionCatalog } = await import("./a2ui/catalog");
  discussionCatalog();
});

beforeEach(() => {
  resetLogBufferForTest();
  resetSurfaceModelsForTest();
  resetSurfaceRenderersForTest();
  clearConversationThreads();
  setModalOpening(null);
  dispatchMock.mockReset();
  openUrlMock.mockClear();
  invokeMock.mockReset();
  handlers = {};
  routeInvoke(invokeMock, handlers);
  publishThread({
    id: THREAD,
    target: { kind: "draft", draftId: "draft-1" },
    resolved: false,
    comments: [],
  } as unknown as Discussion);
});

afterEach(() => {
  cleanup();
  setModalOpening(null);
});

function mount(record: AgentSurface) {
  handlers.get_agent_surface = () => record;
  return render(
    <AgentSurfacePanel attachment={{ kind: "agentSurface", surfaceId: record.id }} threadId={THREAD} />,
  );
}

const loaded = () => loadedPanel(screen.getByTestId);

function submitCalls() {
  return invokeMock.mock.calls.filter(([cmd]) => cmd === "submit_surface_action").map(([, a]) => a);
}

const DIGEST = "ab".repeat(32);

describe("the surface panel, further parts", () => {
  it("ASF-FR-QXYW / ASF-FR-MGVD: a link press never navigates, and a javascript link opens nothing and logs its scheme alone", async () => {
    mount(
      surfaceOf([
        {
          id: "root",
          component: "Text",
          text: "[good](https://a2ui.org) and [bad](javascript:alert(1))",
        } as SurfaceComponent,
      ]),
    );
    const panel = await loaded();
    const good = await waitFor(() => {
      const anchor = panel.querySelector<HTMLAnchorElement>('a[title="https://a2ui.org"], a[href="https://a2ui.org"]');
      expect(anchor).not.toBeNull();
      return anchor as HTMLAnchorElement;
    });
    // fireEvent returns false when a handler prevented the default action.
    expect(fireEvent.click(good)).toBe(false);
    expect(openUrlMock).toHaveBeenCalledTimes(1);
    expect(openUrlMock).toHaveBeenCalledWith("https://a2ui.org");
    // The application's Markdown draws the link inert, with its address in `title`.
    const bad = [...panel.querySelectorAll("a")].find((a) => a.textContent === "bad") as HTMLAnchorElement;
    expect(bad).toBeDefined();
    expect(fireEvent.click(bad)).toBe(false);
    expect(openUrlMock).toHaveBeenCalledTimes(1);
    const refused = logRecords().filter((r) => r.message === "agent surface link refused");
    expect(refused.map((r) => r.fields)).toEqual([{ scheme: "javascript" }]);
    expect(JSON.stringify(logRecords())).not.toContain("alert");
    expect(panel.querySelector('a[href^="javascript"]')).toBeNull();
  });

  it("ASF-FR-NOBK: a source that is not https, project, or attachment shows the line and reads nothing", async () => {
    mount(
      surfaceOf([
        { id: "root", component: "Column", children: ["plain", "data"] },
        { id: "plain", component: "Image", url: "http://example.com/a.png" },
        { id: "data", component: "Image", url: "data:image/png;base64,AAAA" },
      ] as SurfaceComponent[]),
    );
    const panel = await loaded();
    await waitFor(() => expect(screen.getAllByText("This media cannot be shown.")).toHaveLength(2));
    expect(panel.querySelector("img")).toBeNull();
    expect(invokeMock.mock.calls.some(([c]) => c === "read_surface_media")).toBe(false);
  });

  it("ASF-FR-NOBK / ASF-FR-IKTX: an attachment reads through the backend, and an AudioPlayer plays on a press alone", async () => {
    handlers.read_surface_media = (args) =>
      args.source === `attachment:${DIGEST}`
        ? { mediaType: "audio/mpeg", data: "SUQz" }
        : Promise.reject(new Error("media_not_found"));
    mount(
      surfaceOf([
        { id: "root", component: "Column", children: ["clip", "web"] },
        { id: "clip", component: "AudioPlayer", url: `attachment:${DIGEST}`, description: "The call" },
        { id: "web", component: "AudioPlayer", url: "https://example.com/a.mp3" },
      ] as SurfaceComponent[]),
    );
    const panel = await loaded();
    await waitFor(() => expect(panel.querySelectorAll("audio")).toHaveLength(2));
    const [clip, web] = [...panel.querySelectorAll("audio")];
    expect(clip.getAttribute("src")).toBe("data:audio/mpeg;base64,SUQz");
    expect(clip).toHaveAttribute("aria-label", "The call");
    expect(web.getAttribute("src")).toBe("https://example.com/a.mp3");
    for (const audio of [clip, web]) {
      expect(audio).toHaveAttribute("controls");
      expect(audio.autoplay).toBe(false);
    }
    expect(invokeMock).toHaveBeenCalledWith("read_surface_media", {
      surfaceId: "s-1",
      source: `attachment:${DIGEST}`,
    });
  });

  it("ASF-FR-MGVD: a failed media read logs the id and the code, never the source", async () => {
    handlers.read_surface_media = () => Promise.reject(new Error("media_not_found"));
    mount(surfaceOf([{ id: "root", component: "Image", url: "project:secret/plan.png" } as SurfaceComponent]));
    await loaded();
    expect(await screen.findByText("This media cannot be shown.")).toBeInTheDocument();
    const warns = logRecords().filter((r) => r.message === "agent surface media could not be read");
    expect(warns.map((w) => w.fields)).toEqual([{ surfaceId: "s-1", error: "media_not_found" }]);
    expect(JSON.stringify(logRecords())).not.toContain("plan.png");
  });

  it("ASF-FR-NEPE: the dialog closes the other overlays, and Tab keeps the focus inside it", async () => {
    const closeOthers = vi.fn();
    setModalOpening(closeOthers);
    mount(
      surfaceOf([
        { id: "root", component: "Modal", trigger: "open", content: "body" },
        { id: "open", component: "Button", child: "open_label", action: { event: { name: "noop" } } },
        { id: "open_label", component: "Text", text: "Details" },
        { id: "body", component: "Column", children: ["first", "second"] },
        { id: "first", component: "Button", child: "first_label", action: { event: { name: "one" } } },
        { id: "first_label", component: "Text", text: "One" },
        { id: "second", component: "Button", child: "second_label", action: { event: { name: "two" } } },
        { id: "second_label", component: "Text", text: "Two" },
      ] as SurfaceComponent[]),
    );
    const panel = await loaded();
    fireEvent.click(await findButton(panel, "Details"));
    const dialog = await screen.findByRole("dialog");
    expect(closeOthers).toHaveBeenCalledTimes(1);
    const close = await waitFor(() => {
      const button = [...dialog.querySelectorAll("button")].find((b) => b.textContent === "Close");
      expect(button).toBeDefined();
      return button as HTMLButtonElement;
    });
    await findButton(dialog, "Two");
    // Tab from the last control goes to the first, and Shift+Tab from the first to the last.
    close.focus();
    fireEvent.keyDown(window, { key: "Tab" });
    expect(dialog.contains(document.activeElement)).toBe(true);
    expect(document.activeElement).not.toBe(close);
    const first = document.activeElement as HTMLElement;
    fireEvent.keyDown(window, { key: "Tab", shiftKey: true });
    expect(document.activeElement).toBe(close);
    expect(first).not.toBe(close);
  });

  it("ASF-FR-TURS / ASF-FR-GHUZ: @index and a relative pointer draw each item of a Row template", async () => {
    mount(
      surfaceOf(
        [
          { id: "root", component: "Row", children: { componentId: "item", path: "/items" } },
          {
            id: "item",
            component: "Text",
            text: {
              "@call": "formatString",
              args: { value: "${@index(offset: 1)}. ${name}" },
            },
          },
        ] as SurfaceComponent[],
        { items: [{ name: "alpha" }, { name: "beta" }] },
      ),
    );
    const panel = await loaded();
    await waitFor(() => expect(panel.textContent).toMatch(/alpha[\s\S]*beta/));
    const texts = [...panel.querySelectorAll("a2ui-basic-text")].map((t) => t.textContent?.trim());
    expect(texts).toContain("1. alpha");
    expect(texts).toContain("2. beta");
    expect(panel.querySelector("a2ui-basic-row")).not.toBeNull();
  });

  it("ASF-FR-GHUZ: a call that names an undeclared function, or that fails, draws an empty text, and the panel still draws", async () => {
    mount(
      surfaceOf([
        { id: "root", component: "Column", children: ["unknown", "broken", "after"] },
        { id: "unknown", component: "Text", text: { "@call": "notAFunction", args: {} } },
        // An argument that the function does not declare makes the official call throw.
        {
          id: "broken",
          component: "Text",
          text: { "@call": "formatDate", args: { value: "2026-01-02", format: "yyyy", extra: 1 } },
        },
        { id: "after", component: "Text", text: "After the calls" },
      ] as SurfaceComponent[]),
    );
    const panel = await loaded();
    await waitFor(() => expect(panel.textContent).toContain("After the calls"));
    const texts = [...panel.querySelectorAll("a2ui-basic-text")].map((t) => t.textContent?.trim());
    expect(texts).toEqual(["", "", "After the calls"]);
    expect(panel.textContent).not.toContain("2026");
    expect(panel.textContent).not.toContain("This part cannot be shown.");
    expect(panel.querySelector(".agent-surface__error")).toBeNull();
  });

  it("ASF-FR-QBTR / ASF-FR-TURS / AUI-FR-AFKN: a Button in a template sends the value of its own item", async () => {
    handlers.submit_surface_action = () => new Promise(() => {});
    mount(
      surfaceOf(
        [
          { id: "root", component: "List", children: { componentId: "pick", path: "/items" } },
          {
            id: "pick",
            component: "Button",
            child: "pick_label",
            action: { event: { name: "pick", context: { name: { "@path": "name" } } } },
          },
          { id: "pick_label", component: "Text", text: { "@path": "name" } },
        ] as SurfaceComponent[],
        { items: [{ name: "one" }, { name: "two" }] },
      ),
    );
    const panel = await loaded();
    fireEvent.click(await findButton(panel, "two"));
    await waitFor(() => expect(submitCalls()).toHaveLength(1));
    expect((submitCalls()[0] as { action: { context: unknown; name: string } }).action).toMatchObject({
      name: "pick",
      sourceComponentId: "pick",
      context: { name: "two" },
    });
  });

  it("ASF-FR-XOSG: two presses in one task send one submission", async () => {
    handlers.submit_surface_action = () => new Promise(() => {});
    mount(
      surfaceOf([
        { id: "root", component: "Button", child: "l", action: { event: { name: "go" } } },
        { id: "l", component: "Text", text: "Go" },
      ] as SurfaceComponent[]),
    );
    const panel = await loaded();
    const go = await findButton(panel, "Go");
    await waitFor(() => expect(go).toBeEnabled());
    // Both presses run before React draws the pending state.
    act(() => {
      go.click();
      go.click();
    });
    await waitFor(() => expect(screen.getByTestId("agent-surface").dataset.state).toBe("sending"));
    expect(submitCalls()).toHaveLength(1);
  });
});

/**
 * The lazy load of the official renderer (`ASF-agent-surfaces.md`
 * ASF-FR-RTRX).
 *
 * The renderer module is a mock whose load waits on a gate that the test
 * opens. Thus the test sees the panel before and after the chunk loads, with
 * no dependency on how fast the real chunk loads.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";

const chunk = vi.hoisted(() => {
  let open: () => void = () => {};
  const gate = new Promise<void>((resolve) => (open = resolve));
  return { gate, open: () => open(), requested: false };
});

vi.mock("./a2ui/SurfaceRenderer", async () => {
  chunk.requested = true;
  await chunk.gate;
  return {
    default: () => <p>The renderer drew the panel.</p>,
    resetSurfaceRenderersForTest: () => {},
  };
});

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { AgentSurfacePanel } from ".";
import { resetSurfaceModelsForTest } from "./model";
import { clearConversationThreads } from "../../state/conversationThreads";
import { resetLogBufferForTest } from "../../logging";
import { THREAD, routeInvoke, surfaceOf, type Handler } from "../../test/agentSurfaceSupport";
import type { SurfaceComponent } from "../../types";

let handlers: Record<string, Handler> = {};

beforeEach(() => {
  resetLogBufferForTest();
  resetSurfaceModelsForTest();
  clearConversationThreads();
  invokeMock.mockReset();
  handlers = {};
  routeInvoke(invokeMock, handlers);
});

afterEach(() => {
  cleanup();
});

describe("the renderer chunk", () => {
  it("ASF-FR-RTRX: the chunk loads with the first panel, and the panel shows a placeholder until it is loaded", async () => {
    expect(chunk.requested).toBe(false);
    handlers.get_agent_surface = () => surfaceOf([{ id: "root", component: "Text", text: "Hi" } as SurfaceComponent]);
    render(<AgentSurfacePanel attachment={{ kind: "agentSurface", surfaceId: "s-1" }} threadId={THREAD} />);
    // The record is read, and the panel waits for the chunk.
    await screen.findByText("Loading the panel…");
    await vi.waitFor(() => expect(screen.getByTestId("agent-surface").dataset.state).toBe("open"));
    expect(chunk.requested).toBe(true);
    expect(screen.getByText("Loading the panel…")).toBeInTheDocument();
    expect(screen.queryByText("The renderer drew the panel.")).not.toBeInTheDocument();
    await act(async () => chunk.open());
    expect(await screen.findByText("The renderer drew the panel.")).toBeInTheDocument();
    expect(screen.queryByText("Loading the panel…")).not.toBeInTheDocument();
  });
});

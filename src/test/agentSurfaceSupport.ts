/**
 * The fixtures of the surface panel tests (`ASF-agent-surfaces.md`). Each
 * test file mocks the Tauri modules itself; this module holds the records,
 * the strict `invoke` routing, and the waits on the official renderer, whose
 * controls are custom elements that update after a microtask.
 */
import { waitFor } from "@testing-library/react";
import { expect } from "vitest";
import type { Mock } from "vitest";

import type { AgentSurface, SurfaceComponent } from "../types";

export type Handler = (args: Record<string, unknown>) => unknown;

export const THREAD = "thread-1";

/** A strict invoke: the log flush answers, any other unknown command rejects. */
export function routeInvoke(invokeMock: Mock, handlers: Record<string, Handler>): void {
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    if (cmd === "append_log_records") return undefined;
    const handler = handlers[cmd];
    if (!handler) throw new Error(`unexpected command ${cmd}`);
    return handler(args ?? {});
  });
}

/** A record whose log is one createSurface of `components` and `dataModel`. */
export function surfaceOf(
  components: SurfaceComponent[],
  dataModel: Record<string, unknown> = {},
  over: Partial<AgentSurface> = {},
): AgentSurface {
  const id = over.id ?? "s-1";
  return {
    id,
    discussionId: THREAD,
    target: { kind: "draft", draftId: "draft-1" },
    commentId: "c-1",
    catalogId: "synthesis:a2ui/v1",
    messages: [
      { version: "1.0", createSurface: { surfaceId: id, catalogId: "synthesis:a2ui/v1", components, dataModel } },
    ],
    authoredBy: { kind: "agent", agentId: "a-1", handle: "arch" },
    createdAt: "2026-01-01T00:00:00Z",
    state: "open",
    ...over,
  } as AgentSurface;
}

/** The panel, once its record and its renderer have loaded. */
export async function loadedPanel(getByTestId: (id: string) => HTMLElement): Promise<HTMLElement> {
  await waitFor(() => {
    const panel = getByTestId("agent-surface");
    expect(panel.dataset.state).not.toBe("loading");
    expect(panel.querySelector(".agent-surface__loading")).toBeNull();
    // The official renderer fills the panel in a later render, after the lazy
    // chunk loads. On a slow runner a test that reads the panel before then finds
    // it empty, so the panel counts as loaded once its root element is there.
    // A React card renders its options with its root. Nested content of a Lit
    // root comes later, and a test of it still waits with `waitFor`.
    const content = panel.querySelector(".agent-surface__fieldset");
    if (content) expect(content.childElementCount).toBeGreaterThan(0);
  });
  return getByTestId("agent-surface");
}

/** The input of the official TextField whose label reads `label`. */
export function textFieldInput(root: ParentNode, label: string): HTMLInputElement {
  const field = [...root.querySelectorAll("a2ui-basic-textfield")].find(
    (f) => f.querySelector("label")?.textContent?.trim() === label,
  );
  const input = field?.querySelector("input, textarea");
  if (!input) throw new Error(`no TextField labelled ${label}`);
  return input as HTMLInputElement;
}

/** The button of the official Button whose text reads `text`. */
export function officialButton(root: ParentNode, text: string): HTMLButtonElement {
  const button = [...root.querySelectorAll("button")].find(
    (b) => b.textContent?.trim() === text,
  );
  if (!button) throw new Error(`no Button ${text}`);
  return button as HTMLButtonElement;
}

/** The input of a TextField, once the official element has drawn it. */
export function findTextField(root: ParentNode, label: string): Promise<HTMLInputElement> {
  return waitFor(() => textFieldInput(root, label));
}

/** A Button, once it has drawn. */
export function findButton(root: ParentNode, text: string): Promise<HTMLButtonElement> {
  return waitFor(() => officialButton(root, text));
}

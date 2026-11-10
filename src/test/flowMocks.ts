/**
 * The mocks of `@tauri-apps/api/core` and `@tauri-apps/api/event` that the
 * Flow canvas tests share. This module imports no application code, so a
 * `vi.mock` factory can import it without a cycle:
 *
 * ```ts
 * vi.mock("@tauri-apps/api/core", async () => ({
 *   invoke: (await import("../test/flowMocks")).invoke,
 * }));
 * ```
 */
import { vi } from "vitest";

export const invokeMock = vi.fn();

/** What the mocked `invoke` calls. */
export const invoke = (...args: unknown[]) => invokeMock(...args);

/** Every listener a test registered through the mocked `listen`, by event. */
export const listeners = new Map<string, Array<(ev: { payload: unknown }) => void>>();

export const listen = vi.fn(async (name: string, cb: (ev: { payload: unknown }) => void) => {
  const list = listeners.get(name) ?? [];
  list.push(cb);
  listeners.set(name, list);
  return () => {
    listeners.set(
      name,
      (listeners.get(name) ?? []).filter((x) => x !== cb),
    );
  };
});

/** Fire a backend event to every listener of it. */
export function emitEvent(name: string, payload: unknown = {}) {
  for (const cb of listeners.get(name) ?? []) cb({ payload });
}

/** Drop the listeners of the last test. Call in `beforeEach`. */
export function resetListeners() {
  listeners.clear();
}

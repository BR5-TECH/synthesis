type Handler = (e: { payload?: unknown }) => void;

const handlers: Record<string, Handler[]> = {};

export async function listen(name: string, handler: Handler) {
  (handlers[name] ??= []).push(handler);
  return () => {
    handlers[name] = (handlers[name] ?? []).filter((h) => h !== handler);
  };
}

export async function once(name: string, handler: Handler) {
  return listen(name, handler);
}

export async function emit() {}

// Test hook: fire a backend event from Playwright.
(globalThis as Record<string, unknown>).__fireBusEvent = (
  name: string,
  payload?: unknown,
) => {
  for (const h of [...(handlers[name] ?? [])]) h({ payload });
};

export type UnlistenFn = () => void;

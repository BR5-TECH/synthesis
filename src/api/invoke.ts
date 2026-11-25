/**
 * The one `invoke` the frontend calls the backend through (`PWN-project-windows.md`
 * PWN-FR-ISOL).
 *
 * A project window knows its project id. This wrapper adds it to the payload of
 * every call as `projectId`, so the backend serves the call from the state of
 * that project and from no other. A window that holds no project (the Project
 * picker, the Settings window) sends the payload unchanged.
 */
import { invoke as coreInvoke, type InvokeArgs, type InvokeOptions } from "@tauri-apps/api/core";

/** The name of the payload field that holds the project id. */
export const PROJECT_ID_FIELD = "projectId";

let windowProjectId: string | null = null;
let payloadProjectIdEnabled = true;

/**
 * Switch the payload field off. The test setup switches it off, so a test of
 * one feature compares the arguments of the feature alone. The tests of the
 * wrapper switch it on again.
 */
export function setPayloadProjectIdEnabled(enabled: boolean): void {
  payloadProjectIdEnabled = enabled;
}

/** Set the project id of this window, or clear it with `null`. */
export function setWindowProjectId(projectId: string | null): void {
  windowProjectId = projectId;
}

/** The project id of this window, or `null` while the window holds no project. */
export function getWindowProjectId(): string | null {
  return windowProjectId;
}

function isPayloadObject(args: InvokeArgs | undefined): args is Record<string, unknown> {
  return (
    args !== undefined &&
    args !== null &&
    typeof args === "object" &&
    !Array.isArray(args) &&
    !(args instanceof ArrayBuffer) &&
    !(args instanceof Uint8Array)
  );
}

/**
 * CEX-FR-QZPN: the order key of a command whose later call must win, or
 * `null` for a command whose calls may run at the same time. A `save_` command
 * stores a whole state, so the last call is the state to keep.
 *
 * The backend runs each command on its own worker, so two calls sent without
 * waiting can finish in either order (`CEX-command-execution.md`
 * CEX-FR-FUUP). The calls of one key go out one at a time, so the later write
 * always lands last.
 */
const PROJECT_WINDOW_LIST = "project window list";
const STATE_WRITE = /^save_/;
export function orderKeyOf(cmd: string): string | null {
  if (cmd === "activate_project_window" || cmd === "set_project_window_attention") {
    return PROJECT_WINDOW_LIST;
  }
  return STATE_WRITE.test(cmd) ? cmd : null;
}

/** The last call of each order key that has not settled yet. */
const tails = new Map<string, Promise<unknown>>();

/** Test seam: forget the calls that a test left unsettled. */
export function resetInvokeOrderForTest(): void {
  tails.clear();
}

function inOrder<T>(key: string, call: () => Promise<T>): Promise<T> {
  const before = tails.get(key);
  // With no call before it, the call goes out now, in the caller's tick.
  const next = before ? before.then(call, call) : call();
  const tail = Promise.resolve(next).catch(() => undefined);
  tails.set(key, tail);
  void tail.then(() => {
    if (tails.get(key) === tail) tails.delete(key);
  });
  return next;
}

export function invoke<T>(
  cmd: string,
  ...rest: [args?: InvokeArgs, options?: InvokeOptions]
): Promise<T> {
  const key = orderKeyOf(cmd);
  if (key === null) return send<T>(cmd, rest);
  return inOrder(key, () => send<T>(cmd, rest));
}

function send<T>(cmd: string, rest: [args?: InvokeArgs, options?: InvokeOptions]): Promise<T> {
  const [args, options] = rest;
  if (!payloadProjectIdEnabled || windowProjectId === null || (args !== undefined && !isPayloadObject(args))) {
    return coreInvoke<T>(cmd, ...rest);
  }
  // A call that names its own project (`activate project window`) keeps it.
  return coreInvoke<T>(cmd, { [PROJECT_ID_FIELD]: windowProjectId, ...(args ?? {}) }, options);
}

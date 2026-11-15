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

export function invoke<T>(
  cmd: string,
  ...rest: [args?: InvokeArgs, options?: InvokeOptions]
): Promise<T> {
  const [args, options] = rest;
  if (!payloadProjectIdEnabled || windowProjectId === null || (args !== undefined && !isPayloadObject(args))) {
    return coreInvoke<T>(cmd, ...rest);
  }
  // A call that names its own project (`activate project window`) keeps it.
  return coreInvoke<T>(cmd, { [PROJECT_ID_FIELD]: windowProjectId, ...(args ?? {}) }, options);
}

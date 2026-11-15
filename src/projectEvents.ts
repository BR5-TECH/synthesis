import { listen, type EventCallback, type UnlistenFn } from "@tauri-apps/api/event";
import { getWindowProjectId } from "./api/invoke";

/**
 * PWN-FR-AINZ: `listen` for an event of one project. The backend puts the id of
 * the project in the `projectId` field of the payload. An event with the id of
 * another project is dropped, so no project window applies the event of
 * another project. An event with no id, and a window with no project, pass.
 */
export function listenProject<T>(
  event: string,
  handler: EventCallback<T>,
): Promise<UnlistenFn> {
  return listen<T>(event, (ev) => {
    const own = getWindowProjectId();
    const payload = ev.payload as { projectId?: unknown } | null;
    const sent =
      payload !== null && typeof payload === "object" ? payload.projectId : undefined;
    if (own !== null && typeof sent === "string" && sent !== own) return;
    handler(ev);
  });
}

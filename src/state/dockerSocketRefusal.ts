/**
 * A start or a merge that the backend refuses for a missing Docker socket
 * (`../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-BXKR,
 * `NTF-notifications.md` NTF-FR-24).
 *
 * The surface that met the refusal reports it here, and the main window's one
 * notification surface (`../hooks/useAppNotifications.ts`) raises the Error toast.
 * A raise is made from that file alone, so the dialog and the stream list only
 * report.
 */
import type { Raise } from "./notifications";
import { mintAddress } from "./notificationAddress";

/** NTF-FR-07: one slot, so a retry replaces the toast instead of stacking one. */
export const DOCKER_SOCKET_TOAST_KEY = "docker-socket-unavailable";

type Listener = (path: string | null) => void;
const listeners = new Set<Listener>();

/** The raising surface listens here. Returns the way to stop. */
export function onDockerSocketRefusal(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/** Called by a surface that was refused with `docker_socket_unavailable`. */
export function reportDockerSocketRefusal(path: string | null): void {
  for (const listener of [...listeners]) listener(path);
}

/**
 * GSD-FR-BXKR: the raise for one refusal. The toast addresses the Docker section
 * of the Project group of the Settings window, where the socket path is
 * corrected.
 */
export function dockerSocketRaise(
  path: string | null,
  projectKey: string,
  worktree: string,
): Raise {
  return {
    key: DOCKER_SOCKET_TOAST_KEY,
    // GSD-FR-BXKR: the refusal is Error.
    level: "Error",
    title: "Docker socket not available",
    body: path
      ? `Start Docker or restore the socket at ${path}, or correct the path in Project settings.`
      : "Start Docker or restore the socket, or correct the path in Project settings.",
    address: mintAddress(projectKey, worktree, {
      kind: "settings",
      scope: "project",
      section: "docker",
    }),
  };
}

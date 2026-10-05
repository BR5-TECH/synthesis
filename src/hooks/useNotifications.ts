/**
 * Activation routing (`NTF-notifications.md` NTF-FR-16 through NTF-FR-20).
 *
 * The window has already been raised and focused by the time an activation
 * arrives (`../core/NTD-notification-delivery.md` NTD-FR-09), so nothing here
 * raises it — this decides *where in the window* the author lands, and says so
 * when the answer is nowhere.
 *
 * [`resolveActivation`] is the whole decision, as a pure function of the address
 * and a snapshot of what is open. Every branch of NTF-FR-17 and NTF-FR-19 is
 * therefore testable without a shell, a backend, or a notification centre; the
 * hook below only performs what it returns.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { onNotificationActivated } from "../events";
import { logInfo } from "../logging";
import {
  describeTarget,
  parseAddress,
  type NotificationTarget,
} from "../state/notificationAddress";
import { forgetPosted } from "../state/notifications";

/** What the shell knows about the open project when an activation arrives. */
export interface ActivationContext {
  projectKey: string | null;
  worktree: string | null;
  /** Whether a project file still resolves — deleted files route nowhere. */
  fileExists: (path: string) => boolean;
  /** Whether a draft still exists — an archived or deleted one routes nowhere. */
  draftExists: (draftId: string) => boolean;
}

/** What the shell should do about an activation. */
export type ActivationOutcome =
  | { kind: "open"; target: NotificationTarget }
  /** NTF-FR-19: change nothing, and state what could not be reached. */
  | { kind: "state"; message: string };

/**
 * NTF-FR-17 / NTF-FR-19: where an activated address lands the author.
 *
 * The refusals are deliberately total. An address naming another project,
 * another worktree, or a target that no longer resolves changes **nothing** —
 * no project is switched, no worktree is switched, no tab is closed, and no
 * preference is written on the author's behalf. A notification clicked an hour
 * later must not rearrange the workspace out from under whatever the author has
 * been doing since.
 */
export function resolveActivation(
  payload: string,
  context: ActivationContext,
): ActivationOutcome {
  const address = parseAddress(payload);
  // NTF-FR-19: a grammar this facility cannot parse is treated exactly as an
  // unreachable target — the author gets the same statement either way, because
  // the difference is not one they can act on.
  if (!address) {
    return { kind: "state", message: "That notification no longer points anywhere." };
  }

  if (!context.projectKey || !context.worktree) {
    return {
      kind: "state",
      message: "Open a project to go where that notification points.",
    };
  }
  if (address.projectKey !== context.projectKey) {
    return {
      kind: "state",
      message: "That notification points into a different project.",
    };
  }
  if (address.worktree !== context.worktree) {
    return {
      kind: "state",
      message: "That notification points into a different worktree.",
    };
  }

  const { target } = address;
  if (target.kind === "file" && !context.fileExists(target.path)) {
    return {
      kind: "state",
      message: `${describeTarget(target)} is no longer in the project.`,
    };
  }
  if (target.kind === "draft" && !context.draftExists(target.draftId)) {
    return { kind: "state", message: "That draft no longer exists." };
  }

  return { kind: "open", target };
}

/** What the hook needs in order to perform an outcome. */
export interface ActivationHandlers {
  openFile: (path: string) => void;
  openDraft: (draftId: string) => void;
  openDashboard: () => void;
  openPanelSurface: (surface: string) => void;
  openBottomSurface: (surface: string) => void;
  /** NTF-FR-17 / GRU-FR-BLSS: open the Runs panel's graduation section on a run. */
  openRun: (runId: string) => void;
  openGlobalSettings: () => void;
  openProjectSettings: () => void;
}

/**
 * Subscribe to activations and route them.
 *
 * Returns the statement of NTF-FR-20 and its dismissal, which the shell renders
 * through `NotificationStatement`.
 */
export function useNotifications(
  context: ActivationContext,
  handlers: ActivationHandlers,
): { statement: string | null; dismissStatement: () => void } {
  const [statement, setStatement] = useState<string | null>(null);

  // The subscription is established once and must not be torn down and rebuilt
  // every time the open project or a handler identity changes — an activation
  // landing in that gap would be lost, and a notification clicked exactly once
  // has no second chance. Refs keep the listener stable while what it reads
  // stays current.
  const contextRef = useRef(context);
  const handlersRef = useRef(handlers);
  contextRef.current = context;
  handlersRef.current = handlers;

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void onNotificationActivated((activation) => {
      // The address is consumed: the backend already withdrew it as part of the
      // activation (NTD-FR-10), so the facility must stop believing it is
      // showing or NTF-FR-14 would later withdraw an id that is long gone.
      forgetPosted(activation.payload);

      const outcome = resolveActivation(activation.payload, contextRef.current);
      const h = handlersRef.current;
      if (outcome.kind === "state") {
        logInfo(["frontend"], "notification activation could not be routed", {
          key: activation.key,
        });
        setStatement(outcome.message);
        return;
      }

      logInfo(["frontend"], "notification activation routed", {
        key: activation.key,
        targetKind: outcome.target.kind,
      });
      // NTF-FR-18: exactly the navigation the author's own equivalent gesture
      // performs, and no more.
      switch (outcome.target.kind) {
        case "dashboard":
          h.openDashboard();
          break;
        case "file":
          h.openFile(outcome.target.path);
          break;
        case "draft":
          h.openDraft(outcome.target.draftId);
          break;
        // NTF-FR-17 / GRU-FR-BLSS: a `run` address opens the bottom panel on the
        // Runs surface's graduation section with that run selected. It marks no
        // tab (NTF-FR-28): a run is not a file and has none.
        case "run":
          h.openRun(outcome.target.runId);
          break;
        case "panel":
          h.openPanelSurface(outcome.target.surface);
          break;
        case "bottom":
          h.openBottomSurface(outcome.target.surface);
          break;
        case "settings":
          if (outcome.target.which === "global") h.openGlobalSettings();
          else h.openProjectSettings();
          break;
      }
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const dismissStatement = useCallback(() => setStatement(null), []);
  return { statement, dismissStatement };
}

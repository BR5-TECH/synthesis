/**
 * The model downgrade raise (`../../specifications/ui/NTF-notifications.md`
 * NTF-FR-AGTB through NTF-FR-UGJO; `GRU-graduation-runs.md` GRU-FR-HMPM).
 *
 * A provider served another model than the request named. The backend finds
 * it and emits `"model downgrade detected"` once for each pair of a turn
 * (AGC-FR-UVKF). This module decides what the main window does with it: it
 * reads the suppression rules, writes the log record, and gives the shell a
 * Warn raise with the Suppress menu unless a rule mutes the pair.
 */
import * as api from "../api";
import { logDebug, logWarn } from "../logging";
import type {
  ModelDowngrade,
  ModelDowngradeSuppression,
  SuppressionDuration,
} from "../types";
import { mintAddress, type NotificationTarget } from "./notificationAddress";
import type { Raise } from "./notifications";
import type { OpenRoot } from "./proposalRaises";
import { showToast, type ToastActionMenu } from "./toasts";

/** NTF-FR-LHVU: the key of the Error toast a failed save shows. */
export const SUPPRESS_FAILED_KEY = "model-downgrade-suppress-failed";

/** NTF-FR-JXQL: the four items of the Suppress menu, in their order. */
export const SUPPRESS_ITEMS: readonly { id: SuppressionDuration; label: string }[] = [
  { id: "hour", label: "1 hour" },
  { id: "day", label: "1 day" },
  { id: "week", label: "1 week" },
  { id: "forever", label: "Forever" },
];

/** NTF-FR-AGTB: one key for one requested→served pair. */
export function downgradeKey(requested: string, served: string): string {
  return `model-downgrade:${requested}→${served}`;
}

/**
 * NTF-FR-CXWO: the place of the work — the draft, the artifact file, the
 * Notes panel, or the run. Null where the event names none.
 */
export function downgradeTarget(payload: ModelDowngrade): NotificationTarget | null {
  if (payload.source === "graduation") {
    return payload.runId ? { kind: "run", runId: payload.runId } : null;
  }
  const target = payload.origin?.target;
  switch (target?.kind) {
    case "draft":
      return { kind: "draft", draftId: target.draftId };
    // AGC-FR-07: the artifact id is its project-relative path.
    case "artifact":
      return { kind: "file", path: target.artifactId };
    case "note":
      return { kind: "panel", surface: "notes" };
    default:
      return null;
  }
}

/**
 * NTF-FR-CYTU: whether a rule mutes the pair at `nowMs` — the two ids equal
 * the pair as exact strings, and the expiry is absent or later than now.
 */
export function isMuted(
  rules: readonly ModelDowngradeSuppression[],
  requested: string,
  served: string,
  nowMs: number,
): boolean {
  return rules.some((rule) => {
    if (rule.requestedModel !== requested || rule.servedModel !== served) return false;
    if (rule.expiresAt == null) return true;
    const expiry = Date.parse(rule.expiresAt);
    return Number.isFinite(expiry) && expiry > nowMs;
  });
}

/**
 * NTF-FR-LHVU: the Suppress menu of one pair. A pick saves the rule. A failed
 * save shows an Error toast and rejects, so the downgrade toast stays.
 */
export function suppressMenu(requested: string, served: string): ToastActionMenu {
  return {
    label: "Suppress",
    items: SUPPRESS_ITEMS.map((item) => ({ ...item })),
    onPick: async (id) => {
      try {
        await api.saveModelDowngradeSuppression(
          requested,
          served,
          id as SuppressionDuration,
        );
      } catch (e) {
        const body = e instanceof Error ? e.message : String(e);
        logWarn(["ai", "frontend"], "model downgrade suppression not saved", {
          duration: id,
          error: body,
        });
        showToast(
          {
            key: SUPPRESS_FAILED_KEY,
            level: "Error",
            title: "Suppression not saved",
            body,
            address: null,
          },
          { priority: true },
        );
        throw e;
      }
    },
  };
}

/**
 * NTF-FR-AGTB / NTF-FR-CXWO / NTF-FR-FPBH: the raise of one downgrade, or null
 * where no project is open or the event names no place.
 */
export function downgradeRaise(payload: ModelDowngrade, root: OpenRoot): Raise | null {
  if (!root.projectKey || !root.worktree) return null;
  const target = downgradeTarget(payload);
  if (!target) return null;
  const { requestedModel: requested, servedModel: served } = payload;
  return {
    key: downgradeKey(requested, served),
    level: "Warn",
    title: "Model changed",
    body: `Requested ${requested}, served ${served}.`,
    address: mintAddress(payload.projectKey, payload.worktree, target),
    showWhileLooking: true,
    menu: suppressMenu(requested, served),
    dismissLabel: "Acknowledge",
  };
}

/**
 * NTF-FR-CYTU / NTF-FR-DYHQ: read the rules, write the one `WARN` record, and
 * return the raise unless a rule mutes the pair. A read that fails mutes
 * nothing. The shell makes the raise, because a raise is made from
 * `../hooks/useAppNotifications.ts` alone.
 *
 * `root` is read again after the rules arrive, because the author may close or
 * switch the project in the meantime. An event of a project that is not open
 * then raises nothing (NTF-FR-CXWO).
 */
export async function modelDowngradeRaise(
  payload: ModelDowngrade,
  root: () => OpenRoot,
  now: () => number = Date.now,
): Promise<Raise | null> {
  let rules: ModelDowngradeSuppression[] = [];
  // With no project open there are no rules to read (GSS-FR-PCPW).
  if (root().projectKey) {
    try {
      rules = (await api.loadModelDowngradeSuppressions()) ?? [];
    } catch (e) {
      logWarn(["ai", "frontend"], "model downgrade suppressions not read", {
        error: String(e),
      });
    }
  }
  const { requestedModel, servedModel, provider, source } = payload;
  const suppressed = isMuted(rules, requestedModel, servedModel, now());
  // NTF-FR-DYHQ: the ids, the provider, the source, and the decision. No
  // message text and no prompt.
  logWarn(["ai"], "model downgrade", {
    requestedModel,
    servedModel,
    provider,
    source,
    suppressed,
  });
  if (suppressed) return null;
  const open = root();
  const reason = !open.projectKey
    ? "no-project"
    : open.projectKey !== payload.projectKey
      ? "other-project"
      : downgradeTarget(payload) === null
        ? "no-target"
        : null;
  if (reason !== null) {
    logDebug(["ai", "frontend"], "model downgrade not raised", { reason, source });
    return null;
  }
  return downgradeRaise(payload, open);
}

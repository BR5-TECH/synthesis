/**
 * The surface panel (`ASF-agent-surfaces.md`; `CMT-comments.md` CMT-FR-DBQC).
 *
 * An agent showed the author a small panel (`SSF-show-surface-tool.md`). The
 * panel reads its record, and the official A2UI renderer draws its message log
 * (ASF-FR-HRQL), loaded with the first panel (ASF-FR-RTRX). The panel sends one
 * action when the author presses a button. A submitted panel stays read-only
 * and says what was sent.
 */
import { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { getAgentSurface, submitSurfaceAction } from "../../api";
import { onAgentSurfaceChanged } from "../../events";
import { logWarn } from "../../logging";
import { useConversationThread } from "../../state/conversationThreads";
import { dispatchDecision } from "../DraftDiscussion/decisionDispatch";
import type { CardSend } from "./a2ui/host";
import type { RendererAction } from "./a2ui/SurfaceRenderer";
import type {
  AgentSurface,
  AgentSurfaceReference,
  Participant,
  SurfaceComponent,
} from "../../types";

/** ASF-FR-RTRX: the renderer is a chunk of its own, loaded with the first panel. */
const SurfaceRenderer = lazy(() => import("./a2ui/SurfaceRenderer"));

/** AUI-FR-TYBX: the current components of a log: createSurface, then updateComponents by id. */
export function currentComponents(messages: readonly unknown[]): SurfaceComponent[] {
  let components: SurfaceComponent[] = [];
  for (const message of messages) {
    const m = message as {
      createSurface?: { components?: SurfaceComponent[] };
      updateComponents?: { components?: SurfaceComponent[] };
    };
    if (m.createSurface) components = [...(m.createSurface.components ?? [])];
    for (const update of m.updateComponents?.components ?? []) {
      const at = components.findIndex((c) => c.id === update.id);
      if (at >= 0) components[at] = update;
      else components.push(update);
    }
  }
  return components;
}

/** The typed codes a submission can fail with (AUI contract surface). */
const KNOWN_CODES = new Set([
  "no_project_open",
  "surface_not_found",
  "already_submitted",
  "unknown_action",
  "invalid_context",
  "discussion_resolved",
  "comment_failed",
  "write_failed",
]);

/** ASF-FR-NLJW: what the panel says of a failed submission. */
function failureText(code: string): string {
  switch (code) {
    case "discussion_resolved":
      return "This conversation is resolved, so nothing was sent.";
    case "unknown_action":
    case "invalid_context":
      return "This button does not match the panel any more, so nothing was sent.";
    case "surface_not_found":
      return "This panel is no longer available, so nothing was sent.";
    default:
      return "The values were not sent. Try again.";
  }
}

function codeOf(error: unknown): string {
  const raw = error instanceof Error ? error.message : String(error);
  return KNOWN_CODES.has(raw) ? raw : "other";
}

function nameOf(participant: Participant): string {
  if (participant.kind === "human") return participant.displayName ?? participant.login;
  return participant.handle;
}

function timeOf(at: string): string {
  const date = new Date(at);
  if (Number.isNaN(date.getTime())) return at;
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(date);
}

/**
 * AUI-FR-XMUS / AUI-FR-WCRS: the label of the pressed button: a Button's
 * literal Text child, or the button of a card, or else the action name.
 */
function buttonLabel(byId: ReadonlyMap<string, SurfaceComponent>, sourceId: string, name: string): string {
  const source = byId.get(sourceId);
  const text = (value: unknown) => (typeof value === "string" && value.trim() !== "" ? value.trim() : undefined);
  switch (source?.component) {
    case "Button": {
      const child = typeof source.child === "string" ? byId.get(source.child) : undefined;
      return (child?.component === "Text" ? text(child.text) : undefined) ?? name;
    }
    case "ChoiceCard":
    case "ChecklistCard":
      return text(source.sendLabel) ?? "Send";
    case "ConfirmCard": {
      const decline = source.decline as { label?: unknown; name?: unknown } | undefined;
      const confirm = source.confirm as { label?: unknown } | undefined;
      return (decline?.name === name ? text(decline?.label) : text(confirm?.label)) ?? name;
    }
    case "ComparisonCard":
      return text((source.pick as { label?: unknown } | undefined)?.label) ?? "Pick";
    default:
      return name;
  }
}

export function AgentSurfacePanel({
  attachment,
  threadId,
}: {
  attachment: AgentSurfaceReference;
  threadId: string;
}) {
  const [surface, setSurface] = useState<AgentSurface | null>(null);
  const [failed, setFailed] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const surfaceId = attachment.surfaceId;

  // ASF-FR-ASQV / ASF-FR-RRXP: the record is read, and a failed read offers Retry.
  useEffect(() => {
    let cancelled = false;
    setFailed(false);
    // A panel that now names another surface shows nothing of the one before.
    setSurface((held) => (held && held.id !== surfaceId ? null : held));
    getAgentSurface(surfaceId).then(
      (read) => {
        if (!cancelled) setSurface(read);
      },
      () => {
        if (cancelled) return;
        // ASF-FR-MGVD: the id alone.
        logWarn(["frontend"], "agent surface could not be read", { surfaceId });
        setFailed(true);
      },
    );
    return () => {
      cancelled = true;
    };
  }, [surfaceId, attempt]);

  // ASF-FR-CUEI: the panel follows its own record.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    void onAgentSurfaceChanged((changed) => {
      if (changed.id === surfaceId) {
        setSurface(changed);
        setFailed(false);
      }
    }).then(
      (fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      },
      () => {},
    );
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [surfaceId]);

  if (!surface) {
    return (
      <div
        className="agent-surface agent-surface--placeholder"
        data-testid="agent-surface"
        data-state={failed ? "unavailable" : "loading"}
      >
        {failed ? (
          <>
            <span>This panel could not be loaded.</span>
            <button type="button" className="btn btn--sm" onClick={() => setAttempt((n) => n + 1)}>
              Retry
            </button>
          </>
        ) : (
          <span className="agent-surface__loading">Loading the panel…</span>
        )}
      </div>
    );
  }
  return <LoadedPanel key={surface.id} surface={surface} threadId={threadId} onSurface={setSurface} />;
}

function LoadedPanel({
  surface,
  threadId,
  onSurface,
}: {
  surface: AgentSurface;
  threadId: string;
  onSurface: (surface: AgentSurface) => void;
}) {
  const thread = useConversationThread(threadId);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const sending = useRef(false);
  const byId = useMemo(
    () => new Map(currentComponents(surface.messages).map((c) => [c.id, c])),
    [surface.messages],
  );
  const submission = surface.submission;
  const resolved = thread?.resolved === true;
  const readOnly = pending || submission !== undefined || resolved;

  const submit = useCallback(
    (sent: RendererAction) => {
      if (submission || resolved || sending.current) return;
      // AUI-FR-AFKN: every key the button declares is sent; a binding whose
      // pointer holds no value sends null.
      const declared = (
        byId.get(sent.sourceComponentId)?.action as { event?: { context?: Record<string, unknown> } } | undefined
      )?.event?.context;
      const context = { ...sent.context };
      for (const key of Object.keys(declared ?? {})) if (context[key] === undefined) context[key] = null;
      const action = { ...sent, context };
      // ASF-FR-XOSG: one submission at a time, also for two presses in one task.
      sending.current = true;
      setPending(true);
      setError(null);
      submitSurfaceAction(surface.id, action).then(
        (result) => {
          sending.current = false;
          setPending(false);
          onSurface(result.surface);
          // ASF-FR-DHGO: the agents come back as they do after a decision.
          void dispatchDecision(
            { id: surface.id, threadId: surface.discussionId, target: surface.target },
            { commentId: result.commentId },
            thread,
            [],
            "",
          );
        },
        (failure: unknown) => {
          sending.current = false;
          setPending(false);
          const code = codeOf(failure);
          // ASF-FR-MGVD: the id and the typed code alone.
          logWarn(["frontend"], "agent surface action failed", { surfaceId: surface.id, error: code });
          if (code === "already_submitted") {
            // ASF-FR-NLJW: another window sent it; draw it as it now stands.
            getAgentSurface(surface.id).then(onSurface, () => setError(failureText(code)));
            return;
          }
          setError(failureText(code));
        },
      );
    },
    [submission, resolved, byId, surface.id, surface.discussionId, surface.target, thread, onSurface],
  );
  // ASF-FR-MLKG: a card sends the context of its kind.
  const sendCard = useCallback(
    (sourceId: string, send: CardSend) =>
      submit({
        name: send.name,
        sourceComponentId: sourceId,
        context: send.context,
        dataModel: send.dataModel ?? {},
      }),
    [submit],
  );

  const state = submission ? "submitted" : resolved ? "resolved" : pending ? "sending" : "open";
  return (
    <div className="agent-surface" data-testid="agent-surface" data-state={state}>
      <Suspense fallback={<span className="agent-surface__loading">Loading the panel…</span>}>
        <SurfaceRenderer record={surface} readOnly={readOnly} onAction={submit} sendCard={sendCard} />
      </Suspense>
      {error && (
        <p className="agent-surface__error" role="alert">
          {error}
        </p>
      )}
      {submission && (
        <p className="agent-surface__sent">
          Sent “{buttonLabel(byId, submission.sourceComponentId, submission.name)}” by{" "}
          {nameOf(submission.by)} on {timeOf(submission.at)}
        </p>
      )}
    </div>
  );
}

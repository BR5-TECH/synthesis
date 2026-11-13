/**
 * The OpenSpec failure notice
 * (`../../specifications/ui/OFN-openspec-failure-notices.md`).
 *
 * One sentence for the failure, from the one table of
 * `../state/openspec/failureNotice.ts`, the actions of its class, and a
 * **Details** disclosure with the CLI's own messages as plain text. Every
 * surface that states an OpenSpec failure renders this component, or the
 * short form of the table where it has no room for actions (OFN-FR-QZEC).
 * The surfaces are those of OFN-FR-HEVB.
 */
import { Fragment, useEffect, useId, useRef, useState } from "react";

import { getOpenspecStatus } from "../api";
import { logInfo, logWarn } from "../logging";
import { openOpenSpecChange } from "../state/openspec/changeRoutes";
import {
  diagnosticLines,
  diagnosticsText,
  failureClass,
  logFailureNotice,
  noticeSentence,
  repairSentence,
  settingsActionOf,
} from "../state/openspec/failureNotice";
import { openDockerSettings, openOpenSpecSettings } from "../state/openspec/settingsRoutes";
import { openGraduationRunRoute } from "../state/graduation/runRoute";
import type { ChangeLocation, OpenSpecFailure } from "../types";

/** OFN-FR-DZSQ: how long the notice states that the text is copied. */
export const COPIED_STATEMENT_MS = 2000;

export interface OpenSpecFailureNoticeProps {
  failure: OpenSpecFailure;
  /** OFN-FR-LKAT: the surface, for the log record. */
  surface: string;
  /** The change of the surface, where the failure names none. */
  change?: string | null;
  /**
   * OFN-FR-RLSO: the location of the failure, for **Open change**. Null on the
   * tab of that change, and where the surface knows no location.
   */
  location?: ChangeLocation | null;
  /** The stream name of a stream location, for the tab it opens. */
  streamName?: string;
  /**
   * OFN-FR-CBLG / OFN-FR-DPIJ: the failed operation again, with the same
   * arguments. Null on a surface that has its own retry.
   */
  onRetry?: (() => Promise<unknown> | void) | null;
  /** SMP-FR-KVKD: false shows the sentence and Details alone. */
  actions?: boolean;
  /**
   * SMP-FR-KVKD: false leaves the sentence out, where the surface states it in
   * a line of its own. The notice then shows its Details alone.
   */
  withSentence?: boolean;
  /** OFN-FR-PFYW: the frame sentence of the surface, such as that no run started. */
  frame?: string | null;
  /**
   * OFN-FR-LKAT: false where the surface logs the notice itself, because it
   * states the sentence in a line of its own (`useShortFormLog`).
   */
  log?: boolean;
  /**
   * SNV-FR-56: called before Open change or Open run opens another surface,
   * so a host overlay, such as a dropdown, closes first.
   */
  onLeave?: (() => void) | null;
  className?: string;
  testId?: string;
}

export function OpenSpecFailureNotice({
  failure,
  surface,
  change = null,
  location = null,
  streamName,
  onRetry = null,
  actions = true,
  withSentence = true,
  frame = null,
  log = true,
  onLeave = null,
  className,
  testId = "openspec-failure-notice",
}: OpenSpecFailureNoticeProps) {
  const klass = failureClass(failure);
  const sentence = noticeSentence(failure, change);
  const [retrying, setRetrying] = useState(false);
  // The guard of the Retry in flight. A ref, so a second press before the
  // next render does nothing too.
  const inFlight = useRef(false);
  const [open, setOpen] = useState(false);
  const [copied, setCopied] = useState(false);
  const live = useRef(true);
  const detailsId = useId();
  const detailsRef = useRef<HTMLDivElement | null>(null);

  // OFN-FR-KMPW: Details that open in a scrolled container, such as the
  // merge confirmation of the stream list, come into view.
  useEffect(() => {
    if (open) detailsRef.current?.scrollIntoView?.({ block: "nearest" });
  }, [open]);

  useEffect(() => {
    live.current = true;
    return () => {
      live.current = false;
    };
  }, []);

  // OFN-FR-LKAT: one record for each notice that appears.
  useEffect(() => {
    if (sentence !== null && log) logFailureNotice(surface, failure);
    // The failure object is the notice; a new one is a new notice.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [failure]);

  // OFN-FR-DZSQ: the copied statement stays for 2 seconds.
  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), COPIED_STATEMENT_MS);
    return () => clearTimeout(timer);
  }, [copied]);

  if (sentence === null) return null;

  const repair = repairSentence(failure);
  const name = failure.change || change;
  const settings = settingsActionOf(failure);
  const showRetry = actions && klass === "transient" && onRetry !== null;
  const showChange = actions && klass === "content" && location !== null && Boolean(name);
  const showRun = actions && Boolean(failure.repairRunId);
  const diagnostics = failure.diagnostics ?? [];

  /** OFN-FR-CBLG: a re-check of the CLI, then the failed operation again. */
  const retry = async () => {
    if (inFlight.current || !onRetry) return;
    inFlight.current = true;
    setRetrying(true);
    logInfo(["frontend"], "an openspec failure notice retries", { surface, code: failure.code });
    try {
      await getOpenspecStatus(true);
    } catch (e) {
      logWarn(["frontend"], "the openspec status re-check of a retry failed", {
        surface,
        reason: e instanceof Error ? e.name : typeof e,
      });
    }
    try {
      await onRetry();
    } catch {
      // The surface states the outcome of its own operation.
    } finally {
      inFlight.current = false;
      if (live.current) setRetrying(false);
    }
  };

  /** OFN-FR-DZSQ: every diagnostic to the clipboard, as plain text. */
  const copy = async () => {
    const clipboard = navigator.clipboard;
    if (!clipboard?.writeText) {
      logWarn(["frontend"], "the openspec diagnostics could not be copied", {
        surface,
        reason: "unavailable",
      });
      return;
    }
    try {
      await clipboard.writeText(diagnosticsText(diagnostics));
      // The count alone: the diagnostics reach the author alone (OFP-FR-WRDX).
      logInfo(["frontend"], "the openspec diagnostics were copied", {
        surface,
        count: diagnostics.length,
      });
      if (live.current) setCopied(true);
    } catch (e) {
      logWarn(["frontend"], "the openspec diagnostics could not be copied", {
        surface,
        reason: e instanceof Error ? e.name : typeof e,
      });
    }
  };

  const hasActions = showRetry || showChange || showRun || (actions && settings !== null);
  const statement = [sentence, repair, frame].filter(Boolean).join(" ");

  return (
    <div
      className={["ofn", className].filter(Boolean).join(" ")}
      data-testid={testId}
      data-class={klass}
    >
      {/* OFN-FR-ONNF: the sentence is announced once, when the notice appears.
          The actions and Details stand outside the live region, so opening
          Details announces nothing again. */}
      {withSentence && (
        <p className="ofn__sentence" role="alert">
          {statement}
        </p>
      )}
      {(hasActions || diagnostics.length > 0) && (
        <div className="ofn__actions">
          {actions && settings === "openspec" && (
            <button type="button" className="btn btn--sm btn--default" onClick={openOpenSpecSettings}>
              Open OpenSpec settings
            </button>
          )}
          {actions && settings === "docker" && (
            <button type="button" className="btn btn--sm btn--default" onClick={openDockerSettings}>
              Open Docker settings
            </button>
          )}
          {showRetry && (
            // OFN-FR-CBLG: disabled while the two calls are in flight. It
            // keeps its focus stop, so a Retry by keyboard keeps the focus.
            <button
              type="button"
              className="btn btn--sm btn--default"
              aria-disabled={retrying ? "true" : undefined}
              onClick={() => void retry()}
            >
              Retry
            </button>
          )}
          {showChange && location && name && (
            <button
              type="button"
              className="btn btn--sm btn--default"
              onClick={() => {
                logInfo(["frontend"], "an openspec failure notice opens the change", {
                  surface,
                  locationKind: location.kind,
                  withThread: Boolean(failure.threadId),
                });
                onLeave?.();
                openOpenSpecChange(location, name, {
                  thread: failure.threadId ?? null,
                  streamName,
                });
              }}
            >
              Open change
            </button>
          )}
          {showRun && (
            <button
              type="button"
              className="btn btn--sm btn--default"
              onClick={() => {
                onLeave?.();
                openGraduationRunRoute(failure.repairRunId!);
              }}
            >
              Open run
            </button>
          )}
          {diagnostics.length > 0 && (
            <button
              type="button"
              className="btn btn--sm btn--ghost ofn__toggle"
              aria-expanded={open}
              // The list is mounted only while open, so the button names it
              // only then.
              aria-controls={open ? detailsId : undefined}
              onClick={() => setOpen((was) => !was)}
            >
              Details <span aria-hidden="true">{open ? "▴" : "▾"}</span>
            </button>
          )}
        </div>
      )}
      {open && diagnostics.length > 0 && (
        // OFN-FR-ONNF: Details is not live, also where a host region is, such
        // as the comment log of a discussion. Only "Copied." is announced.
        <div
          ref={detailsRef}
          className="ofn__details"
          id={detailsId}
          aria-live="off"
          data-testid="openspec-failure-details"
        >
          <div className="ofn__details-head">
            <span className="ofn__copied t-meta" role="status">
              {copied ? "Copied." : ""}
            </span>
            <button type="button" className="btn btn--sm btn--ghost" onClick={() => void copy()}>
              Copy
            </button>
          </div>
          {/* OFN-FR-UNEF: plain text alone; React escapes every character. */}
          <ol className="ofn__diagnostics" aria-label="OpenSpec CLI messages">
            {diagnostics.map((diagnostic, index) => (
              <li key={index} className="ofn__diagnostic" data-testid="openspec-failure-diagnostic">
                {/* OFN-FR-IHSQ: each line is a block, and a newline stands
                    between two lines, so that the text form keeps the fix
                    on a line of its own. */}
                {diagnosticLines(diagnostic).map((line, at) => (
                  <Fragment key={at}>
                    {at > 0 && "\n"}
                    <div className="ofn__line">{line}</div>
                  </Fragment>
                ))}
              </li>
            ))}
          </ol>
        </div>
      )}
    </div>
  );
}

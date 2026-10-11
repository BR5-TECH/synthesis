/**
 * The Create a PR window
 * (`../../specifications/ui/CPR-create-pull-request.md`).
 *
 * One window for every surface that opens a pull request: a work stream row,
 * the Changes panel footer, and the Git panel's PRs section. The opener gives
 * it a source (CPR-FR-JOIG) and everything after that is the same.
 *
 * The window creates the pull request and nothing else. It commits nothing,
 * pushes nothing and pulls nothing (CPR-FR-VYPG): a head branch GitHub cannot
 * see is a state it names, with what the author must do about it.
 *
 * The shell mounts it, because it is a floating overlay of the main window and
 * mutually exclusive with the others (SNV-FR-56, CPR-FR-IWDK). It gives way to
 * the token picker (CPR-FR-VZUZ) by handing back what the author typed through
 * `onRequestToken` and unmounting; the shell mounts it again with `resume`.
 */
import { useEffect, useId, useMemo, useRef, useState } from "react";

import * as api from "../api";
import { logInfo, logWarn } from "../logging";
import type { CreatedPullRequest, PullRequestHeadState } from "../types";
import { baseBranchNames } from "./CreatePullRequest/baseBranches";
import {
  pullRequestBlocks,
  pullRequestFailureMessage,
  pullRequestHeadReadMessage,
} from "./CreatePullRequest/messages";
import type {
  PullRequestInput,
  PullRequestResume,
  PullRequestSource,
} from "./CreatePullRequest/types";
import {
  isTokenMissing,
  isTokenSelectionRequired,
  parseRejection,
} from "./Git/errors";
import { Icon } from "./icons";

export interface CreatePullRequestWindowProps {
  source: PullRequestSource;
  /** CPR-FR-VZUZ: set where the window returns after the token picker. */
  resume?: PullRequestResume | null;
  /** CPR-FR-IWDK: the control that opened the window, which gets focus back. */
  returnFocus?: HTMLElement | null;
  /** CPR-FR-SSQI: Cancel, Escape and the backdrop. */
  onClose: () => void;
  /** CPR-FR-ITWJ: the pull request exists. */
  onCreated: (created: CreatedPullRequest) => void;
  /** CPR-FR-VZUZ: the window gives way to the picker, holding `input`. */
  onRequestToken: (input: PullRequestInput) => void;
  /** CPR-FR-FGGU: the route to the Global settings GitHub section. */
  onOpenGlobalSettings: () => void;
  /** CPR-FR-XMRL: a request runs, so the shell must not dismiss the window. */
  onSubmittingChange: (submitting: boolean) => void;
}

type HeadRead =
  | { status: "loading" }
  | { status: "ready"; state: PullRequestHeadState }
  | { status: "failed"; message: string };

interface Failure {
  message: string;
  missingToken: boolean;
}

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [href], [tabindex]:not([tabindex="-1"])';

export function CreatePullRequestWindow({
  source,
  resume,
  returnFocus,
  onClose,
  onCreated,
  onRequestToken,
  onOpenGlobalSettings,
  onSubmittingChange,
}: CreatePullRequestWindowProps) {
  const initial = resume?.input;
  const [title, setTitle] = useState(initial?.title ?? source.title);
  const [body, setBody] = useState(initial?.body ?? "");
  const [base, setBase] = useState(initial?.base ?? source.base);
  const [draft, setDraft] = useState(initial?.draft ?? false);
  const [branches, setBranches] = useState<string[] | null>(null);
  const [head, setHead] = useState<HeadRead>({ status: "loading" });
  const [submitting, setSubmitting] = useState(false);
  const [failure, setFailure] = useState<Failure | null>(
    resume?.notice ? { message: resume.notice, missingToken: false } : null,
  );
  const ids = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const titleRef = useRef<HTMLInputElement>(null);
  const submittingRef = useRef(false);
  const readSeq = useRef(0);
  const resumedRef = useRef(false);
  const mounted = useRef(true);

  // CPR-FR-IWDK: focus returns to the control that opened the window.
  useEffect(() => {
    const opener =
      returnFocus ?? (document.activeElement as HTMLElement | null);
    mounted.current = true;
    return () => {
      mounted.current = false;
      if (opener?.isConnected) opener.focus();
    };
  }, []);

  // CPR-FR-NQPS: the first focus lands on the Title field.
  useEffect(() => {
    titleRef.current?.focus();
  }, []);

  // CPR-FR-FZHF: the project's branches, by the names GitHub knows them.
  useEffect(() => {
    let live = true;
    api
      .listBranches()
      .then((all) => {
        if (live) setBranches(baseBranchNames(all, source.head));
      })
      .catch((error) => {
        // A listing that failed says nothing about the base branch, so it is
        // not marked missing: the window offers the base it was given.
        logWarn(["frontend", "remote"], "pull request branch listing failed", {
          error: parseRejection(error).code,
        });
      });
    return () => {
      live = false;
    };
  }, [source.head]);

  // CPR-FR-SQGZ: read the head branch's state, and again for another base. A
  // response of an older read is discarded (CPR non-functional).
  const readHead = (forBase: string) => {
    const seq = ++readSeq.current;
    setHead({ status: "loading" });
    api
      .getPullRequestHeadState(source.head, forBase || null)
      .then((state) => {
        if (seq === readSeq.current && mounted.current)
          setHead({ status: "ready", state });
      })
      .catch((error) => {
        const message = pullRequestHeadReadMessage(error, source.head, forBase);
        logWarn(["frontend", "remote"], "pull request head state read failed", {
          head: source.head,
          base: forBase,
          error: parseRejection(error).code,
        });
        if (seq === readSeq.current && mounted.current)
          setHead({ status: "failed", message });
      });
  };

  useEffect(() => {
    readHead(base);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [base, source.head]);

  const blocks = useMemo(
    () => (head.status === "ready" ? pullRequestBlocks(head.state) : []),
    [head],
  );
  const baseMissing =
    branches !== null && base !== "" && !branches.includes(base);
  const baseOptions = useMemo(() => {
    const list = branches ?? [];
    return list.includes(base) || base === "" ? list : [base, ...list];
  }, [branches, base]);

  const titleBlank = title.trim() === "";
  const reasons: string[] = [];
  if (titleBlank) reasons.push("A pull request needs a title.");
  if (baseMissing) reasons.push(`The base branch ${base} does not exist. Choose another.`);
  if (head.status === "loading") reasons.push("Reading the branch…");
  if (head.status === "failed") reasons.push("The branch could not be read.");
  for (const block of blocks) reasons.push(block.text);
  const canSubmit = reasons.length === 0 && !submitting;

  const setRunning = (running: boolean) => {
    submittingRef.current = running;
    // A window that is gone must not clear the guard of one that replaced it.
    if (!mounted.current) return;
    setSubmitting(running);
    onSubmittingChange(running);
  };

  // CPR-FR-KMHY: one request at a time, also for a second activation in the
  // same moment as the first. `fromResume` bounds the token loop to one retry.
  const submit = async (fromResume = false): Promise<void> => {
    if (submittingRef.current) return;
    setRunning(true);
    setFailure(null);
    logInfo(["frontend", "remote"], "pull request creation started", {
      head: source.head,
      base,
      draft,
    });
    try {
      const created = await api.createPullRequest(
        title,
        body,
        base,
        source.head,
        draft,
      );
      logInfo(["frontend", "remote"], "pull request created", {
        number: created.number,
      });
      setRunning(false);
      // A project that changed while the request ran has dropped the window.
      if (mounted.current) onCreated(created);
    } catch (error) {
      const code = parseRejection(error).code;
      logWarn(["frontend", "remote"], "pull request creation failed", {
        head: source.head,
        base,
        error: code,
      });
      setRunning(false);
      if (!mounted.current) return;
      if (isTokenSelectionRequired(error) && !fromResume) {
        // CPR-FR-VZUZ: the picker is an overlay of this window's own kind, so
        // the window gives way and comes back with what was typed.
        onRequestToken({ title, body, base, draft });
        return;
      }
      setFailure({
        message: pullRequestFailureMessage(error),
        missingToken: isTokenMissing(error),
      });
    }
  };

  // CPR-FR-VZUZ: a token was chosen, so the submission runs once with what was
  // held.
  useEffect(() => {
    if (resume?.submit && !resumedRef.current) {
      resumedRef.current = true;
      void submit(true);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // CPR-FR-SSQI / CPR-FR-XMRL: Escape closes the window unless a request runs.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      if (!submittingRef.current) onClose();
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  // CPR-FR-IWDK: focus stays inside the window.
  const trapTab = (e: React.KeyboardEvent) => {
    if (e.key !== "Tab") return;
    const nodes = Array.from(
      dialogRef.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [],
    );
    if (nodes.length === 0) return;
    const first = nodes[0];
    const last = nodes[nodes.length - 1];
    const at = document.activeElement;
    if (e.shiftKey && at === first) {
      e.preventDefault();
      last.focus();
    } else if (!e.shiftKey && at === last) {
      e.preventDefault();
      first.focus();
    }
  };

  const titleId = `${ids}-title`;
  const reasonId = `${ids}-reason`;

  return (
    <div
      className="scrim"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget && !submittingRef.current) onClose();
      }}
    >
      <div
        className="modal create-pr"
        role="dialog"
        aria-modal="true"
        aria-labelledby={`${ids}-heading`}
        ref={dialogRef}
        onKeyDown={trapTab}
        data-testid="create-pr-window"
      >
        <div className="modal__head">
          <Icon.GitPull size={14} />
          <h2 className="modal__title" id={`${ids}-heading`}>
            Create a PR
          </h2>
          <button
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            aria-label="Close"
            disabled={submitting}
            onClick={onClose}
          >
            <Icon.X size={12} />
          </button>
        </div>

        <div className="modal__body create-pr__body">
          <div className="picker-field">
            <label className="picker-field__label" htmlFor={titleId}>
              Title
            </label>
            <input
              id={titleId}
              ref={titleRef}
              className="input"
              value={title}
              disabled={submitting}
              onChange={(e) => setTitle(e.target.value)}
            />
          </div>

          <div className="picker-field">
            <label className="picker-field__label" htmlFor={`${ids}-body`}>
              Description
            </label>
            <textarea
              id={`${ids}-body`}
              className="textarea"
              rows={6}
              value={body}
              disabled={submitting}
              onChange={(e) => setBody(e.target.value)}
            />
          </div>

          {/* CPR-FR-QVYZ: the head branch is shown, never edited. */}
          <div className="create-pr__branches">
            <div className="picker-field">
              <span className="picker-field__label" id={`${ids}-head-label`}>
                Head branch
              </span>
              <span
                className="create-pr__head t-meta"
                aria-labelledby={`${ids}-head-label`}
                title={source.head}
                data-testid="create-pr-head"
              >
                {source.head}
              </span>
            </div>
            <div className="picker-field">
              <label className="picker-field__label" htmlFor={`${ids}-base`}>
                Base branch
              </label>
              <select
                id={`${ids}-base`}
                className="input"
                value={base}
                disabled={submitting}
                onChange={(e) => setBase(e.target.value)}
                data-testid="create-pr-base"
              >
                {baseOptions.map((name) => (
                  <option key={name} value={name}>
                    {branches !== null && !branches.includes(name)
                      ? `${name} (missing)`
                      : name}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <label className="create-pr__draft">
            <input
              type="checkbox"
              checked={draft}
              disabled={submitting}
              onChange={(e) => setDraft(e.target.checked)}
            />
            <span>Draft</span>
          </label>

          {/* CPR-FR-SQGZ / CPR-FR-VYPG: what stands between the branch and a
              pull request, in words. */}
          <div
            className="create-pr__state"
            aria-live="polite"
            data-testid="create-pr-state"
          >
            {head.status === "loading" && (
              <p className="t-muted" data-testid="create-pr-loading">
                Reading the branch…
              </p>
            )}
            {head.status === "failed" && (
              <p className="create-pr__block" role="alert" data-testid="create-pr-read-error">
                {head.message}
              </p>
            )}
            {blocks.map((block) => (
              <p
                key={block.key}
                className="create-pr__block"
                data-testid={`create-pr-block-${block.key}`}
              >
                ⚠ {block.text}
              </p>
            ))}
            {head.status !== "loading" && (
              <button
                type="button"
                className="btn btn--ghost btn--sm"
                disabled={submitting}
                data-testid="create-pr-recheck"
                onClick={() => readHead(base)}
              >
                {head.status === "failed" ? "Try again" : "Check again"}
              </button>
            )}
          </div>

          {/* CPR-FR-SQEP: a failure sits directly above the action row. */}
          {failure && (
            <div
              className="create-pr__failure"
              role="alert"
              data-testid="create-pr-failure"
            >
              <span>{failure.message}</span>
              {failure.missingToken && (
                <button
                  type="button"
                  className="btn btn--ghost btn--sm"
                  data-testid="create-pr-global-settings"
                  onClick={onOpenGlobalSettings}
                >
                  Global settings
                </button>
              )}
            </div>
          )}
        </div>

        <div className="modal__actions">
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            disabled={submitting}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="button"
            className="btn btn--primary btn--sm"
            disabled={!canSubmit}
            aria-describedby={reasons.length > 0 ? reasonId : undefined}
            data-testid="create-pr-submit"
            onClick={() => void submit()}
          >
            {submitting ? "Creating the pull request…" : "Submit"}
          </button>
          {reasons.length > 0 && !submitting && (
            <span id={reasonId} className="sr-only">
              {reasons.join(" ")}
            </span>
          )}
        </div>
      </div>
    </div>
  );
}

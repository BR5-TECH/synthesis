/**
 * The Create a PR window
 * (`../../specifications/ui/CPR-create-pull-request.md`).
 *
 * One window for every surface that opens a pull request: a work stream row,
 * the Changes panel footer, and the Git panel's PRs section. The opener gives
 * it a source (CPR-FR-JOIG). The window keeps what the author did for each
 * head branch for the application session, and a new opening restores it
 * instead of the source (CPR-FR-SSQI, `state/pullRequestSessions`).
 *
 * When the remote lacks the head branch or some of its commits, **Submit**
 * pushes the branch first, then creates the pull request, then links it to its
 * tasks and features (CPR-FR-PLWK, CPR-FR-ODTB, CPR-FR-BLEN). The window
 * commits nothing and pulls nothing: those states are blocks it names, with
 * what the author must do about them (CPR-FR-VYPG).
 *
 * **Generate** and the combine of run summaries run as PR summary runs in the
 * background (CPR-FR-UNAN, CPR-FR-WCBQ). Their follower writes the result into
 * the saved state whether the window is open or not
 * (`state/pullRequestSummaries`).
 *
 * The shell mounts it, because it is a floating overlay of the main window and
 * mutually exclusive with the others (SNV-FR-56, CPR-FR-IWDK). It gives way to
 * the token picker (CPR-FR-VZUZ) by handing back what the author typed through
 * `onRequestToken` and unmounting; the shell mounts it again with `resume`.
 */
import { useEffect, useId, useRef, useState } from "react";

import * as api from "../api";
import { logInfo, logWarn } from "../logging";
import {
  clearPullRequestSession,
  markPullRequestWindowOpen,
  openPullRequestSession,
  updatePullRequestSession,
  usePullRequestSession,
  type PullRequestSession,
} from "../state/pullRequestSessions";
import {
  discoverPullRequestSummaryRun,
  generatePullRequestSummary,
} from "../state/pullRequestSummaries";
import type { CreatedPullRequest, StackStep } from "../types";
import { DescriptionField } from "./CreatePullRequest/DescriptionField";
import { FeaturesField, useFeatureCatalog } from "./CreatePullRequest/FeaturesField";
import {
  issuesToLink,
  linkFailureOf,
  linkResultOf,
  type PullRequestLinkResult,
} from "./CreatePullRequest/linking";
import { pullRequestFailureMessage, pushFailureMessage } from "./CreatePullRequest/messages";
import {
  stackedOn,
  useAgenticIntegration,
  useBranchTasks,
  useOpenPullRequests,
} from "./CreatePullRequest/reads";
import { TasksList } from "./CreatePullRequest/TasksList";
import type {
  PullRequestInput,
  PullRequestResume,
  PullRequestSource,
} from "./CreatePullRequest/types";
import { useHeadState } from "./CreatePullRequest/useHeadState";
import { isTokenMissing, isTokenSelectionRequired, parseRejection } from "./Git/errors";
import { Icon } from "./icons";
import { useInlineWriteGuard } from "./MainBranchWrite";

export interface CreatePullRequestWindowProps {
  source: PullRequestSource;
  /** CPR-FR-VZUZ: set where the window returns after the token picker. */
  resume?: PullRequestResume | null;
  /** CPR-FR-IWDK: the control that opened the window, which gets focus back. */
  returnFocus?: HTMLElement | null;
  /** CPR-FR-SSQI: Cancel, Escape and the backdrop. */
  onClose: () => void;
  /**
   * CPR-FR-ITWJ / CPR-FR-UXXF / CPR-FR-YDLV: the pull request exists, how it
   * is linked, and what the stack step did when it was submitted to a stack.
   */
  onCreated: (
    created: CreatedPullRequest,
    link: PullRequestLinkResult | null,
    stack?: StackStep | null,
  ) => void;
  /** CPR-FR-VZUZ: the window gives way to the picker, holding `input`. */
  onRequestToken: (input: PullRequestInput) => void;
  /** CPR-FR-FGGU: the route to the Global settings GitHub section. */
  onOpenGlobalSettings: () => void;
  /** CPR-FR-XMRL: a request runs, so the shell must not dismiss the window. */
  onSubmittingChange: (submitting: boolean) => void;
}

interface Failure {
  message: string;
  missingToken: boolean;
}

/** CPR-FR-XMRL: what the running request does now. */
type Phase = "pushing" | "creating" | "linking";

const PHASE_WORDS: Record<Phase, string> = {
  pushing: "Pushing the branch…",
  creating: "Creating the pull request…",
  linking: "Linking the issues…",
};

const FOCUSABLE =
  'button:not([disabled]), input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [href], [tabindex]:not([tabindex="-1"])';

/**
 * CPR-FR-JOIG / CPR-FR-SSQI: the saved state of the head branch, made from the
 * source when none exists. What the picker held is put back (CPR-FR-VZUZ).
 */
function openSession(source: PullRequestSource, resume: PullRequestResume | null | undefined) {
  const { created } = openPullRequestSession(source);
  if (resume?.input) updatePullRequestSession(source.head, { ...resume.input });
  return created;
}

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
  const head = source.head;
  // CPR-FR-WCBQ: true at the first opening for the head branch.
  const [fresh] = useState(() => openSession(source, resume));
  const saved = usePullRequestSession(head);
  // A project change clears the store before the shell drops the window.
  const session: PullRequestSession = saved ?? {
    head,
    title: source.title,
    body: "",
    base: source.base,
    draft: false,
    features: [],
    summary: null,
    combined: false,
  };
  const { title, body, base, draft, features, summary } = session;
  const set = (patch: Partial<PullRequestSession>) => updatePullRequestSession(head, patch);

  const [submitting, setSubmitting] = useState(false);
  const [phase, setPhase] = useState<Phase>("creating");
  const [failure, setFailure] = useState<Failure | null>(
    resume?.notice ? { message: resume.notice, missingToken: false } : null,
  );
  const ids = useId();
  const dialogRef = useRef<HTMLDivElement>(null);
  const titleRef = useRef<HTMLInputElement>(null);
  const submittingRef = useRef(false);
  const resumedRef = useRef(false);
  const mounted = useRef(true);

  const { branches, read, readHead, blocks, pushNotice, baseMissing, baseOptions } =
    useHeadState(head, base, mounted);
  const tasks = useBranchTasks(head, fresh);
  // CPR-FR-BLEN: the submission links the tasks of the newest read.
  const tasksRef = useRef(tasks);
  tasksRef.current = tasks;
  const openPulls = useOpenPullRequests();
  const integration = useAgenticIntegration();
  const catalog = useFeatureCatalog();
  const stacked = stackedOn(openPulls, base);
  // CPR-FR-QXTB: with the parent in the stack as its base, Submit adds the
  // pull request to the stack.
  const toStack = source.stackParent !== undefined && base === source.stackParent;
  const submitRef = useRef<HTMLButtonElement>(null);
  const retrySubmit = useRef<() => void>(() => {});

  // MBW-FR-RMCO / MBW-FR-URIL: the push of the head branch is a write. The
  // guard of the head branch is read when a push is due, before Submit is
  // available. This window is an open overlay, so a confirmation is an inline
  // section of it, and a protected head adds the GitHub line (MBW-FR-JZME).
  const writeGuard = useInlineWriteGuard({
    write: "push",
    branch: head,
    enabled: pushNotice !== null,
    onCancel: onClose,
    // MBW-FR-WCBQ: focus stays in the window when the section goes.
    onConfirmed: () => (submitRef.current?.disabled ? titleRef : submitRef).current?.focus(),
  });

  // CPR-FR-IWDK: focus returns to the control that opened the window.
  useEffect(() => {
    const opener = returnFocus ?? (document.activeElement as HTMLElement | null);
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

  // CPR-FR-VOEB: the window learns the PR summary run of the head branch. While
  // it is open, the completion of that run raises no toast over it.
  useEffect(() => {
    void discoverPullRequestSummaryRun(head);
    return markPullRequestWindowOpen(head);
  }, [head]);

  const titleBlank = title.trim() === "";
  const reasons: string[] = [];
  if (titleBlank) reasons.push("A pull request needs a title.");
  if (baseMissing) reasons.push(`The base branch ${base} does not exist. Choose another.`);
  if (read.status === "loading") reasons.push("Reading the branch…");
  if (read.status === "failed") reasons.push("The branch could not be read.");
  // CPR-FR-BLEN: the tasks are linked on submit, so they are read first.
  if (tasks.status === "loading") reasons.push("Reading the tasks…");
  for (const block of blocks) reasons.push(block.text);
  // MBW-FR-IOGF: until the author confirms, Submit is unavailable.
  const canSubmit = reasons.length === 0 && !submitting && !writeGuard.blocked;

  // CPR-FR-VOEB / CPR-FR-IOGF: why Generate is unavailable.
  const generating = summary?.status === "starting" || summary?.status === "running";
  const generateBlock = generating
    ? "A PR summary run for this head branch is in progress."
    : integration === "none"
      ? "This project resolves no agentic integration, so Generate is unavailable."
      : integration === "loading"
        ? "Reading the agentic integration…"
        : null;

  const setRunning = (running: boolean) => {
    submittingRef.current = running;
    // A window that is gone must not clear the guard of one that replaced it.
    if (!mounted.current) return;
    setSubmitting(running);
    onSubmittingChange(running);
  };

  // CPR-FR-BLEN: link the tasks, then the features. A link that fails does not
  // fail the submission.
  const linkIssues = async (number: number): Promise<PullRequestLinkResult | null> => {
    const current = tasksRef.current;
    if (current.status === "failed") {
      logWarn(["frontend", "remote"], "pull request tasks not linked, read failed", {
        head,
        number,
      });
    }
    const issues = issuesToLink(current.status === "ready" ? current.tasks : [], features);
    if (issues.length === 0) return null;
    setPhase("linking");
    try {
      const outcome = await api.linkPullRequestIssues(number, issues);
      logInfo(["frontend", "remote"], "pull request issues linked", {
        head,
        number,
        issueCount: issues.length,
        linkedCount: outcome.linked.length,
        failedCount: outcome.failed.length,
        // The codes alone: a refusal of GitHub carries its own words after them.
        failedCodes: [...new Set(outcome.failed.map((f) => f.error.split(":")[0].trim()))],
        closesOnMerge: outcome.closesOnMerge,
      });
      return linkResultOf(outcome);
    } catch (error) {
      logWarn(["frontend", "remote"], "pull request issue link failed", {
        head,
        number,
        issueCount: issues.length,
        error: parseRejection(error).code,
      });
      return linkFailureOf(issues);
    }
  };

  // CPR-FR-KMHY: one request at a time, also for a second activation in the
  // same moment as the first. `fromResume` bounds the token loop to one retry.
  const submit = async (fromResume = false): Promise<void> => {
    if (submittingRef.current) return;
    setRunning(true);
    setFailure(null);
    // CPR-FR-ODTB: the push is due when the last read says the remote lacks
    // the branch or some of its commits.
    setPhase(pushNotice !== null ? "pushing" : "creating");
    if (pushNotice !== null && !(await pushHead(fromResume))) return;
    if (mounted.current) setPhase("creating");
    logInfo(["frontend", "remote"], "pull request creation started", {
      head,
      base,
      draft,
      toStack,
    });
    let created: CreatedPullRequest;
    let stackStep: StackStep | null = null;
    try {
      if (toStack) {
        const answer = await api.submitPullRequestToStack(title, body, base, head, draft);
        created = { number: answer.number, url: answer.url };
        stackStep = answer.stack;
        logInfo(["frontend", "remote"], "pull request stack step finished", {
          head,
          number: answer.number,
          state: answer.stack?.state,
          stackNumber: answer.stack?.stackNumber,
          error: answer.stack?.error?.split(":")[0],
        });
      } else {
        created = await api.createPullRequest(title, body, base, head, draft);
      }
    } catch (error) {
      const code = parseRejection(error).code;
      logWarn(["frontend", "remote"], "pull request creation failed", {
        head,
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
      return;
    }
    logInfo(["frontend", "remote"], "pull request created", { number: created.number });
    // A project that changed while the request ran has dropped the window, and
    // a link now would reach the repository of the project that is open.
    if (!mounted.current) {
      logWarn(["frontend", "remote"], "pull request issue link skipped, window gone", {
        head,
        number: created.number,
      });
      setRunning(false);
      return;
    }
    const link = await linkIssues(created.number);
    setRunning(false);
    if (!mounted.current) return;
    // CPR-FR-ESXZ: the next opening for this head branch starts from the source.
    clearPullRequestSession(head);
    onCreated(created, link, stackStep);
  };

  // CPR-FR-ODTB: push the head branch before the creation. True when the
  // creation may follow; false when the push failed and the failure is shown.
  const pushHead = async (fromResume: boolean): Promise<boolean> => {
    logInfo(["frontend", "remote"], "pull request head push started", {
      head,
      reason: pushNotice?.key,
    });
    try {
      await api.pushBranch(head);
    } catch (error) {
      logWarn(["frontend", "remote"], "pull request head push failed", {
        head,
        error: parseRejection(error).code,
      });
      setRunning(false);
      if (!mounted.current) return false;
      // MBW-FR-YSMC: a refusal of the main branch write check shows the
      // section with the guard it carries. A confirm submits once more, so the
      // push runs again.
      if (writeGuard.adoptRefusal(error, () => retrySubmit.current())) return false;
      if (isTokenSelectionRequired(error) && !fromResume) {
        // CPR-FR-VZUZ: the push asks for a token as the creation does.
        onRequestToken({ title, body, base, draft });
        return false;
      }
      setFailure({
        message: pushFailureMessage(error, head),
        missingToken: isTokenMissing(error),
      });
      return false;
    }
    logInfo(["frontend", "remote"], "pull request head pushed", { head });
    // A window that is gone creates nothing: the project may have changed.
    if (!mounted.current) {
      logInfo(["frontend", "remote"], "pull request creation skipped, window gone", { head });
      setRunning(false);
      return false;
    }
    // CPR-FR-ODTB: the remote now holds the branch, so the state is read anew.
    readHead(base);
    return true;
  };

  retrySubmit.current = () => void submit();

  // CPR-FR-VZUZ: a token was chosen, so the submission runs once with what was
  // held. It waits for the head state, which decides whether a push is due,
  // and runs only when nothing makes Submit unavailable (CPR-FR-HGXL).
  useEffect(() => {
    if (!resume?.submit || resumedRef.current) return;
    if (read.status === "loading" || tasks.status === "loading") return;
    // MBW-FR-RMCO: a due push waits for the guard read too.
    if (writeGuard.pending) return;
    resumedRef.current = true;
    if (canSubmit) void submit(true);
    else
      logInfo(["frontend", "remote"], "pull request resume held back", {
        head,
        reasons: reasons.length,
      });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [read, tasks, writeGuard.pending]);

  // CPR-FR-SSQI / CPR-FR-XMRL: Escape closes the window unless a request runs.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // An open feature list owns the key: it closes itself first.
      if (e.target instanceof Element && e.target.closest(".fsel__menu")) return;
      e.stopPropagation();
      if (!submittingRef.current) onClose();
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [onClose]);

  // CPR-FR-IWDK: focus stays inside the window.
  const trapTab = (e: React.KeyboardEvent) => {
    if (e.key !== "Tab") return;
    const nodes = Array.from(dialogRef.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? [])
      .filter((node) => !node.closest("[hidden]"));
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
              onChange={(e) => set({ title: e.target.value })}
            />
          </div>

          <DescriptionField
            id={`${ids}-body`}
            body={body}
            onBody={(next) => set({ body: next })}
            disabled={submitting}
            generateBlock={generateBlock}
            onGenerate={() => void generatePullRequestSummary(head)}
            summary={summary}
            head={head}
          />

          {/* CPR-FR-QVYZ: the head branch is shown, never edited. */}
          <div className="create-pr__branches">
            <div className="picker-field">
              <span className="picker-field__label" id={`${ids}-head-label`}>
                Head branch
              </span>
              <span
                className="create-pr__head t-meta"
                aria-labelledby={`${ids}-head-label`}
                title={head}
                data-testid="create-pr-head"
              >
                {head}
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
                onChange={(e) => set({ base: e.target.value })}
                data-testid="create-pr-base"
              >
                {baseOptions.map((name) => (
                  <option key={name} value={name}>
                    {branches !== null && !branches.includes(name) ? `${name} (missing)` : name}
                  </option>
                ))}
              </select>
            </div>
          </div>

          {/* CPR-FR-IPKX / CPR-FR-JBPQ: where the pull request goes. A failed
              read states no kind. */}
          {openPulls.status === "ready" && base !== "" && (
            <p className="create-pr__notice create-pr__stacked" data-testid="create-pr-target">
              <Icon.Info size={12} aria-hidden="true" />
              <span>
                {stacked
                  ? `Stacked on #${stacked.number} “${stacked.title}”. Its closing keywords close no issue when it merges, because GitHub closes an issue only when a pull request merges into the default branch.`
                  : `The pull request goes into ${base}.`}
              </span>
            </p>
          )}

          {/* CPR-FR-HZRN: Submit adds the pull request to the stack. */}
          {toStack && (
            <p className="create-pr__notice create-pr__stack" data-testid="create-pr-stack">
              <Icon.Info size={12} aria-hidden="true" />
              <span>{`Submit adds the pull request to the stack of ${base}.`}</span>
            </p>
          )}

          <label className="create-pr__draft">
            <input
              type="checkbox"
              checked={draft}
              disabled={submitting}
              onChange={(e) => set({ draft: e.target.checked })}
            />
            <span>Draft</span>
          </label>

          <div className="create-pr__links">
            <FeaturesField
              catalog={catalog}
              features={features}
              disabled={submitting}
              labelId={`${ids}-features`}
              onChange={(next) => set({ features: next })}
            />
            <TasksList read={tasks} labelId={`${ids}-tasks`} />
          </div>

          {/* CPR-FR-SQGZ / CPR-FR-VYPG: what stands between the branch and a
              pull request, in words. */}
          <div className="create-pr__state" aria-live="polite" data-testid="create-pr-state">
            {read.status === "loading" && (
              <p className="t-muted" data-testid="create-pr-loading">
                Reading the branch…
              </p>
            )}
            {read.status === "failed" && (
              <p className="create-pr__block" role="alert" data-testid="create-pr-read-error">
                {read.message}
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
            {/* CPR-FR-PLWK: information, not a block. */}
            {pushNotice && (
              <p className="create-pr__notice" data-testid={`create-pr-notice-${pushNotice.key}`}>
                <Icon.Info size={12} aria-hidden="true" />
                <span>{pushNotice.text}</span>
              </p>
            )}
            {read.status !== "loading" && (
              <button
                type="button"
                className="btn btn--ghost btn--sm"
                disabled={submitting}
                data-testid="create-pr-recheck"
                onClick={() => readHead(base)}
              >
                {read.status === "failed" ? "Try again" : "Check again"}
              </button>
            )}
          </div>

          {/* CPR-FR-SQEP: a failure sits directly above the action row. */}
          {failure && (
            <div className="create-pr__failure" role="alert" data-testid="create-pr-failure">
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

        {writeGuard.section}
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
            ref={submitRef}
            className="btn btn--primary btn--sm"
            disabled={!canSubmit}
            aria-describedby={
              [reasons.length > 0 ? reasonId : null, writeGuard.describedBy]
                .filter(Boolean)
                .join(" ") || undefined
            }
            data-testid="create-pr-submit"
            onClick={() => void submit()}
          >
            {submitting ? PHASE_WORDS[phase] : "Submit"}
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

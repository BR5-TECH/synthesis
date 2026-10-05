/**
 * The Ready tasks section of the Git panel (`GIT-git.md` GIT-FR-OGHO through
 * GIT-FR-TZUI).
 *
 * It renders the window's GitHub polling view and offers its actions. It holds
 * no schedule and no claim of its own: both belong to `useGithubPolling`, which
 * the window mounts, because this panel is mounted only while it shows
 * (GIT-FR-CVSB).
 */
import { useEffect } from "react";

import type { GithubPollingController } from "../hooks/useGithubPolling";
import {
  formatInstant,
  githubPollingErrorMessage,
  readyTasksState,
  refreshAvailable,
} from "../state/githubPolling";
import type {
  GithubPendingClaim,
  GithubPollingView,
  GithubReadyTask,
  GithubShadowDraftRow,
} from "../types";
import { Icon } from "./icons";

/** What the shell hands the Git panel for this section. */
export interface ReadyTasksBinding {
  polling: GithubPollingController;
  /** GIT-FR-HNGQ: the route to the GitHub polling section of Project settings. */
  onOpenProjectSettings: () => void;
}

interface ReadyTasksProps extends ReadyTasksBinding {
  /** GIT-FR-OZYT: open a shadow draft's New Artifact tab. */
  onOpenDraft?: (draftId: string) => void;
}

/** GIT-FR-OZYT: a shadow draft's status, in words. */
function shadowStatusText(row: GithubShadowDraftRow): string {
  if (row.status === "graduated") return "Graduated";
  return row.locked ? "Graduating" : "GitHub task, ready to graduate";
}

const repoOf = (owner: string, name: string) => `${owner}/${name}`;

/**
 * The count the Git panel's section control shows: the unclaimed ready tasks.
 */
export function readyTaskCount(view: GithubPollingView | null): number | null {
  if (!view || view.tasks.length === 0) return null;
  return view.tasks.length;
}

/**
 * The narrow column of the section: what the window polls and when. It states
 * the configuration in words, so the wide column can hold the rows alone.
 */
export function ReadyTasksSummary({ polling }: { polling: GithubPollingController }) {
  const view = polling.view;
  if (!view) return null;
  const minutes = view.settings.intervalMinutes;
  return (
    <div className="ready-tasks__summary t-ui-xs" data-testid="ready-tasks-summary">
      <div>
        <span className="t-muted">Project </span>
        {view.settings.projectNodeId === null
          ? "none selected"
          : (view.configuration.projectTitle ?? "selected")}
      </div>
      <div>
        <span className="t-muted">Repository </span>
        {view.repository
          ? repoOf(view.repository.owner, view.repository.name)
          : "not resolved"}
      </div>
      <div>
        <span className="t-muted">Polling </span>
        {minutes === null
          ? "off — Refresh only"
          : `every ${minutes} ${minutes === 1 ? "minute" : "minutes"}`}
      </div>
      <div>
        <span className="t-muted">Last poll </span>
        {formatInstant(view.lastSuccessAt)}
      </div>
    </div>
  );
}

/** GIT-FR-HNGQ / GIT-FR-RUAH / GIT-FR-LUSE … GIT-FR-RYPO: the wide column. */
export function ReadyTasks({ polling, onOpenProjectSettings, onOpenDraft }: ReadyTasksProps) {
  const { view, rowBusy, rowErrors } = polling;

  // GIT-FR-OGHO: the view is read when the section mounts, and again on every
  // `"github polling changed"` — which the window's hook follows.
  useEffect(() => {
    void polling.reload();
    // Mount-only.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const inFlight = polling.pollInFlight || view?.polling === true;
  const canRefresh = refreshAvailable(view);
  const state = readyTasksState(view, polling.pollInFlight);

  /** GIT-FR-RYPO: a disabled Refresh states why, in words. */
  const refreshReason = inFlight
    ? "A poll is running"
    : !view
      ? "The polling state is not read yet"
      : view.settings.projectNodeId === null
        ? "No GitHub Project is selected"
        : view.configuration.state === "invalid"
          ? "The polling configuration has an error"
          : null;

  const settingsRoute = (
    <button
      type="button"
      className="btn btn--default btn--sm"
      onClick={onOpenProjectSettings}
    >
      Open Project settings
    </button>
  );

  const rowError = (issueNumber: number) =>
    rowErrors.get(issueNumber) ? (
      <p className="ready-tasks__row-error t-ui-xs" role="alert">
        {rowErrors.get(issueNumber)}
      </p>
    ) : null;

  const taskRow = (task: GithubReadyTask) => {
    const busy = rowBusy.get(task.issueNumber);
    return (
      <li
        key={`task:${task.issueNumber}`}
        className="ready-tasks__row"
        data-testid={`ready-task-${task.issueNumber}`}
      >
        <div className="ready-tasks__row-main">
          <span className="ready-tasks__title">{task.title}</span>
          <span className="ready-tasks__meta t-ui-xs">
            {repoOf(task.repositoryOwner, task.repositoryName)} · #{task.issueNumber}
          </span>
          <span className="badge badge--ok">{task.status}</span>
        </div>
        <div className="ready-tasks__row-actions">
          <button
            type="button"
            className="ready-tasks__link t-ui-xs"
            disabled={busy !== undefined}
            aria-label={`Open issue #${task.issueNumber} ${task.title} on GitHub`}
            title={task.url}
            onClick={() => polling.openTaskIssue(task.issueNumber)}
          >
            Issue #{task.issueNumber} ↗
          </button>
          <button
            type="button"
            className="btn btn--primary btn--sm"
            disabled={busy !== undefined}
            aria-busy={busy === "claim" || undefined}
            aria-label={`${busy === "claim" ? "Claiming" : "Claim and graduate"} #${task.issueNumber} ${task.title}`}
            onClick={() => void polling.claim(task.issueNumber)}
          >
            {busy === "claim" ? "Claiming…" : "Claim and graduate"}
          </button>
        </div>
        {rowError(task.issueNumber)}
      </li>
    );
  };

  const pendingRow = (claim: GithubPendingClaim) => {
    const busy = rowBusy.get(claim.issueNumber);
    return (
      <li
        key={`pending:${claim.issueNumber}`}
        className="ready-tasks__row"
        data-testid={`pending-claim-${claim.issueNumber}`}
      >
        <div className="ready-tasks__row-main">
          <span className="ready-tasks__title">
            Claim of issue #{claim.issueNumber} is not finished
          </span>
          <span className="ready-tasks__meta t-ui-xs">
            {repoOf(claim.repositoryOwner, claim.repositoryName)} · claimed{" "}
            {formatInstant(claim.claimedAt)}
          </span>
        </div>
        <div className="ready-tasks__row-actions">
          <button
            type="button"
            className="btn btn--default btn--sm"
            disabled={busy !== undefined}
            aria-busy={busy === "retry" || undefined}
            aria-label={`${busy === "retry" ? "Retrying" : "Retry"} claim of issue #${claim.issueNumber}`}
            onClick={() => void polling.retry(claim.issueNumber)}
          >
            {busy === "retry" ? "Retrying…" : "Retry"}
          </button>
        </div>
        {rowError(claim.issueNumber)}
      </li>
    );
  };

  const shadowRow = (row: GithubShadowDraftRow) => {
    const busy = rowBusy.get(row.issueNumber);
    const graduatable = row.status === "github_shadow" && !row.locked;
    return (
      <li
        key={`shadow:${row.draftId}`}
        className="ready-tasks__row"
        data-testid={`shadow-draft-${row.draftId}`}
      >
        <div className="ready-tasks__row-main">
          <span className="ready-tasks__title">{row.name}</span>
          <span className="ready-tasks__meta t-ui-xs">
            {repoOf(row.repositoryOwner, row.repositoryName)} · #{row.issueNumber}
          </span>
          <span className="badge badge--accent">{shadowStatusText(row)}</span>
        </div>
        <div className="ready-tasks__row-actions">
          <button
            type="button"
            className="ready-tasks__link t-ui-xs"
            aria-label={`Open issue #${row.issueNumber} of draft ${row.name} on GitHub`}
            title={row.issueUrl}
            onClick={() =>
              polling.openShadowIssue(row.draftId, row.issueUrl, row.issueNumber)
            }
          >
            Issue #{row.issueNumber} ↗
          </button>
          <button
            type="button"
            className="ready-tasks__link t-ui-xs"
            aria-label={`Open draft ${row.name}`}
            onClick={() => onOpenDraft?.(row.draftId)}
          >
            Open draft
          </button>
          {graduatable && (
            <button
              type="button"
              className="btn btn--primary btn--sm"
              disabled={busy !== undefined}
              aria-label={`Graduate ${row.name} (#${row.issueNumber})`}
              onClick={() => polling.graduateShadow(row.draftId, row.name)}
            >
              <Icon.Graduate size={12} /> Graduate
            </button>
          )}
        </div>
        {rowError(row.issueNumber)}
      </li>
    );
  };

  const showRows =
    state !== "unread" &&
    state !== "not-configured" &&
    state !== "configuration-error";

  return (
    <div className="ready-tasks" data-testid="ready-tasks">
      <div className="git__right-head">
        <span className="t-eyebrow">READY TASKS</span>
        {view?.stale && (
          <span className="badge badge--warn" data-testid="ready-tasks-stale">
            stale
          </span>
        )}
        <span className="spacer" />
        {refreshReason && !inFlight && (
          <span className="t-meta">{refreshReason}</span>
        )}
        <button
          type="button"
          className="btn btn--default btn--sm"
          disabled={!canRefresh || inFlight}
          aria-busy={inFlight || undefined}
          aria-label={
            inFlight
              ? "Refreshing ready tasks"
              : refreshReason
                ? `Refresh ready tasks (unavailable: ${refreshReason})`
                : "Refresh ready tasks"
          }
          onClick={() => void polling.refresh()}
        >
          <Icon.Refresh size={12} /> {inFlight ? "Refreshing…" : "Refresh"}
        </button>
      </div>

      <div className="ready-tasks__body" aria-live="polite">
        {polling.readError && (
          <p className="ready-tasks__error t-ui-sm" role="alert">
            {polling.readError}
          </p>
        )}
        {polling.refreshError && (
          <p className="ready-tasks__error t-ui-sm" role="alert">
            {polling.refreshError}
          </p>
        )}

        {state === "unread" && !polling.readError && (
          <p className="ready-tasks__state t-ui-sm">Loading…</p>
        )}

        {state === "not-configured" && (
          <div className="ready-tasks__state" data-testid="ready-tasks-not-configured">
            <p className="t-ui-sm">
              Not configured: no GitHub Project is selected for polling.
            </p>
            {settingsRoute}
          </div>
        )}

        {state === "configuration-error" && view && (
          <div className="ready-tasks__state" data-testid="ready-tasks-config-error">
            <p className="ready-tasks__error t-ui-sm" role="alert">
              {view.configuration.error ??
                githubPollingErrorMessage(view.configuration.errorCode ?? "")}
            </p>
            <p className="t-ui-sm">Polling is disabled until this is fixed.</p>
            {settingsRoute}
          </div>
        )}

        {showRows && view && (
          <>
            {/* GIT-FR-RUAH: the error of the failed poll, above the rows the
                last successful poll found. */}
            {view.stale && (
              <div className="ready-tasks__stale" data-testid="ready-tasks-stale-note">
                <p className="ready-tasks__error t-ui-sm" role="alert">
                  {view.lastError ??
                    githubPollingErrorMessage(view.lastErrorCode ?? "")}
                </p>
                <p className="t-ui-xs t-muted">
                  {view.lastSuccessAt
                    ? `Stale: these rows are from the last successful poll, at ${formatInstant(view.lastSuccessAt)}.`
                    : "Stale: no poll has succeeded yet, so no ready task is known."}
                </p>
              </div>
            )}

            {state === "loading" && (
              <p className="ready-tasks__state t-ui-sm" aria-busy="true">
                Loading ready tasks…
              </p>
            )}
            {state === "unpolled" && (
              <p className="ready-tasks__state t-ui-sm">
                Not polled yet. Use Refresh to read the ready tasks.
              </p>
            )}
            {state === "empty" && (
              <p className="ready-tasks__state t-ui-sm" data-testid="ready-tasks-empty">
                No ready tasks. The last poll, at {formatInstant(view.lastSuccessAt)},
                found no unclaimed Task with Status “Ready”.
              </p>
            )}
            {state === "rows" && view.tasks.length === 0 && view.stale && view.lastSuccessAt && (
              <p className="ready-tasks__state t-ui-sm">
                The last successful poll found no unclaimed task. That result is
                stale.
              </p>
            )}

            {view.tasks.length > 0 && (
              <ul
                className="ready-tasks__list"
                aria-label="Unclaimed ready tasks"
                data-stale={view.stale || undefined}
              >
                {view.tasks.map(taskRow)}
              </ul>
            )}

            {view.pendingClaims.length > 0 && (
              <>
                <div className="git__section-head">Unfinished claims</div>
                <ul className="ready-tasks__list" aria-label="Unfinished claims">
                  {view.pendingClaims.map(pendingRow)}
                </ul>
              </>
            )}

            {view.shadows.length > 0 && (
              <>
                <div className="git__section-head">GitHub drafts</div>
                <ul className="ready-tasks__list" aria-label="GitHub drafts">
                  {view.shadows.map(shadowRow)}
                </ul>
              </>
            )}
          </>
        )}
      </div>
    </div>
  );
}

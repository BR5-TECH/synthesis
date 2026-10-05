/**
 * The GitHub polling section of Project settings (`SET-project-settings.md`
 * SET-FR-TTTB through SET-FR-GXJU).
 *
 * It selects the GitHub Project whose Ready tasks the main window polls and
 * the interval it polls at. Each change persists at once, outside the section
 * dirty state (SET-FR-VLQJ), so this section holds nothing to save.
 */
import { useCallback, useEffect, useState } from "react";

import * as api from "../api";
import { onGithubPollingChanged } from "../events";
import { logWarn } from "../logging";
import { githubPollingErrorMessage, refusalCode } from "../state/githubPolling";
import {
  GITHUB_POLLING_INTERVALS,
  type GithubPollingInterval,
  type GithubPollingView,
  type GithubProjectOption,
} from "../types";

/** The select's value for "no Project" and for "Off". */
const NONE = "";

/** SET-FR-GKTA: the configuration, in words. */
function configurationText(view: GithubPollingView): {
  text: string;
  error: boolean;
} {
  const { configuration, settings } = view;
  const title = configuration.projectTitle ?? "the selected Project";
  switch (configuration.state) {
    case "unset":
      return {
        text: "No GitHub Project is selected, so nothing is polled.",
        error: false,
      };
    case "invalid":
      return {
        text: `${
          configuration.error ??
          githubPollingErrorMessage(configuration.errorCode ?? "")
        } Polling is disabled until this is fixed.`,
        error: true,
      };
    case "valid":
      return {
        text:
          settings.intervalMinutes === null
            ? `Configured with “${title}”. The interval is Off, so tasks are read only when you use Refresh in the Git panel.`
            : `Configured with “${title}”. Ready tasks are polled every ${settings.intervalMinutes} ${settings.intervalMinutes === 1 ? "minute" : "minutes"}.`,
        error: false,
      };
    default:
      return {
        text: `“${title}” is selected. It is checked on the next poll.`,
        error: false,
      };
  }
}

export function GithubPollingSection() {
  const [view, setView] = useState<GithubPollingView | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const [projects, setProjects] = useState<GithubProjectOption[] | null>(null);
  const [projectsError, setProjectsError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  /** SET-FR-TTTB / SET-FR-GXJU: the stored selection and its configuration. */
  const loadState = useCallback(async () => {
    try {
      setView(await api.getGithubPollingState());
      setReadError(null);
    } catch (e) {
      logWarn(["frontend"], "github polling state could not be read", {
        code: refusalCode(e),
      });
      setReadError(githubPollingErrorMessage(e));
    }
  }, []);

  /** SET-FR-TTTB / SET-FR-GXJU: the Projects the token can read. */
  const loadProjects = useCallback(async () => {
    setProjectsError(null);
    try {
      const list = await api.listGithubProjects();
      setProjects(Array.isArray(list) ? list : []);
    } catch (e) {
      logWarn(["frontend", "remote"], "github projects could not be listed", {
        code: refusalCode(e),
      });
      setProjects(null);
      setProjectsError(githubPollingErrorMessage(e));
    }
  }, []);

  useEffect(() => {
    void loadState();
    void loadProjects();
  }, [loadState, loadProjects]);

  // SET-FR-GXJU: a configuration error a poll found appears without the
  // window being reopened.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onGithubPollingChanged(() => void loadState()).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [loadState]);

  /**
   * SET-FR-VLQJ: persist at once. The controls render the stored values, so a
   * refused write leaves them showing the previous ones.
   */
  const save = async (
    projectNodeId: string | null,
    intervalMinutes: GithubPollingInterval | null,
  ) => {
    setSaving(true);
    setSaveError(null);
    try {
      setView(await api.setGithubPollingSettings(projectNodeId, intervalMinutes));
    } catch (e) {
      logWarn(["frontend"], "github polling settings were refused", {
        code: refusalCode(e),
      });
      setSaveError(githubPollingErrorMessage(e));
    } finally {
      setSaving(false);
    }
  };

  const selected = view?.settings.projectNodeId ?? null;
  const interval = view?.settings.intervalMinutes ?? null;
  // SET-FR-GXJU: a stored selection the list does not hold — the list failed,
  // or the Project is gone — stays selected rather than being dropped.
  const storedMissing =
    selected !== null && !(projects ?? []).some((p) => p.nodeId === selected);
  const report = view ? configurationText(view) : null;

  return (
    <div
      className="card"
      style={{ padding: "14px 16px", marginBottom: 16 }}
      data-testid="settings-github-polling"
    >
      <div style={{ fontSize: "var(--fs-ui-md)", fontWeight: 600, color: "var(--fg-1)" }}>
        Ready tasks from GitHub
      </div>
      <div className="t-ui-sm t-muted" style={{ marginBottom: 12 }}>
        The Git panel lists the open Tasks of this repository whose Status in
        the selected GitHub Project is “Ready”.
      </div>

      {view === null && !readError && <div className="t-muted t-ui-sm">Loading…</div>}
      {readError && (
        <span className="picker-error" role="alert" style={{ display: "block" }}>
          ✗ {readError}
        </span>
      )}

      {view && (
        <div style={{ display: "grid", gridTemplateColumns: "140px 1fr", gap: "10px 12px", alignItems: "center" }}>
          <label className="t-ui-sm" htmlFor="github-polling-project">
            GitHub Project
          </label>
          <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <select
              id="github-polling-project"
              className="input input--sm"
              value={selected ?? NONE}
              disabled={saving}
              onChange={(e) =>
                void save(e.target.value === NONE ? null : e.target.value, interval)
              }
            >
              <option value={NONE}>None</option>
              {storedMissing && selected !== null && (
                <option value={selected}>
                  {view.configuration.projectTitle ?? "The stored Project"}
                </option>
              )}
              {(projects ?? []).map((p) => (
                <option key={p.nodeId} value={p.nodeId}>
                  {p.title} — {p.ownerLogin} (#{p.number})
                </option>
              ))}
            </select>
            {projects === null && !projectsError && (
              <span className="t-ui-xs t-muted">Loading Projects…</span>
            )}
          </div>

          <label className="t-ui-sm" htmlFor="github-polling-interval">
            Polling interval
          </label>
          <select
            id="github-polling-interval"
            className="input input--sm"
            style={{ justifySelf: "start" }}
            value={interval === null ? NONE : String(interval)}
            disabled={saving}
            onChange={(e) =>
              void save(
                selected,
                e.target.value === NONE
                  ? null
                  : (Number(e.target.value) as GithubPollingInterval),
              )
            }
          >
            <option value={NONE}>Off</option>
            {GITHUB_POLLING_INTERVALS.map((m) => (
              <option key={m} value={String(m)}>
                {m} {m === 1 ? "minute" : "minutes"}
              </option>
            ))}
          </select>
        </div>
      )}

      {projectsError && (
        <div style={{ display: "flex", alignItems: "center", gap: 8, marginTop: 10 }}>
          <span className="picker-error" role="alert" data-testid="github-projects-error">
            ✗ {projectsError}
          </span>
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            onClick={() => void loadProjects()}
          >
            Retry
          </button>
        </div>
      )}

      {saveError && (
        <span
          className="picker-error"
          role="alert"
          data-testid="github-polling-save-error"
          style={{ display: "block", marginTop: 10 }}
        >
          ✗ {saveError}
        </span>
      )}

      {report && (
        <p
          className={report.error ? "picker-error t-ui-sm" : "t-ui-sm"}
          data-testid="github-polling-configuration"
          role={report.error ? "alert" : undefined}
          style={{ marginTop: 12, marginBottom: 0 }}
        >
          {report.text}
        </p>
      )}
      {view && view.settings.intervalMinutes === null && view.settings.projectNodeId !== null && view.configuration.state !== "valid" && (
        <p className="t-ui-xs t-muted" style={{ marginTop: 6, marginBottom: 0 }}>
          The interval is Off: nothing is polled on launch or on a timer, and
          Refresh in the Git panel still works.
        </p>
      )}
    </div>
  );
}

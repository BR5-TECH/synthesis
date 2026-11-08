/**
 * Project settings → **Project Agents** → Multi-persona conversations
 * (SET-FR-WOSJ, SET-FR-AGRZ, SET-FR-VUHW).
 *
 * The switch that turns advisor mode and Synthesis on and off together, and the
 * join timeout. Both are stored for this project on this machine only, and both
 * persist at once, outside the section dirty state and the save-before-close
 * sweep.
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../api";
import { logInfo, logWarn } from "../logging";
import type { AgentGroupSettings } from "../types";

/** PSS-FR-CQML: the range the backend accepts, in seconds. */
export const MIN_JOIN_TIMEOUT_SECS = 5;
export const MAX_JOIN_TIMEOUT_SECS = 600;

const RANGE_MESSAGE = `Enter a whole number from ${MIN_JOIN_TIMEOUT_SECS} to ${MAX_JOIN_TIMEOUT_SECS}.`;

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** SET-FR-AGRZ: a whole number of seconds in range, or null. */
export function parseJoinTimeout(text: string): number | null {
  if (!/^\d+$/.test(text.trim())) return null;
  const value = Number(text.trim());
  return value >= MIN_JOIN_TIMEOUT_SECS && value <= MAX_JOIN_TIMEOUT_SECS ? value : null;
}

export function ProjectAgentGroupSettings() {
  const [settings, setSettings] = useState<AgentGroupSettings | null>(null);
  const [timeoutText, setTimeoutText] = useState("");
  const [timeoutError, setTimeoutError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** The last settings the backend stored, which a refusal restores. */
  const confirmed = useRef<AgentGroupSettings | null>(null);
  const writes = useRef<Promise<unknown>>(Promise.resolve());

  // SET-FR-VUHW: read when the section mounts.
  useEffect(() => {
    let cancelled = false;
    api
      .loadAgentGroupSettings()
      .then((loaded) => {
        if (cancelled) return;
        confirmed.current = loaded;
        setSettings(loaded);
        setTimeoutText(String(loaded.joinTimeoutSecs));
      })
      .catch((e) => {
        if (cancelled) return;
        setError(errorText(e));
        logWarn(["frontend"], "agent group settings could not be read", { error: errorText(e) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  /** SET-FR-WOSJ, SET-FR-AGRZ: persist at once; a refusal restores the stored value. */
  const save = (next: AgentGroupSettings) => {
    setSettings(next);
    setError(null);
    writes.current = writes.current
      .catch(() => undefined)
      .then(() => api.saveAgentGroupSettings(next))
      .then((saved) => {
        confirmed.current = saved;
        logInfo(["frontend"], "agent group settings saved", {
          enabled: saved.enabled,
          joinTimeoutSecs: saved.joinTimeoutSecs,
        });
      })
      .catch((e) => {
        const back = confirmed.current;
        if (back) {
          setSettings(back);
          setTimeoutText(String(back.joinTimeoutSecs));
        }
        setError(errorText(e));
        logWarn(["frontend"], "agent group settings not saved", { error: errorText(e) });
      });
  };

  const commitTimeout = () => {
    if (!settings) return;
    const value = parseJoinTimeout(timeoutText);
    if (value === null) {
      // SET-FR-AGRZ: refused inline and not saved.
      setTimeoutError(RANGE_MESSAGE);
      return;
    }
    setTimeoutError(null);
    if (value !== settings.joinTimeoutSecs) save({ ...settings, joinTimeoutSecs: value });
  };

  return (
    <section
      data-testid="project-agent-group-settings"
      aria-labelledby="agent-group-heading"
      style={{ marginBottom: 24, maxWidth: 560 }}
    >
      <h3 id="agent-group-heading" className="t-label">
        Multi-persona conversations
      </h3>
      <p className="t-p" style={{ marginBottom: 12 }}>
        Hear more voices. When you tag one agent, the other agents advise it. When you tag several
        agents or use @all, one of them writes a Synthesis of the replies. The choice belongs to
        you: it is stored on this machine and is never committed.
      </p>

      <label style={{ display: "flex", alignItems: "center", gap: 8 }}>
        <input
          type="checkbox"
          role="switch"
          data-testid="agent-group-switch"
          checked={settings?.enabled ?? false}
          disabled={settings === null}
          onChange={(event) => settings && save({ ...settings, enabled: event.target.checked })}
        />
        <span>Multi-persona conversations</span>
      </label>

      <div style={{ marginTop: 12 }}>
        <label className="t-ui-sm" htmlFor="agent-group-timeout">
          Join timeout (seconds)
        </label>
        <input
          id="agent-group-timeout"
          data-testid="agent-group-timeout"
          className="input"
          type="text"
          inputMode="numeric"
          value={timeoutText}
          disabled={settings === null}
          aria-invalid={timeoutError !== null}
          aria-describedby="agent-group-timeout-note"
          style={{ width: 96, marginLeft: 8 }}
          onChange={(event) => {
            setTimeoutText(event.target.value);
            setTimeoutError(null);
          }}
          onBlur={commitTimeout}
          onKeyDown={(event) => {
            if (event.key === "Enter") commitTimeout();
          }}
        />
        <p id="agent-group-timeout-note" className="t-ui-xs t-muted">
          The wait starts when most of the agents have answered. Agents that have not answered
          when it ends are skipped.
        </p>
        {timeoutError && (
          <p className="t-ui-xs" role="alert" data-testid="agent-group-timeout-error" style={{ color: "var(--danger)" }}>
            {timeoutError}
          </p>
        )}
      </div>

      {error && (
        <p className="t-ui-xs" role="alert" style={{ marginTop: 8, color: "var(--danger)" }}>
          {error}
        </p>
      )}
    </section>
  );
}

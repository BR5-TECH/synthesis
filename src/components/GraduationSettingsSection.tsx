/**
 * The Graduation section of Project settings
 * (`../../specifications/ui/SET-project-settings.md` SET-FR-DHFS, SET-FR-YMRV).
 *
 * It holds one row: the execution time limit, which is the project's shared
 * execution timeout (PSS-FR-TQMV) shown in whole minutes. An interrupted run
 * that reached the limit routes here (GRU-FR-QLRQ), so the author can raise it
 * before they continue the run.
 */

import { useEffect, useRef, useState } from "react";

import * as api from "../api";
import { logInfo, logWarn } from "../logging";

/** SET-FR-YMRV: the shortest limit the field commits, in minutes. */
export const TIME_LIMIT_MIN_MINUTES = 1;
/** SET-FR-YMRV: the longest limit the field commits, in minutes (PSS-FR-TQMV). */
export const TIME_LIMIT_MAX_MINUTES = 360;
/** SET-FR-DHFS: the placeholder, which is the two-hour default in minutes. */
const TIME_LIMIT_PLACEHOLDER = "120";
const MS_PER_MINUTE = 60_000;

/** The stored value as the field shows it: minutes, or empty when unset. */
function shownMinutes(ms: number | null): string {
  return ms === null ? "" : String(ms / MS_PER_MINUTE);
}

/**
 * SET-FR-YMRV: what the typed text commits — `null` to clear back to unset, a
 * whole number of minutes in range as milliseconds, or `undefined` for an
 * entry that is refused.
 */
export function timeLimitToCommit(text: string): number | null | undefined {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  if (!/^\d+$/.test(trimmed)) return undefined;
  const minutes = Number(trimmed);
  if (minutes < TIME_LIMIT_MIN_MINUTES || minutes > TIME_LIMIT_MAX_MINUTES) return undefined;
  return minutes * MS_PER_MINUTE;
}

const REFUSAL = `Enter a whole number of minutes from ${TIME_LIMIT_MIN_MINUTES} to ${TIME_LIMIT_MAX_MINUTES}, or leave the field empty for the default.`;

/** SET-FR-DHFS / SET-FR-YMRV: the Graduation section's one row. */
export function GraduationSettingsSection() {
  /** The stored limit: `undefined` while the read is outstanding. */
  const [stored, setStored] = useState<number | null | undefined>(undefined);
  const [text, setText] = useState("");
  const [error, setError] = useState("");
  const [loadError, setLoadError] = useState("");
  /** A commit on its way, so Enter and the blur after it send one write. */
  const inFlight = useRef(false);

  useEffect(() => {
    let cancelled = false;
    api
      .loadProjectConfig()
      .then((config) => {
        if (cancelled) return;
        const ms = config.executionTimeoutMs ?? null;
        setStored(ms);
        setText(shownMinutes(ms));
      })
      .catch((e) => {
        if (cancelled) return;
        setLoadError(String(e));
        logWarn(["frontend"], "the execution time limit could not be read", {
          error: String(e),
        });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const shown = shownMinutes(stored ?? null);
  // The stored value as it is shown is never an entry to refuse, even where it
  // is not a whole number of minutes: nothing was typed.
  const unchanged = text.trim() === shown;
  const toCommit = timeLimitToCommit(text);
  const invalid = !unchanged && toCommit === undefined;

  const commit = async () => {
    if (inFlight.current || stored === undefined) return;
    if (unchanged || toCommit === (stored ?? null)) {
      setText(shown);
      setError("");
      return;
    }
    // SET-FR-YMRV: a refused entry is said inline, and the stored value stays.
    if (toCommit === undefined) {
      setError(REFUSAL);
      return;
    }
    setError("");
    inFlight.current = true;
    try {
      // The save writes the line-ending convention every time, so it carries
      // the stored value through unchanged (PSS-FR-17). A payload that names
      // no concurrency limit leaves it as it stands (PSS-FR-ZVSD).
      const current = await api.loadProjectConfig();
      await api.saveProjectConfig({
        lineEndings: current.lineEndings,
        executionTimeoutMs: toCommit,
      });
      setStored(toCommit);
      setText(shownMinutes(toCommit));
      logInfo(["frontend"], "the execution time limit was saved", {
        executionTimeoutMs: toCommit,
      });
    } catch (e) {
      setText(shown);
      setError(String(e));
      logWarn(["frontend"], "the execution time limit could not be saved", {
        error: String(e),
      });
    } finally {
      inFlight.current = false;
    }
  };

  if (loadError) {
    return (
      <span className="picker-error" data-testid="graduation-time-limit-load-error">
        ✗ {loadError}
      </span>
    );
  }

  return (
    <>
      <div className="picker-field" style={{ marginBottom: 4 }}>
        <label className="picker-field__label" htmlFor="graduation-time-limit">
          Execution time limit
        </label>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <input
            id="graduation-time-limit"
            className="input"
            type="text"
            inputMode="numeric"
            autoComplete="off"
            data-testid="graduation-time-limit"
            aria-invalid={invalid || undefined}
            aria-describedby="graduation-time-limit-note"
            disabled={stored === undefined}
            placeholder={TIME_LIMIT_PLACEHOLDER}
            style={{
              width: 96,
              borderColor: invalid ? "var(--danger, #d9534f)" : undefined,
            }}
            value={text}
            onChange={(e) => {
              setText(e.target.value);
              setError("");
            }}
            onBlur={() => void commit()}
            onKeyDown={(e) => {
              if (e.key === "Enter") void commit();
            }}
          />
          <span className="t-ui-xs t-muted">min</span>
        </div>
      </div>
      <div
        id="graduation-time-limit-note"
        className="t-ui-xs t-muted"
        data-testid="graduation-time-limit-note"
        style={{ marginBottom: error ? 4 : 18 }}
      >
        Bounds each graduation turn, and each conversation turn whose provider
        sets no turn timeout. Empty uses 2 hours.
      </div>
      {error && (
        <span
          className="picker-error"
          role="alert"
          style={{ display: "block", marginBottom: 18 }}
          data-testid="graduation-time-limit-error"
        >
          ✗ {error}
        </span>
      )}
    </>
  );
}

import { useEffect, useRef, useState } from "react";

import { logWarn } from "../logging";

/** GLS-FR-DLMP, GLS-FR-UTNL: the smallest percent a limit field commits. */
export const PERCENT_LIMIT_MIN = 1;
/** GLS-FR-DLMP, GLS-FR-UTNL: the largest percent a limit field commits. */
export const PERCENT_LIMIT_MAX = 100;

/**
 * GLS-FR-WQHT, GLS-FR-QHNI: what the typed text commits — a whole percent in
 * range, or `undefined` for an entry that commits nothing. An empty field
 * commits nothing: the setting always holds a value.
 */
export function percentLimitToCommit(text: string): number | undefined {
  const trimmed = text.trim();
  if (!/^\d+$/.test(trimmed)) return undefined;
  const percent = Number(trimmed);
  if (percent < PERCENT_LIMIT_MIN || percent > PERCENT_LIMIT_MAX) return undefined;
  return percent;
}

/** What one percent limit field reads, writes, and says. */
export interface PercentLimitConfig {
  /** The label of the field. */
  label: string;
  /** The prefix of every element id and test id of the row. */
  idPrefix: string;
  /** The note below the field. */
  note: string;
  /** The setting in words, for the log records. */
  logName: string;
  load: () => Promise<number>;
  save: (percent: number) => Promise<number>;
}

/** A refusal in words the author can act on. */
function describeRefusal(e: unknown): string {
  return String(e).includes("percent_out_of_range")
    ? "Enter a whole number from 1 to 100."
    : "The limit could not be saved.";
}

/**
 * GLS-FR-DLMP / GLS-FR-WQHT, GLS-FR-UTNL / GLS-FR-QHNI: one percent limit of
 * the AI API section, one setting for every provider.
 *
 * The typed text is held locally and committed on Enter or on blur, so a value
 * on its way somewhere — `3` while typing `30` — does not reach the backend. It
 * applies at once, with no section save. A refused save restores the stored
 * value and shows the refusal beside the field.
 */
export function AiApiPercentLimitRow({ config }: { config: PercentLimitConfig }) {
  /** The stored value, or `null` until the first read answers. */
  const [stored, setStored] = useState<number | null>(null);
  const [text, setText] = useState("");
  const [error, setError] = useState("");
  /** A commit on its way, so Enter and the blur after it send one call. */
  const inFlight = useRef(false);

  useEffect(() => {
    let cancelled = false;
    Promise.resolve()
      .then(() => config.load())
      .then((percent) => {
        if (cancelled || typeof percent !== "number") return;
        setStored(percent);
        setText(String(percent));
      })
      .catch((e) => {
        if (cancelled) return;
        logWarn(["frontend", "ai"], `the ${config.logName} could not be read`, {
          error: String(e),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [config]);

  const shown = stored === null ? "" : String(stored);
  const unchanged = text.trim() === shown;
  const toCommit = percentLimitToCommit(text);
  const invalid = !unchanged && toCommit === undefined;

  const commit = async () => {
    if (inFlight.current || stored === null) return;
    if (unchanged || toCommit === undefined || toCommit === stored) {
      setText(shown);
      return;
    }
    setError("");
    inFlight.current = true;
    try {
      const saved = await config.save(toCommit);
      setStored(saved);
      setText(String(saved));
    } catch (e) {
      // GLS-FR-WQHT: the field keeps the value it held before.
      setText(shown);
      setError(describeRefusal(e));
      logWarn(["frontend", "ai"], `the ${config.logName} was refused`, {
        percent: toCommit,
        error: String(e),
      });
    } finally {
      inFlight.current = false;
    }
  };

  const fieldId = config.idPrefix;
  return (
    <div style={{ marginBottom: 16 }} data-testid={`${fieldId}-row`}>
      <div className="picker-field" style={{ marginBottom: 4 }}>
        <label className="picker-field__label" htmlFor={fieldId}>
          {config.label}
        </label>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <input
            id={fieldId}
            className="input"
            type="text"
            inputMode="numeric"
            autoComplete="off"
            data-testid={fieldId}
            aria-invalid={invalid || undefined}
            aria-describedby={`${fieldId}-note`}
            disabled={stored === null}
            style={{
              width: 72,
              borderColor: invalid ? "var(--danger, #d9534f)" : undefined,
            }}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onBlur={() => void commit()}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !invalid) void commit();
            }}
          />
          <span className="t-ui-xs t-muted">% of the context window</span>
        </div>
      </div>
      <div id={`${fieldId}-note`} className="t-ui-xs t-muted" data-testid={`${fieldId}-note`}>
        {config.note}
      </div>
      {invalid && (
        <span
          className="picker-error"
          style={{ display: "block", marginTop: 4 }}
          data-testid={`${fieldId}-invalid`}
        >
          Enter a whole number from 1 to 100.
        </span>
      )}
      {error && (
        <span
          className="picker-error"
          style={{ display: "block", marginTop: 4 }}
          data-testid={`${fieldId}-error`}
        >
          ✗ {error}
        </span>
      )}
    </div>
  );
}

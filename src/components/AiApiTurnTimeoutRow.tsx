import { useEffect, useRef, useState } from "react";

import * as api from "../api";
import { logWarn } from "../logging";
import type { AiApiIntegration, AiApiProviderId } from "../types";

/** AII-FR-QSOR: the shortest turn timeout the field commits, in seconds. */
export const TURN_TIMEOUT_MIN_S = 30;
/** AII-FR-QSOR: the longest turn timeout the field commits, in seconds. */
export const TURN_TIMEOUT_MAX_S = 3600;
/** AII-FR-QSOR: the placeholder, which is the default bound in seconds. */
const TURN_TIMEOUT_PLACEHOLDER = "300";

/** The stored value as the field shows it: whole seconds, or empty. */
function shownSeconds(turnTimeoutMs: number | null): string {
  return turnTimeoutMs === null ? "" : String(turnTimeoutMs / 1000);
}

/**
 * AII-FR-QSOR: what the typed text commits — `null` to clear, a whole number of
 * milliseconds in range, or `undefined` for an entry that commits nothing.
 */
export function turnTimeoutToCommit(text: string): number | null | undefined {
  const trimmed = text.trim();
  if (trimmed === "") return null;
  if (!/^\d+$/.test(trimmed)) return undefined;
  const seconds = Number(trimmed);
  if (seconds < TURN_TIMEOUT_MIN_S || seconds > TURN_TIMEOUT_MAX_S) return undefined;
  return seconds * 1000;
}

interface AiApiTurnTimeoutRowProps {
  provider: AiApiProviderId;
  turnTimeoutMs: number | null;
  /** The record the backend returned after a committed change. */
  onSaved: (integration: AiApiIntegration) => void;
  /** Names a typed refusal in terms the author can act on. */
  describeError: (e: unknown) => string;
  /**
   * The refusal shown beside this provider's field. The section holds it per
   * provider, so a refusal that arrives after the author moved to another tab
   * is still waiting on this one when they come back (AII-FR-28).
   */
  error: string;
  onError: (provider: AiApiProviderId, message: string) => void;
}

/**
 * AII-FR-QSOR / AII-FR-QTZF: the turn timeout of one provider, in whole
 * seconds.
 *
 * The typed text is held locally and committed on Enter or on blur, so a value
 * on its way somewhere — `30` while typing `300` — does not reach the backend.
 * It applies at once, with no section save (AII-FR-28), and a refusal keeps the
 * stored value and renders its error beside the field.
 */
export function AiApiTurnTimeoutRow({
  provider,
  turnTimeoutMs,
  onSaved,
  describeError,
  error,
  onError,
}: AiApiTurnTimeoutRowProps) {
  const [text, setText] = useState(shownSeconds(turnTimeoutMs));

  // Adopt a value changed from outside the field: another tab's record
  // arriving, a Clear, or the stored value after a commit.
  useEffect(() => {
    setText(shownSeconds(turnTimeoutMs));
  }, [turnTimeoutMs]);

  /** A commit on its way, so Enter and the blur after it send one call. */
  const inFlight = useRef(false);

  const shown = shownSeconds(turnTimeoutMs);
  // The stored value as it is shown is never an entry to refuse, even where it
  // is not a whole number of seconds: nothing was typed.
  const unchanged = text.trim() === shown;
  const toCommit = turnTimeoutToCommit(text);
  const invalid = !unchanged && toCommit === undefined;
  const fieldId = `ai-api-turn-timeout-${provider}`;

  const commit = async () => {
    if (inFlight.current) return;
    // AII-FR-QTZF: an entry that commits nothing goes back to the stored value,
    // as does an entry that only spells the stored value differently.
    if (unchanged || toCommit === undefined || toCommit === turnTimeoutMs) {
      setText(shown);
      return;
    }
    onError(provider, "");
    inFlight.current = true;
    try {
      onSaved(await api.setAiApiTurnTimeout(provider, toCommit));
    } catch (e) {
      // AII-FR-28: the field keeps the value it held before, and the typed
      // error is rendered beside it.
      setText(shownSeconds(turnTimeoutMs));
      onError(provider, describeError(e));
      logWarn(["frontend", "ai"], "ai api turn timeout refused", {
        provider,
        error: String(e),
      });
    } finally {
      inFlight.current = false;
    }
  };

  return (
    <>
      <div className="picker-field" style={{ marginBottom: 4 }}>
        <label className="picker-field__label" htmlFor={fieldId}>
          Turn timeout
        </label>
        <div style={{ display: "flex", gap: 8, alignItems: "center" }}>
          <input
            id={fieldId}
            className="input"
            type="text"
            inputMode="numeric"
            autoComplete="off"
            data-testid="ai-api-turn-timeout"
            aria-invalid={invalid || undefined}
            placeholder={TURN_TIMEOUT_PLACEHOLDER}
            style={{
              width: 96,
              borderColor: invalid ? "var(--danger, #d9534f)" : undefined,
            }}
            value={text}
            onChange={(e) => setText(e.target.value)}
            onBlur={() => void commit()}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !invalid) void commit();
            }}
          />
          <span className="t-ui-xs t-muted">s</span>
        </div>
      </div>
      <div
        className="t-ui-xs t-muted"
        data-testid="ai-api-turn-timeout-note"
        style={{ marginBottom: error ? 4 : 18 }}
      >
        Bounds each agent conversation turn on this provider. Empty uses the
        project&apos;s execution timeout, or 5 minutes.
      </div>
      {error && (
        <span
          className="picker-error"
          style={{ display: "block", marginBottom: 18 }}
          data-testid="ai-api-turn-timeout-error"
        >
          ✗ {error}
        </span>
      )}
    </>
  );
}

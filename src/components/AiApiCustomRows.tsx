import { useRef, useState } from "react";

import * as api from "../api";
import { logWarn } from "../logging";
import type { AiApiIntegration, AiApiProviderId, ReasoningChoice } from "../types";
import { FilterableSelect } from "./FilterableSelect";
import { customReasoningOptions, tokenLimitToCommit } from "./aiApiCustom";

/** AII-FR-44 / AII-FR-XGIM: the entry meaning "send no reasoning". */
const CUSTOM_MODEL_DEFAULT_LABEL = "Model default";

/** The selector entry id a stored Custom choice renders as. */
function customReasoningValue(choice: ReasoningChoice | null): string {
  return choice?.kind === "effort" ? `effort:${choice.effort}` : "";
}

interface CustomReasoningRowProps {
  selectedReasoning: ReasoningChoice | null;
  onSelect: (entryId: string) => void;
}

/**
 * AII-FR-XGIM: the Custom tab's reasoning effort. It is the default for every
 * Custom model, so it renders with or without a selected model, and an
 * agent's own reasoning wins over it.
 */
export function AiApiCustomReasoningRow({
  selectedReasoning,
  onSelect,
}: CustomReasoningRowProps) {
  return (
    <>
      <div className="picker-field" style={{ marginBottom: 4 }}>
        <label className="picker-field__label">Reasoning effort</label>
        <FilterableSelect
          label="Reasoning effort"
          testId="ai-api-reasoning"
          value={customReasoningValue(selectedReasoning)}
          options={customReasoningOptions(CUSTOM_MODEL_DEFAULT_LABEL)}
          onChange={onSelect}
        />
      </div>
      <div
        className="t-ui-xs t-muted"
        data-testid="ai-api-reasoning-note"
        style={{ marginBottom: 18 }}
      >
        The default for every Custom model. An agent&apos;s own reasoning wins
        over it.
      </div>
    </>
  );
}

/** Which of the two token fields a row edits. */
export type TokenLimitKind = "maxOutputTokens" | "maxContextSize";

const TOKEN_FIELDS: Record<
  TokenLimitKind,
  {
    label: string;
    testId: string;
    placeholder: string;
    note: string;
    save: (provider: AiApiProviderId, tokens: number | null) => Promise<AiApiIntegration>;
  }
> = {
  // AII-FR-FBVE: the placeholder is the limit an empty field uses.
  maxOutputTokens: {
    label: "Max output tokens",
    testId: "ai-api-max-output-tokens",
    placeholder: "128000",
    note: "The output limit of every Custom call. Empty uses 128000.",
    save: (provider, tokens) => api.setAiApiMaxOutputTokens(provider, tokens),
  },
  // AII-FR-FBVE: the placeholder is the default window an empty field uses.
  maxContextSize: {
    label: "Max context size",
    testId: "ai-api-max-context-size",
    placeholder: "1000000",
    note:
      "Wins over the context size the model reports. It is the window for " +
      "the document size limit and the history budget. Never sent to the host.",
    save: (provider, tokens) => api.setAiApiMaxContextSize(provider, tokens),
  },
};

/** The stored value as the field shows it. */
function shown(tokens: number | null): string {
  return tokens === null ? "" : String(tokens);
}

interface TokenLimitRowProps {
  kind: TokenLimitKind;
  /** The provider of the tab the field stands in. */
  provider: AiApiProviderId;
  tokens: number | null;
  /** The record the backend returned after a committed change. */
  onSaved: (integration: AiApiIntegration) => void;
  /** Names a typed refusal in terms the author can act on. */
  describeError: (e: unknown) => string;
}

/**
 * AII-FR-UQLF / AII-FR-TWIR / AII-FR-FBVE: one token field of a provider tab.
 *
 * The typed text is held locally and committed on Enter or on blur, so a value
 * on its way somewhere does not reach the backend. It applies at once, with no
 * section save (AII-FR-28). An entry that is not a valid value shows an inline
 * message and commits nothing.
 */
export function AiApiTokenLimitRow({
  kind,
  provider,
  tokens,
  onSaved,
  describeError,
}: TokenLimitRowProps) {
  const field = TOKEN_FIELDS[kind];
  const [text, setText] = useState(shown(tokens));
  const [error, setError] = useState("");

  // Adopt a value changed from outside the field — a Clear, or the stored
  // value after a commit — during the render, as the turn timeout row does.
  const [adopted, setAdopted] = useState(tokens);
  if (!Object.is(adopted, tokens)) {
    setAdopted(tokens);
    setText(shown(tokens));
  }

  const inFlight = useRef(false);
  const unchanged = text.trim() === shown(tokens);
  const toCommit = tokenLimitToCommit(text);
  const invalid = !unchanged && toCommit === undefined;
  const fieldId = `ai-api-${field.testId}`;

  const commit = async () => {
    if (inFlight.current) return;
    // AII-FR-TWIR: an entry that commits nothing goes back to the stored value.
    if (unchanged || toCommit === undefined || toCommit === tokens) {
      setText(shown(tokens));
      return;
    }
    setError("");
    inFlight.current = true;
    try {
      onSaved(await field.save(provider, toCommit));
    } catch (e) {
      // AII-FR-28: the field keeps the value it held before, and the typed
      // error is rendered beside it.
      setText(shown(tokens));
      setError(describeError(e));
      logWarn(["frontend", "ai"], "ai api token limit refused", {
        provider,
        field: kind,
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
          {field.label}
        </label>
        <input
          id={fieldId}
          className="input"
          type="text"
          inputMode="numeric"
          autoComplete="off"
          data-testid={field.testId}
          aria-invalid={invalid || undefined}
          placeholder={field.placeholder}
          style={{
            width: 140,
            borderColor: invalid ? "var(--danger, #d9534f)" : undefined,
          }}
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={() => void commit()}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !invalid) void commit();
          }}
        />
      </div>
      {invalid && (
        <span
          className="picker-error"
          style={{ display: "block", marginBottom: 4 }}
          data-testid={`${field.testId}-invalid`}
        >
          ✗ Enter a whole number from 1 to 4294967295, or leave the field empty.
        </span>
      )}
      <div
        className="t-ui-xs t-muted"
        data-testid={`${field.testId}-note`}
        style={{ marginBottom: error ? 4 : 18 }}
      >
        {field.note}
      </div>
      {error && (
        <span
          className="picker-error"
          style={{ display: "block", marginBottom: 18 }}
          data-testid={`${field.testId}-error`}
        >
          ✗ {error}
        </span>
      )}
    </>
  );
}

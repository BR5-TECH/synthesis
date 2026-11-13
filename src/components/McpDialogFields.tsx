/**
 * The parts of the MCP server dialog that repeat: a field with its problem
 * line, a secret field, and the environment and header lists
 * (`MSS-mcp-servers-settings.md` MSS-FR-BHNZ, MSS-FR-IUMU, MSS-FR-BEQD).
 */
import { useId, type ReactNode } from "react";

import type { McpListRow, McpSecretInput } from "../state/mcpValidation";
import { newRow } from "../state/mcpValidation";

/** MSS-FR-BEQD: the problem line under one field. */
export function ProblemLine({ id, text }: { id: string; text: string | null | undefined }) {
  if (!text) return null;
  return (
    <p id={id} className="mcp-dialog__problem" data-testid="mcp-field-problem">
      {text}
    </p>
  );
}

/** One labelled field of the dialog, with its problem under it. */
export function Field({
  label,
  hint,
  problem,
  children,
}: {
  label: string;
  hint?: string;
  problem?: string | null;
  children: (props: { id: string; describedBy: string | undefined; invalid: boolean }) => ReactNode;
}) {
  const id = useId();
  const problemId = `${id}-problem`;
  return (
    <div className="mcp-dialog__field">
      <label className="mcp-dialog__label" htmlFor={id}>
        {label}
      </label>
      <div className="mcp-dialog__control">
        {children({ id, describedBy: problem ? problemId : undefined, invalid: Boolean(problem) })}
        {hint && <p className="t-ui-xs t-muted mcp-dialog__hint">{hint}</p>}
        <ProblemLine id={problemId} text={problem} />
      </div>
    </div>
  );
}

/**
 * MSS-FR-IUMU: the state of a secret beside its field. The field itself is
 * always empty: a stored value never comes back to the UI.
 */
export function SecretState({
  present,
  input,
  onChange,
  label,
}: {
  present: boolean;
  input: McpSecretInput;
  onChange: (next: McpSecretInput) => void;
  label: string;
}) {
  if (input.value !== "") {
    return <span className="mcp-dialog__secret-state t-ui-xs">Replaced when you save</span>;
  }
  if (input.clear) {
    return (
      <span className="mcp-dialog__secret-state t-ui-xs">
        Cleared when you save ·{" "}
        <button
          type="button"
          className="mcp-dialog__link"
          aria-label={`Keep ${label}`}
          onClick={() => onChange({ ...input, clear: false })}
        >
          Keep
        </button>
      </span>
    );
  }
  return (
    <span className="mcp-dialog__secret-state t-ui-xs">
      {present ? "Set on this computer" : "Not set"}
      {present && (
        <>
          {" · "}
          <button
            type="button"
            className="mcp-dialog__link"
            aria-label={`Clear ${label}`}
            onClick={() => onChange({ ...input, clear: true })}
          >
            Clear
          </button>
        </>
      )}
    </span>
  );
}

/** MSS-FR-HIWH, MSS-FR-IUMU: a secret field that is not a list row (`Token`, `Client secret`). */
export function SecretField({
  label,
  slot,
  present,
  input,
  problem,
  onChange,
}: {
  label: string;
  slot: string;
  present: boolean;
  input: McpSecretInput;
  problem?: string | null;
  onChange: (next: McpSecretInput) => void;
}) {
  return (
    <Field label={label} problem={problem}>
      {({ id, describedBy, invalid }) => (
        <div className="mcp-dialog__secret">
          <input
            id={id}
            className="input input--mono"
            type="password"
            autoComplete="off"
            data-secret-slot={slot}
            value={input.value}
            aria-invalid={invalid}
            aria-describedby={describedBy}
            onChange={(event) => onChange({ ...input, value: event.target.value })}
          />
          <SecretState present={present} input={input} onChange={onChange} label={label} />
        </div>
      )}
    </Field>
  );
}

export interface KeyValueListProps {
  /** "Environment" or "Headers". */
  title: string;
  /** "variable" or "header", for the accessible names of the row controls. */
  noun: string;
  /** The prefix of the slot of a secret row: `env` or `header`. */
  slotPrefix: "env" | "header";
  rows: McpListRow[];
  /** The rows of the list carry problems under the field key `<slotPrefix>:<row key>`. */
  problems: Record<string, string>;
  /** Whether this computer has the value of a slot. */
  present: (slot: string) => boolean;
  onChange: (rows: McpListRow[]) => void;
  /** MSS-FR-BEQD: the author left a field of a row. */
  onTouch: (field: string) => void;
}

/**
 * MSS-FR-BHNZ: the environment list or the header list. Each row has a name, a
 * value, and a `Secret` check box. A secret row shows the state of its stored
 * value (MSS-FR-IUMU) and an empty field.
 */
export function KeyValueList({
  title,
  noun,
  slotPrefix,
  rows,
  problems,
  present,
  onChange,
  onTouch,
}: KeyValueListProps) {
  const baseId = useId();
  const update = (key: string, patch: Partial<McpListRow>) => {
    onChange(rows.map((row) => (row.key === key ? { ...row, ...patch } : row)));
    onTouch(`${slotPrefix}:${key}`);
  };
  return (
    <fieldset className="mcp-dialog__list" data-testid={`mcp-${slotPrefix}-list`}>
      <legend className="t-label">{title}</legend>
      {rows.length === 0 && (
        <p className="t-ui-xs t-muted mcp-dialog__hint">No {noun}s.</p>
      )}
      {rows.map((row, index) => {
        const field = `${slotPrefix}:${row.key}`;
        const problemId = `${baseId}-${row.key}-problem`;
        const slot = `${slotPrefix}:${row.name.trim()}`;
        const label = row.name.trim() || `${noun} ${index + 1}`;
        const problem = problems[field];
        return (
          <div key={row.key} className="mcp-dialog__list-item" data-testid={`mcp-${slotPrefix}-row`}>
            <div className="mcp-dialog__list-row">
              <input
                className="input input--mono"
                aria-label={`Name of ${noun} ${index + 1}`}
                value={row.name}
                aria-invalid={Boolean(problem)}
                aria-describedby={problem ? problemId : undefined}
                onChange={(event) => update(row.key, { name: event.target.value })}
                onBlur={() => onTouch(field)}
              />
              <input
                className="input input--mono"
                aria-label={`Value of ${label}`}
                type={row.secret ? "password" : "text"}
                autoComplete="off"
                data-secret-slot={row.secret ? slot : undefined}
                value={row.value}
                onChange={(event) => update(row.key, { value: event.target.value })}
                onBlur={() => onTouch(field)}
              />
              <label className="mcp-dialog__secret-check t-ui-sm">
                <input
                  type="checkbox"
                  aria-label={`Secret ${label}`}
                  checked={row.secret}
                  onChange={(event) => update(row.key, { secret: event.target.checked, clear: false })}
                />
                Secret
              </label>
              <button
                type="button"
                className="btn btn--ghost btn--sm"
                aria-label={`Remove ${noun} ${label}`}
                onClick={() => onChange(rows.filter((r) => r.key !== row.key))}
              >
                ✕
              </button>
            </div>
            {row.secret && (
              <SecretState
                present={present(slot)}
                input={row}
                label={label}
                onChange={(next) => update(row.key, { value: next.value, clear: next.clear })}
              />
            )}
            <ProblemLine id={problemId} text={problem} />
          </div>
        );
      })}
      <button
        type="button"
        className="btn btn--ghost btn--sm btn--inline-start"
        onClick={() => onChange([...rows, newRow()])}
      >
        + Add {noun}
      </button>
    </fieldset>
  );
}

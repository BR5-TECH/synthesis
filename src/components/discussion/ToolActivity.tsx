/**
 * The tool list of an agent turn (`CTA-comment-agent-turns.md` CTA-FR-GGYJ,
 * CTA-FR-BJWN, CTA-FR-FPNP, CTA-FR-QMWO).
 *
 * A disclosure control whose label is the status line of a pending
 * contribution or the fixed text of an activity row. It is collapsed when it
 * first renders. Expanded, it lists each call with its tool name, its key
 * input as plain text, and its state as a word.
 */
import { useId, useState, type ReactNode } from "react";

import { Icon } from "../icons";
import {
  activityRowLabel,
  toolCallStateLabel,
  type ActivityRow,
} from "../../state/turnActivity";
import type { TurnToolCall } from "../../types";

export interface ToolCallListProps {
  calls: readonly TurnToolCall[];
}

/** CTA-FR-BJWN / CTA-FR-FPNP: one entry per call, in the order given. */
export function ToolCallList({ calls }: ToolCallListProps) {
  return (
    <ul className="tool-calls" data-testid="tool-call-list">
      {calls.map((call) => (
        <li
          key={call.id}
          className="tool-calls__entry"
          data-testid="tool-call-entry"
          data-state={call.state}
        >
          <span className="tool-calls__name">{call.tool}</span>
          {/* CTA-FR-FPNP: plain text, wrapped. React escapes it, and nothing
              here reads it as Markdown or HTML. */}
          {call.keyInput && (
            <span className="tool-calls__input" data-testid="tool-call-input">
              {call.keyInput}
            </span>
          )}
          <span className="tool-calls__state" data-testid="tool-call-state">
            {toolCallStateLabel(call.state)}
          </span>
        </li>
      ))}
    </ul>
  );
}

export interface ToolDisclosureProps {
  /** The text of the control: a status line, or an activity row's text. */
  label: ReactNode;
  calls: readonly TurnToolCall[];
  className?: string;
  testId?: string;
}

/**
 * CTA-FR-GGYJ: a native button, so a click, Enter, and Space each toggle it.
 * It carries `aria-expanded` and controls the list it shows.
 */
export function ToolDisclosure({ label, calls, className, testId }: ToolDisclosureProps) {
  const [expanded, setExpanded] = useState(false);
  const listId = useId();
  return (
    <>
      <button
        type="button"
        className={`tool-disclosure ${className ?? ""}`.trim()}
        data-testid={testId}
        aria-expanded={expanded}
        aria-controls={listId}
        onClick={() => setExpanded((open) => !open)}
      >
        <Icon.CaretRight
          size={11}
          className="icon tool-disclosure__chevron"
          aria-hidden="true"
        />
        <span>{label}</span>
      </button>
      {expanded && (
        <div id={listId}>
          <ToolCallList calls={calls} />
        </div>
      )}
    </>
  );
}

export interface ActivityRowsProps {
  rows: readonly ActivityRow[];
  /** `stream` for the draft discussion column, which draws its own rows. */
  variant?: "card" | "stream";
}

/**
 * CTA-FR-ZBIB / CTA-FR-QMWO / CTA-FR-ZOGA: the activity rows at the foot of a
 * discussion, in the order their turns ended. Each is a disclosure of that
 * turn's tool list, with no timestamp, no cancel control, and no Retry.
 */
export function ActivityRows({ rows, variant = "card" }: ActivityRowsProps) {
  if (rows.length === 0) return null;
  const rowClass = variant === "stream" ? "dds-message dds-activity" : "comment comment--activity";
  return (
    <>
      {rows.map((row) => (
        <div
          key={row.turnId}
          className={rowClass}
          data-testid="activity-row"
          data-turn-id={row.turnId}
        >
          <ToolDisclosure
            label={activityRowLabel(row)}
            calls={row.calls}
            testId="activity-row-toggle"
          />
        </div>
      ))}
    </>
  );
}

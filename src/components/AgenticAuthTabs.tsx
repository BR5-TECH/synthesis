import type { ReactNode } from "react";
import { SETTINGS_TABLIST_STYLE, settingsTabStyle } from "./settingsTabs";
import { toneColor } from "./aiIntegrationsShared";

/**
 * The parts the credential sub-tabs of the agentic CLI tabs share —
 * `specifications/ui/AII-ai-integrations.md` AII-FR-IUUM (Claude Code) and
 * AII-FR-LOYM (Codex), which renders its strip "in the form of AII-FR-IUUM".
 *
 * Choosing a sub-tab only changes the tab's candidate. Nothing is committed
 * until Verify succeeds.
 */

/** One sub-tab of a strip: the mode it stands for and the label it shows. */
export interface AuthTab<M extends string> {
  mode: M;
  label: string;
}

/** AII-FR-IUUM / AII-FR-LOYM: the strip that chooses a credential shape. */
export function AgenticAuthTabs<M extends string>({
  ariaLabel,
  tabs,
  mode,
  onChoose,
}: {
  ariaLabel: string;
  tabs: ReadonlyArray<AuthTab<M>>;
  mode: M;
  onChoose: (mode: M) => void;
}) {
  return (
    <div
      role="tablist"
      aria-label={ariaLabel}
      style={{ ...SETTINGS_TABLIST_STYLE, marginBottom: 14 }}
    >
      {tabs.map((tab) => (
        <button
          key={tab.mode}
          role="tab"
          aria-selected={tab.mode === mode}
          className="btn btn--ghost btn--sm"
          data-testid={`agentic-auth-tab-${tab.mode}`}
          style={settingsTabStyle(tab.mode === mode)}
          onClick={() => onChoose(tab.mode)}
        >
          {tab.label}
        </button>
      ))}
    </div>
  );
}

/** One labelled field in the shape the other agentic fields use. */
export function AgenticField({
  id,
  label,
  note,
  noteId,
  noteTestId,
  warn,
  marginBottom = 10,
  children,
}: {
  id: string;
  label: string;
  note?: string;
  noteId?: string;
  noteTestId?: string;
  warn?: boolean;
  marginBottom?: number;
  children: ReactNode;
}) {
  return (
    <div className="picker-field" style={{ marginBottom }}>
      <label className="picker-field__label" htmlFor={id}>
        {label}
      </label>
      {children}
      {note && (
        <div
          id={noteId}
          className="t-ui-xs"
          data-testid={noteTestId}
          style={{ marginTop: 4, color: toneColor(warn ? "warn" : "muted") }}
        >
          {note}
        </div>
      )}
    </div>
  );
}

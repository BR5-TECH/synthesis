import type { EffortOption } from "../types";
import { shownEffort } from "./agenticEfforts";

/**
 * One reasoning-effort selector of an agentic tab — the default selector of
 * `specifications/ui/AII-ai-integrations.md` AII-FR-24, or one row's selector in
 * the per-task section (AII-FR-QDLW).
 *
 * It offers `options`, the levels `../core/AIC-agentic-integrations.md`
 * AIC-FR-SFMS offers for its effective model, after one leading entry: the
 * backend default for the default selector, "same as default" for a row. A
 * stored level the list does not hold renders as that leading entry
 * (AII-FR-BMGM).
 */
export function AgenticEffortSelect({
  id,
  ariaLabel,
  value,
  options,
  leadingLabel,
  onChange,
}: {
  id: string;
  /** For a row, whose visible label is the row's task name (AII-FR-QDLW). */
  ariaLabel?: string;
  value: string | null | undefined;
  options: EffortOption[];
  leadingLabel: string;
  onChange: (effortId: string | null) => void;
}) {
  return (
    <select
      id={id}
      className="select"
      aria-label={ariaLabel}
      value={shownEffort(value, options)}
      onChange={(e) => onChange(e.target.value || null)}
    >
      <option value="">{leadingLabel}</option>
      {options.map((eff) => (
        <option key={eff.id} value={eff.id}>
          {eff.label}
        </option>
      ))}
    </select>
  );
}

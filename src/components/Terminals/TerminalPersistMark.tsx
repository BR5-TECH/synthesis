/**
 * TRM-FR-QFYS, TRM-FR-PHTL: the persistent marker of a terminal that survives
 * an IDE restart. Its text is the tooltip and the accessible name, so the
 * marker never depends on its colour alone.
 */
import { Icon } from "../icons";

export const PERSISTENT_TEXT = "Survives IDE restart";

export function TerminalPersistMark({ className, size = 10 }: { className: string; size?: number }) {
  return (
    <span className={className} role="img" aria-label={PERSISTENT_TEXT} title={PERSISTENT_TEXT}>
      <Icon.Persistent size={size} aria-hidden="true" />
    </span>
  );
}

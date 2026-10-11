import { TO_BE_IMPLEMENTED_MESSAGE } from "./agenticImplemented";

/**
 * The statement at the top of a settings tab whose vendor is not implemented
 * yet (AII-FR-OTQY, SET-FR-GVED). The tab's own controls follow it, grayed and
 * disabled.
 */
export function ToBeImplementedNotice() {
  return (
    <div
      className="to-be-implemented"
      role="note"
      data-testid="to-be-implemented"
    >
      <div className="to-be-implemented__title">{TO_BE_IMPLEMENTED_MESSAGE}</div>
      <div className="to-be-implemented__detail">
        Only Claude Code is tested. This integration cannot be configured yet.
      </div>
    </div>
  );
}

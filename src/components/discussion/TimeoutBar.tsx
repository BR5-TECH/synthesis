/**
 * The timeout bar under a reply composer's field
 * (`CVP-conversation-presentation.md` CVP-FR-JSHF, CVP-FR-IJEY, CVP-FR-XZTZ,
 * CVP-FR-YBXQ, CVP-FR-UYHO).
 *
 * It draws the share of the whole-turn deadline that is left, on the local
 * clock, once each half second. It calls no operation, it is not a live
 * region, and it takes no pointer input and no focus.
 */
import { useEffect, useState } from "react";

import { percentLeft, type TurnDeadline } from "../../state/turnDeadline";

/** CVP-FR-XZTZ: at least once each second. */
export const TIMEOUT_BAR_TICK_MS = 500;

export function TimeoutBar({ deadline }: { deadline: TurnDeadline }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    // CVP-FR-XZTZ: a new deadline is drawn at once, not at the next tick.
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), TIMEOUT_BAR_TICK_MS);
    return () => clearInterval(timer);
  }, [deadline.startedMs, deadline.endsMs]);
  const left = percentLeft(deadline, now);
  return (
    <div
      className="timeout-bar"
      role="progressbar"
      aria-label="Time left for the agent turn"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(left)}
      data-testid="timeout-bar"
    >
      <div className="timeout-bar__fill" style={{ width: `${left}%` }} />
    </div>
  );
}

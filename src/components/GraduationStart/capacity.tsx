/**
 * What the start dialog knows about the project-wide limit of graduation runs
 * (`../../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-LHQY,
 * GSD-FR-KDBU, GSD-FR-NOID).
 *
 * The hook reads `get_graduation_capacity` when the dialog opens and again on
 * every "graduation queue changed" event. A read that fails, or that returns
 * nothing, makes the capacity unread. The dialog then treats the limit as not
 * full. The notes below state each fact in words.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../../api";
import { onGraduationQueueChanged } from "../../events";
import { logError, logWarn } from "../../logging";
import { graduationErrorMessage, splitTyped } from "../../state/graduation";
import type { GraduationCapacity } from "../../types";

/** The state of the dialog's knowledge of the capacity. */
export type CapacityRead =
  | { status: "pending" }
  | { status: "read"; capacity: GraduationCapacity }
  | { status: "unread"; reason: string | null };

/**
 * GSD-FR-LHQY: read the capacity now and on every queue change while mounted.
 * A late answer of an older read never replaces a newer one.
 */
export function useGraduationCapacity(draftId: string): CapacityRead {
  const [read, setRead] = useState<CapacityRead>({ status: "pending" });
  const sequence = useRef(0);
  const live = useRef(true);

  const readCapacity = useCallback(async () => {
    const mine = ++sequence.current;
    try {
      const found = await api.getGraduationCapacity();
      if (!live.current || mine !== sequence.current) return;
      if (!found) {
        // GSD-FR-NOID: an empty answer is a capacity that is not read.
        logWarn(["frontend"], "the graduation capacity read returned nothing", {
          draftId,
        });
        setRead({ status: "unread", reason: null });
        return;
      }
      setRead({ status: "read", capacity: found });
    } catch (reason) {
      if (!live.current || mine !== sequence.current) return;
      // The typed code alone: a refusal's detail can carry project paths.
      const [code] = splitTyped(String(reason));
      logError(["frontend"], "the graduation capacity could not be read", {
        draftId,
        code,
      });
      setRead({ status: "unread", reason: graduationErrorMessage(String(reason)) });
    }
  }, [draftId]);

  useEffect(() => {
    live.current = true;
    void readCapacity();
    let unlisten: (() => void) | null = null;
    void onGraduationQueueChanged(() => void readCapacity())
      .then((fn) => {
        if (live.current) unlisten = fn;
        else fn();
      })
      .catch(() => {
        // Without the event the dialog keeps the read it made when it opened.
        logWarn(["frontend"], "the graduation queue events could not be followed", {
          draftId,
        });
      });
    return () => {
      live.current = false;
      unlisten?.();
    };
  }, [readCapacity, draftId]);

  return read;
}

/** The limit as words, for example "2 graduation runs". */
function limitWords(limit: number): string {
  return `${limit} graduation ${limit === 1 ? "run" : "runs"}`;
}

/** GSD-FR-NOID: the capacity could not be read. */
export function CapacityUnread({ reason }: { reason: string | null }) {
  return (
    <p
      className="graduation-start__notice"
      data-testid="graduation-start-capacity-unread"
    >
      The project's limit of graduation runs could not be read, so this window
      does not know if a project slot is free.
      {reason ? ` ${reason}` : ""}
    </p>
  );
}

export interface SlotWaitProps {
  /** The numeric limit that is full. */
  limit: number;
  /** What the run goes to: a stream the author chose, or a stream made here. */
  target: { kind: "stream"; name: string; occupied: boolean } | { kind: "new" };
}

/** GSD-FR-LHQY / GSD-FR-KDBU: the run waits for a project slot. */
export function SlotWait({ limit, target }: SlotWaitProps) {
  const held = `the project already works its limit of ${limitWords(limit)}`;
  let text: string;
  if (target.kind === "new") {
    text = `The new stream is free, but ${held}. This run waits for a project slot.`;
  } else if (target.occupied) {
    text = `Also, ${held}. This run waits for a project slot too.`;
  } else {
    text = `“${target.name}” is free, but ${held}. This one waits for a project slot.`;
  }
  return (
    <p className="t-ui-xs graduation-start__note" data-testid="graduation-start-slot-wait">
      {text}
    </p>
  );
}

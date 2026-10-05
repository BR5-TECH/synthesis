/**
 * What a run does with work standing in its stream
 * (`../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-TBQX,
 * GSD-FR-NWSC, GSD-FR-MZTB).
 *
 * One ladder, one selection, in the same card the escalation's answers use:
 * the three positions are alternatives to each other rather than a list of
 * settings. Under the two that commit stands the optional message that commit
 * takes. It renders no path set and reads no working copy — what stands in the
 * stream when the run starts is not what stands there while this is answered.
 *
 * It is asked only where the run will wait for its stream (GSD-FR-WQPD); the
 * surface that asks decides that, because it is the surface that knows which
 * stream is chosen.
 */

import { useEffect, useRef } from "react";

import {
  STANDING_WORK_POSITIONS,
  standingWorkCommits,
} from "../state/graduation";
import type { StandingWork } from "../types";

export interface StandingWorkChoiceProps {
  /** Namespaces the radio group, so two of these on one surface stay apart. */
  name: string;
  value: StandingWork;
  disabled?: boolean;
  onChange: (value: StandingWork) => void;
  /** GSD-FR-MZTB: what the commit says. Empty takes `messageDefault`. */
  message: string;
  onMessage: (message: string) => void;
  /** The run's own name, which an empty message commits under. */
  messageDefault: string;
}

export function StandingWorkChoice({
  name,
  value,
  disabled,
  onChange,
  message,
  onMessage,
  messageDefault,
}: StandingWorkChoiceProps) {
  // GSD-FR-MZTB: the message belongs to the commit, so a choice that commits
  // nothing is not asked for one.
  const commits = standingWorkCommits(value);
  const field = `standing-work-message-${name}`;
  const messageRef = useRef<HTMLDivElement>(null);
  // A field that appears below the fold of a body that scrolls is one only the
  // scrollbar reports. It is brought into view when the author's own choice
  // makes it appear, and not when it is simply there at the first render —
  // which would scroll the question above it out of sight.
  const settled = useRef(false);
  useEffect(() => {
    if (commits && settled.current) {
      messageRef.current?.scrollIntoView?.({ block: "nearest" });
    }
    settled.current = true;
  }, [commits]);
  return (
    <fieldset className="graduation__answer" data-testid="standing-work-choice">
      <legend className="graduation__ask">
        Work standing in the stream when this run starts
      </legend>
      {STANDING_WORK_POSITIONS.map((position) => (
        <label
          className="graduation__choice"
          key={position.value}
          data-selected={value === position.value ? "true" : undefined}
        >
          <input
            type="radio"
            name={`standing-work-${name}`}
            value={position.value}
            checked={value === position.value}
            disabled={disabled}
            onChange={() => onChange(position.value)}
          />
          <span className="graduation__option">
            <span className="graduation__option-summary">{position.summary}</span>
            <span className="graduation__option-description">
              {position.description}
            </span>
          </span>
        </label>
      ))}

      {commits && (
        <div
          className="picker-field"
          ref={messageRef}
          data-testid="standing-work-message"
        >
          <label className="picker-field__label" htmlFor={field}>
            Commit message (optional)
          </label>
          <input
            id={field}
            className="input standing-work__message"
            type="text"
            value={message}
            disabled={disabled}
            // The run's own name, which is what an empty field commits under,
            // so what "optional" means is legible without a sentence.
            placeholder={messageDefault}
            onChange={(event) => onMessage(event.target.value)}
          />
        </div>
      )}
    </fieldset>
  );
}

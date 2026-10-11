/**
 * Where a graduation run works
 * (`../../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-BZHW).
 *
 * One choice of four: an existing stream, a stream this dialog creates, a
 * temporary stream this dialog creates, and the worktree that is active when
 * the author confirms. Each choice shows only
 * its own fields.
 *
 * The four are one segmented toggle on one line, with four segments of equal
 * width. It is a radio group with a roving tab stop: the Tab key reaches the
 * chosen segment, and the arrow keys, Home and End move to another segment that
 * is not disabled and choose it.
 */
import { useRef } from "react";
import type { KeyboardEvent } from "react";

export type Destination = "stream" | "new" | "temporary" | "direct";

/**
 * How a segment was chosen. A click (also Enter or Space) is a decision, so the
 * dialog can take focus on to the fields of the choice. The arrow keys, Home
 * and End only walk the toggle, so focus stays on it (GSD-FR-WMTD).
 */
export type DestinationInput = "click" | "arrows";

export interface DestinationChoiceProps {
  value: Destination;
  disabled: boolean;
  /** GSD-FR-LDGM / GSD-FR-CXVA: no stream to choose from. */
  noStream: boolean;
  onChange: (value: Destination, via: DestinationInput) => void;
}

const CHOICES: Array<{ value: Destination; label: string }> = [
  { value: "stream", label: "Existing stream" },
  { value: "new", label: "New stream" },
  { value: "temporary", label: "Temporary stream" },
  { value: "direct", label: "Work directly" },
];

export function DestinationChoice({
  value,
  disabled,
  noStream,
  onChange,
}: DestinationChoiceProps) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);

  // GSD-FR-LDGM: a project with no stream has none to choose.
  const isDisabled = (choice: Destination) =>
    disabled || (choice === "stream" && noStream);

  // The Tab key reaches the chosen segment. Where that one is disabled, it
  // reaches the first segment that is not, so the group is never unreachable.
  const chosen = CHOICES.findIndex((choice) => choice.value === value);
  const tabStop =
    chosen >= 0 && !isDisabled(value)
      ? chosen
      : CHOICES.findIndex((choice) => !isDisabled(choice.value));

  // Choose the segment `step` places from `from`, past every disabled one.
  const move = (from: number, step: 1 | -1) => {
    for (let i = 1; i <= CHOICES.length; i++) {
      const next = (from + step * i + CHOICES.length * i) % CHOICES.length;
      if (isDisabled(CHOICES[next].value)) continue;
      if (CHOICES[next].value !== value) onChange(CHOICES[next].value, "arrows");
      refs.current[next]?.focus();
      return;
    }
  };

  const onKeyDown = (event: KeyboardEvent, index: number) => {
    switch (event.key) {
      case "ArrowRight":
      case "ArrowDown":
        event.preventDefault();
        move(index, 1);
        break;
      case "ArrowLeft":
      case "ArrowUp":
        event.preventDefault();
        move(index, -1);
        break;
      case "Home":
        event.preventDefault();
        move(CHOICES.length - 1, 1);
        break;
      case "End":
        event.preventDefault();
        move(0, -1);
        break;
    }
  };

  return (
    <fieldset className="graduation-start__destination" data-testid="graduation-destination">
      <legend className="picker-field__label">Where the run works</legend>
      <div
        className="graduation-start__destination-row"
        role="radiogroup"
        aria-label="Where the run works"
      >
        {CHOICES.map((choice, index) => {
          const active = value === choice.value;
          return (
            <button
              key={choice.value}
              ref={(element) => {
                refs.current[index] = element;
              }}
              type="button"
              role="radio"
              value={choice.value}
              className="graduation-start__destination-option"
              aria-checked={active}
              data-active={active ? "true" : "false"}
              tabIndex={index === tabStop ? 0 : -1}
              disabled={isDisabled(choice.value)}
              title={choice.label}
              onClick={() => {
                if (!active) onChange(choice.value, "click");
              }}
              onKeyDown={(event) => onKeyDown(event, index)}
            >
              {choice.label}
            </button>
          );
        })}
      </div>
    </fieldset>
  );
}

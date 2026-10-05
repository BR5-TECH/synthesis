/**
 * The Rich and Source switch of a Markdown Document tab (DTV-FR-VZNE,
 * DTV-FR-HQTN).
 *
 * The switch is a radio group with a roving tab stop. The Tab key reaches the
 * active choice, and the arrow keys move to the other choice and select it.
 */
import { useRef } from "react";
import type { KeyboardEvent } from "react";
import type { DocumentViewMode } from "../../state/documentViewState";

const CHOICES: { mode: DocumentViewMode; label: string }[] = [
  { mode: "rich", label: "Rich" },
  { mode: "source", label: "Source" },
];

export function ViewSwitch({
  mode,
  onChange,
}: {
  mode: DocumentViewMode;
  onChange: (mode: DocumentViewMode) => void;
}) {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);

  const move = (to: number) => {
    const next = (to + CHOICES.length) % CHOICES.length;
    onChange(CHOICES[next].mode);
    refs.current[next]?.focus();
  };

  const onKeyDown = (event: KeyboardEvent, index: number) => {
    switch (event.key) {
      case "ArrowRight":
      case "ArrowDown":
        event.preventDefault();
        move(index + 1);
        break;
      case "ArrowLeft":
      case "ArrowUp":
        event.preventDefault();
        move(index - 1);
        break;
      case "Home":
        event.preventDefault();
        move(0);
        break;
      case "End":
        event.preventDefault();
        move(CHOICES.length - 1);
        break;
    }
  };

  return (
    <div className="document-viewer__switch" role="radiogroup" aria-label="View">
      {CHOICES.map((choice, index) => {
        const active = choice.mode === mode;
        return (
          <button
            key={choice.mode}
            ref={(element) => {
              refs.current[index] = element;
            }}
            type="button"
            role="radio"
            aria-checked={active}
            tabIndex={active ? 0 : -1}
            className="document-viewer__choice"
            data-active={active ? "true" : "false"}
            onClick={() => onChange(choice.mode)}
            onKeyDown={(event) => onKeyDown(event, index)}
          >
            {choice.label}
          </button>
        );
      })}
    </div>
  );
}

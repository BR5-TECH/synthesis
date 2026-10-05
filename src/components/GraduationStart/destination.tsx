/**
 * Where a graduation run works
 * (`../../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-BZHW).
 *
 * One choice of three: an existing stream, a stream this dialog creates, and
 * the worktree that is active when the author confirms. Each choice shows only
 * its own fields.
 */

export type Destination = "stream" | "new" | "direct";

export interface DestinationChoiceProps {
  value: Destination;
  disabled: boolean;
  /** GSD-FR-LDGM / GSD-FR-CXVA: no stream to choose from. */
  noStream: boolean;
  onChange: (value: Destination) => void;
}

const CHOICES: Array<{ value: Destination; label: string }> = [
  { value: "stream", label: "Existing stream" },
  { value: "new", label: "New stream" },
  { value: "direct", label: "Work directly" },
];

export function DestinationChoice({
  value,
  disabled,
  noStream,
  onChange,
}: DestinationChoiceProps) {
  return (
    <fieldset className="graduation-start__destination" data-testid="graduation-destination">
      <legend className="picker-field__label">Where the run works</legend>
      <div className="graduation-start__destination-row">
        {CHOICES.map((choice) => (
          <label
            key={choice.value}
            className="graduation__choice"
            data-selected={value === choice.value}
          >
            <input
              type="radio"
              name="graduation-destination"
              value={choice.value}
              checked={value === choice.value}
              // GSD-FR-LDGM: a project with no stream has none to choose.
              disabled={disabled || (choice.value === "stream" && noStream)}
              onChange={() => onChange(choice.value)}
            />
            <span>{choice.label}</span>
          </label>
        ))}
      </div>
    </fieldset>
  );
}

/**
 * The fields of **New stream** inside the start dialog
 * (`../../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-QGTC).
 *
 * A name and the branch the stream is created from, on the terms of
 * `WSS-work-stream-selector.md` WSS-FR-XZRO, without a second overlay.
 */

export interface NewStreamFieldsProps {
  name: string;
  onName: (name: string) => void;
  branch: string;
  onBranch: (branch: string) => void;
  branches: string[];
  nameError: string | null;
  disabled: boolean;
  nameRef: React.RefObject<HTMLInputElement | null>;
}

export function NewStreamFields({
  name,
  onName,
  branch,
  onBranch,
  branches,
  nameError,
  disabled,
  nameRef,
}: NewStreamFieldsProps) {
  return (
    <div data-testid="graduation-new-stream">
      <label className="picker-field">
        <span className="picker-field__label">Name</span>
        <input
          className="input"
          ref={nameRef}
          value={name}
          disabled={disabled}
          aria-invalid={nameError ? true : undefined}
          aria-describedby={nameError ? "graduation-new-stream-error" : undefined}
          onChange={(event) => onName(event.target.value)}
        />
      </label>
      {nameError && (
        <p
          className="stream-select__error"
          id="graduation-new-stream-error"
          role="alert"
          data-testid="graduation-new-stream-error"
        >
          {nameError}
        </p>
      )}
      <label className="picker-field">
        <span className="picker-field__label">Created from</span>
        <select
          className="select"
          value={branch}
          disabled={disabled}
          onChange={(event) => onBranch(event.target.value)}
        >
          {branches.map((entry) => (
            <option key={entry} value={entry}>
              {entry}
            </option>
          ))}
        </select>
      </label>
      <p className="t-ui-xs graduation-start__note">
        The stream is free, so the run starts at once.
      </p>
    </div>
  );
}

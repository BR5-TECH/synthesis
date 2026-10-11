/**
 * A single-line path field with its native picker button inside the frame, at
 * the trailing end (SET-FR-HYTR).
 *
 * The button is a control of the field and not a neighbour of it: it sits in
 * the same frame, follows the field in the tab order, and carries its own
 * accessible name. The frame is the one the image-name field uses
 * (`joined-field`), so the Docker section reads as one set of fields.
 */
import { Icon } from "./icons";

export interface PathFieldProps {
  id: string;
  /** The accessible name of the text field. */
  label: string;
  value: string;
  placeholder?: string;
  /** The id of a `<datalist>` that suggests values while the author types. */
  list?: string;
  /** Which picker the button opens. */
  kind: "file" | "folder";
  /** The accessible name of the picker button. */
  pickLabel: string;
  onChange: (value: string) => void;
  onBlur?: () => void;
  onPick: () => void;
}

export function PathField({
  id,
  label,
  value,
  placeholder,
  list,
  kind,
  pickLabel,
  onChange,
  onBlur,
  onPick,
}: PathFieldProps) {
  return (
    <div className="joined-field">
      <input
        id={id}
        type="text"
        aria-label={label}
        className="joined-field__part"
        list={list}
        spellCheck={false}
        autoComplete="off"
        value={value}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        onBlur={onBlur}
      />
      <button
        type="button"
        className="joined-field__action"
        aria-label={pickLabel}
        title={pickLabel}
        onClick={onPick}
      >
        {kind === "file" ? <Icon.File size={14} /> : <Icon.FolderOpen size={14} />}
      </button>
    </div>
  );
}

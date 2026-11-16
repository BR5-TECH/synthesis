/**
 * One status selector of the GitHub Project settings section
 * (`SET-project-settings.md` SET-FR-UGSP, SET-FR-JGAK, SET-FR-JNLK,
 * SET-FR-QZYV).
 *
 * It lists the options of the selected Project's `Status` field by name. A
 * stored name that the list does not hold stays selected and is marked
 * unavailable, so nothing is replaced behind the author's back. The options
 * that the other selectors hold are disabled, so the three statuses always
 * differ.
 */
import type { GithubProjectStatusOption } from "../types";

/** The select's value while no status name is stored. */
const UNSET = "";

export function StatusSelector({
  id,
  value,
  others,
  optional = false,
  options,
  loading,
  disabled,
  onChange,
}: {
  id: string;
  /** The stored status name, or `null` where none is stored. */
  value: string | null;
  /** The names the other selectors hold, each with that selector's name. */
  others: ReadonlyArray<{ name: string | null; label: string }>;
  /** GPP-FR-BLVX: an unset optional status is not an error. */
  optional?: boolean;
  /** The options of the `Status` field; `null` while unread or unreadable. */
  options: GithubProjectStatusOption[] | null;
  /** The status read is outstanding. */
  loading: boolean;
  disabled: boolean;
  onChange: (name: string) => void;
}) {
  const loaded = options !== null;
  // SET-FR-JNLK: a stored name the list does not hold renders as selected.
  const unavailable =
    loaded && value !== null && !options.some((o) => o.name === value);
  return (
    <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
      <select
        id={id}
        className="input input--sm"
        style={{ justifySelf: "start" }}
        value={value ?? UNSET}
        // SET-FR-JGAK: no choice while the options are unread.
        disabled={disabled || !loaded}
        aria-invalid={(value === null && !optional) || unavailable ? true : undefined}
        onChange={(e) => {
          if (e.target.value !== UNSET) onChange(e.target.value);
        }}
      >
        {value === null && (
          <option value={UNSET} disabled>
            Choose an option
          </option>
        )}
        {value !== null && (!loaded || unavailable) && (
          <option value={value}>
            {unavailable ? `${value} (unavailable)` : value}
          </option>
        )}
        {(options ?? []).map((o) => {
          // SET-FR-QZYV: an option another selector holds cannot be chosen here.
          const holder = others.find((h) => h.name === o.name);
          return (
            <option key={o.id} value={o.name} disabled={holder !== undefined}>
              {holder ? `${o.name} (the ${holder.label})` : o.name}
            </option>
          );
        })}
      </select>
      {loading && (
        <span className="t-ui-xs t-muted" data-testid={`${id}-loading`}>
          Loading statuses…
        </span>
      )}
      {unavailable && (
        <span className="picker-error t-ui-xs" data-testid={`${id}-unavailable`}>
          This status no longer exists in the Project. Choose another option.
        </span>
      )}
    </div>
  );
}

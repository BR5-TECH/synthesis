// ---------------------------------------------------------------------------
// Reminder instants
// ---------------------------------------------------------------------------

/** Two digits, for the `datetime-local` value format. */
function pad(n: number): string {
  return String(n).padStart(2, "0");
}

/**
 * An RFC 3339 instant as the local wall-clock string a `datetime-local` input
 * takes, or the empty string when there is no reminder to seed it with.
 */
export function toLocalInput(iso: string | undefined): string {
  if (!iso) return "";
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return "";
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}`;
}

/** The inverse: a picked local wall-clock time as the RFC 3339 instant stored. */
export function fromLocalInput(value: string): string | null {
  if (!value) return null;
  const d = new Date(value);
  return Number.isNaN(d.getTime()) ? null : d.toISOString();
}

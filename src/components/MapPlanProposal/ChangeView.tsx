/**
 * MPV-FR-CCDM / MPV-FR-LYKS: one changed field of a `Change issue` row, as the
 * value before and the value after. Text is drawn as text: no value is read as
 * markup (the card draws no raw HTML).
 */
import type { RowChange } from "../../types";
import { closeReasonWord, FIELD_WORD, plainValue, TEXT_FIELDS } from "./words";

function LabelChange({ change }: { change: RowChange }) {
  const added = Array.isArray(change.added) ? change.added : [];
  const removed = Array.isArray(change.removed) ? change.removed : [];
  return (
    <span className="mpv-change__labels">
      {added.map((name) => (
        <span key={`+${name}`} className="mpv-label mpv-label--added" data-testid="mpv-label-added">
          <span aria-hidden="true">+ </span>
          <span className="mpv-sr-only">Add label </span>
          {name}
        </span>
      ))}
      {removed.map((name) => (
        <span key={`-${name}`} className="mpv-label mpv-label--removed" data-testid="mpv-label-removed">
          <span aria-hidden="true">− </span>
          <span className="mpv-sr-only">Remove label </span>
          {name}
        </span>
      ))}
    </span>
  );
}

export function ChangeView({ change }: { change: RowChange }) {
  const name = FIELD_WORD[change.field] ?? change.field;
  let value;
  if (change.field === "close") {
    value = <strong className="mpv-change__close">{closeReasonWord(change)}</strong>;
  } else if (change.field === "labels") {
    value = <LabelChange change={change} />;
  } else if (TEXT_FIELDS.has(change.field)) {
    value = (
      <span className="mpv-change__text">
        <del className="mpv-del" data-testid="mpv-before">
          {plainValue(change.before, "empty")}
        </del>{" "}
        <ins className="mpv-ins" data-testid="mpv-after">
          {plainValue(change.after, "empty")}
        </ins>
      </span>
    );
  } else {
    value = (
      <span className="mpv-change__pair">
        <span className="mpv-del" data-testid="mpv-before">
          {plainValue(change.before)}
        </span>
        <span aria-hidden="true"> → </span>
        <span className="mpv-sr-only"> to </span>
        <span className="mpv-ins" data-testid="mpv-after">
          {plainValue(change.after)}
        </span>
      </span>
    );
  }
  return (
    <div className="mpv-change" data-field={change.field}>
      <dt className="mpv-change__field">{name}</dt>
      <dd className="mpv-change__value">{value}</dd>
    </div>
  );
}

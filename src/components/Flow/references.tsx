import { ARTIFACT_TYPES, typeChip } from "../../artifactTypes";
import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import { optionLabel, referenceLabel, type Reference } from "./geometry";

export interface ReferenceRowsProps {
  references: Reference[];
  /** FLO-FR-10: a reference is removed on its own, from its row. */
  onRemove?: (artifactId: string) => void;
  onOpen: (artifact: ArtifactOption) => void;
  /** FLO-FR-KHBY: while a sync runs, no reference is removed. */
  disabled?: boolean;
  className?: string;
}

/**
 * FLO-FR-11 / FLO-FR-12 / FLO-FR-YDWT: one row per stored reference. A
 * resolved reference shows the artifact's display name and type chip and opens
 * the artifact. One that does not resolve names the stored id and opens
 * nothing. Nothing is removed for the author.
 */
export function ReferenceRows({
  references,
  onRemove,
  onOpen,
  disabled = false,
  className,
}: ReferenceRowsProps) {
  if (references.length === 0) return null;
  return (
    <div className={className ?? "flow-refs"}>
      {references.map((ref) => (
        <span className="flow-ref-row" key={ref.id}>
          {ref.state === "resolved" ? (
            <button
              type="button"
              className="flow-ref"
              aria-label={`Open ${ref.artifact.displayName}`}
              onClick={() => onOpen(ref.artifact)}
            >
              <span className="chip-type" data-type={ref.artifact.artifactType}>
                {typeChip(ref.artifact.artifactType)}
              </span>
              <span className="flow-ref__name">{ref.artifact.displayName}</span>
              <span className="flow-ref__go" aria-hidden="true">
                ↗
              </span>
            </button>
          ) : ref.state === "unresolved" ? (
            <span className="flow-ref flow-ref--unresolved" data-testid="flow-ref-unresolved">
              <span aria-hidden="true">⚠</span> unresolved: {ref.id}
            </span>
          ) : (
            <span className="flow-ref">{ref.id}</span>
          )}
          {onRemove && (
            <button
              type="button"
              className="flow-ref-row__remove"
              aria-label={`Remove reference ${referenceLabel(ref)}`}
              disabled={disabled}
              onClick={() => onRemove(ref.id)}
            >
              ×
            </button>
          )}
        </span>
      ))}
    </div>
  );
}

/** FLO-FR-10: the types the picker offers, in the order of the type list. */
const PICKER_GROUPS = ARTIFACT_TYPES.filter((t) => t.value !== "flow");

/**
 * FLO-FR-10: the picker of project artifacts. It adds and keeps no value of its
 * own, and it never offers an artifact that is already referenced.
 */
export function ArtifactPicker({
  artifacts,
  referenced,
  onAdd,
  label = "Add reference",
  prompt = "+ Reference",
  disabled = false,
  className,
}: {
  artifacts: ArtifactOption[];
  referenced: ReadonlySet<string>;
  onAdd: (artifactId: string) => void;
  label?: string;
  prompt?: string;
  /** FLO-FR-KHBY: while a sync runs, the picker adds nothing. */
  disabled?: boolean;
  className?: string;
}) {
  return (
    <select
      className={className ?? "flow-picker"}
      aria-label={label}
      value=""
      disabled={disabled}
      onChange={(e) => {
        if (!disabled && e.target.value) onAdd(e.target.value);
      }}
    >
      <option value="">{prompt}</option>
      {PICKER_GROUPS.map(({ value, label: groupLabel }) => {
        const group = artifacts.filter(
          (a) => a.artifactType === value && !referenced.has(a.id),
        );
        if (group.length === 0) return null;
        return (
          <optgroup key={value} label={groupLabel}>
            {group.map((a) => (
              <option key={a.id} value={a.id}>
                {optionLabel(a)}
              </option>
            ))}
          </optgroup>
        );
      })}
    </select>
  );
}

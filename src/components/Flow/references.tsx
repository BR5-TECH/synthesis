import { typeChip } from "../../artifactTypes";
import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import { Icon } from "../icons";
import {
  REFERENCE_GROUPS,
  optionLabel,
  referenceLabel,
  type Reference,
} from "./geometry";

export interface ElementReferencesProps {
  references: Reference[];
  /** FLO-FR-10: the remove affordances appear only while the element is selected. */
  selected: boolean;
  onRemoveArtifact: (artifactId: string) => void;
  onOpenArtifact: (artifact: ArtifactOption) => void;
}

/**
 * FLO-FR-11: one row per reference, in the order the element carries them —
 * a loop's on exactly a node's terms (FLO-FR-37).
 *
 * A resolved reference renders as the artifact's display name with its type chip
 * and a click-through; one that does not resolve names the stored id instead, and
 * nothing in the graph is removed on the user's behalf.
 */
export function ElementReferences({
  references,
  selected,
  onRemoveArtifact,
  onOpenArtifact,
}: ElementReferencesProps) {
  return (
    <>
      {references.map((ref) => (
        <div className="flow-node__ref-row" key={ref.id}>
          {ref.state === "resolved" ? (
            <button
              className="flow-node__ref"
              aria-label={`Open ${ref.artifact.displayName}`}
              onClick={() => onOpenArtifact(ref.artifact)}
            >
              <span className="chip-type" data-type={ref.artifact.artifactType}>
                {typeChip(ref.artifact.artifactType)}
              </span>
              <span className="flow-node__ref-name">
                {ref.artifact.displayName}
              </span>
              <span aria-hidden="true">↗</span>
            </button>
          ) : ref.state === "unresolved" ? (
            <div className="flow-node__ref flow-node__ref--unresolved">
              ⚠ unresolved: {ref.id}
            </div>
          ) : (
            <div className="flow-node__ref">{ref.id}</div>
          )}
          {selected && (
            <button
              className="btn btn--ghost btn--icon btn--sm"
              aria-label={`Remove reference ${referenceLabel(ref)}`}
              onClick={() => onRemoveArtifact(ref.id)}
            >
              <Icon.X size={11} />
            </button>
          )}
        </div>
      ))}
    </>
  );
}

/**
 * FLO-FR-10: the picker, listing the artifacts an element can be built out of.
 *
 * It adds rather than replaces — an element may reference any number of
 * artifacts — so it holds no value of its own and returns to its prompt after
 * each choice. An artifact the element already references is not offered again;
 * a reference is dropped from its own row, never by changing a select, so an
 * unresolved one cannot be cleared by accident.
 */
export function ArtifactPicker({
  artifacts,
  referenced,
  onAdd,
}: {
  artifacts: ArtifactOption[];
  referenced: Set<string>;
  onAdd: (artifactId: string) => void;
}) {
  return (
    <select
      className="select select--sm flow-node__picker"
      aria-label="Add artifact"
      value=""
      onChange={(e) => {
        if (e.target.value) onAdd(e.target.value);
      }}
    >
      <option value="">Add artifact…</option>
      {REFERENCE_GROUPS.map(({ type, label }) => {
        const group = artifacts.filter(
          (a) => a.artifactType === type && !referenced.has(a.id),
        );
        if (group.length === 0) return null;
        return (
          <optgroup key={type} label={label}>
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

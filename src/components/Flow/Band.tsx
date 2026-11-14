import type { ReactNode } from "react";

import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import type { FlowDocument } from "../../state/flowDocument";
import type { Reference } from "./geometry";
import { ArtifactPicker, ReferenceRows } from "./references";

export interface BandProps {
  doc: FlowDocument | null;
  references: Reference[];
  artifacts: ArtifactOption[];
  /** MDL-FR-WMGU: the host keeps every edit affordance inert. */
  inert: boolean;
  /** MDL-FR-KKYH: the name and the description of a GitHub plan are read-only. */
  readOnlyMeta: boolean;
  /** What the host adds at the trailing end of the band. */
  extras?: ReactNode;
  onName: (name: string) => void;
  onDescription: (text: string) => void;
  onAddReference: (artifactId: string) => void;
  onRemoveReference: (artifactId: string) => void;
  onOpenReference: (artifact: ArtifactOption) => void;
}

/**
 * The band above the canvas: the Flow's name and description (FLO-FR-25) and
 * its references (FLO-FR-YDWT). A host adds what it owns at the trailing end
 * (MDL-FR-KKYH).
 */
export function Band(props: BandProps) {
  const { doc, inert, readOnlyMeta } = props;
  const editable = !!doc && !inert;
  const nameMissing = !!doc && !readOnlyMeta && doc.name.trim() === "";
  const name = doc?.name ?? "";
  const description = doc?.description ?? "";
  return (
    <div className="flow-band" data-testid="flow-band">
      <input
        className="flow-band__name"
        aria-label="Flow name"
        placeholder="Flow name"
        aria-required={!readOnlyMeta}
        aria-invalid={nameMissing}
        aria-describedby={nameMissing ? "flow-name-required" : undefined}
        data-missing={nameMissing}
        size={Math.min(Math.max(name.length + 1, 10), 40)}
        value={name}
        readOnly={readOnlyMeta}
        disabled={!editable}
        onChange={(e) => props.onName(e.target.value)}
      />
      {nameMissing && (
        <span className="flow-band__required" id="flow-name-required" data-testid="flow-name-required">
          Name required
        </span>
      )}
      <input
        className="flow-band__description"
        aria-label="Flow description"
        placeholder="Description"
        size={Math.min(Math.max(description.length + 1, 12), 60)}
        value={description}
        readOnly={readOnlyMeta}
        disabled={!editable}
        onChange={(e) => props.onDescription(e.target.value)}
      />
      <div className="flow-band__refs" data-testid="flow-references">
        <ReferenceRows
          references={props.references}
          onOpen={props.onOpenReference}
          onRemove={editable ? props.onRemoveReference : undefined}
          className="flow-refs flow-refs--band"
        />
        {doc && (
          <ArtifactPicker
            artifacts={props.artifacts}
            referenced={new Set(doc.references)}
            onAdd={props.onAddReference}
            label={doc.references.length === 0 ? "Add HLD" : "Add Flow reference"}
            prompt={doc.references.length === 0 ? "Add HLD" : "+"}
            disabled={!editable}
            className={`flow-picker flow-picker--band${doc.references.length === 0 ? " flow-picker--hld" : ""}`}
          />
        )}
      </div>
      <span className="flow-band__spacer" />
      {props.extras}
    </div>
  );
}

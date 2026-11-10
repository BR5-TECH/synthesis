import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import type { FlowDocument } from "../../state/flowDocument";
import type { FlowSyncState } from "../../state/flowSessions";
import { repoOf } from "./chips";
import type { Reference } from "./geometry";
import { ArtifactPicker, ReferenceRows } from "./references";
import { reviewCount } from "./useFlowSync";

export interface BandProps {
  doc: FlowDocument | null;
  references: Reference[];
  artifacts: ArtifactOption[];
  sync: FlowSyncState;
  onName: (name: string) => void;
  onDescription: (text: string) => void;
  onAddReference: (artifactId: string) => void;
  onRemoveReference: (artifactId: string) => void;
  onOpenReference: (artifact: ArtifactOption) => void;
  onSync: () => void;
  onOpenReview: () => void;
}

/**
 * The band above the canvas: the Flow's name and description (FLO-FR-25), its
 * references (FLO-FR-YDWT), and the Sync control at the trailing end
 * (FLO-FR-AKMV, FLO-FR-KHBY, FLO-FR-WLHA).
 */
export function Band(props: BandProps) {
  const { doc, sync } = props;
  const running = sync.running !== null;
  const editable = !!doc && !running;
  const nameMissing = !!doc && doc.name.trim() === "";
  const count = reviewCount(sync);
  const repo = doc?.sync?.links[0] ? repoOf(doc.sync.links[0]) : null;
  const name = doc?.name ?? "";
  const description = doc?.description ?? "";
  return (
    <div className="flow-band" data-testid="flow-band">
      <input
        className="flow-band__name"
        aria-label="Flow name"
        placeholder="Flow name"
        aria-required="true"
        aria-invalid={nameMissing}
        aria-describedby={nameMissing ? "flow-name-required" : undefined}
        data-missing={nameMissing}
        size={Math.min(Math.max(name.length + 1, 10), 40)}
        value={name}
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
      {sync.error && (
        <span className="flow-band__error" role="alert" data-testid="flow-sync-error">
          {sync.error}
        </span>
      )}
      {doc && (
        <div className="flow-sync" data-running={running}>
          <button
            type="button"
            className="flow-sync__btn"
            aria-busy={sync.running === "sync"}
            disabled={running}
            onClick={props.onSync}
          >
            <span className="flow-sync__icon" aria-hidden="true">
              ⟳
            </span>
            {sync.running === "sync"
              ? "Syncing…"
              : sync.running === "resolve"
                ? "Applying…"
                : doc.sync
                  ? "Sync"
                  : "Connect to GitHub"}
          </button>
          {count > 0 && (
            <button
              type="button"
              className="flow-sync__count"
              aria-label={`Review ${count} sync ${count === 1 ? "item" : "items"}`}
              onClick={props.onOpenReview}
            >
              {count}
            </button>
          )}
          {repo && <span className="flow-sync__repo">{repo}</span>}
        </div>
      )}
    </div>
  );
}

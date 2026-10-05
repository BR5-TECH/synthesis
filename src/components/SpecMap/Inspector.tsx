import type { MapRef } from "../../state/specMap/types";
import { Icon } from "../icons";
import { CompletenessBar } from "./CompletenessBar";
import type { InspectorModel, InspectorRow } from "./inspectorModel";

export interface InspectorActions {
  onClose: () => void;
  onSelect: (ref: MapRef) => void;
  onFocus: (id: string) => void;
  onZoomTo: (ref: MapRef) => void;
  onCreate: (parentId: string | null) => void;
  onEdit: (id: string) => void;
  onDelete: (id: string) => void;
  onNewDraft: (id: string) => void;
  onOpenSpec: (code: string) => void;
  onOpenDraft: (draftId: string, name: string) => void;
}

function Row({ row, onSelect }: { row: InspectorRow; onSelect: (ref: MapRef) => void }) {
  const content = (
    <>
      {row.badge && (
        <span className="smap-code" data-folder={row.badge.folder} data-danger={row.danger || undefined}>
          {row.badge.text}
        </span>
      )}
      <span className="smap-row__label">{row.label}</span>
      {row.rollup && <CompletenessBar rollup={row.rollup} variant="row" />}
      <span className="smap-row__count">{row.count}</span>
    </>
  );
  if (!row.ref) {
    return (
      <div className="smap-row" data-danger={row.danger || undefined}>
        {content}
      </div>
    );
  }
  const ref = row.ref;
  return (
    <button type="button" className="smap-row" onClick={() => onSelect(ref)}>
      {content}
    </button>
  );
}

/**
 * SMI-FR-QWOP .. SMI-FR-ZLOA: the reading pane of the selected node, and the
 * actions that start work from it.
 */
export function Inspector({
  model,
  overlay,
  actions,
}: {
  model: InspectorModel;
  overlay: boolean;
  actions: InspectorActions;
}) {
  return (
    <aside className="smap-inspector" data-overlay={overlay || undefined} aria-label="Inspector">
      <div className="smap-inspector__head">
        <span className="smap-inspector__title">Inspector</span>
        <span className="smap-inspector__kind">{model.kind === "empty" ? "" : model.kindName}</span>
        <button
          type="button"
          className="btn btn--ghost btn--icon btn--sm"
          aria-label="Hide inspector"
          title="Hide inspector"
          onClick={actions.onClose}
        >
          <Icon.X size={12} />
        </button>
      </div>
      <div className="smap-inspector__body">
        <InspectorBody model={model} actions={actions} />
      </div>
    </aside>
  );
}

function InspectorBody({ model, actions }: { model: InspectorModel; actions: InspectorActions }) {
  if (model.kind === "empty") {
    return (
      <>
        <p className="smap-inspector__hint">Select a node to inspect it.</p>
        <div className="smap-actions">
          <button type="button" className="btn btn--sm smap-action" onClick={() => actions.onCreate(null)}>
            <Icon.Plus size={12} /> {model.newRootLabel}
          </button>
        </div>
      </>
    );
  }

  if (model.kind === "planned") {
    return (
      <>
        <div className="smap-inspector__crumbs">Planned under {model.holderLabel}</div>
        <div className="smap-inspector__heading">
          <span className="smap-code smap-code--draft">draft</span>
          <span className="smap-inspector__name">{model.name}</span>
        </div>
        <div className="smap-actions">
          <button
            type="button"
            className="btn btn--primary btn--sm"
            onClick={() => actions.onOpenDraft(model.draftId, model.name)}
          >
            Open draft
          </button>
        </div>
      </>
    );
  }

  const r = model.rollup;
  return (
    <>
      <div className="smap-inspector__crumbs">{model.breadcrumb}</div>
      <div className="smap-inspector__heading">
        <span className="smap-code" data-folder={model.badge.folder}>
          {model.badge.text}
        </span>
        <span className="smap-inspector__name">{model.title}</span>
      </div>
      <div className="smap-summary">
        <div className="smap-summary__label">Summary</div>
        <div className="smap-summary__text">{model.summary}</div>
      </div>
      <CompletenessBar rollup={r} variant="inspector" />
      <div className="smap-inspector__grid">
        <span className="smap-count" data-state="verified">{r.verified} verified</span>
        <span className="smap-count" data-state="built">{r.built} built</span>
        <span className="smap-count" data-state="drafted">{r.drafted} drafted</span>
        <span className="smap-count" data-state="gap">{r.gap} gaps</span>
      </div>
      <div className="smap-inspector__totals">{model.totals}</div>

      <div className="smap-section-label">{model.childHeading}</div>
      <div className="smap-rows">
        {model.children.map((row) => (
          <Row key={row.key} row={row} onSelect={actions.onSelect} />
        ))}
      </div>

      <div className="smap-section-label">Depends on</div>
      <div className="smap-rows">
        {model.dependsOn.map((row) => (
          <Row key={row.key} row={row} onSelect={actions.onSelect} />
        ))}
      </div>

      {model.attention.length > 0 && (
        <div className="smap-attention" role="note">
          <div className="smap-attention__title">
            <span className="smap-dot" data-state="gap" />
            Needs attention
          </div>
          {model.attention.map((line) => (
            <div key={line} className="smap-attention__line">
              {line}
            </div>
          ))}
        </div>
      )}

      {model.kind === "spec" ? (
        <div className="smap-actions">
          <button type="button" className="btn btn--primary btn--sm" onClick={() => actions.onOpenSpec(model.code)}>
            Open in editor
          </button>
          <button type="button" className="btn btn--sm smap-action" onClick={() => actions.onZoomTo(model.ref)}>
            Zoom to
          </button>
        </div>
      ) : (
        <>
          <div className="smap-actions">
            <button type="button" className="btn btn--primary btn--sm" onClick={() => actions.onFocus(model.id)}>
              {model.openLabel}
            </button>
            <button type="button" className="btn btn--sm smap-action" onClick={() => actions.onZoomTo(model.ref)}>
              Zoom to
            </button>
          </div>
          <div className="smap-actions">
            {model.newChildLabel && (
              <button type="button" className="btn btn--sm smap-action" onClick={() => actions.onCreate(model.id)}>
                <Icon.Plus size={12} /> {model.newChildLabel}
              </button>
            )}
            <button type="button" className="btn btn--sm smap-action" onClick={() => actions.onEdit(model.id)}>
              Rename
            </button>
            <button
              type="button"
              className="btn btn--danger btn--sm"
              disabled={!model.canDelete}
              title={model.canDelete ? undefined : "Move the children out first"}
              onClick={() => actions.onDelete(model.id)}
            >
              Delete
            </button>
          </div>
          <div className="smap-actions">
            <button type="button" className="btn btn--sm smap-action" onClick={() => actions.onNewDraft(model.id)}>
              <Icon.Plus size={12} /> New draft
            </button>
          </div>
        </>
      )}
    </>
  );
}

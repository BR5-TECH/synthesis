import type { ReactNode } from "react";
import type { ChangeChip, MapRef } from "../../state/specMap/types";
import { Icon } from "../icons";
import { CompletenessBar } from "./CompletenessBar";
import type { InspectorModel, InspectorRow } from "./inspectorModel";

export interface InspectorActions {
  onClose: () => void;
  onSelect: (ref: MapRef) => void;
  onFocus: (id: string) => void;
  onZoomTo: (ref: MapRef) => void;
  onOpenSpec: (code: string) => void;
  /** SMI-FR-UKIV: open the change of a change chip in its location. */
  onOpenChange: (chip: ChangeChip) => void;
}

/**
 * The insertion points of `SDL-specification-delivery-links.md` in the
 * inspector. Each one gets the subject, a spec node or an index node, and
 * returns what to render there, or null for nothing. A change chip subject
 * gets no slot.
 */
export interface InspectorSlots {
  /** SDL-FR-VAGU: the **Delivered by** list, after the **Depends on** list. */
  deliveredBy?: (subject: MapRef) => ReactNode;
  /** SDL-FR-CFHH: **Plan work**, in the action row after the primary and secondary actions. */
  planWork?: (subject: MapRef) => ReactNode;
}

/** SMI-FR-ZLOA / SMI-FR-BXET: the card of the lines that need attention. */
function Attention({ lines }: { lines: string[] }) {
  if (lines.length === 0) return null;
  return (
    <div className="smap-attention" role="note">
      <div className="smap-attention__title">
        <span className="smap-dot" data-state="gap" />
        Needs attention
      </div>
      {lines.map((line, i) => (
        <div key={`${i}:${line}`} className="smap-attention__line">
          {line}
        </div>
      ))}
    </div>
  );
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
  slots,
}: {
  model: InspectorModel;
  overlay: boolean;
  actions: InspectorActions;
  slots?: InspectorSlots;
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
        <InspectorBody model={model} actions={actions} slots={slots ?? {}} />
      </div>
    </aside>
  );
}

function InspectorBody({
  model,
  actions,
  slots,
}: {
  model: InspectorModel;
  actions: InspectorActions;
  slots: InspectorSlots;
}) {
  if (model.kind === "empty") {
    // SMI-FR-KDGX: the sentence alone, with no action.
    return <p className="smap-inspector__hint">Select a node to inspect it.</p>;
  }

  if (model.kind === "change") {
    // SMI-FR-ZMZB / SMI-FR-UKIV
    return (
      <>
        <div className="smap-inspector__crumbs" data-testid="spec-map-change-holder">On {model.holderLabel}</div>
        <div className="smap-inspector__heading">
          <span className="smap-code smap-code--change">change</span>
          <span className="smap-inspector__name">{model.chip.change}</span>
        </div>
        <div className="smap-section-label">Location</div>
        <div className="smap-rows">
          <div className="smap-row" data-testid="spec-map-change-location">
            <span className="smap-row__label">{model.locationLabel}</span>
          </div>
        </div>
        <Attention lines={model.attention} />
        <div className="smap-actions">
          <button
            type="button"
            className="btn btn--primary btn--sm"
            onClick={() => actions.onOpenChange(model.chip)}
          >
            Open change
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

      {/* SMI-FR-BWIY: an OpenSpec index holds no dependency. */}
      {model.dependsOn && (
        <>
          <div className="smap-section-label">Depends on</div>
          <div className="smap-rows">
            {model.dependsOn.map((row) => (
              <Row key={row.key} row={row} onSelect={actions.onSelect} />
            ))}
          </div>
        </>
      )}

      {/* SDL-FR-VAGU: the Delivered by list follows the Depends on list. */}
      {slots.deliveredBy?.(model.ref)}

      <Attention lines={model.attention} />

      <div className="smap-actions">
        {model.kind === "spec" ? (
          <button type="button" className="btn btn--primary btn--sm" onClick={() => actions.onOpenSpec(model.code)}>
            Open in editor
          </button>
        ) : (
          <button type="button" className="btn btn--primary btn--sm" onClick={() => actions.onFocus(model.id)}>
            {model.openLabel}
          </button>
        )}
        <button type="button" className="btn btn--sm smap-action" onClick={() => actions.onZoomTo(model.ref)}>
          Zoom to
        </button>
        {/* SDL-FR-CFHH: Plan work follows the primary and secondary actions. */}
        {slots.planWork?.(model.ref)}
      </div>
    </>
  );
}

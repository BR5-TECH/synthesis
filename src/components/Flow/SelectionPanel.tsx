import { useLayoutEffect, useRef } from "react";

import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import {
  allowedKinds,
  isContainerKind,
  type FlowDocument,
  type FlowItem,
  type FlowItemKind,
} from "../../state/flowDocument";
import type { ItemDeliveryState } from "../../types";
import type { PanelPlacement, Reference } from "./geometry";
import { KIND_GLYPHS, kindLabel } from "./kindGlyphs";
import { CommentMarkdown } from "../CommentMarkdown";
import {
  DescriptionSection,
  DraftSection,
  FLOW_LOOK,
  FootprintSection,
  ReferencesSection,
  type ItemEditHandlers,
} from "./panelSections";

export interface SelectionPanelProps extends ItemEditHandlers {
  doc: FlowDocument;
  item: FlowItem;
  state?: ItemDeliveryState;
  references: Reference[];
  artifacts: ArtifactOption[];
  /** FLO-FR-ZEJU: every project path, and whether the tree has been read. */
  paths: string[];
  treeLoaded: boolean;
  inert: boolean;
  at: PanelPlacement;
  /** The height of the panel's content, which decides where it opens. */
  onMeasure?: (height: number) => void;
  onRename: () => void;
  /** FLO-FR-NRBV: the error the host shows on the item, at the top of the panel. */
  error?: string;
  /** FLO-FR-NRBV: a draft creation for the item is in flight. */
  creating?: boolean;
  /**
   * MDL-FR-XJBV: a description is one edit when the field loses focus, not one
   * per key, because each edit of a plan binding is a request.
   */
  describeOnBlur?: boolean;
  /** The kinds a change of kind cannot reach beyond the plan rules (GDP-FR-EJTS). */
  kindFilter?: (kind: FlowItemKind) => boolean;
}

/**
 * The editing controls of the selected item of a Flow tab, in a panel attached
 * below its card or header and drawn over the canvas (FLO layout notes). It
 * changes no item's size and no container's bounds. What it offers depends on
 * the kind. In the Delivery lens, the issue view takes its place (MIV).
 */
export function SelectionPanel(props: SelectionPanelProps) {
  const { doc, item, state, inert, onMeasure } = props;
  const leaf = !isContainerKind(item.kind);
  const kinds = allowedKinds(doc, item.id).filter(
    (k) => k === item.kind || (props.kindFilter?.(k) ?? true),
  );
  const fieldsRef = useRef<HTMLDivElement>(null);
  const look = FLOW_LOOK;

  // FLO layout notes: the content's full height, also while the panel is
  // bound and scrolls, so the canvas can open the panel where it fits.
  useLayoutEffect(() => {
    const el = fieldsRef.current;
    if (el && el.scrollHeight > 0) onMeasure?.(el.scrollHeight + 2);
  });

  return (
    <div
      className="flow-panel"
      role="region"
      aria-label={`Edit ${kindLabel(item.kind).toLowerCase()} ${item.name}`}
      data-testid="flow-selection-panel"
      data-placement={props.at.placement}
      style={{
        left: props.at.left,
        top: props.at.top,
        bottom: props.at.bottom,
        maxHeight: props.at.maxHeight,
      }}
    >
      <div className="flow-panel__fields" ref={fieldsRef}>
        <fieldset className="flow-panel__fieldset" disabled={inert}>
          <div className="flow-panel__row flow-panel__row--head">
            <span className="flow-panel__title">
              <span aria-hidden="true">{KIND_GLYPHS[item.kind]}</span> {kindLabel(item.kind)}
            </span>
            <span className="flow-panel__spacer" />
            <button type="button" className={look.ghost} disabled={inert} onClick={props.onRename}>
              Rename
            </button>
            <button type="button" className={look.danger} disabled={inert} onClick={props.onRemove}>
              Remove
            </button>
          </div>
          {props.error && (
            <div className="flow-panel__error" role="alert" data-testid="flow-panel-error">
              {props.error}
            </div>
          )}

          <DescriptionSection
            item={item}
            inert={inert}
            describeOnBlur={props.describeOnBlur === true}
            onDescribe={props.onDescribe}
            look={look}
            renderBody={(markdown) => (
              <CommentMarkdown
                body={markdown}
                className="flow-desc flow-panel__desc"
                testId={`flow-item-description-${item.id}`}
              />
            )}
          />

          <div className="flow-panel__row">
            <label className="flow-panel__label" htmlFor={`flow-kind-${item.id}`}>
              Kind
            </label>
            <select
              id={`flow-kind-${item.id}`}
              className="flow-select"
              aria-label="Kind"
              disabled={inert}
              value={item.kind}
              onChange={(e) => props.onKind(e.target.value as FlowItemKind)}
            >
              {kinds.map((k) => (
                <option key={k} value={k}>
                  {`${KIND_GLYPHS[k]} ${kindLabel(k)}`}
                </option>
              ))}
            </select>
            {item.kind === "milestone" && (
              <>
                <label className="flow-panel__label" htmlFor={`flow-due-${item.id}`}>
                  Due
                </label>
                <input
                  id={`flow-due-${item.id}`}
                  className="flow-input"
                  type="date"
                  aria-label="Due"
                  disabled={inert}
                  value={item.due ?? ""}
                  onChange={(e) => props.onDue(e.target.value)}
                />
              </>
            )}
          </div>

          {leaf && (
            <>
              <DraftSection
                doc={doc}
                item={item}
                state={state}
                inert={inert}
                creating={props.creating}
                look={look}
                handlers={props}
              />
              <FootprintSection
                item={item}
                paths={props.paths}
                treeLoaded={props.treeLoaded}
                inert={inert}
                look={look}
                handlers={props}
              />
            </>
          )}

          <ReferencesSection
            item={item}
            references={props.references}
            artifacts={props.artifacts}
            inert={inert}
            handlers={props}
          />
        </fieldset>
      </div>
    </div>
  );
}

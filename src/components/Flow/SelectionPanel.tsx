import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from "react";

import * as api from "../../api";
import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import { errorLogCode } from "../../errorLogCode";
import { logWarn } from "../../logging";
import {
  allowedKinds,
  isContainerKind,
  itemLinkingDraft,
  type FlowDocument,
  type FlowItem,
  type FlowItemKind,
} from "../../state/flowDocument";
import type { DraftSummary, ItemDeliveryState } from "../../types";
import type { PanelPlacement, Reference } from "./geometry";
import { KIND_GLYPHS, kindLabel } from "./kindGlyphs";
import { ArtifactPicker, ReferenceRows } from "./references";

export interface SelectionPanelProps {
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
  onRemove: () => void;
  onDescribe: (text: string) => void;
  onKind: (kind: FlowItemKind) => void;
  onLinkDraft: (draftId: string | null) => void;
  onAddFootprint: (path: string) => void;
  onRemoveFootprint: (path: string) => void;
  onDue: (due: string) => void;
  onAddReference: (artifactId: string) => void;
  onRemoveReference: (artifactId: string) => void;
  onOpenReference: (artifact: ArtifactOption) => void;
  /**
   * MDL-FR-XJBV: a description is one edit when the field loses focus, not one
   * per key, because each edit of a GitHub plan is a request.
   */
  describeOnBlur?: boolean;
  /**
   * The kinds a change of kind cannot reach beyond the plan rules: a GitHub
   * plan changes no kind to or from a milestone (GDP-FR-EJTS).
   */
  kindFilter?: (kind: FlowItemKind) => boolean;
  /** SDL-FR-KNOP / MDL-FR-QEXS: rows the host adds at the end of the panel. */
  extras?: ReactNode;
}

/**
 * FLO-FR-13: the Markdown source of a description. In a plan binding it is
 * one edit when the field loses focus.
 */
function DescriptionField({
  item,
  inert,
  onBlurOnly,
  onDescribe,
}: {
  item: FlowItem;
  inert: boolean;
  onBlurOnly: boolean;
  onDescribe: (text: string) => void;
}) {
  const [text, setText] = useState(item.description ?? "");
  return (
    <textarea
      id={`flow-desc-${item.id}`}
      className="flow-panel__src"
      aria-label="Description"
      placeholder="Markdown"
      rows={3}
      disabled={inert}
      value={onBlurOnly ? text : (item.description ?? "")}
      onChange={(e) => {
        if (onBlurOnly) setText(e.target.value);
        else onDescribe(e.target.value);
      }}
      onBlur={() => {
        if (onBlurOnly && text !== (item.description ?? "")) onDescribe(text);
      }}
    />
  );
}

/**
 * The editing controls of the selected item, in a panel attached below its
 * card or header and drawn over the canvas (FLO layout notes). It changes no
 * item's size and no container's bounds. What it offers depends on the kind.
 */
export function SelectionPanel(props: SelectionPanelProps) {
  const { doc, item, state, inert } = props;
  const leaf = !isContainerKind(item.kind);
  const kinds = allowedKinds(doc, item.id).filter(
    (k) => k === item.kind || (props.kindFilter?.(k) ?? true),
  );
  const [drafts, setDrafts] = useState<DraftSummary[] | null>(null);
  const fieldsRef = useRef<HTMLDivElement>(null);
  const { onMeasure } = props;

  // FLO layout notes: the content's full height, also while the panel is
  // bound and scrolls, so the canvas can open the panel where it fits.
  useLayoutEffect(() => {
    const el = fieldsRef.current;
    if (el && el.scrollHeight > 0) onMeasure?.(el.scrollHeight + 2);
  });

  // FLO-FR-RMVR: the worktree's drafts, read when a task or a defect is selected.
  useEffect(() => {
    if (!leaf) return;
    let live = true;
    void Promise.resolve()
      .then(() => api.listDrafts())
      .then((h) => {
        if (live) setDrafts(h?.drafts ?? []);
      })
      .catch((e) => {
        logWarn(["frontend"], "drafts not listed for the Flow", { code: errorLogCode(e) });
        if (live) setDrafts([]);
      });
    return () => {
      live = false;
    };
  }, [leaf, item.id]);

  const linkable = (drafts ?? []).filter((d) => {
    const owner = itemLinkingDraft(doc, d.id);
    return owner === undefined;
  });
  const footprint = item.footprint ?? [];
  const pathSet = new Set(props.paths);
  const referenced = new Set(item.artifactIds ?? []);

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
            <button
              type="button"
              className="flow-btn flow-btn--xs flow-btn--quiet"
              disabled={inert}
              onClick={props.onRename}
            >
              Rename
            </button>
            <button
              type="button"
              className="flow-btn flow-btn--xs flow-btn--danger"
              disabled={inert}
              onClick={props.onRemove}
            >
              Remove
            </button>
          </div>

          <label className="flow-panel__label" htmlFor={`flow-desc-${item.id}`}>
            Description
          </label>
          {/* FLO-FR-13: the Markdown source, in a plain field with no toolbar. */}
          <DescriptionField
            item={item}
            inert={inert}
            onBlurOnly={props.describeOnBlur === true}
            onDescribe={props.onDescribe}
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
                  {KIND_GLYPHS[k]} {kindLabel(k)}
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
              <span className="flow-panel__label">Draft</span>
              <div className="flow-panel__row">
                <select
                  className="flow-select"
                  aria-label="Link draft"
                  disabled={inert}
                  value=""
                  onChange={(e) => {
                    if (e.target.value) props.onLinkDraft(e.target.value);
                  }}
                >
                  <option value="">{drafts === null ? "Loading drafts…" : "Link draft…"}</option>
                  {linkable.map((d) => (
                    <option key={d.id} value={d.id}>
                      {d.name}
                    </option>
                  ))}
                </select>
                {item.draftId !== undefined && (
                  <button
                    type="button"
                    className="flow-btn flow-btn--xs"
                    aria-label="Unlink draft"
                    disabled={inert}
                    onClick={() => props.onLinkDraft(null)}
                  >
                    Unlink
                  </button>
                )}
              </div>
              {state?.draftMissing && (
                <div className="flow-panel__note">The linked draft {item.draftId} is missing.</div>
              )}

              <span className="flow-panel__label">Footprint</span>
              {footprint.map((p) => {
                const missing = props.treeLoaded && !pathSet.has(p);
                return (
                  <div className="flow-fp" key={p} data-missing={missing}>
                    <span className="flow-fp__path">{p}</span>
                    {missing && <span className="flow-fp__missing">missing</span>}
                    <button
                      type="button"
                      className="flow-fp__remove"
                      aria-label={`Remove path ${p}`}
                      disabled={inert}
                      onClick={() => props.onRemoveFootprint(p)}
                    >
                      ×
                    </button>
                  </div>
                );
              })}
              <select
                className="flow-select"
                aria-label="Add path"
                disabled={inert}
                value=""
                onChange={(e) => {
                  if (e.target.value) props.onAddFootprint(e.target.value);
                }}
              >
                <option value="">+ Add path</option>
                {props.paths
                  .filter((p) => !footprint.includes(p))
                  .map((p) => (
                    <option key={p} value={p}>
                      {p}
                    </option>
                  ))}
              </select>
            </>
          )}

          <span className="flow-panel__label">References</span>
          <ReferenceRows
            references={props.references}
            onOpen={props.onOpenReference}
            onRemove={props.onRemoveReference}
            disabled={inert}
            className="flow-refs flow-refs--panel"
          />
          <div className="flow-panel__row">
            <ArtifactPicker
              artifacts={props.artifacts}
              referenced={referenced}
              onAdd={props.onAddReference}
              disabled={inert}
            />
          </div>
        </fieldset>
        {props.extras}
      </div>
    </div>
  );
}

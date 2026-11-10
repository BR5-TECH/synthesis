import { useEffect, useState } from "react";

import * as api from "../../api";
import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import { logWarn } from "../../logging";
import {
  allowedKinds,
  isContainerKind,
  itemLinkingDraft,
  syncLinkOf,
  type FlowDocument,
  type FlowItem,
  type FlowItemKind,
} from "../../state/flowDocument";
import type { DraftSummary, ItemDeliveryState } from "../../types";
import type { Reference } from "./geometry";
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
  at: { left: number; top: number; maxHeight?: number };
  onRename: () => void;
  onRemove: () => void;
  onDescribe: (text: string) => void;
  onKind: (kind: FlowItemKind) => void;
  onLinkDraft: (draftId: string | null) => void;
  onAddFootprint: (path: string) => void;
  onRemoveFootprint: (path: string) => void;
  onDue: (due: string) => void;
  onLocalOnly: (on: boolean) => void;
  onLinkExisting: (number: number) => Promise<string | null>;
  onAddReference: (artifactId: string) => void;
  onRemoveReference: (artifactId: string) => void;
  onOpenReference: (artifact: ArtifactOption) => void;
}

/**
 * FLO-FR-TYEZ: Link to existing. It takes an issue or milestone number, and a
 * refusal shows its typed text and stores nothing.
 */
function LinkExisting({
  milestone,
  inert,
  onLink,
}: {
  milestone: boolean;
  inert: boolean;
  onLink: (n: number) => Promise<string | null>;
}) {
  const [open, setOpen] = useState(false);
  const [value, setValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const what = milestone ? "Milestone number" : "Issue number";
  if (!open) {
    return (
      <button
        type="button"
        className="flow-btn flow-btn--xs"
        disabled={inert}
        onClick={() => setOpen(true)}
      >
        Link to existing…
      </button>
    );
  }
  const n = Number(value);
  const valid = Number.isInteger(n) && n > 0;
  return (
    <form
      className="flow-panel__link"
      onSubmit={(e) => {
        e.preventDefault();
        if (!valid || busy) return;
        setBusy(true);
        setError(null);
        void onLink(n).then((refused) => {
          setBusy(false);
          if (refused) setError(refused);
          else {
            setOpen(false);
            setValue("");
          }
        });
      }}
    >
      <input
        className="flow-input flow-input--num"
        type="number"
        min={1}
        aria-label={what}
        placeholder={milestone ? "Milestone" : "Issue #"}
        value={value}
        autoFocus
        onChange={(e) => setValue(e.target.value)}
      />
      <button type="submit" className="flow-btn flow-btn--xs" disabled={!valid || busy || inert}>
        {busy ? "Linking…" : "Link"}
      </button>
      <button type="button" className="flow-btn flow-btn--xs flow-btn--quiet" onClick={() => setOpen(false)}>
        Cancel
      </button>
      {error && (
        <div className="flow-panel__error" role="alert">
          {error}
        </div>
      )}
    </form>
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
  const kinds = allowedKinds(doc, item.id);
  const link = syncLinkOf(doc, item.id);
  const [drafts, setDrafts] = useState<DraftSummary[] | null>(null);

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
        logWarn(["frontend"], "drafts not listed for the Flow", { reason: String(e) });
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
      style={{ left: props.at.left, top: props.at.top, maxHeight: props.at.maxHeight }}
    >
      <div className="flow-panel__fields">
        <fieldset className="flow-panel__fieldset" disabled={inert}>
          <div className="flow-panel__row flow-panel__row--head">
            <span className="flow-panel__title">
              <span aria-hidden="true">{KIND_GLYPHS[item.kind]}</span> {kindLabel(item.kind)}
            </span>
            <span className="flow-panel__spacer" />
            <button type="button" className="flow-btn flow-btn--xs flow-btn--quiet" onClick={props.onRename}>
              Rename
            </button>
            <button
              type="button"
              className="flow-btn flow-btn--xs flow-btn--danger"
              onClick={props.onRemove}
            >
              Remove
            </button>
          </div>

          <label className="flow-panel__label" htmlFor={`flow-desc-${item.id}`}>
            Description
          </label>
          {/* FLO-FR-13: the Markdown source, in a plain field with no toolbar. */}
          <textarea
            id={`flow-desc-${item.id}`}
            className="flow-panel__src"
            aria-label="Description"
            placeholder="Markdown"
            rows={3}
            value={item.description ?? ""}
            onChange={(e) => props.onDescribe(e.target.value)}
          />

          <div className="flow-panel__row">
            <label className="flow-panel__label" htmlFor={`flow-kind-${item.id}`}>
              Kind
            </label>
            <select
              id={`flow-kind-${item.id}`}
              className="flow-select"
              aria-label="Kind"
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

          <label className="flow-switch">
            <input
              type="checkbox"
              role="switch"
              aria-label="Local only"
              checked={item.localOnly === true}
              onChange={(e) => props.onLocalOnly(e.target.checked)}
            />
            <span className="flow-switch__track" aria-hidden="true" />
            Local only
          </label>

          <span className="flow-panel__label">References</span>
          <ReferenceRows
            references={props.references}
            onOpen={props.onOpenReference}
            onRemove={props.onRemoveReference}
            className="flow-refs flow-refs--panel"
          />
          <div className="flow-panel__row">
            <ArtifactPicker
              artifacts={props.artifacts}
              referenced={referenced}
              onAdd={props.onAddReference}
            />
            {doc.sync && !link && (
              <LinkExisting
                milestone={item.kind === "milestone"}
                inert={inert}
                onLink={props.onLinkExisting}
              />
            )}
          </div>
        </fieldset>
      </div>
    </div>
  );
}

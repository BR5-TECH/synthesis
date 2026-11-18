/**
 * The edit sections of a selected item that the panel of a Flow tab
 * (`FLO-flow.md`) and the issue view of the Delivery lens
 * (`MIV-map-issue-view.md` MIV-FR-HHQR) share: the description, the draft, the
 * footprint, and the references. Each surface gives its own look: the classes
 * of its buttons and controls.
 */
import { useEffect, useRef, useState, type ReactNode } from "react";

import * as api from "../../api";
import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import { errorLogCode } from "../../errorLogCode";
import { logWarn } from "../../logging";
import { parseMarkdownBlocks } from "../../diff/markdown";
import { itemLinkingDraft, type FlowDocument, type FlowItem, type FlowItemKind } from "../../state/flowDocument";
import type { DraftSummary, ItemDeliveryState } from "../../types";
import type { Reference } from "./geometry";
import { ArtifactPicker, ReferenceRows } from "./references";

/** The classes a surface gives its buttons and controls. */
export interface PanelLook {
  ghost: string;
  danger: string;
  plain: string;
  control: (base: string) => string;
  /** The glyph of the draft row and of each kind option. */
  glyphs: boolean;
}

/** The look of the panel of a Flow tab. */
export const FLOW_LOOK: PanelLook = {
  ghost: "flow-btn flow-btn--xs flow-btn--quiet",
  danger: "flow-btn flow-btn--xs flow-btn--danger",
  plain: "flow-btn flow-btn--xs",
  control: (base) => base,
  glyphs: true,
};

/** MIV layout notes: the issue view takes the buttons of the design system. */
export const ISSUE_LOOK: PanelLook = {
  ghost: "btn btn--ghost btn--sm",
  danger: "btn btn--danger btn--sm",
  plain: "btn btn--default btn--sm",
  control: (base) => `${base} input`,
  glyphs: false,
};

/** The edits of a selected item that a panel or an issue view offers. */
export interface ItemEditHandlers {
  onRemove: () => void;
  onDescribe: (text: string) => void;
  onKind: (kind: FlowItemKind) => void;
  onLinkDraft: (draftId: string | null) => void;
  /** DBR-FR-GFGG / FLO-FR-QZUR: open the linked draft. */
  onOpenDraft?: (draftId: string) => void;
  /** FLO-FR-NRBV: create a draft for a task or a defect with no draft link. */
  onCreateDraft?: () => void;
  onAddFootprint: (path: string) => void;
  onRemoveFootprint: (path: string) => void;
  onDue: (due: string) => void;
  onAddReference: (artifactId: string) => void;
  onRemoveReference: (artifactId: string) => void;
  onOpenReference: (artifact: ArtifactOption) => void;
}

/**
 * Whether a description opens with its own `Description` heading. The section
 * then keeps its label for assistive technology alone, so the word does not
 * show twice.
 */
export function opensWithDescriptionHeading(markdown: string | undefined): boolean {
  if (!markdown) return false;
  const first = parseMarkdownBlocks(markdown).find((b) => b.kind !== "break" && b.kind !== "comment");
  return first?.kind === "heading" && first.text.trim().toLowerCase() === "description";
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
  rows,
  className,
  focusOnShow = false,
}: {
  item: FlowItem;
  inert: boolean;
  onBlurOnly: boolean;
  onDescribe: (text: string) => void;
  rows: number;
  className: string;
  /** MIV-FR-LENY: the field takes the focus when Edit shows it. */
  focusOnShow?: boolean;
}) {
  const [text, setText] = useState(item.description ?? "");
  // FLO-FR-13 / MDL-FR-XJBV / MIV-FR-BEZE: a field that goes away with no blur
  // (Done, a close, or the canvas turning inert) still makes its one edit.
  const pending = useRef({ text, onBlurOnly, onDescribe, saved: item.description ?? "" });
  pending.current = { text, onBlurOnly, onDescribe, saved: pending.current.saved };
  useEffect(
    () => () => {
      const p = pending.current;
      if (p.onBlurOnly && p.text !== p.saved) p.onDescribe(p.text);
    },
    [],
  );
  return (
    <textarea
      id={`flow-desc-${item.id}`}
      className={className}
      aria-label="Description"
      placeholder="Markdown"
      rows={rows}
      autoFocus={focusOnShow}
      disabled={inert}
      value={onBlurOnly ? text : (item.description ?? "")}
      onChange={(e) => {
        if (onBlurOnly) setText(e.target.value);
        else onDescribe(e.target.value);
      }}
      onBlur={() => {
        if (onBlurOnly && text !== pending.current.saved) {
          pending.current.saved = text;
          onDescribe(text);
        }
      }}
    />
  );
}

/**
 * FLO-FR-13 / MIV-FR-LENY: the description as rich text, and its Markdown
 * source in a plain field with no toolbar behind **Edit**. `renderBody` draws
 * the rich text, so each surface picks its renderer.
 */
export function DescriptionSection({
  item,
  inert,
  describeOnBlur,
  onDescribe,
  look,
  renderBody,
  rows = 3,
  fieldClassName = "flow-panel__src",
  className = "flow-panel__section",
  onEditingChange,
}: {
  item: FlowItem;
  inert: boolean;
  describeOnBlur: boolean;
  onDescribe: (text: string) => void;
  look: PanelLook;
  renderBody: (markdown: string) => ReactNode;
  rows?: number;
  fieldClassName?: string;
  className?: string;
  /** MIV-FR-SWZY: the source field shows or goes away. */
  onEditingChange?: (editing: boolean) => void;
}) {
  // FLO-FR-13: the section opens on the rich text; Edit shows the source.
  const [editing, setEditingState] = useState(false);
  const setEditing = (next: boolean) => {
    setEditingState(next);
    onEditingChange?.(next);
  };
  return (
    <div className={className} data-testid="flow-description-section">
      <div className="flow-panel__row flow-panel__row--label">
        <label
          className={
            !editing && opensWithDescriptionHeading(item.description)
              ? "flow-panel__label sr-only"
              : "flow-panel__label"
          }
          htmlFor={`flow-desc-${item.id}`}
        >
          Description
        </label>
        <span className="flow-panel__spacer" />
        <button
          type="button"
          className={look.ghost}
          aria-label={editing ? "Done editing the description" : "Edit the description"}
          aria-pressed={editing}
          disabled={inert}
          onClick={() => setEditing(!editing)}
        >
          {editing ? "Done" : "Edit"}
        </button>
      </div>
      {editing && !inert ? (
        <div
          onKeyDown={(e) => {
            // MIV-FR-SWZY: in the issue view, Escape leaves the source and
            // keeps the view open.
            if (e.key === "Escape" && onEditingChange) {
              e.stopPropagation();
              setEditing(false);
            }
          }}
        >
          <DescriptionField
            item={item}
            inert={inert}
            onBlurOnly={describeOnBlur}
            onDescribe={onDescribe}
            rows={rows}
            className={fieldClassName}
            focusOnShow={onEditingChange !== undefined}
          />
        </div>
      ) : item.description ? (
        renderBody(item.description)
      ) : (
        <div className="flow-panel__empty">No description.</div>
      )}
    </div>
  );
}

/** FLO-FR-RMVR: the worktree's drafts, read when a task or a defect is shown. */
export function useLinkableDrafts(doc: FlowDocument, item: FlowItem, leaf: boolean) {
  const [drafts, setDrafts] = useState<DraftSummary[] | null>(null);
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
  const linkable = (drafts ?? []).filter((d) => itemLinkingDraft(doc, d.id) === undefined);
  return { loading: drafts === null, linkable };
}

/** FLO-FR-RMVR / FLO-FR-NRBV / FLO-FR-QZUR: the draft of a task or a defect. */
export function DraftSection({
  doc,
  item,
  state,
  inert,
  creating,
  look,
  handlers,
  className = "flow-panel__section",
}: {
  doc: FlowDocument;
  item: FlowItem;
  state?: ItemDeliveryState;
  inert: boolean;
  creating?: boolean;
  look: PanelLook;
  handlers: Pick<ItemEditHandlers, "onLinkDraft" | "onOpenDraft" | "onCreateDraft">;
  className?: string;
}) {
  const { loading, linkable } = useLinkableDrafts(doc, item, true);
  return (
    <div className={className}>
      <span className="flow-panel__label">Draft</span>
      {item.draftId === undefined && handlers.onCreateDraft && (
        <div className="flow-panel__row">
          <button
            type="button"
            className={look.plain}
            disabled={inert || creating}
            onClick={handlers.onCreateDraft}
          >
            {creating ? "Creating draft…" : "Create draft"}
          </button>
        </div>
      )}
      {item.draftId !== undefined && !state?.draftMissing && handlers.onOpenDraft && (
        <div className="flow-panel__row">
          <button
            type="button"
            className="flow-draft"
            aria-label={`Open draft ${state?.draft?.name ?? item.draftId}`}
            data-testid="flow-panel-draft"
            onClick={() => handlers.onOpenDraft!(item.draftId!)}
          >
            {look.glyphs && <span aria-hidden="true">✎</span>}
            <span className="flow-draft__name">{state?.draft?.name ?? item.draftId}</span>
            {state?.run && (
              <span className="flow-draft__run">{state.run.state.replace(/_/g, " ")}</span>
            )}
          </button>
        </div>
      )}
      <div className="flow-panel__row">
        <select
          className={look.control("flow-select")}
          aria-label="Link draft"
          disabled={inert}
          value=""
          onChange={(e) => {
            if (e.target.value) handlers.onLinkDraft(e.target.value);
          }}
        >
          <option value="">{loading ? "Loading drafts…" : "Link draft…"}</option>
          {linkable.map((d) => (
            <option key={d.id} value={d.id}>
              {d.name}
            </option>
          ))}
        </select>
        {item.draftId !== undefined && (
          <button
            type="button"
            className={look.plain}
            aria-label="Unlink draft"
            disabled={inert}
            onClick={() => handlers.onLinkDraft(null)}
          >
            Unlink
          </button>
        )}
      </div>
      {state?.draftMissing && (
        <div className="flow-panel__note">The linked draft {item.draftId} is missing.</div>
      )}
    </div>
  );
}

/** FLO-FR-ZEJU: the footprint of a task or a defect. */
export function FootprintSection({
  item,
  paths,
  treeLoaded,
  inert,
  look,
  handlers,
  className = "flow-panel__section",
}: {
  item: FlowItem;
  paths: string[];
  treeLoaded: boolean;
  inert: boolean;
  look: PanelLook;
  handlers: Pick<ItemEditHandlers, "onAddFootprint" | "onRemoveFootprint">;
  className?: string;
}) {
  const footprint = item.footprint ?? [];
  const pathSet = new Set(paths);
  return (
    <div className={className}>
      <span className="flow-panel__label">Footprint</span>
      {footprint.map((p) => {
        const missing = treeLoaded && !pathSet.has(p);
        return (
          <div className="flow-fp" key={p} data-missing={missing}>
            <span className="flow-fp__path">{p}</span>
            {missing && <span className="flow-fp__missing">missing</span>}
            <button
              type="button"
              className="flow-fp__remove"
              aria-label={`Remove path ${p}`}
              disabled={inert}
              onClick={() => handlers.onRemoveFootprint(p)}
            >
              ×
            </button>
          </div>
        );
      })}
      <select
        className={look.control("flow-select")}
        aria-label="Add path"
        disabled={inert}
        value=""
        onChange={(e) => {
          if (e.target.value) handlers.onAddFootprint(e.target.value);
        }}
      >
        <option value="">+ Add path</option>
        {paths
          .filter((p) => !footprint.includes(p))
          .map((p) => (
            <option key={p} value={p}>
              {p}
            </option>
          ))}
      </select>
    </div>
  );
}

/** FLO-FR-10 / FLO-FR-12: the references of an item and the picker to add one. */
export function ReferencesSection({
  item,
  references,
  artifacts,
  inert,
  handlers,
  className = "flow-panel__section",
}: {
  item: FlowItem;
  references: Reference[];
  artifacts: ArtifactOption[];
  inert: boolean;
  handlers: Pick<ItemEditHandlers, "onAddReference" | "onRemoveReference" | "onOpenReference">;
  className?: string;
}) {
  return (
    <div className={className}>
      <span className="flow-panel__label">References</span>
      <ReferenceRows
        references={references}
        onOpen={handlers.onOpenReference}
        onRemove={handlers.onRemoveReference}
        disabled={inert}
        className="flow-refs flow-refs--panel"
      />
      <div className="flow-panel__row">
        <ArtifactPicker
          artifacts={artifacts}
          referenced={new Set(item.artifactIds ?? [])}
          onAdd={handlers.onAddReference}
          disabled={inert}
        />
      </div>
    </div>
  );
}

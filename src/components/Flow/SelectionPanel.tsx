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
import { Icon } from "../icons";
import { KindChip } from "./boardItems";
import { KIND_GLYPHS, kindLabel } from "./kindGlyphs";
import { ArtifactPicker, ReferenceRows } from "./references";
import { CommentMarkdown } from "../CommentMarkdown";
import { parseMarkdownBlocks } from "../../diff/markdown";

/**
 * Whether a description opens with its own `Description` heading. The panel
 * then keeps its label for assistive technology alone, so the word does not
 * show twice.
 */
function opensWithDescriptionHeading(markdown: string | undefined): boolean {
  if (!markdown) return false;
  const first = parseMarkdownBlocks(markdown).find((b) => b.kind !== "break" && b.kind !== "comment");
  return first?.kind === "heading" && first.text.trim().toLowerCase() === "description";
}

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
  /** DBR-FR-GFGG / FLO-FR-QZUR: open the linked draft from the panel. */
  onOpenDraft?: (draftId: string) => void;
  /** DBR-FR-CJSG: the error the host shows on the item, at the top of the panel. */
  error?: string;
  /** FLO-FR-NRBV: a draft creation for the item is in flight. */
  creating?: boolean;
  /** FLO-FR-NRBV: create a draft for a task or a defect with no draft link. */
  onCreateDraft?: () => void;
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
  /**
   * MDL-FR-XKCH: the panel is the popover of the board: a header with the
   * kind chip, the address, and the name; labelled sections; and a footer.
   */
  board?: boolean;
  /** MDL-FR-XKCH: the address of the item in the header of the popover. */
  address?: ReactNode;
  /** MDL-FR-XKCH: the status dot and word of the item, in its own section. */
  status?: ReactNode;
  /** MDL-FR-XCJN: **Discuss** in the footer of the popover. */
  onDiscuss?: () => void;
  /** MDL-FR-XKCH / MDL-FR-QEXS: the host's actions in the footer, after **Discuss**. */
  footer?: ReactNode;
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
  // FLO-FR-13 / MDL-FR-XJBV: a field that goes away with no blur (Done, or
  // the canvas turning inert) still makes its one edit.
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
        if (onBlurOnly && text !== pending.current.saved) {
          pending.current.saved = text;
          onDescribe(text);
        }
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
  // FLO-FR-13: the panel opens on the rich text; Edit shows the source.
  const [editing, setEditing] = useState(false);
  const fieldsRef = useRef<HTMLDivElement>(null);
  const headRef = useRef<HTMLDivElement>(null);
  const footRef = useRef<HTMLDivElement>(null);
  const { onMeasure, board = false } = props;

  // FLO layout notes: the content's full height, also while the panel is
  // bound and scrolls, so the canvas can open the panel where it fits.
  useLayoutEffect(() => {
    const el = fieldsRef.current;
    const frame = (headRef.current?.offsetHeight ?? 0) + (footRef.current?.offsetHeight ?? 0);
    if (el && el.scrollHeight > 0) onMeasure?.(el.scrollHeight + frame + 2);
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

  /** MDL-FR-XKCH: the buttons of the popover are the buttons of the design system. */
  const ghost = board ? "btn btn--ghost btn--sm" : "flow-btn flow-btn--xs flow-btn--quiet";
  const danger = board ? "btn btn--danger btn--sm" : "flow-btn flow-btn--xs flow-btn--danger";
  const plain = board ? "btn btn--default btn--sm" : "flow-btn flow-btn--xs";
  const control = (base: string) => (board ? `${base} input` : base);

  const renameRemove = (
    <>
      <button type="button" className={ghost} disabled={inert} onClick={props.onRename}>
        Rename
      </button>
      <button type="button" className={danger} disabled={inert} onClick={props.onRemove}>
        Remove
      </button>
    </>
  );

  return (
    <div
      className={board ? "flow-panel dm-pop" : "flow-panel"}
      role="region"
      aria-label={`Edit ${kindLabel(item.kind).toLowerCase()} ${item.name}`}
      data-testid="flow-selection-panel"
      data-placement={props.at.placement}
      data-board={board}
      style={{
        left: props.at.left,
        top: props.at.top,
        bottom: props.at.bottom,
        maxHeight: props.at.maxHeight,
      }}
    >
      {board && (
        <div className="dm-pop__head" ref={headRef}>
          <div className="dm-pop__top">
            {item.kind === "milestone" ? (
              <Icon.Flag size={14} className="icon dm-lane__flag" aria-label="Milestone" />
            ) : (
              <KindChip kind={item.kind} />
            )}
            <span className="dm-pop__ref">{props.address}</span>
            {renameRemove}
          </div>
          <div className="dm-pop__title">{item.name || "Unnamed"}</div>
        </div>
      )}
      <div className={board ? "flow-panel__fields dm-pop__body" : "flow-panel__fields"} ref={fieldsRef}>
        <fieldset className="flow-panel__fieldset" disabled={inert}>
          {!board && (
            <div className="flow-panel__row flow-panel__row--head">
              <span className="flow-panel__title">
                <span aria-hidden="true">{KIND_GLYPHS[item.kind]}</span> {kindLabel(item.kind)}
              </span>
              <span className="flow-panel__spacer" />
              {renameRemove}
            </div>
          )}
          {props.error && (
            <div className="flow-panel__error" role="alert" data-testid="flow-panel-error">
              {props.error}
            </div>
          )}

          {board && props.status && (
            <div className="flow-panel__section" data-testid="flow-panel-status">
              <span className="flow-panel__label">Status</span>
              {props.status}
            </div>
          )}

          <div className="flow-panel__section">
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
                className={ghost}
                aria-label={editing ? "Done editing the description" : "Edit the description"}
                aria-pressed={editing}
                disabled={inert}
                onClick={() => setEditing((e) => !e)}
              >
                {editing ? "Done" : "Edit"}
              </button>
            </div>
            {/* FLO-FR-13: the full description as rich text, and its Markdown
                source in a plain field with no toolbar behind Edit. */}
            {editing && !inert ? (
              <DescriptionField
                item={item}
                inert={inert}
                onBlurOnly={props.describeOnBlur === true}
                onDescribe={props.onDescribe}
              />
            ) : item.description ? (
              <CommentMarkdown
                body={item.description}
                className="flow-desc flow-panel__desc"
                testId={`flow-item-description-${item.id}`}
              />
            ) : (
              <div className="flow-panel__empty">No description.</div>
            )}
          </div>

          <div className="flow-panel__row flow-panel__row--kind">
            <label className="flow-panel__label" htmlFor={`flow-kind-${item.id}`}>
              Kind
            </label>
            <select
              id={`flow-kind-${item.id}`}
              className={control("flow-select")}
              aria-label="Kind"
              disabled={inert}
              value={item.kind}
              onChange={(e) => props.onKind(e.target.value as FlowItemKind)}
            >
              {kinds.map((k) => (
                <option key={k} value={k}>
                  {board ? kindLabel(k) : `${KIND_GLYPHS[k]} ${kindLabel(k)}`}
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
                  className={control("flow-input")}
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
              <div className="flow-panel__section">
                <span className="flow-panel__label">Draft</span>
                {item.draftId === undefined && props.onCreateDraft && (
                  <div className="flow-panel__row">
                    <button
                      type="button"
                      className={plain}
                      disabled={inert || props.creating}
                      onClick={props.onCreateDraft}
                    >
                      {props.creating ? "Creating draft…" : "Create draft"}
                    </button>
                  </div>
                )}
                {item.draftId !== undefined && !state?.draftMissing && props.onOpenDraft && (
                  <div className="flow-panel__row">
                    <button
                      type="button"
                      className="flow-draft"
                      aria-label={`Open draft ${state?.draft?.name ?? item.draftId}`}
                      data-testid="flow-panel-draft"
                      onClick={() => props.onOpenDraft!(item.draftId!)}
                    >
                      {!board && <span aria-hidden="true">✎</span>}
                      <span className="flow-draft__name">{state?.draft?.name ?? item.draftId}</span>
                      {state?.run && (
                        <span className="flow-draft__run">{state.run.state.replace(/_/g, " ")}</span>
                      )}
                    </button>
                  </div>
                )}
                <div className="flow-panel__row">
                  <select
                    className={control("flow-select")}
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
                      className={plain}
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
              </div>

              <div className="flow-panel__section">
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
                  className={control("flow-select")}
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
              </div>
            </>
          )}

          <div className="flow-panel__section">
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
          </div>
        </fieldset>
        {props.extras}
      </div>
      {board && (props.onDiscuss || props.footer) && (
        <div className="dm-pop__foot" ref={footRef} data-testid="flow-panel-foot">
          {props.onDiscuss && (
            <button type="button" className="btn btn--default btn--sm" onClick={props.onDiscuss}>
              Discuss
            </button>
          )}
          {props.footer}
        </div>
      )}
    </div>
  );
}

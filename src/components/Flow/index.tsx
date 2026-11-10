import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  ancestorsOf,
  CONTAINER_HEADER_H,
  findItem,
  isContainerKind,
  ITEM_KINDS,
  leavesUnder,
  syncLinkOf,
  type FlowDocument,
  type FlowItem,
} from "../../state/flowDocument";
import type { FlowViolation } from "../../types";
import { ActionControl } from "../ActionControl";
import { Icon } from "../icons";
import { Band } from "./Band";
import { EdgeHandles, EdgeLayer, EdgePanel, routeEdges } from "./edges";
import { edgeMidpoint, panelPlacement, resolveReferences, todayIso, ZOOM_STEP } from "./geometry";
import { CardView, ContainerView, type ItemFacts, type ItemHandlers } from "./items";
import { KIND_GLYPHS, kindLabel } from "./kindGlyphs";
import { SelectionPanel } from "./SelectionPanel";
import { SyncReview } from "./SyncReview";
import { useFlowCanvas, type FlowCanvasProps } from "./useFlowCanvas";
import { useItemActions } from "./useItemActions";

export type { CreatedDraftOutcome, FlowCanvasProps } from "./useFlowCanvas";

/** FLO-FR-05 / FLO-FR-47: the violations a report named, each at its item or edge. */
function ViolationList({
  violations,
  doc,
}: {
  violations: readonly FlowViolation[];
  doc: FlowDocument | null;
}) {
  if (violations.length === 0) return null;
  return (
    <ul className="flow-violations" data-testid="flow-violations">
      {violations.map((v, i) => {
        const at = v.elementId ?? v.edgeId;
        const named = v.elementId && doc ? findItem(doc, v.elementId)?.name : undefined;
        return (
          <li key={i} data-code={v.code}>
            {at && <code className="flow-violations__at">{named ? `${named} (${at})` : at}</code>}{" "}
            {v.message}
          </li>
        );
      })}
    </ul>
  );
}

/**
 * `FLO-flow.md`: the Flow tab, a delivery plan on a canvas.
 *
 * The plan lives in the shell's `FlowSessionStore` (FLO-FR-30), so a tab
 * switch neither reloads it nor loses its edits. Selection, pan, zoom, and
 * collapse are view state of this canvas (FLO-FR-22). Items are laid out flat
 * in canvas coordinates: containers first, outermost first, then the edges,
 * then the cards.
 */
export function FlowCanvas(props: FlowCanvasProps) {
  const { flowId } = props;
  const c = useFlowCanvas(props);
  const a = useItemActions(c, props);
  const { doc, session, sync, delivery, project, frames, view, inert } = c;
  /** The measured height of the selected item's panel content. */
  const [panelHeight, setPanelHeight] = useState<{ id: string; height: number } | null>(null);
  const selectedId = c.selected?.id ?? null;
  const onPanelMeasure = useCallback(
    (height: number) => {
      if (!selectedId) return;
      setPanelHeight((prev) =>
        prev?.id === selectedId && prev.height === height ? prev : { id: selectedId, height },
      );
    },
    [selectedId],
  );

  const byId = useMemo(
    () => new Map(project.artifacts.map((x) => [x.id, x])),
    [project.artifacts],
  );

  /** FLO-FR-KWFH: each leaf's overlaps, the other items and the shared paths. */
  const overlapByItem = useMemo(() => {
    const out = new Map<string, { ids: Set<string>; paths: Set<string> }>();
    for (const o of delivery.overlaps ?? []) {
      const [x, y] = o.items;
      for (const [me, other] of [
        [x, y],
        [y, x],
      ] as const) {
        const entry = out.get(me) ?? { ids: new Set<string>(), paths: new Set<string>() };
        entry.ids.add(other);
        o.paths.forEach((p) => entry.paths.add(p));
        out.set(me, entry);
      }
    }
    return out;
  }, [delivery.overlaps]);

  const nameOf = (id: string) => (doc ? findItem(doc, id)?.name || id : id);

  const factsOf = (item: FlowItem): ItemFacts => {
    const state = delivery.states?.get(item.id);
    const overlap = overlapByItem.get(item.id);
    return {
      state,
      blockers: (state?.blockedBy ?? [])
        .filter((id) => doc && findItem(doc, id))
        .map((id) => ({ id, name: nameOf(id) })),
      overlap:
        overlap && !isContainerKind(item.kind)
          ? {
              ids: [...overlap.ids],
              names: [...overlap.ids].map(nameOf),
              paths: [...overlap.paths],
            }
          : null,
      link: doc ? syncLinkOf(doc, item.id) : undefined,
      references: resolveReferences(item.artifactIds, byId, project.loaded),
    };
  };

  const handlersOf = (item: FlowItem): ItemHandlers => ({
    onDragStart: (e) => c.onItemDragStart(item.id, e),
    onPointerUp: () => c.onItemPointerUp(item.id),
    onConnectStart: (e) => c.beginConnection(item.id, e),
    onRename: (name) => a.rename(item.id, name),
    onStartRename: () => c.setEditingName(item.id),
    onEndRename: () => c.setEditingName(null),
    onSelectItems: (ids) => {
      c.setSelection(ids);
      c.setSelectedEdge(null);
    },
    onOpenTracker: a.openTracker,
    onOpenReference: a.openReferenced,
  });

  const today = todayIso();
  const shown = doc ? doc.items.filter((i) => !frames.hiddenBy(i.id)) : [];
  const containers = doc
    ? shown
        .filter((i) => isContainerKind(i.kind))
        .sort((x, y) => ancestorsOf(doc, x.id).length - ancestorsOf(doc, y.id).length)
    : [];
  const cards = shown.filter((i) => !isContainerKind(i.kind));
  const routes = doc ? routeEdges(doc, frames) : [];
  const refusedRoute =
    c.refusal?.attempt && doc ? frames.route(c.refusal.attempt.from, c.refusal.attempt.to) : null;
  const selectionSet = new Set(c.selection);

  /** Canvas coordinates to the coordinates of the canvas element. */
  const toScreen = (x: number, y: number) => ({
    left: view.x + x * view.scale,
    top: view.y + y * view.scale,
  });

  const panelAt = (() => {
    const item = c.selected;
    if (!item || frames.hiddenBy(item.id)) return null;
    const r = frames.rectOf(item.id);
    if (!r) return null;
    const below = isContainerKind(item.kind) ? r.y + CONTAINER_HEADER_H : r.y + r.height;
    const head = toScreen(r.x, r.y);
    const foot = toScreen(r.x, below);
    const el = c.surfaceRef.current;
    return panelPlacement(
      { left: head.left, top: head.top, bottom: foot.top },
      { width: el?.clientWidth ?? 0, height: el?.clientHeight ?? 0 },
      panelHeight?.id === item.id ? panelHeight.height : undefined,
    );
  })();

  const edgeSelected = doc && c.selectedEdge ? doc.edges.find((e) => e.id === c.selectedEdge) : undefined;
  const edgePanelAt = (() => {
    if (!edgeSelected) return null;
    const r = routes.find((x) => x.edge.id === edgeSelected.id)?.route;
    if (!r) return null;
    const mid = edgeMidpoint(r.ax, r.ay, r.bx, r.by, r.faceA, r.faceB);
    const at = toScreen(mid.x, mid.y);
    return { left: at.left + 12, top: at.top + 12 };
  })();

  const blockedCount = delivery.states
    ? [...delivery.states.values()].filter((s) => s.blocked).length
    : null;
  const readout = doc
    ? [
        `${doc.items.length} ${doc.items.length === 1 ? "item" : "items"}`,
        `${doc.edges.length} ${doc.edges.length === 1 ? "edge" : "edges"}`,
        ...(blockedCount !== null ? [`${blockedCount} blocked`] : []),
        ...(delivery.overlaps
          ? [`${delivery.overlaps.length} ${delivery.overlaps.length === 1 ? "overlap" : "overlaps"}`]
          : []),
      ].join(" · ")
    : "";

  const flowRefs = doc ? resolveReferences(doc.references, byId, project.loaded) : [];

  return (
    <div className="flow-surface" data-syncing={inert}>
      <Band
        doc={doc}
        references={flowRefs}
        artifacts={project.referenceable}
        sync={sync}
        onName={a.setFlowNameTo}
        onDescription={a.setFlowDescriptionTo}
        onAddReference={a.addFlowRef}
        onRemoveReference={a.removeFlowRef}
        onOpenReference={a.openReferenced}
        onSync={a.sync}
        onOpenReview={() => a.setReviewOpen(true)}
      />

      {/* FLO-FR-47: a refused write lists every violation and keeps the plan. */}
      {session?.error && (
        <div className="flow-banner flow-banner--error" role="alert">
          {session.violations.length > 0 ? (
            <>
              <strong>This Flow was not written.</strong>
              <ViolationList violations={session.violations} doc={doc} />
            </>
          ) : (
            session.error
          )}
        </div>
      )}

      {/* FLO-FR-05 / FLO-FR-46: a body the validation refused shows no plan,
          offers no editing, and is never written. */}
      {session?.parseError ? (
        <div className="flow-error" role="alert">
          <strong>This file could not be read as a Flow.</strong>
          {session.violations.length > 0 ? (
            <ViolationList violations={session.violations} doc={null} />
          ) : (
            <span>{session.parseError}</span>
          )}
        </div>
      ) : !flowId ? (
        <div className="flow-error">No Flow is open.</div>
      ) : !session?.loaded || !doc ? (
        <div className="flow-error">Loading…</div>
      ) : (
        <div
          className="flow"
          ref={c.surfaceRef}
          onPointerDown={c.onSurfacePointerDown}
          onPointerMove={c.onSurfacePointerMove}
          onPointerUp={() => c.endGesture()}
          onPointerCancel={() => c.endGesture({ abandon: true })}
        >
          <div
            className="flow-viewport"
            style={{
              transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
              transformOrigin: "0 0",
            }}
          >
            {containers.map((item) => {
              const r = frames.rectOf(item.id)!;
              const facts = factsOf(item);
              return (
                <ContainerView
                  key={item.id}
                  item={item}
                  at={{ x: r.x, y: r.y }}
                  size={{ width: r.width, height: r.height }}
                  facts={facts}
                  selected={selectionSet.has(item.id)}
                  magnet={c.magnet === item.id}
                  editingName={c.editingName === item.id}
                  inert={inert}
                  handlers={handlersOf(item)}
                  collapsed={c.collapsed.has(item.id)}
                  leaves={leavesUnder(doc, item.id).length}
                  duePast={
                    item.due !== undefined && item.due < today && facts.state?.state !== "done"
                  }
                  onToggleCollapsed={() => c.toggleCollapsed(item.id)}
                  onResizeStart={(sides, e) => c.onResizeStart(item.id, sides, e)}
                />
              );
            })}

            <EdgeLayer
              routes={routes}
              selectedEdge={c.selectedEdge}
              canvasId={c.canvasId}
              preview={c.connectionPreview}
              refused={refusedRoute}
            />

            {cards.map((item) => (
              <CardView
                key={item.id}
                item={item}
                at={c.absOf(item.id)}
                facts={factsOf(item)}
                selected={selectionSet.has(item.id)}
                magnet={c.magnet === item.id}
                editingName={c.editingName === item.id}
                inert={inert}
                handlers={handlersOf(item)}
                creating={a.creating === item.id}
                draftError={a.draftErrors[item.id]}
                onMeasure={c.onMeasureCard}
                onCreateDraft={() => void a.createDraftFor(item.id)}
                onOpenDraft={a.openDraft}
                onUnlinkDraft={() => a.linkDraft(item.id, null)}
              />
            ))}

            <EdgeHandles
              doc={doc}
              routes={routes}
              selectedEdge={c.selectedEdge}
              onSelect={(id) => {
                c.setSelectedEdge(id);
                c.setSelection([]);
              }}
            />
          </div>

          {/* FLO-FR-16 / FLO-FR-OAOZ / FLO-FR-GZCH: the feedback of a refusal. */}
          {c.refusal && (
            <div className="flow-refusal" role="status" data-testid="flow-refusal">
              <strong className="flow-refusal__title">{c.refusal.title}</strong>
              <span>{c.refusal.text}</span>
              <button
                type="button"
                className="flow-refusal__close"
                aria-label="Dismiss"
                onClick={() => c.setRefusal(null)}
              >
                ×
              </button>
            </div>
          )}

          {c.selected && panelAt && (
            <SelectionPanel
              key={c.selected.id}
              doc={doc}
              item={c.selected}
              state={delivery.states?.get(c.selected.id)}
              references={resolveReferences(c.selected.artifactIds, byId, project.loaded)}
              artifacts={project.referenceable}
              paths={project.paths}
              treeLoaded={project.loaded}
              inert={inert}
              at={panelAt}
              onMeasure={onPanelMeasure}
              onRename={() => c.setEditingName(c.selected!.id)}
              onRemove={() => a.requestRemove(c.selected!.id)}
              onDescribe={(text) => a.describe(c.selected!.id, text)}
              onKind={(kind) => a.requestKind(c.selected!.id, kind)}
              onLinkDraft={(id) => a.linkDraft(c.selected!.id, id)}
              onAddFootprint={(p) => a.addFootprint(c.selected!.id, p)}
              onRemoveFootprint={(p) => a.removeFootprint(c.selected!.id, p)}
              onDue={(due) => a.setDueDate(c.selected!.id, due)}
              onLocalOnly={(on) => a.setLocal(c.selected!.id, on)}
              onLinkExisting={(n) => a.linkExisting(c.selected!.id, n)}
              onAddReference={(id) => a.addReference(c.selected!.id, id)}
              onRemoveReference={(id) => a.removeReference(c.selected!.id, id)}
              onOpenReference={a.openReferenced}
            />
          )}

          {edgeSelected && edgePanelAt && (
            <EdgePanel
              doc={doc}
              edge={edgeSelected}
              at={edgePanelAt}
              inert={inert}
              onType={(type) => a.retypeEdge(edgeSelected.id, type)}
              onReverse={() => a.reverse(edgeSelected.id)}
              onRemove={() => a.dropEdge(edgeSelected.id)}
            />
          )}

          {sync.reviewOpen && (
            <SyncReview
              doc={doc}
              sync={sync}
              onClose={() => a.setReviewOpen(false)}
              onApply={a.applyResolutions}
              onSyncAgain={a.sync}
              onSelectItem={c.revealItem}
              onOpenLink={a.openTracker}
            />
          )}

          {/* FLO-FR-07: one add affordance, a menu of the five kinds. */}
          <div
            className="flow-addmenu"
            onKeyDown={(e) => {
              if (e.key === "Escape" && c.addMenuOpen) {
                e.stopPropagation();
                c.setAddMenuOpen(false);
              }
            }}
          >
            <button
              type="button"
              className="flow-overlay flow-overlay--add"
              aria-label="Add to plan"
              title="Add to plan"
              aria-haspopup="menu"
              aria-expanded={c.addMenuOpen}
              disabled={inert}
              onClick={() => (c.addMenuOpen ? c.setAddMenuOpen(false) : c.openAddMenu())}
            >
              <Icon.Plus size={14} />
            </button>
            {c.addMenuOpen && (
              <AddMenu
                onPick={(kind) => {
                  c.setAddMenuOpen(false);
                  c.addKind(kind);
                }}
              />
            )}
          </div>

          {/* FLO-FR-34 / FLO-FR-35: the action control, over the Flow file. */}
          <div className="flow-actionhost">
            <ActionControl
              discussions={c.control.discussions}
              artifactType="flow"
              itemNoun="flow"
              ownerLabel={flowId ?? undefined}
              missing={!flowId}
              dismissSignal={c.dismissAction}
              onSurfaceOpened={() => c.setAddMenuOpen(false)}
            />
          </div>

          {/* FLO-FR-22: the view controls. They change no document state. */}
          <div className="flow-overlay flow-overlay--view">
            <button type="button" className="flow-overlay__btn" aria-label="Zoom out" title="Zoom out" onClick={() => c.zoomBy(-ZOOM_STEP)}>
              <Icon.Minus size={14} />
            </button>
            <span className="flow-overlay__zoom" data-testid="flow-zoom">
              {Math.round(view.scale * 100)}%
            </span>
            <button type="button" className="flow-overlay__btn" aria-label="Zoom in" title="Zoom in" onClick={() => c.zoomBy(ZOOM_STEP)}>
              <Icon.Plus size={14} />
            </button>
            <button type="button" className="flow-overlay__btn" aria-label="Fit" title="Fit" onClick={c.onFit}>
              <Icon.Fit size={14} />
            </button>
          </div>

          {/* FLO-FR-23: a readout, not a budget. */}
          <div className="flow-status" data-testid="flow-status">
            {readout}
          </div>

          {/* FLO-FR-45: removing a container that holds anything asks first. */}
          {a.pendingRemoval && (
            <div className="flow-dialog" role="alertdialog" aria-label="Remove item" data-testid="flow-remove-confirm">
              <p>
                Remove <strong>{a.pendingRemoval.name}</strong> and the {a.pendingRemoval.count}{" "}
                {a.pendingRemoval.count === 1 ? "item" : "items"} under it?
              </p>
              <div className="flow-dialog__actions">
                <button type="button" className="flow-btn" onClick={() => a.setPendingRemoval(null)}>
                  Cancel
                </button>
                <button type="button" className="flow-btn flow-btn--danger-solid" onClick={a.confirmRemove}>
                  Remove
                </button>
              </div>
            </div>
          )}

          {/* FLO-FR-HGIY: a kind change that removes a draft link or a footprint asks first. */}
          {a.pendingKind && (
            <div className="flow-dialog" role="alertdialog" aria-label="Change kind" data-testid="flow-kind-confirm">
              <p>
                Change <strong>{a.pendingKind.name}</strong> to{" "}
                {kindLabel(a.pendingKind.kind).toLowerCase()}? This removes its{" "}
                {a.pendingKind.drops
                  .map((d) => (d === "draftId" ? "draft link" : "footprint"))
                  .join(" and its ")}
                .
              </p>
              <div className="flow-dialog__actions">
                <button type="button" className="flow-btn" onClick={() => a.setPendingKind(null)}>
                  Cancel
                </button>
                <button type="button" className="flow-btn flow-btn--primary" onClick={a.confirmKind}>
                  Change kind
                </button>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** FLO-FR-07: the five kinds, each with its glyph, in the action menu's treatment. */
function AddMenu({ onPick }: { onPick: (kind: (typeof ITEM_KINDS)[number]) => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    ref.current?.querySelector("button")?.focus();
  }, []);
  return (
    <div ref={ref} className="draft-actions__menu flow-addmenu__list" role="menu" data-testid="flow-add-menu">
      {ITEM_KINDS.map((kind) => (
        <button key={kind} type="button" role="menuitem" className="draft-actions__item" onClick={() => onPick(kind)}>
          <span className="flow-addmenu__glyph" aria-hidden="true">
            {KIND_GLYPHS[kind]}
          </span>
          {kindLabel(kind)}
        </button>
      ))}
    </div>
  );
}

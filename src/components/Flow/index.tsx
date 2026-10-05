import {
  addElementArtifact,
  findElement,
  moveElement,
  removeEdge,
  removeElementArtifact,
  removeLoop,
  removeNode,
  renameEdge,
  renameLoop,
  renameNode,
  setFlowDescription,
  setFlowName,
  setLoopMaxPasses,
  setNodePrompt,
  minLoopSize,
  LOOP_HEADER_H,
} from "../../state/flowDocument";
import type { FlowViolation } from "../../types";
import { ActionControl } from "../ActionControl";
import { Icon } from "../icons";
import { ZOOM_STEP, edgeCurve, resolveReferences } from "./geometry";
import { FlowLoopView, FlowNodeView } from "./nodes";
import { useFlowCanvas, type FlowCanvasProps } from "./useFlowCanvas";

/** FLO-FR-05 / FLO-FR-47: the violations a report named, as a list. */
function ViolationList({ violations }: { violations: readonly FlowViolation[] }) {
  if (violations.length === 0) return null;
  return (
    <ul className="flow-violations" data-testid="flow-violations">
      {violations.map((v, i) => (
        <li key={i} data-code={v.code}>
          {(v.elementId ?? v.edgeId) && (
            <code className="flow-violations__at">{v.elementId ?? v.edgeId}</code>
          )}{" "}
          {v.message}
        </li>
      ))}
    </ul>
  );
}

/**
 * `FLO-flow.md`: the Flow canvas.
 *
 * The graph lives in the shell's `FlowSessionStore`, not here (FLO-FR-30): only
 * the active tab's surface is mounted, so a graph held in component state would
 * be lost the moment the user switched tabs. Selection, pan, and zoom do stay
 * local — FLO-FR-22 makes them view state rather than document state, so they
 * are neither serialized nor persisted and a reopened Flow presents a default
 * view of the saved layout.
 *
 * Elements are laid out flat in canvas coordinates rather than nested in the
 * DOM: the document stores a position relative to whatever holds the element
 * (FLO-FR-21), and `absolutePosition` resolves it once here. Loops render behind
 * their members, which is what makes a container read as the region it is
 * without taking the members out of the one coordinate space edges are drawn in.
 *
 * Every mutation goes through `edit`, which is what raises the Flow's unsaved
 * state (FLO-FR-26). A drag is the exception to the "every move is an edit"
 * reading: it commits once on release rather than on each pointer move, so
 * dragging an element across the canvas is one edit and one dirty transition.
 */
export function FlowCanvas({
  flowId,
  flows,
  onOpenArtifact,
}: FlowCanvasProps) {
  const {
    referenceable,
    treeLoaded,
    byId,
    session,
    doc,
    control,
    selected,
    selectedElement,
    setSelectedElement,
    selectedEdge,
    setSelectedEdge,
    arrowId,
    arrowActiveId,
    onMeasureNode,
    view,
    magnet,
    refusal,
    addMenuOpen,
    setAddMenuOpen,
    dismissAction,
    pendingMove,
    setPendingMove,
    pendingLoopRemoval,
    setPendingLoopRemoval,
    surfaceRef,
    addTriggerRef,
    addMenuRef,
    connectingRef,
    magnetRef,
    edit,
    absOf,
    sizeOf,
    onAddNode,
    onAddLoop,
    openAddMenu,
    closeAddMenu,
    onFit,
    zoomBy,
    onSurfacePointerDown,
    onSurfacePointerMove,
    onElementDragStart,
    onLoopResizeStart,
    endGesture,
    onOpenReferenced,
    onRemoveLoop,
    edgeAnchors,
    nameMissing,
    connectionPreview,
    orderedLoops,
    counts,
    beginConnection,
  } = useFlowCanvas({ flowId, flows, onOpenArtifact });

  return (
    <div className="flow-surface">
      {/* FLO-FR-25: what this Flow is and what it does, at the top of the tab.
          The name is required — marked, never enforced by refusing to save. */}
      <div className="flow-header">
        <input
          className="flow-header__name"
          aria-label="Flow name"
          placeholder="Flow name"
          required
          aria-required="true"
          aria-invalid={nameMissing}
          aria-describedby={nameMissing ? "flow-name-required" : undefined}
          data-missing={nameMissing}
          value={doc?.name ?? ""}
          disabled={!doc}
          onChange={(e) => edit((d) => setFlowName(d, e.target.value))}
        />
        <input
          className="flow-header__description"
          aria-label="Flow description"
          placeholder="Description (optional)"
          value={doc?.description ?? ""}
          disabled={!doc}
          onChange={(e) => edit((d) => setFlowDescription(d, e.target.value))}
        />
        {nameMissing && (
          <span
            className="flow-header__required"
            id="flow-name-required"
            data-testid="flow-name-required"
          >
            Name required
          </span>
        )}
        {/* FLO-FR-26: the canvas's own unsaved marker, alongside the tab's. */}
        {session?.dirty && (
          <span className="t-ui-xs t-muted" data-testid="flow-dirty">
            unsaved
          </span>
        )}
      </div>

      {/* FLO-FR-16 / FLO-FR-17 / FLO-FR-40: the immediate feedback a refused
          connection produces. */}
      {refusal && (
        <div className="flow-banner" role="status">
          {refusal}
        </div>
      )}

      {/* A failed load or write, surfaced on the tab showing the Flow — which is
          what makes a close refuse until a write succeeds (TAB-FR-13).
          FLO-FR-47: a write the validation refused lists every violation it
          named, against the element or edge each sits on. */}
      {session?.error && (
        <div className="flow-banner flow-banner--error" role="alert">
          {session.violations.length > 0 ? (
            <>
              <strong>This Flow was not written.</strong>
              <ViolationList violations={session.violations} />
            </>
          ) : (
            session.error
          )}
        </div>
      )}

      {/* FLO-FR-05 / FLO-FR-46: a body the backend did not call a Flow. The
          canvas renders no graph and offers no editing, and the tab performs no
          write, so a file the editor cannot read is never replaced by an empty
          graph. */}
      {session?.parseError ? (
        <div className="flow-error" role="alert">
          <strong>This file could not be read as a Flow.</strong>
          {session.violations.length > 0 ? (
            <ViolationList violations={session.violations} />
          ) : (
            <span>{session.parseError}</span>
          )}
        </div>
      ) : !flowId ? (
        <div className="flow-error">No Flow is open.</div>
      ) : !session?.loaded ? (
        <div className="flow-error">Loading…</div>
      ) : (
        <div
          className="flow"
          ref={surfaceRef}
          onPointerDown={onSurfacePointerDown}
          onPointerMove={onSurfacePointerMove}
          onPointerUp={() => endGesture()}
          onPointerCancel={() => endGesture({ abandon: true })}
        >
          <div
            className="flow-viewport"
            style={{
              transform: `translate(${view.x}px, ${view.y}px) scale(${view.scale})`,
              transformOrigin: "0 0",
            }}
          >
            {/* FLO-FR-38: the containers, behind what they hold. */}
            {orderedLoops.map((loop) => (
              <FlowLoopView
                key={loop.id}
                loop={loop}
                at={absOf(loop.id)}
                size={sizeOf(loop)}
                references={resolveReferences(loop, byId, treeLoaded)}
                artifacts={referenceable}
                selected={selectedElement === loop.id}
                magnet={magnet === loop.id}
                onDragStart={(e) => onElementDragStart(loop.id, e)}
                onResizeStart={(e) => onLoopResizeStart(loop, e)}
                onConnectStart={(e) => beginConnection(loop.id, e)}
                onConnectEnd={() => {
                  if (connectingRef.current) magnetRef.current = loop.id;
                }}
                onRename={(name) => edit((d) => renameLoop(d, loop.id, name))}
                onAddArtifact={(artifactId) =>
                  edit((d) => addElementArtifact(d, loop.id, artifactId))
                }
                onRemoveArtifact={(artifactId) =>
                  edit((d) => removeElementArtifact(d, loop.id, artifactId))
                }
                onOpenArtifact={onOpenReferenced}
                onSetMaxPasses={(value) =>
                  edit((d) => setLoopMaxPasses(d, loop.id, value))
                }
                onRemove={() => onRemoveLoop(loop.id)}
              />
            ))}

            <svg
              className="flow-edges"
              width="100%"
              height="100%"
              aria-hidden="true"
            >
              {/* FLO-FR-48: the arrowhead every edge arrives with. `orient`
                  turns it to the curve's own tangent at the vertex it sits on,
                  so a head follows the bend rather than the straight line
                  between the two elements; `refX` at the tip puts that tip on
                  the anchor the edge lands at. Sized in stroke widths, so a
                  selected edge's heavier stroke carries a head to match it —
                  the emphasis is the edge's and the head is part of it. */}
              <defs>
                {[
                  { id: arrowId, fill: "var(--border-3)" },
                  { id: arrowActiveId, fill: "var(--accent)" },
                ].map((m) => (
                  <marker
                    key={m.id}
                    id={m.id}
                    viewBox="0 0 10 10"
                    refX="10"
                    refY="5"
                    markerWidth="5"
                    markerHeight="5"
                    orient="auto"
                  >
                    <path d="M 0 0 L 10 5 L 0 10 z" fill={m.fill} />
                  </marker>
                ))}
              </defs>

              {(doc?.edges ?? []).map((e) => {
                const a = edgeAnchors(e);
                if (!a) return null;
                const active = selectedEdge === e.id;
                return (
                  <path
                    key={e.id}
                    data-testid={`flow-edge-path-${e.id}`}
                    d={edgeCurve(a.ax, a.ay, a.bx, a.by, a.faceA, a.faceB)}
                    fill="none"
                    stroke={active ? "var(--accent)" : "var(--border-3)"}
                    strokeWidth={active ? 2 : 1.4}
                    strokeLinecap="round"
                    // FLO-FR-48: at the target end and nowhere else — which end
                    // carries it is the whole of what tells A→B from B→A.
                    markerEnd={`url(#${active ? arrowActiveId : arrowId})`}
                  />
                );
              })}

              {/* FLO-FR-32 / FLO-FR-33: the connection being drawn, from its
                  source to the pointer — or to the element it has snapped to,
                  which is what shows the author where releasing would land it.
                  It carries the head of a finished edge at its free end, so
                  which way the connection would run is part of what the author
                  is shown before releasing rather than after. */}
              {connectionPreview && (
                <path
                  data-testid="flow-connection"
                  data-magnet={connectionPreview.snapped}
                  d={connectionPreview.d}
                  fill="none"
                  stroke="var(--accent)"
                  strokeWidth={connectionPreview.snapped ? 2 : 1.4}
                  strokeDasharray={connectionPreview.snapped ? undefined : "5 4"}
                  strokeLinecap="round"
                  markerEnd={`url(#${arrowActiveId})`}
                />
              )}
            </svg>

            {/* FLO-FR-19: an edge's label, rendered along the edge and renamed
                in place. An unlabelled edge still carries its marker, so it can
                be selected, labelled, and removed like any other. */}
            {(doc?.edges ?? []).map((e) => {
              const a = edgeAnchors(e);
              if (!a || !doc) return null;
              const active = selectedEdge === e.id;
              const nameOf = (id: string) => findElement(doc, id)?.name ?? id;
              // FLO-FR-19: `A→B` and `B→A` are two edges (FLO-FR-17) whose
              // midpoints coincide, so their labels would land on the same
              // point and one would bury the other's affordance entirely. Each
              // is nudged perpendicular to the edge, deterministically by
              // direction, so both stay legible and both stay clickable.
              const reciprocal = doc.edges.some(
                (o) => o.from === e.to && o.to === e.from,
              );
              const dx = a.bx - a.ax;
              const dy = a.by - a.ay;
              const length = Math.hypot(dx, dy) || 1;
              // One constant, not one per direction: `(dx, dy)` already points
              // the opposite way on the reverse edge, so the perpendicular it
              // builds flips with it. A sign that flipped as well would cancel
              // that out and land both labels on the same point.
              const nudge = reciprocal ? 14 : 0;
              return (
                <div
                  key={e.id}
                  className="flow-edge-label"
                  data-testid={`flow-edge-${e.id}`}
                  data-edge-id={e.id}
                  data-selected={active}
                  style={{
                    left: (a.ax + a.bx) / 2 + (-dy / length) * nudge,
                    top: (a.ay + a.by) / 2 + (dx / length) * nudge,
                  }}
                >
                  {active ? (
                    <>
                      <input
                        className="flow-edge-label__input"
                        aria-label="Edge label"
                        placeholder="Label"
                        value={e.label ?? ""}
                        onChange={(ev) =>
                          edit((d) => renameEdge(d, e.id, ev.target.value))
                        }
                      />
                      {/* FLO-FR-20: removes only this edge; both endpoint
                          elements remain with their other edges intact. */}
                      <button
                        className="btn btn--ghost btn--icon btn--sm"
                        aria-label="Remove edge"
                        onClick={() => {
                          edit((d) => removeEdge(d, e.id));
                          setSelectedEdge(null);
                        }}
                      >
                        <Icon.X size={11} />
                      </button>
                    </>
                  ) : (
                    <span
                      className="flow-edge-label__text"
                      aria-label={`Edge ${nameOf(e.from)} to ${nameOf(e.to)}`}
                    >
                      {e.label ?? "＋"}
                    </span>
                  )}
                </div>
              );
            })}

            {(doc?.nodes ?? []).map((n) => (
              <FlowNodeView
                key={n.id}
                node={n}
                at={absOf(n.id)}
                references={resolveReferences(n, byId, treeLoaded)}
                artifacts={referenceable}
                selected={selectedElement === n.id}
                magnet={magnet === n.id}
                onMeasure={onMeasureNode}
                onDragStart={(e) => onElementDragStart(n.id, e)}
                onConnectStart={(e) => beginConnection(n.id, e)}
                // A release landing on an element connects to that element
                // whatever the magnet had chosen — including when the rules
                // refuse it, which is how the author finds out why (FLO-FR-16,
                // FLO-FR-17, FLO-FR-40).
                onConnectEnd={() => {
                  if (connectingRef.current) magnetRef.current = n.id;
                }}
                onRename={(name) => edit((d) => renameNode(d, n.id, name))}
                onAddArtifact={(artifactId) =>
                  edit((d) => addElementArtifact(d, n.id, artifactId))
                }
                onRemoveArtifact={(artifactId) =>
                  edit((d) => removeElementArtifact(d, n.id, artifactId))
                }
                onSetPrompt={(prompt) =>
                  edit((d) => setNodePrompt(d, n.id, prompt))
                }
                onOpenArtifact={onOpenReferenced}
                onRemove={() => {
                  edit((d) => removeNode(d, n.id));
                  setSelectedElement(null);
                }}
              />
            ))}
          </div>

          {/* FLO-FR-07: adding to the graph is one affordance on the canvas
              itself, at the corner opposite the view controls, that expands into
              a menu of what can be added. */}
          <div
            className="flow-addmenu"
            // The wrapper holds both the trigger and the list, so Escape is
            // caught wherever activation left focus (FLO-FR-07).
            onKeyDown={(e) => {
              if (e.key === "Escape" && addMenuOpen) {
                e.stopPropagation();
                closeAddMenu({ restoreFocus: true });
              }
            }}
          >
            <button
              ref={addTriggerRef}
              className="flow-overlay flow-overlay--add"
              aria-label="Add to flow"
              title="Add to flow"
              aria-haspopup="menu"
              aria-expanded={addMenuOpen}
              onClick={() => (addMenuOpen ? closeAddMenu() : openAddMenu())}
            >
              <Icon.Plus size={14} />
            </button>
            {/* FLO-FR-07: the same treatment the action control's menu takes
                (ACT-FR-05) — named entries carrying a glyph apiece, stacked
                beside the affordance that opened them — so the two menus a Flow
                tab can open read as one kind of surface rather than two. */}
            {addMenuOpen && (
              <div
                ref={addMenuRef}
                className="draft-actions__menu flow-addmenu__list"
                role="menu"
                data-testid="flow-add-menu"
              >
                <button
                  role="menuitem"
                  className="draft-actions__item"
                  onClick={() => {
                    // FLO-FR-07: focus goes back to the affordance that opened
                    // the menu rather than to the document body — a keyboard
                    // author who added a node is still on the canvas's add
                    // control, ready to add the next one.
                    closeAddMenu({ restoreFocus: true });
                    onAddNode();
                  }}
                >
                  <Icon.Node size={13} /> Node
                </button>
                <button
                  role="menuitem"
                  className="draft-actions__item"
                  onClick={() => {
                    closeAddMenu({ restoreFocus: true });
                    onAddLoop();
                  }}
                >
                  <Icon.Refresh size={13} /> Loop
                </button>
              </div>
            )}
          </div>

          {/* FLO-FR-34 / FLO-FR-35: the one control every tab carries, over the
              one action a Flow affords. It sits leading of the view controls,
              which keep the trailing end of that line, and it covers none of
              them. A Flow tab lends the comment rail no margin, so its
              discussions are read in the floating panel this control opens. */}
          <ActionControl
            discussions={control.discussions}
            artifactType="flow"
            itemNoun="flow"
            ownerLabel={flowId ?? undefined}
            /* FLO-FR-36: a discussion targets the Flow file, not an element. */
            missing={!flowId}
            /* ACT-FR-12: the add menu and this control are two of the tab's
               transient surfaces, and at most one of them is ever open. */
            dismissSignal={dismissAction}
            onSurfaceOpened={() => setAddMenuOpen(false)}
          />

          {/* FLO-FR-22: the view controls, on the canvas they act on. They
              change no document state, so they sit apart from the Flow's own
              fields entirely. */}
          <div className="flow-overlay flow-overlay--view">
            <button
              className="flow-overlay__btn"
              aria-label="Zoom out"
              title="Zoom out"
              onClick={() => zoomBy(-ZOOM_STEP)}
            >
              <Icon.Minimize size={14} />
            </button>
            <span className="flow-overlay__zoom t-ui-xs" data-testid="flow-zoom">
              {Math.round(view.scale * 100)}%
            </span>
            <button
              className="flow-overlay__btn"
              aria-label="Zoom in"
              title="Zoom in"
              onClick={() => zoomBy(ZOOM_STEP)}
            >
              <Icon.Plus size={14} />
            </button>
            <button
              className="flow-overlay__btn"
              aria-label="Fit"
              title="Fit"
              onClick={onFit}
            >
              <Icon.Fit size={14} />
            </button>
          </div>

          {/* FLO-FR-23: a Flow is variable-size and nothing here caps it, so the
              counts are a readout rather than a budget. */}
          <div className="flow-status t-ui-xs t-muted" data-testid="flow-status">
            {counts}
            {selected ? ` · selected: ${selected.name}` : ""}
          </div>

          {/* FLO-FR-43: the move costs connections, so the author is asked
              before it lands. Cancelling moves nothing and touches no edge. */}
          {pendingMove && (
            <div className="flow-dialog" role="alertdialog" data-testid="flow-move-confirm">
              <p>
                Moving <strong>{pendingMove.name}</strong> to another container
                removes {pendingMove.severed.length} connection
                {pendingMove.severed.length === 1 ? "" : "s"}.
              </p>
              <div className="flow-dialog__actions">
                <button
                  className="btn btn--sm"
                  onClick={() => setPendingMove(null)}
                >
                  Cancel
                </button>
                <button
                  className="btn btn--sm btn--primary"
                  onClick={() => {
                    const move = pendingMove;
                    setPendingMove(null);
                    edit((d) =>
                      moveElement(d, move.elementId, move.absolute, move.newParentId),
                    );
                  }}
                >
                  Move and remove
                </button>
              </div>
            </div>
          )}

          {/* FLO-FR-45: removing a loop that holds anything asks which the
              author means. */}
          {pendingLoopRemoval && (
            <div
              className="flow-dialog"
              role="alertdialog"
              data-testid="flow-remove-loop-confirm"
            >
              <p>
                This loop holds other elements. Remove them with it, or release
                them onto the canvas?
              </p>
              <div className="flow-dialog__actions">
                <button
                  className="btn btn--sm"
                  onClick={() => setPendingLoopRemoval(null)}
                >
                  Cancel
                </button>
                <button
                  className="btn btn--sm"
                  onClick={() => {
                    const id = pendingLoopRemoval;
                    setPendingLoopRemoval(null);
                    setSelectedElement(null);
                    edit((d) => removeLoop(d, id, "release"));
                  }}
                >
                  Release contents
                </button>
                <button
                  className="btn btn--sm btn--danger"
                  onClick={() => {
                    const id = pendingLoopRemoval;
                    setPendingLoopRemoval(null);
                    setSelectedElement(null);
                    edit((d) => removeLoop(d, id, "cascade"));
                  }}
                >
                  Remove contents
                </button>
              </div>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/** Re-exported so the canvas and the document rule agree on a loop's floor. */
export { minLoopSize, LOOP_HEADER_H };

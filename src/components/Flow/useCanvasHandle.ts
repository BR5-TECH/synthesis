import { useEffect, useImperativeHandle, useState, type Ref } from "react";

import { logInfo, logWarn } from "../../logging";
import {
  addItem,
  addItemArtifact,
  findItem,
  renameItem,
  type FlowDocument,
} from "../../state/flowDocument";
import type { FlushResult } from "../../state/editSessions";
import { isEditHeld, waitEditGate } from "../../state/mainBranchWrite";
import type { FlowCanvasState } from "./useFlowCanvas";

/** MDL-FR-EEDO: a task to add at the top level. */
export interface FlowTaskRequest {
  name: string;
  /** Artifact ids (ASC-FR-13), each referenced once, in this order. */
  references: readonly string[];
}

/**
 * What a host can ask of a mounted canvas, through its `ref`
 * (`MDL-map-delivery-lens.md`, `SDL-specification-delivery-links.md`).
 */
export interface FlowCanvasHandle {
  /**
   * FLO-FR-27 / MDL-FR-JRGK: write the pending changes of a file binding now.
   * A refused write resolves `ok: false` (FLO-FR-47), and the host stops the
   * publish. A plan binding writes nothing and resolves `ok: true`.
   */
  flush(): Promise<FlushResult>;
  /**
   * MDL-FR-JIKK / SDL-FR-MCGI: select the item, expand each collapsed container
   * that holds it, and centre it. A request made before the plan has loaded
   * applies when it loads; an id the plan does not hold changes nothing.
   */
  reveal(itemId: string): void;
  /**
   * MDL-FR-EEDO: add a `task` at the top level with the name and references
   * given, and select it. Resolves the new item's id, or null when nothing was
   * added. In a plan binding this is the `create_item` edit, then the
   * `set_references` edit once the first answer names the item.
   */
  addTask(request: FlowTaskRequest): Promise<string | null>;
  /** MDL-FR-QAJN: store the empty layout; the canvas lays the plan out again. */
  resetLayout(): void;
}

/** The document with a new top-level task named `name`, and its id. */
function withTask(doc: FlowDocument, name: string): { doc: FlowDocument; id: string } | null {
  const added = addItem(doc, "task");
  if (!added.ok) return null;
  return { doc: renameItem(added.doc, added.itemId, name), id: added.itemId };
}

function withReferences(doc: FlowDocument, id: string, references: readonly string[]): FlowDocument {
  return references.reduce((d, r) => addItemArtifact(d, id, r), doc);
}

/** The host API of the canvas (`FlowCanvasHandle`). */
export function useCanvasHandle(c: FlowCanvasState, ref: Ref<FlowCanvasHandle> | undefined) {
  const [revealing, setRevealing] = useState<string | null>(null);
  const { source } = c;
  const ready = c.doc !== null && c.layoutReady;

  // MDL-FR-JIKK: a request that came before the plan applies once it renders.
  useEffect(() => {
    if (!revealing || !ready) return;
    setRevealing(null);
    if (!c.revealItem(revealing)) {
      logWarn(["frontend"], "plan item to show is not in the plan", { mode: source.mode });
    }
  });

  const addTask = async ({ name, references }: FlowTaskRequest): Promise<string | null> => {
    const doc = c.doc;
    if (!doc || c.inert) return null;
    if (source.mode === "file") {
      let first: { doc: FlowDocument; id: string } | null = null;
      const add = () =>
        c.edit((d) => {
          const t = withTask(d, name);
          first = t;
          return t ? withReferences(t.doc, t.id, references) : d;
        });
      let changed = add();
      // EDT-FR-UPNU: the gate held the change: the branch read is under way,
      // or the author must confirm a write to a main branch. The task goes in
      // once the gate lets it, and not at all after a cancel.
      if (!changed && isEditHeld()) {
        logInfo(["frontend"], "plan task waits for the edit gate", { mode: "file" });
        if (!(await waitEditGate())) {
          logWarn(["frontend"], "plan task not added: the write was not confirmed", { mode: "file" });
          return null;
        }
        changed = add();
      }
      const added = first as { id: string } | null;
      if (!changed || !added) return null;
      c.setSelection([added.id]);
      c.setSelectedEdge(null);
      logInfo(["frontend"], "plan task added", { mode: "file", references: references.length });
      return added.id;
    }
    const answer = source.editAndWait((d) => withTask(d, name)?.doc ?? d);
    const created = answer ? await answer : null;
    if (!created?.ok) {
      logWarn(["frontend"], "plan task not added", { mode: "plan" });
      return null;
    }
    const known = new Set(doc.items.map((i) => i.id));
    const fresh = created.document.items.filter(
      (i) => !known.has(i.id) && i.kind === "task" && i.parentId === undefined && i.name === name,
    );
    const id = fresh[fresh.length - 1]?.id;
    if (!id) {
      logWarn(["frontend"], "plan task answer names no new task", { mode: "plan" });
      return null;
    }
    if (references.length > 0 && findItem(created.document, id)) {
      void source.editAndWait((d) => withReferences(d, id, references), created.document);
    }
    c.setSelection([id]);
    c.setSelectedEdge(null);
    logInfo(["frontend"], "plan task added", { mode: "plan", references: references.length });
    return id;
  };

  useImperativeHandle(ref, (): FlowCanvasHandle => ({
    flush: async () => {
      const binding = c.binding;
      if (binding.kind === "file" && binding.flowId) {
        return binding.flows.flush(binding.flowId);
      }
      return { ok: true };
    },
    reveal: (itemId: string) => setRevealing(itemId),
    addTask,
    resetLayout: c.resetLayout,
  }));
}

import { useState } from "react";

import * as api from "../../api";
import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import { logInfo, logWarn } from "../../logging";
import {
  addFlowReference,
  addFootprintEntry,
  addItemArtifact,
  docOrUnchanged,
  findItem,
  isContainerKind,
  kindChangeDrops,
  removalScope,
  removeEdge,
  removeFlowReference,
  removeFootprintEntry,
  removeItem,
  removeItemArtifact,
  renameItem,
  reverseEdge,
  setDue,
  setEdgeType,
  setFlowDescription,
  setFlowName,
  setItemDescription,
  setItemDraft,
  setItemKind,
  type FlowEdgeType,
  type FlowItemKind,
  type KindDroppedField,
} from "../../state/flowDocument";
import { showToast } from "../../state/toasts";
import { errorText } from "./geometry";
import type { CreatedDraftOutcome, FlowCanvasProps, FlowCanvasState } from "./useFlowCanvas";

/** FLO-FR-45: a removal that waits for the author's answer. */
export interface PendingRemoval {
  itemId: string;
  name: string;
  count: number;
}

/**
 * FLO-FR-NRBV: the error of a draft that was created while the plan could not
 * take its link, for example because the write of the plan is held.
 */
export function unlinkedDraftText(name: string): string {
  return `The change ${name} was created, but this item does not link it: the plan could not be edited. Use Link change to link it.`;
}

/** FLO-FR-HGIY: a kind change that would remove a draft link or a footprint. */
export interface PendingKind {
  itemId: string;
  name: string;
  kind: FlowItemKind;
  drops: KindDroppedField[];
}

/**
 * The edits and the backend calls of the canvas's controls: names, kinds,
 * descriptions, references, drafts, footprints, due dates, edges, removal, and
 * the tracker link. Every rule is the document module's.
 */
export function useItemActions(c: FlowCanvasState, props: FlowCanvasProps) {
  const { onOpenArtifact, onCreateDraft, onOpenDraft, onOpenDraftById } = props;
  const { doc, edit, tryEdit, inert } = c;
  const flowId = c.source.flowId;
  const [pendingRemoval, setPendingRemoval] = useState<PendingRemoval | null>(null);
  const [pendingKind, setPendingKind] = useState<PendingKind | null>(null);
  /** FLO-FR-NRBV: the error of a refused draft creation, by item. */
  const [draftErrors, setDraftErrors] = useState<Record<string, string>>({});
  const [creating, setCreating] = useState<string | null>(null);

  const rename = (id: string, name: string) => edit((d) => renameItem(d, id, name));
  const describe = (id: string, text: string) => edit((d) => setItemDescription(d, id, text));
  const addReference = (id: string, artifactId: string) =>
    edit((d) => addItemArtifact(d, id, artifactId));
  const removeReference = (id: string, artifactId: string) =>
    edit((d) => removeItemArtifact(d, id, artifactId));
  const setFlowNameTo = (name: string) => edit((d) => setFlowName(d, name));
  const setFlowDescriptionTo = (text: string) => edit((d) => setFlowDescription(d, text));
  const addFlowRef = (artifactId: string) => edit((d) => addFlowReference(d, artifactId));
  const removeFlowRef = (artifactId: string) => edit((d) => removeFlowReference(d, artifactId));
  const setDueDate = (id: string, due: string) => tryEdit((d) => setDue(d, id, due), "kind");
  const addFootprint = (id: string, path: string) =>
    tryEdit((d) => addFootprintEntry(d, id, path), "link");
  const removeFootprint = (id: string, path: string) =>
    edit((d) => removeFootprintEntry(d, id, path));

  /**
   * FLO-FR-HGIY: change an item's kind. A change that removes a draft link or a
   * footprint asks first.
   */
  const requestKind = (id: string, kind: FlowItemKind) => {
    const item = doc ? findItem(doc, id) : undefined;
    if (!doc || !item || inert) return;
    const drops = kindChangeDrops(doc, id, kind).filter((f) => f !== "due");
    if (drops.length > 0) {
      setPendingKind({ itemId: id, name: item.name, kind, drops });
      return;
    }
    tryEdit((d) => setItemKind(d, id, kind), "kind", kind);
  };
  const confirmKind = () => {
    const p = pendingKind;
    setPendingKind(null);
    if (p) tryEdit((d) => setItemKind(d, p.itemId, p.kind), "kind", p.kind);
  };

  /**
   * FLO-FR-08 / FLO-FR-45: remove an item. A container that holds anything
   * asks first, naming how many items go with it. In a plan binding the host
   * asks how to remove, and removes (MDL-FR-XQGD).
   */
  const requestRemove = (id: string) => {
    const item = doc ? findItem(doc, id) : undefined;
    if (!doc || !item || inert) return;
    const { itemCount } = removalScope(doc, id);
    if (c.source.requestRemoval) {
      c.source.requestRemoval({ itemId: id, name: item.name, kind: item.kind, count: itemCount });
      return;
    }
    if (isContainerKind(item.kind) && itemCount > 0) {
      setPendingRemoval({ itemId: id, name: item.name, count: itemCount });
      return;
    }
    edit((d) => removeItem(d, id));
    c.setSelection([]);
  };
  const confirmRemove = () => {
    const p = pendingRemoval;
    setPendingRemoval(null);
    if (!p) return;
    edit((d) => removeItem(d, p.itemId));
    c.setSelection([]);
  };

  /** FLO-FR-CEVI / FLO-FR-GZCH: an edge's type and direction. */
  const retypeEdge = (edgeId: string, type: FlowEdgeType) =>
    tryEdit((d) => setEdgeType(d, edgeId, type), "retype");
  const reverse = (edgeId: string) => tryEdit((d) => reverseEdge(d, edgeId), "reverse");
  /** FLO-FR-20: remove one edge; both items stay. */
  const dropEdge = (edgeId: string) => {
    edit((d) => removeEdge(d, edgeId));
    c.setSelectedEdge(null);
  };

  /** FLO-FR-12: open a resolved reference in its natural surface. */
  const openReferenced = (artifact: ArtifactOption) => {
    void api
      .openArtifactById(artifact.id)
      .then((opened) =>
        onOpenArtifact?.({
          id: opened.key,
          name: artifact.displayName,
          artifactType: opened.kind === "flow" ? "flow" : artifact.artifactType,
        }),
      )
      .catch((e) => {
        logWarn(["frontend"], "flow reference did not open", {
          artifact: artifact.id,
          code: (e as { code?: string })?.code ?? "",
        });
        // FLO-FR-12: a reference that does not open says why (NTF-FR-TSRL).
        showToast({
          key: `flow-reference:${artifact.id}`,
          level: "Error",
          title: `${artifact.displayName} not opened`,
          body: errorText(e),
          address: null,
        });
      });
  };

  /** FLO-FR-ULEM: open an item's tracker page through `"open tracker link (url)"`. */
  const openTracker = (url: string) => {
    void Promise.resolve()
      .then(() => api.openTrackerLink(url))
      .catch((e) => {
        logWarn(["frontend", "remote"], "tracker link did not open", {
          code: (e as { code?: string })?.code ?? "",
        });
        showToast({
          key: "flow-tracker-link",
          level: "Error",
          title: "Link not opened",
          body: errorText(e),
          address: null,
        });
      });
  };

  /**
   * FLO-FR-NRBV: create a draft named after the item, store its id, and open
   * it in a New Artifact tab. A refused creation stores nothing and shows its
   * error on the item.
   */
  const createDraftFor = async (id: string) => {
    const item = doc ? findItem(doc, id) : undefined;
    if (!item || inert || creating) return;
    setCreating(id);
    setDraftErrors(({ [id]: _drop, ...rest }) => rest);
    let outcome: CreatedDraftOutcome;
    try {
      outcome = onCreateDraft
        ? await onCreateDraft(item.name)
        : await api
            .createDraft({ name: item.name })
            .then((created) => ({
              ok: true as const,
              draft: { id: created.draft.id, name: created.draft.name },
            }));
    } catch (e) {
      outcome = { ok: false, error: errorText(e) };
    }
    setCreating(null);
    if (!outcome.ok) {
      if (outcome.error !== null) {
        const error = `The change was not created. ${outcome.error}`;
        setDraftErrors((prev) => ({ ...prev, [id]: error }));
        logWarn(["frontend"], "flow task draft not created", { flow: flowId ?? "", item: id });
      }
      return;
    }
    const draft = outcome.draft;
    // EDT-FR-UPNU: a write held while the draft was created drops the link.
    // The item then shows why, and no draft opens that no item links.
    if (!edit((d) => docOrUnchanged(d, setItemDraft(d, id, draft.id)))) {
      setDraftErrors((prev) => ({ ...prev, [id]: unlinkedDraftText(draft.name) }));
      logWarn(["frontend"], "flow task draft created but not linked", {
        flow: flowId ?? "",
        item: id,
      });
      return;
    }
    logInfo(["frontend"], "flow task draft created", { flow: flowId ?? "", item: id });
    onOpenDraft?.(draft);
  };

  /** FLO-FR-QZUR: open a linked draft through `"open draft (id)"`. */
  const openDraft = (draftId: string) => {
    if (onOpenDraftById) {
      onOpenDraftById(draftId);
      return;
    }
    void Promise.resolve()
      .then(() => api.openDraft(draftId))
      .catch((e) => {
        logWarn(["frontend"], "flow draft did not open", {
          draft: draftId,
          code: (e as { code?: string })?.code ?? "",
        });
        // A draft that does not open says why (NTF-FR-TSRL).
        showToast({
          key: `flow-draft:${draftId}`,
          level: "Error",
          title: "Change not opened",
          body: errorText(e),
          address: null,
        });
      });
  };

  /** FLO-FR-RMVR: link a draft, or unlink with null. The draft is not changed. */
  const linkDraft = (id: string, draftId: string | null) =>
    tryEdit((d) => setItemDraft(d, id, draftId), "link");

  return {
    rename,
    describe,
    addReference,
    removeReference,
    setFlowNameTo,
    setFlowDescriptionTo,
    addFlowRef,
    removeFlowRef,
    setDueDate,
    addFootprint,
    removeFootprint,
    requestKind,
    pendingKind,
    setPendingKind,
    confirmKind,
    requestRemove,
    pendingRemoval,
    setPendingRemoval,
    confirmRemove,
    retypeEdge,
    reverse,
    dropEdge,
    openReferenced,
    openTracker,
    createDraftFor,
    creating,
    draftErrors,
    openDraft,
    linkDraft,
  };
}

export type ItemActions = ReturnType<typeof useItemActions>;

import { useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from "react";

import { logWarn } from "../../logging";
import {
  planEditsBetween,
  touchedItems,
  type FlowDocument,
} from "../../state/flowDocument";
import { FlowLayoutStore } from "../../state/flowLayouts";
import type { FlowSession } from "../../state/flowSessions";
import { flowLayoutScope, type TrackerFacts } from "../../types";
import type { FlowBinding, FlowPlanAnswer, FlowProposalView, FlowRemovalRequest } from "./binding";
import { errorText } from "./geometry";
import { useDeliveryState, type DeliveryView } from "./useDeliveryState";

const noSubscribe = () => () => {};
const noVersion = () => 0;
const NO_PENDING: ReadonlySet<string> = new Set();
const NO_ERRORS: Readonly<Record<string, string>> = {};
const NO_PROPOSAL: FlowProposalView = { marks: new Map(), ghosts: new Map(), reviewed: null };

/** What the canvas reads and changes, whichever binding it runs with. */
export interface FlowSource {
  mode: "file" | "plan";
  /** The Flow file of the file binding. */
  flowId?: string;
  /** The editing session of the file binding. */
  session?: FlowSession;
  /** The plan, or null while it loads or after a body was refused. */
  doc: FlowDocument | null;
  /** The plan has settled: loaded, refused, or failed. */
  loaded: boolean;
  /** DMS-FR-DMWD: the layout scope, or null with no Flow open. */
  scope: string | null;
  layouts: FlowLayoutStore;
  delivery: DeliveryView;
  /** FLO-FR-ULEM: the tracker facts of each item of a plan binding. */
  tracker: ReadonlyMap<string, TrackerFacts>;
  /** FLO-FR-ULEM: the repository of the tracker tags. */
  repository: string | null;
  /** MDL-FR-PUCM: the items with an edit pending, and the errors to show. */
  pending: ReadonlySet<string>;
  itemErrors: Readonly<Record<string, string>>;
  /** DBR-FR-AGIN .. DBR-FR-PMGS: the plan proposals as the board shows them. */
  proposal: FlowProposalView;
  /**
   * FLO-FR-26: apply one edit of the plan. Returns true when the edit changed
   * the plan. In a plan binding it goes to the host as semantic edits.
   */
  edit: (fn: (d: FlowDocument) => FlowDocument) => boolean;
  /**
   * The answer of an edit of a plan binding, or null where the edit changed
   * nothing. `base` is the plan the edit starts from, for an edit that follows
   * an answer the host has not rendered yet; without it, the edit starts from
   * the newest plan. The file binding writes itself and gives null.
   */
  editAndWait: (
    fn: (d: FlowDocument) => FlowDocument,
    base?: FlowDocument,
  ) => Promise<FlowPlanAnswer> | null;
  /** MDL-FR-XQGD: hand a removal to the host. Absent in the file binding. */
  requestRemoval?: (request: FlowRemovalRequest) => void;
}

/**
 * The canvas's view of its binding (`./binding.ts`). The file binding reads
 * the shell's `FlowSessionStore` and asks for the delivery state itself
 * (FLO-FR-STUN). The plan binding reads what the host gives and hands each edit
 * back to it (MDL-FR-XJBV).
 */
export function useFlowSource(binding: FlowBinding): FlowSource {
  const file = binding.kind === "file" ? binding : null;
  const plan = binding.kind === "plan" ? binding : null;
  const flows = file?.flows ?? null;
  const flowId = file?.flowId;
  useSyncExternalStore(flows?.subscribe ?? noSubscribe, flows?.getVersion ?? noVersion);
  const [ownLayouts] = useState(() => new FlowLayoutStore());

  const session = flows && flowId ? flows.get(flowId) : undefined;
  const fileDoc = session?.doc ?? null;
  const delivery = useDeliveryState(fileDoc, file !== null);

  // MDL-FR-PUCM: the host renders the plan an edit gives at once. Until that
  // render, the next edit starts from that plan and not from the older one.
  const latest = useRef<{ seen: FlowDocument | null; next: FlowDocument | null }>({
    seen: null,
    next: null,
  });
  const planDoc = plan?.document ?? null;
  if (latest.current.seen !== planDoc) latest.current = { seen: planDoc, next: null };
  const planRef = useRef(plan);
  planRef.current = plan;

  const tracker = useMemo(
    () => new Map((plan?.tracker ?? []).map((t) => [t.itemId, t])),
    [plan?.tracker],
  );
  const trackerRef = useRef(tracker);
  trackerRef.current = tracker;

  const planDelivery = useMemo<DeliveryView>(
    () => ({
      states: plan?.states ? new Map(plan.states.map((s) => [s.itemId, s])) : null,
      overlaps: plan?.overlaps ? [...plan.overlaps] : null,
    }),
    [plan?.states, plan?.overlaps],
  );

  const editAndWait = useCallback(
    (fn: (d: FlowDocument) => FlowDocument, base?: FlowDocument): Promise<FlowPlanAnswer> | null => {
      if (flows) {
        if (flowId) flows.applyEdit(flowId, fn);
        return null;
      }
      const host = planRef.current;
      const before = base ?? latest.current.next ?? latest.current.seen;
      if (!host || !before) return null;
      const next = fn(before);
      if (next === before) return null;
      const edits = planEditsBetween(
        before,
        next,
        (id) => trackerRef.current.get(id)?.updatedAt ?? "",
      );
      if (edits.length === 0) return null;
      const earlier = latest.current.next;
      latest.current.next = next;
      // MDL-FR-PUCM: a refused edit is rolled back. Where the host did not
      // render another plan since, the next edit starts from the plan before
      // this one, and not from the plan the refusal did not make.
      const rollBack = () => {
        if (latest.current.next === next) latest.current.next = earlier;
      };
      // MDL-FR-PUCM: the host takes the edit now, so it renders at once.
      let answer: Promise<FlowPlanAnswer>;
      try {
        answer = host.onEdit({ edits, next, itemIds: touchedItems(before, next) });
      } catch (e) {
        rollBack();
        answer = Promise.reject(e);
      }
      return Promise.resolve(answer).then(
        (a) => {
          if (!a.ok) rollBack();
          return a;
        },
        (e): FlowPlanAnswer => {
          rollBack();
          logWarn(["frontend"], "plan edit was not taken", {
            kinds: edits.map((x) => x.kind).join(","),
          });
          return { ok: false, error: errorText(e) };
        },
      );
    },
    [flows, flowId],
  );

  const edit = useCallback(
    (fn: (d: FlowDocument) => FlowDocument): boolean => {
      if (flows) return flowId ? flows.applyEdit(flowId, fn) : false;
      return editAndWait(fn) !== null;
    },
    [flows, flowId, editAndWait],
  );

  const scope = file
    ? flowId
      ? flowLayoutScope(flowId)
      : null
    : (plan?.scope ?? null);
  const layouts = file ? file.flows.layouts : (plan?.layouts ?? ownLayouts);
  useEffect(() => {
    if (scope) layouts.ensure(scope);
  }, [scope, layouts]);

  if (plan) {
    return {
      mode: "plan",
      doc: planDoc,
      loaded: planDoc !== null,
      scope,
      layouts,
      delivery: planDelivery,
      tracker,
      repository: plan.repository,
      pending: plan.pending ?? NO_PENDING,
      itemErrors: plan.itemErrors ?? NO_ERRORS,
      proposal: plan.proposal ?? NO_PROPOSAL,
      edit,
      editAndWait,
      requestRemoval: plan.onRemoveRequest,
    };
  }
  return {
    mode: "file",
    ...(flowId !== undefined ? { flowId } : {}),
    ...(session ? { session } : {}),
    doc: fileDoc,
    loaded: session?.loaded ?? false,
    scope,
    layouts,
    delivery,
    tracker,
    repository: null,
    pending: NO_PENDING,
    itemErrors: NO_ERRORS,
    proposal: NO_PROPOSAL,
    edit,
    editAndWait,
  };
}

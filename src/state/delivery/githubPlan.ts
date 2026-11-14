/**
 * The GitHub plan of the Delivery lens (`MDL-map-delivery-lens.md`
 * MDL-FR-XJBV .. MDL-FR-HNLP): the last answer of `"load github delivery plan
 * (refresh)"` and of `"apply github delivery edit (edit)"`, the edits on their
 * way to GitHub, and the errors to show on the items.
 *
 * It lives in the map session and not in the lens, so an edit that is pending
 * when the Map tab unmounts still lands, and the next mount shows the plan at
 * once (SMP-FR-QNUH, the cached plan of the MDL NFR).
 */
import { applyGithubDeliveryEdit, loadGithubDeliveryPlan } from "../../api/delivery";
import { errorLogCode } from "../../errorLogCode";
import { logInfo, logWarn } from "../../logging";
import type { GithubDeliveryEdit, GithubDeliveryPlan } from "../../types/delivery";
import { deliveryErrorMessage } from "../deliveryErrors";
import { parseFlowDocument, removeItem, type FlowDocument } from "../flowDocument";
import { showToast } from "../toasts";

/** The two operations the plan session calls; a test replaces them. */
export interface GithubPlanOps {
  load: (refresh: boolean) => Promise<GithubDeliveryPlan>;
  apply: (edit: GithubDeliveryEdit) => Promise<GithubDeliveryPlan>;
}

const DEFAULT_OPS: GithubPlanOps = {
  load: (refresh) => loadGithubDeliveryPlan(refresh),
  apply: (edit) => applyGithubDeliveryEdit(edit),
};

/** MDL-FR-PUCM: one change of the canvas, as the plan binding hands it over. */
export interface GithubPlanChange {
  edits: GithubDeliveryEdit[];
  next: FlowDocument;
  itemIds: string[];
}

export type GithubPlanAnswer = { ok: true; document: FlowDocument } | { ok: false; error: string };

/** MDL-FR-HNLP: the text an item shows when it changed on GitHub. */
export const CHANGED_ON_GITHUB =
  "This item changed on GitHub, so the change was not made. The plan shows it as GitHub has it now.";

export interface GithubPlanState {
  /** The last answer, or null before the first one. */
  answer: GithubDeliveryPlan | null;
  /** The plan of the last answer. */
  doc: FlowDocument | null;
  /** MDL-FR-PUCM: the plan to render: the last answer with the pending edits applied. */
  shown: FlowDocument | null;
  /** MDL-FR-NRJM: a load is in flight. */
  loading: boolean;
  /** MDL-FR-NRJM: the error text of a load that refused with no plan to show. */
  loadError: string | null;
  /** MDL-FR-PUCM: the items with an edit pending. */
  pending: ReadonlySet<string>;
  /** MDL-FR-PUCM / MDL-FR-HNLP: the error text to show on an item. */
  itemErrors: Readonly<Record<string, string>>;
}

/** A `document` of an answer is a Flow document of version 3, as JSON (GDP contract). */
function toDocument(raw: unknown): FlowDocument | null {
  const parsed = parseFlowDocument(JSON.stringify(raw ?? {}));
  return parsed.ok ? parsed.doc : null;
}

/** The item an edit names, for its `updatedAt` (GDP-FR-GBDF). */
function itemOfEdit(edit: GithubDeliveryEdit): string | null {
  if ("itemId" in edit) return edit.itemId;
  if (edit.kind === "add_edge" || edit.kind === "remove_edge") return edit.to;
  return null;
}

export class GithubPlanSession {
  private state: GithubPlanState = {
    answer: null,
    doc: null,
    shown: null,
    loading: false,
    loadError: null,
    pending: new Set(),
    itemErrors: {},
  };
  private listeners = new Set<() => void>();
  private version = 0;
  /** MDL-FR-LZIE: the order the requests went out in; a newer answer wins. */
  private issued = 0;
  private applied = 0;
  private loadsInFlight = 0;
  /** MDL-FR-XWGP: the changes go to the backend one by one, in order. */
  private tail: Promise<unknown> = Promise.resolve();
  private queued = 0;
  /** MDL-FR-PUCM: the changes not answered yet, in the order they were made. */
  private waiting: GithubPlanChange[] = [];
  private pendingCount = new Map<string, number>();
  private started = false;

  constructor(
    /** DMS-FR-DMWD: the node id of the GitHub Project the plan is of. */
    readonly projectNodeId: string,
    private readonly ops: GithubPlanOps = DEFAULT_OPS,
  ) {}

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  getVersion = (): number => this.version;

  snapshot = (): GithubPlanState => this.state;

  private set(patch: Partial<GithubPlanState>): void {
    this.state = { ...this.state, ...patch };
    this.version += 1;
    for (const listener of [...this.listeners]) listener();
  }

  /**
   * MDL-FR-LZIE: the first render loads with `refresh` false, and then with
   * `refresh` true. A session that has started loads nothing more here.
   */
  start = (): void => {
    if (this.started) return;
    this.started = true;
    // The loading state holds through both loads, so a cache miss of the first
    // does not show the error state before the second answers.
    this.loadsInFlight += 1;
    this.set({ loading: true });
    void this.load(false)
      .then(() => this.load(true))
      .finally(() => {
        this.loadsInFlight -= 1;
        if (this.loadsInFlight === 0) this.set({ loading: false });
      });
  };

  /** MDL-FR-LZIE: true once `start` ran, also when every load of it refused. */
  hasStarted = (): boolean => this.started;

  /**
   * MDL-FR-LZIE / MDL-FR-NRJM: one load. A newer answer replaces an older one.
   *
   * MDL-FR-PUCM: a load asked for while edits are on their way goes out only
   * after the last of them answered. A load sent before an edit wrote could
   * otherwise answer after that edit, and its newer order would replace the
   * plan of the edit with the plan from before it.
   */
  load = async (refresh: boolean): Promise<boolean> => {
    this.loadsInFlight += 1;
    if (!this.state.loading) this.set({ loading: true });
    let seq = 0;
    try {
      while (this.queued > 0) await this.tail;
      seq = ++this.issued;
      const answer = await this.ops.load(refresh);
      if (seq <= this.applied) return true;
      this.accept(answer, seq);
      logInfo(["frontend", "remote"], "github delivery plan loaded", {
        refresh,
        items: this.state.doc?.items.length ?? 0,
        stale: answer.stale,
      });
      return true;
    } catch (e) {
      logWarn(["frontend", "remote"], "github delivery plan load refused", {
        refresh,
        code: errorLogCode(e),
      });
      if (seq > this.applied && this.state.answer === null) {
        this.set({ loadError: deliveryErrorMessage(e) });
      }
      return false;
    } finally {
      this.loadsInFlight -= 1;
      if (this.loadsInFlight === 0) this.set({ loading: false });
    }
  };

  /** MDL-FR-NRJM: Retry of the error state. */
  retry = (): Promise<boolean> => {
    this.set({ loadError: null });
    return this.load(true);
  };

  private accept(answer: GithubDeliveryPlan, seq: number): void {
    const doc = toDocument(answer.document);
    if (!doc) {
      logWarn(["frontend", "remote"], "github delivery plan answer is not a plan", {});
      return;
    }
    this.applied = seq;
    this.set({
      answer,
      doc,
      loadError: null,
      // MDL-FR-PUCM: while edits are pending, the plan they give stays shown.
      ...(this.queued === 0 ? { shown: doc } : {}),
    });
  }

  /**
   * GDP-FR-GBDF: an edit names the `updated_at` the tracker holds now. It is
   * filled when the edit goes out, from the newest answer, so an edit that
   * follows another edit of the same item does not name a value that the first
   * edit made old.
   */
  private fresh(edit: GithubDeliveryEdit): GithubDeliveryEdit {
    if (!("updatedAt" in edit)) return edit;
    const id = itemOfEdit(edit);
    const now = id ? this.state.answer?.tracker.find((t) => t.itemId === id)?.updatedAt : undefined;
    return now ? { ...edit, updatedAt: now } : edit;
  }

  private mark(ids: readonly string[], delta: 1 | -1): void {
    for (const id of ids) {
      const n = (this.pendingCount.get(id) ?? 0) + delta;
      if (n > 0) this.pendingCount.set(id, n);
      else this.pendingCount.delete(id);
    }
  }

  /**
   * MDL-FR-PUCM / MDL-FR-XWGP / MDL-FR-HNLP: render a change at once, send its
   * edits after every change made before it, and answer with the plan or the
   * refusal. `quiet` keeps a refusal off the items, for a caller that shows it
   * in place (MDL-FR-PUKL).
   */
  change = (change: GithubPlanChange, options: { quiet?: boolean } = {}): Promise<GithubPlanAnswer> => {
    this.queued += 1;
    this.waiting.push(change);
    this.mark(change.itemIds, 1);
    const itemErrors = { ...this.state.itemErrors };
    for (const id of change.itemIds) delete itemErrors[id];
    this.set({ shown: change.next, pending: new Set(this.pendingCount.keys()), itemErrors });

    const run = async (): Promise<GithubPlanAnswer> => {
      for (const raw of change.edits) {
        const edit = this.fresh(raw);
        const seq = ++this.issued;
        try {
          const answer = await this.ops.apply(edit);
          if (seq > this.applied) this.accept(answer, seq);
        } catch (e) {
          return this.refused(change, edit, e, options.quiet === true);
        }
      }
      logInfo(["frontend", "remote"], "github delivery edit applied", {
        kinds: change.edits.map((e) => e.kind).join(","),
      });
      return { ok: true, document: this.state.doc ?? change.next };
    };

    const done = this.tail.then(run).finally(() => {
      this.queued -= 1;
      this.waiting = this.waiting.filter((c) => c !== change);
      this.mark(change.itemIds, -1);
      this.set({
        pending: new Set(this.pendingCount.keys()),
        ...(this.queued === 0 ? { shown: this.state.doc } : {}),
      });
    });
    this.tail = done.catch(() => undefined);
    return done;
  };

  private refused(
    change: GithubPlanChange,
    edit: GithubDeliveryEdit,
    e: unknown,
    quiet: boolean,
  ): GithubPlanAnswer {
    const code = errorLogCode(e);
    logWarn(["frontend", "remote"], "github delivery edit refused", { kind: edit.kind, code });
    const text = code === "changed_on_tracker" ? CHANGED_ON_GITHUB : deliveryErrorMessage(e);
    // MDL-FR-PUCM: a refusal restores the plan of the last answer. A change
    // made after it and not answered yet stays shown until it answers.
    const later = this.waiting.filter((c) => c !== change);
    this.set({ shown: later.length > 0 ? later[later.length - 1].next : this.state.doc });
    if (!quiet) {
      const doc = this.state.doc;
      const shownOn = change.itemIds.filter((id) => doc?.items.some((i) => i.id === id));
      if (shownOn.length > 0) {
        const itemErrors = { ...this.state.itemErrors };
        for (const id of shownOn) itemErrors[id] = text;
        this.set({ itemErrors });
      } else {
        // The item the change made is not in the plan to carry the error.
        showToast(
          { key: "github-delivery-edit", level: "Error", title: "Change not made", body: text, address: null },
          { priority: true },
        );
      }
    }
    // MDL-FR-HNLP: the plan is read again, and the edit is not applied again.
    if (code === "changed_on_tracker") void this.load(true);
    // GDP-FR-MSVS: the writes a partial edit made stay, so the plan shows them.
    if (code === "partial_edit") void this.load(false);
    return { ok: false, error: text };
  }

  /** MDL-FR-XQGD: the removal the author chose in the remove dialog. */
  remove = (
    itemId: string,
    mode: "remove_from_board" | "close_not_planned",
  ): Promise<GithubPlanAnswer> | null => {
    const base = this.state.shown;
    if (!base) return null;
    const updatedAt = this.state.answer?.tracker.find((t) => t.itemId === itemId)?.updatedAt ?? "";
    return this.change({
      edits: [{ kind: "remove_item", itemId, mode, updatedAt }],
      next: removeItem(base, itemId),
      itemIds: [itemId],
    });
  };

  /** MDL-FR-PUKL: add an issue that exists on GitHub into a container or the top level. */
  addExistingIssue = (number: number, containerId: string | null): Promise<GithubPlanAnswer> | null => {
    const base = this.state.shown;
    if (!base) return null;
    return this.change(
      {
        edits: [{ kind: "add_existing_issue", number, containerId }],
        next: base,
        itemIds: containerId ? [containerId] : [],
      },
      { quiet: true },
    );
  };

  /** Clear the error an item shows, for example when the author dismisses it. */
  clearItemError = (itemId: string): void => {
    if (!(itemId in this.state.itemErrors)) return;
    const itemErrors = { ...this.state.itemErrors };
    delete itemErrors[itemId];
    this.set({ itemErrors });
  };
}

/**
 * MPV-FR-CZDO: the one shared state of the plan proposals of the Map tab. The
 * summary rows in the discussion, the review sheet over the Delivery canvas,
 * and the marks and the ghost cards of the board all read it.
 *
 * It holds the proposals of each discussion that a view asked for, read with
 * `"list map plan proposals (discussion id)"` and kept current by `"map plan
 * proposals changed"`; the rows whose decision runs; the refusal text of each
 * row; and the open review. It decides nothing on its own: each decision is
 * one backend call (MPV-FR-XYDR).
 */
import * as api from "../../api";
import { onMapPlanProposalsChanged } from "../../events";
import { logInfo, logWarn } from "../../logging";
import { showToast } from "../toasts";
import type { MapPlanProposal, MapPlanRow } from "../../types";
import {
  firstSelection,
  nextPendingKey,
  rowId,
  rowItemId,
  rowStatuses,
  rowTitle,
  type RowStatus,
} from "./derive";

export type Decision = "accept" | "reject" | "reopen";

export interface OpenReview {
  discussionId: string;
  proposalId: string;
  selectedKey: string | null;
}

export interface ReviewSnapshot {
  /** The proposals of a discussion: undefined before a view asks, null until the first read lands. */
  proposals: ReadonlyMap<string, MapPlanProposal[] | null>;
  /** The discussions whose last read failed while no proposal was held. */
  unavailable: ReadonlySet<string>;
  /** `rowId(proposal id, row key)` of each row whose call runs. */
  busy: ReadonlySet<string>;
  /** The refusal text of a row, by `rowId`. */
  refusals: ReadonlyMap<string, string>;
  review: OpenReview | null;
  /** MPV-FR-XAOG: the last change of a status, for assistive technology. */
  announcement: string;
  /** MPV-FR-AAFF: the handle of the agent that recorded each proposal. */
  proposers: ReadonlyMap<string, string>;
  /** DBR-FR-AGIN: the active Map thread, whose proposals mark the board. */
  activeDiscussionId: string | null;
}

/** The text of a typed error of a refused call. */
export function refusalText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object") {
    const { message, code } = e as { message?: unknown; code?: unknown };
    if (typeof message === "string" && message !== "") return message;
    if (typeof code === "string" && code !== "") return code;
  }
  return e instanceof Error ? e.message : "The call was refused.";
}

/** The typed code of a refused call, for the log. Never the message. */
export function refusalCode(e: unknown): string {
  if (e && typeof e === "object") {
    const { code } = e as { code?: unknown };
    if (typeof code === "string") return code;
  }
  return e instanceof Error ? e.name : typeof e;
}

/** MPV-FR-WKVL: what an applied row did, in one sentence. */
function appliedSentence(row: MapPlanRow): string {
  if (row.kind === "new_draft") return "A draft was created.";
  if (row.kind === "create_issue" && row.result?.issueNumber !== undefined) {
    return `Issue #${row.result.issueNumber} was created.`;
  }
  if (row.kind === "modify_issue") return `Issue #${row.fields.number} was changed on GitHub.`;
  return "The row was applied.";
}

export class MapPlanReviewStore {
  private state: ReviewSnapshot = {
    proposals: new Map(),
    unavailable: new Set(),
    busy: new Set(),
    refusals: new Map(),
    review: null,
    announcement: "",
    proposers: new Map(),
    activeDiscussionId: null,
  };
  private version = 0;
  private listeners = new Set<() => void>();
  /** Counts the reads of each discussion, so an older answer never replaces a newer one. */
  private sequence = new Map<string, number>();
  private unlisten: (() => void) | null = null;
  /** Counts the subscriptions to the event; a new one starts when it was released. */
  private listenGeneration = 0;
  private listening = false;
  private holders = 0;
  /** MPV-FR-PBAD: the summary-row button of each proposal, which takes the focus back. */
  private returns = new Map<string, HTMLElement>();

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  };

  getVersion = (): number => this.version;

  snapshot = (): ReviewSnapshot => this.state;

  private set(patch: Partial<ReviewSnapshot>) {
    this.state = { ...this.state, ...patch };
    this.version++;
    for (const listener of [...this.listeners]) listener();
  }

  /**
   * Hold the store for a view that owns it. The last release stops following
   * the event; a later `want` follows it again.
   */
  retain = (): (() => void) => {
    this.holders++;
    this.listen();
    let released = false;
    return () => {
      if (released) return;
      released = true;
      this.holders--;
      if (this.holders === 0) this.stopListening();
    };
  };

  /** Stop following the event. Tests and the last release only. */
  dispose = (): void => {
    this.stopListening();
  };

  private stopListening() {
    this.listenGeneration++;
    this.listening = false;
    this.unlisten?.();
    this.unlisten = null;
  }

  /** MPV-FR-IBSF: make sure the proposals of a discussion are held. */
  want = (discussionId: string): void => {
    this.listen();
    if (this.state.proposals.has(discussionId)) return;
    const proposals = new Map(this.state.proposals);
    proposals.set(discussionId, null);
    this.set({ proposals });
    this.read(discussionId);
  };

  /** MPV-FR-CZDO: follow `"map plan proposals changed"` once for every discussion. */
  private listen() {
    if (this.listening) return;
    this.listening = true;
    const generation = this.listenGeneration;
    onMapPlanProposalsChanged((payload) => {
      if (this.state.proposals.has(payload.discussionId)) this.read(payload.discussionId);
    }).then(
      (fn) => {
        if (generation !== this.listenGeneration) fn();
        else this.unlisten = fn;
      },
      () => {
        logWarn(["frontend"], "map plan proposals event could not be followed", {});
      },
    );
  }

  /** Read the proposals of a discussion again. */
  read = (discussionId: string): void => {
    const mine = (this.sequence.get(discussionId) ?? 0) + 1;
    this.sequence.set(discussionId, mine);
    api.listMapPlanProposals(discussionId).then(
      (list) => {
        if (this.sequence.get(discussionId) !== mine) return;
        this.hold(discussionId, Array.isArray(list) ? list : []);
      },
      () => {
        if (this.sequence.get(discussionId) !== mine) return;
        logWarn(["frontend"], "map plan proposals could not be read", {});
        if (this.state.proposals.get(discussionId) != null) return;
        const unavailable = new Set(this.state.unavailable);
        unavailable.add(discussionId);
        this.set({ unavailable });
      },
    );
  };

  /** Put a list of proposals in place, and announce each status that changed. */
  private hold(discussionId: string, list: MapPlanProposal[]) {
    const before = this.state.proposals.get(discussionId) ?? null;
    const proposals = new Map(this.state.proposals);
    proposals.set(discussionId, list);
    const unavailable = new Set(this.state.unavailable);
    unavailable.delete(discussionId);
    const changed = before === null ? [] : this.changedStatuses(before, list);
    this.set({
      proposals,
      unavailable,
      ...(changed.length > 0 ? { announcement: changed.join(". ") } : {}),
    });
  }

  /** MPV-FR-XAOG: the rows whose state differs between two reads. */
  private changedStatuses(before: MapPlanProposal[], after: MapPlanProposal[]): string[] {
    const held = new Map<string, string>();
    for (const p of before) for (const r of p.rows) held.set(rowId(p.id, r.key), r.state);
    const out: string[] = [];
    for (const p of after) {
      const statuses = rowStatuses(p, new Set());
      p.rows.forEach((r, i) => {
        const was = held.get(rowId(p.id, r.key));
        if (was !== undefined && was !== r.state) out.push(`${rowTitle(r)}: ${statuses[i]}`);
      });
    }
    return out;
  }

  /** The proposal a decision answered, in place of the held one. */
  private replace(proposal: MapPlanProposal) {
    const list = this.state.proposals.get(proposal.discussionId) ?? [];
    const next = list.some((p) => p.id === proposal.id)
      ? list.map((p) => (p.id === proposal.id ? proposal : p))
      : [...list, proposal];
    this.sequence.set(proposal.discussionId, (this.sequence.get(proposal.discussionId) ?? 0) + 1);
    this.hold(proposal.discussionId, next);
  }

  /** The held proposal of a discussion, or null. */
  proposalOf = (discussionId: string, proposalId: string): MapPlanProposal | null =>
    this.state.proposals.get(discussionId)?.find((p) => p.id === proposalId) ?? null;

  private setBusy(id: string, on: boolean) {
    const busy = new Set(this.state.busy);
    if (on) busy.add(id);
    else busy.delete(id);
    this.set({ busy });
  }

  private setRefusal(id: string, text: string | null) {
    const refusals = new Map(this.state.refusals);
    if (text === null) refusals.delete(id);
    else refusals.set(id, text);
    this.set({ refusals });
  }

  /**
   * MPV-FR-XYDR: one decision on one row. Resolves true when the backend
   * answered, false when it refused or nothing was sent. A decision on a row
   * whose call runs does nothing (MPV-FR-QOPR).
   */
  decide = async (discussionId: string, proposalId: string, rowKey: string, action: Decision): Promise<boolean> => {
    const id = rowId(proposalId, rowKey);
    const row = this.proposalOf(discussionId, proposalId)?.rows.find((r) => r.key === rowKey);
    if (!row || this.state.busy.has(id)) return false;
    this.setBusy(id, true);
    this.setRefusal(id, null);
    logInfo(["frontend"], "map plan row decision started", { action, rowKind: row.kind });
    try {
      const call =
        action === "accept"
          ? api.acceptMapPlanRow
          : action === "reject"
            ? api.rejectMapPlanRow
            : api.reopenMapPlanRow;
      const answered = await call(discussionId, proposalId, rowKey);
      this.replace(answered);
      const decided = answered.rows.find((r) => r.key === rowKey);
      logInfo(["frontend"], "map plan row decided", {
        action,
        rowKind: row.kind,
        state: decided?.state ?? "unknown",
        code: decided?.error?.code ?? "",
      });
      const key = `map-plan-row-${proposalId}-${rowKey}`;
      if (action === "accept" && decided?.state === "accepted") {
        showToast({ key, level: "Info", title: "Plan change applied", body: appliedSentence(decided), address: null });
      } else if (action === "accept" && decided?.state === "failed") {
        showToast({ key, level: "Error", title: "Plan change failed", body: decided.error?.message ?? "", address: null });
      }
      return true;
    } catch (e) {
      logWarn(["frontend"], "map plan row decision refused", { action, rowKind: row.kind, code: refusalCode(e) });
      const text = refusalText(e);
      this.setRefusal(id, text);
      showToast({
        key: `map-plan-row-${proposalId}-${rowKey}`,
        level: "Error",
        title:
          action === "accept"
            ? "Plan change not accepted"
            : action === "reject"
              ? "Plan change not rejected"
              : "Plan change not reopened",
        body: text,
        address: null,
      });
      // The held rows may be older than the backend's, so read them again.
      this.read(discussionId);
      return false;
    } finally {
      this.setBusy(id, false);
    }
  };

  /**
   * MPV-FR-TWSY: decide each `pending` row that is not blocked, one after the
   * other, in proposal order. A pass that unblocks a row listed before its
   * parent is followed by another pass; each row is sent at most once.
   */
  decideAll = async (discussionId: string, proposalId: string, action: "accept" | "reject"): Promise<void> => {
    const sent = new Set<string>();
    for (let progress = true; progress; ) {
      progress = false;
      const keys = this.proposalOf(discussionId, proposalId)?.rows.map((r) => r.key) ?? [];
      for (const key of keys) {
        const row = this.proposalOf(discussionId, proposalId)?.rows.find((r) => r.key === key);
        if (!row || row.state !== "waiting" || sent.has(key)) continue;
        if (action === "accept" && row.blockedBy !== null) continue;
        sent.add(key);
        if (await this.decide(discussionId, proposalId, key, action)) progress = true;
      }
    }
  };

  /** MPV-FR-AAFF: ask the agent that proposed to prepare the failed rows again. */
  requestReprepare = async (discussionId: string, proposalId: string): Promise<void> => {
    const id = rowId(proposalId, "");
    if (this.state.busy.has(id)) return;
    this.setBusy(id, true);
    this.setRefusal(id, null);
    logInfo(["frontend"], "map plan re-prepare requested", {});
    try {
      const answered = await api.requestMapPlanReprepare(discussionId, proposalId);
      this.replace(answered);
    } catch (e) {
      logWarn(["frontend"], "map plan re-prepare refused", { code: refusalCode(e) });
      const text = refusalText(e);
      this.setRefusal(id, text);
      showToast({ key: `map-plan-reprepare-${proposalId}`, level: "Error", title: "Re-prepare not requested", body: text, address: null });
    } finally {
      this.setBusy(id, false);
    }
  };

  /** MPV-FR-RJKZ / MPV-FR-CAQL: open the sheet of a proposal. */
  openReview = (discussionId: string, proposalId: string): void => {
    const proposal = this.proposalOf(discussionId, proposalId);
    const selectedKey = proposal ? firstSelection(proposal, rowStatuses(proposal, this.state.busy)) : null;
    logInfo(["frontend"], "map plan review opened", { rows: proposal?.rows.length ?? 0 });
    this.set({ review: { discussionId, proposalId, selectedKey } });
  };

  /** MPV-FR-PBAD: close the sheet, and give the focus back to its summary row. */
  closeReview = (returnFocus = false): void => {
    const review = this.state.review;
    if (review === null) return;
    this.set({ review: null });
    if (returnFocus) this.returns.get(review.proposalId)?.focus({ preventScroll: true });
  };

  /** Select a row of the open sheet. */
  select = (rowKey: string): void => {
    const review = this.state.review;
    if (review === null || review.selectedKey === rowKey) return;
    this.set({ review: { ...review, selectedKey: rowKey } });
  };

  /** MPV-FR-CAQL: after a decision on the selected row, select the next `pending` row. */
  advanceFrom = (rowKey: string): void => {
    const review = this.state.review;
    if (review === null || review.selectedKey !== rowKey) return;
    const proposal = this.proposalOf(review.discussionId, review.proposalId);
    if (!proposal) return;
    const statuses: RowStatus[] = rowStatuses(proposal, this.state.busy);
    this.select(nextPendingKey(proposal, statuses, rowKey));
  };

  /** DBR-FR-AGIN: the active Map thread. Its proposals are read and mark the board. */
  setActiveDiscussion = (discussionId: string | null): void => {
    if (discussionId !== null) this.want(discussionId);
    if (this.state.activeDiscussionId !== discussionId) this.set({ activeDiscussionId: discussionId });
  };

  /** DBR-FR-AGIN / DBR-FR-WKNI: the proposals of the active thread and the one under review. */
  boardProposals = (): MapPlanProposal[] => {
    const out = [...(this.state.proposals.get(this.state.activeDiscussionId ?? "") ?? [])];
    const review = this.state.review;
    if (review) {
      const reviewed = this.proposalOf(review.discussionId, review.proposalId);
      if (reviewed && !out.some((p) => p.id === reviewed.id)) out.push(reviewed);
    }
    return out;
  };

  /** DBR-FR-PMGS: the item id of the row the sheet selects, or null. */
  reviewedItemId = (): string | null => {
    const review = this.state.review;
    if (!review || review.selectedKey === null) return null;
    const proposal = this.proposalOf(review.discussionId, review.proposalId);
    const row = proposal?.rows.find((r) => r.key === review.selectedKey);
    return proposal && row ? rowItemId(proposal.id, row) : null;
  };

  /** MPV-FR-AAFF: note the handle of the agent that recorded a proposal. */
  noteProposer = (proposalId: string, handle: string): void => {
    if (this.state.proposers.get(proposalId) === handle) return;
    const proposers = new Map(this.state.proposers);
    proposers.set(proposalId, handle);
    this.set({ proposers });
  };

  /** MPV-FR-PBAD: the element that takes the focus back when the sheet of a proposal closes. */
  registerReturn = (proposalId: string, element: HTMLElement | null): void => {
    if (element === null) this.returns.delete(proposalId);
    else this.returns.set(proposalId, element);
  };
}

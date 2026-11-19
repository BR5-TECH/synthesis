/**
 * The props of the Flow canvas (`FLO-flow.md`) and the view state a host keeps
 * for it (SMP-FR-RNLH).
 */
import type { ReactNode } from "react";

import type { FlowItem } from "../../state/flowDocument";
import type { ItemDeliveryState, OpenableArtifact } from "../../types";
import type { FlowBinding } from "./binding";
import type { BoardStatusFilter } from "./boardFilter";
import type { BoardStatusNames } from "./boardStatus";
import type { View } from "./geometry";
import type { IssueViewHost } from "./issueView/host";

/** FLO-FR-NRBV: what the shell answers to a request to create a task's draft. */
export type CreatedDraftOutcome =
  | { ok: true; draft: { id: string; name: string } }
  /** `error` is null where the author cancelled the creation. */
  | { ok: false; error: string | null };

export interface FlowCanvasProps {
  /** The plan the canvas shows, and where its edits go (`./binding.ts`). */
  binding: FlowBinding;
  /**
   * MDL-FR-WMGU: the host keeps every edit affordance inert, for example while
   * a publish runs.
   */
  inert?: boolean;
  /** MDL-FR-KKYH / MDL-FR-JRGK: what the host adds at the trailing end of the band. */
  bandExtras?: ReactNode;
  /**
   * MDL-FR-LSKL / MDL-FR-IEWL / MDL-FR-MWMO: the host's notice lines, below
   * the band and above the canvas.
   */
  belowBand?: ReactNode;
  /**
   * MIV-FR-SVXH: the issue view of the board. The canvas shows it in place of
   * the panel of a selected item only where the board has a host for it.
   */
  issueView?: IssueViewHost;
  /** DBR-FR-TCPE: the Ready and In Progress status names of a GitHub plan. */
  statusNames?: BoardStatusNames;
  /** MDL-FR-PUKL: entries the host adds to the add menu; `close` closes it. */
  addMenuExtras?: (close: () => void) => ReactNode;
  /** FLO-FR-12: open a referenced artifact in its natural surface. */
  onOpenArtifact?: (item: OpenableArtifact) => void;
  /**
   * FLO-FR-NRBV: create a draft with the given name through
   * `"create draft (name)"`. Without it, the canvas calls the operation itself.
   */
  onCreateDraft?: (name: string) => Promise<CreatedDraftOutcome>;
  /** FLO-FR-NRBV: open a draft that was just created in a New Artifact tab. */
  onOpenDraft?: (draft: { id: string; name: string }) => void;
  /** FLO-FR-QZUR: open a linked draft through `"open draft (id)"`. */
  onOpenDraftById?: (draftId: string) => void;
  /**
   * SMP-FR-RNLH / SMP-FR-QNUH: the view and the selection the canvas starts
   * from. Without it, the canvas starts from the default view (FLO-FR-22).
   */
  initialView?: FlowViewState;
  /** SMP-FR-RNLH: each change of the view or of the selection. */
  onViewChange?: (state: FlowViewState) => void;
  /** MDL-FR-GDUP: each new answer of the delivery state, or null before one. */
  onDeliveryStates?: (states: ReadonlyMap<string, ItemDeliveryState> | null) => void;
  /**
   * MDL-FR-HULA / MDL-FR-QELI: the **Discuss** entry of the context menu of an
   * item or a milestone. Without it, an item has no context menu.
   */
  onDiscuss?: (item: FlowItem) => void;
  /** DBR-FR-ERHW: lay the plan out as the board of the Delivery lens. */
  board?: boolean;
  /**
   * FLO-FR-OHZB / MDL-FR-MIMF: the status filter of the board. Only the
   * Delivery lens gives one (MDL-FR-XHNZ). Without it, every item shows.
   */
  statusFilter?: BoardStatusFilter;
  /** MDL-FR-BWEF: the count of the cards and groups the filter hides, on each change. */
  onHiddenCount?: (count: number) => void;
}

/** SMP-FR-RNLH: the view state of a canvas that a host keeps for it. */
export interface FlowViewState {
  view: View;
  selection: string[];
}

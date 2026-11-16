/**
 * `DBR-delivery-board.md`: the words and the colours that a card, a group, and
 * a column of the board show: the kind chip (DBR-FR-DZSU), the status in
 * lowercase (DBR-FR-HWHO), and the status dot (DBR-FR-QZPA, DBR-FR-TCPE,
 * DBR-FR-GHDZ).
 */
import { createContext } from "react";

import type { FlowItemKind } from "../../state/flowDocument/types";
import type { FlowItemState, TrackerFacts } from "../../types";
import { STATE_LOOK } from "./chips";

/** DBR-FR-DZSU: the three-letter code of each kind that a card or a group can have. */
export const KIND_CHIPS: Readonly<Record<FlowItemKind, string>> = {
  milestone: "MLS",
  epic: "EPC",
  feature: "FTR",
  task: "TSK",
  defect: "DEF",
};

/** DBR-FR-QZPA: the five colours of the status dot. */
export type BoardDot = "backlog" | "ready" | "in progress" | "in review" | "done";

/**
 * DBR-FR-TCPE: the Ready and In Progress status names of the GitHub Project
 * settings (GPP-FR-VXSB). A null name is unset.
 */
export interface BoardStatusNames {
  ready: string | null;
  inProgress: string | null;
}

export const NO_STATUS_NAMES: BoardStatusNames = { ready: null, inProgress: null };

/** DBR-FR-TCPE: the status names of the plan, for every card of the board. */
export const BoardStatusNamesContext = createContext<BoardStatusNames>(NO_STATUS_NAMES);

/** What the status of an item comes from: its issue, or its delivery state. */
export interface BoardStatusSource {
  tracker?: TrackerFacts;
  state?: FlowItemState;
}

const same = (a: string, b: string | null) => b !== null && a.toLowerCase() === b.toLowerCase();

/**
 * DBR-FR-HWHO: the status word in lowercase: the issue's `Status` name in a
 * GitHub plan, or the name of the delivery state in a Flow file. Null where
 * the item shows no status.
 */
export function boardStatusWord({ tracker, state }: BoardStatusSource): string | null {
  if (tracker) {
    if (tracker.ref.kind !== "issue") return state ? STATE_LOOK[state].word.toLowerCase() : null;
    return (tracker.status || "No status").toLowerCase();
  }
  return state ? STATE_LOOK[state].word.toLowerCase() : null;
}

/**
 * DBR-FR-TCPE / DBR-FR-GHDZ: the colour of the status dot. Null where the item
 * shows no status (DBR-FR-QZPA).
 */
export function boardDot(source: BoardStatusSource, names: BoardStatusNames): BoardDot | null {
  const { tracker, state } = source;
  if (tracker && tracker.ref.kind === "issue") {
    const status = tracker.status ?? "";
    if (same(status, "Done") || tracker.state === "closed") return "done";
    if (same(status, names.ready) || same(status, "Ready")) return "ready";
    if (same(status, names.inProgress) || same(status, "In progress")) return "in progress";
    if (same(status, "In review")) return "in review";
    return "backlog";
  }
  switch (state) {
    case "drafting":
      return "ready";
    case "in_progress":
      return "in progress";
    case "done":
      return "done";
    case "not_started":
    case "dropped":
      return "backlog";
    default:
      return null;
  }
}

/** DBR-FR-BYQX: the badge class of the state of a milestone column. */
export function milestoneBadgeClass(state: FlowItemState | undefined): string {
  if (state === "in_progress") return "badge badge--live";
  if (state === "done") return "badge badge--ok";
  return "badge";
}

import {
  findItem,
  type FlowDocument,
  type FlowItemKind,
  type FlowRefusal,
} from "../../state/flowDocument";

/** What the author tried to do, which names the feedback (FLO-FR-16). */
export type FlowAction =
  | "connect"
  | "retype"
  | "reverse"
  | "move"
  | "add"
  | "link"
  | "kind";

/** The first words of the feedback: what did not happen. */
const TITLES: Record<FlowAction, string> = {
  connect: "Not connected.",
  retype: "Not changed.",
  reverse: "Not reversed.",
  move: "Not moved.",
  add: "Not added.",
  link: "Not linked.",
  kind: "Not changed.",
};

/** The feedback a refusal gives on the canvas. */
export interface RefusalMessage {
  title: string;
  text: string;
  /** FLO-FR-GZCH: the items the feedback names, which the canvas marks. */
  itemIds: string[];
  /** FLO-FR-16: the connection that was refused, which the canvas draws. */
  attempt?: { from: string; to: string };
}

const KIND_NAMES: Record<FlowItemKind, string> = {
  milestone: "a milestone",
  epic: "an epic",
  feature: "a feature",
  task: "a task",
  defect: "a defect",
};

/** A list in words: "A", "A or B", "A, B or C". */
function joinWords(words: string[], last: string): string {
  if (words.length <= 1) return words.join("");
  return `${words.slice(0, -1).join(", ")} ${last} ${words[words.length - 1]}`;
}

/**
 * FLO-FR-OAOZ: the containers a kind can join, in words, for example "A task
 * can sit only in a feature."
 */
export function permittedText(
  kind: FlowItemKind,
  permitted: readonly (FlowItemKind | null)[],
): string {
  const subject = KIND_NAMES[kind];
  const head = subject.charAt(0).toUpperCase() + subject.slice(1);
  if (permitted.length === 1 && permitted[0] === null) {
    return `${head} can sit only at the top level of the plan.`;
  }
  const places = permitted.map((p) => (p === null ? "the top level" : KIND_NAMES[p]));
  return `${head} can sit only in ${joinWords(places, "or")}.`;
}

/**
 * FLO-FR-16 / FLO-FR-17 / FLO-FR-ZEPI / FLO-FR-GZCH / FLO-FR-OAOZ: the text of
 * a refusal. A dependency cycle names its items in the order the work blocks:
 * each item blocks the next, and the last name is the first again.
 */
export function refusalMessage(
  doc: FlowDocument,
  action: FlowAction,
  refusal: FlowRefusal,
  kind?: FlowItemKind,
): RefusalMessage {
  const nameOf = (id: string) => findItem(doc, id)?.name || id;
  const ids = refusal.itemIds ?? [];
  const title = TITLES[action];
  let text: string;
  switch (refusal.code) {
    case "self":
      text = "An item cannot connect to itself.";
      break;
    case "duplicate_edge":
      text = "An edge already joins these two items.";
      break;
    case "edge_within_lineage":
      text =
        action === "connect"
          ? "An item cannot connect to an item that holds it or that it holds."
          : "An edge would join an item to an item that holds it.";
      break;
    case "dependency_cycle":
      if (action === "move") {
        text = `This move makes a cycle among ${joinWords(ids.map(nameOf), "and")}.`;
      } else {
        // `cycleClosedBy` lists the items from the blocked end back along what
        // each one waits for, so the blocking order is that list reversed.
        const order = [...ids].reverse();
        const names = [...order, order[0]].filter((x) => x !== undefined).map(nameOf);
        const what = action === "connect" ? "This edge" : "This change";
        text = `${what} makes a cycle: ${names.join(" → ")}.`;
      }
      break;
    case "invalid_parent_kind":
      text = kind
        ? permittedText(kind, refusal.permitted ?? [])
        : "The container does not permit this kind.";
      break;
    case "into_own_subtree":
      text = "An item cannot go into itself or into an item it holds.";
      break;
    case "member_not_permitted":
      text = `${joinWords(ids.map(nameOf), "and")} cannot sit in that kind.`;
      break;
    case "not_empty":
      text = "A container that holds items cannot become a task or a defect.";
      break;
    case "duplicate_tracker_link":
      text = `${joinWords(ids.map(nameOf), "and")} already links that item.`;
      break;
    case "tracker_kind_mismatch":
      text = "A milestone links only a milestone, and every other kind links only an issue.";
      break;
    case "already_linked":
      text = "This item already has a link.";
      break;
    case "not_synced":
      text = "This Flow is not connected to GitHub.";
      break;
    case "draft_already_linked":
      text = `${joinWords(ids.map(nameOf), "and")} already links that draft.`;
      break;
    default:
      text = "The plan does not permit this change.";
  }
  return { title, text, itemIds: refusal.code === "dependency_cycle" ? ids : [] };
}

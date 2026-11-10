import type { FlowItemState, FlowLeafCounts } from "../../types";
import type { SyncLink } from "../../state/flowDocument";

/**
 * FLO-FR-KQCE: each state has a shape and a word, so a chip never says its
 * state in colour alone.
 */
export const STATE_LOOK: Readonly<Record<FlowItemState, { shape: string; word: string }>> = {
  not_started: { shape: "○", word: "Not started" },
  drafting: { shape: "◔", word: "Drafting" },
  in_progress: { shape: "◑", word: "In progress" },
  done: { shape: "✓", word: "Done" },
  dropped: { shape: "–", word: "Dropped" },
};

/** FLO-FR-KQCE: the status chip. No chip until the delivery state answers. */
export function StatusChip({ state }: { state: FlowItemState | undefined }) {
  if (!state) return null;
  const look = STATE_LOOK[state];
  return (
    <span className="flow-chip" data-state={state} data-testid="flow-status-chip">
      <span className="flow-chip__shape" aria-hidden="true">
        {look.shape}
      </span>
      {look.word}
    </span>
  );
}

/**
 * FLO-FR-IKMM: the Blocked marker. Its tooltip names each blocking item that is
 * not done, and activating it selects them. It changes nothing in the drafts or
 * the runs.
 */
export function BlockedMarker({
  blockers,
  onActivate,
}: {
  blockers: string[];
  onActivate: () => void;
}) {
  const names = blockers.join(", ");
  return (
    <button
      type="button"
      className="flow-marker flow-marker--blocked"
      title={`Blocked by ${names}`}
      aria-label={`Blocked by ${names}. Select the blocking items.`}
      onClick={onActivate}
    >
      <span aria-hidden="true">⊘</span> Blocked
    </button>
  );
}

/**
 * FLO-FR-KWFH: the Overlap marker. Its tooltip names each other item of the
 * overlap and the shared paths, and activating it selects those items.
 */
export function OverlapMarker({
  others,
  paths,
  onActivate,
}: {
  others: string[];
  paths: string[];
  onActivate: () => void;
}) {
  const text = `Overlaps with ${others.join(", ")} on ${paths.join(", ")}`;
  return (
    <button
      type="button"
      className="flow-marker flow-marker--overlap"
      title={text}
      aria-label={`${text}. Select the overlapping items.`}
      onClick={onActivate}
    >
      <span aria-hidden="true">⧉</span> Overlap
    </button>
  );
}

/** The repository a link names, as `owner/repo`. */
export function repoOf(link: SyncLink): string {
  return `${link.ref.owner}/${link.ref.repo}`;
}

/** FLO-FR-ULEM: what a tracker tag reads: the number, or the milestone number. */
export function tagText(link: SyncLink): string {
  return link.ref.kind === "milestone"
    ? `milestone ${link.ref.number}`
    : `#${link.ref.number}`;
}

/**
 * FLO-FR-ULEM: the tracker tag. It names the repository and the number, and
 * activating it opens the tracker item.
 */
export function TrackerTag({
  link,
  onOpen,
}: {
  link: SyncLink;
  onOpen: (url: string) => void;
}) {
  const full =
    link.ref.kind === "milestone"
      ? `${repoOf(link)} milestone ${link.ref.number}`
      : `${repoOf(link)}#${link.ref.number}`;
  return (
    <button
      type="button"
      className="flow-tag"
      title={`Open ${full} on GitHub`}
      aria-label={`Open ${full} on GitHub`}
      onClick={() => onOpen(link.ref.url)}
    >
      {tagText(link)}
    </button>
  );
}

/** FLO-FR-TZSU: the mark of an item that a sync never creates on the tracker. */
export function LocalOnlyMark() {
  return (
    <span className="flow-local" title="A sync does not create this item on GitHub">
      local only
    </span>
  );
}

/**
 * FLO-FR-XAPK: a milestone's due date. A date before today on a milestone that
 * is not done is marked as past, in words as well as in colour.
 */
export function DueDate({ due, past }: { due: string; past: boolean }) {
  return (
    <span className="flow-due" data-past={past} data-testid="flow-due">
      due {due}
      {past && <span className="flow-due__past"> · past</span>}
    </span>
  );
}

const COUNT_ORDER: ReadonlyArray<[keyof FlowLeafCounts, string]> = [
  ["notStarted", "not started"],
  ["drafting", "drafting"],
  ["inProgress", "in progress"],
  ["done", "done"],
  ["dropped", "dropped"],
];

/**
 * FLO-FR-ZASL: the leaves under a collapsed container, by state. Before the
 * delivery state answers, the line gives the number of leaves alone.
 */
export function leafCountText(counts: FlowLeafCounts | null | undefined, leaves: number): string {
  if (!counts) return leaves === 1 ? "1 item" : `${leaves} items`;
  const parts = COUNT_ORDER.filter(([k]) => counts[k] > 0).map(
    ([k, word]) => `${counts[k]} ${word}`,
  );
  return parts.length > 0 ? parts.join(" · ") : "No items";
}

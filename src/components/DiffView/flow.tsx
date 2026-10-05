import { useMemo } from "react";
import {
  changedFacts,
  changedOnly,
  flowDiff,
  type FlowDiffEntry,
  type FlowFact,
} from "../../diff/flowDiff";
import type { FileRevisions } from "../../types";

// ---------------------------------------------------------------------------
// Rich rendering of a Flow (DFV-FR-33 – DFV-FR-35)
// ---------------------------------------------------------------------------

/** Which revision's telling of an entry a surface is showing. */
type FlowSide = "before" | "after" | "both";

const FLOW_SIGN: Record<string, string> = {
  added: "+",
  removed: "−",
  changed: "~",
  none: " ",
  unchanged: " ",
};

/** A fact's value, as the side being rendered has it. */
function FlowFactValue({ fact, side }: { fact: FlowFact; side: FlowSide }) {
  if (side === "before") return <span>{fact.before}</span>;
  if (side === "after") return <span>{fact.after}</span>;
  if (fact.status === "changed") {
    return (
      <span>
        <span className="diff-flow__was">{fact.before}</span>
        <span aria-hidden="true"> → </span>
        <span>{fact.after}</span>
      </span>
    );
  }
  return <span>{fact.after ?? fact.before}</span>;
}

/**
 * One entry — the Flow's own fields, a node, or a connection — with the facts
 * the surface is showing about it.
 *
 * The marking is a whole-entry treatment carrying a sign character as well as a
 * colour, so the diff reads without colour discrimination (DFV NFR), and each
 * fact carries its own marking within an entry that changed.
 */
function FlowEntryView({
  entry,
  side,
  mark,
  facts,
}: {
  entry: FlowDiffEntry;
  side: FlowSide;
  mark: "added" | "removed" | "changed" | "none";
  facts: FlowFact[];
}) {
  const title =
    side === "before"
      ? entry.titleBefore
      : side === "after"
        ? entry.titleAfter
        : (entry.titleAfter ?? entry.titleBefore);
  return (
    <div
      className="diff-flow__entry"
      data-mark={mark}
      data-kind={entry.kind}
      data-testid={`flow-diff-${entry.key}`}
    >
      <div className="diff-flow__title">
        <span className="diff-flow__sign" aria-hidden="true">
          {FLOW_SIGN[mark]}
        </span>
        <span className="diff-flow__kind">{entry.kind}</span>
        <span className="diff-flow__name">{title || "(unnamed)"}</span>
      </div>
      {facts.map((fact, i) => (
        <div className="diff-flow__fact" data-mark={fact.status} key={i}>
          <span className="diff-flow__sign" aria-hidden="true">
            {FLOW_SIGN[fact.status]}
          </span>
          <span className="diff-flow__label">{fact.label}</span>
          <FlowFactValue fact={fact} side={side} />
        </div>
      ))}
    </div>
  );
}

/** The diff, or the state that says why there is none to render. */
function useFlowEntries(revisions: FileRevisions | null) {
  return useMemo(
    () => flowDiff(revisions?.old ?? null, revisions?.new ?? null),
    [revisions],
  );
}

function FlowUnreadable({ error }: { error: string }) {
  return (
    <div className="changes-state" data-state="error">
      This revision could not be read as a Flow: {error}
    </div>
  );
}

/**
 * DFV-FR-34: **Unified** over a Flow — what changed about it and nothing else,
 * in document order, each entry showing only the facts that differ.
 */
export function FlowUnified({ revisions }: { revisions: FileRevisions | null }) {
  const result = useFlowEntries(revisions);
  if (!result.ok) return <FlowUnreadable error={result.error} />;
  const entries = changedOnly(result.entries);
  if (entries.length === 0) {
    // DFV-FR-36: distinct from the no-change state of DFV-FR-28, which is
    // reached when the comparison reports no change to the file at all.
    return (
      <div className="changes-state" data-state="unchanged-graph">
        The file changed, but the Flow it describes did not.
      </div>
    );
  }
  return (
    <div className="diff diff-flow" data-testid="diff-flow-unified">
      {entries.map((entry) => (
        <FlowEntryView
          key={entry.key}
          entry={entry}
          side="both"
          mark={entry.status === "unchanged" ? "none" : entry.status}
          facts={changedFacts(entry)}
        />
      ))}
    </div>
  );
}

/**
 * DFV-FR-35: **Side-by-side** over a Flow — the old Flow left, the new right,
 * each entry opposite its counterpart, with filler where one revision has none.
 */
export function FlowSideBySide({ revisions }: { revisions: FileRevisions | null }) {
  const result = useFlowEntries(revisions);
  if (!result.ok) return <FlowUnreadable error={result.error} />;
  return (
    <div className="diff diff-sbs diff-flow" data-testid="diff-flow-side-by-side">
      <div className="diff-sbs__heading">
        <span className="diff-sbs__pane">old</span>
        <span className="diff-sbs__pane">new</span>
      </div>
      <div className="diff-sbs__scroll">
        {result.entries.map((entry) => (
          <div className="diff-sbs__row" key={entry.key}>
            {entry.titleBefore !== null ? (
              <FlowEntryView
                entry={entry}
                side="before"
                mark={entry.status === "unchanged" ? "none" : "removed"}
                facts={entry.facts.filter((f) => f.before !== undefined)}
              />
            ) : (
              <div className="diff-flow__entry" data-mark="filler" aria-hidden />
            )}
            {entry.titleAfter !== null ? (
              <FlowEntryView
                entry={entry}
                side="after"
                mark={entry.status === "unchanged" ? "none" : "added"}
                facts={entry.facts.filter((f) => f.after !== undefined)}
              />
            ) : (
              <div className="diff-flow__entry" data-mark="filler" aria-hidden />
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

/**
 * DFV-FR-34: **Final** over a Flow — the whole of the new Flow with what
 * changed marked in place, and what was removed shown in the position it held,
 * since a Flow that no longer has a step is the one edit the outcome cannot
 * otherwise show.
 */
export function FlowFinal({ revisions }: { revisions: FileRevisions | null }) {
  const result = useFlowEntries(revisions);
  // DFV-FR-31: a Flow the comparison added is new throughout, and marking every
  // part of it says only what the comparison already says.
  const wholeFileIsNew = revisions != null && revisions.old == null;
  if (!result.ok) return <FlowUnreadable error={result.error} />;
  return (
    <div className="diff diff-flow" data-testid="diff-flow-final">
      {result.entries.map((entry) => (
        <FlowEntryView
          key={entry.key}
          entry={entry}
          // `both`: a changed entry states what each fact was and became, and a
          // fact only one revision holds — a reference the step lost — renders
          // the value it has rather than the empty side (DFV-FR-33).
          side="both"
          mark={
            wholeFileIsNew || entry.status === "unchanged" ? "none" : entry.status
          }
          facts={
            wholeFileIsNew
              ? entry.facts.map((f) => ({ ...f, status: "unchanged" as const }))
              : entry.facts
          }
        />
      ))}
    </div>
  );
}



/**
 * The review card of a plan proposal (`MPV-map-proposal-review.md`), drawn in an
 * agent comment of a Map discussion that carries a `map_plan_proposal`
 * attachment (SMP-FR-PPJB).
 *
 * The card decides nothing for the author. Each row has its own Accept and
 * Reject, and a decision changes no other row (MPV-FR-XYDR). Nothing reaches
 * GitHub, and no draft is created, before the author accepts the row that does
 * it.
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { showToast } from "../../state/toasts";
import { Icon } from "../icons";
import type { MapNodeRef, MapPlanProposal, MapPlanRow } from "../../types";
import { RowView } from "./RowView";
import { useMapPlanProposals } from "./useMapPlanProposals";
import { errorText, kindWord, refusalCode, refusalText, rowTitle, stateWord } from "./words";

export { MapPlanHostContext, useMapPlanHost, type MapPlanHost } from "./host";

type Action = "accept" | "reject";

/** MPV-FR-WKVL: what an accepted row did, in one sentence. */
function acceptedSentence(row: MapPlanRow | undefined): string {
  if (!row) return "The row was accepted.";
  if (row.kind === "new_draft") return "A draft was created.";
  if (row.kind === "create_issue" && row.result?.issueNumber !== undefined) {
    return `Issue #${row.result.issueNumber} was created.`;
  }
  if (row.kind === "modify_issue") return `Issue #${row.fields.number} was changed on GitHub.`;
  return "The row was accepted.";
}

/** MPV-FR-FUQG: the title of the row that a row names as its parent, or null. */
function parentTitleOf(proposal: MapPlanProposal, row: MapPlanRow): string | null {
  const key =
    row.blockedBy?.rowKey ??
    (row.kind === "create_issue" && row.fields.parent && "row" in row.fields.parent
      ? row.fields.parent.row
      : null);
  if (key === null) return null;
  const parent = proposal.rows.find((r) => r.key === key);
  return parent ? rowTitle(parent) : null;
}

/** MPV-FR-XAOG: the rows whose state words differ from the held ones. */
function changedRows(held: ReadonlyMap<string, string>, rows: readonly MapPlanRow[]): string[] {
  return rows
    .filter((row) => held.has(row.key) && held.get(row.key) !== stateWord(row))
    .map((row) => `${kindWord(row)} ${rowTitle(row)}: ${stateWord(row)}`);
}

export function MapPlanProposalCard({
  discussionId,
  proposalId,
}: {
  discussionId: string;
  proposalId: string;
}) {
  const { proposals, unavailable, replace, reload } = useMapPlanProposals(discussionId);
  const [busy, setBusy] = useState<ReadonlySet<string>>(() => new Set());
  const [refusals, setRefusals] = useState<Readonly<Record<string, string>>>({});
  const [announcement, setAnnouncement] = useState("");
  const alive = useRef(true);
  const states = useRef<Map<string, string> | null>(null);
  const proposal = proposals?.find((p) => p.id === proposalId) ?? null;

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  // MPV-FR-XAOG: a change of a state is announced; the first read is not.
  useEffect(() => {
    if (!proposal) return;
    const next = new Map(proposal.rows.map((row) => [row.key, stateWord(row)]));
    if (states.current !== null) {
      const changed = changedRows(states.current, proposal.rows);
      if (changed.length > 0) setAnnouncement(changed.join(". "));
    }
    states.current = next;
  }, [proposal]);

  const decide = async (row: MapPlanRow, action: Action) => {
    if (!proposal || busy.has(row.key)) return;
    const proposalKey = proposal.id;
    setBusy((held) => new Set(held).add(row.key));
    setRefusals(({ [row.key]: _dropped, ...rest }) => rest);
    logInfo(["frontend"], "map plan row decision started", { action, rowKind: row.kind });
    try {
      const call = action === "accept" ? api.acceptMapPlanRow : api.rejectMapPlanRow;
      const answered = await call(discussionId, proposalKey, row.key);
      if (!alive.current) return;
      replace(answered);
      const decided = answered.rows.find((r) => r.key === row.key);
      logInfo(["frontend"], "map plan row decided", {
        action,
        rowKind: row.kind,
        state: decided?.state ?? "unknown",
        code: decided?.error?.code ?? "",
      });
      const toastKey = `map-plan-row-${proposalKey}-${row.key}`;
      if (action === "accept" && decided?.state === "accepted") {
        showToast({ key: toastKey, level: "Info", title: "Plan row accepted", body: acceptedSentence(decided), address: null });
      } else if (action === "accept" && decided?.state === "failed") {
        showToast({ key: toastKey, level: "Error", title: "Plan row failed", body: errorText(decided), address: null });
      }
    } catch (e) {
      logWarn(["frontend"], "map plan row decision refused", {
        action,
        rowKind: row.kind,
        code: refusalCode(e),
      });
      if (!alive.current) return;
      const text = refusalText(e);
      setRefusals((held) => ({ ...held, [row.key]: text }));
      showToast({
        key: `map-plan-row-${proposalKey}-${row.key}`,
        level: "Error",
        title: action === "accept" ? "Plan row not accepted" : "Plan row not rejected",
        body: text,
        address: null,
      });
      // The held rows may be older than the backend's, so read them again.
      reload(true);
    } finally {
      if (alive.current) {
        setBusy((held) => {
          const next = new Set(held);
          next.delete(row.key);
          return next;
        });
      }
    }
  };

  if (unavailable && proposals === null) {
    return (
      <p className="mpv-card mpv-card--note" role="status">
        The plan proposal could not be read.
      </p>
    );
  }
  if (proposals === null) {
    return (
      <p className="mpv-card mpv-card--note" role="status">
        Reading the plan proposal…
      </p>
    );
  }
  if (!proposal) return null;

  const waiting = proposal.rows.filter((row) => row.state === "waiting").length;
  const total = proposal.rows.length;
  return (
    <section className="mpv-card" aria-label="Plan proposal" data-testid="map-plan-proposal" data-settled={waiting === 0 || undefined}>
      <header className="mpv-card__head">
        <Icon.Layers size={12} />
        <span className="mpv-card__title">Plan proposal</span>
        <span className="mpv-card__count" data-testid="mpv-count">
          {total} {total === 1 ? "row" : "rows"} · {waiting === 0 ? "all rows decided" : `${waiting} waiting`}
        </span>
      </header>
      <ol className="mpv-rows">
        {proposal.rows.map((row) => (
          <RowView
            key={row.key}
            row={row}
            parentTitle={parentTitleOf(proposal, row)}
            busy={busy.has(row.key)}
            refusal={refusals[row.key] ?? null}
            onAccept={() => void decide(row, "accept")}
            onReject={() => void decide(row, "reject")}
          />
        ))}
      </ol>
      {waiting > 0 && (
        <p className="mpv-card__hint">The agent proposes again after every row is decided.</p>
      )}
      <div className="mpv-sr-only" role="status" aria-live="polite" data-testid="mpv-announcement">
        {announcement}
      </div>
    </section>
  );
}

/** MPR-FR-PMLX: the focus a message was sent with, as a chip below its body. */
export function MapNodeChip({ node }: { node: MapNodeRef }) {
  return (
    <span className="comment-attach__chip" data-testid="comment-attachment-map-node" title={`Focus: ${node.label}`}>
      <Icon.Node size={11} />
      <span className="comment-attach__chip-name">{node.label}</span>
      <span className="comment-attach__chip-type">focus</span>
    </span>
  );
}

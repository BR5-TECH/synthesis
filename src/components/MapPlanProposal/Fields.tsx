/**
 * MPV-FR-CCDM .. MPV-FR-OCYF: the field changes of a row: an uppercase label
 * column and the value before struck through, an arrow, and the value after.
 */
import { useContext, type ReactNode } from "react";

import type { MapPlanProposal, MapPlanRow, RowChange, TrackerFacts } from "../../types";
import { rowTitle } from "../../state/mapPlan/derive";
import { boardDot, BoardStatusNamesContext } from "../Flow/boardStatus";
import { Icon } from "../icons";
import { closedWord, FIELD_ORDER, plainValue } from "./words";

type Tone = "old" | "new";

function Value({ tone, mono, children }: { tone: Tone; mono?: boolean; children: ReactNode }) {
  return <span className={`pp-v${mono ? " pp-v--mono" : ""} pp-v--${tone}`}>{children}</span>;
}

/** MPV-FR-JBNC: a Status value with the status dot of the board. */
function StatusValue({ tone, name }: { tone: Tone; name: string }) {
  const names = useContext(BoardStatusNamesContext);
  const tracker: TrackerFacts = {
    itemId: "",
    ref: { kind: "issue", number: 0, url: "" },
    state: "open",
    closeReason: null,
    updatedAt: "",
    status: name,
    claimEligible: false,
    externalBlockers: [],
  };
  const dot = boardDot({ tracker }, names);
  return (
    <Value tone={tone}>
      <i className="dm-dot" data-s={dot ?? "backlog"} aria-hidden="true" />
      {name}
    </Value>
  );
}

function Arrow() {
  return (
    <span className="pp-arrow" aria-label="to">
      <Icon.ArrowRight size={12} aria-hidden="true" />
    </span>
  );
}

function present(value: unknown): boolean {
  return value !== null && value !== undefined && value !== "";
}

/** MPV-FR-CGOG: the title of the row of the same proposal that a parent value
 * names, or `null` where the value names a plan issue. */
function parentRowTitle(proposal: MapPlanProposal, value: unknown): string | null {
  if (typeof value !== "object" || value === null || !("row" in value)) return null;
  const key = (value as { row: unknown }).row;
  const named = proposal.rows.find((r) => r.key === key);
  return named ? rowTitle(named) : "a new issue";
}

/** One side of a field change, or nothing where the side is empty. */
function side(proposal: MapPlanProposal, change: RowChange, tone: Tone): ReactNode {
  const value = tone === "old" ? change.before : change.after;
  switch (change.field) {
    case "status":
      return present(value) ? <StatusValue tone={tone} name={plainValue(value)} /> : null;
    case "parent": {
      const title = parentRowTitle(proposal, value);
      return (
        <Value tone={tone} mono={title === null}>
          {title ?? plainValue(value)}
        </Value>
      );
    }
    case "milestone":
      return (
        <Value tone={tone} mono>
          {plainValue(value)}
        </Value>
      );
    default:
      return <Value tone={tone}>{plainValue(value)}</Value>;
  }
}

/** MPV-FR-CCDM / MPV-FR-JBNC / MPV-FR-LYKS: the value cell of one change. */
function ChangeValue({ proposal, change }: { proposal: MapPlanProposal; change: RowChange }) {
  if (change.field === "title") {
    return (
      <div className="pp-fields__v pp-fields__v--stack">
        <span className="pp-text-old">{plainValue(change.before)}</span>
        <span className="pp-text-new">{plainValue(change.after)}</span>
      </div>
    );
  }
  if (change.field === "labels") {
    return (
      <div className="pp-fields__v">
        {(change.added ?? []).map((name) => (
          <Value key={`+${name}`} tone="new">
            + {name}
          </Value>
        ))}
        {(change.removed ?? []).map((name) => (
          <Value key={`-${name}`} tone="old">
            <span className="pp-sr-only">removed </span>
            {name}
          </Value>
        ))}
      </div>
    );
  }
  if (change.field === "close") {
    return (
      <div className="pp-fields__v">
        <Value tone="old">open</Value>
        <Arrow />
        <Value tone="new">{closedWord(change)}</Value>
      </div>
    );
  }
  return (
    <div className="pp-fields__v">
      {side(proposal, change, "old")}
      {present(change.before) && <Arrow />}
      {side(proposal, change, "new")}
    </div>
  );
}

/** MPV-FR-OCYF: the fields an added row sets, each as a value after alone. */
function createdFields(proposal: MapPlanProposal, row: MapPlanRow & { kind: "create_issue" }) {
  const f = row.fields;
  const out: { label: string; value: ReactNode }[] = [];
  const cell = (value: ReactNode) => <div className="pp-fields__v">{value}</div>;
  if (f.milestone) out.push({ label: "Milestone", value: cell(<Value tone="new" mono>{f.milestone.title}</Value>) });
  if (f.type) out.push({ label: "Type", value: cell(<Value tone="new">{f.type}</Value>) });
  if (f.parent) {
    const parent =
      "issue" in f.parent
        ? `#${f.parent.issue}`
        : (() => {
            const named = proposal.rows.find((r) => r.key === (f.parent as { row: string }).row);
            return named ? rowTitle(named) : "a new issue";
          })();
    out.push({ label: "Parent", value: cell(<Value tone="new" mono>{parent}</Value>) });
  }
  if (f.labels.length > 0) {
    out.push({
      label: "Labels",
      value: cell(
        f.labels.map((name) => (
          <Value key={name} tone="new">
            + {name}
          </Value>
        )),
      ),
    });
  }
  return out;
}

export function Fields({ proposal, row }: { proposal: MapPlanProposal; row: MapPlanRow }) {
  let entries: { label: string; value: ReactNode }[] = [];
  if (row.kind === "modify_issue") {
    entries = FIELD_ORDER.flatMap(({ field, label }) => {
      const change = row.fields.changes.find((c) => c.field === field);
      return change ? [{ label, value: <ChangeValue proposal={proposal} change={change} /> }] : [];
    });
  } else if (row.kind === "create_issue") {
    entries = createdFields(proposal, row);
  }
  if (entries.length === 0) return null;
  return (
    <div className="pp-fields" data-testid="pp-fields">
      {entries.map(({ label, value }) => (
        <FieldRow key={label} label={label}>
          {value}
        </FieldRow>
      ))}
    </div>
  );
}

function FieldRow({ label, children }: { label: string; children: ReactNode }) {
  return (
    <>
      <span className="pp-fields__k">{label}</span>
      {children}
    </>
  );
}

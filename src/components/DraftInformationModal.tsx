/**
 * Draft information (`specifications/ui/DFI-draft-information.md`).
 *
 * The surface that answers "what did this draft cost?". A centred modal overlay
 * showing one draft's **captured lifetime totals** and its **GitHub publication
 * record**: it decides nothing, changes nothing, and starts nothing, and dismissing it leaves the
 * draft, its conversations, its graduation runs, and its history exactly as they
 * were (DFI-FR-LTJA, DFI-FR-EBIL).
 *
 * Every figure comes from the one read and none is calculated here
 * (DFI-FR-RQVE): the surface adds nothing, treats no unavailable bucket as a
 * zero, and reads a total's availability from the payload. A total the
 * application did not capture is stated as **not captured** and one whose record
 * is damaged or partial carries a marker saying so — never a `0`, a dash, a
 * blank, or an omitted row, because a zero is a claim and these are not
 * (DFI-FR-HSYB).
 */
import { useCallback, useEffect, useRef, useState } from "react";

import { getDraftPublication, openPublicationIssue, readDraftStatistics } from "../api";
import { onDraftPublicationChanged, onDraftStatisticsChanged } from "../events";
import { logWarn } from "../logging";
import type {
  DraftPublicationView,
  DraftStatistics,
  Measure,
  PublicationRecord,
  TokenPair,
} from "../types";
import { Icon } from "./icons";
import { describePublicationChoice } from "../state/publicationChoice";

/**
 * DFI-FR-BZQN / DFI-FR-TRXG / DFI-FR-WQLE / DFI-FR-KAVX: the publication group.
 *
 * The current record leads it under its own subheading, then every earlier
 * record newest first. It is read-only: no record can be edited, reordered, or
 * removed, and a standing attempt is not acted on here — that is the draft's own
 * tab (`NAW-new-artifact.md` NAW-FR-LQAF). The group is **absent** where the
 * draft holds no record, which is not the same thing as a statistic that was
 * never captured and carries none of its wording.
 */
function PublicationGroup({
  draftId,
  publication,
}: {
  draftId: string;
  publication: DraftPublicationView | null;
}) {
  const current = publication?.current ?? null;
  if (!current) return null;
  const earlier = (publication?.history ?? []).filter(
    (record) => record.marker !== current.marker,
  );
  const open = (url: string) =>
    void openPublicationIssue(draftId, url).catch((e) =>
      logWarn(["frontend"], "a publication issue could not be opened", {
        draftId,
        reason: String(e),
      }),
    );
  return (
    <>
      <div className="draft-info__group t-ui-sm">Published to GitHub</div>
      <div className="draft-info__subgroup t-ui-xs">Current</div>
      <PublicationEntry record={current} onOpen={open} />
      {earlier.length > 0 && (
        <>
          <div className="draft-info__subgroup t-ui-xs">History</div>
          {earlier.map((record) => (
            <PublicationEntry key={record.marker} record={record} onOpen={open} />
          ))}
        </>
      )}
    </>
  );
}

/** DFI-FR-BZQN: one record — repository, issue, link, instant, and marker. */
function PublicationEntry({
  record,
  onOpen,
}: {
  record: PublicationRecord;
  onOpen: (url: string) => void;
}) {
  const repository = `${record.repositoryOwner}/${record.repositoryName}`;
  return (
    <div className="draft-info__publication">
      <button
        className="btn btn--ghost btn--sm draft-info__publication-issue"
        onClick={() => onOpen(record.issueUrl)}
        title={record.issueUrl}
      >
        {`${repository} \u00b7 #${record.issueNumber}`}
      </button>
      <span className="t-ui-xs">{formatBoundary(record.publishedAt)}</span>
      {/* NAW-FR-QEZG: root or sub-issue, with the Type and milestone the record
          holds. A record written before the choice existed reads as a root issue. */}
      <span className="t-ui-xs" data-testid="publication-record-choice">
        {describePublicationChoice(record.choice)}
      </span>
      <span className="t-ui-xs">{`marker ${record.marker}`}</span>
    </div>
  );
}

/** DFI-FR-HSYB: what an unavailable figure reads as, in place of a number. */
export const NOT_CAPTURED = "not captured";

/** DFI-FR-HSYB: what the incomplete marker says, in words a reader hears. */
export const PARTIAL_RECORD = "the captured record for this figure is partial";

/**
 * DFI-FR-MODK: the six buckets and their total, in the fixed order every
 * reading of them uses.
 *
 * Fixed in the surface rather than derived from the payload, so two drafts read
 * the same way and **Refinement** is always first.
 */
const BUCKETS = [
  ["refinement", "Refinement"],
  ["authoring", "Authoring"],
  ["validationHandoffPublication", "Validation / hand-off / publication"],
  ["implementation", "Implementation"],
  ["reviews", "Reviews"],
  ["reconciliation", "Reconciliation"],
  ["total", "Total"],
] as const;

/** A duration, as an author reads one. Never rendered for an absent value. */
export function formatDuration(ms: number): string {
  const totalMinutes = Math.floor(ms / 60_000);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;
  if (hours === 0 && totalMinutes === 0) {
    // Under a minute is still time that was spent, so it is stated in seconds
    // rather than rounded away to a zero the author would read as "none".
    return `${Math.floor(ms / 1000)}s`;
  }
  return hours === 0 ? `${minutes}m` : `${hours}h ${String(minutes).padStart(2, "0")}m`;
}

/** A count or a token figure, grouped so a six-figure number is readable. */
export function formatCount(value: number): string {
  // Grouped by hand rather than through `toLocaleString`, which picks its
  // separator from the host's locale data — a comma here, a narrow no-break
  // space there — and would make one draft's figures read differently from
  // another's on a second machine.
  const digits = Math.trunc(Math.abs(value)).toString();
  let grouped = "";
  for (let index = 0; index < digits.length; index += 1) {
    const fromEnd = digits.length - index;
    if (index > 0 && fromEnd % 3 === 0) grouped += " ";
    grouped += digits[index];
  }
  return value < 0 ? `-${grouped}` : grouped;
}

/** DFI-FR-NTPG: the day the draft's captured record begins on. */
export function formatBoundary(at: string): string {
  const date = new Date(at);
  if (Number.isNaN(date.getTime())) return at;
  return date.toLocaleDateString("en-GB", {
    day: "numeric",
    month: "short",
    year: "numeric",
  });
}

/**
 * DFI-FR-HSYB: one figure, rendered as what the read says it is.
 *
 * The row is always present. An unavailable figure states that it was not
 * captured; an incomplete one renders the value it has beside a marker whose
 * hover and accessible name say the record is partial.
 */
function Figure({
  measure,
  format,
  label,
}: {
  measure: Measure;
  format: (value: number) => string;
  label: string;
}) {
  if (measure.availability === "unavailable" || measure.value === null) {
    return (
      <span className="draft-info__value draft-info__value--absent">
        <span aria-label={`${label}, ${NOT_CAPTURED}`}>{NOT_CAPTURED}</span>
      </span>
    );
  }
  const text = format(measure.value);
  if (measure.availability === "incomplete") {
    return (
      <span className="draft-info__value">
        <span aria-label={`${label}, ${text}, ${PARTIAL_RECORD}`}>{text}</span>{" "}
        <span
          className="draft-info__partial"
          data-testid="draft-info-partial"
          role="img"
          aria-label={PARTIAL_RECORD}
          title={PARTIAL_RECORD}
        >
          <Icon.AlertTriangle size={11} />
        </span>
      </span>
    );
  }
  return (
    <span className="draft-info__value">
      <span aria-label={`${label}, ${text}`}>{text}</span>
    </span>
  );
}

/** One labelled row of the body. Nothing in it is a control (DFI-FR-LTJA). */
function Row({
  label,
  measure,
  format,
  indent,
}: {
  label: string;
  measure: Measure;
  format: (value: number) => string;
  indent?: boolean;
}) {
  return (
    <div className={indent ? "draft-info__row draft-info__row--nested" : "draft-info__row"}>
      <span className="draft-info__label t-ui-sm">{label}</span>
      <Figure measure={measure} format={format} label={label} />
    </div>
  );
}

/** A bucket's input and output figures, each carrying its own availability. */
function TokenRow({ label, pair, indent }: { label: string; pair: TokenPair; indent?: boolean }) {
  return (
    <div className={indent ? "draft-info__row draft-info__row--nested" : "draft-info__row"}>
      <span className="draft-info__label t-ui-sm">{label}</span>
      <span className="draft-info__pair">
        <Figure measure={pair.input} format={formatCount} label={`${label} input`} />
        <Figure measure={pair.output} format={formatCount} label={`${label} output`} />
      </span>
    </div>
  );
}

export function DraftInformationModal({
  draftId,
  draftName,
  returnFocus,
  onClose,
}: {
  draftId: string;
  draftName: string;
  /**
   * Where focus goes when the modal closes. Supplied rather than read from
   * `document.activeElement`, because the menu entry that opened it unmounts
   * with the menu and leaves nothing to hand focus back to.
   */
  returnFocus?: HTMLElement | null;
  onClose: () => void;
}) {
  const [statistics, setStatistics] = useState<DraftStatistics | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  // DFI-FR-BZQN: the publication group's own read, kept apart from the
  // statistics read because the two answer different questions and either may
  // be unavailable while the other is not.
  const [publication, setPublication] = useState<DraftPublicationView | null>(
    null,
  );
  const ref = useRef<HTMLDivElement>(null);

  const read = useCallback(async () => {
    try {
      setStatistics(await readDraftStatistics(draftId));
      setFailure(null);
    } catch (e) {
      // DFI-FR-CVAX: a failed read is stated as one, with a retry, and leaves
      // the previous figures standing rather than replacing them with zeros.
      // Reported to the session log as well as to the surface, on the same
      // terms the editing observer reports its own refusals: a read that keeps
      // failing is otherwise invisible to whoever has to diagnose it.
      logWarn(["frontend"], "a draft's statistics could not be read", {
        draftId,
        reason: String(e),
      });
      setFailure(String(e));
    }
  }, [draftId]);

  // DFI-FR-BZQN: read when the modal opens and again on each
  // `"draft publication changed"` naming this draft. DFI-FR-KAVX: this surface
  // reads publication and never starts, retries, recovers, or abandons one.
  const readPublication = useCallback(async () => {
    try {
      setPublication(await getDraftPublication(draftId));
    } catch (e) {
      // A publication read that fails leaves the group absent rather than
      // taking the statistics down with it.
      logWarn(["frontend"], "a draft's publication could not be read", {
        draftId,
        reason: String(e),
      });
    }
  }, [draftId]);

  useEffect(() => {
    void read();
    void readPublication();
  }, [read, readPublication]);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    onDraftPublicationChanged((payload) => {
      if (payload.draftId === draftId) void readPublication();
    })
      .then((off) => {
        if (cancelled) off();
        else unlisten = off;
      })
      .catch((e) =>
        logWarn(["frontend"], "a draft's publication subscription was refused", {
          draftId,
          reason: String(e),
        }),
      );
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [draftId, readPublication]);

  // DFI-FR-ZGBU: re-read and redraw on each `"draft statistics changed"` naming
  // this draft, so a modal left open while an agent works follows the totals
  // rather than showing what was true when it opened. An event naming another
  // draft is nothing to this modal.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    onDraftStatisticsChanged((payload) => {
      if (payload.draftId === draftId) void read();
    })
      .then((off) => {
        if (cancelled) off();
        else unlisten = off;
      })
      // A subscription that cannot be established leaves the modal showing what
      // the one read returned rather than following the totals, which is a
      // smaller thing than a rejection nothing is awaiting: this surface is a
      // reading surface, and it must not take the window down to say so.
      .catch((e) =>
        logWarn(["frontend"], "a draft's statistics subscription was refused", {
          draftId,
          reason: String(e),
        }),
      );
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [draftId, read]);

  // DFI-FR-EBIL: Escape and the backdrop dismiss it, and the modal takes and
  // traps focus while it is open.
  useEffect(() => {
    const opener = returnFocus ?? (document.activeElement as HTMLElement | null);
    const focusables = () =>
      Array.from(
        ref.current?.querySelectorAll<HTMLElement>(
          'button:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ) ?? [],
      );
    focusables()[0]?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onClose();
        return;
      }
      if (e.key !== "Tab") return;
      const items = focusables();
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement as HTMLElement | null;
      if (e.shiftKey && (active === first || !ref.current?.contains(active))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      if (opener?.isConnected) opener.focus();
    };
    // Mount-only: re-running would steal focus back to the close control every
    // time a change event redrew the body.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div
      className="scrim"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div
        ref={ref}
        className="modal drafts-overlay draft-info"
        role="dialog"
        aria-modal="true"
        aria-label={`Information for ${draftName}`}
      >
        <div className="modal__head">
          <div className="modal__title">{draftName}</div>
          <button
            className="btn btn--icon"
            aria-label="Close"
            onClick={onClose}
          >
            <Icon.X size={12} />
          </button>
        </div>
        <div className="modal__body draft-info__body">
          {failure && (
            <div className="draft-info__failure" role="alert">
              <span className="t-ui-sm">This draft's statistics could not be read.</span>
              <button className="btn btn--ghost btn--sm" onClick={() => void read()}>
                Retry
              </button>
            </div>
          )}
          {statistics && (
            <>
              {/* DFI-FR-RQVE: the five counters and the conversation token
                  pair, in this order and no other. */}
              <Row label="Time spent editing" measure={statistics.editingTimeMs} format={formatDuration} />
              <Row label="AI interactions" measure={statistics.aiInteractions} format={formatCount} />
              <Row label="Draft edits" measure={statistics.draftEdits} format={formatCount} />
              <Row label="Accepted proposals" measure={statistics.acceptedProposals} format={formatCount} />
              <Row label="Rejected proposals" measure={statistics.rejectedProposals} format={formatCount} />
              <div className="draft-info__group t-ui-sm">Conversation tokens</div>
              <Row label="Input" measure={statistics.conversationTokens.input} format={formatCount} indent />
              <Row label="Output" measure={statistics.conversationTokens.output} format={formatCount} indent />

              <div className="draft-info__group t-ui-sm">Agent time</div>
              {BUCKETS.map(([key, label]) => (
                <Row
                  key={`time-${key}`}
                  label={label}
                  measure={statistics.agentTimeMs[key]}
                  format={formatDuration}
                  indent
                />
              ))}

              <div className="draft-info__group t-ui-sm">Agent-reported tokens</div>
              {BUCKETS.map(([key, label]) => (
                <TokenRow
                  key={`tokens-${key}`}
                  label={label}
                  pair={statistics.agentTokens[key]}
                  indent
                />
              ))}

              {/* DFI-FR-BZQN: the publication group, last of the four and
                  absent where the draft holds no record — an absent group is
                  not an unavailable statistic (DFI-FR-TRXG). */}
              <PublicationGroup draftId={draftId} publication={publication} />

              {/* DFI-FR-NTPG: the instant the draft's captured record begins
                  at, or that nothing has been captured for it. */}
              <div className="draft-info__footer t-ui-xs">
                {statistics.boundaryAt
                  ? `Statistics cover activity captured since ${formatBoundary(statistics.boundaryAt)}.`
                  : "Nothing has been captured for this draft yet."}
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

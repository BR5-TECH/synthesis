import { useEffect, useRef, useState } from "react";

import {
  findItem,
  PERMITTED_PARENTS,
  syncLinkOf,
  type FlowDocument,
} from "../../state/flowDocument";
import type { FlowSyncState, ReviewConflict } from "../../state/flowSessions";
import type { SkippedIssue, SyncChoice } from "../../types";
import { tagText } from "./chips";
import { kindGlyph } from "./kindGlyphs";
import { permittedText } from "./refusals";
import { fieldLabel, formatSyncValue, isLongText } from "./syncValues";

/** FSR-FR-QAKW: the words of each choice. */
const CHOICE_WORDS: Record<SyncChoice, string> = {
  keep_flow: "Keep Flow",
  take_tracker: "Take GitHub",
  unlink: "Unlink",
  restore: "Restore",
  remove_item: "Remove item",
};

/** FSR-FR-CSOT: the reason of a skipped issue, in words. */
const SKIP_REASONS: Record<string, string> = {
  no_kind_fits: "No kind fits its Type",
  issue_type_unavailable: "Its Type is not available in the repository",
  foreign_repository: "It is in another repository",
};

export interface SyncReviewProps {
  doc: FlowDocument;
  sync: FlowSyncState;
  onClose: () => void;
  onApply: (resolutions: { conflictId: string; choice: SyncChoice }[]) => Promise<boolean>;
  onSyncAgain: () => void;
  onSelectItem: (itemId: string) => void;
  onOpenLink: (url: string) => void;
}

/** FSR-FR-RDZI: the conflicts grouped by item, in document order; gone items last. */
function groupConflicts(doc: FlowDocument, conflicts: ReviewConflict[]) {
  const groups = new Map<string, ReviewConflict[]>();
  for (const c of conflicts) {
    const list = groups.get(c.itemId) ?? [];
    list.push(c);
    groups.set(c.itemId, list);
  }
  const order = new Map(doc.items.map((i, n) => [i.id, n]));
  return [...groups.entries()].sort(([a], [b]) => {
    const na = order.get(a) ?? Number.MAX_SAFE_INTEGER;
    const nb = order.get(b) ?? Number.MAX_SAFE_INTEGER;
    return na - nb;
  });
}

/** FSR-FR-IFQE: a value that shows its first lines and expands in place. */
function Value({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  const long = isLongText(text);
  return (
    <span className="flow-review__value">
      <span className="flow-review__text" data-clamped={long && !open}>
        {text}
      </span>
      {long && (
        <button type="button" className="flow-review__more" onClick={() => setOpen((o) => !o)}>
          {open ? "Show less" : "Show all"}
        </button>
      )}
    </span>
  );
}

/** FSR-FR-BRIA: the rule an `invalid_on_tracker` value breaks, in words. */
function invalidRule(doc: FlowDocument, c: ReviewConflict): string {
  const item = findItem(doc, c.itemId);
  const value = formatSyncValue(doc, c.field, c.tracker);
  const field = fieldLabel(c.field).toLowerCase();
  const head = `GitHub sets its ${field} to ${value}.`;
  if (!item) return head;
  if (c.field === "type") {
    return `${head} The kind that Type gives cannot sit where this item sits.`;
  }
  return `${head} ${permittedText(item.kind, PERMITTED_PARENTS[item.kind])}`;
}

function ConflictRow({
  doc,
  conflict,
  choice,
  onChoose,
}: {
  doc: FlowDocument;
  conflict: ReviewConflict;
  choice: SyncChoice | undefined;
  onChoose: (c: SyncChoice) => void;
}) {
  const c = conflict;
  const what =
    c.kind === "field"
      ? fieldLabel(c.field)
      : c.kind === "removed_in_flow"
        ? "Removed in this Flow"
        : c.kind === "removed_on_tracker"
          ? "Not found on GitHub"
          : "Not valid in this plan";
  return (
    <div className="flow-review__row" data-testid={`flow-conflict-${c.id}`} data-stale={!!c.stale}>
      {c.kind === "field" ? (
        <>
          <div className="flow-review__field">{what}</div>
          <div className="flow-review__vals">
            <span className="flow-review__side">In this Flow</span>
            <Value text={formatSyncValue(doc, c.field, c.flow)} />
            <span className="flow-review__side">On GitHub</span>
            <Value text={formatSyncValue(doc, c.field, c.tracker)} />
          </div>
        </>
      ) : (
        <>
          <div className="flow-review__state">{what}</div>
          {c.kind === "invalid_on_tracker" && (
            <div className="flow-review__why">{invalidRule(doc, c)}</div>
          )}
        </>
      )}
      {c.stale ? (
        <div className="flow-review__stale">Changed since the sync</div>
      ) : (
        <div
          className="flow-review__choices"
          role="radiogroup"
          aria-label={`${c.itemName}: ${what}`}
        >
          {c.choices.map((option) => (
            <label key={option} className="flow-radio">
              <input
                type="radio"
                name={`flow-choice-${c.id}`}
                value={option}
                checked={choice === option}
                onChange={() => onChoose(option)}
              />
              {CHOICE_WORDS[option]}
            </label>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * FSR-FR-CSOT / FSR-FR-KDMM: a skipped issue. It links to the tracker through
 * its URL, and an item whose issue was not created has neither number nor URL.
 */
function SkippedRow({ issue, onOpenLink }: { issue: SkippedIssue; onOpenLink: (url: string) => void }) {
  const url = issue.url;
  const label = issue.number !== null ? `#${issue.number} ${issue.title}` : issue.title;
  return (
    <li className="flow-review__skipped">
      {url ? (
        <button
          type="button"
          className="flow-review__link"
          aria-label={`Open ${label} on GitHub`}
          onClick={() => onOpenLink(url)}
        >
          {label} <span aria-hidden="true">↗</span>
        </button>
      ) : (
        <span className="flow-review__skipped-title">{label}</span>
      )}
      <span className="flow-review__reason">{SKIP_REASONS[issue.reason] ?? issue.reason}</span>
    </li>
  );
}

/**
 * `FSR-flow-sync-review.md`: the sync review. A panel at the leading side of
 * the Flow tab, over the canvas, and not a floating overlay of the window. It
 * lists what the last sync or resolve could not decide, takes one choice per
 * row, and applies the chosen rows in one call.
 */
export function SyncReview({
  doc,
  sync,
  onClose,
  onApply,
  onSyncAgain,
  onSelectItem,
  onOpenLink,
}: SyncReviewProps) {
  const [choices, setChoices] = useState<Record<string, SyncChoice>>({});
  const rootRef = useRef<HTMLElement>(null);
  const running = sync.running !== null;

  // FSR-FR-AUWG: a new list starts with no choice made.
  useEffect(() => setChoices({}), [sync.conflicts]);
  // FSR-FR-MMVF: the panel takes focus when it opens.
  useEffect(() => rootRef.current?.focus(), []);

  const open = sync.conflicts.filter((c) => !c.stale);
  const chosen = open.filter((c) => choices[c.id] !== undefined);
  const stale = sync.conflicts.some((c) => c.stale);
  const skipped = sync.skipped;
  const inSync = sync.settled && sync.conflicts.length === 0 && skipped.length === 0;
  const summary = [
    `${sync.conflicts.length} ${sync.conflicts.length === 1 ? "conflict" : "conflicts"}`,
    `${skipped.length} skipped`,
  ].join(" · ");

  return (
    <section
      ref={rootRef}
      className="flow-review"
      aria-label="Sync review"
      tabIndex={-1}
      data-testid="flow-sync-review"
      onKeyDown={(e) => {
        // FSR-FR-ODJL: Escape closes the panel while focus is inside it.
        if (e.key === "Escape") {
          e.stopPropagation();
          onClose();
        }
      }}
    >
      <header className="flow-review__head">
        <h2 className="flow-review__title">Sync review</h2>
        <button type="button" className="flow-review__close" aria-label="Close sync review" onClick={onClose}>
          ×
        </button>
      </header>
      {!inSync && <div className="flow-review__sum">{summary}</div>}
      <div className="flow-review__body">
        {inSync && (
          <p className="flow-review__done" role="status">
            <span aria-hidden="true">✓</span> Everything is in sync
          </p>
        )}
        {groupConflicts(doc, sync.conflicts).map(([itemId, list]) => {
          const item = findItem(doc, itemId);
          const name = item?.name ?? list[0].itemName;
          const link = syncLinkOf(doc, itemId);
          const url = list.find((c) => c.url)?.url ?? link?.ref.url ?? null;
          const tag = link ? tagText(link) : "GitHub";
          return (
            <div className="flow-review__group" key={itemId} data-testid={`flow-review-group-${itemId}`}>
              <div className="flow-review__ghead">
                <span className="flow-glyph" aria-hidden="true">
                  {item ? kindGlyph(item.kind) : "•"}
                </span>
                {item ? (
                  <button
                    type="button"
                    className="flow-review__item"
                    aria-label={`Show ${name} on the canvas`}
                    onClick={() => onSelectItem(itemId)}
                  >
                    {name}
                  </button>
                ) : (
                  <span className="flow-review__item flow-review__item--gone">{name}</span>
                )}
                {url ? (
                  <button
                    type="button"
                    className="flow-tag flow-review__tag"
                    aria-label={`Open ${tag} of ${name} on GitHub`}
                    onClick={() => onOpenLink(url)}
                  >
                    {tag} <span aria-hidden="true">↗</span>
                  </button>
                ) : (
                  link && <span className="flow-tag flow-review__tag">{tag}</span>
                )}
              </div>
              {list.map((c) => (
                <ConflictRow
                  key={c.id}
                  doc={doc}
                  conflict={c}
                  choice={choices[c.id]}
                  onChoose={(choice) => setChoices((prev) => ({ ...prev, [c.id]: choice }))}
                />
              ))}
            </div>
          );
        })}
        {stale && (
          <div className="flow-review__again">
            <span>Some conflicts changed since the sync.</span>
            <button type="button" className="flow-btn flow-btn--xs" disabled={running} onClick={onSyncAgain}>
              Sync again
            </button>
          </div>
        )}
        {skipped.length > 0 && (
          <div className="flow-review__skips">
            <h3 className="flow-review__label">Skipped</h3>
            <ul className="flow-review__list">
              {skipped.map((s, n) => (
                <SkippedRow key={`${s.number ?? "x"}-${n}`} issue={s} onOpenLink={onOpenLink} />
              ))}
            </ul>
          </div>
        )}
      </div>
      {sync.applyError && (
        <div className="flow-review__error" role="alert">
          {sync.applyError}
        </div>
      )}
      <footer className="flow-review__foot">
        <button type="button" className="flow-btn" onClick={onClose}>
          Close
        </button>
        <button
          type="button"
          className="flow-btn flow-btn--primary"
          aria-busy={sync.running === "resolve"}
          disabled={chosen.length === 0 || running}
          onClick={() => {
            void onApply(chosen.map((c) => ({ conflictId: c.id, choice: choices[c.id] })));
          }}
        >
          {sync.running === "resolve" ? "Applying…" : "Apply"}
        </button>
      </footer>
    </section>
  );
}

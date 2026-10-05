import { useEffect, useRef, type ReactNode } from "react";
import { Icon } from "./icons";
import { LINE_ENDING_CHOICES } from "../hooks/useLineEndings";
import {
  indentationLabel,
  SPACE_WIDTHS,
  type Indentation,
} from "../state/indentation";
import { actionableName, destinationOf } from "../state/operationActivation";
import type { DiffTotals, LineEndings, Operation } from "../types";

/**
 * STB-FR-HJVM: how many overlay rows show at once. The row area is exactly this
 * tall when more operations are in flight, and the rows scroll inside it. The
 * stylesheet holds the same number (`.status-bar__overlay-rows`).
 */
export const OVERLAY_VISIBLE_ROWS = 5;

/**
 * STB-status-bar.md — the persistent full-width strip along the bottom edge of
 * the main window (STB-FR-01).
 *
 * Three regions, each anchored independently to its own edge or to the strip's
 * centre (STB-FR-03), so an empty centre never lets the trailing controls drift:
 *
 *  - **leading** — the Global settings and Project settings buttons. Each opens
 *    its native settings **child window** (per `SWN-settings-windows.md`
 *    SWN-FR-01) and creates no tab; together with the application menu's two
 *    entries they are how either window is opened from the shell (STB-FR-04,
 *    SWN-FR-13).
 *  - **centre** — the most recently started in-flight operation, or nothing at
 *    all when none is running (STB-FR-05–STB-FR-09).
 *  - **trailing** — the project's line-ending convention, the active artifact's
 *    indentation, and the uncommitted diff summary, in that order with the
 *    diffstat closest to the trailing edge (STB-FR-16–STB-FR-30).
 *
 * The strip exposes no control to hide, collapse, or resize it, and nothing
 * about it is persisted (STB-FR-02 / SNV-FR-42).
 */
export interface StatusBarProps {
  /** STB-FR-04: opens, or focuses, the Global settings child window. */
  onGlobalSettings: () => void;
  /** STB-FR-04: opens, or focuses, the Project settings child window. */
  onSettings: () => void;

  // --- progress (STB-FR-05–STB-FR-15) ---
  operations: Operation[];
  overlayOpen: boolean;
  onOpenOverlay: () => void;
  onCloseOverlay: () => void;
  /**
   * STB-FR-RWPD: open the surface that owns an operation's target. Called only
   * for a row whose operation carries a destination, after the overlay closed.
   */
  onActivateOperation: (operation: Operation) => void;

  // --- line endings (STB-FR-16–STB-FR-20) ---
  lineEndings: LineEndings | null;
  onSelectLineEndings: (value: LineEndings) => void;

  /**
   * STB-FR-21 / STB-FR-22: the active Editor tab's artifact convention, or
   * `null` when the active tab is not an Editor tab (or none is active) — in
   * which case the control renders a neutral disabled state naming no
   * convention, because there is no artifact for it to describe.
   */
  indentation: Indentation | null;
  onSelectIndentation: (value: Indentation) => void;

  /** STB-FR-25 / STB-FR-29: the totals, or null to render nothing in their place. */
  diffTotals: DiffTotals | null;
}

export function StatusBar({
  onGlobalSettings,
  onSettings,
  operations,
  overlayOpen,
  onOpenOverlay,
  onCloseOverlay,
  onActivateOperation,
  lineEndings,
  onSelectLineEndings,
  indentation,
  onSelectIndentation,
  diffTotals,
}: StatusBarProps) {
  return (
    <div className="status-bar" data-testid="status-bar">
      <div className="status-bar__leading">
        {/* STB-FR-04: graphical icon controls. Each opens its settings child
            window, focuses that window when it is already open, and closes the
            other one first when that one is (SWN-FR-05 … SWN-FR-07) — all of
            which is the backend's single decision, so these only ask. Neither
            creates a tab, and the activity bar carries no settings control
            (SNV-FR-03). */}
        <button
          className="status-bar__btn"
          onClick={onGlobalSettings}
          title="Global settings"
          aria-label="Global settings"
        >
          <Icon.Boxes size={14} />
        </button>
        <button
          className="status-bar__btn"
          onClick={onSettings}
          title="Project settings"
          aria-label="Project settings"
        >
          <Icon.Settings size={14} />
        </button>
      </div>

      <ProgressRegion
        operations={operations}
        overlayOpen={overlayOpen}
        onOpenOverlay={onOpenOverlay}
        onCloseOverlay={onCloseOverlay}
        onActivateOperation={onActivateOperation}
      />

      <div className="status-bar__trailing">
        <LineEndingControl value={lineEndings} onSelect={onSelectLineEndings} />
        <IndentationControl
          value={indentation}
          onSelect={onSelectIndentation}
        />
        <DiffSummary totals={diffTotals} />
      </div>
    </div>
  );
}

// ---------------------------------------------------------------------------
// Centre region — progress (STB-FR-05–STB-FR-15)
// ---------------------------------------------------------------------------

interface ProgressRegionProps {
  operations: Operation[];
  overlayOpen: boolean;
  onOpenOverlay: () => void;
  onCloseOverlay: () => void;
  onActivateOperation: (operation: Operation) => void;
}

function ProgressRegion({
  operations,
  overlayOpen,
  onOpenOverlay,
  onCloseOverlay,
  onActivateOperation,
}: ProgressRegionProps) {
  // STB-FR-05 / STB-FR-06: the most recently started of the operations
  // currently in flight. The list arrives already ordered (PRG-FR-02), so the
  // "fall back as each terminates" rule needs no bookkeeping of its own —
  // whoever is at the head is what renders.
  const displayed = operations[0];

  /**
   * STB-FR-14: a pointer-down outside both the overlay and the centre region
   * dismisses it. The ref wraps both, so a click on a row or on the bar itself
   * is "inside" (the bar's own click toggles, handled below).
   */
  const regionRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!overlayOpen) return;
    const onDown = (e: MouseEvent) => {
      if (regionRef.current && !regionRef.current.contains(e.target as Node)) {
        onCloseOverlay();
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCloseOverlay();
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [overlayOpen, onCloseOverlay]);

  // STB-FR-14: the overlay is anchored to the centre region, so when the last
  // in-flight operation terminates the region it hangs from is no longer
  // rendered (STB-FR-07) and it closes on its own.
  useEffect(() => {
    if (overlayOpen && !displayed) onCloseOverlay();
  }, [overlayOpen, displayed, onCloseOverlay]);

  // STB-FR-07: with nothing in flight the region renders nothing at all — no
  // track, no placeholder, no idle label — and is not a click target. The empty
  // div keeps the three-column anchoring (STB-FR-03) without being one.
  if (!displayed) {
    return <div className="status-bar__center" data-testid="status-bar-center" />;
  }

  return (
    <div
      className="status-bar__center"
      data-testid="status-bar-center"
      ref={regionRef}
    >
      <button
        type="button"
        className="status-bar__progress"
        data-testid="status-bar-progress"
        aria-label={`${displayed.label} — show in-flight operations`}
        aria-expanded={overlayOpen}
        // STB-FR-10: clicking opens the overlay; STB-FR-14: clicking again
        // closes it.
        onClick={() => (overlayOpen ? onCloseOverlay() : onOpenOverlay())}
      >
        <ProgressBar operation={displayed} />
      </button>
      {overlayOpen && (
        <InFlightOverlay
          operations={operations}
          // STB-FR-RWPD: the overlay closes, and then the owning surface opens.
          onActivate={(operation) => {
            onCloseOverlay();
            onActivateOperation(operation);
          }}
        />
      )}
    </div>
  );
}

/**
 * STB-FR-08 / STB-FR-09: an operation's label beside its bar. A reported total
 * renders as a determinate bar filled to the proportion completed; no total
 * renders as an indeterminate busy state. An operation whose `kind` the UI does
 * not recognise gets exactly this treatment too — nothing here branches on
 * `kind` at all (PRG-FR-12).
 */
function ProgressBar({
  operation,
  renderLabel,
}: {
  operation: Operation;
  /** STB-FR-RWPD: an actionable overlay row supplies its label as a control. */
  renderLabel?: (label: string) => ReactNode;
}) {
  const determinate =
    operation.total !== undefined &&
    operation.total > 0 &&
    operation.completed !== undefined;
  const fraction = determinate
    ? Math.min(1, Math.max(0, operation.completed! / operation.total!))
    : 0;

  return (
    <>
      {renderLabel ? (
        <span className="status-bar__progress-label">
          {renderLabel(operation.label)}
        </span>
      ) : (
        <span className="status-bar__progress-label" title={operation.label}>
          {operation.label}
        </span>
      )}
      <span
        className="status-bar__track"
        data-testid="progress-track"
        // STB-FR-08: the same element switches rendering in place when a total
        // arrives, rather than being replaced by a new one — React keeps the
        // node and only its data attribute and width change.
        data-determinate={determinate}
        role="progressbar"
        aria-valuemin={determinate ? 0 : undefined}
        aria-valuemax={determinate ? operation.total : undefined}
        aria-valuenow={determinate ? operation.completed : undefined}
      >
        <span
          className="status-bar__fill"
          style={determinate ? { width: `${fraction * 100}%` } : undefined}
        />
      </span>
    </>
  );
}

/**
 * STB-FR-11 / STB-FR-HJVM: every operation currently in flight, most-recently
 * started first, each with its own label and bar. At most
 * {@link OVERLAY_VISIBLE_ROWS} rows show at once; more scroll vertically.
 *
 * STB-FR-13: it offers no control to start, cancel, retry, or dismiss anything;
 * cancelling belongs to the surface that owns the operation (RUN-FR-05).
 */
function InFlightOverlay({
  operations,
  onActivate,
}: {
  operations: Operation[];
  onActivate: (operation: Operation) => void;
}) {
  const overflowing = operations.length > OVERLAY_VISIBLE_ROWS;
  return (
    <div
      className="status-bar__overlay"
      data-testid="in-flight-overlay"
      role="dialog"
      aria-label="In-flight operations"
    >
      <div className="status-bar__overlay-title">In flight</div>
      <div
        className="status-bar__overlay-rows"
        data-testid="in-flight-rows"
        data-visible-rows={OVERLAY_VISIBLE_ROWS}
        data-overflowing={overflowing}
        role="list"
        aria-label="In-flight operations"
        // A region that scrolls takes a focus stop, so the keyboard reaches the
        // rows below the fifth when none of the rows above is a control.
        tabIndex={overflowing ? 0 : undefined}
      >
        {operations.map((operation) => (
          <OverlayRow
            key={operation.id}
            operation={operation}
            onActivate={onActivate}
          />
        ))}
      </div>
    </div>
  );
}

/**
 * STB-FR-RWPD / STB-FR-DNLC / STB-FR-YQFE: a row whose operation carries a
 * destination is a control named by its label and its target. A row without one
 * is plain text — not a button, not a disabled button — and takes no focus stop.
 *
 * The control is the label; the bar stays a sibling so it keeps its progressbar
 * semantics, and the label's pointer area covers the whole row.
 */
function OverlayRow({
  operation,
  onActivate,
}: {
  operation: Operation;
  onActivate: (operation: Operation) => void;
}) {
  const destination = destinationOf(operation);
  if (!destination) {
    return (
      <div
        className="status-bar__overlay-row"
        role="listitem"
        data-actionable="false"
      >
        <ProgressBar operation={operation} />
      </div>
    );
  }
  return (
    <div
      className="status-bar__overlay-row status-bar__overlay-row--action"
      role="listitem"
      data-actionable="true"
    >
      <ProgressBar
        operation={operation}
        renderLabel={(label) => (
          <button
            type="button"
            className="status-bar__overlay-action"
            aria-label={actionableName(operation, destination)}
            title={label}
            onClick={() => onActivate(operation)}
          >
            {label}
          </button>
        )}
      />
    </div>
  );
}

// ---------------------------------------------------------------------------
// Trailing region (STB-FR-16–STB-FR-30)
// ---------------------------------------------------------------------------

/**
 * STB-FR-16 / STB-FR-17 / STB-FR-20: the project's line-ending convention.
 * Present and enabled whatever the active tab is, because the convention is a
 * property of the project rather than of a tab.
 */
function LineEndingControl({
  value,
  onSelect,
}: {
  value: LineEndings | null;
  onSelect: (value: LineEndings) => void;
}) {
  if (!value) return null;
  return (
    <select
      className="status-bar__select"
      data-testid="line-ending-select"
      aria-label="Line endings"
      title="Line endings — every artifact write uses this"
      value={value}
      onChange={(e) => onSelect(e.target.value as LineEndings)}
    >
      {LINE_ENDING_CHOICES.map(([choice, label]) => (
        <option key={choice} value={choice}>
          {label}
        </option>
      ))}
    </select>
  );
}

/** Serialised form of an `Indentation`, so it can ride on a `<select>` value. */
function indentKey(indentation: Indentation): string {
  return indentation.kind === "tabs"
    ? "tabs"
    : `spaces-${indentation.width}`;
}

/**
 * The options to offer while `current` is the artifact's convention.
 *
 * `SPACE_WIDTHS` are the widths a user can *choose*, but detection reports the
 * artifact's real unit (EDT-FR-37) and that need not be one of them — a
 * CommonMark ordered list whose continuation lines align under `"1. "` is
 * indented three spaces, and one stray single-space line is enough to make the
 * unit 1. STB-FR-21 requires the control to describe the artifact, so a detected
 * width outside the offered set joins the list rather than leaving a controlled
 * `<select>` matching no option and rendering blank.
 */
function indentChoicesFor(current: Indentation): [string, Indentation][] {
  const widths = new Set<number>(SPACE_WIDTHS);
  if (current.kind === "spaces") widths.add(current.width);
  return [
    ["tabs", { kind: "tabs" }],
    ...[...widths]
      .sort((a, b) => a - b)
      .map(
        (width) =>
          [`spaces-${width}`, { kind: "spaces", width }] as [
            string,
            Indentation,
          ],
      ),
  ];
}

/**
 * STB-FR-21–STB-FR-24: the active artifact's indentation convention.
 *
 * STB-FR-22: when the active tab is editing no artifact — Dashboard, Flow,
 * Search results, History detail, and a Diff tab whose comparison deletes the
 * file — or when no tab is active, this renders a neutral disabled state naming
 * no convention, because there is no artifact for it to describe. Neither
 * settings surface is among them: each is a child window rather than a tab of
 * this window (per `SWN-settings-windows.md` SWN-FR-01).
 */
function IndentationControl({
  value,
  onSelect,
}: {
  value: Indentation | null;
  onSelect: (value: Indentation) => void;
}) {
  if (!value) {
    return (
      <span
        className="status-bar__inert"
        data-testid="indentation-inert"
        aria-label="Indentation"
        aria-disabled="true"
      >
        Indentation
      </span>
    );
  }
  const choices = indentChoicesFor(value);
  return (
    <select
      className="status-bar__select"
      data-testid="indentation-select"
      aria-label="Indentation"
      title={`Indentation — ${indentationLabel(value)}`}
      value={indentKey(value)}
      onChange={(e) => {
        const chosen = choices.find(([key]) => key === e.target.value);
        if (chosen) onSelect(chosen[1]);
      }}
    >
      {choices.map(([key, choice]) => (
        <option key={key} value={key}>
          {indentationLabel(choice)}
        </option>
      ))}
    </select>
  );
}

/**
 * STB-FR-25 / STB-FR-28 / STB-FR-29: the uncommitted added/removed line totals.
 *
 * `null` renders nothing at all rather than zeros or an error message — the
 * not-a-repository case (STB-FR-29) and the not-yet-loaded case both look like
 * a strip with no diffstat, which is the honest rendering of "there is no
 * comparison here". Zeros are a real answer and do render.
 *
 * Binary entries contribute to neither total; that summation happens in the
 * backend, which never fabricates counts for them (STB-FR-28 / CHC-FR-08).
 */
function DiffSummary({ totals }: { totals: DiffTotals | null }) {
  if (!totals) return null;
  return (
    <span className="status-bar__diffstat" data-testid="status-bar-diffstat">
      <span className="status-bar__added">+{totals.addedLines}</span>
      <span className="status-bar__removed">−{totals.removedLines}</span>
    </span>
  );
}

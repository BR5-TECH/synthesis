/**
 * DDS-FR-CTXI … DDS-FR-HJMF: the context indicator at the trailing end of the
 * discussion column's header, and the warning beside it.
 *
 * The indicator says how much of the model's context window a turn uses. The
 * warning stands only while the document under discussion is above the size
 * limit, because only the author can split or shorten it. Neither is a control:
 * they block nothing, and they open their tooltips on hover and on keyboard
 * focus.
 */
import { Icon } from "../icons";
import { Tooltip } from "../Tooltip";
import {
  fullTokens,
  indicatorName,
  indicatorText,
  type ShownContext,
} from "./contextFigures";

/** The rows of the split tooltip, in the order DDS-FR-WRNB names them. */
const SPLIT_ROWS = [
  ["Instructions", "instructions"],
  ["Document", "document"],
  ["Discussion history", "history"],
  ["Current comment", "currentComment"],
  ["Tool results", "toolResults"],
] as const;

/** The ring is drawn on a 16-unit box; the stroke runs on this radius. */
const RING_R = 6;
const RING_C = 2 * Math.PI * RING_R;

/** A small ring that fills with the share of the window. Decoration only. */
function Ring({ percent }: { percent: number }) {
  const filled = Math.min(Math.max(percent, 0), 100) / 100;
  return (
    <svg className="dds-context__ring" viewBox="0 0 16 16" aria-hidden="true">
      <circle className="dds-context__track" cx="8" cy="8" r={RING_R} />
      <circle
        className="dds-context__fill"
        cx="8"
        cy="8"
        r={RING_R}
        strokeDasharray={`${filled * RING_C} ${RING_C}`}
        transform="rotate(-90 8 8)"
      />
    </svg>
  );
}

function SplitTip({ shown }: { shown: ShownContext }) {
  return (
    <span className="dds-context-tip" data-testid="context-indicator-tip">
      <span className="dds-context-tip__head">
        <span>Context</span>
        <span className="dds-context-tip__num">
          {shown.window === null
            ? `${shown.estimate ? "~" : ""}${fullTokens(shown.tokens)} tokens`
            : `${shown.estimate ? "~" : ""}${fullTokens(shown.tokens)} of ${fullTokens(shown.window)}`}
        </span>
      </span>
      {SPLIT_ROWS.map(([label, key]) => (
        <span className="dds-context-tip__row" key={key}>
          <span>{label}</span>
          <span className="dds-context-tip__num">~{fullTokens(shown.split[key])} est.</span>
        </span>
      ))}
      <span className="dds-context-tip__foot">
        {shown.window === null
          ? "The model's context window is not known."
          : `Window ${fullTokens(shown.window)} tokens.`}
      </span>
    </span>
  );
}

function WarningTip({ shown }: { shown: ShownContext }) {
  const limit = shown.limitTokens ?? 0;
  const share =
    shown.window && shown.window > 0 ? Math.round((limit * 100) / shown.window) : null;
  return (
    <span className="dds-context-tip" data-testid="context-warning-tip">
      <span className="dds-context-tip__row">
        <span>Document</span>
        <span className="dds-context-tip__num">
          ~{fullTokens(shown.document?.estimatedTokens ?? 0)} tokens est.
        </span>
      </span>
      <span className="dds-context-tip__row">
        <span>Limit</span>
        <span className="dds-context-tip__num">
          {fullTokens(limit)}
          {share === null ? "" : ` (${share}% of window)`}
        </span>
      </span>
      <span className="dds-context-tip__foot">
        The agent gets only part of this document. Split or shorten the document.
      </span>
    </span>
  );
}

/**
 * DDS-FR-WRNB: whether the discussion menu beside the indicator is open. Its
 * tooltips stay closed while it is, so neither covers the menu's items.
 */
function menuOpen(anchor: HTMLElement): boolean {
  return (
    anchor.closest("header")?.querySelector('.dds-discussion__actions [aria-expanded="true"]') !=
    null
  );
}

export function ContextIndicator({ shown }: { shown: ShownContext | null }) {
  if (shown === null) return null;
  const name = indicatorName(shown);
  return (
    <span className="dds-context" data-testid="context-indicator-group">
      <Tooltip label={name} side="below" content={<SplitTip shown={shown} />} blocked={menuOpen}>
        {(bind) => (
          <span
            {...bind}
            // A meter where a share is known; where it is not, the name alone
            // states the value.
            role={shown.percent === null ? "img" : "meter"}
            aria-valuemin={shown.percent === null ? undefined : 0}
            aria-valuemax={shown.percent === null ? undefined : 100}
            aria-valuenow={shown.percent === null ? undefined : shown.percent}
            tabIndex={0}
            className="dds-context__value"
            data-estimate={shown.estimate || undefined}
            data-testid="context-indicator"
          >
            {shown.percent !== null && <Ring percent={shown.percent} />}
            {/* In a narrow column the ring stands alone; the accessible name
                still states the value (DDS-FR-CTXI). */}
            <span className="dds-context__text">{indicatorText(shown)}</span>
          </span>
        )}
      </Tooltip>
      {shown.aboveLimit && (
        <Tooltip
          label="The document is above the size limit"
          side="below"
          blocked={menuOpen}
          content={<WarningTip shown={shown} />}
        >
          {(bind) => (
            <span
              {...bind}
              role="img"
              tabIndex={0}
              className="dds-context__warning"
              data-testid="context-warning"
            >
              <Icon.AlertTriangle size={12} />
            </span>
          )}
        </Tooltip>
      )}
    </span>
  );
}

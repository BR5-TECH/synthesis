/**
 * DDS-FR-CTXI … DDS-FR-HJMF: the context indicator at the trailing end of the
 * discussion column's header, and the warning beside it.
 *
 * The indicator says how much of the model's context window a turn uses, as a
 * percent with a small ring whose tone shows the level (DDS-FR-WUCD). The
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
  levelLabel,
  ringFill,
  type ContextLevel,
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

/**
 * DDS-FR-WCET / DDS-FR-WUCD: a small ring that fills with the share of the
 * window, full at 100% and above, in the tone of its level. Decoration only:
 * the percent text and the accessible name carry the value and the level.
 */
function Ring({ percent, level }: { percent: number; level: ContextLevel }) {
  const filled = ringFill(percent);
  return (
    <svg
      className="dds-context__ring"
      viewBox="0 0 16 16"
      aria-hidden="true"
      data-level={level}
      data-testid="context-indicator-ring"
    >
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
          {`${shown.estimate ? "~" : ""}${fullTokens(shown.tokens)} of ${fullTokens(shown.window)}`}
        </span>
      </span>
      {SPLIT_ROWS.map(([label, key]) => (
        <span className="dds-context-tip__row" key={key}>
          <span>{label}</span>
          <span className="dds-context-tip__num">~{fullTokens(shown.split[key])} est.</span>
        </span>
      ))}
      <span className="dds-context-tip__foot">
        {`Window ${fullTokens(shown.window)} tokens. ${levelLabel(shown.level)}.`}
      </span>
    </span>
  );
}

function WarningTip({ shown }: { shown: ShownContext }) {
  const limit = shown.limitTokens;
  const share = shown.window > 0 ? Math.round((limit * 100) / shown.window) : null;
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
            role="meter"
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.min(shown.percent, 100)}
            aria-valuetext={`${shown.percent}%, level ${shown.level}`}
            tabIndex={0}
            className="dds-context__value"
            data-estimate={shown.estimate || undefined}
            data-level={shown.level}
            data-testid="context-indicator"
          >
            <Ring percent={shown.percent} level={shown.level} />
            {/* DDS-FR-WRNB: the percent stays visible at every width, so the
                colour of the ring is never the only carrier of the level. */}
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

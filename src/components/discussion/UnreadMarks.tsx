/**
 * CVP-FR-TWRL / CVP-FR-44: where the author stopped reading, and the way to it.
 *
 * Both are text as well as shape: the divider names how many messages are new
 * and the indicator is a labelled button, so neither depends on a colour.
 */
import { Icon } from "../icons";

/** Drawn above the first message that arrived while the author was reading back. */
export function UnreadDivider({ count }: { count: number }) {
  return (
    <div
      className="dds-unread"
      role="separator"
      aria-label={`${count} unread`}
      data-testid="discussion-unread-divider"
    >
      <span className="dds-unread__rule" aria-hidden="true" />
      <span className="dds-unread__count t-ui-xs">{count} new</span>
      <span className="dds-unread__rule" aria-hidden="true" />
    </div>
  );
}

function label(count: number): string {
  return count === 1 ? "1 new message" : `${count} new messages`;
}

/**
 * The unread indicator: a control naming the number of new messages that scrolls
 * to the first of them when activated.
 *
 * It stands inside a polite status region, so the arrival is announced without
 * taking focus (CVP-FR-43). The region is always present and the button is
 * rendered only while something is unread.
 */
export function UnreadIndicator({
  count,
  onActivate,
}: {
  count: number;
  onActivate: () => void;
}) {
  return (
    <div className="discussion-unread" role="status" data-testid="discussion-unread-status">
      {count > 0 && (
        <button
          type="button"
          className="discussion-unread__button t-ui-xs"
          data-testid="discussion-unread-indicator"
          aria-label={`${label(count)}, go to the first unread message`}
          onClick={onActivate}
        >
          {label(count)}
          <Icon.Caret size={12} />
        </button>
      )}
    </div>
  );
}

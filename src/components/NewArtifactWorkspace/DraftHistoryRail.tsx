/**
 * The draft's **History** rail (`../../../specifications/ui/NAW-new-artifact.md`
 * NAW-FR-07, NAW-FR-36 … NAW-FR-42).
 *
 * A way of *reading* what the prompt has been, and nothing else: no control
 * here creates, deletes, restores, renames, or edits anything. It is bounded so
 * the two columns beside it keep a legible measure at the minimum window size
 * the shell supports, and when it is hidden nothing of it remains — the whole
 * rail goes rather than merely its list.
 */
import { liveMarkerFor, sourceLabelOf, standingOf, timestampOf } from "./labels";
import type { DraftHistoryEntry, DraftHistoryList } from "../../types";

export function DraftHistoryRail({
  history,
  entries,
  error,
  viewing,
  liveRowRef,
  onRetry,
  onBackToLive,
  onRead,
}: {
  history: DraftHistoryList | null;
  /** DHS-FR-05: the versions, oldest first, as the backend returned them. */
  entries: DraftHistoryEntry[];
  /** NAW-FR-41: why the History could not be read, when it could not. */
  error: string | null;
  /** The version being read, or `null` while the live prompt is. */
  viewing: DraftHistoryEntry | null;
  liveRowRef: React.RefObject<HTMLButtonElement | null>;
  onRetry: () => void;
  onBackToLive: () => void;
  onRead: (entry: DraftHistoryEntry) => void;
}) {
  const liveMarker = liveMarkerFor(history);
  return (

          <div className="draft-rail" aria-label="History">
            <div className="draft-rail__header t-ui-xs">
              <span className="draft-rail__title">History</span>
            </div>

            {/* NAW-FR-36: the rail says in words what it records, so an author
                is never left to infer from an unchanging list that their typing
                failed to save. */}
            <p className="draft-rail__legend t-ui-xs">
              The prompt as it stood before and after each change you accepted
              from an agent. Your own edits are the live prompt, which is the
              Original until the first change lands.
            </p>

            {/* NAW-FR-41: a history that could not be reconciled renders its own
                recoverable state, and the prompt is shown as neither current nor
                stale until it resolves. */}
            {error !== null && (
              <div className="draft-rail__error t-ui-xs" role="alert">
                <p>{error}</p>
                <button className="btn btn--sm" onClick={onRetry}>
                  Try again
                </button>
              </div>
            )}

            {/* NAW-FR-37 / NAW-FR-39: the live-prompt row, pinned at the head
                and rendered throughout — including while a version is being
                read, so the author can see at a glance both what they are
                reading and what the prompt now says. */}
            <button
              className={
                viewing === null
                  ? "draft-version draft-version--live draft-version--reading"
                  : "draft-version draft-version--live"
              }
              ref={liveRowRef}
              aria-current={viewing === null ? "true" : undefined}
              onClick={onBackToLive}
            >
              <span className="draft-version__marker" aria-hidden="true">
                {viewing === null ? "▸" : " "}
              </span>
              <span className="draft-version__body">
                <span className="draft-version__source">Live prompt</span>
                {/* NAW-FR-38: a draft nobody has proposed a change to holds one
                    version and it is this row — the `Original`, which stays the
                    live prompt until an acceptance supersedes it. */}
                {history !== null && entries.length === 0 && (
                  <span className="draft-version__standing">Original</span>
                )}
                {liveMarker !== null && (
                  <span className="draft-version__standing" role="status">
                    {liveMarker}
                  </span>
                )}
              </span>
            </button>

            {/* The divider and the column below it stand only where there is
                something in them: a draft whose prompt has never been superseded
                has no past to head, and an empty list under a heading reads as a
                history that failed to load. */}
            {entries.length > 0 && (
              <div className="draft-rail__divider t-ui-xs" aria-hidden="true">
                versions
              </div>
            )}

            {/* NAW-FR-07: every version, **newest first**, so the version the
                prompt most recently settled on sits directly under the live
                prompt it is compared against and the `Original` closes the
                column. The list arrives oldest-first (DHS-FR-05); the order is
                reversed here rather than there, because ascending `seq` is what
                makes the backend's answer a total order and this is a fact about
                where a reader's eye starts.

                No control here creates, deletes, restores, renames, or edits
                anything: the rail is a way of reading what the prompt has
                been. */}
            <div className="draft-rail__versions">
              {[...entries].reverse().map((entry) => {
                const stamp = timestampOf(entry.createdAt);
                const reading = viewing?.id === entry.id;
                return (
                  <button
                    key={entry.id}
                    className={
                      reading
                        ? "draft-version draft-version--reading"
                        : "draft-version"
                    }
                    aria-current={reading ? "true" : undefined}
                    onClick={() => onRead(entry)}
                  >
                    <span className="draft-version__marker" aria-hidden="true">
                      {reading ? "▸" : " "}
                    </span>
                    <span className="draft-version__body">
                      <span className="draft-version__source">
                        {sourceLabelOf(entry)}
                      </span>
                      <span className="draft-version__standing">
                        {standingOf(entry, entries)}
                      </span>
                      <span
                        className="draft-version__at"
                        title={stamp.absolute}
                      >
                        {stamp.short}
                      </span>
                    </span>
                  </button>
                );
              })}
            </div>
          </div>
  );
}

/**
 * One removed stream, in the listing
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-RPYH,
 * WSS-FR-OSVE).
 */
import type { RemovedStreamNotice } from "../../types";

export interface RemovedNoticeProps {
  notice: RemovedStreamNotice;
  onDismiss: (streamId: string) => void;
}

export function RemovedNotice({ notice, onDismiss }: RemovedNoticeProps) {
  return (
    <div
      className="stream-select__notice"
      role="status"
      data-testid={`stream-notice-${notice.streamId}`}
    >
      <p className="stream-select__notice-text">
        <span className="stream-select__name">{notice.name}</span> was removed
        because its working copy is gone.
        {notice.branchSurvives && (
          <>
            {" "}
            Its branch <code>{notice.branch}</code> stays.
          </>
        )}
      </p>
      <button
        type="button"
        className="btn btn--ghost btn--sm"
        aria-label={`Dismiss the notice about ${notice.name}`}
        data-testid={`stream-notice-dismiss-${notice.streamId}`}
        onClick={() => onDismiss(notice.streamId)}
      >
        Dismiss
      </button>
    </div>
  );
}

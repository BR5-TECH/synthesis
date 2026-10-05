import { canStartPush, pushUnavailableReason } from "../../gitSync";
import type { UpstreamSyncState } from "../../types";
import { Icon } from "../icons";

/**
 * GIT-FR-PZIE, GIT-FR-KLNK: the Logs section's large view. It holds the
 * section's **Pull** and **Push** controls and the push/pull output area, the
 * record of every transfer the author invokes anywhere in the application. A
 * push started here, from the Changes panel, or from the top chrome is one
 * operation and lands in the same transcript (GIT-FR-QMYB).
 */
export function LogsView({
  sync,
  transferring,
  lines,
  authNote,
  onPull,
  onPush,
}: {
  sync: UpstreamSyncState | null;
  transferring: boolean;
  lines: string[];
  authNote: string;
  onPull: () => void;
  onPush: () => void;
}) {
  return (
    <>
      <div className="git__right-head">
        <span className="t-eyebrow">PUSH / PULL OUTPUT</span>
        <span className="spacer" />
        <button
          className="btn btn--default btn--sm"
          disabled={transferring}
          onClick={onPull}
        >
          <Icon.GitPull size={12} /> Pull
        </button>
        {/* GTC-FR-21: nothing to publish means nothing to push. The Changes
            panel's footer answers to the same read, so the two never disagree
            about it. */}
        <button
          className="btn btn--default btn--sm"
          disabled={!canStartPush(sync, transferring)}
          title={pushUnavailableReason(sync, transferring) ?? undefined}
          onClick={onPush}
        >
          Push
        </button>
      </div>
      {authNote && (
        <div className="proj-switch__error" data-testid="git-auth-note">
          {authNote}
        </div>
      )}
      <div
        className="git__output"
        data-testid="git-transfer-output"
        aria-label="Push and pull output"
      >
        {lines.length === 0 ? (
          <div className="t-meta" style={{ padding: "6px 12px" }}>
            No transfers yet.
          </div>
        ) : (
          lines.map((line, i) => (
            <div key={i} className="git__output-line">
              {line}
            </div>
          ))
        )}
      </div>
    </>
  );
}

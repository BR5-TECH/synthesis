import type { ReactNode } from "react";

/**
 * GIT-FR-TFAU: a region's state in words. Loading and empty are announced as a
 * status; a failure is an alert and offers a retry when the region can read
 * again.
 */
export function RegionNote({
  kind,
  children,
  onRetry,
  retryLabel = "Retry",
  testId,
}: {
  kind: "loading" | "empty" | "error";
  children: ReactNode;
  onRetry?: () => void;
  retryLabel?: string;
  testId?: string;
}) {
  return (
    <div
      className={kind === "error" ? "git__state git__state--error" : "git__state"}
      role={kind === "error" ? "alert" : "status"}
      data-testid={testId}
    >
      <span>{children}</span>
      {kind === "error" && onRetry && (
        <button
          type="button"
          className="btn btn--default btn--sm"
          onClick={onRetry}
        >
          {retryLabel}
        </button>
      )}
    </div>
  );
}

/** The panel's mark for a selected row: words, never colour alone. */
export function SelectedMark() {
  return <span className="badge badge--accent">selected</span>;
}

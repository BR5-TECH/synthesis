/**
 * The toast of a failed startup refresh of the publication metadata
 * (`NTF-notifications.md` NTF-FR-POEW, served by
 * `../../specifications/core/GHP-github-publication.md` GHP-FR-ISJI and
 * GHP-FR-KPXF).
 *
 * The main window alone mounts this hook, so one failure shows one toast. The
 * backend keeps the failure until it is taken. The hook takes it once when it
 * starts to listen, and again each time the event comes, so a failure raised
 * before the listener existed still shows. A periodic or a later refresh
 * raises nothing.
 *
 * The hook listens only while the window shows an open project (`rootKey` is
 * not null). The opening of a project clears every toast (NTF-FR-15), so a
 * failure taken over the Project picker would be lost. The backend keeps it
 * until the window shows the project, and a worktree change listens again.
 */
import { useEffect } from "react";
import { takePublicationMetadataStartupFailure } from "../api/drafts";
import { onPublicationMetadataStartupFailed } from "../events";
import { logDebug, logWarn } from "../logging";
import { showToast } from "../state/toasts";

/** NTF-FR-POEW: the key of the toast. */
export const METADATA_STARTUP_FAILED_KEY = "publication-metadata-refresh-failed";
/** NTF-FR-POEW: the title of the toast. */
export const METADATA_STARTUP_FAILED_TITLE = "GitHub publication data not loaded";

export function usePublicationMetadataStartupToast(rootKey: string | null): void {
  useEffect(() => {
    if (rootKey === null) return;
    let cancelled = false;
    const take = async () => {
      try {
        const failure = await takePublicationMetadataStartupFailure();
        // The backend discarded the failure when it answered, so it shows
        // also where the window unmounted meanwhile. The toast store outlives
        // the hook, and the key keeps it to one toast.
        if (!failure) return;
        logWarn(["frontend", "remote"], "publication metadata startup refresh failed", {
          errorCode: failure.errorCode,
        });
        showToast({
          key: METADATA_STARTUP_FAILED_KEY,
          level: "Error",
          title: METADATA_STARTUP_FAILED_TITLE,
          body: failure.error,
          address: null,
        });
      } catch (err) {
        logDebug(["frontend", "remote"], "publication metadata startup failure not read", {
          error: String(err),
        });
      }
    };
    const subscription = onPublicationMetadataStartupFailed(() => {
      if (!cancelled) void take();
    });
    void subscription.then(() => {
      if (!cancelled) void take();
    });
    return () => {
      cancelled = true;
      void subscription.then((unlisten) => unlisten());
    };
  }, [rootKey]);
}

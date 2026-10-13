/**
 * The graduation run a draft is bound to (`NAW-new-artifact.md` NAW-FR-44,
 * NAW-FR-BJQX). Split out of `index.tsx` to keep that file within the size
 * rule.
 */
import { useCallback, useEffect, useState } from "react";

import * as api from "../../api";
import type { DraftGraduation } from "../../types";
import { onGraduationRunChanged } from "../../events";
import { isTerminalRun } from "../../state/graduation";

export function useDraftGraduation(draftId: string, draftsRevision: number) {
  /**
   * NAW-FR-44 / NAW-FR-BJQX: the graduation run this draft is bound to, read from
   * the queue rather than remembered here (DRS-FR-18). It is what makes the tab
   * read-only while a run holds the prompt, and read-only for good once the
   * draft is `graduated`.
   */
  const [graduation, setGraduation] = useState<DraftGraduation | null>(null);

  /**
   * NAW-FR-44 / DRS-FR-18: the draft's graduation run, re-read whenever
   * anything about drafts or about a run changes. Never cached beyond that: the
   * lock is the *run's* state, and a tab holding its own copy would go on
   * offering to edit a prompt a run had taken.
   */
  const reloadGraduation = useCallback(async () => {
    try {
      const run = await api.getDraftGraduation(draftId);
      setGraduation(
        run
          ? {
              runId: run.id,
              state: run.state,
              locked: !isTerminalRun(run.state),
              graduated: run.commits.length > 0 && run.state !== "discarded",
            }
          : null,
      );
    } catch {
      // No project, or no queue yet. Neither is a lock, and neither is
      // something to report over an editing surface.
      setGraduation(null);
    }
  }, [draftId]);

  useEffect(() => {
    void reloadGraduation();
  }, [reloadGraduation, draftsRevision]);

  useEffect(() => {
    let cancelled = false;
    const subscription = onGraduationRunChanged(() => {
      if (!cancelled) void reloadGraduation();
    });
    return () => {
      cancelled = true;
      void subscription.then((un: () => void) => un()).catch(() => {});
    };
  }, [reloadGraduation]);

  return { graduation, reloadGraduation };
}

import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../../api";
import { logDebug, logWarn } from "../../logging";
import type {
  PullRequestDetail,
  PullRequestListState,
  PullRequestSummary,
  PullRequestTimeline,
} from "../../types";
import { parseRejection } from "./errors";
import { withGithubToken } from "./githubToken";
import { useLoad } from "./useLoad";

/**
 * GIT-FR-OBZW through GIT-FR-LKRX: the PRs section's state.
 *
 * Every read reaches GitHub, so each answers a missing token on the terms of
 * GIT-FR-10: a selection-required failure opens the picker and the read runs
 * again on confirmation. The detail read and the timeline read of one selection
 * start together and share one picker, so the author is asked once.
 *
 * A change of the filter or of the selection supersedes older responses
 * (GIT-FR-LKRX): each region has its own request slot.
 */
export function usePullRequests(
  enabled: boolean,
  onRequestGithubToken?: () => Promise<boolean>,
) {
  const [filter, setFilterState] = useState<PullRequestListState>("open");
  const [selected, setSelected] = useState<number | null>(null);
  const list = useLoad<PullRequestSummary[]>();
  const detail = useLoad<PullRequestDetail>();
  const timeline = useLoad<PullRequestTimeline>();

  // One picker at a time: a second read that needs a token waits for the first
  // one's answer instead of opening a second picker.
  const picker = useRef<Promise<boolean> | null>(null);
  const requestToken = useCallback(() => {
    if (!onRequestGithubToken) return undefined;
    return () => {
      if (!picker.current) {
        picker.current = onRequestGithubToken().finally(() => {
          picker.current = null;
        });
      }
      return picker.current;
    };
  }, [onRequestGithubToken]);

  const { run: runList } = list;
  const { run: runDetail, reset: resetDetail } = detail;
  const { run: runTimeline, reset: resetTimeline } = timeline;

  const loadList = useCallback(
    (state: PullRequestListState, keep: boolean) => {
      logDebug(["frontend", "remote"], "pull request list read started", { state });
      void runList(
        async () => {
          try {
            const rows = await withGithubToken(
              () => api.listPullRequests(state),
              requestToken(),
            );
            return rows ?? [];
          } catch (e) {
            logWarn(["frontend", "remote"], "pull request list read failed", {
              state,
              code: parseRejection(e).code,
            });
            throw e;
          }
        },
        { keep },
      );
    },
    [runList, requestToken],
  );

  const loadSelection = useCallback(
    (id: number, keep: boolean) => {
      logDebug(["frontend", "remote"], "pull request read started", { id });
      void runDetail(
        async () => {
          try {
            return await withGithubToken(
              () => api.getPullRequestDetail(id),
              requestToken(),
            );
          } catch (e) {
            logWarn(["frontend", "remote"], "pull request detail read failed", {
              id,
              code: parseRejection(e).code,
            });
            throw e;
          }
        },
        { keep },
      );
      void runTimeline(
        async () => {
          try {
            const read = await withGithubToken(
              () => api.listPullRequestTimeline(id),
              requestToken(),
            );
            return {
              truncated: read?.truncated ?? false,
              items: read?.items ?? [],
            };
          } catch (e) {
            logWarn(["frontend", "remote"], "pull request timeline read failed", {
              id,
              code: parseRejection(e).code,
            });
            throw e;
          }
        },
        { keep },
      );
    },
    [runDetail, runTimeline, requestToken],
  );

  // Loaded once, when the section is first shown.
  const started = useRef(false);
  useEffect(() => {
    if (!enabled || started.current) return;
    started.current = true;
    loadList("open", false);
  }, [enabled, loadList]);

  /** GIT-FR-OBZW: another filter clears the selection and reads its own list. */
  const setFilter = useCallback(
    (next: PullRequestListState) => {
      if (next === filter) return;
      setFilterState(next);
      setSelected(null);
      resetDetail();
      resetTimeline();
      loadList(next, false);
    },
    [filter, loadList, resetDetail, resetTimeline],
  );

  const select = useCallback(
    (id: number) => {
      if (id === selected) return;
      setSelected(id);
      loadSelection(id, false);
    },
    [selected, loadSelection],
  );

  const refreshList = useCallback(() => loadList(filter, true), [filter, loadList]);
  const refreshSelection = useCallback(() => {
    if (selected !== null) loadSelection(selected, true);
  }, [selected, loadSelection]);

  return {
    filter,
    setFilter,
    selected,
    select,
    list: list.state,
    detail: detail.state,
    timeline: timeline.state,
    refreshList,
    refreshSelection,
  };
}

export type PullRequestsController = ReturnType<typeof usePullRequests>;

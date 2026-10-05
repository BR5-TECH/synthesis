/**
 * The draft tab's discussion wiring (`../../../specifications/ui/DDS-draft-discussion.md`
 * DDS-FR-QMBC, DDS-FR-VHZN, `NAW-new-artifact.md` NAW-FR-32).
 *
 * This hook holds the state that is about the tab: which discussion is
 * focused, the signal that moves the column's caret, and the fragment group.
 * It renders nothing. The discussion itself renders in the column.
 */
import { useCallback, useMemo, useState } from "react";

import type { UseDiscussionsResult } from "../../hooks/useDiscussions";
import type { OpenDiscussionRequest } from "../discussion";
import { useDraftFragments } from "./useDraftFragments";

export function useDraftDiscussionWiring(input: {
  draftId: string;
  discussions: UseDiscussionsResult;
  /** The draft-relative path of the prompt on screen. */
  path: string | null;
  /** The Markdown source of that prompt. */
  source: string;
}) {
  const { draftId, discussions, path, source } = input;
  const [focusedThreadId, setFocusedThreadId] = useState<string | null>(null);
  /** ACT-FR-QWNP: raised by the control's Discuss, which opens no composer. */
  const [discussFocus, setDiscussFocus] = useState(0);

  /**
   * NAW-FR-32 / ACT-FR-QWNP: begin the draft's first discussion from the
   * column, and focus it so the author is reading it the moment it exists.
   */
  const openDiscussion = useCallback(
    async (request: OpenDiscussionRequest) => {
      const created = await discussions.open(
        request.body,
        request.attachments,
        request.fragmentTarget,
      );
      setFocusedThreadId(created.id);
      return created;
    },
    [discussions],
  );

  /**
   * DDS-FR-QMBC / DDS-FR-VHZN: every discussion of the draft, the whole-target
   * and the fragment ones alike, as the column lists them.
   */
  const draftDiscussions = useMemo(
    () => discussions.discussions.map((d) => d.thread),
    [discussions.discussions],
  );
  const fragmentsGroup = useDraftFragments({
    draftId,
    path,
    source,
    discussions: draftDiscussions,
    selectedThreadId: focusedThreadId,
    setSelectedThreadId: setFocusedThreadId,
    onOpeningStarted: () => setDiscussFocus((n) => n + 1),
  });

  return {
    focusedThreadId,
    setFocusedThreadId,
    discussFocus,
    setDiscussFocus,
    openDiscussion,
    draftDiscussions,
    fragmentsGroup,
  };
}

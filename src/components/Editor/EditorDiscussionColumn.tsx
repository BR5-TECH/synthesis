/**
 * CMT-FR-01 / DDS-FR-KTVW: the Editor tab's discussion column — the splitter and
 * the column beside the document, rendered by the same components as the New
 * Artifact tab's, so a file's discussion looks and behaves as a draft's does.
 *
 * Split out of `EditorView.tsx` to keep that file within the size rule. It
 * holds no state: the column's split is the shared column store's, and the
 * discussions are `useEditorDiscussions`'s.
 */
import { useMemo, type RefObject } from "react";

import { DiscussionColumn, Splitter } from "../DraftDiscussion";
import { COMPOSER_KEY } from "../../hooks/useDiscussions";
import { DRAFT_KEY } from "../../hooks/useComments";
import { requestPendingFocus } from "../../state/discussionFocus";
import { logDebug } from "../../logging";
import type { Discussion, FragmentTarget } from "../../types";
import type { CommentSelection } from "./selection";
import type { useEditorDiscussions } from "./useEditorDiscussions";

export function EditorDiscussionColumn({
  group,
  splitRef,
  artifactId,
  artifactName,
  artifactType,
  identityBlock,
  onChooseToken,
  blocked,
  draft,
  setDraft,
  focusedThread,
  setFocusedThread,
  onFocusFragment,
  showResolved,
  onShowResolved,
  focusSignal,
}: {
  group: ReturnType<typeof useEditorDiscussions>;
  /** The grid the splitter's drag is measured against. */
  splitRef: RefObject<HTMLDivElement | null>;
  artifactId: string;
  artifactName: string | undefined;
  artifactType: string | undefined;
  identityBlock: { message: string; route?: string; opensPicker?: boolean } | null;
  /** CMT-FR-25: open the token picker. */
  onChooseToken: () => void;
  blocked: boolean;
  /** CMT-FR-05: the passage a Comment affordance opened the composer for. */
  draft: CommentSelection | null;
  setDraft: (draft: CommentSelection | null) => void;
  focusedThread: string | null;
  setFocusedThread: (id: string | null) => void;
  /** CMT-FR-28: a discussion's fragment quote was activated. */
  onFocusFragment: (discussionId: string) => void;
  showResolved: boolean;
  onShowResolved: (shown: boolean) => void;
  /** ACT-FR-BRZK: raised by Discuss to put the caret in the column's composer. */
  focusSignal: number;
}) {
  const { comments, control, split } = group;
  // The discussion target names the file, and so do the statements: a
  // specification is called what it is, and anything else is a file.
  const noun = artifactType === "spec" ? "specification" : "file";

  /**
   * DDS-FR-QMBC / CMT-FR-05: the passage the opening composer carries. Derived
   * from the selection the affordance recorded, which moves with the buffer
   * (CMT-FR-20), so a post names the passage as it stands now.
   */
  const openingFragment = useMemo<FragmentTarget | null>(
    () =>
      draft
        ? {
            owner: { kind: "artifact", artifactId },
            path: artifactId,
            start: draft.start,
            end: draft.end,
            quote: draft.quote,
          }
        : null,
    [draft, artifactId],
  );

  /**
   * CMT-FR-34: the refusal of the last post, in the one opening composer the
   * column has. A fragment discussion is opened by `useComments` and a
   * whole-file one by the action control, and each files its refusal under its
   * own key.
   */
  const errors = useMemo(
    () => ({
      ...group.railErrors,
      [COMPOSER_KEY]: draft
        ? comments.errors[DRAFT_KEY]
        : control.discussions.errors[COMPOSER_KEY],
    }),
    [group.railErrors, draft, comments.errors, control.discussions.errors],
  );

  return (
    <>
      {/* DDS-FR-PNXR: the splitter, dragged and also moved by keyboard. It
          stands only while there are two columns to split (DDS-FR-XQMF). */}
      <Splitter
        ratio={split.view.ratio}
        onChange={split.setRatio}
        boundsRef={splitRef}
        hidden={split.view.hidden}
      />
      {/* DDS-FR-XQMF: hidden rather than unmounted, so a reply part way
          written survives the column being hidden. */}
      <DiscussionColumn
        hidden={split.view.hidden}
        column={group.column}
        target={{ kind: "artifact", artifactId }}
        noun={noun}
        material={noun}
        testId="editor-discussion-column"
        discussions={group.columnDiscussions}
        selectedThreadId={focusedThread}
        onSelectThread={setFocusedThread}
        identity={comments.identity}
        identityBlock={
          identityBlock
            ? {
                message: identityBlock.message,
                route: identityBlock.route,
                // CMT-FR-25: the one refusal the author can resolve from here.
                action: identityBlock.opensPicker
                  ? { label: "Choose a token…", onActivate: onChooseToken }
                  : undefined,
              }
            : null
        }
        agents={comments.agents}
        onReply={group.railReply}
        onSetResolved={group.railSetResolved}
        errors={errors}
        blocked={blocked}
        anchor={null}
        onOpenDiscussion={(request) =>
          // ACT-FR-19: a fragment discussion is the anchored hook's and a
          // whole-file one the action control's, each with its own dispatch
          // rules (CTA-FR-DNMV, CTA-FR-HCMJ).
          request.fragmentTarget
            ? comments.openThread(request.fragmentTarget, request.body, request.attachments)
            : control.discussions.open(request.body, request.attachments)
        }
        onOpened={(created: Discussion) => {
          // ACT-FR-16: the composer goes, and focus lands in the reply composer
          // of the new discussion in the column.
          logDebug(["frontend"], "the discussion column opened a discussion", {
            discussionId: created.id,
            fragment: draft !== null,
          });
          setDraft(null);
          requestPendingFocus(created.id, "composer");
          setFocusedThread(created.id);
        }}
        openingFragment={openingFragment}
        onCancelFragment={() => setDraft(null)}
        availabilityOf={group.availabilityOf}
        onFocusFragment={(_fragment, discussion) => onFocusFragment(discussion.id)}
        ownerLabel={artifactName ?? artifactId}
        focusSignal={focusSignal}
        turnsFor={group.turnsFor}
        showResolved={showResolved}
        onShowResolved={onShowResolved}
      />
    </>
  );
}

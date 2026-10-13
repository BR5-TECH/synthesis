/**
 * The action control of the New Artifact tab (`NAW-new-artifact.md`
 * NAW-FR-27, NAW-FR-28): which entries a draft puts in the control of
 * `ACT-action-control.md`, and when each one is enabled. Split out of
 * `index.tsx` to keep that file within the size rule.
 */
import type { DiscussionControl } from "../../hooks/useDiscussionControl";
import type { UseDraftPublicationResult } from "../../hooks/useDraftPublication";
import { setDraftDiscussionHidden } from "../../state/draftDiscussion";
import type { DraftSessionStore } from "../../state/draftSessions";
import { ActionControl } from "../ActionControl";
import { publicationActionTitle } from "../DraftPublication";
import { copyLivePrompt } from "./copySource";
import {
  SHADOW_ARCHIVE_REASON,
  SHADOW_DISCUSS_REASON,
  SHADOW_PUBLISH_REASON,
} from "./GithubShadow";

export function DraftActionControl({
  draftId,
  name,
  drafts,
  control,
  discussionHidden,
  shadow,
  locked,
  graduated,
  archived,
  blocked,
  publication,
  onGraduate,
  onPublish,
  onToggleArchive,
  onDiscussFocus,
  dismissSignal,
  onOverlayChange,
  onDiscussionOpened,
}: {
  draftId: string;
  name: string;
  drafts: DraftSessionStore;
  control: DiscussionControl;
  discussionHidden: boolean;
  shadow: boolean;
  locked: boolean;
  graduated: boolean;
  archived: boolean;
  /** The graduation start dialog is open over the tab. */
  blocked: boolean;
  publication: UseDraftPublicationResult;
  onGraduate: () => void;
  onPublish: () => void;
  onToggleArchive: () => void;
  onDiscussFocus: () => void;
  dismissSignal: number;
  onOverlayChange: (open: boolean) => void;
  onDiscussionOpened: (threadId: string) => void;
}) {
  return (
    <ActionControl
      discussions={control.discussions}
      composer={control.composer}
      onComposerChange={control.setComposer}
      attachments={control.attachments}
      /* CVP-FR-08 / CVP-FR-64: the same owner the rail's own cards name,
         so a discussion created here reads the draft's name rather than
         its opaque id wherever it is presented (per the CommentRail's
         ownerLabel/ownerSurface above). */
      ownerLabel={name}
      ownerSurface="draft"
      itemNoun="draft"
      controlLabel="Draft actions"
      // NAW-FR-34: enabled whenever a draft is open, on the same
      // unconditional terms Graduate is — a draft always holds its prompt,
      // so there is always material for an agent to be asked about, and an
      // empty prompt is a thing an author may well want to discuss.
      canDiscuss
      /* ACT-FR-QWNP / ACT-FR-23: with the column shown the conversation is
         already beside the draft, so the action has nothing left to do and
         says so rather than reading as a control that does nothing. */
      discussUnavailable={
        // NAW-FR-AXFQ: a shadow draft offers no Discuss.
        shadow
          ? SHADOW_DISCUSS_REASON
          : discussionHidden
            ? undefined
            : "The discussion is already beside the draft"
      }
      /* ACT-FR-QWNP / DDS-FR-JWNC: the conversation is a column of this tab
         rather than a composer to open. The action is only reachable while
         that column is hidden, so it brings the column back — by the route
         the action row's own control takes, which is what keeps the two in
         agreement and persists the state (per DDS-FR-XQMF) — and then puts
         the caret in it. */
      onDiscuss={() => {
        setDraftDiscussionHidden(draftId, false);
        onDiscussFocus();
      }}
      // NAW-FR-32 / ACT-FR-19: this tab lends the rail a margin, so the
      // discussions pin at its head and no floating panel is rendered.
      discussionsInRail
      blocked={blocked}
      graduate={{
        label: "Graduate",
        onActivate: onGraduate,
        // NAW-FR-17 / NAW-FR-17: no precondition on status or content —
        // a draft always holds its one prompt, and an empty prompt is a
        // thing the author may graduate as readily as a full one. The one
        // thing that disables it is a run already holding this draft, or
        // a draft a run has already graduated.
        disabled: locked,
        title: locked
          ? graduated
            ? "This draft has already been graduated"
            : "A graduation run is already working on this draft"
          : "Write the specification this prompt describes",
      }}
      copySource={{
        label: "Copy as Source",
        // NAW-FR-NVYZ: the live prompt as the editor holds it at this
        // moment. NAW-FR-FJXX: no status, lock, or empty prompt disables it.
        onActivate: () => copyLivePrompt(drafts, draftId),
        title: "Copy the prompt text to the clipboard",
      }}
      publish={{
        label: "Publish to GitHub",
        onActivate: onPublish,
        /* NAW-FR-ZQMX: enabled only where the one read reports the draft
           publishable. NAW-FR-VBHT: the reason is that read's, taken
           unchanged rather than composed here. NAW-FR-44: a `graduated`
           draft is not disabled by its status — publication changes the
           draft's metadata rather than its prompt. */
        disabled:
          shadow ||
          publication.busy ||
          !publication.view?.eligibility.publishable,
        title: shadow
          ? SHADOW_PUBLISH_REASON
          : publicationActionTitle(publication),
      }}
      archive={{
        label: archived ? "Restore" : "Archive",
        onActivate: onToggleArchive,
        // NAW-FR-AXFQ: neither Archive nor Restore on a shadow draft.
        disabled: locked || shadow,
        title: shadow ? SHADOW_ARCHIVE_REASON : undefined,
      }}
      // NAW-FR-30 / CMT-FR-32: the margin's card menus join this tab's
      // floating surfaces under one rule — at most one of them all is open
      // at any moment. Each side reports its openings to the other.
      dismissSignal={dismissSignal}
      onOverlayChange={onOverlayChange}
      // NAW-FR-32 / CMT-FR-53: focused at the head of the margin, not
      // merely present — the conversation is where it will be read from
      // the moment it exists.
      onDiscussionOpened={onDiscussionOpened}
    />
  );
}

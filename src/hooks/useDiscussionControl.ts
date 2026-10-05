/**
 * Everything a tab needs to carry the action control (`ACT-action-control.md`).
 *
 * The control's own state lives in the component; what lives here is the pair the
 * component cannot own — the item's discussions (ACT-FR-22) and the composer
 * state that belongs to the item rather than to the tab (ACT-FR-14). Both are
 * keyed on the target, so a tab closed and reopened on the same item finds its
 * half-written message where it left it.
 *
 * A host with a comment rail feeds the same `discussions` result to it
 * (ACT-FR-19); a host without one lets the control render the floating panel
 * (ACT-FR-20). Either way there is one read of the log per tab, not two.
 */
import { useMemo } from "react";
import { fileToInput, guessMediaType } from "../components/CommentAttachments";
import type { ComposerAttachments } from "../components/ActionControl";
import {
  useDiscussions,
  type SharedConversationContext,
  type UseDiscussionsResult,
} from "./useDiscussions";
import {
  useDiscussionComposer,
  type PendingComposerAttachment,
} from "../state/discussionComposers";
import type { DiscussionTarget } from "../types";

/**
 * Where the composer's body and strip are held, when the host already has a
 * store of its own.
 *
 * The New Artifact tab keeps the draft's in the draft's session, which is
 * cleared with the project and the worktree exactly as the rest of a draft's
 * session state is; every other host has no such store and uses the module one
 * (`../state/discussionComposers`). Either satisfies ACT-FR-14 — the composer
 * belongs to the item and survives the tab — so the seam is which store, not
 * whether there is one.
 */
export interface ComposerBinding {
  body: string;
  attachments: readonly PendingComposerAttachment[];
  setBody: (body: string) => void;
  setAttachments: (attachments: PendingComposerAttachment[]) => void;
  current: () => PendingComposerAttachment[];
}

export interface DiscussionControl {
  discussions: UseDiscussionsResult;
  composer: string;
  setComposer: (text: string) => void;
  attachments: ComposerAttachments;
}

export function useDiscussionControl(
  target: DiscussionTarget | undefined,
  enabled = true,
  composerStore?: ComposerBinding,
  shared?: SharedConversationContext,
): DiscussionControl {
  const discussions = useDiscussions(
    target,
    enabled && target !== undefined,
    shared,
  );
  const fallback = useDiscussionComposer(target);
  const { body, attachments, setBody, setAttachments, current } =
    composerStore ?? fallback;

  const composerAttachments: ComposerAttachments = useMemo(
    () => ({
      pending: attachments,
      inputs: attachments.map((p) => p.input),
      addFiles: async (files: File[]) => {
        const encoded: PendingComposerAttachment[] = await Promise.all(
          files.map(async (file) => ({
            input: await fileToInput(file),
            name: file.name !== "" ? file.name : "attachment",
          })),
        );
        setAttachments([...current(), ...encoded]);
      },
      addLink: (url: string, label = "") => {
        const trimmed = url.trim();
        if (trimmed === "") return;
        const named = label.trim();
        setAttachments([
          ...current(),
          {
            input: {
              kind: "url",
              url: trimmed,
              mediaType: guessMediaType(trimmed),
              ...(named === "" ? {} : { label: named }),
            },
            name: named === "" ? trimmed : named,
          },
        ]);
      },
      removeAt: (index: number) =>
        setAttachments(current().filter((_, i) => i !== index)),
      clear: () => setAttachments([]),
    }),
    [attachments, setAttachments, current],
  );

  return {
    discussions,
    composer: body,
    setComposer: setBody,
    attachments: composerAttachments,
  };
}

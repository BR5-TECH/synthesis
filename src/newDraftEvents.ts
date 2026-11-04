/**
 * DCP-FR-ZDSP: the decomposition event, split out of `events.ts` to keep that
 * file small. `events.ts` re-exports it.
 */

import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type { NewDraftProposalsChangedPayload } from "./types";

/** DCP-FR-ZDSP: a new-draft proposal was recorded, updated, or decided, or a decomposition was confirmed. */
export const NEW_DRAFT_PROPOSALS_CHANGED = "new-draft-proposals-changed";

/**
 * DCR-FR-QQLE: subscribed at the shell, so a raise reaches the author with the
 * draft's tab closed, and the store stays current for every surface.
 */
export const onNewDraftProposalsChanged = (
  handler: (payload: NewDraftProposalsChangedPayload) => void,
): Promise<UnlistenFn> =>
  listen<NewDraftProposalsChangedPayload>(NEW_DRAFT_PROPOSALS_CHANGED, (event) =>
    handler(event.payload),
  );

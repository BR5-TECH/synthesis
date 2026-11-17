/**
 * The planning conversation of the Map tab (`MPR-map-plan-proposals.md`). This
 * module is the only place the command names of the Map threads and of the
 * plan proposals live.
 */
import { invoke } from "./invoke";

import type { AttachmentInput, Discussion, MapNodeRef, MapPlanProposal } from "../types";

/** MPR-FR-MYLA: the latest open thread of a node, or of the whole map where no node is named. Creates nothing. */
export const findMapDiscussion = (mapNode?: MapNodeRef | null) =>
  invoke<Discussion | null>("find_map_discussion", { mapNode: mapNode ?? null });

/** MPR-FR-MAXH: a new thread, about a node or the whole map, with its first comment. */
export const openMapDiscussion = (args: {
  mapNode?: MapNodeRef | null;
  body: string;
  attachments: AttachmentInput[];
  /** CMS-FR-UYVK. */
  spokenOriginal?: string | null;
}) =>
  invoke<Discussion>("open_map_discussion", {
    mapNode: args.mapNode ?? null,
    body: args.body,
    attachments: args.attachments,
    spokenOriginal: args.spokenOriginal ?? null,
  });

/** MPR-FR-NRNZ: every Map thread, resolved ones included, in the order they were opened. */
export const listMapDiscussions = () => invoke<Discussion[]>("list_map_discussions");

/** MPV-FR-IBSF: the proposals of a Map discussion, oldest first. */
export const listMapPlanProposals = (discussionId: string) =>
  invoke<MapPlanProposal[]>("list_map_plan_proposals", { discussionId });

/** MPV-FR-XYDR: accept one row. */
export const acceptMapPlanRow = (discussionId: string, proposalId: string, rowKey: string) =>
  invoke<MapPlanProposal>("accept_map_plan_row", { discussionId, proposalId, rowKey });

/** MPV-FR-XYDR: reject one row. */
export const rejectMapPlanRow = (discussionId: string, proposalId: string, rowKey: string) =>
  invoke<MapPlanProposal>("reject_map_plan_row", { discussionId, proposalId, rowKey });

/** MPV-FR-XYDR / MPR-FR-MVRX: move a rejected row back to waiting. */
export const reopenMapPlanRow = (discussionId: string, proposalId: string, rowKey: string) =>
  invoke<MapPlanProposal>("reopen_map_plan_row", { discussionId, proposalId, rowKey });

/** MPV-FR-AAFF / MPR-FR-GIJN: ask the agent that proposed to prepare the failed rows again. */
export const requestMapPlanReprepare = (discussionId: string, proposalId: string) =>
  invoke<MapPlanProposal>("request_map_plan_reprepare", { discussionId, proposalId });

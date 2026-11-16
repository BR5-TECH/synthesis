/**
 * The planning conversation of the Map tab (`MPR-map-plan-proposals.md`). This
 * module is the only place the command names of the Map threads and of the
 * plan proposals live.
 */
import { invoke } from "./invoke";

import type { AttachmentInput, Discussion, MapNodeRef, MapPlanProposal } from "../types";

/** MPR-FR-MYLA: the thread of a node, or the Map thread where no node is named. Creates nothing. */
export const findMapDiscussion = (mapNode?: MapNodeRef | null) =>
  invoke<Discussion | null>("find_map_discussion", { mapNode: mapNode ?? null });

/** MPR-FR-MAXH: the thread of a node, made with its first comment where it does not exist. */
export const getOrCreateMapDiscussion = (args: {
  mapNode?: MapNodeRef | null;
  body: string;
  attachments: AttachmentInput[];
  /** CMS-FR-UYVK. */
  spokenOriginal?: string | null;
}) =>
  invoke<Discussion>("get_or_create_map_discussion", {
    mapNode: args.mapNode ?? null,
    body: args.body,
    attachments: args.attachments,
    spokenOriginal: args.spokenOriginal ?? null,
  });

/** MPR contract: the Map thread first, then the node threads, newest first. */
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

/**
 * The shared discussion surface and composer
 * (`CVP-conversation-presentation.md` CVP-FR-SDMQ).
 *
 * Every owner layout renders a discussion with these two components and draws
 * no message list or composer of its own.
 */
export { DiscussionSurface } from "./DiscussionSurface";
export type { DiscussionSurfaceProps } from "./DiscussionSurface";
export { DiscussionComposer } from "./DiscussionComposer";
export type {
  DiscussionComposerProps,
  OpenDiscussionRequest,
} from "./DiscussionComposer";
export { DiscussionBadges } from "./DiscussionParts";
export type { OwnerAvailability } from "./DiscussionParts";
export { UnreadDivider, UnreadIndicator } from "./UnreadMarks";
export { isPostAccelerator, isImeComposing, postAcceleratorHint } from "./composerKeys";
export { useDiscussionFollow, TAIL_SLACK } from "./useDiscussionFollow";

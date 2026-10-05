/** The props the rail's cards and the surfaces that borrow them take. */
import type { PendingAttachment } from "../CommentAttachments";
import type { AnchoredThread } from "../../state/commentAnchors";
import type {
  AgentTurn,
  AttachmentInput,
  CommentQuote,
  Participant,
  ProjectAgent,
} from "../../types";

/**
 * CVP-FR-47: a composer whose content belongs to a presentation instance rather
 * than to the component rendering it.
 *
 * A conversation the author has moved keeps its half-written reply, its quotes,
 * and its pending attachments through every mode change and through the surface
 * it was in being unmounted, which local state cannot do. A card of a
 * conversation nobody has moved passes nothing and keeps its own state.
 */
export interface ControlledComposer {
  body: string;
  quotes: readonly CommentQuote[];
  attachments: readonly PendingAttachment[];
  setBody: (body: string) => void;
  setQuotes: (quotes: readonly CommentQuote[]) => void;
  setAttachments: (attachments: PendingAttachment[]) => void;
}

export interface ThreadCardProps {
  entry: AnchoredThread;
  top?: number;
  positioned: boolean;
  blocked: boolean;
  disabled: boolean;
  focused: boolean;
  onFocus: () => void;
  menuOpen: boolean;
  onToggleMenu: () => void;
  onCloseMenu: () => void;
  onReply: (
    threadId: string,
    body: string,
    quotes: CommentQuote[],
    attachments: AttachmentInput[],
  ) => Promise<void>;
  onSetLock: (threadId: string, locked: boolean) => Promise<void>;
  onSetResolved: (threadId: string, resolved: boolean) => Promise<void>;
  error?: string;
  agents: readonly ProjectAgent[];
  pendingTurns: readonly AgentTurn[];
  turnFailure?: string;
  /** CTA-FR-RQQJ: this conversation's current recoverable failure, if it has one. */
  failedTurn?: AgentTurn;
  /**
   * CTA-FR-ARBB: the turn whose images the selected provider and model could not
   * take, if this conversation carries one.
   */
  imageNotice?: AgentTurn;
  retryingTurnIds?: ReadonlySet<string>;
  onRetryTurn?: (turnId: string) => void;
  onCancelTurn: (turnId: string) => void;
  onMeasure?: (threadId: string, height: number) => void;
  /**
   * CVP-FR-32: the same conversation rendered on a different surface. `embedded`
   * drops the card's own frame and its anchor quote, both of which the overlay's
   * and the tab's chrome already carry; nothing about the messages changes, which
   * is why a message reads the same wherever it is read.
   *
   * `stream` is the draft's discussion column and that column alone
   * (per `../../../specifications/ui/DDS-draft-discussion.md` DDS-FR-ZMXQ). It
   * is not one of CVP-FR-32's modes and never could be: a draft conversation
   * holds no presentation instance at all (CVP-FR-DKPW), so the column renders
   * the same comments in its own forms — cards only where an exchange carries
   * state (DDS-FR-BRHN), and a rail rather than a fill on the author's own
   * message (DDS-FR-VTKD). The other three variants are untouched by it.
   */
  variant?: "card" | "embedded" | "stream";
  composer?: ControlledComposer;
  /**
   * CVP-FR-32: the identity the author is writing as, so a message they wrote is
   * distinguishable at a glance from one written to them.
   *
   * Read only in the `embedded` variant. A card in the margin is a narrow box
   * where every message already carries its author's name on its own line, and
   * splitting it into two columns would cost the prose the width it has; the
   * overlay and the tab are wide enough to read as the group chat they are.
   */
  identity?: Participant | null;
  /**
   * CVP-FR-TWRL: the message list scrolls within itself and the
   * composer is pinned below it, so the composer is always visible at the foot
   * of the surface rather than scrolling away with the conversation.
   *
   * The scroll container is the message list rather than the surface around it,
   * so the surface hands its scroll handling in here.
   */
  messagesRef?: React.Ref<HTMLDivElement>;
  onMessagesScroll?: (event: React.UIEvent<HTMLDivElement>) => void;
  /**
   * DDS-FR-GKMT / DDS-FR-CLBK: something the host draws between two messages —
   * the unread divider, and the collapsed head of a long discussion.
   *
   * A render hook rather than the marks themselves, because what belongs
   * between two messages is a property of the surface reading them rather than
   * of the conversation: a card in the margin has no reading position to mark,
   * and a discussion column has two. It returns nothing for every message it
   * has nothing to say about.
   */
  beforeComment?: (commentId: string, index: number) => React.ReactNode;
  /**
   * DDS-FR-CLBK: the index of the first message to render, so a surface reading
   * a long conversation can collapse its head into one row.
   *
   * The messages above it are not rendered at all rather than hidden with CSS:
   * a month of conversation is what makes this worth doing, and a hidden
   * message still costs a card to build and a markdown body to parse.
   */
  visibleFrom?: number;
}

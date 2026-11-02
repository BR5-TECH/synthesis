/**
 * ACT-FR-QWNP / ACT-FR-BRZK / NAW-FR-32 / DDS-FR-VHZN: how a discussion of a
 * draft or of a file is begun.
 *
 * The composer sits **in the column** rather than in the action control's own
 * floating surface: on this tab the conversation is already on screen, so a
 * floating composer would be a second place to write into a surface that is
 * right there — and it would cover the draft the message is about.
 *
 * The composer is the shared `DiscussionComposer` in its opening mode, so the
 * first message of a discussion is written exactly as every later one is: the
 * mention picker, the attachment strip, and Ctrl+Enter or Cmd+Enter to post
 * (DDS-FR-LPSC). What this file owns is the statement above it, which says what
 * the column holds while it shows no transcript.
 *
 * Why posting is unavailable is said by the column above rather than here
 * (NAW-FR-33, CMT-FR-24): the reason is the same whichever composer is
 * standing, and stating it twice would put two copies of one sentence on the
 * surface at once.
 */
import { DiscussionComposer, type OpenDiscussionRequest } from "../discussion";
import type {
  Discussion,
  DiscussionTarget,
  FragmentTarget,
  ProjectAgent,
  ProposalHunkReference,
} from "../../types";

/** DDS-FR-RHKW: the change a new hunk discussion is about. */
export interface HunkOpening {
  reference: ProposalHunkReference;
  /** The change's place in its proposal, for `change N of M`. */
  position: number;
  total: number;
}

/** DDS layout: the states a column showing no transcript can be in. */
export type OpenState =
  /** Nobody has spoken about this item. */
  | "unspoken"
  /** Every discussion about it is resolved. */
  | "resolved"
  /** The author is writing the first message of another discussion. */
  | "another"
  /** The author is writing the first message of a discussion about a passage. */
  | "fragment"
  /** The author is writing the first message of a discussion about one change. */
  | "hunk";

/**
 * What the column says about the state it is in, and what the composer begins.
 *
 * Each states the case plainly and says what to do next, in the interface's own
 * voice. None of them apologises and none is vague about what the column holds.
 */
function statementFor(
  state: OpenState,
  resolved: number,
  hunk: HunkOpening | null,
  noun: string,
  material: string,
): { lead: string; hint: string } {
  if (state === "hunk") {
    // DDS-FR-RHKW: the composer names the change it is about. DCR-FR-JTXE: and
    // says that talking about it decides nothing.
    const which =
      hunk !== null && hunk.position > 0
        ? `change ${hunk.position} of ${hunk.total}`
        : "one proposed change";
    return {
      lead: `This begins a discussion about ${which}.`,
      hint: "The change is not decided by this discussion. Accept or reject it in the document.",
    };
  }
  if (state === "fragment") {
    return {
      lead: "This begins a discussion about the passage you selected.",
      hint: `The passage stays marked in the ${material} while the discussion is open.`,
    };
  }
  if (state === "resolved") {
    return {
      lead:
        resolved === 1
          ? `The discussion about this ${noun} is resolved.`
          : `Every discussion about this ${noun} is resolved.`,
      hint:
        `Open ${resolved === 1 ? "it" : "them"} above to read what was said, ` +
        "or address an agent by name to begin another.",
    };
  }
  if (state === "another") {
    return {
      lead: `This begins another discussion about this ${noun}.`,
      hint: "The discussions above stay where they are, and this one joins them.",
    };
  }
  return {
    lead: `Nothing has been said about this ${noun} yet.`,
    hint:
      "Address an agent by name and the conversation is read here, beside the " +
      `${material} it is about.`,
  };
}

export function OpenDiscussion({
  agents,
  disabled,
  error,
  state,
  resolved,
  target,
  noun,
  material,
  fragment,
  hunk = null,
  onOpen,
  onOpened,
  onCancel,
}: {
  agents: readonly ProjectAgent[];
  disabled: boolean;
  /** CMT-FR-34: the typed refusal of the last post, if it was refused. */
  error?: string;
  /**
   * DDS layout: which state the column is in.
   *
   * The statement says what the column holds, so each state has its own. A
   * draft whose discussions are every one resolved has been spoken about, and
   * one whose author is opening another has been too — telling either of them
   * that nothing has been said would be untrue. Saying nothing at all is worse:
   * a panel that is empty and silent reads as a fault rather than as a state.
   */
  state: OpenState;
  /** CMT-FR-17: how many resolved discussions the disclosure above holds. */
  resolved: number;
  /** What the discussion is about: the draft, or the file. */
  target: DiscussionTarget;
  /** What the statements call the item: `draft`, `file`, or `specification`. */
  noun: string;
  /** What the item's text is called: the draft's `prompt`, or the file. */
  material: string;
  /** DDS-FR-QMBC: the passage the discussion will be about, if it has one. */
  fragment: FragmentTarget | null;
  /** DDS-FR-RHKW: the change the discussion will be about, if it is one. */
  hunk?: HunkOpening | null;
  onOpen: (request: OpenDiscussionRequest) => Promise<Discussion>;
  onOpened: (discussion: Discussion) => void;
  /** The author discarded the composer. */
  onCancel: () => void;
}) {
  const statement = statementFor(state, resolved, hunk, noun, material);

  return (
    <div className="dds-open" data-testid="draft-open-discussion">
      {/* DDS layout: a statement about an empty column is about the column
          rather than about a message, so it stands in the middle of the space
          it describes rather than at the foot where the newest message would
          be — and the composer below keeps the foot it holds in every other
          state. It takes the measure a message takes, so what stands in the
          empty column is where the first message will come. */}
      <div className="dds-open__statement" data-state={state}>
        <p className="dds-open__lead t-ui-sm">{statement.lead}</p>
        <p className="dds-open__hint t-ui-xs">{statement.hint}</p>
        {fragment !== null && (
          <p
            className="comment-card__quote"
            title={fragment.quote}
            data-testid="discussion-opening-fragment"
          >
            ❝ {fragment.quote}
          </p>
        )}
      </div>
      <DiscussionComposer
        mode="opening"
        target={target}
        fragmentTarget={fragment}
        agents={agents}
        disabled={disabled}
        error={error}
        ariaLabel={`Discuss this ${noun}`}
        placeholder={`Ask an agent about this ${noun}…`}
        alwaysShowCancel={fragment !== null || hunk !== null}
        extraAttachments={hunk !== null ? [hunk.reference] : undefined}
        onOpen={onOpen}
        onOpened={onOpened}
        onCancel={onCancel}
      />
    </div>
  );
}

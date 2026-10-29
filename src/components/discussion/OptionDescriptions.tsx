/**
 * The option descriptions of a submitted question, in history
 * (`../../../specifications/ui/DQA-discussion-question-answering.md`
 * DQA-FR-MDMU, DQA-FR-XRLK, DQA-FR-UHXW; `../../../specifications/ui/CMT-comments.md`
 * CMT-FR-KUPP).
 *
 * The question comment's body keeps the labels alone. The descriptions travel
 * beside it in a structured field (per
 * `../../../specifications/core/CMS-comments-storage.md` CMS-FR-QAHZ), and a
 * reader shows them on request: collapsed by default, so the history stays as
 * short as the decisions it records.
 */
import { CommentMarkdown } from "../CommentMarkdown";
import { Icon } from "../icons";
import type { AgentRoster } from "../agentTags";
import {
  splitQuestion,
  unescapeMarkdown,
} from "../DraftDiscussion/stream/questionParse";
import { stripCitationMarkers } from "../../text/citationMarkers";
import type { Comment } from "../../types";

/**
 * DQA-FR-MDMU: the descriptions a comment carries, in option order. Empty on
 * every comment that is not a described question, and on one an older build
 * wrote, which therefore carries no control.
 */
export function descriptionsOf(comment: Comment): string[] {
  return comment.optionDescriptions ?? [];
}

export interface DescriptionsToggleProps {
  expanded: boolean;
  onToggle: () => void;
  /** The surface's own class beside the shared button classes. */
  className: string;
}

/**
 * DQA-FR-MDMU, CMT-FR-35: the control that shows and hides the descriptions. A
 * glyph rather than a word, named for what it does, and stating its state by
 * more than colour (DQA-FR-EWLB).
 */
export function DescriptionsToggle({
  expanded,
  onToggle,
  className,
}: DescriptionsToggleProps) {
  const label = expanded ? "Hide option descriptions" : "Show option descriptions";
  return (
    <button
      type="button"
      className={`btn btn--ghost btn--icon-xs ${className}`}
      data-testid="option-descriptions-toggle"
      aria-expanded={expanded}
      aria-label={label}
      title={label}
      onClick={onToggle}
    >
      {expanded ? <Icon.Caret size={12} /> : <Icon.CaretRight size={12} />}
    </button>
  );
}

export interface QuestionWithDescriptionsProps {
  body: string;
  descriptions: readonly string[];
  roster: AgentRoster;
  /** The surface's own class for the rendered body (`CommentMarkdown`). */
  bodyClassName?: string;
}

/**
 * DQA-FR-XRLK: a question comment with each description directly beneath its
 * option's label. The body itself is unchanged; this reads it back into the
 * question and its labels. A body this cannot read keeps its own rendering and
 * takes the descriptions as a numbered list below it, so nothing is lost.
 */
export function QuestionWithDescriptions({
  body,
  descriptions,
  roster,
  bodyClassName,
}: QuestionWithDescriptionsProps) {
  const split = splitQuestion(body);
  const readable = split !== null && split.values.length === descriptions.length;
  return (
    <>
      <CommentMarkdown
        body={readable ? split.text : body}
        agentRoster={roster}
        className={bodyClassName}
      />
      <ol className="comment__options" data-testid="option-descriptions">
        {descriptions.map((description, index) => (
          <li key={index} className="comment__option">
            {readable && (
              <span className="comment__option-label">
                {stripCitationMarkers(unescapeMarkdown(split.values[index]))}
              </span>
            )}
            {/* DQA-FR-JADB: a provider citation marker is hidden. */}
            <span className="comment__option-description">
              {stripCitationMarkers(description)}
            </span>
          </li>
        ))}
      </ol>
    </>
  );
}

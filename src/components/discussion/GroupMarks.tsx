/**
 * What a comment of an agent group shows (`CVP-conversation-presentation.md`
 * CVP-FR-OMAS, CVP-FR-HRGH, CVP-FR-VPKO, CVP-FR-LCSG): the **Synthesis** label
 * in its header and the collapsed **Advisors** list under its body. Both are read
 * from the attachments the comment is stored with.
 */
import { useId, useState } from "react";

import { Icon } from "../icons";
import type { AdvisorEntry, AdvisorsList, Attachment } from "../../types";

/** CVP-FR-OMAS: whether a comment is the Synthesis of an agent group. */
export function isSynthesis(attachments: readonly Attachment[]): boolean {
  return attachments.some((a) => a.kind === "synthesis");
}

/** CVP-FR-OMAS: the label beside the author's handle. */
export function SynthesisLabel({ attachments }: { attachments: readonly Attachment[] }) {
  if (!isSynthesis(attachments)) return null;
  return (
    <span className="comment__label t-eyebrow" data-testid="comment-synthesis-label">
      Synthesis
    </span>
  );
}

const REASON_TEXT: Record<NonNullable<AdvisorEntry["reason"]>, string> = {
  timed_out: "timed out",
  failed: "failed",
  cancelled: "cancelled",
};

/** CVP-FR-VPKO: one advisor of the expanded list. */
function AdvisorRow({ advisor }: { advisor: AdvisorEntry }) {
  if (advisor.state === "skipped") {
    return (
      <li className="advisors__entry" data-testid="advisor-entry" data-state="skipped">
        <span className="advisors__handle">@{advisor.handle}</span>{" "}
        <span className="t-muted" data-testid="advisor-reason">
          skipped: {advisor.reason ? REASON_TEXT[advisor.reason] : "not finished"}
        </span>
      </li>
    );
  }
  const points = advisor.points ?? [];
  const questions = advisor.questions ?? [];
  return (
    <li className="advisors__entry" data-testid="advisor-entry" data-state="finished">
      <span className="advisors__handle">@{advisor.handle}</span>
      {points.length > 0 && (
        <>
          <div className="t-eyebrow">Important points</div>
          <ul>
            {points.map((point, i) => (
              <li key={i}>{point}</li>
            ))}
          </ul>
        </>
      )}
      {questions.length > 0 && (
        <>
          <div className="t-eyebrow">Key questions</div>
          <ul>
            {questions.map((question, i) => (
              <li key={i}>{question}</li>
            ))}
          </ul>
        </>
      )}
    </li>
  );
}

/**
 * CVP-FR-HRGH: collapsed when it first renders, with the number of advisors in
 * its control. A native button, so the keyboard and the pointer both toggle it.
 */
export function AdvisorsListView({ attachment }: { attachment: AdvisorsList }) {
  const [expanded, setExpanded] = useState(false);
  const listId = useId();
  const count = attachment.advisors.length;
  return (
    <div className="advisors" data-testid="advisors-list">
      <button
        type="button"
        className="tool-disclosure"
        data-testid="advisors-toggle"
        aria-expanded={expanded}
        aria-controls={listId}
        onClick={() => setExpanded((open) => !open)}
      >
        <Icon.CaretRight
          size={11}
          className="icon tool-disclosure__chevron"
          aria-hidden="true"
        />
        <span>
          Advisors ({count})
        </span>
      </button>
      {expanded && (
        <ul id={listId} className="advisors__list">
          {attachment.advisors.map((advisor) => (
            <AdvisorRow key={advisor.handle} advisor={advisor} />
          ))}
        </ul>
      )}
    </div>
  );
}

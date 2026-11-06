/**
 * The result of a PR summary run
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-DQSA).
 *
 * A completed PR summary run renders the title on one line and the description
 * as Markdown with inert links. It renders no result in any other state, and no
 * control that applies the result to a pull request: the Create a PR window
 * does that (GRU-FR-LJUY).
 */

import { prSummaryResultOf } from "../../state/graduation";
import type { GraduationRun } from "../../types";
import { CommentMarkdown } from "../CommentMarkdown";

export function PrSummaryResult({ run }: { run: GraduationRun }) {
  const result = prSummaryResultOf(run);
  if (!result) return null;
  return (
    <section
      className="graduation__pr-summary"
      aria-label="Pull request summary"
      data-testid="graduation-pr-summary"
    >
      <p className="graduation__pr-summary-title" data-testid="graduation-pr-summary-title">
        {result.title}
      </p>
      <CommentMarkdown
        body={result.description}
        className="graduation__pr-summary-body"
        testId="graduation-pr-summary-description"
      />
    </section>
  );
}

/**
 * The **Description** field of the Create a PR window, its **Write** and
 * **Preview** tabs, **Generate**, and the state of the PR summary run it
 * follows (`../../../specifications/ui/CPR-create-pull-request.md`
 * CPR-FR-ILFN, CPR-FR-UNAN, CPR-FR-VOEB, CPR-FR-URIL, CPR-FR-IOGF,
 * CPR-FR-ODBY).
 *
 * The Write tab is a plain field of Markdown text, exactly as typed. The
 * Preview tab renders it with inert links and edits nothing. The field stays
 * mounted while Preview shows, so the text and the caret are kept.
 */
import { useLayoutEffect, useRef, useState } from "react";

import type { PullRequestSummaryFollow } from "../../state/pullRequestSessions";
import {
  conditionOf,
  conditionSentence,
  PR_SUMMARY_STAGES,
  stageOf,
} from "../../state/graduation";
import { CommentMarkdown } from "../CommentMarkdown";
import { RunProgress } from "../RunProgress";

type Tab = "write" | "preview";

export interface DescriptionFieldProps {
  id: string;
  body: string;
  onBody: (body: string) => void;
  /** CPR-FR-XMRL: a request runs, so nothing can be used. */
  disabled: boolean;
  /** CPR-FR-VOEB / CPR-FR-IOGF: why Generate is unavailable, or null. */
  generateBlock: string | null;
  onGenerate: () => void;
  summary: PullRequestSummaryFollow | null;
  head: string;
}

/** CPR-FR-VOEB: the state of a run that is not terminal, in words. */
function runningWords(summary: PullRequestSummaryFollow): string {
  if (summary.status === "starting") return "Starting the PR summary run…";
  return summary.mode === "combine"
    ? "Writing the description from the run summaries…"
    : "Writing the title and the description from the diff…";
}

function GenerationState({
  summary,
  head,
}: {
  summary: PullRequestSummaryFollow | null;
  head: string;
}) {
  if (!summary) return null;
  if (summary.status === "starting" || summary.status === "running") {
    const run = summary.run;
    return (
      <div className="create-pr__generation" data-testid="create-pr-generation">
        <p className="t-meta create-pr__line" role="status">
          {runningWords(summary)}
        </p>
        {run && (
          <RunProgress
            stages={PR_SUMMARY_STAGES}
            currentStage={stageOf(run) ?? "queued"}
            condition={conditionOf(run)}
            conditionSentence={conditionSentence(run)}
            showLoops={false}
            label={`Progress of Summarize ${head}`}
          />
        )}
      </div>
    );
  }
  if (summary.status === "completed") {
    return (
      <p className="t-meta create-pr__line" role="status" data-testid="create-pr-generation-done">
        {summary.mode === "combine"
          ? "The description is written from the run summaries."
          : "The title and the description are written from the diff."}
      </p>
    );
  }
  // CPR-FR-URIL: the run or its start did not complete, with the typed cause.
  return (
    <p className="create-pr__block create-pr__line" role="alert" data-testid="create-pr-generation-failed">
      The generation did not complete: {summary.message ?? "The run failed."}
    </p>
  );
}

export function DescriptionField({
  id,
  body,
  onBody,
  disabled,
  generateBlock,
  onGenerate,
  summary,
  head,
}: DescriptionFieldProps) {
  const [tab, setTab] = useState<Tab>("write");
  const textRef = useRef<HTMLTextAreaElement>(null);
  const caret = useRef<{ start: number; end: number } | null>(null);
  const writeRef = useRef<HTMLButtonElement>(null);
  const previewRef = useRef<HTMLButtonElement>(null);

  const choose = (next: Tab) => {
    if (next === tab) return;
    const area = textRef.current;
    if (next === "preview" && area) {
      caret.current = { start: area.selectionStart, end: area.selectionEnd };
    }
    setTab(next);
  };

  // CPR-FR-ILFN: back on Write, the caret is where the author left it.
  useLayoutEffect(() => {
    if (tab !== "write" || !caret.current || !textRef.current) return;
    textRef.current.setSelectionRange(caret.current.start, caret.current.end);
  }, [tab]);

  // CPR-FR-NQPS: the arrow keys move between the two tabs.
  const onTabKey = (e: React.KeyboardEvent) => {
    if (e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    const next = tab === "write" ? "preview" : "write";
    choose(next);
    (next === "write" ? writeRef : previewRef).current?.focus();
  };

  const reasonId = `${id}-generate-reason`;
  const tabId = (t: Tab) => `${id}-tab-${t}`;
  const panelId = (t: Tab) => `${id}-panel-${t}`;

  return (
    <div className="picker-field create-pr__description">
      <div className="create-pr__description-head">
        <label className="picker-field__label" htmlFor={id}>
          Description
        </label>
        <div
          className="create-pr__tabs"
          role="tablist"
          aria-label="Description view"
          onKeyDown={onTabKey}
        >
          {(["write", "preview"] as const).map((t) => (
            <button
              key={t}
              ref={t === "write" ? writeRef : previewRef}
              type="button"
              role="tab"
              id={tabId(t)}
              aria-selected={tab === t}
              aria-controls={panelId(t)}
              tabIndex={tab === t ? 0 : -1}
              className="create-pr__tab"
              data-testid={`create-pr-tab-${t}`}
              onClick={() => choose(t)}
            >
              {t === "write" ? "Write" : "Preview"}
            </button>
          ))}
        </div>
        <button
          type="button"
          className="btn btn--ghost btn--sm create-pr__generate"
          disabled={disabled || generateBlock !== null}
          aria-describedby={generateBlock ? reasonId : undefined}
          data-testid="create-pr-generate"
          onClick={onGenerate}
        >
          Generate
        </button>
        {generateBlock && (
          <span id={reasonId} className="sr-only" data-testid="create-pr-generate-reason">
            {generateBlock}
          </span>
        )}
      </div>
      <div
        id={panelId("write")}
        role="tabpanel"
        aria-labelledby={tabId("write")}
        className="create-pr__panel"
        hidden={tab !== "write"}
      >
        <textarea
          id={id}
          ref={textRef}
          className="textarea create-pr__textarea"
          rows={12}
          value={body}
          disabled={disabled}
          spellCheck
          onChange={(e) => onBody(e.target.value)}
        />
      </div>
      <div
        id={panelId("preview")}
        role="tabpanel"
        aria-labelledby={tabId("preview")}
        className="create-pr__panel create-pr__preview"
        hidden={tab !== "preview"}
        tabIndex={0}
        data-testid="create-pr-preview"
      >
        {tab === "preview" &&
          (body.trim() === "" ? (
            <p className="t-muted">Nothing to preview.</p>
          ) : (
            <CommentMarkdown body={body} className="create-pr__preview-body" />
          ))}
      </div>
      <GenerationState summary={summary} head={head} />
      {generateBlock && !(summary?.status === "starting" || summary?.status === "running") && (
        <p className="t-meta create-pr__line" data-testid="create-pr-generate-block">
          {generateBlock}
        </p>
      )}
    </div>
  );
}

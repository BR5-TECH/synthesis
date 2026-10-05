/**
 * The pass history of the selected run
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-YYXN).
 *
 * One row per pass, each carrying its own one-sentence description and its
 * status, and — where the author opens it — what the pass was asked to do, the
 * review's verdict and rationale, its findings, and the instruction the next
 * turn was told.
 *
 * A vertical timeline: the stage row's own track turned through a right angle,
 * one run of work across the top and the passes it took down the side. Every
 * value it draws is a token this section already uses, so it brings no colour,
 * icon or interaction of its own (GRU-FR-NBRO).
 */

import { useLayoutEffect, useRef, useState } from "react";

import { logDebug } from "../../logging";
import { passHeading, severityLabel } from "../../state/graduation";
import type { PassRecord } from "../../types/graduationObservability";

export interface PassHistoryProps {
  passes: PassRecord[];
  /** The pass the run stands at, marked as the one it is on. */
  current: number;
  /**
   * GRU-FR-JAEY: the short cause of the stop while the run is interrupted, or
   * null while it is not. A pass still `working` then reads as stopped.
   */
  stoppedCause?: string | null;
}

export function PassHistory({ passes, current, stoppedCause = null }: PassHistoryProps) {
  // The newest pass is the one the author is most often reading, so it is the
  // one open when the section is first rendered.
  const [open, setOpen] = useState<number | null>(
    passes.length > 0 ? passes[passes.length - 1].pass : null,
  );
  /** The row control of each pass, by pass number. */
  const rowRefs = useRef(new Map<number, HTMLButtonElement>());
  /**
   * The pass whose row the author last activated, and how many times, so a
   * second press on the same row reveals it again.
   */
  const [toggled, setToggled] = useState<{ pass: number; seq: number } | null>(
    null,
  );
  // GRU-FR-ZYDL: an account that opens or closes moves every row under it, and
  // a long one that closes can leave the row the author pressed above the top
  // of the region. Once the layout has changed, the region brings that row back
  // into view. "nearest" moves nothing while the row is already in view.
  useLayoutEffect(() => {
    if (toggled === null) return;
    rowRefs.current
      .get(toggled.pass)
      ?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [toggled]);
  if (passes.length === 0) return null;
  const toggle = (pass: number) => {
    const opening = open !== pass;
    logDebug(
      ["frontend"],
      opening ? "a pass account was opened" : "a pass account was closed",
      { pass },
    );
    setOpen(opening ? pass : null);
    setToggled((held) => ({ pass, seq: (held?.seq ?? 0) + 1 }));
  };
  return (
    <section className="graduation__passes" data-testid="graduation-passes">
      <div className="graduation__passes-head">
        <span className="t-eyebrow">Passes</span>
        <span className="graduation__passes-count t-meta">
          {passes.length === 1 ? "1 pass" : `${passes.length} passes`}
        </span>
      </div>
      <div className="graduation__pass-list">
        {passes.map((pass) => (
          <div
            key={pass.pass}
            className="graduation__pass"
            data-open={open === pass.pass ? "true" : undefined}
            data-current={pass.pass === current ? "true" : undefined}
            data-testid="graduation-pass"
          >
            <button
              type="button"
              className="graduation__pass-row"
              aria-expanded={open === pass.pass}
              ref={(node) => {
                if (node) rowRefs.current.set(pass.pass, node);
                else rowRefs.current.delete(pass.pass);
              }}
              onClick={() => toggle(pass.pass)}
            >
              <span className="graduation__pass-caret" aria-hidden="true">
                {open === pass.pass ? "▾" : "▸"}
              </span>
              <span className="graduation__pass-mark" aria-hidden="true" />
              <span className="graduation__pass-name">
                {passHeading(pass.pass)}
              </span>
              <span className="graduation__pass-reason">
                {passStatusLabel(pass.status, stoppedCause)}
              </span>
              {pass.findings.length > 0 && (
                <span className="graduation__pass-findings t-meta">
                  {pass.findings.length === 1
                    ? "1 finding"
                    : `${pass.findings.length} findings`}
                </span>
              )}
            </button>

            {open === pass.pass && (
              <div className="graduation__account">
                {/* GRU-FR-MRPE: every text below was produced by an agent or a
                    model, so it is rendered as escaped plain text. The box
                    keeps its line structure; no parser reads it. */}
                <div className="graduation__account-block">
                  <p className="graduation__revision-head t-eyebrow">
                    What this pass was asked to do
                  </p>
                  <p className="graduation__revision-text">
                    {pass.task || "Nothing was recorded."}
                  </p>
                </div>

                {(pass.verdict || pass.rationale) && (
                  <div className="graduation__account-block">
                    <p className="graduation__revision-head t-eyebrow">
                      {pass.verdict === "ready"
                        ? "Why the review found the work ready"
                        : pass.verdict === "revise"
                          ? "Why the review asked for a revision"
                          : "What the review said"}
                    </p>
                    {pass.rationale ? (
                      <p className="graduation__revision-text">
                        {pass.rationale}
                      </p>
                    ) : (
                      <p className="graduation__revision-none">
                        The review gave no rationale.
                      </p>
                    )}
                  </div>
                )}

                {pass.findings.length > 0 && (
                  <ul className="graduation__account-findings">
                    {pass.findings.map((finding, index) => (
                      <li key={index} data-testid="graduation-finding">
                        <p className="graduation__revision-text">
                          <span className="graduation__finding-code t-meta">
                            {severityLabel(finding.severity)}
                          </span>{" "}
                          {finding.description}
                        </p>
                        {finding.affectedFiles.length > 0 && (
                          <p className="graduation__finding-paths t-meta">
                            {finding.affectedFiles.join(", ")}
                          </p>
                        )}
                        <p className="graduation__finding-fix graduation__revision-text">
                          {finding.correction}
                        </p>
                      </li>
                    ))}
                  </ul>
                )}

                {pass.nextInstruction && (
                  <div className="graduation__account-block">
                    <p className="graduation__revision-head t-eyebrow">
                      What the next turn was told
                    </p>
                    <p className="graduation__revision-text">
                      {pass.nextInstruction}
                    </p>
                  </div>
                )}
              </div>
            )}
          </div>
        ))}
      </div>
    </section>
  );
}

/**
 * GRU-FR-YYXN: what one pass is doing, or what it decided, in words.
 *
 * How many findings it raised is a count beside this rather than a part of it:
 * the row ellipsizes its description, and a count truncated to "3 fin…" is the
 * one thing on the row that must not be read as half of itself.
 */
export function passStatusLabel(
  status: string,
  stoppedCause: string | null = null,
): string {
  // GRU-FR-JAEY: the pass is not settled, so its record still says `working`.
  // The run is not working, so the row says it stopped and why.
  if (status === "working") return stoppedCause ? `Stopped — ${stoppedCause}` : "Working";
  if (status === "passed") return "The review found the work ready";
  return "The review asked for a revision";
}

/**
 * What a merge run's region says that a draft run's does not
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-NWEC,
 * GRU-FR-AJGM, GRU-FR-JRMA, and
 * `../../../specifications/ui/GEA-graduation-escalation-answering.md`
 * GEA-FR-WQZH).
 *
 * Three small parts: the publication choice the run holds, the paths the merge
 * changes with the ones Git could not settle named, and — once the run is
 * `completed` — the result that landed. Every text here is read text: a path
 * opens nothing, and the author's commit message renders as escaped text
 * (GRU-FR-MRPE).
 */

import {
  fateByPath,
  mergeResultSentence,
  publicationSentence,
  shortCommit,
} from "../../state/graduation";
import type { GraduationRun } from "../../types";
import { PathListSection } from "./changeSet";

/** GRU-FR-NWEC: the publication choice the run holds. */
export function MergePublication({
  run,
  testId = "graduation-merge-publication",
}: {
  run: GraduationRun;
  testId?: string;
}) {
  const merge = run.merge;
  if (!merge) return null;
  const publication = merge.publication;
  return (
    <p
      className="t-meta graduation__merge-publication"
      data-testid={testId}
    >
      Publication: {publicationSentence(publication)}
      {publication.kind === "commit" && (
        <>
          {" "}
          Message: <q>{publication.message}</q>
        </>
      )}
    </p>
  );
}

/**
 * GRU-FR-AJGM: the paths Git could not settle, each with what either side did
 * to it, and how many paths the merge changes in all.
 */
export function MergePaths({ run }: { run: GraduationRun }) {
  const merge = run.merge;
  if (!merge) return null;
  const unresolved = merge.unresolvedPaths ?? [];
  const changed = merge.changedPaths?.length ?? 0;
  const inAll = `${changed} ${changed === 1 ? "path" : "paths"} changed in all`;
  if (unresolved.length === 0) {
    return (
      <p className="t-meta" data-testid="graduation-merge-changed">
        Git settled every path. {inAll}.
      </p>
    );
  }
  return (
    <PathListSection
      // Keyed by run, so another run's list opens by its own rule
      // (GRU-FR-NUCJ).
      key={`unresolved-${run.id}`}
      label="Unresolved"
      paths={unresolved}
      annotations={fateByPath(merge)}
      suffix={` · ${inAll}`}
      testId="graduation-unresolved"
    />
  );
}

/**
 * GRU-FR-JRMA: what landed. A merge run that is not `completed` renders none.
 * The commit is named in short form with the whole id in accessible semantics.
 */
export function MergeResult({ run }: { run: GraduationRun }) {
  const merge = run.merge;
  const sentence = mergeResultSentence(run);
  if (!merge?.result || !sentence) return null;
  const commit = merge.result.published === "commit" ? merge.result.commit : null;
  const merged = merge.result.mergedPaths ?? [];
  return (
    <section className="graduation__merge-result" data-testid="graduation-merge-result">
      <p className="graduation__outcome">
        {sentence}
        {commit && (
          <>
            {" "}
            Commit{" "}
            <code
              data-testid="graduation-merge-commit"
              title={commit}
              aria-label={`Commit ${commit}`}
            >
              {shortCommit(commit)}
            </code>
            .
          </>
        )}
      </p>
      {merged.length > 0 && (
        <PathListSection
          key={`merged-${run.id}`}
          label="Merged"
          paths={merged}
          testId="graduation-merged"
        />
      )}
    </section>
  );
}

/**
 * GEA-FR-WQZH: what a merge run's escalation additionally names — the
 * unresolved paths and the publication choice, both read from the run.
 */
export function MergeEscalationContext({ run }: { run: GraduationRun }) {
  const merge = run.merge;
  if (!merge) return null;
  const unresolved = merge.unresolvedPaths ?? [];
  return (
    <div className="graduation__merge-context" data-testid="graduation-merge-context">
      {unresolved.length > 0 && (
        <PathListSection
          key={`escalation-unresolved-${run.id}`}
          label="Paths the merge asks about"
          paths={unresolved}
          annotations={fateByPath(merge)}
          testId="graduation-escalation-paths"
        />
      )}
      <MergePublication run={run} testId="graduation-escalation-publication" />
    </div>
  );
}

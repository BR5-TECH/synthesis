/**
 * The text of the confirmation and of the hard block, which the standalone
 * dialog and the inline section both render (MBW-FR-RXAR, MBW-FR-JZME,
 * MBW-FR-UNAN, MBW-FR-WCBQ).
 */
import {
  middleTruncate,
  protectionCause,
  reasonSentence,
  writeSentence,
} from "../../state/mainBranchWrite";
import type { MainBranchWriteKind } from "../../types";
import type { PanelState } from "./usePanelState";

/** A branch name cut in the middle, with its whole value in the semantics. */
export function BranchName({ name }: { name: string }) {
  const shown = middleTruncate(name);
  return (
    <code className="main-branch-write__branch" title={name} aria-label={name}>
      {shown}
    </code>
  );
}

export function isHardBlock(state: PanelState): boolean {
  return state.guard?.protection === "protected";
}

/** The words of the panel, with the actions that change what it reads. */
export function MainBranchWriteText({
  state,
  write,
  idPrefix,
}: {
  state: PanelState;
  write: MainBranchWriteKind;
  idPrefix: string;
}) {
  const { guard, readError, confirmError, busy } = state;
  if (!guard) {
    if (!readError) {
      return (
        <p className="main-branch-write__text" id={`${idPrefix}-text`}>
          Reading the branch…
        </p>
      );
    }
    return (
      <div id={`${idPrefix}-text`}>
        <p className="main-branch-write__text">
          The branch of this write could not be checked. Nothing is written until it is.
        </p>
        <p className="main-branch-write__notice" role="alert">
          <span>{readError}</span>
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            disabled={busy}
            onClick={() => void state.tryAgain()}
          >
            Try again
          </button>
        </p>
      </div>
    );
  }
  if (isHardBlock(state)) {
    return (
      <div id={`${idPrefix}-text`}>
        <p className="main-branch-write__text">
          GitHub protects <BranchName name={guard.branch} />. Work reaches it
          through a pull request. Start a new branch for this work.
        </p>
      </div>
    );
  }
  return (
    <div id={`${idPrefix}-text`}>
      <p className="main-branch-write__text">{reasonSentence(guard)}</p>
      <p className="main-branch-write__text">{writeSentence(write, guard.branch)}</p>
      {guard.isDefault && guard.protection === "unknown" && (
        <p className="main-branch-write__notice">
          <span>
            The branch protection could not be read:{" "}
            {protectionCause(guard.protectionError)}
          </span>
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            disabled={busy}
            onClick={() => void state.checkAgain()}
          >
            Check again
          </button>
        </p>
      )}
      {confirmError && (
        <p className="main-branch-write__error" role="alert">
          The confirmation failed: {confirmError}
        </p>
      )}
    </div>
  );
}

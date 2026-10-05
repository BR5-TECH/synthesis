/**
 * The publication surfaces of the New Artifact tab — the publication tag, the
 * publication band, the remote picker, and the recovery choice
 * (`NAW-new-artifact.md` NAW-FR-RVGT, NAW-FR-MFXO, NAW-FR-JBHV, NAW-FR-LQAF,
 * NAW-FR-EOTB, NAW-FR-HZSW, NAW-FR-CBUJ, NAW-FR-XRLD, NAW-FR-QTVX).
 *
 * The tag states the draft's publication and nothing more: it edits, reorders,
 * and removes no record, and its one control leaves the application entirely
 * (NAW-FR-XRLD). The records themselves are read in the Draft Information
 * modal (per `DFI-draft-information.md` DFI-FR-BZQN), so this tab states the
 * current publication rather than listing every one again.
 */
import { useState } from "react";
import { Icon } from "./icons";
import { Modal } from "./DraftsPanelParts";
import { PublicationChooser } from "./PublicationChooser";
import {
  describeMismatches,
  describePublicationChoice,
} from "../state/publicationChoice";
import type {
  PublicationRemoteResolution,
  PublicationSelectionOrigin,
} from "../types";
import type {
  RecoveryPrompt,
  UseDraftPublicationResult,
} from "../hooks/useDraftPublication";

/** The instant a record was published, as the surfaces render it. */
function publishedAt(value: string): string {
  const at = new Date(value);
  return Number.isNaN(at.getTime()) ? value : at.toLocaleString();
}

/** NAW-FR-KDMW: the five conditions the tag states, and no sixth. */
type PublicationTagState =
  | "published"
  | "publishing"
  | "unfinished"
  | "awaiting_choice"
  | "failed";

/**
 * NAW-FR-XPUJ: which condition the tag names where more than one holds.
 *
 * NAW-FR-KDMW turns on one distinction. An attempt in the READ is recorded on
 * disk and is not running: once the publish call has returned, an attempt still
 * standing is one that stopped. So `publishing` comes from the call this tab is
 * waiting on and from nothing else, and an attempt found in the read is
 * `unfinished` — which is honest about an application closed part way through a
 * publish, where whether the issue exists is exactly what the recovery search
 * settles (per `../../specifications/core/GHP-github-publication.md`
 * GHP-FR-ZFPI).
 */
function publicationTagState(
  publication: UseDraftPublicationResult,
): PublicationTagState | null {
  const attempt = publication.view?.attempt ?? null;
  if (attempt?.state === "awaiting_choice") return "awaiting_choice";
  if (publication.error) return "failed";
  if (publication.publishing) return "publishing";
  if (attempt) return "unfinished";
  return publication.view?.current ? "published" : null;
}

/** NAW-FR-QTVX: what a tag that names no record states. */
const TAG_CONDITION: Record<Exclude<PublicationTagState, "published">, string> = {
  publishing: "A publication is in progress.",
  unfinished: "This publication did not finish. Try it again or abandon it.",
  awaiting_choice: "This publication needs a recovery choice.",
  failed: "The last publication did not finish.",
};

/**
 * NAW-FR-CBUJ / NAW-FR-XRLD / NAW-FR-QTVX: the publication tag in the tab's
 * chrome row, beside the archived marker and the run-state tag.
 *
 * A tag that names a record is the route to that issue and is therefore a
 * button. A tag that names an attempt or a failure is not a route out of the
 * application, so it is not one: a control that does nothing when it is
 * activated is worse than a statement.
 */
export function DraftPublicationTag({
  publication,
}: {
  publication: UseDraftPublicationResult;
}) {
  const state = publicationTagState(publication);
  if (state === null) return null;

  if (state === "published") {
    const record = publication.view!.current!;
    const repository = `${record.repositoryOwner}/${record.repositoryName}`;
    // NAW-FR-QTVX: the repository, the issue, and the instant — none of which
    // the tag has the width to say.
    const title = `Open issue #${record.issueNumber} in ${repository}, published ${publishedAt(record.publishedAt)}`;
    return (
      <button
        type="button"
        className="draft-workspace__publication t-ui-xs"
        data-testid="draft-publication-tag"
        data-state="published"
        title={title}
        aria-label={title}
        onClick={() => publication.openIssue(record.issueUrl)}
      >
        <Icon.Link size={11} /> Published #{record.issueNumber}
      </button>
    );
  }

  const label = {
    publishing: "Publishing…",
    unfinished: "Publication unfinished",
    awaiting_choice: "Needs a choice",
    failed: "Publish failed",
  }[state];
  return (
    <span
      className="draft-workspace__publication t-ui-xs"
      data-testid="draft-publication-tag"
      data-state={state}
      // NAW-FR-QTVX: the condition in the accessible name as well as in the
      // hover text. A bare `span` takes neither, so the tag carries a role that
      // may be named — a static one, the band below being what announces a
      // change (NAW-FR-LQAF, NAW-FR-HZSW).
      role="note"
      aria-label={TAG_CONDITION[state]}
      title={TAG_CONDITION[state]}
    >
      {state === "publishing" ? (
        <span className="dot dot--live" aria-hidden="true" />
      ) : (
        <Icon.Warning size={11} />
      )}
      {label}
    </span>
  );
}

/**
 * NAW-FR-LQAF / NAW-FR-HZSW: the publication band under the tab's chrome row,
 * standing only while an attempt or a failure stands.
 *
 * A settled publication needs no band — the tag says everything a record has to
 * say here, and the records are read in Draft Information.
 */
export function DraftPublicationBand({
  publication,
}: {
  publication: UseDraftPublicationResult;
}) {
  const { view, busy, error, retry, abandon } = publication;
  const attempt = view?.attempt ?? null;
  if (!attempt && !error) return null;
  // The band states the condition the tag states, from the same function. Two
  // sentences over one condition can disagree, and the pair that disagreed —
  // a tag naming a failure over a band naming work in flight — is the one this
  // surface exists to prevent.
  const state = publicationTagState(publication) ?? "unfinished";

  return (
    <div
      className="draft-workspace__publication-band t-ui-xs"
      data-testid="draft-publication-band"
      data-state={state}
    >
      {/* NAW-FR-LQAF: the attempt, with the affordances it admits. An attempt
          read back after a relaunch renders exactly the same way, so a draft
          holding one is never left permanently unpublishable. */}
      {attempt && (
        <div className="draft-workspace__publication-line" role="status">
          <span>
            {TAG_CONDITION[state === "published" ? "unfinished" : state]}
            {/* NAW-FR-NHAY: the choice the attempt saved, which a retry keeps. */}
            <span
              className="draft-workspace__publication-choice"
              data-testid="draft-publication-choice"
            >
              {` ${describePublicationChoice(attempt.choice)}.`}
            </span>
          </span>
          <span className="draft-workspace__publication-actions">
            {/* An `awaiting_choice` attempt reaches the recovery choice through
                the same call: the retry searches for the marker again and
                answers `recovery_required`, which opens the dialog. */}
            <button
              className="btn btn--sm"
              type="button"
              disabled={busy}
              onClick={() => void retry()}
            >
              {attempt.state === "awaiting_choice" ? "Recover" : "Retry"}
            </button>
            <button
              className="btn btn--sm"
              type="button"
              disabled={busy}
              onClick={() => void abandon()}
            >
              Abandon
            </button>
          </span>
        </div>
      )}

      {/* NAW-FR-HZSW: the failure is stated where the author is looking, and
          the retry affordance above it stays in place. */}
      {error && (
        <p className="draft-workspace__publication-error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

/**
 * NAW-FR-ZQMX / NAW-FR-VBHT / NAW-FR-PNCL: what a disabled **Publish to
 * GitHub** says, on hover and in its accessible name.
 *
 * Taken from the one read rather than composed here; the local-asset case
 * additionally lists every affected path.
 */
export function publicationActionTitle(
  publication: UseDraftPublicationResult,
): string {
  const eligibility = publication.view?.eligibility;
  const offered = "Publish this draft as a GitHub issue";
  if (!eligibility || eligibility.publishable) return offered;
  const reason = eligibility.reason ?? "Publication is not available";
  return eligibility.localAssets.length > 0
    ? `${reason} ${eligibility.localAssets.join(", ")}`
    : reason;
}

/**
 * NAW-FR-DWKA / NAW-FR-HNVR / NAW-FR-EOTB: the three overlays the flow opens,
 * and none stands unless the flow put it there.
 */
export function PublicationOverlays({
  publication,
}: {
  publication: UseDraftPublicationResult;
}) {
  return (
    <>
      {publication.remotes && (
        <PublicationRemotePicker
          resolution={publication.remotes}
          busy={publication.busy}
          onCancel={publication.closePicker}
          onConfirm={(remoteName, persist) =>
            void publication.confirmPicker(remoteName, persist)
          }
        />
      )}
      {publication.chooser && (
        <PublicationChooser
          state={publication.chooser}
          busy={publication.busy}
          onCancel={publication.closeChooser}
          onConfirm={(choice) => void publication.confirmChooser(choice)}
        />
      )}
      {publication.recovery && (
        <PublicationRecoveryDialog
          prompt={publication.recovery}
          busy={publication.busy}
          error={publication.error}
          onCancel={() => void publication.cancelRecovery()}
          onAnswer={(choice) => void publication.answerRecovery(choice)}
        />
      )}
    </>
  );
}

/** NAW-FR-MFXO: how the picker states the standing choice. */
function originSentence(origin: PublicationSelectionOrigin): string {
  switch (origin) {
    case "automatic":
      return "This is the project's only usable remote, so it is chosen automatically.";
    case "persisted":
      return "This remote is remembered for this project.";
    case "attempt_only":
      return "This choice applies to this publication only.";
    default:
      return "No remote can receive this issue.";
  }
}

/**
 * NAW-FR-RVGT / NAW-FR-MFXO / NAW-FR-JBHV: the remote picker.
 *
 * Every configured remote is listed. An ineligible one renders disabled with
 * its own exact reason beneath it rather than being omitted, so the author
 * reads why a remote they configured cannot receive an issue.
 */
export function PublicationRemotePicker({
  resolution,
  busy,
  onCancel,
  onConfirm,
}: {
  resolution: PublicationRemoteResolution;
  busy: boolean;
  onCancel: () => void;
  onConfirm: (remoteName: string, persist: boolean) => void;
}) {
  const [chosen, setChosen] = useState<string | null>(resolution.selection);
  const [persist, setPersist] = useState(resolution.origin === "persisted");

  return (
    <Modal label="Publish to GitHub" onClose={onCancel}>
      <>
        <div className="modal__head">
          <div className="modal__title">Publish to GitHub</div>
        </div>
        <div className="modal__body">
          <p className="t-ui-sm">Choose the remote this issue goes to.</p>
          {/* NAW-FR-MFXO: whether the standing choice is automatic, persisted,
              or this attempt's alone, in words. */}
          <p className="t-ui-xs draft-publication-picker__origin">
            {originSentence(resolution.origin)}
          </p>
          <ul className="draft-publication-picker__remotes">
            {resolution.remotes.map((remote) => {
              const eligible = remote.eligibility === "eligible";
              return (
                <li key={remote.name}>
                  <label className="t-ui-sm">
                    <input
                      type="radio"
                      name="publication-remote"
                      value={remote.name}
                      disabled={!eligible}
                      checked={chosen === remote.name}
                      onChange={() => setChosen(remote.name)}
                    />
                    <span className="draft-publication-picker__name">
                      {remote.name}
                    </span>
                    <span className="draft-publication-picker__url t-meta">
                      {remote.url}
                    </span>
                  </label>
                  {/* NAW-FR-RVGT: the exact reason, beneath the entry it is
                      about, rather than the entry being dropped. */}
                  {!eligible && remote.reason && (
                    <p className="t-ui-xs draft-publication-picker__reason">
                      {remote.reason}
                    </p>
                  )}
                </li>
              );
            })}
          </ul>
          <label className="t-ui-sm draft-publication-picker__persist">
            <input
              type="checkbox"
              checked={persist}
              onChange={(event) => setPersist(event.target.checked)}
            />
            Use this remote for future publications
          </label>
        </div>
        <div className="modal__actions">
          <button className="btn btn--ghost" disabled={busy} onClick={onCancel}>
            Cancel
          </button>
          <button
            className="btn btn--primary"
            disabled={!chosen || busy}
            onClick={() => chosen && onConfirm(chosen, persist)}
          >
            Publish
          </button>
        </div>
      </>
    </Modal>
  );
}

/**
 * NAW-FR-EOTB: the recovery choice, naming the issue the marker was found on.
 *
 * Cancelling leaves the attempt recoverable and adds no history entry, which is
 * why it is a third answer rather than a dismissal that decides nothing.
 */
export function PublicationRecoveryDialog({
  prompt,
  busy,
  error = null,
  onCancel,
  onAnswer,
}: {
  prompt: RecoveryPrompt;
  busy: boolean;
  /** NAW-FR-HZSW: the failure of the last answer, stated where it is read. */
  error?: string | null;
  onCancel: () => void;
  onAnswer: (choice: "update_existing" | "publish_new") => void;
}) {
  return (
    <Modal label="Recover publication" onClose={onCancel}>
      <>
        <div className="modal__head">
          <div className="modal__title">Recover publication</div>
        </div>
        <div className="modal__body">
          <p className="t-ui-sm">
            {prompt.mismatches.length === 0
              ? `Issue #${prompt.issueNumber} already carries this attempt's marker, and its title or body is not the draft's latest.`
              : `Issue #${prompt.issueNumber} already carries this attempt's marker, and its ${describeMismatches(prompt.mismatches)} differ from the draft's latest title and body and the saved choice.`}
          </p>
          {error && (
            <p className="t-ui-sm draft-workspace__publication-error" role="alert">
              {error}
            </p>
          )}
        </div>
        <div className="modal__actions">
          <button className="btn btn--ghost" disabled={busy} onClick={onCancel}>
            Cancel
          </button>
          <button
            className="btn"
            disabled={busy}
            onClick={() => onAnswer("publish_new")}
          >
            Publish as a new issue
          </button>
          <button
            className="btn btn--primary"
            disabled={busy}
            onClick={() => onAnswer("update_existing")}
          >
            Update existing issue
          </button>
        </div>
      </>
    </Modal>
  );
}

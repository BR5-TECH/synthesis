/**
 * The dialog a graduation starts from
 * (`../../specifications/ui/GSD-graduation-start-dialog.md`).
 *
 * It opens from a draft's own **Graduate** action, from the **Graduate…** entry
 * of a GitHub-shadow row in the Drafts panel, and from the Ready tasks section
 * of the Git panel, and from nowhere else (GSD-FR-QMTF). A GitHub-shadow draft
 * gets the same dialog with the same questions (GSD-FR-LXAF). It asks where the
 * run works (GSD-FR-BZHW): an existing work stream, a stream it creates, or the
 * worktree that is active when the author confirms. For a stream run that will
 * wait, it also asks what the run does with work standing uncommitted there
 * when its turn comes and under what message (GSD-FR-VKLD, GSD-FR-WQPD,
 * GSD-FR-MZTB). A stream run waits when the chosen stream is occupied, or when
 * the project-wide limit of graduation runs is full. A new stream is free, so
 * it asks only where the limit is full (GSD-FR-QGTC). The dialog reads the
 * capacity when it opens and on every queue change (GSD-FR-LHQY). It states
 * each reason for a wait (GSD-FR-KDBU), and it states a capacity it cannot read
 * and then treats the limit as not full (GSD-FR-NOID). Work directly asks
 * nothing. Every answer travels with the run, so a run that waits behind
 * another one asks the author nothing when it starts.
 */

import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../api";
import { onWorktreeContextChanged } from "../events";
import { logError, logInfo } from "../logging";
import {
  DEFAULT_STANDING_WORK,
  defaultCommitMessage,
  graduationErrorMessage,
  splitTyped,
} from "../state/graduation";
import type {
  DirectGraduationPreflight,
  GraduationRun,
  StandingWork,
  WorkStreamSummary,
} from "../types";
import {
  isOccupied,
  isProjectLimitFull,
  standingWorkCommits,
} from "../state/graduation";
import { Icon } from "./icons";
import { StandingWorkChoice } from "./StandingWorkChoice";
import { CapacityUnread, SlotWait, useGraduationCapacity } from "./GraduationStart/capacity";
import { DestinationChoice, type Destination } from "./GraduationStart/destination";
import { DirectChoice, directBlocker } from "./GraduationStart/direct";
import { NewStreamFields } from "./GraduationStart/newStream";
import { useGraduationQueueRuns } from "./GraduationStart/queue";
import { refusalText, splitRefusal } from "./WorkStreamSelector/refusals";

/** The refusals of a direct start that the preflight must be read again for. */
const DIRECT_REFUSALS = [
  "worktree_identity_changed",
  "direct_branch_changed",
  "direct_worktree_detached",
  "direct_worktree_dirty",
];

export interface GraduationStartProps {
  draftId: string;
  draftName: string;
  onClose: () => void;
  onStarted: (run: GraduationRun) => void;
}

export function GraduationStart({
  draftId,
  draftName,
  onClose,
  onStarted,
}: GraduationStartProps) {
  const [streams, setStreams] = useState<WorkStreamSummary[] | null>(null);
  const [streamId, setStreamId] = useState<string>("");
  /** GSD-FR-BZHW: where the run works. */
  const [destination, setDestination] = useState<Destination>("stream");
  /** GSD-FR-QGTC: the fields of New stream. */
  const [newName, setNewName] = useState("");
  const [newBranch, setNewBranch] = useState("");
  const [branches, setBranches] = useState<string[]>([]);
  const [nameError, setNameError] = useState<string | null>(null);
  /** GSD-FR-FQQP: what the active worktree says, read when Work directly is chosen. */
  const [preflight, setPreflight] = useState<DirectGraduationPreflight | null>(null);
  const [preflightError, setPreflightError] = useState<string | null>(null);
  const nameRef = useRef<HTMLInputElement>(null);
  const [standingWork, setStandingWork] =
    useState<StandingWork>(DEFAULT_STANDING_WORK);
  const [message, setMessage] = useState("");
  /** GSD-FR-HVDN: the runs the default of the message is read from. */
  const queueRuns = useGraduationQueueRuns(draftId);
  /** The stream the message was last defaulted for, and whether the author typed since. */
  const defaultedFor = useRef<string | null>(null);
  const messageEdited = useRef(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  /** GSD-FR-CXVA: whether the streams listing itself could not be read. */
  const [unreadable, setUnreadable] = useState(false);
  const pickerRef = useRef<HTMLSelectElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  /** GSD-FR-LHQY: the project-wide limit of graduation runs. */
  const capacityRead = useGraduationCapacity(draftId);
  const errorRef = useRef<HTMLParagraphElement>(null);

  // GSD-FR-WMTD / SNV-FR-56: it is the one overlay while it stands, and Escape
  // dismisses it — from the keyboard, wherever focus is, on the terms every
  // other window of the shell sets.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, busy]);

  // GSD-FR-WMTD: the first thing the dialog asks takes focus as soon as there
  // is something to choose, so it is answerable without a pointer. A dialog
  // with nothing to answer — no stream, or a listing that could not be read —
  // puts the keyboard on its own close control instead, because focus left
  // outside an overlay reaches the surface underneath it.
  useEffect(() => {
    if (streams === null) return;
    (pickerRef.current ?? closeRef.current)?.focus();
  }, [streams]);

  // GSD-FR-JYRP: a refusal renders at the foot of a body that scrolls, so it
  // is brought into view. A refusal the author cannot see reads as a Graduate
  // control that did nothing.
  useEffect(() => {
    if (error) errorRef.current?.scrollIntoView?.({ block: "nearest" });
  }, [error]);

  useEffect(() => {
    let live = true;
    api
      .listWorkStreams()
      .then((found) => {
        if (!live) return;
        // GSD-FR-ZPWN / WSS-FR-OQYG: a stream the backend reports missing has
        // no working copy to run in, so it is not one this window offers.
        const usable = found.filter(({ stream }) => !stream.isMissing);
        setStreams(usable);
        setStreamId(usable[0]?.stream.id ?? "");
        // GSD-FR-BZHW / GSD-FR-LDGM: the dialog rests on an existing stream
        // where the project holds one, and on New stream where it holds none.
        setDestination(usable.length > 0 ? "stream" : "new");
      })
      .catch((reason) => {
        if (!live) return;
        setStreams([]);
        setUnreadable(true);
        setDestination("new");
        // The typed code alone: a refusal's detail carries project paths, and
        // nothing downstream redacts what reaches the Logs panel.
        const [code] = splitTyped(String(reason));
        logError(["frontend"], "the project's work streams could not be read", {
          draftId,
          code,
        });
        setError(graduationErrorMessage(String(reason)));
      });
    return () => {
      live = false;
    };
  }, []);

  /**
   * GSD-FR-FQQP / GSD-FR-YBLC: read the active worktree. Held on the draft's id
   * alone so a read made on an event does not restart the dialog.
   */
  const readPreflight = useCallback(async () => {
    try {
      const found = await api.preflightDirectGraduation();
      setPreflight(found);
      setPreflightError(null);
    } catch (reason) {
      const [code] = splitTyped(String(reason));
      logError(["frontend"], "the active worktree could not be read for a direct start", {
        draftId,
        code,
      });
      setPreflight(null);
      setPreflightError(graduationErrorMessage(String(reason)));
    }
  }, [draftId]);

  // GSD-FR-FQQP: the preflight is read when Work directly is chosen, and again
  // whenever the worktree or its branch changes under the open dialog.
  useEffect(() => {
    if (destination !== "direct") return;
    void readPreflight();
    let unlisten: (() => void) | null = null;
    let live = true;
    void onWorktreeContextChanged(() => void readPreflight()).then((fn) => {
      if (live) unlisten = fn;
      else fn();
    });
    return () => {
      live = false;
      unlisten?.();
    };
  }, [destination, readPreflight]);

  // GSD-FR-QGTC: the branches a new stream may be created from, read when New
  // stream is chosen. The branch checked out in the active worktree rests first.
  useEffect(() => {
    if (destination !== "new") return;
    nameRef.current?.focus();
    let live = true;
    api
      .listWorktreesAndBranches()
      .then((listing) => {
        if (!live) return;
        const active = listing.worktrees.find((entry) => entry.isActive);
        const names = [
          active?.branch,
          ...listing.worktrees
            .filter((entry) => !entry.stream && !entry.isDetached)
            .map((entry) => entry.branch),
          ...listing.branches.map((entry) => entry.name),
        ].filter((name): name is string => Boolean(name));
        const unique = Array.from(new Set(names));
        setBranches(unique);
        setNewBranch((held) => held || unique[0] || "");
      })
      .catch(() => {
        if (live) setBranches([]);
      });
    return () => {
      live = false;
    };
  }, [destination]);

  // GSD-FR-HVDN: the message of the first selected stream is prefilled once
  // the runs are read, unless the author typed before they arrived. New stream
  // has no such default, so nothing is prefilled while it is the destination.
  useEffect(() => {
    if (destination !== "stream") return;
    if (!queueRuns || !streamId || defaultedFor.current === streamId) return;
    defaultedFor.current = streamId;
    if (!messageEdited.current) setMessage(defaultCommitMessage(queueRuns, streamId));
  }, [queueRuns, streamId, destination]);

  /**
   * GSD-FR-HVDN: another existing stream replaces the message with its default,
   * also where the author edited the previous value.
   */
  const editMessage = (value: string) => {
    messageEdited.current = true;
    setMessage(value);
  };

  const selectStream = (nextId: string) => {
    setStreamId(nextId);
    messageEdited.current = false;
    defaultedFor.current = queueRuns ? nextId : null;
    setMessage(queueRuns ? defaultCommitMessage(queueRuns, nextId) : "");
  };

  /**
   * GSD-FR-HVDN: New stream starts with an empty field, so the message the
   * other destination held is cleared. A return to Existing stream applies the
   * selected stream's default again.
   */
  const selectDestination = (value: Destination) => {
    setError(null);
    if (value === "new" && destination !== "new") {
      messageEdited.current = false;
      defaultedFor.current = null;
      setMessage("");
    } else if (value === "stream" && destination === "new") {
      selectStream(streamId);
    }
    setDestination(value);
  };

  /** GSD-FR-TZGT: re-read the listing, keeping a stream the dialog created. */
  const adoptStream = async (createdId: string) => {
    try {
      const found = await api.listWorkStreams();
      const usable = found.filter(({ stream }) => !stream.isMissing);
      setStreams(usable);
    } catch {
      // The listing is a convenience here; the stream exists either way.
    }
    // GSD-FR-HVDN: a stream the dialog created is no other selection, so the
    // message stays as the author left it.
    defaultedFor.current = createdId;
    setStreamId(createdId);
    setDestination("stream");
    setNewName("");
  };

  const start = async () => {
    // GSD-FR-BFOU: one start at a time.
    if (busy) return;
    if (destination === "stream" && !streamId) return;
    if (destination === "new" && !newName.trim()) return;
    if (destination === "direct" && (!preflight || directBlocker(preflight))) return;
    setBusy(true);
    setError(null);
    setNameError(null);
    let createdStream: string | null = null;
    try {
      let run: GraduationRun;
      if (destination === "direct" && preflight) {
        // GSD-FR-USOH: the start is bound to the worktree and branch the
        // author was shown; the backend refuses where either has moved.
        run = await api.startDirectGraduation(
          draftId,
          preflight.worktreePath,
          preflight.branch ?? "",
        );
      } else if (destination === "new") {
        // GSD-FR-QGTC: the stream is made first, free, and the run starts on it.
        try {
          const created = await api.createWorkStream(newName.trim(), newBranch || undefined);
          createdStream = created.id;
        } catch (reason) {
          // A name refusal renders against the name field; the dialog stays.
          const [code] = splitRefusal(String(reason));
          logError(["frontend"], "a work stream could not be created for a graduation", {
            draftId,
            code,
          });
          setNameError(refusalText(String(reason)));
          return;
        }
        // GSD-FR-QGTC: the choice travels only where the limit made it ask.
        run = await api.startGraduation(
          draftId,
          createdStream,
          newWaits ? standingWork : DEFAULT_STANDING_WORK,
          newWaits && standingWorkCommits(standingWork)
            ? message.trim() || null
            : null,
        );
      } else {
        run = await api.startGraduation(
          draftId,
          streamId,
          streamWaits ? standingWork : DEFAULT_STANDING_WORK,
          // GSD-FR-MZTB: the message belongs to a commit, so a choice that makes
          // none carries none — whatever the author typed before they chose it.
          streamWaits && standingWorkCommits(standingWork)
            ? message.trim() || null
            : null,
        );
      }
      onStarted(run);
    } catch (reason) {
      // GSD-FR-JYRP: the refusal is rendered here, every answer is left as the
      // author set it, and nothing about the draft changed.
      const text = String(reason);
      const [code] = splitTyped(text);
      setError(graduationErrorMessage(text));
      logError(["frontend"], "graduation could not be started", { draftId, code });
      if (createdStream) {
        // GSD-FR-TZGT: the stream the dialog made stays, and a retry starts on
        // it instead of making a second.
        logInfo(["frontend"], "a refused start kept the work stream the dialog created", {
          draftId,
        });
        await adoptStream(createdStream);
      }
      // GSD-FR-YBLC: a refusal that names the worktree is read again.
      if (destination === "direct" && DIRECT_REFUSALS.includes(code)) {
        await readPreflight();
      }
    } finally {
      setBusy(false);
    }
  };

  const chosen = streams?.find((s) => s.stream.id === streamId);
  // GSD-FR-WQPD: a stream that dispatches at once makes the question one the
  // author can answer by looking, so it is not asked. What the run carries
  // then is the resting position and no message of its own.
  const occupied = isOccupied(chosen);
  // GSD-FR-LHQY / GSD-FR-NOID: a limit that is full, or a capacity not read.
  const capacity = capacityRead.status === "read" ? capacityRead.capacity : null;
  const limitFull = isProjectLimitFull(capacity);
  const fullLimit = typeof capacity?.limit === "number" ? capacity.limit : null;
  // GSD-FR-WQPD: a stream run waits when its stream is occupied or the limit is
  // full. A new stream is free, so only the limit makes it wait (GSD-FR-QGTC).
  const streamWaits = occupied || limitFull;
  const newWaits = limitFull;
  const canStart =
    !busy &&
    (destination === "stream"
      ? Boolean(streamId)
      : destination === "new"
        ? newName.trim() !== ""
        : Boolean(preflight) && !directBlocker(preflight));

  return (
    <div
      className="scrim"
      role="presentation"
      onClick={(event) => {
        // GSD-FR-WMTD: a click on the backdrop cancels, as it does on every
        // other window of the shell.
        if (event.target === event.currentTarget && !busy) onClose();
      }}
    >
      <div
        className="modal graduation-start"
        role="dialog"
        aria-modal="true"
        aria-labelledby="graduation-start-title"
        aria-describedby="graduation-start-what"
        data-testid="graduation-start"
      >
        <div className="modal__head">
          <Icon.Graduate size={14} />
          <h2 className="modal__title" id="graduation-start-title">
            Graduate “{draftName}”
          </h2>
          <button
            type="button"
            className="btn btn--ghost btn--icon btn--sm"
            ref={closeRef}
            aria-label="Close"
            disabled={busy}
            onClick={onClose}
          >
            <Icon.X size={12} />
          </button>
        </div>

        <div className="modal__body">
          <p className="t-ui-sm graduation-start__what" id="graduation-start-what">
            An agent does the work this prompt asks for, in a work stream, and
            commits it there when a review lets it through.
          </p>

          {streams === null && (
            <p className="t-ui-sm">Reading the project's work streams…</p>
          )}

          {/* GSD-FR-CXVA: the read failed, so what the project holds is
              unknown. Saying it has no stream would be a statement this window
              cannot make. */}
          {unreadable && (
            <p
              className="graduation-start__notice"
              data-testid="graduation-start-unreadable"
            >
              The project's work streams could not be read, so there is no
              existing stream to choose. What went wrong is below.
            </p>
          )}

          {/* GSD-FR-LDGM: a project with no stream has none to choose, and the
              dialog says where else the run can work. */}
          {streams !== null && streams.length === 0 && !unreadable && (
            <p
              className="graduation-start__notice"
              data-testid="graduation-start-no-streams"
            >
              This project has no work stream yet. Create one here, or work
              directly in the active worktree.
            </p>
          )}

          {streams !== null && (
            <DestinationChoice
              value={destination}
              disabled={busy}
              noStream={streams.length === 0}
              onChange={selectDestination}
            />
          )}

          {streams !== null && destination === "stream" && streams.length > 0 && (
            <>
              <div className="picker-field">
                <label
                  className="picker-field__label"
                  htmlFor="graduation-stream"
                >
                  Work stream
                </label>
                <select
                  id="graduation-stream"
                  className="select"
                  ref={pickerRef}
                  value={streamId}
                  disabled={busy}
                  onChange={(event) => selectStream(event.target.value)}
                >
                  {streams.map(({ stream, queuedRunCount }) => (
                    <option key={stream.id} value={stream.id}>
                      {stream.name}
                      {queuedRunCount > 0 ? ` · ${queuedRunCount} queued` : ""}
                    </option>
                  ))}
                </select>
                {/* GSD-FR-HRJE: a busy stream is offered like every other one,
                    and what choosing it means for when the run starts stands
                    with the control it is about. */}
                {chosen?.stream.busyRunId && (
                  <p
                    className="t-ui-xs graduation-start__note"
                    data-testid="graduation-start-busy"
                  >
                    A run is working in “{chosen.stream.name}” now. This one
                    waits behind it.
                  </p>
                )}
                {/* GSD-FR-LHQY / GSD-FR-KDBU: the limit is a reason of its own,
                    stated beside the busy note when both apply. */}
                {limitFull && fullLimit !== null && chosen && (
                  <SlotWait
                    limit={fullLimit}
                    target={{
                      kind: "stream",
                      name: chosen.stream.name,
                      occupied,
                    }}
                  />
                )}
              </div>

              {streamWaits && (
                <StandingWorkChoice
                  name="graduation-start"
                  value={standingWork}
                  disabled={busy}
                  onChange={setStandingWork}
                  message={message}
                  onMessage={editMessage}
                  messageDefault={draftName}
                />
              )}
            </>
          )}

          {/* GSD-FR-NOID: a capacity that is not read is stated, for the two
              destinations that a project slot can make wait. */}
          {streams !== null &&
            destination !== "direct" &&
            capacityRead.status === "unread" && (
              <CapacityUnread reason={capacityRead.reason} />
            )}

          {/* GSD-FR-QGTC: a stream made here is free, so it asks the
              standing-work choice only where the limit is full. */}
          {streams !== null && destination === "new" && (
            <NewStreamFields
              name={newName}
              onName={(value) => {
                setNewName(value);
                setNameError(null);
              }}
              branch={newBranch}
              onBranch={setNewBranch}
              branches={branches}
              nameError={nameError}
              disabled={busy}
              nameRef={nameRef}
            />
          )}

          {streams !== null && destination === "new" && newWaits && fullLimit !== null && (
            <>
              <SlotWait limit={fullLimit} target={{ kind: "new" }} />
              <StandingWorkChoice
                name="graduation-start"
                value={standingWork}
                disabled={busy}
                onChange={setStandingWork}
                message={message}
                onMessage={editMessage}
                messageDefault={draftName}
              />
            </>
          )}

          {/* GSD-FR-FQQP: the worktree the run will write in. It asks no
              standing-work choice and no commit message. */}
          {streams !== null && destination === "direct" && (
            <DirectChoice preflight={preflight} unreadable={preflightError} />
          )}

          {error && (
            <p
              className="graduation-start__notice graduation-start__notice--refused"
              role="alert"
              ref={errorRef}
              data-testid="graduation-start-error"
            >
              {error}
            </p>
          )}
        </div>

        <div className="modal__actions">
          <button
            type="button"
            className="btn btn--ghost"
            disabled={busy}
            onClick={onClose}
          >
            Cancel
          </button>
          <button
            type="button"
            className="btn btn--primary"
            onClick={start}
            disabled={!canStart}
            data-testid="graduation-start-confirm"
          >
            {busy
              ? "Starting…"
              : destination === "new"
                ? "Create stream and graduate"
                : destination === "direct"
                  ? "Graduate here"
                  : "Graduate"}
          </button>
        </div>
      </div>
    </div>
  );
}

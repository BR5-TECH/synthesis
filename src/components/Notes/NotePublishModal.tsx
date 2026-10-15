/**
 * The note publish modal (`specifications/ui/NIP-note-issue-publication.md`).
 *
 * It moves a note into a GitHub issue: the author writes a title, edits the
 * description that opens on the note body, and chooses the parent, the Type,
 * the milestone, and the labels. **Publish** creates the issue and deletes the
 * note in one backend call (`"publish note to github"`, GHP-FR-JGNI).
 *
 * It reuses the dialog, the list states, and the parent combobox of the draft
 * publication chooser (NAW-FR-PSMO, NAW-FR-WMEP, NAW-FR-YLKD), and the lists
 * come from the backend's metadata cache (GHP-FR-JTYL) and one refresh
 * (GHP-FR-SIMK). It keeps no state after it closes (NIP-FR-DQQN).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import * as api from "../../api";
import { onPublicationMetadataChanged } from "../../events";
import { logDebug, logError, logInfo, logWarn } from "../../logging";
import { githubPollingErrorMessage } from "../../state/githubPolling";
import {
  MAX_BODY_CHARS,
  MAX_TITLE_CHARS,
  charCount,
  linkFailedText,
  notePublicationErrorText,
  showNoteNotRemovedToast,
  showNotePublishFailedToast,
  showNotePublishedToast,
} from "../../state/notePublication";
import type {
  MetadataList,
  NoteListItem,
  NotePublicationChoiceInput,
  PublicationLabel,
  PublicationMetadata,
  PublicationMilestone,
} from "../../types";
import { Modal } from "../DraftsPanelParts";
import { FilterableSelect } from "../FilterableSelect";
import {
  Notice,
  matchesParent,
  parentAnnouncement,
  parentOption,
} from "../PublicationChooser";

const ROOT = "root";
const SUB = "sub";
const NONE = "";

/** What the modal knows about the lists (NIP-FR-HYUJ, NIP-FR-GPEO). */
interface MetadataState {
  metadata: PublicationMetadata | null;
  /** The refresh of NIP-FR-HYUJ has not answered yet. */
  refreshing: boolean;
  /** The refresh failed and the cache held no list (NIP-FR-GPEO). */
  error: string | null;
}

type ListView<T> =
  | { kind: "loading" }
  | { kind: "unavailable"; error: string }
  | { kind: "empty" }
  | { kind: "loaded"; items: T[] };

/** NIP-FR-GPEO / NAW-FR-WMEP: the four states a list renders. */
function viewOf<T>(
  state: MetadataState,
  pick: (metadata: PublicationMetadata) => MetadataList<T>,
): ListView<T> {
  if (!state.metadata) {
    if (state.refreshing) return { kind: "loading" };
    return { kind: "unavailable", error: state.error ?? "GitHub could not be read." };
  }
  const list = pick(state.metadata);
  if (list.state === "failed") {
    return { kind: "unavailable", error: list.error ?? "GitHub could not be read." };
  }
  return list.items.length === 0 ? { kind: "empty" } : { kind: "loaded", items: list.items };
}

/** The ids a list holds, or `null` while it is loading or unavailable. */
function idsOf<T>(view: ListView<T>, id: (item: T) => string): string[] | null {
  if (view.kind === "empty") return [];
  return view.kind === "loaded" ? view.items.map(id) : null;
}

/**
 * NIP-FR-HYUJ / NIP-FR-PWYE: the cached lists of `remoteName`, one refresh,
 * and the change event while the modal is open.
 */
function useNoteMetadata(remoteName: string): MetadataState {
  const [state, setState] = useState<MetadataState>({
    metadata: null,
    refreshing: true,
    error: null,
  });

  useEffect(() => {
    let live = true;
    void (async () => {
      try {
        const cached = await api.getPublicationMetadataCache(remoteName);
        if (live && cached) setState((s) => ({ ...s, metadata: s.metadata ?? cached }));
      } catch (e) {
        logWarn(["frontend", "remote"], "publication metadata cache could not be read", {
          error: String(e),
        });
      }
      try {
        const answer = await api.refreshPublicationMetadata();
        logDebug(["frontend", "remote"], "note publication metadata refresh ended", {
          outcome: answer.outcome,
          errorCode: answer.errorCode,
        });
        if (!live) return;
        const fresh =
          answer.remoteName === remoteName && answer.metadata ? answer.metadata : null;
        setState((s) => ({
          metadata: fresh ?? s.metadata,
          refreshing: false,
          error: answer.error ?? (fresh ? null : "GitHub could not be read."),
        }));
      } catch (e) {
        logWarn(["frontend", "remote"], "note publication metadata refresh failed", {
          error: String(e),
        });
        if (live) {
          setState((s) => ({ ...s, refreshing: false, error: githubPollingErrorMessage(e) }));
        }
      }
    })();
    const subscription = onPublicationMetadataChanged((payload) => {
      if (!live || payload.remoteName !== remoteName || !payload.metadata) return;
      const metadata = payload.metadata;
      setState((s) => ({ ...s, metadata }));
    });
    return () => {
      live = false;
      void subscription.then((unlisten) => unlisten());
    };
  }, [remoteName]);

  return state;
}

/** NIP-FR-VLFR: a control, or the notice that stands in its place. */
function listNotice<T>(view: ListView<T>, what: string, without: string): string | null {
  switch (view.kind) {
    case "loading":
      return `Loading ${what}…`;
    case "unavailable":
      return `${what[0].toUpperCase()}${what.slice(1)} are unavailable: ${view.error} The issue publishes without ${without}.`;
    case "empty":
      return `There are no ${what}. The issue publishes without ${without}.`;
    default:
      return null;
  }
}

function SelectField({
  id,
  label,
  value,
  onChange,
  noneLabel,
  options,
}: {
  id: string;
  label: string;
  value: string;
  onChange: (next: string) => void;
  noneLabel: string;
  options: { value: string; label: string }[];
}) {
  return (
    <div className="draft-publication-chooser__field">
      <label className="t-ui-sm" htmlFor={id}>
        {label}
      </label>
      <select
        id={id}
        className="input input--sm"
        value={value}
        onChange={(e) => onChange(e.target.value)}
      >
        <option value={NONE}>{noneLabel}</option>
        {options.map((o) => (
          <option key={o.value} value={o.value}>
            {o.label}
          </option>
        ))}
      </select>
    </div>
  );
}

/** NIP-FR-DKXP: the multi-select of the repository labels. */
function LabelsField({
  labels,
  selected,
  onToggle,
}: {
  labels: PublicationLabel[];
  selected: string[];
  onToggle: (name: string, checked: boolean) => void;
}) {
  return (
    <div className="draft-publication-chooser__field" style={{ alignItems: "flex-start" }}>
      <span className="t-ui-sm" id="note-publication-labels-label">
        Labels
      </span>
      <div
        role="group"
        aria-labelledby="note-publication-labels-label"
        className="note-publication__labels"
      >
        {labels.map((label) => (
          <label key={label.name} className="t-ui-sm note-publication__label" title={label.description ?? undefined}>
            <input
              type="checkbox"
              checked={selected.includes(label.name)}
              onChange={(e) => onToggle(label.name, e.target.checked)}
            />
            <span
              className="note-publication__swatch"
              style={{ background: `#${label.color || "888888"}` }}
              aria-hidden
            />
            <span>{label.name}</span>
          </label>
        ))}
      </div>
    </div>
  );
}

export interface NotePublishTarget {
  remoteName: string;
  /** `owner/name` of the publication repository (NIP-FR-TVNN). */
  repository: string;
}

export function NotePublishModal({
  item,
  target,
  returnFocus,
  onClose,
  onPublished,
  onNoteKept,
}: {
  item: NoteListItem;
  target: NotePublishTarget;
  returnFocus: HTMLElement | null;
  onClose: () => void;
  /** NIP-FR-MTGO: the issue exists and the note is deleted. */
  onPublished: (noteId: string) => void;
  /** NIP-FR-GQVQ: the issue exists and the note stays. */
  onNoteKept: () => void;
}) {
  const state = useNoteMetadata(target.remoteName);
  // NIP-FR-DZWQ: the root option is selected when the modal opens.
  const [option, setOption] = useState(ROOT);
  const [parentNumber, setParentNumber] = useState(NONE);
  // NIP-FR-UMKW / NIP-FR-PWAB: `null` until the Type list first renders loaded.
  const [type, setType] = useState<string | null>(null);
  const [milestone, setMilestone] = useState(NONE);
  const [labels, setLabels] = useState<string[]>([]);
  const [title, setTitle] = useState("");
  const [body, setBody] = useState(item.note.body);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const [error, setError] = useState<string | null>(null);

  const metadata = state.metadata;
  const parents = viewOf(state, (m) => m.parents);
  const types = viewOf(state, (m) => m.issueTypes);
  const milestones = viewOf(state, (m) => m.milestones);
  const labelList = viewOf(state, (m) => m.labels);

  // NIP-FR-UMKW: the opening value is the resolved Note issue Type, or No Type.
  const typesLoaded = types.kind === "loaded";
  const opening = metadata?.noteIssueType?.resolved ?? NONE;
  useEffect(() => {
    if (typesLoaded && type === null) setType(opening);
  }, [typesLoaded, type, opening]);
  const typeValue = type ?? NONE;
  const storedTypeMissing =
    typesLoaded && metadata?.noteIssueType != null && metadata.noteIssueType.resolved === null;

  // NIP-FR-ONHC / NIP-FR-PWAB: a choice the new list no longer holds resets.
  const parentIds = idsOf(parents, (p) => String(p.number));
  const typeIds = idsOf(types, (t) => t.name);
  const milestoneIds = idsOf(milestones, (m) => String(m.number));
  const labelIds = idsOf(labelList, (l) => l.name);
  useEffect(() => {
    if (parentNumber !== NONE && parentIds && !parentIds.includes(parentNumber)) {
      setParentNumber(NONE);
    }
  }, [parentNumber, parentIds]);
  useEffect(() => {
    if (type && type !== NONE && typeIds && !typeIds.includes(type)) setType(NONE);
  }, [type, typeIds]);
  useEffect(() => {
    if (milestone !== NONE && milestoneIds && !milestoneIds.includes(milestone)) {
      setMilestone(NONE);
    }
  }, [milestone, milestoneIds]);
  useEffect(() => {
    if (labelIds && labels.some((l) => !labelIds.includes(l))) {
      setLabels((current) => current.filter((l) => labelIds.includes(l)));
    }
  }, [labels, labelIds]);

  const parentItems = parents.kind === "loaded" ? parents.items : null;
  const parentOptions = useMemo(() => (parentItems ?? []).map(parentOption), [parentItems]);
  const parentMatcher = useMemo(() => matchesParent(parentItems ?? []), [parentItems]);
  // NIP-FR-YEKZ: the sub-issue option is offered only with a parent to choose.
  const subAvailable = parents.kind === "loaded";
  const selected = subAvailable ? option : ROOT;
  const parent =
    selected === SUB && parentItems
      ? (parentItems.find((p) => String(p.number) === parentNumber) ?? null)
      : null;

  const trimmed = title.trim();
  const bodyTooLong = charCount(body) > MAX_BODY_CHARS;
  // NIP-FR-IJAL: every condition that disables Publish.
  const publishDisabled =
    busy ||
    trimmed === "" ||
    bodyTooLong ||
    (selected === SUB && parent === null) ||
    (!metadata && state.refreshing);

  /** NIP-FR-SJGY: the choice as the backend receives it. */
  const choice = (): NotePublicationChoiceInput => ({
    parentIssueNumber: parent ? parent.number : null,
    issueType: typesLoaded && typeValue !== NONE ? typeValue : null,
    milestoneNumber:
      milestones.kind === "loaded" && milestone !== NONE ? Number(milestone) : null,
    labels: labelList.kind === "loaded" ? labels : [],
  });

  const publish = async () => {
    if (busyRef.current || publishDisabled) return;
    busyRef.current = true;
    setBusy(true);
    setError(null);
    const noteId = item.note.id;
    const sent = choice();
    logInfo(["frontend", "remote"], "note publication requested", {
      noteId,
      parent: sent.parentIssueNumber,
      typed: sent.issueType !== null,
      milestoned: sent.milestoneNumber !== null,
      labels: sent.labels.length,
    });
    try {
      const outcome = await api.publishNoteToGithub({
        noteId,
        title: trimmed,
        body,
        choice: sent,
      });
      if (outcome.kind === "published") {
        showNotePublishedToast(outcome.issueNumber, outcome.issueUrl, target.repository);
        onPublished(noteId);
        return;
      }
      if (outcome.kind === "note_not_removed") {
        logWarn(["frontend", "remote"], "published note was not removed", {
          noteId,
          issueNumber: outcome.issueNumber,
          errorCode: outcome.errorCode,
        });
        showNoteNotRemovedToast(outcome.issueNumber, outcome.issueUrl, outcome.errorCode);
        onNoteKept();
        return;
      }
      // NIP-FR-SNLZ: the issue exists without its parent, and the note stays.
      const text = linkFailedText(outcome.issueNumber);
      logWarn(["frontend", "remote"], "note issue was not linked to its parent", {
        noteId,
        issueNumber: outcome.issueNumber,
      });
      setError(text);
      showNotePublishFailedToast(text);
    } catch (e) {
      // NIP-FR-ISQT: the modal stays open with every value.
      const text = notePublicationErrorText(e);
      logError(["frontend", "remote"], "note publication failed", { noteId, error: String(e) });
      setError(text);
      showNotePublishFailedToast(text);
    } finally {
      busyRef.current = false;
      setBusy(false);
    }
  };

  // NIP-FR-WUID: no dismissal closes the modal while a call is in flight.
  const dismiss = useCallback(() => {
    if (!busyRef.current) onClose();
  }, [onClose]);

  const typeNotice = listNotice(types, "issue Types", "a Type");
  const milestoneNotice = listNotice(milestones, "open milestones", "a milestone");
  const labelNotice = listNotice(labelList, "labels", "labels");

  return (
    <Modal label="Publish to GitHub" onClose={dismiss} returnFocus={returnFocus} closeOnScrim={false}>
      {/* NIP-FR-YQFL: focus moves to the first enabled option, so the head
          with its close control comes last in the DOM and first on screen. */}
      <div className="note-publication">
        <div className="modal__body note-publication__body-scroll">
          <p className="t-ui-sm">
            Repository:{" "}
            <span className="draft-publication-picker__name" data-testid="note-publication-repository">
              {target.repository}
            </span>
          </p>
          <div role="radiogroup" aria-label="Parent" className="draft-publication-chooser__options">
            <label className="t-ui-sm draft-publication-chooser__option">
              <input
                type="radio"
                name="note-publication-parent"
                value={SUB}
                checked={selected === SUB}
                disabled={!subAvailable}
                onChange={() => setOption(SUB)}
              />
              <span className="draft-publication-picker__name">Publish as a sub-issue of</span>
            </label>
            <div className="draft-publication-chooser__detail">
              {parents.kind === "loading" && <Notice>Loading parent issues…</Notice>}
              {parents.kind === "unavailable" && (
                <Notice>
                  {`Parent issues are unavailable: ${parents.error} Root publication is still available.`}
                </Notice>
              )}
              {parents.kind === "empty" && (
                <Notice>
                  {`No open issue of ${
                    metadata?.settings.parentIssueTypes.join(", ") ?? "the configured Types"
                  } was found. Root publication is still available.`}
                </Notice>
              )}
              {parents.kind === "loaded" && (
                <FilterableSelect
                  options={parentOptions}
                  value={parentNumber}
                  alwaysNotify
                  onChange={(id) => {
                    // NIP-FR-YEKZ: choosing a parent selects the sub-issue option.
                    setParentNumber(id);
                    setOption(SUB);
                  }}
                  label="Parent issue"
                  placeholder="Choose a parent issue"
                  emptyText="No matching issue"
                  matches={parentMatcher}
                  announce={parentAnnouncement}
                  fitWindow
                  testId="note-publication-parent"
                />
              )}
            </div>
            <label className="t-ui-sm draft-publication-chooser__option">
              <input
                type="radio"
                name="note-publication-parent"
                value={ROOT}
                checked={selected === ROOT}
                onChange={() => setOption(ROOT)}
              />
              <span className="draft-publication-picker__name">Publish as a root issue</span>
            </label>
          </div>

          <div className="note-publication__fields">
            {typeNotice ? (
              <Notice>{typeNotice}</Notice>
            ) : (
              <SelectField
                id="note-publication-type"
                label="Type"
                value={typeValue}
                onChange={setType}
                noneLabel="No Type"
                options={(types.kind === "loaded" ? types.items : []).map((t) => ({
                  value: t.name,
                  label: t.name,
                }))}
              />
            )}
            {storedTypeMissing && (
              <Notice>
                {`The Note issue Type “${metadata?.noteIssueType?.name}” is not available in this repository, so the Type starts on No Type.`}
              </Notice>
            )}
            {milestoneNotice ? (
              <Notice>{milestoneNotice}</Notice>
            ) : (
              <SelectField
                id="note-publication-milestone"
                label="Milestone"
                value={milestone}
                onChange={setMilestone}
                noneLabel="No milestone"
                options={(milestones.kind === "loaded" ? milestones.items : []).map(
                  (m: PublicationMilestone) => ({ value: String(m.number), label: m.title }),
                )}
              />
            )}
            {labelNotice ? (
              <Notice>{labelNotice}</Notice>
            ) : (
              <LabelsField
                labels={labelList.kind === "loaded" ? labelList.items : []}
                selected={labels}
                onToggle={(name, checked) =>
                  setLabels((current) =>
                    checked ? [...current, name] : current.filter((l) => l !== name),
                  )
                }
              />
            )}
          </div>

          <label className="t-ui-sm note-publication__text" htmlFor="note-publication-title">
            Title
          </label>
          <input
            id="note-publication-title"
            className="input input--sm"
            value={title}
            onChange={(e) => setTitle(Array.from(e.target.value).slice(0, MAX_TITLE_CHARS).join(""))}
          />
          <label className="t-ui-sm note-publication__text" htmlFor="note-publication-body">
            Description
          </label>
          <textarea
            id="note-publication-body"
            className="input note-publication__body"
            value={body}
            rows={6}
            aria-invalid={bodyTooLong || undefined}
            onChange={(e) => setBody(e.target.value)}
          />
          {bodyTooLong && (
            <p className="picker-error t-ui-xs" role="alert">
              The description holds more than 65,536 characters. Shorten it to publish.
            </p>
          )}
          {item.discussionThreadId && (
            <p className="t-ui-sm note-publication__warning" role="status">
              This note has a discussion. The discussion is deleted with the note.
            </p>
          )}
          {item.note.reminder && (
            <p className="t-ui-sm t-muted" role="status">
              The reminder of this note is removed too.
            </p>
          )}
          {error && (
            <p className="picker-error t-ui-sm" role="alert" data-testid="note-publication-error">
              {error}
            </p>
          )}
        </div>
        <div className="modal__actions">
          <button className="btn btn--ghost" onClick={dismiss} disabled={busy}>
            Cancel
          </button>
          <button
            className="btn btn--primary"
            disabled={publishDisabled}
            onClick={() => void publish()}
          >
            Publish
          </button>
        </div>
        <div className="modal__head note-publication__head">
          <div className="modal__title">Publish to GitHub</div>
          <button
            type="button"
            className="btn btn--ghost btn--sm"
            aria-label="Close"
            disabled={busy}
            onClick={dismiss}
          >
            ✕
          </button>
        </div>
      </div>
    </Modal>
  );
}

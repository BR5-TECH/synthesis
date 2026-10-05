/**
 * The Notes vertical panel (`specifications/ui/NTS-notes.md`).
 *
 * A three-position scope selector (NTS-FR-09) decides what is shown: the notes
 * on the entity the active tab is bound to, the project-wide notes, or every
 * note in the project grouped by what it is attached to. Notes are read,
 * written, retimed, reassigned and deleted entirely here — each row edits in
 * place (NTS-FR-16) and carries an overflow menu (NTS-FR-18).
 *
 * The record, its timestamps and the unresolved marking belong to
 * `../../specifications/core/NTC-notes-storage.md`; the selector position and
 * the filter text are persisted by `useNotesPanelState` (PSS-FR-19). Everything
 * this file decides is presentation: grouping, ordering, filtering, and the four
 * floating overlays, of which at most one is ever open (NTS-FR-24).
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import * as api from "../../api";
import type { NotesPanel } from "../../hooks/useNotesPanelState";
import type {
  NoteListItem,
  NoteScope,
  NotesEntity,
  NotesScopePosition,
  OpenableArtifact } from "../../types";
import { CommentMarkdown } from "../CommentMarkdown";
import { Icon } from "../icons";
import { SelectorRow, type SelectorPosition } from "../SelectorRow";
import { UnresolvedMarker } from "../UnresolvedMarker";
import {
  groupKey,
  groupsForFilter,
  lastKnownPath,
  matchesFilter,
  sortMostRecentFirst } from "../notesGrouping";
import { formatRelative } from "../ProjectPicker";
import { rememberNoteLabel } from "../../state/ownerAvailability";
import type { DiscussionReveal } from "../../state/revealDiscussion";
import { logInfo } from "../../logging";
import { onDiscussionChanged } from "../../events";
import {
  TIMESTAMP_REFRESH_MS,
  absoluteTitle,
  artifactTargets,
  autoGrow,
  errorMessage,
  formatAbsolute,
  newNoteScope,
  type MoveTarget,
  type Overlay } from "./helpers";
import { NoteOverlay, excerpt, noteLabel } from "./overlays";
import { fromLocalInput, toLocalInput } from "./reminderTime";

export { TIMESTAMP_REFRESH_MS, artifactTargets, newNoteScope } from "./helpers";
export { fromLocalInput, toLocalInput } from "./reminderTime";

interface NotesProps {
  /** NTS-FR-09 / NTS-FR-13: the sticky selector position and filter text. */
  panel: NotesPanel;
  /**
   * NTS-FR-02: the entity the active tab is bound to, or the artifact a Library
   * **Notes** action named (LCM-FR-05). `null` when nothing entity-scoped is
   * active, which is what makes the entity position unavailable (NTS-FR-03).
   */
  entity: NotesEntity | null;
  /** NTS-FR-11: follow an entity group header to the artifact it names. */
  onOpenArtifact: (artifact: OpenableArtifact) => void;
  /**
   * NTS-FR-26: the one route that reveals a discussion. **Discuss** calls it, and
   * the panel renders no part of the conversation itself (NTS-FR-29).
   */
  onReveal?: (reveal: DiscussionReveal) => void;
  /** NTS-FR-30: called once the backend reports a note and its discussion deleted. */
  onNoteDeleted?: (noteId: string) => void;
}

export function Notes({
  panel,
  entity,
  onOpenArtifact,
  onReveal,
  onNoteDeleted,
}: NotesProps) {
  const [items, setItems] = useState<NoteListItem[] | null>(null);
  const [loadError, setLoadError] = useState("");
  /** NTS-FR-16: the one row in inline edit, and its unsaved body. */
  const [editingId, setEditingId] = useState<string | null>(null);
  const [draft, setDraft] = useState("");
  /**
   * The note `addNote` wrote but that has never been given a body.
   *
   * Creating a note now writes it before the user has typed anything, so
   * something has to take an abandoned one away again — and an editor closes
   * six different ways (Cancel, Escape, a blank Save, the note leaving the
   * rendered list, a scope change, the panel unmounting). Holding the id here,
   * rather than deciding from the row's stored body at each of those sites,
   * is what lets a single reap cover all of them; the ref mirrors it so the
   * unmount path can read it without a stale closure.
   */
  const [blankId, setBlankId] = useState<string | null>(null);
  const blankRef = useRef<string | null>(null);
  const setBlank = (id: string | null) => {
    blankRef.current = id;
    setBlankId(id);
  };
  /**
   * The row whose editor has an operation in flight — a save, or the delete an
   * emptied body commits.
   *
   * While one is running the editor's own affordances are unavailable and the
   * leave-the-list reap below stands down: the operation owns the row's outcome,
   * and a second path acting on the same row would either delete it twice or
   * close the editor from under a continuation that then resets it. The ref is
   * what makes the guard hold against two clicks inside one tick.
   */
  const [busyId, setBusyId] = useState<string | null>(null);
  const busyRef = useRef<string | null>(null);
  const setBusy = (id: string | null) => {
    busyRef.current = id;
    setBusyId(id);
  };
  const [overlay, setOverlay] = useState<Overlay | null>(null);
  /** An error from a row's own operation, attached to that row. */
  const [rowError, setRowError] = useState<{ id: string; message: string } | null>(
    null,
  );
  /** NTS-FR-04: the error a failed create renders, below the affordance. */
  const [createError, setCreateError] = useState("");
  /** A create is in flight, so a second click cannot write the note twice. */
  const [creating, setCreating] = useState(false);
  const [moveTargets, setMoveTargets] = useState<MoveTarget[] | null>(null);
  const [moveFilter, setMoveFilter] = useState("");
  const [reminderDraft, setReminderDraft] = useState("");

  /**
   * NTS-FR-09 / NTS-FR-03: what is actually rendered. The entity position has
   * nothing to bind to while no entity-scoped tab is active, so the panel falls
   * back to project-wide — without rewriting the persisted position, so opening
   * an artifact tab returns the panel to that entity's notes (NTS-FR-09).
   */
  const position: NotesScopePosition =
    panel.position === "entity" && !entity ? "project" : panel.position;

  /**
   * What the load depends on. In the project-wide and all-notes positions the
   * active tab's entity is not part of it, so a tab change leaves the rendered
   * list alone (NTS-FR-08) instead of re-fetching the same notes.
   */
  const loadKey = position === "entity" ? `entity:${entity?.id ?? ""}` : position;

  // Ordinal of the newest load, so a slow one settling after a newer one
  // cannot paint the previous scope's notes over the current ones.
  const loadSeq = useRef(0);

  /**
   * NTS-FR-28: keep each row's `discussionThreadId` current, so **Discuss**
   * always reaches the conversation the note actually has.
   *
   * The association is created by the opening composer posting, which happens in
   * a conversation tab rather than in this panel — so without this the list the
   * panel loaded on mount still says the note has no discussion. Choosing
   * Discuss again would then open a *fresh* opening composer, and the message
   * typed into it would be silently dropped: `get_or_create_note_discussion` is
   * idempotent, so it returns the discussion that already exists and appends
   * nothing (CMS-FR-62). The author would see their reply vanish into what looks
   * like a new conversation.
   *
   * Followed as an event rather than re-read, on the terms every other
   * conversation surface follows it (per `CMP-comments-panel.md` CMP-FR-18): the
   * payload carries the folded thread, so the row is updated from the write that
   * changed it and the panel issues no call of its own.
   */
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onDiscussionChanged((thread) => {
      if (thread.target.kind !== "note") return;
      const noteId = thread.target.noteId;
      setItems((prev) => {
        if (prev === null) return prev;
        const index = prev.findIndex((item) => item.note.id === noteId);
        if (index === -1) return prev;
        if (prev[index].discussionThreadId === thread.id) return prev;
        const next = [...prev];
        next[index] = { ...prev[index], discussionThreadId: thread.id };
        return next;
      });
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const load = useCallback(async () => {
    // Nothing loads until the persisted position has landed (NTS-FR-09): a load
    // against the default position would fetch notes the panel is about to stop
    // showing, and flash them on the way.
    if (!panel.restored) return;
    const ticket = ++loadSeq.current;
    try {
      const loaded =
        position === "entity" && entity
          ? await api.listNotesForEntity(entity.id)
          : position === "all"
            ? await api.listAllNotes()
            : await api.listProjectNotes();
      if (ticket !== loadSeq.current) return;
      setItems(loaded);
      setLoadError("");
    } catch (e) {
      if (ticket !== loadSeq.current) return;
      setItems([]);
      setLoadError(errorMessage(e));
    }
    // `position` and `entity?.id` are what `loadKey` is made of; the effect
    // below keys on `loadKey` so a tab change outside the entity position does
    // not re-run this.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [loadKey, panel.restored]);

  useEffect(() => {
    void load();
  }, [load]);

  // NTS-FR-14 non-functional: relative timestamps go stale on their own, so the
  // rendered rows are recomputed as time passes without the user reopening the
  // panel.
  const [, setTick] = useState(0);
  useEffect(() => {
    const timer = setInterval(() => setTick((n) => n + 1), TIMESTAMP_REFRESH_MS);
    return () => clearInterval(timer);
  }, []);

  // NTS-FR-24: every overlay dismisses on Escape or an outside click, without
  // invoking anything. The inline editor is deliberately not part of this — it
  // survives an outside click and closes only on Save or Cancel (NTS-FR-17).
  useEffect(() => {
    if (!overlay) return;
    const dismiss = () => setOverlay(null);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOverlay(null);
    };
    window.addEventListener("click", dismiss);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("click", dismiss);
      window.removeEventListener("keydown", onKey);
    };
  }, [overlay]);

  const all = items ?? [];
  const noteById = (id: string) => all.find((i) => i.note.id === id);
  const grouped = position === "all";
  // A note that has never been given a body matches no filter text, so the
  // blank note being written right now is exempt — otherwise `+ Note` under an
  // active filter would create a row and hide it in the same breath.
  const groups = grouped ? groupsForFilter(all, panel.text, blankId) : [];
  const flat = grouped
    ? []
    : sortMostRecentFirst(
        all.filter(
          (i) => i.note.id === blankId || matchesFilter(i, panel.text, false),
        ),
      );
  const visible = grouped ? groups.flatMap((g) => g.items) : flat;

  /**
   * NTS-FR-17: an editor whose note leaves the rendered list — the selector
   * moved, the filter excluded it, or its scope changed — closes, discarding
   * its unsaved content.
   *
   * This is also the catch-all reap for a never-written note: the row can go
   * out from under the editor for reasons no button handler sees (the active
   * tab rebinding, a failed reload), and a blank note left behind that way is
   * invisible to the filter and nearly unreachable in the UI.
   */
  useEffect(() => {
    if (!editingId || visible.some((i) => i.note.id === editingId)) return;
    // An operation already owns this row (see `busyId`). Reaping here would race
    // its delete; if it fails, the editor stays open and this effect runs again
    // with the row still gone.
    if (busyId) return;
    const closing = editingId;
    setEditingId(null);
    setDraft("");
    if (blankRef.current === closing) {
      setBlank(null);
      void api
        .deleteNote(closing)
        .then(() => load())
        .catch(() => {});
    }
  });

  // The last of the six: the panel itself going away (a surface switch, a
  // project or worktree change). Nothing renders after this, so the blank note
  // is removed rather than reported on.
  useEffect(
    () => () => {
      const abandoned = blankRef.current;
      if (abandoned) void api.deleteNote(abandoned).catch(() => {});
    },
    [],
  );

  // --- Operations --------------------------------------------------------

  const run = async (noteId: string, op: () => Promise<unknown>) => {
    try {
      await op();
      setRowError(null);
      await load();
    } catch (e) {
      setRowError({ id: noteId, message: errorMessage(e) });
    }
  };

  /**
   * NTS-FR-26 / NTS-FR-27 / NTS-FR-28: reveal the note's one conversation.
   *
   * The panel is list-only (NTS-FR-29). It calls the one route with the note's
   * identity, and the route opens or focuses the note's conversation tab. A note
   * that already has a discussion is opened with every message in it, and a note
   * that has none opens the opening composer, which creates nothing until the
   * author posts. Which of the two it is comes from the `discussionThreadId` the
   * row already carries, so choosing Discuss costs no call to find out.
   */
  const discuss = (item: NoteListItem) => {
    const noteId = item.note.id;
    logInfo(["frontend"], "note discussion requested", {
      noteId,
      existing: item.discussionThreadId !== undefined });
    const ownerLabel = noteLabel(item);
    rememberNoteLabel(noteId, ownerLabel);
    onReveal?.({
      discussionId: item.discussionThreadId,
      target: { kind: "note", noteId },
      fragmentTarget: null,
      ownerLabel,
      subject: excerpt(item.note.body) });
  };

  /**
   * NTS-FR-30: deleting a note deletes its discussion with it.
   *
   * The conversation tab is closed on the backend's reported success **alone**
   * (CVP-FR-61): the session is memory rather than the source of transactional
   * truth, so a typed failure leaves the conversation open and readable, keeps
   * the row, and reports the error inline — the author is never told a note is
   * gone while it, or its conversation, is still there.
   */
  const removeNote = async (id: string) => {
    try {
      await api.deleteNote(id);
      onNoteDeleted?.(id);
      setRowError(null);
      await load();
    } catch (e) {
      logInfo(["frontend"], "note deletion refused", { noteId: id });
      setRowError({ id, message: errorMessage(e) });
    }
  };

  /**
   * Delete the note the editor is over and close the editor onto nothing — the
   * way a never-written note is abandoned (see `cancelEdit`).
   *
   * When the removal fails the editor stays open carrying the error, for the
   * same reason a failed Save does: closing over a row the user cannot act on
   * is worse than leaving them somewhere they can retry.
   */
  const removeEdited = async (id: string) => {
    setBusy(id);
    try {
      await api.deleteNote(id);
      onNoteDeleted?.(id);
      // It is gone, so nothing else may reap it — least of all the unmount path,
      // which would otherwise delete an id that no longer exists.
      if (blankRef.current === id) setBlank(null);
      setEditingId(null);
      setDraft("");
      setRowError(null);
      await load();
    } catch (e) {
      setRowError({ id, message: errorMessage(e) });
    } finally {
      setBusy(null);
    }
  };

  /**
   * NTS-FR-16: Cancel closes the editor leaving the stored note untouched.
   *
   * With one exception: a note that has never been given a body. Creating a
   * note now writes it immediately (see `addNote`), so abandoning one has to
   * take the empty row away with it — otherwise every abandoned `+ Note` leaves
   * a blank row in the list that nothing can find and only the overflow menu
   * can remove.
   */
  const cancelEdit = () => {
    const id = editingId;
    // Not while the row's own save or delete is in flight: closing the editor
    // here would let the user reopen it, and the continuation would then reset
    // the editor they had just reopened.
    if (!id || busyRef.current) return;
    if (blankRef.current !== id) {
      setEditingId(null);
      setDraft("");
      return;
    }
    void removeEdited(id);
  };

  /**
   * NTS-FR-16: Save writes the edited body and returns the row to its rendered
   * form.
   *
   * Any body, including an empty one: emptying a note out and committing it is
   * an edit like any other, and the note survives it — the row then renders the
   * empty-note placeholder instead of text. Deleting is what Delete is for.
   * Saving nothing into a note that was never typed into is also a commit, so
   * that note stops being reapable too; Cancel and Escape remain the way to
   * walk away from one.
   *
   * The editor is closed only once the write has landed. Closing it first would
   * mean a failed write — a transient IPC error, or a note deleted from under
   * the row — leaves the row showing its old body with an error beneath it and
   * the text the user just typed gone for good.
   */
  const saveEdit = (id: string) => {
    const body = draft;
    // One operation per editor: a second Save inside the first one's round trip
    // would write the same body twice. The Save button is disabled from the same
    // state; this is what holds if a click reaches the handler anyway.
    if (busyRef.current) return;
    setBusy(id);
    // Committing is the opposite of abandoning, so the note stops being reapable
    // here — before the write, not after it. Otherwise the paths that reap on the
    // way past (an unmount, or the row leaving the list) would delete a note
    // mid-commit, and a failed write would leave one deletable by a later reap
    // with the user's typed body still in it. A commit that fails leaves an
    // ordinary note behind instead, which is now a thing a note can be.
    if (blankRef.current === id) setBlank(null);
    void (async () => {
      try {
        await api.updateNote(id, { body });
        setEditingId(null);
        setDraft("");
        setRowError(null);
        await load();
      } catch (e) {
        setRowError({ id, message: errorMessage(e) });
      } finally {
        setBusy(null);
      }
    })();
  };

  /**
   * NTS-FR-04: create a note from the panel.
   *
   * The note is written straight away and appears in the list — at the top of
   * it, since the list is most-recently-edited first (NTS-FR-15) — already in
   * inline edit with the caret in it. The user types into the row the note will
   * live in, rather than into a composer somewhere else that then turns into a
   * row. `cancelEdit` is what takes an abandoned blank one back out.
   *
   * A new note attaches to the entity the active tab is bound to, or to the
   * project when no tab is entity-bound — in every position, including
   * all-notes, whose grouping is a view rather than a writable scope.
   */
  const addNote = () => {
    // A second click while the first is still in flight must not write two —
    // and, per NTS-FR-17's one-editor-at-a-time rule, `+ Note` is the second
    // door into the editor and is closed while the first is open. Without this
    // a create would take the editor away from a row mid-edit, discarding what
    // was typed there and stranding the previous blank note.
    if (creating || editingId) return;
    setCreating(true);
    const scope: NoteScope = newNoteScope(position, entity);
    void (async () => {
      try {
        const created = await api.createNote({ scope, body: "" });
        setCreateError("");
        if (created?.id) setBlank(created.id);
        await load();
        if (created?.id) {
          setEditingId(created.id);
          setDraft("");
        }
      } catch (e) {
        setCreateError(errorMessage(e));
      } finally {
        setCreating(false);
      }
    })();
  };

  const openOverlay = (kind: Overlay["kind"], noteId: string, anchor: DOMRect) => {
    // NTS-FR-24: opening one overlay dismisses any other. A single piece of
    // state makes that structural rather than a rule to remember.
    setOverlay({ kind, noteId, x: anchor.left, y: anchor.bottom + 2 });
  };

  /**
   * The project's artifacts and Flows. Read from the Library tree, the one
   * operation NTS-FR-22 delegates for this — and the only place the artifact
   * *type* of a note's entity is knowable, which NTS-FR-11's routing needs.
   * Never throws: a tree that cannot be read leaves an empty list rather than
   * an overlay stuck on "Loading…".
   */
  const loadTargets = async (): Promise<MoveTarget[]> => {
    try {
      return artifactTargets(await api.loadProjectTree());
    } catch {
      return [];
    }
  };

  const startMove = (noteId: string, anchor: DOMRect) => {
    setMoveFilter("");
    setMoveTargets(null);
    openOverlay("move", noteId, anchor);
    void loadTargets().then(setMoveTargets);
  };

  /**
   * NTS-FR-11: follow an entity group header. The artifact's type is resolved
   * first, because it is what decides the surface the artifact opens in — a
   * Flow belongs in a Flow tab, not an Editor one (LIB-FR-03). An entity the
   * tree no longer holds still opens, by name, rather than doing nothing.
   */
  const openEntity = (entityId: string, name: string) => {
    void (async () => {
      const target = (await loadTargets()).find((t) => t.id === entityId);
      onOpenArtifact({
        id: entityId,
        name,
        artifactType: target?.artifactType });
    })();
  };

  const startReminder = (item: NoteListItem, anchor: DOMRect) => {
    setReminderDraft(toLocalInput(item.note.reminder));
    openOverlay("reminder", item.note.id, anchor);
  };

  const commitReminder = (id: string) => {
    const iso = fromLocalInput(reminderDraft);
    setOverlay(null);
    if (!iso) return;
    void run(id, () => api.updateNote(id, { reminder: iso }));
  };

  // --- Rendering ---------------------------------------------------------

  const scopeLabel =
    position === "entity"
      ? (entity?.name ?? "Current artifact")
      : position === "project"
        ? "Project-wide notes"
        : "All notes";

  /**
   * NTS-FR-09 / SNV-FR-62: the three scope positions, in the order the
   * requirement names them. The entity position's full name is whatever tab is
   * bound to it, which is why the tag stays fixed and the tooltip is what
   * varies — a filename in the row itself would resize the control every time
   * the active tab changed.
   */
  const scopePositions: SelectorPosition<NotesScopePosition>[] = useMemo(
    () => [
      {
        value: "entity",
        tag: "This file",
        title: entity?.name ?? "Current artifact",
        // Nothing entity-scoped is active, so the position has nothing to bind
        // to (NTS-FR-03).
        disabled: !entity },
      { value: "project", tag: "Project", title: "Project-Wide Notes" },
      { value: "all", tag: "All", title: "All Notes" },
    ],
    [entity],
  );

  /** Nothing is listed: either a load failed, or there is genuinely nothing. */
  const empty = !!loadError || (!!items && visible.length === 0);

  const emptyMessage =
    panel.text.trim() && all.length > 0
      ? "No note matches the filter."
      : position === "entity"
        ? "No notes on this artifact yet."
        : position === "project"
          ? "No project-wide notes yet."
          : "No notes in this project yet.";

  const renderRow = (item: NoteListItem) => {
    const note = item.note;
    const editing = editingId === note.id;
    return (
      <div
        key={note.id}
        className="note"
        data-testid="note-row"
        data-note-id={note.id}
        data-editing={editing}
        // NTS-FR-07: a note attached to a historical revision renders visually
        // distinct from one on the current version.
        data-historical={!!note.revision}
      >
        {editing ? (
          // The editor replaces the rendered body in place, with the same
          // typography and no box of its own, so the text does not shift under
          // the caret when a row enters edit.
          <textarea
            autoFocus
            ref={autoGrow}
            className="note__editor"
            aria-label="Edit note"
            value={draft}
            onChange={(e) => {
              setDraft(e.target.value);
              autoGrow(e.currentTarget);
            }}
            onKeyDown={(e) => {
              // NTS-FR-16: Escape is Cancel — the stored note is untouched.
              if (e.key === "Escape") {
                e.stopPropagation();
                cancelEdit();
              }
            }}
          />
        ) : (
          <>
            <div className="note__body">
              {/* A note may hold no text at all: emptying one out and saving is
                  an ordinary edit, and Delete is what removes a note. The row
                  says so in the middle of the space the text would occupy,
                  rather than rendering as a blank card. */}
              {note.body.trim() ? (
                /* NTS-FR-25: the body is Markdown and reads as rich text, at
                   the panel's own UI scale rather than the document scale —
                   a note row is a row, not a page. */
                <div className="note__text">
                  <CommentMarkdown body={note.body} />
                </div>
              ) : (
                <div className="note__text note__text--empty">
                  This note is empty
                </div>
              )}
              {/* NTS-FR-18: always rendered, on every row, including the
                  Unresolved group's. */}
              <button
                className="btn btn--ghost btn--icon btn--sm note__more"
                aria-label="Note actions"
                title="Note actions"
                onClick={(e) => {
                  e.stopPropagation();
                  openOverlay(
                    "menu",
                    note.id,
                    e.currentTarget.getBoundingClientRect(),
                  );
                }}
              >
                ⋯
              </button>
            </div>
            {/* NTS-FR-23: an unresolved note renders its last-known path in
                place of an entity name, and is reattached through Move…. */}
            {item.unresolved && (
              <div className="note__unresolved">
                <span className="note__unresolved-path">
                  {lastKnownPath(item)}
                </span>
                <UnresolvedMarker />
              </div>
            )}
          </>
        )}
        {/* The footer line is present in both states, so entering edit swaps
            what is in it rather than growing the row. */}
        <div className="note__meta">
          {note.reminder && (
            <span className="note__reminder" title={formatAbsolute(note.reminder)}>
              <Icon.Bell size={10} /> {formatAbsolute(note.reminder)}
            </span>
          )}
          {note.revision && (
            <span className="badge badge--accent">@ {note.revision}</span>
          )}
          {editing ? (
            // NTS-FR-16: Save and Cancel at the row's bottom-right, Cancel
            // leading — icons, in the space the timestamp occupies when the row
            // is merely rendered.
            <div className="note__edit-actions">
              <button
                className="btn btn--ghost btn--icon btn--sm"
                aria-label="Cancel edit"
                title="Cancel"
                // Both are unavailable while this row's own save or delete is in
                // flight, so the outcome is decided once.
                disabled={busyId === note.id}
                onClick={cancelEdit}
              >
                <Icon.X size={12} />
              </button>
              <button
                className="btn btn--ghost btn--icon btn--sm note__save"
                aria-label="Save note"
                title="Save"
                disabled={busyId === note.id}
                onClick={() => saveEdit(note.id)}
              >
                <Icon.Check size={12} />
              </button>
            </div>
          ) : (
            <span
              className="note__when"
              style={{ marginLeft: "auto" }}
              title={absoluteTitle(note)}
            >
              {formatRelative(note.updatedAt)}
            </span>
          )}
        </div>
        {rowError?.id === note.id && (
          <div className="note__error">{rowError.message}</div>
        )}
      </div>
    );
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div className="panel-header">
        <span className="panel-header__title">Notes</span>
        <span
          className="t-ui-xs"
          style={{ color: "var(--fg-3)", textTransform: "none", letterSpacing: 0 }}
        >
          {scopeLabel}
        </span>
      </div>

      {/* NTS-FR-09 / NTS-FR-12: the filter and the selector are pinned above
          the scrollable list and stay visible while it scrolls. SNV-FR-58 puts
          the text filter first, directly beneath the panel header, and the
          scope selector beneath it and directly above the list it narrows. */}
      <div className="panel-controls">
        <div className="search-input" style={{ height: 24 }}>
          <Icon.Search size={12} />
          <input
            placeholder="Filter notes…"
            aria-label="Filter notes"
            value={panel.text}
            onChange={(e) => panel.setText(e.target.value)}
          />
        </div>
        <SelectorRow
          label="Notes scope"
          positions={scopePositions}
          value={position}
          onChange={panel.setPosition}
        />
      </div>

      <div
        className={
          // With nothing to list, the container centres its one message
          // (NTS non-functional: the panel should not look broken when empty).
          empty ? "vpanel__body notes__body--empty" : "vpanel__body"
        }
      >
        {loadError && <div className="notes__message">{loadError}</div>}
        {!loadError && items && visible.length === 0 && (
          <div className="notes__message">{emptyMessage}</div>
        )}
        {grouped
          ? groups.map((group) => (
              // Keyed on the group's identity, not its label: two entities can
              // share a basename, and keying on the rendered text would collide
              // them into one React key.
              <div className="note-group" key={groupKey(group)}>
                {group.key.kind === "entity" ? (
                  // NTS-FR-11: an entity header follows to its artifact, by the
                  // same routing the Library click-through uses. The Project and
                  // Unresolved headers name no entity and are not click-throughs.
                  <button
                    // SNV-FR-57: an entity header carries a filename, which is
                    // content and keeps its on-disk case; the Project and
                    // Unresolved headers are labels and keep the eyebrow
                    // treatment the base class gives them.
                    className="note-group__header note-group__header--entity note-group__header--link"
                    // The full path, so two artifacts with the same basename are
                    // told apart before the user clicks one.
                    title={
                      group.key.kind === "entity" ? group.key.entityId : undefined
                    }
                    onClick={() =>
                      group.key.kind === "entity" &&
                      openEntity(group.key.entityId, group.key.name)
                    }
                  >
                    {group.label}
                  </button>
                ) : (
                  <div className="note-group__header">{group.label}</div>
                )}
                {group.items.map(renderRow)}
              </div>
            ))
          : flat.map(renderRow)}
      </div>

      {/* NTS-FR-04: the create affordance is pinned below the list. */}
      <div className="notes__create">
        <button
          className="btn btn--sm"
          aria-label="New note"
          // Unavailable while a create is in flight, and while a row is already
          // in inline edit — `+ Note` is the second door into the one-editor
          // rule of NTS-FR-17.
          disabled={creating || !!editingId}
          onClick={addNote}
        >
          <Icon.Plus size={12} /> Note
        </button>
        {createError && <div className="note__error">{createError}</div>}
      </div>

      {overlay && (
        <NoteOverlay
          overlay={overlay}
          item={noteById(overlay.noteId)}
          moveTargets={moveTargets}
          moveFilter={moveFilter}
          setMoveFilter={setMoveFilter}
          reminderDraft={reminderDraft}
          setReminderDraft={setReminderDraft}
          onClose={() => setOverlay(null)}
          onEdit={(item) => {
            setOverlay(null);
            // NTS-FR-17: at most one row is in inline edit at a time, so Edit on
            // a second row does nothing until the open one is resolved.
            if (editingId) return;
            setEditingId(item.note.id);
            setDraft(item.note.body);
          }}
          onMove={(item, anchor) => startMove(item.note.id, anchor)}
          onPickMoveTarget={(id, scope) => {
            setOverlay(null);
            void run(id, () => api.updateNote(id, { scope }));
          }}
          onDiscuss={(item) => {
            setOverlay(null);
            discuss(item);
          }}
          onReminder={(item, anchor) => startReminder(item, anchor)}
          onClearReminder={(item) => {
            setOverlay(null);
            void run(item.note.id, () =>
              api.updateNote(item.note.id, { reminder: null }),
            );
          }}
          onCommitReminder={commitReminder}
          onAskDelete={(item, anchor) =>
            openOverlay("confirmDelete", item.note.id, anchor)
          }
          onConfirmDelete={(id) => {
            setOverlay(null);
            void removeNote(id);
          }}
        />
      )}
    </div>
  );
}

// ---------------------------------------------------------------------------

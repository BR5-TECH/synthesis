import type { AttachmentInput } from "../types";
import type { FlushResult } from "./editSessions";
import { EditSessionStore } from "./editSessions";
import { draftDocKey, draftTransport } from "./documentTransport";

/**
 * The per-draft workspace state of every draft touched in this application
 * session (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-08, NAW-FR-10,
 * NAW-FR-12).
 *
 * Owned by the shell rather than by the New Artifact tab, for the same reason
 * `EditSessionStore` is: the workspace component is mounted only while its tab
 * is the active one, so state held inside it dies on every tab switch. Three
 * separate requirements depend on it outliving the component —
 *
 *  - NAW-FR-12: the draft's own edit state belongs to the draft, so reopening
 *    the tab returns the buffer, the mode and the undo history as they were;
 *  - NAW-FR-08: whether the History rail is shown belongs to the draft, and
 *    survives the tab closing and reopening within the session;
 *  - NAW-FR-13 / NAW-FR-23 / NAW-FR-24: the shell must be able to *flush* a
 *    dirty draft file before it tears anything down, which it cannot do for
 *    state it cannot reach.
 *
 * The *document* itself — buffer, dirty flag, editing mode, undo history, find
 * state — is not held here. It lives in `docs`, an ordinary `EditSessionStore`
 * over the draft transport, because a draft's prompt is edited in the Editor's
 * own surface and therefore carries the Editor's own retained edit state
 * (NAW-FR-11 / NAW-FR-12). This class holds only what is about the draft rather
 * than about the document.
 *
 * Held in memory only and cleared when the project closes, exactly like the
 * artifact and Flow stores beside it.
 */
export interface DraftSession {
  draftId: string;
  /**
   * NAW-FR-06: the draft-relative path of the prompt the workspace is showing.
   * Null until the record has been read, and for a draft whose storage is not
   * the single prompt DRS-FR-11 requires (NAW-FR-41).
   */
  selected: string | null;
  /**
   * NAW-FR-08: whether the **History rail** is shown. Hidden is the default,
   * because a draft is one prompt being written and the width belongs to the
   * writing.
   */
  railShown: boolean;
  /**
   * NAW-FR-25: where this session believes the draft's **prompt** sits, as of
   * the last time the record was read.
   *
   * Held here rather than in the workspace because the workspace is mounted only
   * while its tab is the active one, and a rename made in the Drafts panel has
   * to move this session's keys whether or not anyone is looking at the draft —
   * a buffer left under the old path is written back on the next flush, which
   * recreates the file the rename moved away from.
   */
  promptPath: string | null;
  /**
   * NAW-FR-15: what the author has typed into the floating composer.
   *
   * It belongs to the **draft** rather than to the composer, so dismissing the
   * composer and reopening it returns what was typed — and so does closing the
   * tab and reopening the draft within the session. It is not part of any draft
   * file, contributes no match to the find panels, occupies no position in any
   * undo history, and is never persisted: it lives here and nowhere else, and
   * goes when the project closes.
   */
  composer: string;
  /**
   * NAW-FR-15 / NAW-FR-31: what the composer has queued but not yet posted.
   *
   * Held beside the typed body and on exactly the same terms — it belongs to the
   * draft, so dismissing the composer and reopening it returns the strip, and so
   * does closing the tab and reopening the draft within the session. Nothing has
   * been sent anywhere until the message is posted (CMT-FR-46), so this is a
   * queue and not a record of anything.
   *
   * Structurally the `PendingAttachment` the rail's composers hold, spelled out
   * here so this module depends on the wire types alone rather than on a
   * component.
   */
  composerAttachments: { input: AttachmentInput; name: string }[];
  /**
   * NAW-FR-57 / NAW-FR-59: whether the tab's find panels are reachable right
   * now — true only while it is showing the **live prompt** of a draft that
   * takes edits.
   *
   * Held here rather than in the workspace because the shell is what greys the
   * Edit menu's two items and routes their accelerators (SNV-FR-43), and the
   * shell cannot see inside the tab: whether a past version is being read
   * (NAW-FR-09) and whether a graduation holds the draft (NAW-FR-44) are both
   * the workspace's own knowledge. The workspace publishes the answer here and
   * the shell reads it, so one rule decides the menu, the accelerator, and what
   * the surface will accept.
   *
   * False until the workspace has mounted and read the draft's record, which is
   * also the state a tab left reading a version is in.
   */
  promptEditable: boolean;
}

export class DraftSessionStore {
  /**
   * NAW-FR-11 / NAW-FR-12: the draft files' edit sessions, keyed by
   * `<draftId>/<draft-relative path>`. An ordinary `EditSessionStore` — the same
   * one an Editor tab is a view onto — so a draft's prompt gets the Editor's
   * retained buffer, dirty flag, mode, undo history and find state without a
   * second implementation of any of them.
   *
   * Built with `autoWrite` off: a prompt's rest-after-typing schedule is the
   * New Artifact tab's own (NAW-FR-13), because writing one also reports the
   * write in the tab and refreshes the draft list. Two schedules over one buffer
   * would write it twice.
   */
  readonly docs = new EditSessionStore(draftTransport, false);

  private sessions = new Map<string, DraftSession>();
  private listeners = new Set<() => void>();
  private version = 0;

  constructor() {
    // The workspace and the shell both need to re-render when a draft file goes
    // dirty or clean, and that transition is recorded on `docs`. Forwarding here
    // means a consumer subscribes to one store rather than remembering to
    // subscribe to two.
    this.docs.subscribe(() => this.notify());
  }

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  getVersion = (): number => this.version;

  private notify(): void {
    this.version += 1;
    this.listeners.forEach((l) => l());
  }

  /** The store key of one file of one draft. */
  key(draftId: string, path: string): string {
    return draftDocKey(draftId, path);
  }

  ensure(draftId: string): DraftSession {
    let session = this.sessions.get(draftId);
    if (!session) {
      session = {
        draftId,
        selected: null,
        railShown: false,
        promptPath: null,
        composer: "",
        composerAttachments: [],
        promptEditable: false,
      };
      this.sessions.set(draftId, session);
    }
    return session;
  }

  get(draftId: string): DraftSession | undefined {
    return this.sessions.get(draftId);
  }

  /** NAW-FR-10: move the selection. */
  select(draftId: string, path: string | null): void {
    const session = this.ensure(draftId);
    if (session.selected === path) return;
    session.selected = path;
    this.notify();
  }

  /**
   * NAW-FR-25: record where the draft's prompt sits, as the record last reported
   * it. Notifies nothing: it is bookkeeping for `followPrompt` rather than
   * anything a surface renders.
   */
  setPromptPath(draftId: string, path: string | null): void {
    this.ensure(draftId).promptPath = path;
  }

  /**
   * NAW-FR-25: follow the prompt of a draft renamed somewhere else.
   *
   * Carries this session's keys — buffer, undo history, selection — from where
   * the file was to where it now is, and reports whether anything moved. Safe on
   * a draft whose keys have already been carried: the two paths agree by then
   * and nothing happens.
   */
  followPrompt(draftId: string, promptPath: string | null): boolean {
    const session = this.ensure(draftId);
    const before = session.promptPath;
    session.promptPath = promptPath;
    if (before === null || promptPath === null || before === promptPath) return false;
    this.renamePath(draftId, before, promptPath);
    return true;
  }

  /**
   * NAW-FR-15: what is in the composer, per draft. Invokes nothing and reaches
   * no integration — this is the whole of what typing into it does.
   */
  setComposer(draftId: string, text: string): void {
    const session = this.ensure(draftId);
    if (session.composer === text) return;
    session.composer = text;
    this.notify();
  }

  /**
   * NAW-FR-31: what the composer has queued, per draft. Like the body above it
   * invokes nothing — a pending attachment reaches the backend only when the
   * message is posted (CMT-FR-46).
   */
  setComposerAttachments(
    draftId: string,
    pending: { input: AttachmentInput; name: string }[],
  ): void {
    const session = this.ensure(draftId);
    session.composerAttachments = pending;
    this.notify();
  }

  /**
   * NAW-FR-57 / NAW-FR-59: record whether this draft's find panels are
   * reachable, as the tab showing it knows it. Notifies only on a change, so a
   * workspace that republishes the same answer on every render costs the shell
   * no re-render.
   */
  setPromptEditable(draftId: string, editable: boolean): void {
    const session = this.ensure(draftId);
    if (session.promptEditable === editable) return;
    session.promptEditable = editable;
    this.notify();
  }

  /**
   * NAW-FR-57: the edit session of this draft's live prompt, or null while the
   * prompt is not editable — a version being read, a draft its graduation
   * holds, or a record that has not landed yet (NAW-FR-09, NAW-FR-44).
   *
   * This is the key the find panels' state hangs off (NAW-FR-12), which is what
   * the shell needs to route an accelerator without reaching into the tab.
   */
  searchTarget(draftId: string): string | null {
    const session = this.sessions.get(draftId);
    if (!session || !session.promptEditable || session.selected === null) {
      return null;
    }
    return this.key(draftId, session.selected);
  }

  /** NAW-FR-08: show or hide the History rail, per draft. */
  setRailShown(draftId: string, shown: boolean): void {
    const session = this.ensure(draftId);
    if (session.railShown === shown) return;
    session.railShown = shown;
    this.notify();
  }

  /**
   * NAW-FR-25: the draft was renamed, which renames its prompt (DRS-FR-14), so
   * the edit state recorded under the old path names nothing.
   *
   * The state is carried to the new key rather than dropped: a rename is not a
   * reason to lose an unsaved buffer or an undo history. The match is on the
   * path or a `/`-terminated prefix of it and never a bare `startsWith`, so a
   * key for `DRS.md` never drags a sibling `DRS.md.bak` along.
   */
  renamePath(draftId: string, from: string, to: string): void {
    const session = this.ensure(draftId);
    const moves = (path: string) => path === from || path.startsWith(`${from}/`);
    for (const key of this.docs.allIds()) {
      const prefix = `${draftId}/`;
      if (!key.startsWith(prefix)) continue;
      const path = key.slice(prefix.length);
      if (!moves(path)) continue;
      this.docs.rekey(key, this.key(draftId, to + path.slice(from.length)));
    }
    if (session.selected !== null && moves(session.selected)) {
      session.selected = to + session.selected.slice(from.length);
      this.notify();
    }
  }

  /** Whether this draft's prompt holds unsaved changes. */
  isDirty(draftId: string): boolean {
    const prefix = `${draftId}/`;
    return this.docs.dirtyIds().some((key) => key.startsWith(prefix));
  }

  /** Draft ids holding at least one unsaved file — the scope of a Save All. */
  dirtyIds(): string[] {
    const ids = new Set<string>();
    for (const key of this.docs.dirtyIds()) {
      const cut = key.indexOf("/");
      if (cut > 0) ids.add(key.slice(0, cut));
    }
    return [...ids];
  }

  /**
   * NAW-FR-13: write every pending file of this draft. A draft with nothing
   * dirty writes nothing and reports success, so a caller sweeping every draft
   * need not check first.
   */
  async flush(draftId: string): Promise<FlushResult> {
    const prefix = `${draftId}/`;
    for (const key of this.docs.dirtyIds()) {
      if (!key.startsWith(prefix)) continue;
      const res = await this.docs.flush(key);
      // Reported against the draft rather than the file, because the draft is
      // what the shell can focus a tab on.
      if (!res.ok) return { ...res, artifactId: draftId };
    }
    return { ok: true };
  }

  /** NAW-FR-23 / NAW-FR-24: write every dirty draft before a teardown. */
  async flushAll(): Promise<FlushResult> {
    for (const id of this.dirtyIds()) {
      const res = await this.flush(id);
      if (!res.ok) return res;
    }
    return { ok: true };
  }

  /** DRP-FR-12 / NAW-FR-20: the draft is gone, so its session goes with it. */
  drop(draftId: string): void {
    const prefix = `${draftId}/`;
    for (const key of this.docs.allIds()) {
      if (key.startsWith(prefix)) this.docs.forget(key);
    }
    if (this.sessions.delete(draftId)) this.notify();
  }

  clear(): void {
    this.sessions.clear();
    this.docs.clear();
    this.notify();
  }
}

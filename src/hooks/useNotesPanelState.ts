/**
 * The Notes panel's scope position and filter text, and their persistence
 * (NTS-FR-09 / NTS-FR-13 / PSS-FR-19).
 *
 * This lives in a hook rather than inside `Notes` for the same reason the
 * Library's does: `VPanel` swaps `<Notes>` out for `<Library>` / `<Changes>`
 * when the user picks another vertical-panel surface, which unmounts it
 * outright. Holding the position there would lose the stickiness NTS-FR-09
 * requires across a panel switch, along with any write still inside the debounce
 * window.
 *
 * The caller is `VPanel`, whose lifetime is exactly one project + one content
 * root. A pending write survives a surface switch, and a worktree switch
 * destroys it with the rest of the subtree rather than flushing it — which is
 * what keeps the outgoing worktree's position out of the incoming worktree's
 * store (PSS-FR-16).
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../api";
import type { NotesScopePosition } from "../types";

/**
 * How long a burst of changes is coalesced before one write: a run of
 * keystrokes in the filter field produces a single save rather than one per
 * character.
 */
export const NOTES_PERSIST_DEBOUNCE_MS = 300;

/** PSS-FR-19: the position a project with nothing persisted starts in. */
export const DEFAULT_SCOPE_POSITION: NotesScopePosition = "entity";

/** What `Notes` renders from and mutates. */
export interface NotesPanel {
  /**
   * NTS-FR-09: the selector's position. This is the *persisted* choice, not
   * necessarily what is rendered — with no entity-scoped tab active the panel
   * renders project-wide without rewriting this value.
   */
  position: NotesScopePosition;
  setPosition: (position: NotesScopePosition) => void;
  text: string;
  setText: (text: string) => void;
  /** False until the persisted record has landed, so nothing renders over it. */
  restored: boolean;
}

function signature(position: NotesScopePosition, text: string): string {
  return JSON.stringify([position, text]);
}

export function useNotesPanelState(): NotesPanel {
  const [position, setPosition] = useState<NotesScopePosition>(
    DEFAULT_SCOPE_POSITION,
  );
  const [text, setText] = useState("");
  const [restored, setRestored] = useState(false);

  // The state last written, so a value that merely arrived from the backend is
  // never written straight back.
  const persisted = useRef<string | null>(null);
  // Dispatch ordinal of the most recent write, so an older write settling out
  // of order cannot claim `persisted`.
  const writeSeq = useRef(0);

  /**
   * Which fields the user has already changed. The selector and the filter
   * input are interactive from the first paint, so a change can land before the
   * restore resolves; applying the stored value over it would silently revert
   * what the user just did.
   */
  const touched = useRef({ position: false, text: false });

  /**
   * The current values, mirrored synchronously so the restore can read what the
   * user has already done without waiting for a render to commit.
   */
  const current = useRef({
    position: DEFAULT_SCOPE_POSITION as NotesScopePosition,
    text: "",
  });

  // NTS-FR-09 / NTS-FR-13: restore the position and the filter for this
  // worktree. Runs once per project + content root, so a worktree switch
  // restores the incoming worktree's own record and carries nothing across.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      let state: Awaited<ReturnType<typeof api.loadNotesPanelState>> | null =
        null;
      try {
        state = await api.loadNotesPanelState();
      } catch {
        // A record that cannot be read leaves the panel on its defaults rather
        // than blocking it.
      }
      if (cancelled) return;

      const storedPosition = state?.scopePosition ?? DEFAULT_SCOPE_POSITION;
      const storedText = state?.textFilter ?? "";
      if (!touched.current.position) {
        current.current.position = storedPosition;
        setPosition(storedPosition);
      }
      if (!touched.current.text) {
        current.current.text = storedText;
        setText(storedText);
      }
      // Seeded with what the store actually holds, NOT with the merged pair.
      // The selector and the filter are interactive from the first paint, so a
      // change can land inside this window; seeding with the merge would make
      // that change equal to `persisted` and the write below would be skipped —
      // silently losing the one choice this whole `touched` dance exists to
      // protect. Seeded with the stored values, a touched field differs and
      // persists on the next effect pass.
      persisted.current = signature(storedPosition, storedText);
      setRestored(true);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // NTS-FR-09 / NTS-FR-13: persist after any change to either value. The record
  // is written whole, so persisting one carries the other through unchanged
  // (PSS-FR-19).
  useEffect(() => {
    if (!restored) return;
    const next = signature(position, text);
    if (persisted.current === next) return;
    const timer = setTimeout(() => {
      // Two writes can be in flight at once. Whichever reaches disk last is
      // what the store holds, so only the newest dispatch may record what is
      // persisted; an older one settling later must not claim the guard.
      const ticket = ++writeSeq.current;
      void api
        .saveNotesPanelState({ scopePosition: position, textFilter: text })
        // Only a write that landed counts as persisted, so a failure is retried
        // by the next change rather than being assumed durable.
        .then(() => {
          if (ticket === writeSeq.current) persisted.current = next;
        })
        .catch(() => {});
    }, NOTES_PERSIST_DEBOUNCE_MS);
    // Cancelling rather than flushing, for the reason in the module doc: by
    // unmount time the backend already resolves project-local storage against
    // the *new* worktree (PSS-FR-16).
    return () => clearTimeout(timer);
  }, [position, text, restored]);

  return {
    position,
    setPosition: (next) => {
      touched.current.position = true;
      current.current.position = next;
      setPosition(next);
    },
    text,
    setText: (next) => {
      touched.current.text = true;
      current.current.text = next;
      setText(next);
    },
    restored,
  };
}

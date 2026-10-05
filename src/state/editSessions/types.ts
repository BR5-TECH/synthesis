import type { EditHistory, EditMode } from "../editHistory";
import type { FindState } from "../findState";
import type { Indentation } from "../indentation";

/**
 * EDT-FR-28–EDT-FR-33: the application-session-lived edit state of every
 * artifact the user has touched.
 *
 * An Editor tab is a *view* onto one of these records, not its owner: the record
 * outlives the tab, so switching tabs (EDT-FR-30), closing a tab and reopening
 * the artifact (EDT-FR-29) all resume the same buffer, dirty state, editing
 * mode, and undo/redo history. Nothing here is persisted — the store is
 * discarded when the project closes or the application exits (EDT-FR-28).
 *
 * The store also owns the *write* path (EDT-FR-31–EDT-FR-33, EDT-FR-70): an
 * artifact writes itself a short rest after the last keystroke, and closing a
 * tab, the project, or the application brings that pending write forward through
 * `"save artifact contents"`. A flush that would auto-resolve a divergence
 * refuses instead (EDT-FR-32).
 *
 * The rest-after-typing timer lives *here* rather than in the Editor because
 * only the active tab's Editor is mounted (EDT-FR-30): a timer owned by the
 * component would be torn down the moment the author switched tabs, and the edit
 * they had just made would sit unwritten until they came back. Held by the store,
 * it lands in the background exactly as EDT-FR-70 requires.
 */
export interface EditSession {
  artifactId: string;
  /** EDT-FR-22: the artifact's single undo/redo history. */
  history: EditHistory;
  /** The whole Markdown the artifact currently holds, in either mode. */
  buffer: string;
  /** EXC-FR-WCOM: the divergence baseline. */
  baseline: string | null;
  /** Checksum carried by an unresolved external change, adopted on resolve. */
  pending: string | null;
  dirty: boolean;
  /** EXC-FR-VTUH: the blocking external-change modal is up for this artifact. */
  conflict: boolean;
  /** EDT-FR-17/EDT-FR-29: the editing mode a reopened tab resumes in. */
  mode: EditMode;
  /**
   * EDT-FR-37/EDT-FR-39: the artifact's indentation convention — what a Tab
   * keypress inserts in raw-text mode, and what the status bar's indentation
   * control reports (STB-FR-21). Detected from the body on load; overridable
   * from that control.
   */
  indentation: Indentation;
  /**
   * Whether the convention above was chosen by the user rather than detected.
   * An override is something the user authored, so — like an edit — it keeps the
   * record alive when the tab closes (EDT-FR-39 / STB-FR-24); a merely detected
   * convention is re-derived on the next load and needs no retention.
   */
  indentationOverridden: boolean;
  /**
   * EFR-FR-GIPZ: the find panel — whether it is open and in which form, the query,
   * the replacement text, and the active match mode. Retained with everything
   * else here, so reopening the artifact restores the panel it was left with.
   */
  find: FindState;
  /**
   * CMT-FR-30: whether the comment rail is open. Part of the artifact's retained
   * edit state, so closing the rail survives the tab closing and reopening within
   * the session, and is discarded with everything else here.
   *
   * `null` means the author has not said, and the default applies: the rail opens
   * for an artifact carrying at least one unresolved thread and stays closed for
   * one carrying none. Only an explicit toggle writes a boolean here, which is
   * what keeps the *default* from looking like a decision — an artifact that was
   * merely opened and read must still be dropped on close (EDT-FR-28).
   */
  railOpen: boolean | null;
  /**
   * CMT-FR-17 / CMT-FR-30: whether the rail's resolved-thread disclosure is
   * expanded. Retained beside `railOpen` for the same reason and on the same
   * terms — the Editor is unmounted and remounted when its tab is closed and
   * reopened, so component state would collapse it every time.
   *
   * Collapsed is the default, so `false` is pristine and expanding it is the
   * author's own choice.
   */
  resolvedOpen: boolean;
  /** Last load/save error, surfaced by whichever tab shows the artifact. */
  error: string | null;
  /** True once the first load attempt has settled. */
  loaded: boolean;
  /**
   * Set when the artifact's tab closes: the next open re-invokes the load and
   * compares its checksum against `baseline` before resuming (EDT-FR-29).
   */
  revalidate: boolean;
  /**
   * Whether a tab is currently open on this artifact. A divergence raised on an
   * artifact nobody has open still refuses a write (`flush`), but must not block
   * a teardown: there would be no tab to show the modal in, and nothing unsaved
   * is at stake — a closed artifact's changes were written as it closed.
   */
  tabOpen: boolean;
  /**
   * EDT-FR-70: a write whose rest elapsed while the tab's external-change modal
   * was unresolved. It is **held** — neither performed nor dropped — and what
   * becomes of it follows the resolution: "Keep my version" performs it at once
   * (EXC-FR-WDEJ), "Load from filesystem" discards it with the buffer it would
   * have carried (EXC-FR-WDAV).
   */
  heldWrite: boolean;
  /**
   * An in-flight `"load artifact contents by id"` for this artifact, if any. A
   * write waits on it (EDT-FR-29): a revalidating load may be about to discover
   * that the file changed on disk, and writing before that lands would overwrite
   * the divergence instead of raising the modal for it.
   */
  pendingLoad: Promise<void> | null;
  /**
   * The tail of this document's write queue. Every `flush` links itself onto it
   * and waits, so two writes of the same document never overlap.
   *
   * Without it the rest elapsing and a Save (or a teardown's sweep) can each
   * start a `"save artifact contents"` while the other is still in flight. Their
   * completions then race: the older buffer can land on disk after the newer
   * one, and its `checksum` becomes the baseline with `dirty` cleared — so the
   * edit that was actually lost looks saved, raises no divergence, and is gone
   * for good.
   */
  writeChain: Promise<void>;
  /**
   * Bumped whenever a load is adopted, so a mounted editing surface re-seeds
   * itself from `buffer`. A reopen that only revalidates does not bump it.
   */
  seedToken: number;
  /**
   * EDT-FR-81: the session is **quiesced** — a surface is about to discard its
   * edits on the author's instruction (the Changes panel's rollback,
   * CHG-FR-60), so it neither schedules nor starts a write.
   *
   * A scheduled write is cancelled rather than performed, a further edit
   * schedules nothing, and every route to a write refuses: the schedule, File →
   * Save and Save All, and a close-time flush alike. A write already *in flight*
   * is untouched by this — it is awaited (`quiesce`), because a write the
   * filesystem has already begun cannot be recalled and a rollback that raced it
   * would be rolling back a file mid-write.
   */
  quiesced: boolean;
}

/** EDT-FR-81: a save that failed while a session was being quiesced. */
export interface QuiesceFailure {
  artifactId: string;
  /** The write's typed failure, as the transport reported it. */
  reason: string;
}

/** Why a flush refused to write (EDT-FR-32). */
export type FlushBlock = "conflict" | "error";

export interface FlushResult {
  ok: boolean;
  /** The reason a blocked flush gave up; absent when `ok`. */
  blocked?: FlushBlock;
  /** The artifact whose flush blocked, so the caller can focus its tab. */
  artifactId?: string;
}

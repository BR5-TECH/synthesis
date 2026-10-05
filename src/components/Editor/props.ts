import type { ReactNode } from "react";

import type { EditorImageHost } from "../hostImages";
import type { EditSessionStore } from "../../state/editSessions";
import type { ArtifactType, NodeType } from "../../types";

/**
 * EDT-FR-84: the review modal this tab hosts, whose candidate keeps its own undo
 * history (PCR-FR-20), and the proposed text of a draft change, which keeps
 * its own too (DCR-FR-24). An undo issued in either reverses an edit there and
 * never one to the artifact.
 *
 * A selector rather than a ref because the modal mounts and unmounts several
 * layers below, and what the accelerator needs to know is only whether the event
 * it is looking at came from inside it.
 */
export const REVIEW_SELECTOR = ".draft-review, .hunk__doc";

/**
 * DCR-FR-05: one proposed change, as the review hands it to the editing
 * surface.
 *
 * The change names the text it alters rather than a position (DCP-FR-HRQN), so
 * this carries that text and lets the surface find it — the same rule an
 * anchored comment is placed by (CMT-FR-06). `hint` is only a hint.
 */
export interface ReviewHunk {
  id: string;
  kind: "add" | "del" | "replace";
  state: "pending" | "accepted" | "rejected" | "discussing";
  /** The exact prompt text the change removes or replaces. Empty for an insertion. */
  before: string;
  /** What the change proposes. Empty for a deletion. */
  after: string;
  /** The text immediately before the change, which is what places an insertion. */
  lead: string;
  /** Where the change was when the proposal was recorded. Only a hint. */
  hint: number;
  /** DCP-FR-BMLX: the prompt no longer holds the text this change names. */
  lost: boolean;
  position: number;
  total: number;
  agent: string;
  /**
   * DCR-FR-24: the proposed text as the author has it, where they have edited
   * it. Absent while it is still the agent's own words.
   */
  draft?: string | null;
  /** DCR-FR-KDSV: a legacy proposal's text is not editable. */
  editable: boolean;
}

/**
 * DDS-FR-ZRPT: what the document column needs to review a proposal in place.
 *
 * Passed as one object rather than as eight props because it is one thing that
 * is either there or not: a draft holding no undecided proposal passes nothing
 * and the surface is exactly the editing surface it always was.
 */
export interface EditorReview {
  /**
   * DCR-FR-30: which proposal is being reviewed.
   *
   * A draft may hold a second proposal on the same open tab once the first is
   * resolved (per `../../../specifications/core/DCP-draft-change-proposals.md`
   * DCP-FR-04), and the surface is not rebuilt for it. Anything that must
   * behave as though the review had just opened reads this rather than its own
   * mount.
   */
  proposalId: string;
  hunks: ReviewHunk[];
  /** DCR-FR-12: the change the action chip is on. */
  focused: string | null;
  /** A decision is in flight, so every decision control is off. */
  busy: boolean;
  onFocus: (hunkId: string) => void;
  onAccept: (hunkId: string) => void;
  onReject: (hunkId: string) => void;
  onDiscuss: (hunkId: string) => void;
  /** DCR-FR-24 / DCR-FR-26: the author rewrote a change's proposed text. */
  onEdit: (hunkId: string, after: string) => void;
  /**
   * DCR-FR-11 / DDS layout: the review bar, rendered by the host and placed
   * here — between the formatting toolbar and the scroller, so it reads as
   * chrome of the document rather than of the tab.
   */
  bar?: ReactNode;
  /** DCR-FR-30: the caret entered the prose, so the accelerators stand down. */
  onCaretInProse: () => void;
  /**
   * DCR-FR-31: which changes the document could draw.
   *
   * Placement is the editing surface's — it is the only thing that holds the
   * rendered document — so a change it could not place is a fact only it has.
   * Reported upward because the review bar is what states it, and a bar that
   * counts a change the page does not show says nothing about the difference.
   */
  onPlaced: (hunkIds: string[]) => void;
}

/**
 * DDS-FR-QMBC: what a host lends the Editor so that the discussions about the
 * passages of its text are marked in it, without a comment rail.
 */
export interface EditorFragmentHost {
  /** The passages the host's fragment discussions are about, as source ranges. */
  fragments: readonly {
    id: string;
    start: number;
    end: number;
    quote: string;
    resolved: boolean;
  }[];
  /** The discussion the host has focused. Its passage is scrolled into view. */
  focusedId: string | null;
  /**
   * The author asked to see a passage again. A new `n` scrolls it into view
   * even when it is the one already focused.
   */
  reveal?: { id: string; n: number } | null;
  /** The author activated Comment on a selection. */
  onComment: (selection: { start: number; end: number; quote: string }) => void;
  /** The author activated the mark of a fragment. */
  onActivate: (discussionId: string) => void;
}

export interface EditorProps {
  /**
   * DDS-FR-QMBC: the fragment discussions of a host that renders discussions
   * itself. Absent for every other surface.
   */
  fragmentHost?: EditorFragmentHost;
  /**
   * DCR-FR-01 / DCR-FR-05: the proposal being reviewed in this document, if
   * one is. The review is rendered **in** the document rather than over it.
   */
  review?: EditorReview;
  /** Backend artifact id (project-relative path) when opened from a real node. */
  artifactId?: string;
  artifactName?: string;
  /**
   * Resolved artifact type. Threaded through for the action control, which
   * offers Implement on a `spec` and Discuss on everything else (EDT-FR-64).
   */
  artifactType?: ArtifactType | NodeType;
  /**
   * EDT-FR-28: the application-session store the artifact's edit state lives in.
   * The shell owns it, so state survives this component (tab switch, close and
   * reopen). A surface that mounts an Editor without one gets a private store,
   * which behaves identically except that nothing outlives the component.
   */
  sessions?: EditSessionStore;
  /**
   * CMT-FR-36: a thread to land on, set when this tab was reached from the
   * Comments panel (`CMP-comments-panel.md` CMP-FR-11). Consumed once — the
   * Editor calls `onThreadFocused` as soon as it has taken it, so a later
   * re-render does not yank the body back to a passage the author has since
   * scrolled away from.
   */
  focusThreadId?: string | null;
  onThreadFocused?: () => void;
  /**
   * NAW-FR-13: called on every user edit, whichever surface produced it.
   *
   * The session store notifies only on the clean→dirty transition, because it
   * runs per keystroke and a notify per keystroke would re-render the shell —
   * so a debounce armed on that transition alone would fire in the middle of the
   * first word rather than at the end of the burst. This is the per-edit signal
   * a draft's autosave restarts its timer on.
   */
  onEdit?: () => void;
  /**
   * CMT-FR-37 / NAW-FR-14: whether the comment rail belongs on this surface.
   *
   * A draft file's threads are served in the *draft* scope of the thread
   * operations (`../../specifications/core/CMS-comments-storage.md` CMS-FR-36),
   * which this surface does not yet distinguish — so a draft passes `false`
   * rather than have the rail load and write artifact-scoped threads against a
   * key that names no artifact.
   */
  showComments?: boolean;
  /**
   * ACT-FR-01: whether this Editor mounts the tab's action control.
   *
   * A tab carries exactly one and never more, so an Editor **embedded** in a tab
   * that has its own — the New Artifact tab, whose control governs the draft
   * rather than the one file of it currently on screen — passes `false`. A draft
   * file is not an item of the project and has no discussions of its own
   * (per `NAW-new-artifact.md` NAW-FR-15).
   */
  showActions?: boolean;
  /**
   * NAW-FR-44: the surface takes no edits at all.
   *
   * A draft a graduation run holds, and one a run has already graduated, are
   * both read-only (`../../specifications/core/GRD-graduation.md` NAW-FR-44,
   * ) — the run is answering that prompt, and a graduated draft is the
   * record of what a published specification was written from. Folded into the
   * same `blocked` the conflict state uses, because "this surface takes no
   * edits" is one condition however it arose, and two of them would drift.
   */
  readOnly?: boolean;
  /**
   * NAW-FR-13: what this surface's own write schedule has to report, rendered in
   * the action row. Absent for an artifact, whose Save is the author's (EDT-FR-04)
   * — a draft's prompt writes itself, so the report is the whole of what it has
   * in place of a Save control.
   */
  report?: string;
  /**
   * EDT-FR-85 / EDT-FR-86: what this surface's **host** lends it about images.
   *
   * Absent for an Editor tab on a project file, which resolves no destination
   * and takes no pasted picture — so a paste there behaves exactly as it does
   * today (EDT-FR-86). A New Artifact tab supplies one, and it is that tab
   * rather than this surface that decides what an image resolves to and what a
   * paste writes (per `../../specifications/ui/NAW-new-artifact.md` NAW-FR-50,
   * NAW-FR-52).
   */
  imageHost?: EditorImageHost;
  /**
   * NAW-FR-54: how the host announces an outcome through the tab's own
   * accessibility feedback. Called with the words to announce.
   */
  announce?: (message: string) => void;
}

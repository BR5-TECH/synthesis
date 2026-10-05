/**
 * Drafts — the vertical-panel surface listing the artifacts being developed in
 * New Artifact tabs before they exist in the project
 * (`../../specifications/ui/DRP-drafts-panel.md`).
 *
 * The panel is a **tree**, not a list: the author cuts folders of their own and
 * files drafts into them, and a folder here is an ordinary directory under the
 * worktree's drafts root — so the shape on screen is the shape on disk
 * (DRS-FR-29). Both halves arrive from `list_drafts` alone, however deep the
 * tree runs (DRP-FR-20); a folder costs no call of its own and expanding one
 * issues nothing.
 *
 * The one call that reads a draft's files is `search_drafts`, and only while the
 * text filter holds something — so a panel opened and never filtered costs one
 * list call whatever the size of the drafts in the worktree.
 *
 * Nothing here is rendered from what the panel *attempted*: every mutation is
 * followed by a re-list (DRP-FR-32), so the tree never shows an organisation the
 * disk does not have.
 */

import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import * as api from "../api";
import { logWarn } from "../logging";
import { githubPollingErrorMessage, refusalCode } from "../state/githubPolling";
import type {
  DraftFolder,
  DraftHierarchy,
  DraftSummary,
  PanelRevealRequest,
} from "../types";
import type { DraftsPanel as DraftsPanelState } from "../hooks/useDraftsPanelState";
import {
  markRevealConsumed,
  revealAlreadyConsumed,
} from "../state/panelReveal";
import {
  ancestorsOf,
  anchorOfRow,
  FILTER_DEBOUNCE_MS,
  folderJoin,
  folderName,
  folderParent,
  isWithin,
  itemKey,
  ROOT,
  toMatchMap,
  type DraftMatches,
  type MenuAnchor,
  type TreeItem,
} from "./draftsTree";
import { createDraftActions } from "./draftsActions";
import { DraftsPanelView } from "./DraftsPanelView";
import {
  makeRowRenderers,
  type Inline,
  type Overlay,
} from "./draftsRows";

/**
 * The tree vocabulary and the floating surfaces live beside this file. They are
 * re-exported here because this module is the panel's public face: a caller —
 * and every test — names `DraftsPanel` and takes what it needs from it.
 */
export {
  ancestorsOf,
  countPhrase,
  folderJoin,
  folderName,
  folderNameProblem,
  folderParent,
  isWithin,
  itemKey,
  toMatchMap,
  anchorOfRow,
  ROOT,
  ROOT_OPTION,
  FILTER_DEBOUNCE_MS,
} from "./draftsTree";
export type {
  DraftFilter,
  DraftMatches,
  MenuAnchor,
  TreeItem,
} from "./draftsTree";

interface DraftsPanelProps {
  panel: DraftsPanelState;
  /** DRP-FR-09: open a draft in its New Artifact tab, or focus the open one. */
  onOpenDraft: (draft: DraftSummary) => void;
  /**
   * DRP-FR-06 / DRP-FR-26: create a draft and open it. The pinned affordance
   * passes no folder and creates at the root; a folder's **New Draft** passes
   * that folder — which is where the draft is *filed* and says nothing about
   * where its specification eventually lands (DRS-FR-07).
   *
   * Resolves the draft that was created, so this panel can put its row straight
   * into rename mode (DRP-FR-36) — which is why the creation asks for no name up
   * front. `null` where the creation failed and the shell has already reported
   * it; there is no row to name in that case.
   *
   * The draft is created holding the project's draft template where one is
   * configured (DRS-FR-39). That copy is the creation operation's: this panel
   * neither reads the template nor passes it, which is what makes both routes
   * produce the same starting content (DRP-FR-06).
   */
  onCreateDraft: (
    folder?: string,
  ) => Promise<{ id: string; name: string } | null> | void;
  /** DRP-FR-12: a deleted draft's tab closes with it. */
  onDraftDeleted: (id: string) => void;
  /**
   * DRP-FR-11 / NAW-FR-04: a draft renamed here renames its open tab too. The
   * tab strip's label follows the draft's name, and which surface the rename
   * was made from is not something the strip knows about.
   */
  onDraftRenamed: (id: string, name: string) => void;
  /**
   * DRP-FR-05 / DRP-FR-18: this panel moved a draft's record, so every other
   * surface showing that draft has to hear about it.
   */
  onDraftChanged: () => void;
  /**
   * Bumped by the shell on every `"drafts changed"` event, so the tree reflects
   * work done anywhere without the author refreshing anything (DRP-FR-05).
   */
  revision: number;
  /**
   * DRP-FR-34: a pending reveal-and-select, naming a draft by its **id**, or
   * null. Reached from the active tab being followed (SNV-FR-64 / SNV-FR-66).
   *
   * The id rather than the name or the folder, so a draft renamed or moved since
   * the caller last saw it is still the row that is reached; and a nonce rather
   * than a bare id, so two consecutive requests for the same draft both run.
   */
  reveal?: PanelRevealRequest | null;
  /**
   * DRP-FR-35 / GRU-FR-MYFA: go to the graduation run a row names. This panel
   * routes to it and renders none of it.
   */
  onOpenRun?: (runId: string) => void;
  /**
   * DRP-FR-YYZU: open the start dialog of `GSD-graduation-start-dialog.md` for
   * a GitHub-shadow draft. This panel invokes nothing itself for it.
   */
  onGraduateDraft?: (draftId: string, draftName: string) => void;
}

export function DraftsPanel({
  panel,
  onOpenDraft,
  onCreateDraft,
  onDraftDeleted,
  onDraftRenamed,
  onDraftChanged,
  revision,
  reveal,
  onOpenRun,
  onGraduateDraft,
}: DraftsPanelProps) {
  const [hierarchy, setHierarchy] = useState<DraftHierarchy | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [overlay, setOverlay] = useState<Overlay | null>(null);
  const [inline, setInline] = useState<Inline | null>(null);
  const [inlineText, setInlineText] = useState("");
  /** DRP-FR-31: why the value typed into the open field could not be taken. */
  const [inlineError, setInlineError] = useState<string | null>(null);
  /** DRP-FR-31: a typed backend error, against the row that initiated it. */
  const [rowErrors, setRowErrors] = useState<Map<string, string>>(new Map());
  /** DRP-FR-31: one gesture produces one call. */
  const [busy, setBusy] = useState<Set<string>>(new Set());
  /** DRP-FR-30: what stays selected across a mutation. */
  const [selected, setSelected] = useState<string | null>(null);
  /** DRP-FR-22: where the open menu hangs from, in viewport coordinates. */
  const [anchor, setAnchor] = useState<MenuAnchor | null>(null);
  /**
   * DRP-FR-28: the control that opened the current chain of floating surfaces,
   * so dismissing a confirmation or the picker lands focus back on the row it
   * was opened from rather than at the top of the document. Held across the
   * menu → dialog step, because the menu entry that opened the dialog is gone
   * by the time the dialog mounts.
   */
  const menuOpener = useRef<HTMLElement | null>(null);
  /** DRP-FR-27: what is being dragged, and the target under the pointer. */
  const [dragging, setDragging] = useState<TreeItem | null>(null);
  const [dropTarget, setDropTarget] = useState<string | null>(null);
  /** DRP-FR-13: which drafts the entered text was found in, and why. */
  const [matches, setMatches] = useState<DraftMatches>(null);
  /** DRP-FR-28: what the tree announces after a mutation. */
  const [announcement, setAnnouncement] = useState("");

  const inlineRef = useRef<HTMLInputElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);
  /** DRP-FR-30: the row to put focus on once the next render has landed. */
  const focusNext = useRef<string | null>(null);
  /** DRP-FR-31: the in-flight set, read synchronously — see `run`. */
  const inFlight = useRef<Set<string>>(new Set());

  const reload = useCallback(
    () =>
      api
        .listDrafts()
        .then((next) => {
          setHierarchy(next);
          setError(null);
          return next;
        })
        .catch((e) => {
          // Deliberately NOT an empty hierarchy: an empty one is the first-class
          // "this worktree has nothing yet" state (DRP-FR-15), and rendering it
          // for a failed read would present a backend fault as a confirmed-empty
          // worktree. `null` keeps the tree unrendered and shows the error alone.
          setHierarchy(null);
          setError(String(e));
          return null;
        }),
    [],
  );

  // DRP-FR-02 / DRP-FR-05: read on mount and whenever the shell reports that the
  // hierarchy moved.
  useEffect(() => {
    void reload();
  }, [reload, revision]);

  useEffect(() => {
    if (!inline) return;
    const field = inlineRef.current;
    field?.focus();
    // DRP-FR-36: the seeded name arrives selected, so the author names the
    // draft by typing over it rather than by clearing it first. The same field
    // behaves the same way when **Rename…** opens it (DRP-FR-11) — there is one
    // rename behaviour rather than a second one for new drafts.
    field?.select();
  }, [inline]);

  // DRP-FR-30: focus follows an item to its new place, after the tree that holds
  // it has rendered.
  useEffect(() => {
    if (!focusNext.current) return;
    const key = focusNext.current;
    focusNext.current = null;
    const row = rootRef.current?.querySelector<HTMLElement>(
      `[data-row-key="${CSS.escape(key)}"]`,
    );
    row?.focus();
  }, [hierarchy]);

  /**
   * DRP-FR-13: run the text filter against the drafts' names *and* their file
   * contents, so a draft is found by a phrase written inside it and not only by
   * what it was called.
   *
   * Dispatched a short rest after the last keystroke rather than on each one:
   * this is the panel's one call that opens a draft's files. An empty field
   * clears the result rather than searching for nothing.
   */
  useEffect(() => {
    const query = panel.text.trim();
    if (query === "") {
      setMatches(null);
      return;
    }
    let cancelled = false;
    const timer = setTimeout(() => {
      void api
        .searchDrafts(query)
        .then((hits) => {
          if (!cancelled) setMatches(toMatchMap(hits));
        })
        .catch((e) => {
          if (cancelled) return;
          // The tree stays as it is rather than emptying: a failed search is not
          // evidence that nothing matches.
          setError(String(e));
        });
    }, FILTER_DEBOUNCE_MS);
    return () => {
      cancelled = true;
      clearTimeout(timer);
    };
  }, [panel.text, revision]);

  const closeInline = useCallback(() => {
    setInline(null);
    setInlineError(null);
  }, []);

  // DRP-FR-16: Escape and an outside click dismiss whichever floating surface is
  // open, invoking nothing.
  useEffect(() => {
    if (!overlay) return;
    const dismiss = () => setOverlay(null);
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") dismiss();
    };
    // Bound on `mousedown` rather than `click` so a press that begins outside
    // dismisses before it can activate anything.
    const onDown = (e: MouseEvent) => {
      const target = e.target as Node | null;
      if (
        target &&
        (target as Element).closest?.(".drafts-overlay, .drafts-tree__menu-btn")
      )
        return;
      dismiss();
    };
    window.addEventListener("keydown", onKey);
    window.addEventListener("mousedown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey);
      window.removeEventListener("mousedown", onDown);
    };
  }, [overlay]);

  /** DRP-FR-16: opening a floating surface closes any open inline field. */
  const openOverlay = (
    next: Overlay | null,
    at?: { anchor: MenuAnchor; row: HTMLElement },
  ) => {
    closeInline();
    if (at) {
      menuOpener.current = at.row;
      setAnchor(at.anchor);
    }
    setOverlay(next);
  };

  /**
   * DRP-FR-22 / DRP-FR-28: the menu of the row the gesture landed on, opened
   * where the pointer is — and reachable without one.
   *
   * A right-click is the pointer route (the Project panel's own, per
   * `LCM-library-context-menu.md`); the Menu key and Shift+F10 are the keyboard
   * route, which a right-click alone would leave without any equivalent.
   */
  const openRowMenu = (
    e: React.MouseEvent | React.KeyboardEvent,
    next: Overlay,
    row: HTMLElement,
  ) => {
    e.preventDefault();
    e.stopPropagation();
    const anchor =
      "clientX" in e && (e.clientX !== 0 || e.clientY !== 0)
        ? { x: e.clientX, y: e.clientY }
        : anchorOfRow(row);
    openOverlay(next, { anchor, row });
  };

  /** Whether a key press is a request for the focused row's context menu. */
  const isMenuKey = (e: React.KeyboardEvent) =>
    e.key === "ContextMenu" || (e.shiftKey && e.key === "F10");

  /** DRP-FR-16: opening an inline field closes any open floating surface. */
  const openInline = (next: Inline, seed: string) => {
    setOverlay(null);
    setInline(next);
    setInlineText(seed);
    setInlineError(null);
  };

  const setRowError = (key: string, message: string | null) =>
    setRowErrors((current) => {
      const next = new Map(current);
      if (message === null) next.delete(key);
      else next.set(key, message);
      return next;
    });

  /**
   * DRP-FR-31: hold the item while its operation is in flight, so a second
   * create, rename, delete, or move on it is not accepted until the first has
   * returned — and DRP-FR-32: re-list whichever way it went, so the tree shows
   * what the backend reports rather than what the panel attempted.
   */
  const run = async (key: string, work: () => Promise<void>) => {
    // Guarded on a ref rather than on `busy`: two gestures dispatched in the
    // same synchronous batch — a double drop, a double-click through a menu —
    // both read the same render's state and both would pass a state-based
    // check, which is exactly the second invocation DRP-FR-31 forbids. The
    // state below is only what disables the affordances.
    if (inFlight.current.has(key)) return false;
    inFlight.current.add(key);
    setBusy((current) => new Set(current).add(key));
    setRowError(key, null);
    let ok = true;
    try {
      await work();
    } catch (e) {
      setRowError(key, String(e));
      ok = false;
    } finally {
      inFlight.current.delete(key);
      setBusy((current) => {
        const next = new Set(current);
        next.delete(key);
        return next;
      });
      await reload();
    }
    return ok;
  };

  // -------------------------------------------------------------------------
  // The tree (DRP-FR-20, DRP-FR-21, DRP-FR-13)
  // -------------------------------------------------------------------------

  const folders = useMemo(() => hierarchy?.folders ?? [], [hierarchy]);
  const drafts = useMemo(() => hierarchy?.drafts ?? [], [hierarchy]);

  /** DRP-FR-21: folders ordered case-insensitively by name, within each parent. */
  const childFolders = useMemo(() => {
    const map = new Map<string, DraftFolder[]>();
    for (const folder of folders) {
      const bucket = map.get(folder.parent) ?? [];
      bucket.push(folder);
      map.set(folder.parent, bucket);
    }
    for (const bucket of map.values())
      bucket.sort((a, b) =>
        folderName(a.path).toLowerCase().localeCompare(folderName(b.path).toLowerCase()),
      );
    return map;
  }, [folders]);

  /**
   * DRP-FR-21: drafts most-recent-activity first — the order `list_drafts`
   * returns them in, preserved rather than re-sorted.
   */
  const draftsByFolder = useMemo(() => {
    const map = new Map<string, DraftSummary[]>();
    for (const draft of drafts) {
      const key = draft.folder ?? ROOT;
      const bucket = map.get(key) ?? [];
      bucket.push(draft);
      map.set(key, bucket);
    }
    return map;
  }, [drafts]);

  const query = panel.text.trim().toLowerCase();

  /**
   * DRP-FR-07 / DRP-FR-13: the text filter narrows whatever the status filter
   * already admits, rather than replacing it.
   */
  const admitsDraft = useCallback(
    (draft: DraftSummary) =>
      // DRP-FR-07: the **graduated** position admits both `graduated` and
      // `published`. Every other single-status position admits its own status
      // alone, and **all drafts** admits everything.
      // DRP-FR-YVXS: the **active** position admits a GitHub-shadow draft
      // whose status is `github_shadow`; one that reports `graduated` falls in
      // the graduated position by its own status.
      (panel.filter === "all" ||
        draft.status === panel.filter ||
        (panel.filter === "graduated" && draft.status === "published") ||
        (panel.filter === "active" && draft.status === "github_shadow")) &&
      (matches === null || matches.has(draft.id)),
    [panel.filter, matches],
  );

  const revealId = reveal?.id ?? null;
  const revealNonce = reveal?.nonce;

  /**
   * DRP-FR-34: the revealed row, scrolled into view once it has rendered — and
   * focused too when the reveal was one the author asked for directly.
   */
  const revealTarget = useRef<{ key: string; focus: boolean } | null>(null);

  /**
   * DRP-FR-34: reveal and select the draft another surface named by id —
   * expanding every ancestor folder, relaxing whichever filter would hide the
   * row, and making it the selection and the focused row.
   *
   * Waits for what it needs rather than acting on a half-loaded panel: the
   * listing has to be in, and a text filter has to have settled, because only
   * those say whether a filter is actually hiding the row — and DRP-FR-34 is
   * explicit that only the filters that *would* hide it may be touched.
   *
   * Keyed on the nonce, not the id: a request naming the draft the last one
   * named still has to run (SNV-FR-68), and a re-list must not re-assert a
   * finished reveal over what the author has since selected (DRP-FR-14).
   */
  useEffect(() => {
    if (!revealId || revealNonce === undefined) return;
    // Shared rather than a local ref: `VPanel` unmounts this panel on every
    // surface switch, and a local guard would let a finished reveal re-apply —
    // re-relaxing filters the author has since re-tightened (SNV-FR-68).
    if (revealAlreadyConsumed(revealNonce)) return;
    // The listing is still in flight; the request stands until it lands.
    if (!hierarchy) return;

    const draft = drafts.find((d) => d.id === revealId);
    if (!draft) {
      // DRP-FR-34 / SNV-FR-67: an id the listed tree does not hold — a draft
      // deleted, graduated, or belonging to another worktree — reveals nothing
      // and reports nothing. The guard is claimed so a later re-list does not
      // keep re-testing a draft that is not coming back, and the tree, the
      // filters and the selection are left exactly as they are.
      markRevealConsumed(revealNonce);
      revealTarget.current = null;
      return;
    }

    // A text filter whose result has not arrived cannot yet say whether it is
    // hiding this row. Waiting costs one more render; guessing would clear a
    // filter that was admitting the draft all along.
    if (panel.text.trim() !== "" && matches === null) return;

    markRevealConsumed(revealNonce);

    // DRP-FR-34: expand every ancestor folder. Recorded like the author's own
    // expansion (DRP-FR-14), unlike the revelation a text filter forces.
    const folder = draft.folder ?? ROOT;
    if (folder !== ROOT) panel.expandAll([...ancestorsOf(folder), folder]);

    // DRP-FR-34: relax only what would actually hide the row. The status filter
    // moves to **all drafts** rather than to the position naming this draft's
    // own status, so nothing else leaves the tree to make room for it.
    if (panel.filter !== "all" && draft.status !== panel.filter)
      panel.setFilter("all");
    if (matches !== null && !matches.has(draft.id)) panel.setText("");

    const key = `draft:${draft.id}`;
    setSelected(key);
    // DRP-FR-34 / DRP-FR-30: a reveal the author asked for directly lands focus
    // on the row. A tab-follow does not: their focus intent is the tab they just
    // activated, and taking it would make their next keystroke move a panel
    // selection instead of doing anything in the tab (SNV-FR-64).
    revealTarget.current = { key, focus: reveal?.focus ?? false };
  }, [revealNonce, revealId, hierarchy, drafts, matches, panel, reveal?.focus]);

  /**
   * DRP-FR-30: land focus on the revealed row once it has actually rendered,
   * which is a render or two after the reveal — the expansion that brings it
   * into the tree is state the effect above only just set.
   *
   * Deliberately un-keyed: it is a ref check that returns immediately unless a
   * reveal is outstanding, and the alternative is enumerating every piece of
   * state that could bring the row into existence.
   */
  useEffect(() => {
    const target = revealTarget.current;
    if (!target) return;
    const row = rootRef.current?.querySelector<HTMLElement>(
      `[data-row-key="${CSS.escape(target.key)}"]`,
    );
    if (!row) return;
    revealTarget.current = null;
    // DRP-FR-34: the reveal ends with the row in view. `focus()` scrolls as a
    // side effect, but only when it is called — a followed row takes no focus,
    // so the scroll has to be asked for in its own right.
    row.scrollIntoView?.({ block: "nearest" });
    if (target.focus) row.focus();
  });

  /**
   * DRP-FR-13 / LIB-FR-09: with a text filter in force a folder renders when its
   * own name matches or its subtree holds an admitted draft; a folder admitted
   * on neither count is hidden. The **status** filter never hides a folder
   * (DRP-FR-07) — a folder left with no admitted draft still renders, carrying
   * the line of DRP-FR-29.
   */
  const visibleFolders = useMemo(() => {
    if (query === "") return null; // every folder renders
    const visible = new Set<string>();
    const reveal = (path: string) => {
      visible.add(path);
      for (const ancestor of ancestorsOf(path)) visible.add(ancestor);
    };
    for (const folder of folders)
      if (folderName(folder.path).toLowerCase().includes(query)) reveal(folder.path);
    for (const draft of drafts)
      if (admitsDraft(draft) && draft.folder) reveal(draft.folder);
    return visible;
  }, [query, folders, drafts, admitsDraft]);

  const folderVisible = (path: string) =>
    visibleFolders === null || visibleFolders.has(path);

  /**
   * DRP-FR-13: revelation the filter forced is not the author's own and is not
   * persisted — clearing the filter returns every folder to the expansion state
   * they left it in.
   */
  const forcedOpen = useMemo(() => {
    if (visibleFolders === null) return null;
    const open = new Set<string>();
    for (const path of visibleFolders)
      for (const ancestor of ancestorsOf(path)) open.add(ancestor);
    for (const draft of drafts)
      if (admitsDraft(draft) && draft.folder) {
        open.add(draft.folder);
        for (const ancestor of ancestorsOf(draft.folder)) open.add(ancestor);
      }
    return open;
  }, [visibleFolders, drafts, admitsDraft]);

  const isExpanded = (path: string) =>
    panel.expanded.has(path) || (forcedOpen?.has(path) ?? false);

  /**
   * DRP-FR-13: the author's own expansion, kept separate from the filter's.
   *
   * A folder the filter forced open is not in `panel.expanded`, so a bare
   * `toggleExpanded` on it would *add* it — leaving it expanded (the filter is
   * still forcing it) and silently persisting an expansion the author was trying
   * to undo, which would still be there when the filter cleared. Collapsing a
   * force-revealed folder therefore only removes the author's own record of it,
   * and the filter goes on showing what it has to show until it is cleared.
   */
  const toggleFolder = (path: string) => {
    if (forcedOpen?.has(path) && !panel.expanded.has(path)) return;
    panel.toggleExpanded(path);
  };

  /**
   * DRP-FR-15: a worktree holding neither a draft nor a folder. A worktree
   * holding folders but no draft is *not* that state.
   */
  const nothingAtAll =
    !error && hierarchy !== null && folders.length === 0 && drafts.length === 0;

  /**
   * DRP-FR-15 / SNV-FR-61: a filter that matches nothing is a different state
   * from an empty worktree, and calls for a different action.
   *
   * It requires that a filter actually be in force *and* that the tree it
   * renders be empty — which, because the status filter never hides a folder
   * (DRP-FR-07), means no top-level folder survived the text filter either. A
   * status filter admitting no draft while folders remain is therefore not this
   * state: the folders render, each carrying the line of DRP-FR-29.
   */
  const filtering = query !== "" || panel.filter !== "all";
  const filteredToNothing =
    !nothingAtAll &&
    hierarchy !== null &&
    filtering &&
    (folders.length > 0 || drafts.length > 0) &&
    !drafts.some(admitsDraft) &&
    (childFolders.get(ROOT) ?? []).every((f) => !folderVisible(f.path));

  // -------------------------------------------------------------------------
  // Drag and drop (DRP-FR-27) and the picker (DRP-FR-28)
  // -------------------------------------------------------------------------

  /**
   * DRP-FR-27: why a drop onto `destination` is refused, or `null` when it is a
   * valid target. The same rules decide which entries the **Move to Folder…**
   * picker marks unavailable, so the two paths cannot disagree.
   */
  const moveRefusal = useCallback(
    (item: TreeItem, destination: string): string | null => {
      if (item.kind === "draft") {
        if ((item.folder ?? ROOT) === destination) return "Already in this folder.";
        // A draft never collides: it is identified in the tree by its id rather
        // than by its name (DRS-FR-30).
        return null;
      }
      if (destination === item.path) return "A folder cannot hold itself.";
      if (isWithin(destination, item.path))
        return "A folder cannot move inside its own subtree.";
      if (folderParent(item.path) === destination) return "Already in this folder.";
      const name = folderName(item.path).toLowerCase();
      const clash = (childFolders.get(destination) ?? []).some(
        (f) => folderName(f.path).toLowerCase() === name,
      );
      return clash ? "A folder of that name is already there." : null;
    },
    [childFolders],
  );

  /**
   * DRP-FR-27: the drop behaviour of anything that can be a target.
   *
   * A **draft row** carries these for the folder it sits in rather than
   * refusing outright: the events would otherwise bubble to the panel body and
   * be read as a drop on the implicit root, so releasing an item over a draft
   * two levels down would silently file it at the top of the tree. Aiming at a
   * row means aiming at where that row sits.
   */
  const dropHandlers = (destination: string) => ({
    onDragOver: (e: React.DragEvent) => {
      if (!dragging) return;
      e.stopPropagation();
      // A target the move would be refused on is never highlighted, and
      // `preventDefault` is what makes a drop possible at all — withholding it
      // is the refusal.
      if (moveRefusal(dragging, destination) !== null) {
        setDropTarget(null);
        return;
      }
      e.preventDefault();
      if (e.dataTransfer) e.dataTransfer.dropEffect = "move";
      setDropTarget(destination);
    },
    onDragLeave: (e: React.DragEvent) => {
      e.stopPropagation();
      setDropTarget((current) => (current === destination ? null : current));
    },
    onDrop: (e: React.DragEvent) => {
      e.stopPropagation();
      e.preventDefault();
      const item = dragging;
      setDragging(null);
      setDropTarget(null);
      // The panel commits on release over a valid target and at no earlier
      // moment.
      if (item && moveRefusal(item, destination) === null)
        void commitMove(item, destination);
    },
  });

  const commitMove = async (item: TreeItem, destination: string) => {
    const key = itemKey(item);
    const label = item.kind === "draft" ? item.name : folderName(item.path);
    const ok = await run(key, async () => {
      if (item.kind === "draft") await api.moveDraftToFolder(item.id, destination);
      else await api.moveDraftsFolder(item.path, destination);
    });
    if (!ok) return;
    const arrived =
      item.kind === "folder"
        ? folderJoin(destination, folderName(item.path))
        : destination;
    const moved: TreeItem =
      item.kind === "draft"
        ? { ...item, folder: destination }
        : { kind: "folder", path: arrived };
    // DRP-FR-30: a moved folder arrives expanded if it was expanded, and the
    // folders around it keep their own state. Expansion is keyed by path, so
    // without this remap the folder and its whole expanded subtree would
    // collapse the moment they changed path.
    if (item.kind === "folder") panel.reparentExpanded(item.path, arrived);
    // DRP-FR-30: focus follows the item, which it can only do if the row is
    // rendered — so the destination and every ancestor of it are revealed
    // rather than leaving the item somewhere collapsed and out of sight.
    panel.expandAll([destination, ...ancestorsOf(destination)]);
    setSelected(itemKey(moved));
    focusNext.current = itemKey(moved);
    setAnnouncement(
      `${label} moved to ${destination === ROOT ? "Drafts" : destination}.`,
    );
    onDraftChanged();
  };

  // -------------------------------------------------------------------------
  // Operations
  // -------------------------------------------------------------------------

  // DRP-FR-06 / DRP-FR-09 .. DRP-FR-13 / DRP-FR-25 / DRP-FR-32: what a menu
  // entry or an inline field actually does — see `./draftsActions`.
  const {
    doOpenDraft,
    doDeleteDraft,
    toggleArchived,
    commitInline,
    doDeleteFolder,
    doNewFolder,
    doNewDraft,
  } = createDraftActions({
    panel,
    drafts,
    childFolders,
    inline,
    inlineText,
    openInline,
    closeInline,
    setInlineError,
    setOverlay,
    setSelected,
    setAnnouncement,
    focusNext,
    run,
    reload,
    onOpenDraft,
    onCreateDraft,
    onDraftDeleted,
    onDraftRenamed,
    onDraftChanged,
  });

  /**
   * DRP-FR-28: ArrowUp and ArrowDown walk the rows in the order they render,
   * which is what a screen reader's tree mode expects. Tab reaches every row
   * too, but it costs two stops per row and does not follow the tree's shape.
   */
  const moveFocusBy = (from: HTMLElement, step: 1 | -1) => {
    const rows = Array.from(
      rootRef.current?.querySelectorAll<HTMLElement>("[data-row-key]") ?? [],
    );
    const at = rows.indexOf(from);
    const next = rows[at + step];
    if (at !== -1 && next) next.focus();
  };

  /**
   * DRP-FR-YYZU: open the issue a GitHub-shadow draft mirrors. The one
   * publication operation this panel invokes; a refusal renders on the row.
   */
  const openShadowIssue = (draft: DraftSummary) => {
    const url = draft.githubIssue?.issueUrl;
    if (!url) return;
    setOverlay(null);
    const key = `draft:${draft.id}`;
    setRowError(key, null);
    api.openPublicationIssue(draft.id, url).catch((e) => {
      logWarn(["frontend"], "shadow draft issue could not be opened", {
        draftId: draft.id,
        code: refusalCode(e),
      });
      setRowError(key, githubPollingErrorMessage(e));
    });
  };

  // DRP-FR-09 / DRP-FR-20: how one row is drawn, given everything the panel
  // holds. The context is the panel itself — see `./draftsRows`.
  const { menuEntry, inlineField, renderDraft, renderFolder } =
    makeRowRenderers({
      panel,
      childFolders,
      draftsByFolder,
      matches,
      admitsDraft,
      folderVisible,
      isExpanded,
      toggleFolder,
      selected,
      setSelected,
      busy,
      rowErrors,
      overlay,
      setOverlay,
      openOverlay,
      openRowMenu,
      isMenuKey,
      anchor,
      inline,
      inlineText,
      setInlineText,
      inlineError,
      setInlineError,
      inlineRef,
      openInline,
      closeInline,
      commitInline,
      dragging,
      setDragging,
      dropTarget,
      setDropTarget,
      moveRefusal,
      dropHandlers,
      doOpenDraft,
      doNewDraft,
      doNewFolder,
      toggleArchived,
      moveFocusBy,
      onOpenRun,
      onGraduateDraft,
      openShadowIssue,
    });

  const rootFolders = (childFolders.get(ROOT) ?? []).filter((f) =>
    folderVisible(f.path),
  );
  const rootDrafts = (draftsByFolder.get(ROOT) ?? []).filter(admitsDraft);
  const rootRefusal = dragging ? moveRefusal(dragging, ROOT) : null;
  const rootIsTarget = dropTarget === ROOT && rootRefusal === null;

  /** DRP-FR-25: what the confirmation counts, so the author knows the size of
   *  what is about to move. */
  const deleteTarget =
    overlay?.kind === "folder-delete"
      ? {
          path: overlay.path,
          drafts: drafts.filter((d) => (d.folder ?? ROOT) === overlay.path).length,
          folders: (childFolders.get(overlay.path) ?? []).length,
          parent: folderParent(overlay.path),
        }
      : null;

  const draftDeleteTarget =
    overlay?.kind === "draft-delete"
      ? (drafts.find((d) => d.id === overlay.id) ?? null)
      : null;

  /**
   * DRP-FR-KDVX: the row **Information** was activated on.
   *
   * Found by id rather than captured with the menu, so a row that re-listed
   * under the open modal still names the same draft.
   */
  const draftInformationTarget =
    overlay?.kind === "draft-information"
      ? (drafts.find((d) => d.id === overlay.id) ?? null)
      : null;

  return (
    <DraftsPanelView
      panel={panel}
      hierarchy={hierarchy}
      error={error}
      folders={folders}
      rootFolders={rootFolders}
      rootDrafts={rootDrafts}
      rootIsTarget={rootIsTarget}
      rootRef={rootRef}
      nothingAtAll={nothingAtAll}
      filteredToNothing={filteredToNothing}
      announcement={announcement}
      overlay={overlay}
      setOverlay={setOverlay}
      anchor={anchor}
      menuOpener={menuOpener}
      openRowMenu={openRowMenu}
      inline={inline}
      closeInline={closeInline}
      dragging={dragging}
      setDragging={setDragging}
      setDropTarget={setDropTarget}
      moveRefusal={moveRefusal}
      commitMove={commitMove}
      deleteTarget={deleteTarget}
      draftDeleteTarget={draftDeleteTarget}
      draftInformationTarget={draftInformationTarget}
      doDeleteDraft={doDeleteDraft}
      doDeleteFolder={doDeleteFolder}
      doNewDraft={doNewDraft}
      doNewFolder={doNewFolder}
      menuEntry={menuEntry}
      inlineField={inlineField}
      renderDraft={renderDraft}
      renderFolder={renderFolder}
    />
  );
}

/**
 * DRP-FR-28: the non-pointer counterpart of a drop — the implicit root and every
 * folder, operated entirely from the keyboard, confirming into the same
 * operations a drop invokes.
 */

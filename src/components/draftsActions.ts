/**
 * What the Drafts panel's menu entries and inline fields actually do: open,
 * rename, delete, archive, and create (DRP-FR-06, DRP-FR-09 .. DRP-FR-13,
 * DRP-FR-25, DRP-FR-26, DRP-FR-32).
 *
 * Every one of them ends the same way — the mutation, then a re-list — because
 * nothing here is rendered from what the panel *attempted* (DRP-FR-32). Reading
 * them together is what makes that shape visible, and what stops a new action
 * being written without it.
 *
 * Plain closures rather than a hook: the tree and the row state stay owned by
 * the panel.
 */
import type { MutableRefObject } from "react";

import * as api from "../api";
import {
  ancestorsOf,
  folderJoin,
  folderName,
  folderNameProblem,
  folderParent,
  itemKey,
  ROOT,
} from "./draftsTree";
import type { Inline, Overlay } from "./draftsRows";
import type { DraftsPanel as DraftsPanelState } from "../hooks/useDraftsPanelState";
import type { DraftFolder, DraftHierarchy, DraftSummary } from "../types";

export interface DraftActionDeps {
  panel: DraftsPanelState;
  drafts: DraftSummary[];
  childFolders: Map<string, DraftFolder[]>;
  inline: Inline | null;
  inlineText: string;
  openInline: (next: Inline, seed: string) => void;
  closeInline: () => void;
  setInlineError: (next: string | null) => void;
  setOverlay: (next: Overlay | null) => void;
  setSelected: (key: string | null) => void;
  setAnnouncement: (message: string) => void;
  /** DRP-FR-30: the row focus lands on once the tree holding it has rendered. */
  focusNext: MutableRefObject<string | null>;
  run: (key: string, work: () => Promise<void>) => Promise<boolean>;
  reload: () => Promise<DraftHierarchy | null>;
  /** DRP-FR-06: the created draft, when the caller resolves one. */
  onOpenDraft: (draft: DraftSummary) => void;
  onCreateDraft: (
    folder?: string,
  ) => Promise<{ id: string; name: string } | null> | void;
  onDraftDeleted: (id: string) => void;
  onDraftRenamed: (id: string, name: string) => void;
  onDraftChanged: () => void;
}

export function createDraftActions(deps: DraftActionDeps) {
  const {
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
  } = deps;

  const doOpenDraft = (draft: DraftSummary) => {
    setSelected(itemKey({ kind: "draft", ...draft, folder: draft.folder ?? ROOT }));
    onOpenDraft(draft);
  };

  const doDeleteDraft = async (draft: DraftSummary) => {
    setOverlay(null);
    const ok = await run(`draft:${draft.id}`, async () => {
      await api.deleteDraft(draft.id);
      onDraftDeleted(draft.id);
    });
    if (ok) setAnnouncement(`${draft.name} deleted.`);
  };

  /**
   * DRP-FR-18: the archive entry moves the draft to the other position. No
   * confirmation, because it destroys nothing and reverses in one action from
   * the same menu — and it moves nothing between folders: the row stays where it
   * sits, gaining or losing the archived marker.
   */
  const toggleArchived = async (draft: DraftSummary) => {
    setOverlay(null);
    const ok = await run(`draft:${draft.id}`, async () => {
      await api.setDraftStatus(
        draft.id,
        draft.status === "archived" ? "active" : "archived",
      );
      onDraftChanged();
    });
    if (ok)
      setAnnouncement(
        `${draft.name} ${draft.status === "archived" ? "restored" : "archived"}.`,
      );
  };

  const commitInline = async () => {
    if (!inline) return;
    const value = inlineText.trim();

    if (inline.kind === "rename-draft") {
      const draft = drafts.find((d) => d.id === inline.id);
      // DRP-FR-11: an empty name leaves the stored name untouched and invokes
      // nothing, so it is abandoned here rather than sent to be refused.
      if (!draft || value === "") {
        closeInline();
        return;
      }
      try {
        const updated = await api.renameDraft(inline.id, value);
        closeInline();
        onDraftRenamed(inline.id, updated.name);
        setSelected(`draft:${inline.id}`);
        await reload();
      } catch (e) {
        // DRP-FR-11 / NAW-FR-26: refused inline against the field, leaving the
        // row as it was.
        setInlineError(String(e));
      }
      return;
    }

    // DRP-FR-23 / DRP-FR-24: validated in the panel before any call, so a
    // malformed name costs no round trip.
    const problem = folderNameProblem(value);
    if (problem) {
      setInlineError(problem);
      return;
    }

    if (inline.kind === "rename-folder") {
      if (value === folderName(inline.path)) {
        setInlineError("That is already the folder's name.");
        return;
      }
      try {
        const renamed = await api.renameDraftsFolder(inline.path, value);
        closeInline();
        // DRP-FR-24: a rename moves nothing — the folder keeps its parent, its
        // subtree and its expansion. Expansion is keyed by path, so it has to
        // be carried across explicitly or the folder collapses on being renamed.
        panel.reparentExpanded(inline.path, renamed.path);
        setSelected(`folder:${renamed.path}`);
        focusNext.current = `folder:${renamed.path}`;
        setAnnouncement(`Folder renamed to ${value}.`);
        await reload();
      } catch (e) {
        setInlineError(String(e));
      }
      return;
    }

    try {
      const created = await api.createDraftsFolder(inline.parent, value);
      closeInline();
      // DRP-FR-23: a committed folder is revealed in the tree, expanded and
      // empty, and becomes the selection.
      panel.expandAll([created.path]);
      setSelected(`folder:${created.path}`);
      focusNext.current = `folder:${created.path}`;
      setAnnouncement(`Folder ${value} created.`);
      await reload();
    } catch (e) {
      setInlineError(String(e));
    }
  };

  const doDeleteFolder = async (path: string) => {
    setOverlay(null);
    const childDrafts = drafts.filter((d) => (d.folder ?? ROOT) === path);
    const childDirs = childFolders.get(path) ?? [];
    const parent = folderParent(path);
    const ok = await run(`folder:${path}`, async () => {
      await api.deleteDraftsFolder(path);
    });
    if (!ok) return;
    // DRP-FR-30 / DRP-FR-25: every direct child moved up to `parent`, so the
    // expansion of each — and of everything beneath it — has to move with it,
    // for the reason a rename's does: expansion is keyed by path.
    for (const child of childDirs)
      panel.reparentExpanded(
        child.path,
        folderJoin(parent, folderName(child.path)),
      );
    // DRP-FR-30: focus lands on the first of the reparented children in the
    // tree's order, or on the folder's parent — the root included — when it had
    // none.
    const first =
      childDirs.length > 0
        ? `folder:${folderJoin(parent, folderName(childDirs[0].path))}`
        : childDrafts.length > 0
          ? `draft:${childDrafts[0].id}`
          : parent === ROOT
            ? null
            : `folder:${parent}`;
    setSelected(first);
    focusNext.current = first;
    setAnnouncement(
      `Folder ${folderName(path)} deleted; its contents moved to ${parent === ROOT ? "Drafts" : parent}.`,
    );
    onDraftChanged();
  };

  const doNewFolder = (parent: string) => {
    // DRP-FR-23: expand the folder the field sits in, so the field is where the
    // folder will be.
    if (parent !== ROOT) panel.expandAll([parent]);
    openInline({ kind: "new-folder", parent }, "");
  };

  /**
   * DRP-FR-06 / DRP-FR-26 / DRP-FR-36: create a draft from either of this
   * panel's two routes and put its row straight into rename mode.
   *
   * The row is revealed — its folder expanded where it was collapsed, and
   * whichever filter would otherwise hide a brand-new active draft relaxed —
   * selected, and rendered with the inline name field of DRP-FR-11 already open,
   * seeded and selected, with keyboard focus in it. The tab the same act opened
   * is the active tab and does not take the keystrokes: the author names the
   * draft first and writes in it second.
   *
   * A create and a rename are two operations rather than one, so the draft
   * exists whether the field is committed, abandoned with Escape, or closed by
   * one of the panel's floating surfaces opening over it (DRP-FR-16).
   */
  const doNewDraft = async (folder: string) => {
    setOverlay(null);
    if (folder !== ROOT) panel.expandAll([...ancestorsOf(folder), folder]);
    const created = await onCreateDraft(folder === ROOT ? undefined : folder);
    // The shell reported the failure itself; there is no row to name.
    if (!created) return;
    // The listing the panel holds predates the creation, so the row the field
    // belongs to is not in it yet.
    await reload();
    // DRP-FR-34's relaxation, for the same reason: a new draft is `active`, so
    // only a filter that would hide an active draft is touched, and a text
    // filter cannot be admitting a name that did not exist when it was typed.
    if (panel.filter !== "all" && panel.filter !== "active") panel.setFilter("all");
    if (panel.text.trim() !== "") panel.setText("");
    setSelected(`draft:${created.id}`);
    openInline({ kind: "rename-draft", id: created.id }, created.name);
  };

  // -------------------------------------------------------------------------
  // Rendering
  // -------------------------------------------------------------------------

  return {
    doOpenDraft,
    doDeleteDraft,
    toggleArchived,
    commitInline,
    doDeleteFolder,
    doNewFolder,
    doNewDraft,
  };
}

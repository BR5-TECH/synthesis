/**
 * The New Artifact tab's lifecycle: opening a draft in one, creating a draft
 * that opens in one, following a rename, and dropping the tab when the draft
 * itself is gone (NAW-new-artifact.md / DRP-drafts-panel.md).
 *
 * Plain closures over the shell's state rather than a hook: the draft store and
 * the strip stay owned by `useShellSession`.
 */
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import * as api from "../../api";
import { logWarn } from "../../logging";
import type { DraftSessionStore } from "../../state/draftSessions";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import { clearDiscussionSession } from "../../state/discussionSession";
import { invalidateDraftDiscussions } from "../../state/draftDiscussionInvalidation";
import type { NewDraftSeed, Tab } from "../../types";
import { neverEmpty } from "./tabRecords";

export interface DraftTabActionDeps {
  drafts: DraftSessionStore;
  /** The live strip, read to find the conversation tabs of a deleted draft. */
  tabsRef: MutableRefObject<Tab[]>;
  setTabs: Dispatch<SetStateAction<Tab[]>>;
  setActiveTab: Dispatch<SetStateAction<string>>;
  activateTab: (id: string) => void;
  bumpDrafts: () => void;
  flashToast: (message: string) => void;
  /** SMD-FR-OYLC: the map session a draft created from the map is placed in. */
  specMap: SpecMapSessionStore;
}

/** SMD-FR-EKWN: what the author reads when the map's New draft fails. */
export const MAP_DRAFT_FAILED = "The draft could not be created.";

export interface DraftTabActions {
  openDraft: (draft: { id: string; name: string }) => void;
  createDraft: (
    seed?: NewDraftSeed,
    failureMessage?: string,
  ) => Promise<{ id: string; name: string } | null>;
  createDraftForMapNode: (nodeId: string) => Promise<void>;
  renameDraftTab: (draftId: string, name: string) => void;
  followDraftPrompt: (draftId: string) => Promise<void>;
  openDraftById: (draftId: string) => Promise<void>;
  dropDraftTab: (draftId: string) => void;
}

export function createDraftTabActions(
  deps: DraftTabActionDeps,
): DraftTabActions {
  const {
    drafts,
    tabsRef,
    setTabs,
    setActiveTab,
    activateTab,
    bumpDrafts,
    flashToast,
    specMap,
  } = deps;

  /**
   * NAW-FR-01 / NAW-FR-02 / TAB-FR-17: open a draft in a New Artifact tab, or
   * jump focus to the tab already showing it. A draft is not a file, so the tab
   * is keyed by the draft's own id — which is what lets a draft and an artifact
   * of the same name coexist in the strip.
   *
   * Opening it closes no overlay and is closed by none: a tab is not a floating
   * overlay (SNV-FR-56).
   */
  const openDraft = (draft: { id: string; name: string }) => {
    const id = `draft:${draft.id}`;
    setTabs((ts) =>
      ts.some((t) => t.id === id)
        ? ts
        : [...ts, { id, label: draft.name, kind: "draft", draftId: draft.id }],
    );
    activateTab(id);
  };

  /**
   * NAW-FR-25: carry a draft's session to wherever the rename just put its
   * prompt.
   *
   * Done here rather than in the New Artifact tab because the tab is mounted
   * only while it is the active one, and the session it would reconcile is
   * deliberately not: a draft renamed from the Drafts panel while its tab sits
   * in the background would otherwise keep a dirty buffer keyed to the old path,
   * and the next flush — a Save All, a project close, a return to the tab —
   * writes it back, recreating the file the rename moved away from.
   *
   * Only drafts this session has actually opened are reconciled: a draft with no
   * session holds no buffer to strand.
   */
  const followDraftPrompt = async (draftId: string) => {
    if (!drafts.get(draftId)) return;
    try {
      const record = await api.openDraft(draftId);
      drafts.followPrompt(draftId, record.promptPath ?? null);
    } catch {
      // A draft deleted or graduated between the rename and this read has no
      // session left to reconcile, and the tab is closing behind it.
    }
  };

  /**
   * NAW-FR-03 / DRP-FR-06 / DRP-FR-26: create a draft and open it in a New
   * Artifact tab, made the active tab.
   *
   * The **Drafts panel** is the only entry point. The File menu's and the
   * Project context menu's **New Artifact** items open the typed-artifact window
   * instead (NTA-FR-14, SNV-FR-24, LCM-FR-08), which creates a project file and
   * reaches nothing here.
   *
   * Resolves the created draft so the panel can put its row straight into
   * rename mode (DRP-FR-36), and `null` where the creation was refused — which
   * this reports itself, there being no row for the panel to name.
   */
  const createDraft = async (
    seed: NewDraftSeed = {},
    failureMessage?: string,
  ): Promise<{ id: string; name: string } | null> => {
    try {
      const created = await api.createDraft({
        // DRS-FR-07: where the draft is *filed*, and the only thing a creation
        // says about where anything goes. Where its specification lands is the
        // graduation agent's choice from the captured prompt (NAW-FR-19), so no
        // entry point seeds a destination. The Drafts panel's folder menu
        // supplies a folder; every other entry point supplies none and the
        // draft is created at the drafts root.
        folder: seed.folder ?? null,
      });
      // NAW-FR-06 / DRS-FR-06: a draft is created holding one Markdown file, and
      // the tab opens on it — so the selection is recorded before the workspace
      // mounts rather than being rediscovered from a tree it has yet to load.
      drafts.select(created.draft.id, created.file);
      // NAW-FR-25: where the draft's name is bound, from the moment it exists.
      drafts.setPromptPath(created.draft.id, created.draft.promptPath ?? created.file);
      bumpDrafts();
      openDraft(created.draft);
      return { id: created.draft.id, name: created.draft.name };
    } catch (e) {
      // DRS-FR-39: a malformed project-public store fails the creation with a
      // typed error rather than creating a draft without the starting content
      // the project asked for — one of the ways this lands, and one nothing else
      // would say anything about.
      logWarn(["frontend"], "draft creation failed", {
        folder: seed.folder ?? "",
        reason: String(e),
      });
      flashToast(failureMessage ?? String(e));
      return null;
    }
  };

  /**
   * SMD-FR-HVBE / SMD-FR-OYLC / SMD-FR-EKWN: the map's **New draft** runs the
   * Drafts panel's own creation at the drafts root (NAW-FR-03), and only a
   * draft that was created is placed on the node.
   */
  const createDraftForMapNode = async (nodeId: string): Promise<void> => {
    const created = await createDraft({}, MAP_DRAFT_FAILED);
    if (!created) return;
    specMap.dispatch({
      kind: "attachDraft",
      nodeId,
      draft: { draftId: created.id, name: created.name },
    });
  };

  /**
   * NAW-FR-04: the strip's label follows the draft's name.
   *
   * NAW-FR-25: and the draft moved, so the revision moves with it. A rename
   * carries the draft's **prompt**, which an open New Artifact tab has to
   * follow — the strip's label alone would leave that tab's action row on the
   * old filename and its auto-save writing to it. The backend's
   * `"drafts changed"` event says the same thing; this says it without waiting
   * for the round trip, and a second bump costs one small record read.
   */
  const renameDraftTab = (draftId: string, name: string) => {
    setTabs((ts) =>
      ts.map((t) => (t.draftId === draftId ? { ...t, label: name } : t)),
    );
    bumpDrafts();
    void followDraftPrompt(draftId);
  };

  /**
   * GRU-FR-RZDI: open a draft in a New Artifact tab by **id** alone, so a draft
   * renamed or moved since a run started is still the one that opens.
   *
   * The name is read from the record rather than supplied, which is the whole
   * point of the id route: a caller holding a run knows the draft's id and knows
   * whatever name it had when the run began, and the latter may be stale.
   */
  const openDraftById = async (draftId: string) => {
    try {
      const record = await api.openDraft(draftId);
      openDraft({ id: record.id, name: record.name });
    } catch (e) {
      flashToast(String(e));
    }
  };

  /**
   * NAW-FR-24 / DRP-FR-12: the draft is gone — graduated or deleted — so its tab
   * goes with it. Closed directly rather than through `closeTab`, which would
   * try to flush a draft whose files no longer exist.
   */
  const dropDraftTab = (draftId: string) => {
    let remaining: Tab[] = [];
    /**
     * CVP-FR-51: deleting a draft deletes its conversations outright, so every
     * conversation tab of one closes with it and its session state is dropped.
     * This is not the unavailable-owner state of CVP-FR-45: there is nothing
     * left to reach.
     */
    const ownsConversation = (t: Tab) =>
      t.kind === "conversation" &&
      t.ownerTarget?.kind === "draft" &&
      t.ownerTarget.draftId === draftId;
    for (const t of tabsRef.current.filter(ownsConversation)) {
      if (t.threadId) clearDiscussionSession(t.threadId);
    }
    setTabs((ts) => {
      remaining = neverEmpty(
        ts.filter((t) => t.draftId !== draftId && !ownsConversation(t)),
      );
      return remaining;
    });
    // SNV-FR-65: a removal, not an activation.
    setActiveTab((current) =>
      remaining.some((t) => t.id === current)
        ? current
        : remaining[remaining.length - 1].id,
    );
    // CMS-FR-39: the draft's discussions went with it, so what is held about
    // them goes too — the unsent text, the reading position, the cached threads.
    invalidateDraftDiscussions(draftId);
    // The draft is gone, so its retained selection and buffer go with it.
    drafts.drop(draftId);
    bumpDrafts();
  };

  return {
    openDraft,
    createDraft,
    createDraftForMapNode,
    renameDraftTab,
    followDraftPrompt,
    openDraftById,
    dropDraftTab,
  };
}

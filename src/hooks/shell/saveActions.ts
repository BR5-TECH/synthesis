/**
 * File → Save, Save All, and the two find accelerators (SNV-FR-28 .. SNV-FR-31,
 * SNV-FR-43): what each of them is enabled over, and what each of them does.
 *
 * Held together because they answer one question between them — what the active
 * tab owns. Save writes it, Save All writes everything like it, and Find is
 * scoped to the ones that present a searchable surface; a tab type added later
 * has to be reflected in all three or the menu and the accelerator disagree.
 *
 * Plain closures rather than a hook: the stores and the strip stay owned by
 * `useShellSession`, and so do the two effects that push the enablement to the
 * native menu.
 */
import { logDebug } from "../../logging";
import type { EditSessionStore, FlushResult } from "../../state/editSessions";
import type { FlowSessionStore } from "../../state/flowSessions";
import type { DraftSessionStore } from "../../state/draftSessions";
import { nextFindForm, type FindForm } from "../../state/findState";
import type { Tab } from "../../types";

export interface SaveActionDeps {
  /** The active tab, or undefined when the strip holds none by that id. */
  activeSaveTab: Tab | undefined;
  sessions: EditSessionStore;
  flows: FlowSessionStore;
  drafts: DraftSessionStore;
  focusBlocker: (artifactId: string) => void;
  focusDraft: (draftId: string) => void;
}

export interface SaveActions {
  saveEnabled: boolean;
  saveAllEnabled: boolean;
  findEnabled: boolean;
  requestSave: () => Promise<void>;
  requestSaveAll: () => Promise<void>;
  requestFind: (requested: FindForm) => void;
}

export function createSaveActions(deps: SaveActionDeps): SaveActions {
  const { activeSaveTab, sessions, flows, drafts, focusBlocker, focusDraft } =
    deps;

  // The content File → Save acts on: the active tab's, when that tab owns
  // something savable (SNV-FR-28). Every other tab type — Dashboard, Search
  // results, History detail, Global settings, Project settings — owns none, so
  // it has no artifact id and yields no target.
  const saveTarget = activeSaveTab?.artifactId
    ? { flow: activeSaveTab.kind === "flow", id: activeSaveTab.artifactId }
    : null;
  /**
   * NAW-FR-13: a New Artifact tab owns savable content too — its draft's files.
   * It carries `draftId` rather than `artifactId` (a draft is not a file), so it
   * needs its own target rather than falling into the branch above.
   */
  const draftSaveTarget = activeSaveTab?.draftId ?? null;

  /**
   * SNV-FR-28: Save is enabled only when the active tab holds unsaved changes to
   * content it owns — a dirty Editor tab (EDT-FR-04) or a dirty Flow tab
   * (FLO-FR-26). Everything else greys it out, including a clean editable tab.
   */
  const saveEnabled = saveTarget
    ? saveTarget.flow
      ? !!flows.get(saveTarget.id)?.dirty
      : !!sessions.get(saveTarget.id)?.dirty
    : draftSaveTarget
      ? drafts.isDirty(draftSaveTarget)
      : false;

  /**
   * SNV-FR-30: Save All is enabled whenever anything in the project is unsaved,
   * whichever tab happens to be active — and greyed out when nothing is.
   */
  const saveAllEnabled =
    sessions.dirtyIds().length > 0 ||
    flows.dirtyIds().length > 0 ||
    drafts.dirtyIds().length > 0;

  /**
   * SNV-FR-43: Find and Find & Replace are scoped to the tabs that present a
   * Markdown editing surface with the find panels above it — an Editor tab, and
   * a New Artifact tab showing its live prompt (NAW-FR-57). Both are greyed out
   * everywhere else, which is what makes ⌘F and ⌘R do nothing on a Dashboard or
   * a Flow tab rather than reaching the universal search bar (FLO-FR-13), and
   * what keeps them inert over a draft's past version or a draft its graduation
   * holds read-only (NAW-FR-09, NAW-FR-44, NAW-FR-59).
   *
   * The target names the store the panel's state hangs off as well as the key
   * within it, because a draft's prompt is edited in its own session store. Both
   * are `EditSessionStore`s carrying the same retained find state (EFR-FR-GBJT /
   * NAW-FR-12), so one routing serves both and neither gets a command path of
   * its own.
   */
  const findTarget: { store: EditSessionStore; id: string } | null = (() => {
    if (activeSaveTab?.kind === "editor" && activeSaveTab.artifactId) {
      return { store: sessions, id: activeSaveTab.artifactId };
    }
    if (activeSaveTab?.kind === "draft" && activeSaveTab.draftId) {
      // NAW-FR-57: the tab answers for its own draft — a live prompt that takes
      // edits names its session, and everything else names nothing.
      const id = drafts.searchTarget(activeSaveTab.draftId);
      return id === null ? null : { store: drafts.docs, id };
    }
    return null;
  })();
  const findEnabled = findTarget !== null;

  /**
   * SNV-FR-43 / EFR-FR-AYNZ / EFR-FR-BJUY: route a find accelerator to the active
   * tab's own document — an artifact, or a draft's live prompt (NAW-FR-57). The
   * panel's state belongs to that document (EFR-FR-GBJT, NAW-FR-12), so toggling
   * it here is all the shell has to do: whichever surface is showing that
   * document renders the result, and the menu does exactly what the accelerator
   * does because it is the same call.
   */
  const requestFind = (requested: FindForm) => {
    // One record per request either way, and none of them carries what the
    // author typed: the query is their own content, so what is recorded is the
    // form asked for, the kind of tab it reached, and the outcome.
    const tabKind = activeSaveTab?.kind ?? "none";
    const session = findTarget ? findTarget.store.get(findTarget.id) : undefined;
    // EFR-FR-HLGY: while a modal blocks the tab both panels are inert, so the
    // accelerator opens nothing there either.
    if (!findTarget || session?.conflict) {
      logDebug(["frontend"], "find request opened no panel", {
        requested,
        tabKind,
        reason: findTarget ? "surface blocked" : "no searchable surface",
      });
      return;
    }
    const form = nextFindForm(session?.find.form ?? null, requested);
    logDebug(["frontend"], "find panel routed to the active tab", {
      requested,
      tabKind,
      form: form ?? "closed",
    });
    findTarget.store.setFind(findTarget.id, { form });
  };

  /**
   * SNV-FR-29: File → Save writes exactly the active tab's content and nothing
   * else — the artifact through the Editor's guarded write path (EDT-FR-34), or
   * the Flow's serialized document through `"save artifact contents"`
   * (FLO-FR-27). A blocked write leaves its modal, confirmation, or error on the
   * tab the user is already looking at.
   *
   * Gated on the same condition that greys the menu item, so a Save that arrived
   * anyway cannot write content the user was told was not savable.
   */
  const requestSave = async () => {
    if (!saveEnabled) return;
    if (draftSaveTarget) {
      await drafts.flush(draftSaveTarget);
      return;
    }
    if (!saveTarget) return;
    // SNV-FR-29 / EDT-FR-70: not forced. Save is enabled only while the tab holds
    // a write that has not landed (SNV-FR-28), so "whatever the buffer holds"
    // and "what is outstanding" are the same thing — and forcing would write a
    // second time when the tab's own rest elapsed between the keypress and this
    // running.
    if (saveTarget.flow) await flows.flush(saveTarget.id);
    else await sessions.flush(saveTarget.id);
  };

  /**
   * SNV-FR-31 / EDT-FR-35 / EDT-FR-36: File → Save All writes every artifact and
   * every Flow holding unsaved changes, whether or not it has an open tab.
   * A blocked write does not stop the sweep — the rest still land — and the
   * first blocker is focused afterwards so the user can resolve it and invoke
   * Save All again.
   */
  const requestSaveAll = async () => {
    // Both id lists are snapshotted before their loop, so a flush mutating the
    // store cannot shorten the sweep out from under it.
    let firstBlock: FlushResult | null = null;
    for (const id of sessions.dirtyIds()) {
      const res = await sessions.flush(id);
      if (!res.ok && !firstBlock) firstBlock = res;
    }
    for (const id of flows.dirtyIds()) {
      const res = await flows.flush(id);
      if (!res.ok && !firstBlock) firstBlock = res;
    }
    // NAW-FR-12: a dirty draft is in the sweep whether or not its tab is open.
    let firstDraftBlock: string | null = null;
    for (const id of drafts.dirtyIds()) {
      const res = await drafts.flush(id);
      if (!res.ok && !firstDraftBlock) firstDraftBlock = id;
    }
    if (firstBlock?.artifactId) focusBlocker(firstBlock.artifactId);
    else if (firstDraftBlock) focusDraft(firstDraftBlock);
  };

  return {
    saveEnabled,
    saveAllEnabled,
    findEnabled,
    requestSave,
    requestSaveAll,
    requestFind,
  };
}

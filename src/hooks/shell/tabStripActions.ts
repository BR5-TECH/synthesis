/**
 * The closes and the pins the **author** performs: one tab's own close control,
 * the pin actions of TAB-FR-34 / TAB-FR-35, and the mass closes of TAB-FR-37 ..
 * TAB-FR-39.
 *
 * Held apart from `./tabClosures`, which holds the closures the shell performs
 * on its own initiative. The difference is the write: everything here brings a
 * pending write forward and can be refused by it (TAB-FR-10, TAB-FR-11), where
 * an automatic closure over a file that no longer exists must not.
 *
 * Plain closures rather than a hook: the strip and the session stores stay
 * owned by `useShellSession`.
 */
import type { Dispatch, MutableRefObject, SetStateAction } from "react";
import { forgetClosedConversationTab } from "./discussionActions";
import {
  hunkCandidateBuffers,
  promptCandidateBuffers,
} from "../../state/candidateBuffers";
import type { EditSessionStore } from "../../state/editSessions";
import type { FlowSessionStore } from "../../state/flowSessions";
import type { DraftSessionStore } from "../../state/draftSessions";
import type { SearchSessionStore } from "../../state/searchSessions";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import type { Tab, TabCloseScope } from "../../types";
import { HOME_TAB_TARGET } from "../../types";
import { SPEC_MAP_TAB, neverEmpty } from "./tabRecords";

export interface TabStripActionDeps {
  /**
   * The live strip, readable synchronously between two awaited closes — see the
   * note on the ref itself in `useShellSession`.
   */
  tabsRef: MutableRefObject<Tab[]>;
  activeTab: string;
  pinnedTabs: ReadonlySet<string>;
  setPinnedTabs: Dispatch<SetStateAction<ReadonlySet<string>>>;
  setTabs: Dispatch<SetStateAction<Tab[]>>;
  setActiveTab: Dispatch<SetStateAction<string>>;
  activateTab: (id: string) => void;
  sessions: EditSessionStore;
  flows: FlowSessionStore;
  drafts: DraftSessionStore;
  searches: SearchSessionStore;
  specMap: SpecMapSessionStore;
  /** TAB-FR-37: the eligible set, held in the hook so its identity is stable. */
  massCloseTargets: (scope: TabCloseScope, targetId: string) => string[];
}

export interface TabStripActions {
  closeTab: (id: string) => Promise<boolean>;
  setTabPinned: (id: string, pinned: boolean) => void;
  closeTabGroup: (scope: TabCloseScope, targetId: string) => Promise<void>;
}

/**
 * TAB-FR-37: the tabs a mass close is eligible to take.
 *
 * Every unpinned tab in the scope's range and nothing else. The
 * context-clicked tab is never among them — whether or not it is itself
 * pinned, the tab the author pointed at being the one they said to keep —
 * which is also why no mass close can empty the strip.
 *
 * A pure function over the strip so the strip computes enablement from the same
 * rule the close will run by (TAB-FR-39): an entry is disabled exactly while
 * its eligible set is empty, which is a statement about pins and never about
 * the clicked tab's position in the strip.
 */
export function eligibleMassCloseTargets(
  tabs: Tab[],
  pinnedTabs: ReadonlySet<string>,
  scope: TabCloseScope,
  targetId: string,
): string[] {
  // TAB-FR-40: the Home affordance has no tab id, but it does have a
  // position — the head of the strip — and that is all a mass close needs
  // from it. Standing before every tab, it leaves nothing on its left and
  // every unpinned tab on its right, which is also the whole strip: the two
  // live entries of its menu take one and the same set.
  const home = targetId === HOME_TAB_TARGET;
  const index = home ? -1 : tabs.findIndex((t) => t.id === targetId);
  if (index < 0 && !home) return [];
  return tabs
    .filter((t, i) => {
      if (t.id === targetId) return false;
      if (pinnedTabs.has(t.id)) return false;
      if (scope === "left") return i < index;
      if (scope === "right") return i > index;
      return true;
    })
    .map((t) => t.id);
}

export function createTabStripActions(
  deps: TabStripActionDeps,
): TabStripActions {
  const {
    tabsRef,
    activeTab,
    setPinnedTabs,
    setTabs,
    setActiveTab,
    activateTab,
    sessions,
    flows,
    drafts,
    searches,
    specMap,
    massCloseTargets,
  } = deps;

  /**
   * Close one tab, reporting whether it actually closed.
   *
   * The boolean is what a mass close reads (TAB-FR-38): a tab that refuses —
   * an unresolved external-change modal, or a failed write (TAB-FR-11,
   * TAB-FR-13, TAB-FR-18) — must not end the pass, and the caller can only
   * distinguish "refused" from "closed" if this says so. Every other caller
   * ignores it.
   */
  const closeTab = async (id: string): Promise<boolean> => {
    const tabs = tabsRef.current;
    // TAB-FR-16 / DSH-FR-02: the Dashboard's close control is inert while it is
    // the only open tab. Enforced here as well as in the strip, because this is
    // where the never-empty invariant (TAB-FR-15) is kept: without the guard a
    // close would empty the strip and the fallback below would immediately
    // reopen the Dashboard, which is a flicker rather than a behaviour.
    if (id === "dashboard" && tabs.length === 1) return false;

    // SCH-FR-20: a Search results tab's search is cancelled when the tab closes
    // (the hook's unmount does that), and its recorded result set goes with it —
    // reopening the same query is a fresh sweep, not a resumed one.
    searches.drop(id);

    // SMP-FR-BXAP: closing the Map tab resets its view and its focus. The index,
    // its edits, and its draft placements stay with the project.
    if (id === SPEC_MAP_TAB.id) specMap.resetView();

    // TAB-FR-10 / EDT-FR-31: closing an Editor tab writes its pending changes
    // first. TAB-FR-11 / EDT-FR-32: a write that cannot proceed safely refuses,
    // and the tab stays open and focused so the user sees the blocker.
    // TAB-FR-12 / TAB-FR-13 / FLO-FR-28: a Flow tab writes and refuses the same
    // way — but retains nothing afterwards, so the Flow reloads from disk.
    const closing = tabs.find((t) => t.id === id);
    const artifactId = closing?.artifactId;
    if (artifactId) {
      // PCR-FR-22 / PCR-FR-25: a candidate edit is written immediately when the
      // Editor tab closes, so an edit is not lost to the tab going. That write
      // reaches proposal storage and never the artifact (PCP-FR-22), so — unlike
      // the artifact's own write below — it cannot refuse the close.
      for (const proposalId of promptCandidateBuffers.pendingKeys()) {
        void promptCandidateBuffers.flush(proposalId);
      }
      const flow = closing?.kind === "flow";
      const res = flow
        ? await flows.flush(artifactId)
        : await sessions.flush(artifactId);
      if (!res.ok) {
        activateTab(id);
        return false;
      }
      // The artifact's edit state outlives the tab (EDT-FR-28); only an artifact
      // that was never edited is forgotten here. A Flow's never outlives it.
      //
      // TAB-FR-10: an artifact can be showing in several tabs at once — an
      // Editor tab and any number of Diff tabs (TAB-FR-06) — and closing one of
      // them leaves the session standing for the tabs that remain. Telling the
      // store the artifact has no tab open while another still shows it would
      // discard an unedited record the remaining tab is reading from.
      // Re-read rather than reusing the snapshot above: the flush was awaited,
      // and an automatic closure (TAB-FR-19, TAB-FR-22) can have taken the
      // sibling tab while it ran. Deciding against the older list would keep a
      // session alive for a tab that is no longer in the strip.
      const stillShowing = tabsRef.current.some(
        (t) => t.id !== id && t.artifactId === artifactId,
      );
      if (flow) flows.closeTab(artifactId);
      else if (!stillShowing) sessions.closeTab(artifactId);
    }

    // NAW-FR-12 / NAW-FR-27 / TAB-FR-18: closing a New Artifact tab writes its
    // pending draft file first and refuses the close if that write fails, on
    // the same terms an Editor tab does. A draft tab carries `draftId` rather
    // than `artifactId` — a draft is not a project file — so it needs its own
    // branch here or the write never happens. The session itself is kept: the
    // buffer and selection outlive the tab (NAW-FR-11), and the draft is not
    // deleted by closing what was looking at it (NAW-FR-27).
    if (closing?.kind === "draft" && closing.draftId) {
      // DCR-FR-26: a candidate edit is written immediately when the New Artifact
      // tab closes, so an edit is not lost to the tab going. That write reaches
      // proposal storage and never the draft (DCP-FR-25), so — unlike the draft
      // file's own write below — it cannot refuse the close.
      for (const key of hunkCandidateBuffers.pendingKeys()) {
        void hunkCandidateBuffers.flush(key);
      }
      const res = await drafts.flush(closing.draftId);
      if (!res.ok) {
        activateTab(id);
        return false;
      }
    }

    /**
     * TAB-FR-25: closing a conversation tab ends the tab, not the conversation.
     * No comment is posted, no lock or resolution is set, and nothing is written
     * to any discussion. The session store keeps the typed text, the attachments,
     * the scroll position, and the unread state. Only a note's opening tab,
     * which has no discussion yet, discards its unposted text (CVP-FR-60). The
     * tab owns no file, so this close is never refused, and an outstanding agent
     * turn is neither cancelled nor waited on.
     */
    forgetClosedConversationTab(closing);

    // What the strip holds once this tab is gone. Captured from the updater
    // rather than computed from `tabs`, which the awaited flush above may have
    // left stale — the filter has to run against the live array.
    let remaining: Tab[] = [];
    setTabs((ts) => {
      const next = ts.filter((t) => t.id !== id);
      remaining = neverEmpty(next);
      // Current from here rather than from the render this schedules, so the
      // next close in a mass-close pass (TAB-FR-38) decides against a strip
      // this one has already left.
      tabsRef.current = remaining;
      return remaining;
    });
    /**
     * Focus follows the strip: whoever is active keeps the focus if their tab
     * survived, and the last remaining tab takes it otherwise — which for a
     * strip that just emptied is the Dashboard that replaced it.
     *
     * Phrased against the *live* active tab rather than this closure's, because
     * the flush above is awaited and the user can activate another tab while it
     * runs. Asking "was the tab I closed the active one?" from the closure gets
     * both racy answers wrong: a stale yes drags focus off the tab the user just
     * switched to, and a stale no leaves `activeTab` naming the tab that was
     * removed — a strip with nothing marked active and a viewport with nothing
     * to route to.
     *
     * SNV-FR-65: the raw setter, deliberately. Focus landing on a neighbour
     * because the tab it was on was removed is a consequence rather than a
     * navigation, so the vertical panel does not follow it.
     */
    setActiveTab((current) =>
      remaining.some((t) => t.id === current)
        ? current
        : remaining[remaining.length - 1].id,
    );
    return true;
  };

  /**
   * TAB-FR-34: pin and unpin the tab the context menu was opened on.
   *
   * Each does exactly one thing. The active tab is unchanged, no tab opens or
   * closes, and the strip does not reorder — a tab holds its position among its
   * neighbours whether pinned or not, there being no pinned group for one to be
   * gathered into. Pinning excludes a tab from the mass closes of TAB-FR-37 and
   * reaches nothing else: it disables no close control, refuses no close, and
   * does not reach the automatic closures of TAB-FR-19, TAB-FR-22, or
   * TAB-FR-14, each of which runs over a pinned tab exactly as over any other.
   */
  const setTabPinned = (id: string, pinned: boolean) => {
    setPinnedTabs((current) => {
      if (current.has(id) === pinned) return current;
      const next = new Set(current);
      if (pinned) next.add(id);
      else next.delete(id);
      return next;
    });
  };


  /**
   * TAB-FR-38: run a mass close.
   *
   * Every eligible tab is attempted, in strip order from the leading end, each
   * down the same path its own close control takes — so an outstanding write is
   * brought forward and completed before its tab goes. A tab that refuses stays
   * open with its blocker visible and does not end the pass, so one file the
   * author must attend to does not leave a dozen they are finished with
   * standing behind it.
   *
   * Sequential rather than concurrent, and deliberately: each close awaits a
   * write and then commits a new strip, and a set of them racing would settle
   * the never-empty fallback and the surviving active tab against whichever
   * order the writes happened to finish in.
   *
   * A mass close is the user closing tabs themselves, so it writes no log
   * record for any of them (TAB-FR-21).
   */
  const closeTabGroup = async (scope: TabCloseScope, targetId: string) => {
    const eligible = massCloseTargets(scope, targetId);
    if (eligible.length === 0) return;

    const activeBefore = activeTab;
    const refused: string[] = [];
    for (const id of eligible) {
      const closed = await closeTab(id);
      if (!closed) refused.push(id);
    }

    // Where at least one tab refused, the first refusal in strip order takes
    // the focus with its blocker visible — the blocker is what needs the
    // author, and it outranks the tidy landing of the clean case. Where none
    // did, the context-clicked tab becomes active if the previously-active tab
    // was among those closed, and the active tab is untouched if it was not.
    if (refused.length > 0) {
      activateTab(refused[0]);
      return;
    }
    // A mass close from the Home affordance (TAB-FR-40) has no tab to hand the
    // focus to — the affordance is not one. Where it emptied the strip the
    // never-empty fallback has already opened a Dashboard tab and focused it
    // (TAB-FR-15), and where pinned tabs survived, `closeTab` settled the
    // active tab on one of them as it went.
    if (targetId === HOME_TAB_TARGET) return;
    if (eligible.includes(activeBefore)) activateTab(targetId);
  };

  return { closeTab, setTabPinned, closeTabGroup };
}

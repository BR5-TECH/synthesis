import type { OpenableArtifact, Tab } from "../types";
import type { EditSessionStore } from "../state/editSessions";
import type { DraftSessionStore } from "../state/draftSessions";
import type { FlowSessionStore } from "../state/flowSessions";
import { Dashboard } from "./Dashboard";
import { FlowCanvas } from "./Flow";
import { Editor } from "./Editor";
import { DiffView } from "./DiffView";
import { SearchResults } from "./SearchResults";
import { NewArtifactWorkspace } from "./NewArtifactWorkspace";
import { ConversationTabView } from "./ConversationTabView";
import type { SearchSessionStore } from "../state/searchSessions";
import type { SpecMapSessionStore } from "../state/specMap/session";
import { SpecMap } from "./SpecMap";
import { DocumentTextViewer } from "./DocumentViewer";
import { PdfViewer } from "./PdfViewer";
import type { Discussion, SearchHit } from "../types";

interface ViewportProps {
  activeTab: string;
  activeT: Tab | undefined;
  onOpenArtifact: (item: OpenableArtifact) => void;
  onOpenRuns: () => void;
  onOpenGit: () => void;
  /** EDT-FR-28: the shell-owned store the Editor's state outlives this mount in. */
  sessions: EditSessionStore;
  /** FLO-FR-30: the shell-owned store a Flow's graph and dirty state live in. */
  flows: FlowSessionStore;
  /** NAW-FR-09 / NAW-FR-11: the store a draft's selection and buffer outlive this mount in. */
  drafts: DraftSessionStore;
  /** SCH-FR-09: follow a result from the Search results tab to its surface. */
  onActivateSearchHit: (hit: SearchHit) => void;
  /** SCH-FR-11: the shell-owned store a tab's finished result set survives in. */
  searches: SearchSessionStore;
  /**
   * CMT-FR-36 / CMP-FR-11: the thread an Editor tab should land on, set by the
   * Comments panel's click-through. Applies only to the tab whose artifact it
   * names, so switching to another tab does not carry the focus along.
   */
  focusThread?: { artifactId: string; threadId: string } | null;
  onThreadFocused?: () => void;
  /** CVP-FR-60: a note's opening post created its discussion; the tab binds to it. */
  onConversationOpened?: (tabId: string, discussion: Discussion) => void;
  /** NAW-FR-04 / NAW-FR-24: the strip follows a draft's rename and its departure. */
  onDraftChanged: () => void;
  /**
   * DRS-FR-22 / NAW-FR-25: bumped whenever a draft's record or file set moves,
   * from this tab or from anywhere else. A New Artifact tab needs it because a
   * rename made in the Drafts panel renames the draft's **primary file** too,
   * and a tab that heard only about the name would go on showing — and writing
   * to — the file's old path.
   */
  draftsRevision: number;
  /**
   * NAW-FR-20 / GRU-FR-MYFA: a graduation was enqueued. The draft is not ended by
   * it — the run is, so the shell routes to the run rather than closing the tab.
   */
  onGraduationStarted: (runId: string) => void;
  /**
   *: the graduation start preflight's optional push needs a GitHub
   * token chosen, which is the shell's picker rather than a tab's.
   */
  /**
   * NAW-FR-BJQX: open the run a locked draft's tab names — and the route the
   * Dashboard's agent-activity rows take (DSH-FR-06, DSH-FR-07).
   */
  onOpenRun: (runId: string) => void;
  /**
   * DSH-FR-12 / NAW-FR-03: open an existing draft in a New Artifact tab, by id.
   * The Dashboard's Active workstreams rows are the only caller here.
   */
  onOpenDraft: (draftId: string) => void;
  /**
   * NAW-FR-28: **Archive** closes the tab, because a draft being retired is one
   * the author is finished looking at. The shell's ordinary tab close, not the
   * graduation path — the draft still exists, and its retained selection and
   * buffers stay with it (NAW-FR-16, NAW-FR-23).
   */
  onCloseTab: (tabId: string) => void;
  onDraftRenamed: (draftId: string, name: string) => void;
  /** SMP-FR-QNUH: the shell-owned map session the Map tab renders from. */
  specMap: SpecMapSessionStore;
  /** SMD-FR-HVBE: create a draft from a map node and place it there. */
  onNewDraftForMapNode: (nodeId: string) => void;
  /** SNV-FR-56: a dialog of the Map tab closes every other floating overlay. */
  onOverlayOpening?: () => void;
}

/**
 * Routes the focused tab to its surface: the Dashboard by tab id, then a Flow
 * tab vs an Editor tab by `kind` (LIB-FR-03). Neither settings surface appears
 * here — each is a native child window rather than a tab of this viewport
 * (per `SWN-settings-windows.md` SWN-FR-01, `TAB-tabs.md` TAB-FR-03).
 * The Editor is keyed by the artifact (falling back to the tab id) so switching
 * artifacts gives a fresh Editor + Tiptap instance. That remount costs the
 * artifact nothing: its buffer, dirty flag, mode and undo history live in the
 * shell's session store, which the Editor resumes from (EDT-FR-29/EDT-FR-30).
 */
export function Viewport({
  activeTab,
  activeT,
  onOpenArtifact,
  onOpenRuns,
  onOpenGit,
  sessions,
  flows,
  drafts,
  onActivateSearchHit,
  searches,
  focusThread,
  onThreadFocused,
  onConversationOpened,
  onDraftChanged,
  draftsRevision,
  onGraduationStarted,
  onOpenRun,
  onOpenDraft,
  onCloseTab,
  onDraftRenamed,
  specMap,
  onNewDraftForMapNode,
  onOverlayOpening,
}: ViewportProps) {
  // TAB-FR-01: the viewport holds tab content and nothing else. The strip never
  // empties while a project is open (TAB-FR-15), so in practice there is always
  // an active tab; this is a defensive floor, not a state the shell reaches —
  // without it an unrecognised id would fall through to the Editor branch below
  // and render one on nothing.
  if (!activeTab) return null;
  if (activeTab === "dashboard")
    return (
      <Dashboard
        onOpenArtifact={onOpenArtifact}
        onOpenRuns={onOpenRuns}
        onOpenGit={onOpenGit}
        // DSH-FR-12: an Active workstreams item opens that draft in a New
        // Artifact tab — it opens no Project panel filter and no other surface.
        onOpenDraft={onOpenDraft}
        // DSH-FR-07: a run row opens the Runs bottom panel for that run.
        onOpenRun={onOpenRun}
      />
    );
  // TAB-FR-QXMV / TAB-FR-23: a conversation tab, keyed on the tab's own id so a
  // note's opening tab turns into its discussion's tab without a remount. It
  // outlives every file the conversation was about (TAB-FR-24).
  if (activeT?.kind === "conversation")
    return (
      <ConversationTabView
        key={activeT.id}
        tab={activeT}
        onOpened={(tabId, discussion) =>
          onConversationOpened?.(tabId, discussion)
        }
      />
    );
  // CHG-FR-18: a Diff tab renders one file's diff for one comparison. Keyed on
  // the tab id, which already encodes that pair (CHG-FR-19).
  if (activeT?.kind === "diff" && activeT.diff)
    return (
      <DiffView key={activeT.id} target={activeT.diff} sessions={sessions} />
    );
  // SCH-FR-08 / SCH-FR-11: the full results page. Keyed on the tab id (which
  // already encodes the query and mode), so returning to it shows the same
  // result set rather than re-dispatching (SCH-FR-11).
  if (activeT?.kind === "search" && activeT.search)
    return (
      <SearchResults
        key={activeT.id}
        tabId={activeT.id}
        target={activeT.search}
        onActivate={onActivateSearchHit}
        sessions={searches}
      />
    );
  // NAW-FR-01: a draft opens in a New Artifact tab, keyed on the draft's own id
  // so switching drafts gives the workspace a fresh mount (TAB-FR-17).
  if (activeT?.kind === "draft" && activeT.draftId)
    return (
      <NewArtifactWorkspace
        key={activeT.draftId}
        draftId={activeT.draftId}
        drafts={drafts}
        // NAW-FR-04 / DRP-FR-11: the tab's label is the shell's copy of the
        // draft name, kept current by every rename wherever it was made, so the
        // chrome inside the tab reads the same name the strip does.
        name={activeT.label}
        onDraftChanged={onDraftChanged}
        draftsRevision={draftsRevision}
        onGraduationStarted={onGraduationStarted}
        onOpenRun={onOpenRun}
        onArchived={() => onCloseTab(activeT.id)}
        onNameChanged={(name) => onDraftRenamed(activeT.draftId!, name)}
      />
    );
  // SMP-FR-KQTD: the one Map tab renders from the shell's map session, which
  // outlives this mount (SMP-FR-QNUH).
  if (activeT?.kind === "map")
    return (
      <SpecMap
        store={specMap}
        // SMI-FR-GAJD: Open in editor takes the shell's ordinary open route.
        onOpenArtifact={onOpenArtifact}
        onNewDraft={onNewDraftForMapNode}
        // SMD-FR-XEPS: by id, so a renamed draft is still the one that opens.
        onOpenDraft={(draft) => onOpenDraft(draft.id)}
        onOverlayOpening={onOverlayOpening}
      />
    );
  // TAB-FR-LKCT / TAB-FR-QXRF: the viewer-only tabs of the Documents
  // collection, each keyed on the document id so a second document gives the
  // viewer a fresh mount. Neither is an Editor and neither holds a session.
  if (activeT?.kind === "document" && activeT.documentId)
    return <DocumentTextViewer key={activeT.id} documentId={activeT.documentId} />;
  if (activeT?.kind === "pdf" && activeT.documentId)
    return <PdfViewer key={activeT.id} documentId={activeT.documentId} />;
  if (activeT?.kind === "flow")
    return (
      <FlowCanvas
        key={activeT.artifactId ?? activeTab}
        flowId={activeT.artifactId}
        flows={flows}
        // FLO-FR-12: a node's artifact click-through opens that artifact in its
        // natural surface, through the same shell entry point every other
        // surface routes an open through (LIB-FR-03 / TAB-FR-07).
        onOpenArtifact={onOpenArtifact}
      />
    );
  return (
    <Editor
      key={activeT?.artifactId ?? activeTab}
      artifactId={activeT?.artifactId}
      artifactName={activeT?.label}
      artifactType={activeT?.type}
      sessions={sessions}
      focusThreadId={
        focusThread && focusThread.artifactId === activeT?.artifactId
          ? focusThread.threadId
          : null
      }
      onThreadFocused={onThreadFocused}
    />
  );
}

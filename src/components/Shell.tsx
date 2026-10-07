import { useEffect, useRef, useState } from "react";
import * as api from "../api";
import { Icon } from "./icons";
import { SearchOverlay } from "./SearchOverlay";
import { useSearch } from "../hooks/useSearch";
import { QUERY_MODES, useSearchQueryMode } from "../hooks/useSearchQueryMode";
import { useDraftsPanelState } from "../hooks/useDraftsPanelState";
import { useLibraryPanelState } from "../hooks/useLibraryPanelState";
import { useNotesPanelState } from "../hooks/useNotesPanelState";
import { useCommentsPanelState } from "../hooks/useCommentsPanelState";
import { AgentsChromeControl } from "./Agents";
import { ProjectSwitcher } from "./ProjectSwitcher";
import { WorktreeSelector, type SwitchOutcome } from "./WorktreeSelector";
import { WorkStreamSelector } from "./WorkStreamSelector";
import type { PullRequestSource } from "./CreatePullRequest/types";
import { PushControl } from "./PushControl";
import { Library } from "./Library";
import { DocumentsPanel } from "./Documents";
import { Notes } from "./Notes";
import { Comments } from "./Comments";
import type { DiscussionReveal } from "../state/revealDiscussion";
import { Changes } from "./Changes";
import { DraftsPanel } from "./DraftsPanel";
import { Tooltip } from "./Tooltip";
import { isToggleActive } from "../state/panelToggle";
import type { CommitFile } from "./CommitMessageModal";
import type { TabAttention } from "./TabStrip";
import type { RollbackFile } from "./RollbackConfirm";
import { THEME_CHOICES } from "./GlobalSettings";
import type {
  BottomSurface,
  CommitOutcome,
  DocumentEntry,
  RollbackResult,
  DiffTarget,
  NotesEntity,
  OpenableArtifact,
  PanelRevealRequest,
  PanelSurface,
  ProjectHandle,
  SearchHit,
  SearchMode,
  ThemePreference,
  TreeNode,
  WorktreeContext,
  WorktreeEntry,
} from "../types";

interface TopChromeProps {
  projectName: string;
  // Path of the open project, so the switcher can flag the current entry
  // (SNV-FR-20).
  projectPath: string;
  // SNV-FR-19 / SNV-FR-22: switch the active project, or leave for the picker.
  onSwitchProject: (handle: ProjectHandle) => void;
  // OVW-FR-11 / EDT-FR-33: gate a switch on writing the outgoing project's
  // pending Editor changes; false cancels it. Required — see ProjectSwitcher.
  onBeforeSwitchProject: () => Promise<boolean>;
  onOpenAnother: () => void;
  // SNV-FR-32 / WTS-FR-02: the worktree rooting the project, or null when the
  // project's content root is not inside a Git repository — in which case no
  // selector is rendered and the project switcher is followed directly by the
  // search bar.
  activeWorktree: WorktreeEntry | null;
  // OVW-FR-12: run a switching operation through the worktree-switch
  // transition. Owned by the shell so the selector and the Git panel share one
  // implementation of it.
  onSwitchWorktree: (
    operation: () => Promise<WorktreeContext>,
  ) => Promise<SwitchOutcome>;
  // WSS-FR-YPDA / CMW-FR-07: a work stream's merge was asked to be committed,
  // so the commit-message window opens for it. The shell owns that window.
  onRequestMergeCommit: (
    streamId: string,
    streamName: string,
  ) => Promise<string | null>;
  // WSS-FR-KHGP: open the Create a PR window for a work stream.
  onCreatePullRequest?: (source: PullRequestSource) => void;
  // WSS-FR-OMAP: a merge refused for uncommitted work routes to the Changes
  // panel, which is where that work is settled.
  onOpenChanges: () => void;
  // WSS-FR-AWRS: open a stream's merge run in the Runs panel, the result a
  // `run` address has (NTF-FR-17).
  onOpenRun: (runId: string) => void;
  // The shared user-global theme *preference* (SNV-FR-15). The selector mirrors
  // the Global settings Appearance section and edits the same value.
  themePref: ThemePreference;
  onSelectTheme: (pref: ThemePreference) => void;
  searchOpen: boolean;
  setSearchOpen: (open: boolean) => void;
  // NAW-FR-01: opening the project switcher's dropdown closes every other
  // floating overlay of the main window, the status bar's in-flight operations
  // overlay included (STB-FR-12).
  onOverlayOpening: () => void;
  // SCH-FR-08: open a Search results tab over this query and mode, dispatching
  // its own `scope = full` search.
  onOpenSearchResults: (query: string, mode: SearchMode) => void;
  // SCH-FR-09: follow an overlay result to its surface.
  onActivateSearchHit: (hit: SearchHit) => void;
  // WTS-FR-35 / GHA-FR-16: the refresh control's remote leg was blocked on token
  // selection. Resolves with whether one was chosen, so the refresh can re-run or
  // abandon its remote half.
  onRequestGithubToken: () => Promise<boolean>;
  // WTS-FR-35 / GHA-FR-19: where the note sends the author when no token is
  // stored at all and there is nothing to pick between.
  onOpenGlobalSettings: () => void;
  // AGT-FR-06 / AGT-FR-07: a roster row, and its add action, open the Global
  // settings window on its Agents section with an editor open — that agent's, or a
  // fresh one for `null`.
  onOpenAgentInSettings: (agentId: string | null) => void;
  // AGT-FR-08: where the roster's empty state names agents as being enrolled.
  onOpenProjectSettings: () => void;
  // SNV-FR-56: the roster's open state, owned by the shell like `searchOpen`.
  agentsRosterOpen: boolean;
  setAgentsRosterOpen: (open: boolean) => void;
}

export function TopChrome({
  projectName,
  projectPath,
  onSwitchProject,
  onBeforeSwitchProject,
  onOpenAnother,
  activeWorktree,
  onSwitchWorktree,
  onRequestMergeCommit,
  onCreatePullRequest,
  onOpenChanges,
  onOpenRun,
  themePref,
  onSelectTheme,
  searchOpen,
  setSearchOpen,
  onOverlayOpening,
  onOpenSearchResults,
  onActivateSearchHit,
  onRequestGithubToken,
  onOpenGlobalSettings,
  onOpenAgentInSettings,
  onOpenProjectSettings,
  agentsRosterOpen,
  setAgentsRosterOpen,
}: TopChromeProps) {
  // SCH-FR-12 / SCH-FR-13: the active query mode, user-global and persisted.
  const { mode, selectMode } = useSearchQueryMode();
  const [query, setQuery] = useState("");
  /**
   * SCH-FR-18 / SCH-FR-20: the overlay's own capped search. `enabled` is the
   * overlay's open state, so dismissing it cancels exactly this search — the
   * Search results tab mounts its own hook and keeps sweeping (SCH-FR-18, SCH-FR-20).
   */
  const search = useSearch({
    query,
    mode,
    scope: "capped",
    enabled: searchOpen,
  });

  // SCH-FR-08: Enter, and the "View all results" button, both open the tab over
  // the current query and mode, and dismiss the overlay (SCH-FR-07).
  const openFullResults = () => {
    if (query.trim().length === 0) return;
    onOpenSearchResults(query, mode);
    setSearchOpen(false);
  };

  // SCH-FR-07: a pointer-down anywhere outside both the search input and the
  // overlay dismisses the overlay. The ref wraps both regions, so a click that
  // lands inside either — a result, a group collapse, the "View all results"
  // button, the input itself — is "inside" and keeps the overlay open.
  const searchRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (!searchOpen) return;
    const onDown = (e: MouseEvent) => {
      if (searchRef.current && !searchRef.current.contains(e.target as Node)) {
        setSearchOpen(false);
      }
    };
    // SCH-FR-07: the window itself losing focus (switching to another app,
    // ⌘-Tab) dismisses the overlay. The input's blur carries a null
    // relatedTarget in that case, so window focus loss is handled here rather
    // than in the input's onBlur guard.
    const onWindowBlur = () => setSearchOpen(false);
    document.addEventListener("mousedown", onDown);
    window.addEventListener("blur", onWindowBlur);
    return () => {
      document.removeEventListener("mousedown", onDown);
      window.removeEventListener("blur", onWindowBlur);
    };
  }, [searchOpen, setSearchOpen]);

  return (
    <div className="top-chrome">
      <div className="top-chrome__left">
        <div className="top-chrome__brand">
          <Icon.Sigma size={16} />
          <span>Synthesis</span>
        </div>
        <span className="sep--v" style={{ height: 16 }} />
        {/* SNV-FR-17: the project label is a working recent-projects switcher. */}
        <ProjectSwitcher
          projectName={projectName}
          projectPath={projectPath}
          onSwitch={onSwitchProject}
          onBeforeSwitch={onBeforeSwitchProject}
          onOpenAnother={onOpenAnother}
          onOpen={onOverlayOpening}
        />
        {/* SNV-FR-32: second and third in the leading cluster — the project
            switcher names which repository is open, the selector names which of
            its checkouts is active, and the refresh control beside it brings the
            branch list back into agreement with the remote. Both the selector and
            the refresh control render only inside a Git repository (WTS-FR-02 /
            WTS-FR-29), in which case the search bar follows the switcher
            directly. */}
        <WorktreeSelector
          active={activeWorktree}
          onSwitch={onSwitchWorktree}
          // WTS-FR-05 / NAW-FR-01: opening this dropdown closes the search
          // overlay and every other floating overlay rather than coexisting
          // with them.
          onOpen={() => {
            setSearchOpen(false);
            onOverlayOpening();
          }}
          // WTS-FR-35: the two routes out of a remote leg that resolved no
          // credential — a picker to choose between stored tokens, or the place
          // one is added when there are none.
          onRequestGithubToken={onRequestGithubToken}
          onOpenGlobalSettings={onOpenGlobalSettings}
        />
        {/* GIT-FR-XXLE / SNV-FR-32: immediately after the refresh control and
            before the work stream selector, under the condition the selector is
            rendered. It starts the one push the Git and Changes panels share. */}
        <PushControl
          inRepository={activeWorktree !== null}
          onRequestGithubToken={onRequestGithubToken}
          onOpenGlobalSettings={onOpenGlobalSettings}
        />
        {/* WSS-FR-JVUF: immediately after the Push control, and only
            inside a Git repository — a stream is a checkout of the same
            repository, and the author moves between the two the same way. */}
        <WorkStreamSelector
          inRepository={activeWorktree !== null}
          activeBranch={activeWorktree?.branch ?? null}
          // WSS-FR-XZRD / SNV-FR-56: opening this dropdown closes the search
          // overlay and every other floating overlay rather than coexisting.
          onOpen={() => {
            setSearchOpen(false);
            onOverlayOpening();
          }}
          onActivate={async (path) => {
            const outcome = await onSwitchWorktree(() =>
              api.activateWorktree(path),
            );
            if (outcome.ok) return { ok: true as const };
            return { ok: false as const, error: outcome.cancelled ? undefined : outcome.error };
          }}
          onRequestMergeCommit={onRequestMergeCommit}
          onCreatePullRequest={onCreatePullRequest}
          onOpenChanges={onOpenChanges}
          onOpenRun={onOpenRun}
        />
      </div>

      <div style={{ position: "relative" }} ref={searchRef}>
        <div className="search-input" onClick={() => setSearchOpen(true)}>
          <Icon.Search size={12} />
          {/* SCH-FR-12: the three query-mode toggles sit INSIDE the input at its
              leading edge, ahead of the text caret, with exactly one active.
              Activating one deactivates the other two; there is no state in
              which none is active. */}
          <div
            role="radiogroup"
            aria-label="Query mode"
            data-testid="query-mode-toggles"
            className="search-input__modes"
          >
            {QUERY_MODES.map((option) => (
              <button
                key={option.value}
                type="button"
                role="radio"
                aria-checked={mode === option.value}
                // The glyph alone says nothing, so the accessible name carries
                // the mode and its effect — the same thing the visible tooltip
                // below shows on hover and on keyboard focus.
                aria-label={`${option.title} — ${option.description}`}
                className="search-input__mode"
                data-active={mode === option.value}
                // The toggles live inside the input's region, so clicking one
                // must not bubble up and re-open the overlay behaviour of the
                // wrapper before the mode change is applied.
                onClick={(e) => {
                  e.stopPropagation();
                  selectMode(option.value);
                  searchInputRef.current?.focus();
                }}
              >
                {option.label}
                {/* A real tooltip rather than the native `title`, which takes a
                    second to appear, never shows on keyboard focus, and cannot
                    hold two lines. `aria-hidden` because the button's own label
                    already carries this text to assistive tech. */}
                <span className="search-input__tip" aria-hidden="true">
                  <strong>{option.title}</strong>
                  {option.description}
                </span>
              </button>
            ))}
          </div>
          <input
            ref={searchInputRef}
            placeholder="Search artifacts, flows, runs…"
            value={query}
            onChange={(e) => {
              setQuery(e.target.value);
              setSearchOpen(true);
            }}
            onKeyDown={(e) => {
              // SCH-FR-08: submitting the query opens the Search results tab.
              if (e.key === "Enter") {
                e.preventDefault();
                openFullResults();
              }
            }}
            onFocus={() => setSearchOpen(true)}
            // SCH-FR-07: the input losing keyboard focus to a control outside
            // the search region dismisses the overlay. Guarding on a
            // relatedTarget outside the ref keeps it open when focus moves to
            // an in-overlay control (e.g. a group header) and avoids the
            // blur-vs-click race for non-focusable result rows (relatedTarget
            // null), whose dismissal is governed by the outside-pointer rule.
            onBlur={(e) => {
              const next = e.relatedTarget as Node | null;
              if (next && searchRef.current && !searchRef.current.contains(next)) {
                setSearchOpen(false);
              }
            }}
          />
          <kbd>⌘K</kbd>
        </div>
        {searchOpen && (
          <SearchOverlay
            inputRef={searchInputRef}
            onClose={() => setSearchOpen(false)}
            onOpenFullResults={openFullResults}
            hits={search.hits}
            running={search.running}
            ended={search.ended}
            error={search.error}
            hasQuery={query.trim().length > 0}
            onActivate={onActivateSearchHit}
          />
        )}
      </div>

      <div className="top-chrome__right">
        <button
          className="btn btn--ghost btn--icon btn--sm"
          title="History"
        >
          <Icon.History size={13} />
        </button>
        {/* SNV-FR-59: the trailing cluster holds the agents control and then the
            theme selector, in that fixed order — the two things that belong to
            the author rather than to the repository. The control is present
            whenever a project is open, whether or not that project has enrolled
            anyone (AGT-FR-02); its count, its roster, and everything the roster
            offers are owned by `AGT-agents.md`. */}
        <AgentsChromeControl
          // SNV-FR-56: the roster is a floating overlay of the main window, so
          // opening it closes every other.
          onOverlayOpening={onOverlayOpening}
          onOpenAgentInSettings={onOpenAgentInSettings}
          onOpenProjectSettings={onOpenProjectSettings}
          open={agentsRosterOpen}
          onOpenChange={setAgentsRosterOpen}
        />
        {/* SNV-FR-15 / SNV-FR-16: a light/dark/system selector that mirrors the
            Global settings Appearance section and edits the same shared
            user-global preference. */}
        <div
          role="radiogroup"
          aria-label="Theme"
          data-testid="chrome-theme-select"
          style={{ display: "inline-flex", gap: 2 }}
        >
          {THEME_CHOICES.map(([value, label]) => (
            <button
              key={value}
              role="radio"
              aria-checked={themePref === value}
              aria-label={label}
              title={`${label} theme`}
              className="btn btn--ghost btn--icon btn--sm"
              data-active={themePref === value}
              style={{
                background:
                  themePref === value ? "var(--bg-active)" : undefined,
                color: themePref === value ? "var(--accent)" : undefined,
              }}
              onClick={() => onSelectTheme(value)}
            >
              {value === "light" && <Icon.Sun size={13} />}
              {value === "dark" && <Icon.Moon size={13} />}
              {value === "system" && <Icon.Settings size={13} />}
            </button>
          ))}
        </div>
        <div className="kbd-row">
          <span
            style={{
              width: 24,
              height: 24,
              borderRadius: "50%",
              background: "var(--accent-soft)",
              color: "var(--accent)",
              display: "inline-flex",
              alignItems: "center",
              justifyContent: "center",
              fontSize: "var(--fs-ui-xs)",
              fontWeight: 600,
            }}
          >
            K
          </span>
        </div>
      </div>
    </div>
  );
}

/**
 * SNV-FR-03 / SNV-FR-44: the activity bar contains one toggle per panel surface
 * and nothing else, in two clusters. The **leading** cluster, flush to the top,
 * drives the vertical panel; the **trailing** cluster, flush to the foot above
 * the status bar, drives the bottom panel. Which zone a toggle drives is what
 * decides its cluster.
 *
 * The Global settings and Project settings entry points are deliberately absent
 * — they live in the status bar's leading region (STB-FR-04 / OVW-FR-07), which
 * is their only route.
 */
interface ActivityToggleProps {
  label: string;
  active: boolean;
  side: "left" | "right";
  onClick: () => void;
  disabled?: boolean;
  /**
   * SNV-FR-70: the needs-attention emphasis this toggle is carrying, drawn on
   * `NTF-notifications.md`'s behalf. The strip draws what that facility holds
   * and decides none of it: the toggle tracks no attention state of its own and
   * reads no run record.
   */
  attention?: TabAttention;
  children: React.ReactNode;
}

/**
 * One toggle: an icon-only button whose only label is its tooltip (SNV-FR-49),
 * which is also its accessible name so the control is reachable without
 * hovering.
 */
function ActivityToggle({
  label,
  active,
  side,
  onClick,
  disabled,
  attention,
  children,
}: ActivityToggleProps) {
  return (
    <Tooltip label={label} side={side}>
      {(aria) => (
        <button
          type="button"
          className="activity-btn"
          data-active={active}
          data-attention={attention}
          disabled={disabled}
          onClick={onClick}
          {...aria}
          // SNV-FR-70 / NTF-FR-36: the accessible needs-attention state, so a
          // strip of nine icons still says in words which one wants the author.
          // Appended to the tooltip's own accessible name rather than replacing
          // it, so the control is still named for what it does.
          aria-label={
            attention
              ? `${aria["aria-label"] ?? label} — needs attention`
              : aria["aria-label"]
          }
        >
          {/* SNV-FR-70: always in the tree so the cluster reserves its space —
              a toggle taking or losing the mark must move no other toggle in
              the strip. */}
          <span className="activity-btn__attention" aria-hidden="true" />
          {children}
        </button>
      )}
    </Tooltip>
  );
}

interface ActivityBarProps {
  /** The vertical panel's bound surface, active only while it is shown. */
  panelSurface: PanelSurface;
  /** SNV-FR-45: a hidden panel marks none of its toggles active. */
  panelHidden: boolean;
  onSelectPanelSurface: (surface: PanelSurface) => void;
  bottomSurface: BottomSurface;
  bottomVisible: boolean;
  onSelectBottomSurface: (surface: BottomSurface) => void;
  /**
   * SNV-FR-48: History describes the active tab's artifact, so its toggle is
   * live only while a tab bound to one is active.
   */
  historyEnabled: boolean;
  /** SNV-FR-06: which edge the strip sits on, so tooltips face inward. */
  side: "left" | "right";
  /**
   * SNV-FR-70 / NTF-FR-39: the emphasis the Runs toggle carries, held by the
   * notification facility and passed through here. It is presentation and
   * nothing else: a marked toggle activates, opens, and hides the bottom panel
   * exactly as an unmarked one does, and nothing about it is persisted with the
   * layout (SNV-FR-08).
   */
  runsAttention?: TabAttention;
}

export function ActivityBar({
  panelSurface,
  panelHidden,
  onSelectPanelSurface,
  bottomSurface,
  bottomVisible,
  onSelectBottomSurface,
  historyEnabled,
  side,
  runsAttention,
}: ActivityBarProps) {
  const vertical = { hidden: panelHidden, surface: panelSurface };
  const bottom = { hidden: !bottomVisible, surface: bottomSurface };
  return (
    <div className="activity-bar">
      <div className="activity-bar__cluster">
        <ActivityToggle
          label="Project"
          side={side}
          active={isToggleActive(vertical, "library")}
          onClick={() => onSelectPanelSurface("library")}
        >
          <Icon.Book size={16} />
        </ActivityToggle>
        {/* DPN-FR-KYSK / SNV-FR-UCJH: Documents sits immediately after Project
            and before Notes — the reference files of the project, beside the
            project's own content. It is not the default selection on project
            open (OVW-FR-05). */}
        <ActivityToggle
          label="Documents"
          side={side}
          active={isToggleActive(vertical, "documents")}
          onClick={() => onSelectPanelSurface("documents")}
        >
          <Icon.Documents size={16} />
        </ActivityToggle>
        <ActivityToggle
          label="Notes"
          side={side}
          active={isToggleActive(vertical, "notes")}
          onClick={() => onSelectPanelSurface("notes")}
        >
          <Icon.Notes size={16} />
        </ActivityToggle>
        {/* CMP-FR-01: Comments sits immediately below Notes in the leading
            cluster (SNV-FR-44) — the project-wide index of the conversations the
            Editor's rail shows one artifact of at a time. */}
        <ActivityToggle
          label="Comments"
          side={side}
          active={isToggleActive(vertical, "comments")}
          onClick={() => onSelectPanelSurface("comments")}
        >
          <Icon.Comment size={16} />
        </ActivityToggle>
        {/* DRP-FR-01: Drafts — the artifacts being developed before they exist
            in the project, which is the one vertical-panel surface that lists
            content the Library deliberately cannot see (ASC-FR-09). It sits
            with the surfaces describing the project's own content, above the
            one describing its Git state. */}
        <ActivityToggle
          label="Drafts"
          side={side}
          active={isToggleActive(vertical, "drafts")}
          onClick={() => onSelectPanelSurface("drafts")}
        >
          <Icon.Layers size={16} />
        </ActivityToggle>
        {/* CHG-FR-01 / SNV-FR-44: Changes closes the leading cluster,
            immediately above the Git toggle at the foot of the strip — the
            working tree it lists and the repository that panel drives are one
            subject, read top to bottom. It is not the default selection on
            project open — the Library is (OVW-FR-05). */}
        <ActivityToggle
          label="Changes"
          side={side}
          active={isToggleActive(vertical, "changes")}
          onClick={() => onSelectPanelSurface("changes")}
        >
          <Icon.Diff size={16} />
        </ActivityToggle>
      </div>

      {/* SNV-FR-44: the bottom-panel cluster, anchored to the foot of the strip
          immediately above the status bar. */}
      <div className="activity-bar__cluster activity-bar__cluster--bottom">
        <ActivityToggle
          label="Runs"
          side={side}
          active={isToggleActive(bottom, "runs")}
          attention={runsAttention}
          onClick={() => onSelectBottomSurface("runs")}
        >
          <Icon.Terminal size={16} />
        </ActivityToggle>
        {/* LOG-FR-01 / SNV-FR-44: Logs sits beside Runs at the head of the
            trailing cluster — the two surfaces that stream lines as they are
            produced. Its toggle is enabled whenever a project is open, whatever
            the active tab is, because a diagnostic record is about the
            application rather than about the open artifact. */}
        <ActivityToggle
          label="Logs"
          side={side}
          active={isToggleActive(bottom, "logs")}
          onClick={() => onSelectBottomSurface("logs")}
        >
          <Icon.Code size={16} />
        </ActivityToggle>
        {/* SNV-FR-48: greyed out with no artifact-bound tab active, but it still
            reads as active while the panel is showing History — the strip says
            where the panel is even when you cannot steer it there. */}
        <ActivityToggle
          label={
            historyEnabled ? "History" : "History — requires an open artifact"
          }
          side={side}
          disabled={!historyEnabled}
          active={isToggleActive(bottom, "history")}
          onClick={() => onSelectBottomSurface("history")}
        >
          <Icon.History size={16} />
        </ActivityToggle>
        {/* SNV-FR-44: Git closes the trailing cluster, sitting at the very foot
            of the strip immediately above the status bar — the surface whose
            branch, staging, and sync state that bar reports (per
            `STB-status-bar.md`), reachable at the corner of the window. */}
        <ActivityToggle
          label="Git"
          side={side}
          active={isToggleActive(bottom, "git")}
          onClick={() => onSelectBottomSurface("git")}
        >
          <Icon.Branch size={16} />
        </ActivityToggle>
      </div>
    </div>
  );
}

interface PanelResizerProps {
  /**
   * SNV-FR-37: the splitter exists only while the panel renders content at full
   * width. A collapsed or hidden panel has no boundary to drag, so nothing is
   * rendered — no resize cursor, no drag target.
   */
  resizable: boolean;
  dragging: boolean;
  onResizeStart: (event: { clientX: number; preventDefault?: () => void }) => void;
}

/**
 * SNV-FR-33: the vertical panel's resize handle — a full-height splitter on the
 * boundary between the panel and the main viewport, presenting a
 * horizontal-resize cursor and moving the boundary live as it is dragged.
 *
 * Rendered inside `.vpanel` (which is `position: relative`) rather than as a
 * grid track of its own: a track would have to be added to `grid-template-areas`
 * in three places, and the handle needs to overlap the panel's 1px border to be
 * comfortably clickable anyway. `role="separator"` with an orientation makes it
 * announce as what it is rather than as an unlabelled div.
 */
export function PanelResizer({
  resizable,
  dragging,
  onResizeStart,
}: PanelResizerProps) {
  if (!resizable) return null;
  return (
    <div
      className="vpanel__resizer"
      data-testid="vpanel-resizer"
      data-dragging={dragging}
      role="separator"
      aria-orientation="vertical"
      aria-label="Resize panel"
      onPointerDown={onResizeStart}
    />
  );
}

interface BottomPanelResizerProps {
  dragging: boolean;
  onResizeStart: (event: { clientY: number; preventDefault?: () => void }) => void;
}

/**
 * SNV-FR-50: the bottom panel's resize handle — a full-width splitter on the
 * boundary between the main viewport and the panel, presenting a
 * vertical-resize cursor and moving the boundary live as it is dragged.
 *
 * Rendered inside `.bottom-panel` (which is `position: relative`) for the same
 * reason `PanelResizer` lives inside `.vpanel`: a grid track of its own would
 * have to be threaded through `grid-template-areas` in three places, and the
 * handle needs to overlap the panel's 1px top border to be comfortably
 * clickable. It exists only while the panel does, so SNV-FR-53's "no splitter
 * while hidden" falls out of the panel not being rendered at all.
 */
export function BottomPanelResizer({
  dragging,
  onResizeStart,
}: BottomPanelResizerProps) {
  return (
    <div
      className="bottom-panel__resizer"
      data-testid="bottom-panel-resizer"
      data-dragging={dragging}
      role="separator"
      aria-orientation="horizontal"
      aria-label="Resize bottom panel"
      onPointerDown={onResizeStart}
    />
  );
}

interface VPanelProps {
  surface: PanelSurface;
  onOpenArtifact: (node: OpenableArtifact) => void;
  // LCM-FR-05: jump from a Library context menu to an artifact's Notes scope.
  onShowNotes: (artifact: OpenableArtifact) => void;
  // NTS-FR-11: follow a Notes group header to its artifact — revealed in the
  // Library and opened in its natural surface.
  onRevealArtifact: (artifact: OpenableArtifact) => void;
  // LCM-FR-10: open the New File modal with a Library folder as its starting
  // location.
  onNewFile: (folder: TreeNode) => void;
  // LCM-FR-08: start a draft whose destination root is a Library folder.
  onNewArtifact: (folder: TreeNode) => void;
  // LCM-FR-09: open the New Folder modal with a Library folder as its starting
  // parent.
  onNewFolder: (folder: TreeNode) => void;
  // NFW-FR-04 / NFI-FR-04: the Library publishes each tree it loads so the shell
  // can offer the project's folder set without scanning again.
  onTreeLoaded?: (tree: TreeNode) => void;
  /**
   * The pending reveal-and-select, or null. Named for the panel it addresses, so
   * a request for a surface that is not the one on screen is simply not passed
   * down (LIB-FR-18, DRP-FR-34, CHG-FR-54).
   *
   * Two unrelated routes produce one: a creation (NFI-FR-12 / NFW-FR-11 /
   * NAW-FR-10 / NTS-FR-11), and the active tab being followed (SNV-FR-64).
   */
  panelReveal: PanelRevealRequest | null;
  // DRP-FR-09: open a draft in its New Artifact tab, or focus the open one.
  onOpenDraft: (draft: { id: string; name: string }) => void;
  // DRP-FR-06 / DRP-FR-26: create a draft and open it. The pinned affordance
  // passes no folder; a folder's New Draft passes the drafts folder to file it
  // in, which is separate from the draft's graduation destination root.
  /**
   * DRP-FR-06 / DRP-FR-26 / DRP-FR-36: create a draft, open it, and resolve it
   * so the Drafts panel can put its row straight into rename mode.
   */
  onCreateDraft: (
    folder?: string,
  ) => Promise<{ id: string; name: string } | null> | void;
  // DRP-FR-12: a deleted draft's tab closes with it.
  onDraftDeleted: (id: string) => void;
  // DRP-FR-11 / NAW-FR-04: a draft renamed from the panel renames its tab too —
  // the strip's label follows the draft's name wherever the rename came from.
  onDraftRenamed: (id: string, name: string) => void;
  /**
   * DRP-FR-18: the panel moved a draft's status, so an open New Artifact tab
   * re-reads its record and follows the change into or out of its archived
   * marker — without the tab closing (NAW-FR-16).
   */
  onDraftChanged: () => void;
  /**
   * DRP-FR-35 / GRU-FR-MYFA: go to a draft's graduation run. The panel routes to
   * it and renders none of it — a run is acted on where runs are.
   */
  onOpenRun: (runId: string) => void;
  /** DRP-FR-YYZU: open the start dialog for a GitHub-shadow draft. */
  onGraduateDraft?: (draftId: string, draftName: string) => void;
  // DRP-FR-05: bumped whenever the draft set moves, so the panel reloads.
  draftsRevision: number;
  // NTS-FR-02: the entity the Notes panel's entity position binds to.
  activeEntity: NotesEntity | null;
  // CMP-FR-10 / NTS-FR-26: the one route that reveals a discussion, called by a
  // Comments row and by a note's Discuss.
  onRevealDiscussion: (reveal: DiscussionReveal) => void;
  // NTS-FR-30: the backend reported a note and its discussion deleted.
  onNoteDeleted?: (noteId: string) => void;
  // CHG-FR-18: open a Diff tab for a changed file under the active comparison.
  onOpenDiff: (target: DiffTarget) => void;
  // CHG-FR-39 / CMW-FR-03 / CHG-FR-41: open the commit message window carrying
  // the commit set, resolving with the outcome of the commit made from it — the
  // commit and the paths it recorded (GTC-FR-19) — or null if it was dismissed
  // or refused.
  onRequestCommitMessage?: (
    files: CommitFile[],
    hidden: CommitFile[],
  ) => Promise<CommitOutcome | null>;
  // CHG-FR-59 – CHG-FR-63: confirm and perform a rollback of the checked set,
  // resolving with the backend's per-path outcome or null if it was dismissed.
  onRequestRollback?: (
    files: RollbackFile[],
    onConfirmed: () => void,
  ) => Promise<RollbackResult | null>;
  // CHG-FR-45 / GHA-FR-16: a push blocked on token selection opens the picker.
  onRequestGithubToken?: () => Promise<boolean>;
  // CHG-FR-45 / GHA-FR-19: no token stored at all — route to where one is added.
  onOpenGlobalSettings?: () => void;
  // CHG-FR-UPFP: open the Create a PR window for the current branch.
  onCreatePullRequest?: (source: PullRequestSource) => void;
  // SNV-FR-67: the Changes panel publishes each comparison it loads, so the
  // shell can resolve a Diff tab's file before activating that panel.
  onChangeSetLoaded?: (paths: string[]) => void;
  // LIB-FR-VMAQ: the Project panel's View map button opens the Map tab.
  onViewSpecMap?: () => void;
  // DPN-FR-CDFO: open a document in its viewer tab, or focus the open one.
  onOpenDocument?: (entry: DocumentEntry) => void;
  // DPN-FR-ZHEJ: the documents a source removal took out of the collection.
  onDocumentsRemoved?: (documentIds: string[]) => void;
  // DPN-FR-ZMBQ / SNV-FR-56: the Add documents choice closes the other overlays.
  onOverlayOpening?: () => void;
}

export function VPanel({
  surface,
  onOpenArtifact,
  onShowNotes,
  onRevealArtifact,
  onNewFile,
  onNewArtifact,
  onNewFolder,
  onTreeLoaded,
  panelReveal,
  activeEntity,
  onRevealDiscussion,
  onNoteDeleted,
  onOpenDiff,
  onRequestCommitMessage,
  onRequestRollback,
  onRequestGithubToken,
  onOpenGlobalSettings,
  onCreatePullRequest: onCreatePullRequestFromPanel,
  onChangeSetLoaded,
  onViewSpecMap,
  onOpenDraft,
  onCreateDraft,
  onDraftDeleted,
  onDraftRenamed,
  onDraftChanged,
  onOpenRun,
  onGraduateDraft,
  draftsRevision,
  onOpenDocument,
  onDocumentsRemoved,
  onOverlayOpening,
}: VPanelProps) {
  // LIB-FR-14 … LIB-FR-17: the Library's expand/filter state lives here rather
  // than inside `Library`, because choosing another surface below unmounts that
  // component and the state has to outlive it (LIB-FR-06). This component's own
  // lifetime is one project + one content root — `App` remounts the shell
  // subtree when either changes — which is also the boundary a pending write
  // must not cross (PSS-FR-16).
  const libraryPanel = useLibraryPanelState();
  // NTS-FR-09 / NTS-FR-13: the Notes panel's selector position and filter text
  // live here for the same reason, and for a stronger one — the position is
  // sticky across a panel switch, which is exactly what unmounts `<Notes>`.
  const notesPanel = useNotesPanelState();
  // CMP-FR-15: the filter text is session memory and survives a panel switch,
  // which is exactly what unmounts `<Comments>`.
  const commentsPanel = useCommentsPanelState();
  // DRP-FR-14: the status filter, the filter text and the set of expanded
  // folders survive a panel switch — which is exactly what unmounts
  // `<DraftsPanel>` — so they live here, persisted per worktree (PSS-FR-20).
  const draftsPanel = useDraftsPanelState();

  if (surface === "documents")
    return (
      <DocumentsPanel
        onOpenDocument={(entry) => onOpenDocument?.(entry)}
        onDocumentsRemoved={(ids) => onDocumentsRemoved?.(ids)}
        onOverlayOpening={onOverlayOpening}
      />
    );
  if (surface === "drafts")
    return (
      <DraftsPanel
        panel={draftsPanel}
        onOpenDraft={onOpenDraft}
        onCreateDraft={onCreateDraft}
        onDraftDeleted={onDraftDeleted}
        onDraftRenamed={onDraftRenamed}
        onDraftChanged={onDraftChanged}
        revision={draftsRevision}
        reveal={panelReveal?.panel === "drafts" ? panelReveal : null}
        onOpenRun={onOpenRun}
        onGraduateDraft={onGraduateDraft}
      />
    );
  if (surface === "changes")
    return (
      <Changes
        reveal={panelReveal?.panel === "changes" ? panelReveal : null}
        onChangeSetLoaded={onChangeSetLoaded}
        onOpenDiff={onOpenDiff}
        onRequestCommitMessage={onRequestCommitMessage}
        onRequestRollback={onRequestRollback}
        onRequestGithubToken={onRequestGithubToken}
        onOpenGlobalSettings={onOpenGlobalSettings}
        onCreatePullRequest={onCreatePullRequestFromPanel}
      />
    );
  if (surface === "comments")
    return <Comments panel={commentsPanel} onReveal={onRevealDiscussion} />;
  if (surface === "notes")
    return (
      <Notes
        panel={notesPanel}
        entity={activeEntity}
        // NTS-FR-11: a group header follows to its artifact by the same routing
        // the Library click-through uses.
        onOpenArtifact={onRevealArtifact}
        // NTS-FR-26: Discuss opens or focuses the note's conversation tab.
        onReveal={onRevealDiscussion}
        onNoteDeleted={onNoteDeleted}
      />
    );
  return (
    <Library
      panel={libraryPanel}
      onOpenArtifact={onOpenArtifact}
      onShowNotes={onShowNotes}
      onNewFile={onNewFile}
      onNewArtifact={onNewArtifact}
      onNewFolder={onNewFolder}
      onTreeLoaded={onTreeLoaded}
      reveal={panelReveal?.panel === "library" ? panelReveal : null}
      onViewSpecMap={onViewSpecMap}
    />
  );
}

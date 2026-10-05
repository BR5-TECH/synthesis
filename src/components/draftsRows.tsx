/**
 * How one row of the Drafts panel's tree is drawn — a draft, and a folder with
 * whatever it holds beneath it (DRP-FR-09, DRP-FR-20, DRP-FR-22, DRP-FR-31).
 *
 * Built from a context the panel hands over rather than from props, because a
 * row reads almost everything the panel holds: which rows are busy, which is
 * being dragged, what the filter admits, and every action a menu entry
 * invokes. Threading forty props through two recursive renderers would say
 * nothing the context does not, and would have to be restated at each level of
 * the recursion.
 */
import type React from "react";

import { Icon } from "./icons";
import { formatRelative } from "./ProjectPicker";
import { stateLabel } from "../state/graduation";
import { AnchoredMenu } from "./DraftsPanelParts";
import { folderName, ROOT } from "./draftsTree";
import type { DraftMatches, MenuAnchor, TreeItem } from "./draftsTree";
import type { DraftsPanel as DraftsPanelState } from "../hooks/useDraftsPanelState";
import { isGithubShadow } from "../types";
import type { DraftFolder, DraftSummary } from "../types";

/** DRP-FR-NPZO: why an entry of a GitHub-shadow row is disabled. */
const SHADOW_REASON =
  "This draft mirrors a GitHub issue, so it cannot be renamed, moved, archived, or deleted";

/** The one floating surface open at a time (DRP-FR-16). */
export type Overlay =
  | { kind: "draft-menu"; id: string }
  | { kind: "folder-menu"; path: string }
  | { kind: "root-menu" }
  | { kind: "draft-delete"; id: string }
  /** DRP-FR-KDVX: the draft's statistics modal (`DFI-draft-information.md`). */
  | { kind: "draft-information"; id: string }
  | { kind: "folder-delete"; path: string }
  | { kind: "move"; item: TreeItem };

/** The one inline edit open at a time. */
export type Inline =
  | { kind: "rename-draft"; id: string }
  | { kind: "rename-folder"; path: string }
  | { kind: "new-folder"; parent: string };

export interface DraftRowContext {
  panel: DraftsPanelState;
  childFolders: Map<string, DraftFolder[]>;
  draftsByFolder: Map<string, DraftSummary[]>;
  matches: DraftMatches;
  admitsDraft: (draft: DraftSummary) => boolean;
  folderVisible: (path: string) => boolean;
  isExpanded: (path: string) => boolean;
  toggleFolder: (path: string) => void;

  selected: string | null;
  setSelected: (key: string | null) => void;
  busy: ReadonlySet<string>;
  rowErrors: ReadonlyMap<string, string>;

  overlay: Overlay | null;
  setOverlay: (next: Overlay | null) => void;
  openOverlay: (
    next: Overlay | null,
    at?: { anchor: MenuAnchor; row: HTMLElement },
  ) => void;
  openRowMenu: (
    e: React.MouseEvent | React.KeyboardEvent,
    next: Overlay,
    row: HTMLElement,
  ) => void;
  isMenuKey: (e: React.KeyboardEvent) => boolean;
  anchor: MenuAnchor | null;

  inline: Inline | null;
  inlineText: string;
  setInlineText: (next: string) => void;
  inlineError: string | null;
  setInlineError: (next: string | null) => void;
  inlineRef: React.RefObject<HTMLInputElement | null>;
  openInline: (next: Inline, seed: string) => void;
  closeInline: () => void;
  commitInline: () => Promise<void>;

  dragging: TreeItem | null;
  setDragging: (item: TreeItem | null) => void;
  dropTarget: string | null;
  setDropTarget: (path: string | null) => void;
  moveRefusal: (item: TreeItem, destination: string) => string | null;
  dropHandlers: (destination: string) => {
    onDragOver: (e: React.DragEvent) => void;
    onDragLeave: (e: React.DragEvent) => void;
    onDrop: (e: React.DragEvent) => void;
  };

  doOpenDraft: (draft: DraftSummary) => void;
  doNewDraft: (folder: string) => Promise<void>;
  doNewFolder: (parent: string) => void;
  toggleArchived: (draft: DraftSummary) => Promise<void>;
  moveFocusBy: (from: HTMLElement, step: 1 | -1) => void;
  onOpenRun?: (runId: string) => void;
  /** DRP-FR-YYZU: open the start dialog for a GitHub-shadow draft. */
  onGraduateDraft?: (draftId: string, draftName: string) => void;
  /** DRP-FR-YYZU: open the issue a GitHub-shadow draft mirrors. */
  openShadowIssue: (draft: DraftSummary) => void;
}

export function makeRowRenderers(ctx: DraftRowContext) {
  const {
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
  } = ctx;

  const menuEntry = (
    label: string,
    icon: React.ReactNode,
    onClick: () => void,
    danger = false,
    /**
     * DRP-FR-33: an inconsistent draft's entries render disabled rather than
     * being hidden, so the menu keeps its shape and the author reads what is
     * unavailable rather than wondering where it went.
     */
    disabled = false,
    /** DRP-FR-NPZO: the reason a disabled entry states. */
    reason?: string,
  ) => (
    <div
      role="menuitem"
      tabIndex={disabled ? -1 : 0}
      aria-disabled={disabled || undefined}
      title={disabled ? reason : undefined}
      className={[
        danger ? "menu-item menu-item--danger" : "menu-item",
        disabled ? "menu-item--disabled" : "",
      ]
        .filter(Boolean)
        .join(" ")}
      onClick={disabled ? undefined : onClick}
      onKeyDown={(e) => {
        if (disabled) return;
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onClick();
        }
      }}
    >
      {icon}
      {label}
    </div>
  );

  const inlineField = (label: string, onCancel: () => void) => (
    <span className="drafts-tree__field">
      <input
        ref={inlineRef}
        className="input input--sm"
        aria-label={label}
        value={inlineText}
        onChange={(e) => {
          setInlineText(e.target.value);
          setInlineError(null);
        }}
        onClick={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          e.stopPropagation();
          if (e.key === "Enter") void commitInline();
          if (e.key === "Escape") onCancel();
        }}
      />
      {inlineError && (
        <p className="t-ui-xs drafts-tree__error" role="alert">
          {inlineError}
        </p>
      )}
    </span>
  );

  const renderDraft = (draft: DraftSummary, depth: number) => {
    const key = `draft:${draft.id}`;
    const item: TreeItem = {
      kind: "draft",
      id: draft.id,
      name: draft.name,
      folder: draft.folder ?? ROOT,
    };
    const renaming = inline?.kind === "rename-draft" && inline.id === draft.id;
    const locked = busy.has(key);
    const rowError = rowErrors.get(key);
    // DRP-FR-ZRJJ / DRP-FR-NPZO: a draft that mirrors a GitHub issue.
    const shadow = isGithubShadow(draft);
    const issue = draft.githubIssue ?? null;
    const issueLabel = issue
      ? `${issue.repositoryOwner}/${issue.repositoryName}#${issue.issueNumber}`
      : "GitHub issue";
    /** DRP-FR-YYZU: why Graduate… is disabled, or null where it is enabled. */
    const graduateBlocked = draft.graduation?.locked
      ? "A graduation run already holds this draft"
      : draft.status === "graduated"
        ? "This draft has already been graduated"
        : null;
    return (
      // DRP-FR-09: the whole row is the target — its name, its file count, its
      // timestamp and the space around them. A row whose second line does
      // nothing is a row the author has to aim at the top half of.
      <div
        className="draft-row"
        key={key}
        data-row-key={key}
        role="treeitem"
        aria-level={depth + 1}
        aria-selected={selected === key}
        aria-label={`Draft ${draft.name}${
          draft.status === "archived"
            ? ", archived"
            : draft.status === "graduated"
              ? ", graduated"
              : draft.status === "published"
                ? ", published"
                : draft.status === "github_shadow"
                  ? ", GitHub task"
                  : ""
        }${shadow ? `, mirrors ${issueLabel}` : ""}${
          draft.graduation?.locked ? ", graduating" : ""
        }`}
        tabIndex={0}
        // DRP-FR-NPZO: a GitHub-shadow row cannot be dragged.
        draggable={!locked && !renaming && !shadow}
        style={{ paddingLeft: depth * 12 }}
        onFocus={() => setSelected(key)}
        onClick={() => !renaming && doOpenDraft(draft)}
        onContextMenu={(e) => {
          // Always consumed, even while the row is locked: a right-click that
          // fell through would open the *root's* menu over the row the author
          // aimed at (DRP-FR-31 leaves the row itself inert, not the panel).
          e.preventDefault();
          e.stopPropagation();
          if (!locked)
            openRowMenu(e, { kind: "draft-menu", id: draft.id }, e.currentTarget);
        }}
        onDragStart={(e) => {
          if (shadow) {
            e.preventDefault();
            return;
          }
          // Guarded: a host that hands the handler no `dataTransfer` would
          // otherwise throw here and break the whole gesture before the panel
          // has recorded what is being dragged. The payload is only there
          // because Firefox will not start a drag without one — the panel
          // reads its own state rather than this.
          if (e.dataTransfer) {
            e.dataTransfer.effectAllowed = "move";
            e.dataTransfer.setData("text/plain", draft.id);
          }
          setDragging(item);
        }}
        onDragEnd={() => {
          setDragging(null);
          setDropTarget(null);
        }}
        {...dropHandlers(draft.folder ?? ROOT)}
        onKeyDown={(e) => {
          if (isMenuKey(e)) {
            if (!locked)
              openRowMenu(e, { kind: "draft-menu", id: draft.id }, e.currentTarget);
            return;
          }
          if (e.key === "Enter" || e.key === " ") {
            e.preventDefault();
            doOpenDraft(draft);
          }
          if (e.key === "ArrowDown" || e.key === "ArrowUp") {
            e.preventDefault();
            moveFocusBy(e.currentTarget, e.key === "ArrowDown" ? 1 : -1);
          }
        }}
      >
        <div className="draft-row__head">
          {renaming ? (
            inlineField("Draft name", closeInline)
          ) : (
            <span className="draft-row__name" title={draft.name}>
              <Icon.Diamond size={11} />
              <span className="draft-row__label">{draft.name}</span>
            </span>
          )}
          {/* DRP-FR-08: an archived draft is told apart where it sits rather
              than by being listed apart. No marker in the archived position,
              where every row would carry one. */}
          {draft.status === "archived" && panel.filter === "all" && (
            <span className="drafts-tree__badge" data-testid="draft-archived-marker">
              archived
            </span>
          )}
          {/* DRP-FR-08: on the same terms, and for the same reason — a
              graduated draft is told apart where it sits. It keeps its label in
              the **graduated** position too, which admits two statuses
              (DRP-FR-07) and would otherwise leave the two indistinguishable. */}
          {draft.status === "graduated" &&
            (panel.filter === "all" || panel.filter === "graduated") && (
              <span
                className="drafts-tree__badge"
                data-testid="draft-graduated-marker"
              >
                Graduated
              </span>
            )}
          {/* DRP-FR-08: a published draft states its own status, distinct from
              a graduated one — the work went to a GitHub issue rather than to a
              specification here. */}
          {draft.status === "published" &&
            (panel.filter === "all" || panel.filter === "graduated") && (
              <span
                className="drafts-tree__badge"
                data-testid="draft-published-marker"
              >
                Published
              </span>
            )}
          {/* DRP-FR-ZRJJ: a GitHub-shadow draft carries a GitHub marker naming
              its repository and issue, and reads `GitHub task` while its
              status is `github_shadow`. Once it reports `graduated` the
              Graduated marker above says so. */}
          {shadow && (
            <span
              className="drafts-tree__badge drafts-tree__badge--github"
              data-testid="draft-github-marker"
              title={`Mirrors GitHub issue ${issueLabel}`}
            >
              <Icon.GitPull size={10} /> GitHub {issueLabel}
            </span>
          )}
          {shadow && draft.status === "github_shadow" && (
            <span
              className="drafts-tree__badge"
              data-testid="draft-github-task-marker"
            >
              GitHub task
            </span>
          )}
          {/* DRP-FR-35: the graduation run this draft is bound to, as the
              run's state in words. It is the route to the run rather than a
              second run surface — a graduation is acted on where it lives
              (GRU-FR-MYFA). */}
          {draft.graduation && (
            <button
              className="drafts-tree__badge drafts-tree__badge--run"
              data-testid="draft-graduation-marker"
              title="Go to this draft's graduation run"
              onClick={(e) => {
                e.stopPropagation();
                onOpenRun?.(draft.graduation!.runId);
              }}
            >
              {stateLabel(draft.graduation.state)}
            </button>
          )}
        </div>
        <div className="draft-row__meta t-ui-xs">
          {/* DRP-FR-33: an inconsistent draft is marked distinctly and states
              that it cannot be opened. The row is never hidden and never
              silently repaired — the author is the one who decides what becomes
              of material that is not a draft. */}
          {draft.inconsistent && (
            <>
              <span
                className="draft-row__inconsistent"
                data-testid="draft-row-inconsistent"
                title="This draft's storage is not the one prompt a draft is, so it cannot be opened"
              >
                cannot be opened
              </span>
              <span aria-hidden="true">·</span>
            </>
          )}
          <span title={draft.updatedAt}>{formatRelative(draft.updatedAt)}</span>
          {/* DRP-FR-19: a change an agent has proposed and the author has not yet
              decided is visible from the panel whether or not that draft's tab is
              open, and told apart from the build indicator because a build in
              flight is work happening and a proposal pending is work waiting. */}
          {draft.hasPendingProposal && (
            <>
              <span aria-hidden="true">·</span>
              <span
                className="draft-row__proposal"
                data-testid="draft-row-proposal"
                title="An agent has proposed a change to this draft"
              >
                <Icon.Diff size={10} /> proposed change
              </span>
            </>
          )}
        </div>

        {/* DRP-FR-17: a row that survived on the strength of its contents rather
            than its name says so. A row matched by its name carries no
            annotation — the reason is already on the row. */}
        {matches?.get(draft.id) === "contents" && (
          <div className="draft-row__match t-ui-xs">
            <Icon.Quote size={10} /> <span>matches text</span>
          </div>
        )}

        {rowError && (
          <p className="t-ui-xs drafts-tree__error" role="alert">
            {rowError}
          </p>
        )}

        {/* DRP-FR-10: exactly six entries, and no folder action among them —
            with a seventh, Go to run, below a divider on a row a graduation
            run holds (DRP-FR-35). */}
        {overlay?.kind === "draft-menu" && overlay.id === draft.id && (
          <AnchoredMenu anchor={anchor} label={`Draft actions for ${draft.name}`}>
            {menuEntry("Open", <Icon.Doc size={12} />, () => {
              setOverlay(null);
              doOpenDraft(draft);
            })}
            {/* DRP-FR-KDVX: immediately after Open, because reading what a
                draft cost is the second thing an author does with a row and
                the only other entry that changes nothing. Enabled on every
                draft row without exception — active, archived, graduated, held
                by a run, and inconsistent alike — because it reads an account
                of the draft rather than touching the draft. */}
            {menuEntry("Information", <Icon.Sigma size={12} />, () =>
              openOverlay({ kind: "draft-information", id: draft.id }),
            )}
            {/* DRP-FR-NPZO / DRP-FR-YYZU: a GitHub-shadow row offers
                Graduate… and the route to its issue, and states why the
                entries that would change the draft are unavailable. */}
            {shadow &&
              menuEntry(
                "Graduate…",
                <Icon.Graduate size={12} />,
                () => {
                  setOverlay(null);
                  onGraduateDraft?.(draft.id, draft.name);
                },
                false,
                graduateBlocked !== null,
                graduateBlocked ?? undefined,
              )}
            {shadow &&
              menuEntry(
                "Open issue on GitHub",
                <Icon.GitPull size={12} />,
                () => openShadowIssue(draft),
                false,
                !issue,
                "This draft names no issue address",
              )}
            {menuEntry(
              "Rename…",
              <Icon.Tag size={12} />,
              () => openInline({ kind: "rename-draft", id: draft.id }, draft.name),
              false,
              draft.inconsistent === true || shadow,
              shadow ? SHADOW_REASON : undefined,
            )}
            {menuEntry(
              "Move to Folder…",
              <Icon.Folder size={12} />,
              () => openOverlay({ kind: "move", item }),
              false,
              draft.inconsistent === true || shadow,
              shadow ? SHADOW_REASON : undefined,
            )}
            {menuEntry(
              draft.status === "archived" ? "Restore" : "Archive",
              <Icon.Archive size={12} />,
              () => void toggleArchived(draft),
              false,
              draft.inconsistent === true || shadow,
              shadow ? SHADOW_REASON : undefined,
            )}
            {menuEntry(
              "Delete",
              <Icon.X size={12} />,
              () => openOverlay({ kind: "draft-delete", id: draft.id }),
              true,
              shadow,
              shadow ? SHADOW_REASON : undefined,
            )}
            {/* DRP-FR-35: the route to the run a graduation holds this row
                for, below a divider so it reads apart from the entries that
                act on the draft itself. */}
            {draft.graduation && (
              <>
                <div className="menu-sep" />
                {menuEntry("Go to run", <Icon.Graduate size={12} />, () => {
                  setOverlay(null);
                  onOpenRun?.(draft.graduation!.runId);
                })}
              </>
            )}
          </AnchoredMenu>
        )}
      </div>
    );
  };

  const renderFolder = (folder: DraftFolder, depth: number): React.ReactNode => {
    const key = `folder:${folder.path}`;
    const name = folderName(folder.path);
    const item: TreeItem = { kind: "folder", path: folder.path };
    const expanded = isExpanded(folder.path);
    const renaming =
      inline?.kind === "rename-folder" && inline.path === folder.path;
    const locked = busy.has(key);
    const rowError = rowErrors.get(key);
    const refusal = dragging ? moveRefusal(dragging, folder.path) : null;
    const isTarget = dropTarget === folder.path && refusal === null;

    const kids = (childFolders.get(folder.path) ?? []).filter((f) =>
      folderVisible(f.path),
    );
    const rows = (draftsByFolder.get(folder.path) ?? []).filter(admitsDraft);

    return (
      <div className="drafts-tree__group" key={key}>
        <div
          className={`drafts-tree__folder${isTarget ? " drafts-tree__folder--drop" : ""}`}
          data-row-key={key}
          data-drop-target={isTarget ? "true" : undefined}
          role="treeitem"
          aria-level={depth + 1}
          aria-expanded={expanded}
          aria-selected={selected === key}
          aria-label={`Folder ${name}`}
          tabIndex={0}
          draggable={!locked && !renaming}
          style={{ paddingLeft: depth * 12 }}
          onFocus={() => setSelected(key)}
          onClick={() => toggleFolder(folder.path)}
          onContextMenu={(e) => {
            e.preventDefault();
            e.stopPropagation();
            if (!locked)
              openRowMenu(
                e,
                { kind: "folder-menu", path: folder.path },
                e.currentTarget,
              );
          }}
          onKeyDown={(e) => {
            if (isMenuKey(e)) {
              if (!locked)
                openRowMenu(
                  e,
                  { kind: "folder-menu", path: folder.path },
                  e.currentTarget,
                );
              return;
            }
            if (e.key === "Enter" || e.key === " ") {
              e.preventDefault();
              toggleFolder(folder.path);
            }
            if (e.key === "ArrowRight" && !expanded) toggleFolder(folder.path);
            if (e.key === "ArrowLeft" && expanded) toggleFolder(folder.path);
            if (e.key === "ArrowDown" || e.key === "ArrowUp") {
              e.preventDefault();
              moveFocusBy(e.currentTarget, e.key === "ArrowDown" ? 1 : -1);
            }
          }}
          onDragStart={(e) => {
            if (e.dataTransfer) {
              e.dataTransfer.effectAllowed = "move";
              e.dataTransfer.setData("text/plain", folder.path);
            }
            setDragging(item);
          }}
          onDragEnd={() => {
            setDragging(null);
            setDropTarget(null);
          }}
          {...dropHandlers(folder.path)}
        >
          <span className="drafts-tree__caret" aria-hidden="true">
            {expanded ? <Icon.Caret size={10} /> : <Icon.CaretRight size={10} />}
          </span>
          <span className="drafts-tree__icon" aria-hidden="true">
            {expanded ? <Icon.FolderOpen size={12} /> : <Icon.Folder size={12} />}
          </span>
          {renaming ? (
            inlineField("Folder name", closeInline)
          ) : (
            <span className="drafts-tree__name" title={name}>
              {name}
            </span>
          )}
        </div>

        {rowError && (
          <p
            className="t-ui-xs drafts-tree__error"
            role="alert"
            style={{ paddingLeft: depth * 12 + 14 }}
          >
            {rowError}
          </p>
        )}

        {/* DRP-FR-22: the two creation entries lead the menu as one cluster,
            separated by a divider from the three that act on the folder itself
            — exactly as the Library's creation cluster leads its own. */}
        {overlay?.kind === "folder-menu" && overlay.path === folder.path && (
          <AnchoredMenu anchor={anchor} label={`Folder actions for ${name}`}>
            {menuEntry("New Draft", <Icon.Diamond size={12} />, () =>
              void doNewDraft(folder.path),
            )}
            {menuEntry("New Folder", <Icon.Folder size={12} />, () => {
              setOverlay(null);
              doNewFolder(folder.path);
            })}
            <div className="menu-sep" />
            {menuEntry("Rename", <Icon.Tag size={12} />, () =>
              openInline({ kind: "rename-folder", path: folder.path }, name),
            )}
            {menuEntry("Move to Folder…", <Icon.FolderOpen size={12} />, () =>
              openOverlay({ kind: "move", item }),
            )}
            {menuEntry(
              "Delete",
              <Icon.X size={12} />,
              () => openOverlay({ kind: "folder-delete", path: folder.path }),
              true,
            )}
          </AnchoredMenu>
        )}

        {expanded && (
          // DRP-FR-27: the whole of a folder's own region is its drop target,
          // not the one line its name sits on. A folder just created is empty,
          // so the only thing under the pointer is the affordance saying so —
          // and an item released there would otherwise bubble to the panel
          // background and be read as a drop on the implicit root. Rows inside
          // stop propagation, so a nested folder still wins over its parent.
          <div className="drafts-tree__children" role="group" {...dropHandlers(folder.path)}>
            {inline?.kind === "new-folder" && inline.parent === folder.path && (
              <div
                className="drafts-tree__folder"
                style={{ paddingLeft: (depth + 1) * 12 }}
              >
                <span className="drafts-tree__caret" aria-hidden="true">
                  <Icon.CaretRight size={10} />
                </span>
                <span className="drafts-tree__icon" aria-hidden="true">
                  <Icon.Folder size={12} />
                </span>
                {inlineField("New folder name", closeInline)}
              </div>
            )}
            {kids.map((child) => renderFolder(child, depth + 1))}
            {rows.map((draft) => renderDraft(draft, depth + 1))}
            {/* DRP-FR-29: a folder holding nothing the current filters admit
                still renders, carrying a short line in place of its children
                rather than disappearing. */}
            {kids.length === 0 &&
              rows.length === 0 &&
              !(inline?.kind === "new-folder" && inline.parent === folder.path) && (
                <p
                  className="t-ui-xs drafts-tree__empty"
                  style={{ paddingLeft: (depth + 1) * 12 }}
                >
                  Nothing here yet.
                </p>
              )}
          </div>
        )}
      </div>
    );
  };

  return { menuEntry, inlineField, renderDraft, renderFolder };
}

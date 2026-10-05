import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent, MouseEvent, RefObject } from "react";
import * as api from "../../api";
import { logDebug, logInfo, logWarn } from "../../logging";
import type { BranchDeletionPlan, GitBranch } from "../../types";
import { Icon } from "../icons";
import { BranchDeleteDialog } from "./BranchDeleteDialog";
import type { DeletionResult } from "./BranchDeleteDialog";
import { BranchInfoOverlay } from "./BranchInfoOverlay";
import { BranchMenu } from "./BranchMenu";
import { parseRejection, rejectionMessage } from "./errors";
import { RegionNote, SelectedMark } from "./parts";

export type BranchKind = "local" | "remote";

const rowKeyOf = (b: GitBranch) => `${b.kind}:${b.name}`;

type Overlay =
  | { type: "info"; branch: GitBranch }
  | { type: "delete"; plan: BranchDeletionPlan; key: string }
  | null;

interface MenuState {
  branch: GitBranch;
  x: number;
  y: number;
}

/** GIT-FR-UDKY: the success message of a deletion, in words. */
function ResultNote({
  result,
  onDismiss,
  noteRef,
}: {
  result: DeletionResult;
  onDismiss: () => void;
  noteRef: RefObject<HTMLDivElement | null>;
}) {
  const remote = result.remote;
  return (
    <div
      className="git-result"
      role="status"
      tabIndex={-1}
      ref={noteRef}
      data-testid="git-delete-result"
    >
      <div className="git-result__lines">
        <div>
          {result.streamName
            ? `Removed work stream ${result.streamName} with its branch ${result.branch}.`
            : `Removed local branch ${result.branch}.`}
        </div>
        <div>
          {result.removedWorktreePath
            ? `Removed worktree ${result.removedWorktreePath}.`
            : "No worktree was linked, so none was removed."}
        </div>
        <div data-testid="git-delete-result-remote">
          {remote.state === "deleted"
            ? `Remote branch ${remote.branch ?? ""}: deleted.`
            : remote.state === "failed"
              ? `Remote branch ${remote.branch ?? ""}: FAILED. It is still on the remote.${
                  remote.error ? ` ${remote.error}` : ""
                }`
              : "Remote branch: not requested."}
        </div>
      </div>
      <button
        type="button"
        className="btn btn--ghost btn--sm"
        onClick={onDismiss}
      >
        Dismiss
      </button>
    </div>
  );
}

/**
 * The Branches rail (GIT-FR-HLGO through GIT-FR-UDKY).
 *
 * A row's click, Enter or Space selects the branch and checks nothing out
 * (GIT-FR-LNEI). Right-click, the context-menu key and Shift+F10 open the row's
 * menu, which offers **Check out** where the active worktree allows it
 * (GIT-FR-04, GIT-FR-ZEKI). One overlay at a time is open (GIT-FR-VCDG), and a
 * running request disables only its own row (GIT-FR-UDKY, GIT-FR-EPSV).
 */
export function GitBranches({
  branches,
  listError,
  actionError,
  kind,
  onKindChange,
  canCheckOutBranches,
  selectedRowKey,
  selectionNonce,
  onSelect,
  onCheckout,
  onChanged,
  onRequestGithubToken,
}: {
  branches: GitBranch[] | null;
  listError: string;
  actionError: string;
  kind: BranchKind;
  onKindChange: (kind: BranchKind) => void;
  canCheckOutBranches: boolean;
  selectedRowKey: string | null;
  selectionNonce: number;
  /** GIT-FR-LNEI: the author selected this row. */
  onSelect: (branch: GitBranch) => void;
  /** GIT-FR-04 / GIT-FR-ZEKI: the menu's **Check out**. */
  onCheckout: (branch: GitBranch) => void;
  /** The branch set changed here: reload the listing. */
  onChanged: () => void;
  onRequestGithubToken?: () => Promise<boolean>;
}) {
  const rowRefs = useRef(new Map<string, HTMLElement>());
  const switchRef = useRef<HTMLDivElement | null>(null);
  const resultRef = useRef<HTMLDivElement | null>(null);
  const [menu, setMenu] = useState<MenuState | null>(null);
  const [overlay, setOverlay] = useState<Overlay>(null);
  const [inspecting, setInspecting] = useState<Set<string>>(new Set());
  const [refusals, setRefusals] = useState<Record<string, string>>({});
  const [result, setResult] = useState<DeletionResult | null>(null);
  // Which control gets focus when the overlay closes.
  const restoreKey = useRef<string | null>(null);
  const restoreToResult = useRef(false);
  // Counts every opening of an overlay, so a late inspection never replaces a
  // window the author opened after asking for it.
  const overlayEpoch = useRef(0);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  // GIT-FR-FZMS: the selected row is scrolled into view, and a repeated request
  // scrolls it again after the author scrolled away.
  useEffect(() => {
    if (selectedRowKey === null) return;
    rowRefs.current.get(selectedRowKey)?.scrollIntoView?.({ block: "nearest" });
  }, [selectedRowKey, selectionNonce, kind, branches]);

  const rows = (branches ?? []).filter((b) => b.kind === kind);

  const restoreFocus = (): HTMLElement | null => {
    if (restoreToResult.current && resultRef.current) return resultRef.current;
    const row = restoreKey.current ? rowRefs.current.get(restoreKey.current) : null;
    return (
      row ??
      switchRef.current?.querySelector<HTMLElement>('[aria-checked="true"]') ??
      null
    );
  };

  const openMenu = (branch: GitBranch, x: number, y: number) => {
    restoreKey.current = rowKeyOf(branch);
    restoreToResult.current = false;
    setMenu({ branch, x, y });
  };

  const openMenuAtRow = (branch: GitBranch, el: HTMLElement) => {
    const box = el.getBoundingClientRect();
    openMenu(branch, box.left + 16, box.bottom);
  };

  const closeMenu = () => {
    const key = restoreKey.current;
    setMenu(null);
    if (key) rowRefs.current.get(key)?.focus();
  };

  const closeOverlay = () => {
    overlayEpoch.current += 1;
    setOverlay(null);
  };

  const showInformation = (branch: GitBranch) => {
    overlayEpoch.current += 1;
    logDebug(["frontend"], "branch information opened", { kind: branch.kind });
    setOverlay({ type: "info", branch });
  };

  /**
   * GIT-FR-QYWP, GIT-FR-GAMV: **Delete** reads `inspect branch deletion` first.
   * A refusal shows beside the row and opens no confirmation.
   */
  const startDelete = async (branch: GitBranch) => {
    const key = rowKeyOf(branch);
    if (inspecting.has(key)) return;
    const epoch = ++overlayEpoch.current;
    setRefusals((r) => {
      const { [key]: _drop, ...rest } = r;
      return rest;
    });
    setInspecting((s) => new Set(s).add(key));
    logDebug(["frontend", "backend"], "branch deletion inspection started", {
      branch: branch.name,
    });
    try {
      const plan = await api.inspectBranchDeletion(branch.name);
      if (!alive.current || overlayEpoch.current !== epoch) return;
      setOverlay({ type: "delete", plan, key });
    } catch (e) {
      logWarn(["frontend", "backend"], "branch deletion refused at inspection", {
        branch: branch.name,
        code: parseRejection(e).code,
      });
      if (!alive.current) return;
      setRefusals((r) => ({
        ...r,
        [key]: rejectionMessage(e, { branch: branch.name }),
      }));
    } finally {
      if (alive.current)
        setInspecting((s) => {
          const next = new Set(s);
          next.delete(key);
          return next;
        });
    }
  };

  const chooseEntry = (id: string) => {
    const target = menu?.branch;
    setMenu(null);
    if (!target) return;
    if (id === "info") showInformation(target);
    else if (id === "checkout") onCheckout(target);
    else if (id === "delete") void startDelete(target);
  };

  const onRowKeyDown = (e: KeyboardEvent<HTMLElement>, b: GitBranch) => {
    if (e.key === "ContextMenu" || (e.shiftKey && e.key === "F10")) {
      e.preventDefault();
      openMenuAtRow(b, e.currentTarget);
      return;
    }
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      onSelect(b);
    }
  };

  const onRowContextMenu = (e: MouseEvent<HTMLElement>, b: GitBranch) => {
    e.preventDefault();
    // The context-menu key raises this event too, without a pointer position.
    if (e.clientX === 0 && e.clientY === 0) openMenuAtRow(b, e.currentTarget);
    else openMenu(b, e.clientX, e.clientY);
  };

  const kinds: [BranchKind, string][] = [
    ["local", "Local"],
    ["remote", "Remote"],
  ];

  return (
    <div style={{ padding: "4px 0" }} data-testid="git-branches">
      <div
        className="git__switch"
        role="radiogroup"
        aria-label="Branch kind"
        ref={switchRef}
        onKeyDown={(e) => {
          if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
            e.preventDefault();
            const next: BranchKind = kind === "local" ? "remote" : "local";
            onKindChange(next);
            requestAnimationFrame(() =>
              switchRef.current
                ?.querySelector<HTMLElement>('[aria-checked="true"]')
                ?.focus(),
            );
          }
        }}
      >
        {kinds.map(([k, label]) => (
          <button
            key={k}
            type="button"
            role="radio"
            className="btn btn--sm git__switch-option"
            aria-checked={kind === k}
            data-active={kind === k}
            tabIndex={kind === k ? 0 : -1}
            onClick={() => onKindChange(k)}
          >
            {label}
          </button>
        ))}
      </div>

      {result && (
        <ResultNote
          result={result}
          noteRef={resultRef}
          onDismiss={() => {
            setResult(null);
            restoreToResult.current = false;
          }}
        />
      )}
      {branches === null && <div className="git__section-head">LOADING…</div>}
      {listError && (
        <div className="proj-switch__error" role="alert" data-testid="git-branch-error">
          ✗ {listError}
        </div>
      )}
      {actionError && (
        <div className="proj-switch__error" role="alert" data-testid="git-branch-error">
          ✗ {actionError}
        </div>
      )}
      {branches !== null && !listError && rows.length === 0 && (
        <RegionNote kind="empty" testId="git-branches-empty">
          {kind === "local" ? "No local branches." : "No remote branches."}
        </RegionNote>
      )}
      {rows.map((b) => {
        const key = rowKeyOf(b);
        const selected = key === selectedRowKey;
        const checking = inspecting.has(key);
        return (
          <div key={key} className="git-branch">
            <div
              ref={(el) => {
                if (el) rowRefs.current.set(key, el);
                else rowRefs.current.delete(key);
              }}
              className="git__file"
              data-selected={selected || undefined}
              aria-current={selected ? "true" : undefined}
              data-testid={selected ? "git-branch-selected" : undefined}
              role="button"
              tabIndex={0}
              aria-haspopup="menu"
              title={b.isCurrent ? `${b.name} (current)` : b.name}
              onClick={() => onSelect(b)}
              onKeyDown={(e) => onRowKeyDown(e, b)}
              onContextMenu={(e) => onRowContextMenu(e, b)}
            >
              <Icon.Branch size={12} />
              <span className="git__file-name">{b.name}</span>
              {b.isCurrent && <span className="badge badge--accent">current</span>}
              {selected && <SelectedMark />}
              {checking && (
                <span className="t-meta" role="status">
                  Checking…
                </span>
              )}
            </div>
            {refusals[key] && (
              <div
                className="proj-switch__error git-branch__refusal"
                role="alert"
                data-testid="git-branch-refusal"
              >
                {refusals[key]}
              </div>
            )}
          </div>
        );
      })}

      {menu && (
        <BranchMenu
          x={menu.x}
          y={menu.y}
          label={`Actions for ${menu.branch.name}`}
          entries={[
            { id: "info", label: "Information", icon: <Icon.Doc size={14} /> },
            // GIT-FR-ZEKI / GIT-FR-04: checkout is offered from the primary
            // worktree alone (WTC-FR-21), and never for the current branch.
            ...(canCheckOutBranches && !menu.branch.isCurrent
              ? [
                  {
                    id: "checkout",
                    label: "Check out",
                    icon: <Icon.Branch size={14} />,
                  },
                ]
              : []),
            ...(menu.branch.kind === "local"
              ? [
                  {
                    id: "delete",
                    label: "Delete",
                    icon: <Icon.Trash size={14} />,
                    danger: true,
                  },
                ]
              : []),
          ]}
          onChoose={chooseEntry}
          onClose={closeMenu}
        />
      )}
      {overlay?.type === "info" && (
        <BranchInfoOverlay
          name={overlay.branch.name}
          kind={overlay.branch.kind}
          onClose={closeOverlay}
          restoreFocus={restoreFocus}
        />
      )}
      {overlay?.type === "delete" && (
        <BranchDeleteDialog
          plan={overlay.plan}
          onClose={closeOverlay}
          restoreFocus={restoreFocus}
          onRequestGithubToken={onRequestGithubToken}
          onDeleted={(done) => {
            logInfo(["frontend", "backend"], "branch deleted", {
              remote: done.remote.state,
              stream: done.streamName !== undefined,
            });
            restoreToResult.current = true;
            setResult(done);
            closeOverlay();
            onChanged();
          }}
        />
      )}
    </div>
  );
}

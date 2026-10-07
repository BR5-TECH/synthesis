import { useEffect, useMemo, useState } from "react";
import * as api from "../api";
import { useGitTransfer } from "../hooks/useGitTransfer";
import { logDebug, logWarn } from "../logging";
import { tokenErrorMessage } from "./GithubTokens";
import { GitBranches } from "./Git/GitBranches";
import type { BranchKind } from "./Git/GitBranches";
import { BranchView } from "./Git/GitBranchView";
import { LogRail, LogView } from "./Git/GitLog";
import { LogsView } from "./Git/GitLogsView";
import { PullRequestRail, PullRequestView } from "./Git/GitPullRequests";
import { useBranchCompare } from "./Git/useBranchCompare";
import type { BranchRef } from "./Git/useBranchCompare";
import { useBranchListing } from "./Git/useBranchListing";
import { CurrentBranchButton } from "./CreatePullRequest/CurrentBranchButton";
import type { PullRequestSource } from "./CreatePullRequest/types";
import { useGitLog } from "./Git/useGitLog";
import { usePullRequests } from "./Git/usePullRequests";
import type { GitBranch, WorktreeContext } from "../types";
import type { SwitchOutcome } from "./WorktreeSelector";
import {
  ReadyTasks,
  ReadyTasksSummary,
  readyTaskCount,
  type ReadyTasksBinding,
} from "./GitReadyTasks";

// GIT-FR-02: the panel's five sections. There is no working-tree section:
// authoring a commit belongs to the Changes panel and the commit message window
// it opens (GIT-FR-08 / CHG-FR-21). GIT-FR-OGHO: Ready tasks is a section of
// this panel and adds no bottom-panel surface. GIT-FR-PZIE: Logs holds the
// push/pull output area.
type GitTab = "commits" | "branches" | "prs" | "ready" | "logs";

interface GitProps {
  // GIT-FR-06: a checkout from the branches section is the same operation the
  // top-chrome worktree selector performs in place, so it is routed through the
  // one worktree-switch transition rather than invoking a checkout directly.
  onSwitchWorktree: (
    operation: () => Promise<WorktreeContext>,
  ) => Promise<SwitchOutcome>;
  // GIT-FR-06 / WTC-FR-21: a branch is checked out only in the repository's
  // primary worktree. False from a linked one, where the section stays a
  // read-only view of the repository's branches.
  canCheckOutBranches: boolean;
  /**
   * GHA-FR-16: open the GitHub token picker for an operation that reported it
   * needs one selected, and resolve `true` once a token has been bound or
   * `false` if the author cancelled — in which case the operation is abandoned
   * (GHA-FR-17).
   */
  onRequestGithubToken?: () => Promise<boolean>;
  /** GIT-FR-05: open the Create a PR window for the current branch. */
  onCreatePullRequest?: (source: PullRequestSource) => void;
  /** GIT-FR-OGHO: the window's GitHub polling view and its actions. */
  readyTasks?: ReadyTasksBinding;
  /** GIT-FR-OZYT: open a draft's New Artifact tab. */
  onOpenDraft?: (draftId: string) => void;
  /**
   * GIT-FR-FZMS: a request to select a branch in the branches section, made from
   * a status bar row that names a push. A nonce, so a second request for the
   * same branch selects again.
   */
  selectBranch?: { branch: string; nonce: number } | null;
  /** GIT-FR-FZMS: the panel has taken the request. */
  onBranchSelected?: () => void;
}

/**
 * The Git panel (`../specifications/ui/GIT-git.md`).
 *
 * GIT-FR-06: the branches section lists the repository's branches whichever
 * worktree is active — describing a branch is not offering to check it out —
 * but offers checkout only from the primary worktree.
 *
 * GIT-FR-11: every section describes the project's **active worktree**, never
 * another worktree of the same repository. The panel is mounted inside the
 * shell subtree keyed on the active worktree, so a switch remounts it and every
 * section reloads against the new content root with nothing — including the
 * inline diff — carried over.
 */
export function Git({
  onSwitchWorktree,
  canCheckOutBranches,
  onRequestGithubToken,
  onCreatePullRequest,
  readyTasks,
  onOpenDraft,
  selectBranch,
  onBranchSelected,
}: GitProps) {
  const [tab, setTab] = useState<GitTab>("commits");
  // GIT-FR-LNEI / GIT-FR-FZMS: the selected branch. A row the author activated
  // names its kind; a status bar request names the branch alone, and the row
  // is resolved from the listing, local first.
  const [selection, setSelection] = useState<{
    name: string;
    kind?: BranchKind;
  } | null>(null);
  // Which request set it, so a repeated request for the same branch scrolls the
  // row into view again after the author scrolled away.
  const [selectionNonce, setSelectionNonce] = useState(0);
  const requestNonce = selectBranch?.nonce;
  useEffect(() => {
    if (!selectBranch) return;
    // GIT-FR-FZMS: the request shows the push/pull output area, and selects
    // the branch for when the author opens the branches section. The selection
    // is applied once the listing holds the branch.
    setTab("logs");
    setSelection({ name: selectBranch.branch });
    setSelectionNonce(selectBranch.nonce);
    logDebug(["frontend"], "the git panel took a branch selection request", {
      nonce: selectBranch.nonce,
    });
    onBranchSelected?.();
    // The request is identified by its nonce.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [requestNonce]);

  /**
   * GIT-FR-PZIE / GIT-FR-QMYB: the push/pull output area, the running state, and
   * the branch's standing against its upstream all belong to the main window
   * rather than to this panel, so a push started from the top chrome or from
   * the Changes panel while this panel is closed is already in the area when it
   * opens. One reading of the standing (CHG-FR-37) serves every Push control, so
   * none offers a push another refuses. No line carries a token secret, and a
   * remote URL echoed by the transport arrives with its credential redacted
   * (GTC-FR-11).
   */
  const transfer = useGitTransfer();
  const { sync, lines: transferLines, running: transferring } = transfer;

  // GIT-FR-06 / GTC-FR-07: the branches section is real. The listing loads once
  // the section is first shown and reloads on remount, which a worktree switch
  // forces, so `current` always names the active worktree's branch. It also
  // follows `"branches changed"` (GIT-FR-11), which reloads this listing alone.
  const {
    branches,
    listError,
    reload: reloadBranches,
    current: listingCurrent,
  } = useBranchListing(
    tab === "branches",
  );
  const [branchError, setBranchError] = useState("");
  const [busy, setBusy] = useState(false);
  // GIT-FR-HLGO: the Local / Remote switch. Local is where the section opens.
  const [branchKind, setBranchKind] = useState<BranchKind>("local");

  // GIT-FR-KPTE through GIT-FR-TFAU and GIT-FR-OBZW through GIT-FR-LKRX: the
  // Log and PRs sections. Their state lives here so it survives a look at
  // another section, and is discarded with the panel when the worktree changes.
  const log = useGitLog(tab === "commits");
  const pullRequests = usePullRequests(tab === "prs", onRequestGithubToken);

  /**
   * GIT-FR-06: checking out from here drives the identical switch transition as
   * the selector — pending changes written, every tab closed, the viewport
   * returned to its fresh state — and the selector's label follows it
   * (WTS-FR-25), because both routes call one operation.
   */
  const onCheckout = async (branch: GitBranch) => {
    // WTC-FR-21: nothing is checked out from a linked worktree, so the rows are
    // labels there. The backend refuses too — this is what keeps the panel from
    // offering an action it would be told it cannot have.
    if (!canCheckOutBranches || branch.isCurrent || busy) return;
    setBusy(true);
    setBranchError("");
    try {
      const outcome = await onSwitchWorktree(() =>
        api.checkOutBranchInActiveWorktree(branch.name),
      );
      // A cancelled flush has already focused the blocking tab; only a typed
      // refusal is the panel's to render.
      if (!outcome.ok && !outcome.cancelled) {
        logWarn(["frontend", "backend"], "branch checkout refused", {
          kind: branch.kind,
        });
        setBranchError(outcome.error);
      }
    } finally {
      setBusy(false);
    }
  };

  /**
   * GHA-FR-16: an operation that reaches GitHub needs the project to resolve a
   * token first, and the two ways it can fail to are answered differently.
   *
   * `selection_required` — tokens are stored but this project has not been
   * pointed at one — opens the picker, and the operation proceeds on confirm or
   * is abandoned on cancel. `none_stored` renders inline with a route to the
   * Global settings GitHub section, because there is nothing to choose between
   * (GHA-FR-19). Anything else resolves a token and the operation runs.
   */
  const [authNote, setAuthNote] = useState("");
  const runAuthenticated = async (label: string, run: () => void) => {
    setAuthNote("");
    let binding;
    try {
      binding = await api.getProjectGithubTokenBinding();
    } catch (e) {
      // A typed rejection reads as a sentence rather than as its wire slug.
      setAuthNote(tokenErrorMessage(e));
      return;
    }
    if (binding.resolution === "none_stored") {
      setAuthNote(
        `${label} needs a GitHub token. Add one in Global settings → GitHub.`,
      );
      return;
    }
    if (binding.resolution === "selection_required") {
      const chosen = (await onRequestGithubToken?.()) ?? false;
      if (!chosen) {
        setAuthNote(`${label} cancelled — no GitHub token was selected.`);
        return;
      }
    }
    run();
  };

  /**
   * GIT-FR-PZIE / GTC-FR-22: the panel's own push. Its output streams into the
   * area below through the same channels a push started anywhere else emits on,
   * so the two are indistinguishable once they land there.
   */
  const runPush = () => {
    setAuthNote("");
    void transfer.pushBranch().then((outcome) => {
      // The terminal event already appends the failure to the transcript; this
      // is the note beside the controls, in the panel's own vocabulary.
      if (outcome.ok || outcome.cause === "busy") return;
      setAuthNote(`Push failed: ${tokenErrorMessage(outcome.error)}`);
    });
  };

  // GIT-FR-LNEI / GIT-FR-FZMS: the one selected row — the row the author
  // activated, or for a request the local branch of that name, else the
  // remote-tracking one. No row when the listing holds neither.
  const selectedRow = (() => {
    if (selection === null || branches === null) return null;
    const named = (kind: BranchKind) =>
      branches.find((b) => b.kind === kind && b.name === selection.name);
    if (selection.kind) return named(selection.kind) ?? null;
    return named("local") ?? named("remote") ?? null;
  })();
  const selectedRowKey = selectedRow
    ? `${selectedRow.kind}:${selectedRow.name}`
    : null;
  // GIT-FR-LNEI: a reloaded listing that no longer holds the selected branch
  // clears the selection and its view. Only a current listing on show can say
  // so: a listing that is due a reload may not hold a branch a request named
  // since it was read (GIT-FR-FZMS).
  useEffect(() => {
    if (
      tab === "branches" &&
      listingCurrent &&
      !listError &&
      selection !== null &&
      branches !== null &&
      selectedRow === null
    ) {
      logDebug(["frontend"], "the selected branch left the listing");
      setSelection(null);
    }
  }, [tab, listingCurrent, listError, selection, branches, selectedRow]);
  const selectedRef = useMemo<BranchRef | null>(
    () => (selectedRow ? { name: selectedRow.name, kind: selectedRow.kind } : null),
    // The row is identified by its key.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [selectedRowKey],
  );
  // GIT-FR-GDMG / GIT-FR-SION: what the selected branch changed against its
  // base. Kept in the shell, so it survives a look at another section.
  const compare = useBranchCompare(selectedRef);
  // GIT-FR-HLGO: a selection request sets the switch to the kind of the row it
  // selects. Applied when the row exists, so a request that arrives while the
  // listing loads is applied when the listing arrives.
  useEffect(() => {
    if (selectedRowKey === null) return;
    setBranchKind(selectedRowKey.startsWith("remote:") ? "remote" : "local");
    // The kind follows the request, and not the author's later choice.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedRowKey, selectionNonce]);

  const tabs: [GitTab, string, number | null][] = [
    ["commits", "Commits", null],
    ["branches", "Branches", null],
    ["prs", "PRs", null],
  ];
  if (readyTasks)
    tabs.push(["ready", "Ready tasks", readyTaskCount(readyTasks.polling.view)]);
  tabs.push(["logs", "Logs", null]);

  return (
    <div className="git">
      <div className="git__left">
        {/* GIT-FR-WQHD: the five controls wrap onto further rows at the
            narrowest width. They never clip, truncate, or scroll sideways. */}
        <div className="git__tabs" role="tablist" aria-label="Git sections">
          {tabs.map(([k, label, badge]) => (
            <div
              key={k}
              className="bottom-tab git__tab"
              role="tab"
              tabIndex={0}
              aria-selected={tab === k}
              data-active={tab === k}
              onClick={() => setTab(k)}
              onKeyDown={(e) => {
                // GIT-FR-RYPO: every section is reachable by keyboard.
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  setTab(k);
                }
              }}
            >
              {label}
              {badge != null && (
                <span className="badge badge--accent" style={{ marginLeft: 4 }}>
                  {badge}
                </span>
              )}
            </div>
          ))}
        </div>

        {tab === "commits" && <LogRail log={log} />}

        {tab === "logs" && (
          <div className="git__state" data-testid="git-logs-rail">
            Every push and pull you start lands here, whichever control started
            it.
          </div>
        )}

        {tab === "branches" && (
          <GitBranches
            branches={branches}
            listError={listError}
            actionError={branchError}
            kind={branchKind}
            onKindChange={setBranchKind}
            canCheckOutBranches={canCheckOutBranches}
            selectedRowKey={selectedRowKey}
            selectionNonce={selectionNonce}
            onSelect={(b) => {
              logDebug(["frontend"], "branch selected", { kind: b.kind });
              setSelection({ name: b.name, kind: b.kind });
            }}
            onCheckout={(b) => void onCheckout(b)}
            onChanged={reloadBranches}
            onRequestGithubToken={onRequestGithubToken}
          />
        )}

        {tab === "ready" && readyTasks && (
          <ReadyTasksSummary polling={readyTasks.polling} />
        )}

        {tab === "prs" && (
          <>
            <div style={{ padding: "6px 12px" }}>
              {/* GIT-FR-05 / GIT-FR-GZUM: opens the shared Create a PR window,
                  which answers a missing or unselected GitHub token itself. */}
              <CurrentBranchButton
                label="Create PR for current branch"
                className="btn btn--default btn--sm"
                withIcon
                testId="git-create-pr"
                onCreatePullRequest={onCreatePullRequest}
              />
            </div>
            <PullRequestRail pr={pullRequests} />
          </>
        )}
      </div>

      <div className="git__right">
        {/* GIT-FR-03: the commit log's files and diff. GIT-FR-GDMG: the
            selected branch against its base. GIT-FR-PZIE: the push/pull output
            area. GIT-FR-FNQA: a pull request's description and timeline. All
            render in the panel's wide column,
            which is what keeps line-shaped output scrollable within the panel
            rather than overflowing into the main viewport (GIT-FR-06). */}
        {tab === "ready" && readyTasks ? (
          <ReadyTasks {...readyTasks} onOpenDraft={onOpenDraft} />
        ) : tab === "commits" ? (
          <LogView log={log} />
        ) : tab === "prs" ? (
          <PullRequestView pr={pullRequests} />
        ) : tab === "logs" ? (
          <LogsView
            sync={sync}
            transferring={transferring}
            lines={transferLines}
            authNote={authNote}
            onPull={() =>
              void runAuthenticated("Pull", () =>
                setAuthNote("Pull is not wired to a remote in this build."),
              )
            }
            onPush={() => void runAuthenticated("Push", runPush)}
          />
        ) : (
          <BranchView branch={selectedRef} compare={compare} />
        )}
      </div>
    </div>
  );
}

import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../api";
import { logDebug, logError, logInfo } from "../logging";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import {
  applyPickerWindowChrome,
  keepPickerWindowSize,
} from "./pickerWindowChrome";

import { Icon } from "./icons";
import { AppIcon } from "./AppIcon";
import {
  RecentProjectMenu,
  type RecentMenuAction,
  type RecentMenuState,
} from "./RecentProjectMenu";
import type {
  CreateMode,
  ProjectHandle,
  RecentProject,
} from "../types";

interface ProjectPickerProps {
  onOpen: (handle: ProjectHandle) => void;
  /**
   * PJT-FR-IOEV: resolve the chosen project without mounting it. The host asks
   * where it opens (This tab or New tab) and mounts it itself.
   */
  resolveOnly?: boolean;
  /**
   * PPK-FR-GMYH: `modal` is the picker over a project window. It keeps the
   * window as it is: no fixed size, no centring.
   */
  presentation?: "window" | "modal";
}

export { PICKER_WINDOW_HEIGHT, PICKER_WINDOW_WIDTH } from "./pickerWindowChrome";

export function formatRelative(iso: string, now: Date = new Date()): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const deltaSec = Math.max(0, Math.floor((now.getTime() - t) / 1000));
  if (deltaSec < 60) return "just now";
  const m = Math.floor(deltaSec / 60);
  if (m < 60) return `${m}m ago`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h ago`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d}d ago`;
  const w = Math.floor(d / 7);
  if (w < 5) return `${w}w ago`;
  const mo = Math.floor(d / 30);
  if (mo < 12) return `${mo}mo ago`;
  const y = Math.floor(d / 365);
  return `${y}y ago`;
}

function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "operation failed";
}

/**
 * PPK-FR-YNME / PPK-FR-MQVU: the existing recent-project operation behind each
 * menu entry. **Hide** is "remove recent project" (`GSS-FR-08`) under the label
 * the user sees. Each entry reads `api` when it runs, not when the module
 * loads, so that a partial mock of `api` in another test does not break.
 */
const ENTRY_OPERATIONS: Record<RecentMenuAction, (path: string) => Promise<void>> = {
  pin: (path) => api.pinRecentProject(path),
  unpin: (path) => api.unpinRecentProject(path),
  hide: (path) => api.removeRecentProject(path),
};

/** The keyboard routes to a context menu (PPK-FR-SDXU). */
function isContextMenuKey(e: React.KeyboardEvent): boolean {
  return e.key === "ContextMenu" || (e.shiftKey && e.key === "F10");
}

export function ProjectPicker({
  onOpen,
  resolveOnly = false,
  presentation = "window",
}: ProjectPickerProps) {
  const [recents, setRecents] = useState<RecentProject[] | null>(null);
  const [recentsError, setRecentsError] = useState("");

  const [git, setGit] = useState("");
  const [gitError, setGitError] = useState("");

  const [browseError, setBrowseError] = useState("");
  const [recentError, setRecentError] = useState<{
    path: string;
    message: string;
  } | null>(null);

  const [name, setName] = useState("");
  const [mode, setMode] = useState<CreateMode>("standalone");
  const [createTarget, setCreateTarget] = useState<string | null>(null);
  const [createError, setCreateError] = useState("");

  const [busy, setBusy] = useState(false);

  // PPK-FR-SDXU: the one open recent entry menu, or null (PPK-FR-VFDK).
  const [menu, setMenu] = useState<RecentMenuState | null>(null);
  const menuRef = useRef<RecentMenuState | null>(null);
  menuRef.current = menu;
  // PPK-FR-LJNO: the error of the last failed Pin, Unpin, or Hide.
  const [entryActionError, setEntryActionError] = useState("");
  // PPK-FR-VFDK: where focus goes after the next commit. `path: null` is the
  // first entry, or the list itself when it is empty.
  const [focusAfter, setFocusAfter] = useState<{ path: string | null } | null>(
    null,
  );
  const recentsBodyRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    // PPK-FR-GMYH: the modal leaves the window of its project as it is.
    if (presentation === "modal") return;
    // PPK-FR-09 / PPK-FR-10 / PPK-FR-11: picker uses a fixed, non-resizable window.
    // PPK-FR-TBNQ: after that, keep the fixed size until the picker unmounts.
    let cancelled = false;
    let stopKeeping: (() => void) | undefined;
    void (async () => {
      await applyPickerWindowChrome();
      if (cancelled) return;
      const stop = await keepPickerWindowSize();
      // The effect may have been cleaned up while the subscription was
      // pending; then its cleanup never saw this guard.
      if (cancelled) stop();
      else stopKeeping = stop;
    })();
    return () => {
      cancelled = true;
      stopKeeping?.();
    };
  }, [presentation]);

  useEffect(() => {
    api.listRecentProjects()
      .then(setRecents)
      .catch((e) => {
        setRecents([]);
        setRecentsError(errorMessage(e));
      });
  }, []);

  useEffect(() => {
    if (!focusAfter) return;
    setFocusAfter(null);
    const body = recentsBodyRef.current;
    if (!body) return;
    // A reload ends some time after the choice. When the user has moved focus
    // to another control in that time, focus stays there.
    const active = document.activeElement;
    if (active && active !== document.body && !body.contains(active)) return;
    const rows = Array.from(
      body.querySelectorAll<HTMLElement>("[data-recent-path]"),
    );
    if (focusAfter.path === null) {
      (rows[0] ?? body).focus();
      return;
    }
    rows.find((row) => row.dataset.recentPath === focusAfter.path)?.focus();
  }, [focusAfter, recents]);

  const dismissMenu = useCallback((restoreFocus: boolean) => {
    const open = menuRef.current;
    setMenu(null);
    if (restoreFocus && open) setFocusAfter({ path: open.path });
  }, []);

  // PPK-FR-YNME / PPK-FR-MQVU / PPK-FR-LJNO: run the entry operation, then
  // reload the list so the ordering and pruning of "list recent projects"
  // apply. When either call fails, the shown list stays unchanged.
  const runEntryAction = async (action: RecentMenuAction, path: string) => {
    logDebug(["frontend"], "recent project action", { action, path });
    let stage: "operation" | "reload" = "operation";
    try {
      await ENTRY_OPERATIONS[action](path);
      stage = "reload";
      const next = await api.listRecentProjects();
      setRecents(next);
      setEntryActionError("");
      setFocusAfter({ path: action === "hide" ? null : path });
      logInfo(["frontend"], "recent project action done", {
        action,
        path,
        entries: next.length,
      });
    } catch (e) {
      const message = errorMessage(e);
      logError(["frontend"], "recent project action failed", {
        action,
        path,
        stage,
        error: message,
      });
      setEntryActionError(
        stage === "operation"
          ? `Could not ${action} project: ${message}`
          : `Could not reload recent projects: ${message}`,
      );
      setFocusAfter({ path });
    }
  };

  // PPK-FR-SDXU / PPK-FR-VEID: open the menu of one entry. A pointer opening
  // gives the pointer. A keyboard opening, or a `contextmenu` event that a
  // key made (it has no pointer position), gives a point on the entry.
  const openMenu = (
    e: React.MouseEvent | React.KeyboardEvent,
    path: string,
  ) => {
    e.preventDefault();
    // While a project opens, the picker is about to go away (PPK-FR-06), and
    // a click on an entry does nothing. The menu does not open either.
    if (busy) return;
    const atPointer =
      "clientX" in e && (e.clientX !== 0 || e.clientY !== 0);
    const box = e.currentTarget.getBoundingClientRect();
    setMenu(
      atPointer
        ? { path, x: e.clientX, y: e.clientY }
        : { path, x: box.left + 24, y: box.bottom, flipY: box.top },
    );
  };

  const chooseMenuEntry = (action: RecentMenuAction) => {
    const open = menuRef.current;
    if (!open) return;
    setMenu(null);
    // PPK-FR-VFDK: Pin and Unpin keep the entry, so focus returns to it now and
    // again when the reloaded list moves it. Hide removes the entry, so focus
    // waits for the reloaded list.
    if (action !== "hide") setFocusAfter({ path: open.path });
    void runEntryAction(action, open.path);
  };

  const menuEntry = menu
    ? recents?.find((r) => r.path === menu.path)
    : undefined;

  // A menu whose entry left the list, or a menu open when a project starts to
  // open, is dropped rather than left in state.
  useEffect(() => {
    if (menu && (busy || !menuEntry)) setMenu(null);
  }, [menu, menuEntry, busy]);

  const runOpen = async (
    op: () => Promise<ProjectHandle>,
    setErr: (s: string) => void,
  ): Promise<ProjectHandle | null> => {
    setErr("");
    setBusy(true);
    let handle: ProjectHandle | null = null;
    try {
      handle = await op();
    } catch (e) {
      setErr(errorMessage(e));
    } finally {
      setBusy(false);
    }
    if (handle) onOpen(handle);
    return handle;
  };

  const onRecentClick = (r: RecentProject) =>
    runOpen(() => (resolveOnly ? api.resolveProjectAtPath(r.path) : api.openProjectAtPath(r.path)), (message) =>
      setRecentError(message ? { path: r.path, message } : null),
    );

  const onBrowse = async () => {
    setBrowseError("");
    try {
      const picked = await openDialog({ directory: true, multiple: false });
      if (typeof picked !== "string") return;
      await runOpen(
        () => (resolveOnly ? api.resolveProjectAtPath(picked) : api.openProjectAtPath(picked)),
        setBrowseError,
      );
    } catch (e) {
      setBrowseError(errorMessage(e));
    }
  };

  const onGitOpen = () => {
    if (!/^(git@|https?:\/\/)/.test(git.trim())) {
      setGitError("URL is not parsable");
      return;
    }
    runOpen(
      () =>
        resolveOnly
          ? api.resolveProjectFromGitUrl(git.trim())
          : api.openProjectFromGitUrl(git.trim()),
      setGitError,
    );
  };

  const onCreateBrowse = async () => {
    setCreateError("");
    try {
      const picked = await openDialog({ directory: true, multiple: false });
      if (typeof picked === "string") setCreateTarget(picked);
    } catch (e) {
      setCreateError(errorMessage(e));
    }
  };

  const onCreate = () => {
    if (!name.trim()) {
      setCreateError("Name is required");
      return;
    }
    if (!createTarget) {
      setCreateError("Target folder is required");
      return;
    }
    runOpen(
      () =>
        resolveOnly
          ? api.resolveNewProject(name.trim(), mode, createTarget)
          : api.createProject(name.trim(), mode, createTarget),
      setCreateError,
    );
  };

  return (
    <div className="picker">
      <div className="picker-frame">
        <div className="picker-head">
          <AppIcon size={32} />
          <span className="t-display" style={{ fontSize: "calc(var(--font-ui-size) * 24 / 13)" }}>
            Synthesis
          </span>
          <span
            className="t-ui-sm t-muted"
            style={{ marginLeft: "auto", fontFamily: "var(--font-mono)" }}
          >
            v0.1.0
          </span>
        </div>

        <div className="picker-grid">
          <div className="picker-card picker-card--recents" data-testid="picker-left">
            <h3 className="picker-card__title">Recent projects</h3>
            <div
              className="picker-recents-body"
              data-testid="picker-recents-body"
              ref={recentsBodyRef}
              // PPK-FR-VFDK: the focus target after Hide empties the list.
              tabIndex={-1}
              // PPK-FR-VFDK: a scroll of the list closes the menu.
              onScroll={() => {
                if (menuRef.current) setMenu(null);
              }}
            >
              {recents === null && (
                <div className="picker-empty t-muted">Loading…</div>
              )}
              {recents !== null && recents.length === 0 && (
                <div className="picker-empty t-muted">
                  No projects yet.
                  <br />
                  Open one or create your first project →
                </div>
              )}
              {recents !== null &&
                recents.length > 0 &&
                recents.map((r) => (
                  <div key={`${r.name}:${r.path}`}>
                    <div
                      className="picker-recent-row"
                      data-recent-path={r.path}
                      tabIndex={0}
                      aria-haspopup="menu"
                      // PPK-FR-02: a primary click opens the project. A right
                      // click raises no `click`, so it never opens it.
                      onClick={() => !busy && onRecentClick(r)}
                      // A right-button press, or a Ctrl+click (the macOS
                      // right-click), does not focus the entry. Focus can
                      // scroll a partly visible entry into view, and that
                      // scroll closes the menu (PPK-FR-VFDK).
                      onMouseDown={(e) => {
                        if (e.button === 2 || (e.button === 0 && e.ctrlKey))
                          e.preventDefault();
                      }}
                      // PPK-FR-SDXU: a right-click opens the menu.
                      onContextMenu={(e) => openMenu(e, r.path)}
                      // PPK-FR-SDXU: the context-menu key and Shift+F10 open
                      // the menu on the focused entry. PPK-FR-02: Enter and
                      // Space on the focused entry open the project, as a
                      // primary click does.
                      onKeyDown={(e) => {
                        if (isContextMenuKey(e)) {
                          openMenu(e, r.path);
                          return;
                        }
                        if (e.target !== e.currentTarget) return;
                        if (e.key !== "Enter" && e.key !== " ") return;
                        e.preventDefault();
                        if (!busy) onRecentClick(r);
                      }}
                    >
                      <Icon.Folder className="picker-recent-row__icon icon" />
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div className="picker-recent-row__name">
                          {r.name}
                          {/* PPK-FR-13: reflect pinned/missing state from
                              "list recent projects". The entry menu changes
                              it (PPK-FR-SDXU). */}
                          {r.pinned && (
                            <span className="badge" style={{ marginLeft: 6 }}>
                              pinned
                            </span>
                          )}
                          {r.missing && (
                            <span
                              className="badge badge--warn"
                              style={{ marginLeft: 6 }}
                            >
                              missing
                            </span>
                          )}
                        </div>
                        <div className="picker-recent-row__path">{r.path}</div>
                      </div>
                      <span className="picker-recent-row__meta">
                        {formatRelative(r.lastOpenedAt)}
                      </span>
                    </div>
                    {recentError && recentError.path === r.path && (
                      <span
                        className="picker-error"
                        style={{ paddingLeft: 28 }}
                      >
                        ✗ {recentError.message}
                      </span>
                    )}
                  </div>
                ))}
            </div>
            {recentsError && (
              <span className="picker-error">✗ {recentsError}</span>
            )}
            {entryActionError && (
              <span className="picker-error" role="alert">
                ✗ {entryActionError}
              </span>
            )}
          </div>

          <div className="picker-right" data-testid="picker-right">
            <div className="picker-card">
              <h3 className="picker-card__title">Open</h3>
              <button
                className="btn btn--default"
                style={{ width: "100%", justifyContent: "center" }}
                disabled={busy}
                onClick={onBrowse}
              >
                <Icon.FolderOpen /> Browse folder…
              </button>
              {browseError && (
                <span className="picker-error">✗ {browseError}</span>
              )}
              <div className="picker-or">or</div>
              <div className="picker-field">
                <label className="picker-field__label">Open from Git URL</label>
                <input
                  className="input input--mono"
                  placeholder="git@github.com:org/repo.git"
                  value={git}
                  onChange={(e) => setGit(e.target.value)}
                />
                {gitError && (
                  <span className="picker-error">✗ {gitError}</span>
                )}
              </div>
              <div style={{ display: "flex", justifyContent: "flex-end" }}>
                <button
                  className="btn btn--primary btn--sm"
                  disabled={busy || !git.trim()}
                  onClick={onGitOpen}
                >
                  Open
                </button>
              </div>
            </div>

            <div className="picker-card">
              <h3 className="picker-card__title">Create new project</h3>
              <div className="picker-field">
                <label className="picker-field__label">Name</label>
                <input
                  className="input"
                  value={name}
                  onChange={(e) => setName(e.target.value)}
                  placeholder="my-project"
                />
              </div>
              <div className="picker-field">
                <label className="picker-field__label">Mode</label>
                <div className="picker-radio-row">
                  <label className="picker-radio">
                    <input
                      type="radio"
                      checked={mode === "standalone"}
                      onChange={() => setMode("standalone")}
                    />{" "}
                    Standalone
                  </label>
                  <label className="picker-radio">
                    <input
                      type="radio"
                      checked={mode === "colocated"}
                      onChange={() => setMode("colocated")}
                    />{" "}
                    Co-located
                  </label>
                </div>
              </div>
              <div className="picker-field">
                <label className="picker-field__label">Target folder</label>
                <button
                  className="btn btn--default btn--sm"
                  style={{ width: "fit-content" }}
                  disabled={busy}
                  onClick={onCreateBrowse}
                >
                  <Icon.FolderOpen /> Browse folder…
                </button>
                {createTarget && (
                  <span
                    className="picker-recent-row__path"
                    style={{ marginTop: 4 }}
                  >
                    {createTarget}
                  </span>
                )}
              </div>
              {createError && (
                <span className="picker-error">✗ {createError}</span>
              )}
              <div style={{ display: "flex", justifyContent: "flex-end" }}>
                <button
                  className="btn btn--primary btn--sm"
                  disabled={busy || !name.trim() || !createTarget}
                  onClick={onCreate}
                >
                  Create
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
      {menu && menuEntry && (
        <RecentProjectMenu
          key={menu.path}
          menu={menu}
          name={menuEntry.name}
          pinned={!!menuEntry.pinned}
          onChoose={chooseMenuEntry}
          onDismiss={dismissMenu}
        />
      )}
    </div>
  );
}

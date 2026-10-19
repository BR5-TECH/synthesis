import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../api";
import { logDebug, logError, logInfo } from "../logging";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

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
}

// Fixed outer size of the picker window (spec PPK-FR-09 / PPK-FR-10 / PPK-FR-11).
// These must match `src-tauri/tauri.conf.json` — Tauri opens the window at
// the configured size, and re-applying it here pins the dimensions in case
// the OS window manager attempted to grow or maximize the window before
// our `setMaximizable(false)`/`setResizable(false)` calls landed.
// The right column is rendered at natural size; the left column absorbs the
// remaining width. The picker CSS (kit.css) is tuned so both columns fit
// inside these dimensions with no scrolling other than the recent list.
export const PICKER_WINDOW_WIDTH = 800;
export const PICKER_WINDOW_HEIGHT = 600;

type PickerWindow = ReturnType<typeof getCurrentWindow>;

// Upper bound (ms) on how long we wait for the OS to finish leaving full-screen
// before sizing/centering the picker. On macOS `setFullscreen(false)` resolves
// when the call is dispatched, not when the Space-exit animation completes, and
// a `setSize`/centering applied mid-transition is silently discarded by the
// window manager — leaving the picker mis-sized and off-center, the exact
// failure SNV-FR-27 prevents. We resolve as soon as the post-exit resize fires;
// this ceiling only bounds the wait if no resize is observed.
const FULLSCREEN_EXIT_SETTLE_MS = 800;

// Leave OS full-screen and wait for the window to actually settle out of it.
// Leaving full-screen changes the outer size, so the platform emits a resize;
// we await that (with the ceiling above as a fallback) so the fixed size and
// centering that follow land on a normal window. Best-effort and self-cleaning:
// the resize listener and timer are always torn down, even if setFullscreen
// rejects on a platform that does not support it.
async function exitFullscreen(w: PickerWindow): Promise<void> {
  let resolveSettled!: () => void;
  const settled = new Promise<void>((resolve) => {
    resolveSettled = resolve;
  });
  let unlisten: (() => void) | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let done = false;
  const finish = () => {
    if (done) return;
    done = true;
    if (timer) clearTimeout(timer);
    if (unlisten) {
      try {
        unlisten();
      } catch {
        // ignore teardown failures
      }
    }
    resolveSettled();
  };
  try {
    if (typeof w.onResized === "function") {
      try {
        // Subscribe before toggling so the post-exit resize is never missed.
        unlisten = await w.onResized(() => finish());
      } catch {
        // resize subscription unavailable — rely on the timeout fallback
      }
    }
    timer = setTimeout(finish, FULLSCREEN_EXIT_SETTLE_MS);
    await w.setFullscreen(false);
    await settled;
  } finally {
    finish();
  }
}

async function applyPickerWindowChrome(): Promise<void> {
  try {
    const w = getCurrentWindow();
    // SNV-FR-27: the picker is never presented inside an OS full-screen space.
    // When File → Close Project (SNV-FR-25) is activated while the shared main
    // window is in native full-screen, the window is still full-screen as the
    // picker reclaims it. Leave full-screen first so the fixed size (PPK-FR-09)
    // and centering (PPK-FR-12) applied below land on a normal window rather
    // than being swallowed by the full-screen space. This is distinct from the
    // maximized handling below — full-screen is a separate OS window state, and
    // a window cannot be unmaximized while still full-screen, so it goes first.
    if (typeof w.isFullscreen === "function") {
      try {
        if (await w.isFullscreen()) await exitFullscreen(w);
      } catch {
        // some platforms reject before the window is fully created; fall
        // through so the fixed size and centering below still apply
      }
    }
    // Order: ensure not maximized, drop resize/maximize affordances, then
    // pin the outer size. setResizable(false) on its own does not always
    // disable the OS maximize control, so we also clear maximizable.
    if (typeof w.isMaximized === "function") {
      try {
        if (await w.isMaximized()) await w.unmaximize();
      } catch {
        // ignore — some platforms reject before window is fully created
      }
    }
    if (typeof w.setMaximizable === "function") {
      try {
        await w.setMaximizable(false);
      } catch {
        // some platforms (e.g. Linux/wayland) may not support this
      }
    }
    await w.setResizable(false);
    await w.setSize(
      new LogicalSize(PICKER_WINDOW_WIDTH, PICKER_WINDOW_HEIGHT),
    );
    // PPK-FR-12: center the picker on the active display. At launch the Rust
    // setup hook already centers, but when the picker is re-shown within a
    // session — after closing (SNV-FR-25) or switching (OVW-FR-11) a project —
    // the shared window still sits at the main shell's last position. The fixed
    // size has just been pinned above, so re-centering now lands correctly. The
    // backend computes the active display from the cursor and never reads a
    // persisted position (PPK-FR-12). Best-effort: swallow if unavailable.
    await api.centerPicker().catch(() => {});
  } catch {
    // Outside the Tauri runtime (tests, plain Vite dev), these APIs reject.
    // The picker still renders; window chrome is a no-op in that environment.
  }
}

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
 * the user sees.
 */
const ENTRY_OPERATIONS: Record<RecentMenuAction, (path: string) => Promise<void>> = {
  pin: api.pinRecentProject,
  unpin: api.unpinRecentProject,
  hide: api.removeRecentProject,
};

/** The keyboard routes to a context menu (PPK-FR-SDXU). */
function isContextMenuKey(e: React.KeyboardEvent): boolean {
  return e.key === "ContextMenu" || (e.shiftKey && e.key === "F10");
}

export function ProjectPicker({ onOpen }: ProjectPickerProps) {
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
    // PPK-FR-09 / PPK-FR-10 / PPK-FR-11: picker uses a fixed, non-resizable window.
    void applyPickerWindowChrome();
  }, []);

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
        : { path, x: box.left + 24, y: box.bottom },
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
    runOpen(() => api.openProjectAtPath(r.path), (message) =>
      setRecentError(message ? { path: r.path, message } : null),
    );

  const onBrowse = async () => {
    setBrowseError("");
    try {
      const picked = await openDialog({ directory: true, multiple: false });
      if (typeof picked !== "string") return;
      await runOpen(() => api.openProjectAtPath(picked), setBrowseError);
    } catch (e) {
      setBrowseError(errorMessage(e));
    }
  };

  const onGitOpen = () => {
    if (!/^(git@|https?:\/\/)/.test(git.trim())) {
      setGitError("URL is not parsable");
      return;
    }
    runOpen(() => api.openProjectFromGitUrl(git.trim()), setGitError);
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
      () => api.createProject(name.trim(), mode, createTarget),
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
                      // A right-button press does not focus the entry. Focus
                      // can scroll a partly visible entry into view, and that
                      // scroll closes the menu (PPK-FR-VFDK).
                      onMouseDown={(e) => {
                        if (e.button === 2) e.preventDefault();
                      }}
                      // PPK-FR-SDXU: a right-click opens the menu.
                      onContextMenu={(e) => openMenu(e, r.path)}
                      // PPK-FR-SDXU: the context-menu key and Shift+F10 open
                      // the menu on the focused entry.
                      onKeyDown={(e) => {
                        if (isContextMenuKey(e)) openMenu(e, r.path);
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

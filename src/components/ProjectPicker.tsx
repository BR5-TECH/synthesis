import { useEffect, useState } from "react";
import * as api from "../api";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

import { Icon } from "./icons";
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
          <svg
            width="32"
            height="32"
            viewBox="0 0 64 64"
            style={{ color: "var(--accent)" }}
          >
            <rect
              x="2"
              y="2"
              width="60"
              height="60"
              rx="14"
              fill="currentColor"
              opacity="0.12"
            />
            <path
              d="M16 14 H48 V20 H26 L36 32 L26 44 H48 V50 H16 V46 L28.5 32 L16 18 Z"
              fill="currentColor"
            />
          </svg>
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
            <div className="picker-recents-body" data-testid="picker-recents-body">
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
                      onClick={() => !busy && onRecentClick(r)}
                    >
                      <Icon.Folder className="picker-recent-row__icon icon" />
                      <div style={{ flex: 1, minWidth: 0 }}>
                        <div className="picker-recent-row__name">
                          {r.name}
                          {/* PPK-FR-13: reflect pinned/missing state from
                              "list recent projects" (read-only; management is
                              in Global settings, not the picker). */}
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
    </div>
  );
}
